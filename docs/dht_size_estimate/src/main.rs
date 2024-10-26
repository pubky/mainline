use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use clap::Parser;
use full_palette::GREY;
use mainline::{ClosestNodes, Id, Node};
use plotters::prelude::*;
use statrs::statistics::*;

const DEFAULT_DHT_SIZE: usize = 2_000_000;

const DEFAULT_LOOKUPS: usize = 12;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Size of the Dht to sample
    dht_size: Option<usize>,
    /// Number of lookups in each simulation before estimating the dht size
    lookups: Option<usize>,
}

fn main() {
    let cli = Cli::parse();

    let dht_size = cli.dht_size.unwrap_or(DEFAULT_DHT_SIZE);
    let lookups = cli.lookups.unwrap_or(DEFAULT_LOOKUPS);

    let cpus = num_cpus::get() - 1;
    let stop_event = Arc::new(AtomicBool::new(false));

    let estimates = Arc::new(Mutex::new(vec![]));

    println!("Building a DHT with {} nodes...", dht_size);
    let dht = build_dht(dht_size);

    let mut handles = Vec::new();
    for _ in 0..cpus {
        let stop_event = stop_event.clone();
        let estimates = estimates.clone();
        let dht = dht.clone();

        let handle = thread::spawn(move || {
            while !stop_event.load(Ordering::Relaxed) {
                let estimate = simulate(&dht, lookups);

                let mut estimates = estimates.lock().unwrap();
                estimates.push(estimate as f64);

                print!("\rsimulations {}", estimates.len());
                std::io::stdout().flush().unwrap();
            }
        });
        handles.push(handle);
    }

    println!("\nEstimating Dht size after {lookups} lookups");

    // Handle Ctrl+C to gracefully stop threads
    ctrlc::set_handler(move || {
        stop_event.store(true, Ordering::Relaxed);
    })
    .expect("Error setting Ctrl+C handler");

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let estimates = estimates.lock().unwrap();
    println!("\nDone.\n");

    draw_plot(dht_size as f64, lookups, &estimates, "plot.png");
}

/// Build a Dht with `size` number of nodes, uniformly distributed across the key space.
fn build_dht(size: usize) -> Arc<BTreeMap<Id, Node>> {
    let mut dht = BTreeMap::new();
    for _ in 0..size {
        let node = Node::random();
        dht.insert(node.id, node);
    }

    Arc::new(dht)
}

/// Simulate the Dht size estimation of a node running and performed
/// `lookups` number of queries to random Ids.
///
/// Does the same as `ClosestNodes` in `mainline`, averaged over number of lookups.
fn simulate(dht: &BTreeMap<Id, Node>, lookups: usize) -> usize {
    (0..lookups)
        .map(|_| {
            let target = Id::random();

            let mut closest_nodes = ClosestNodes::new(target);

            for (_, node) in dht.range(target..).take(200) {
                closest_nodes.add(node.clone())
            }
            for (_, node) in dht.range(..target).rev().take(200) {
                closest_nodes.add(node.clone())
            }

            let estimate = closest_nodes.dht_size_estimate();

            estimate
        })
        .sum::<usize>()
        / lookups
}

fn draw_plot(expected: f64, lookups: usize, data: &Vec<f64>, filename: &str) {
    let data = Data::new(data.to_vec());

    let mean = data.mean().unwrap();
    let std_dev = data.std_dev().unwrap();

    let margin = 3.0 * std_dev;

    let x_min = mean - margin;
    let x_max = mean + margin;

    let mean = data.mean().unwrap();

    println!("Statistics:");
    println!("\tDht size: {:.0}", expected);
    println!("\tMean: {:.0}", mean);
    // println!("\tError factor: {:.3}", (mean - expected) / expected);
    println!("\tStandard Deviation: {:.0}%", (std_dev / expected) * 100.0);
    println!(
        "\t95% Confidence Interval:  +-{:.0}%",
        ((std_dev * 2.0) / expected) * 100.0
    );

    let mut bands = HashMap::new();
    let band_width = ((x_max - x_min) as u32 / 200).max(1);

    let mut y_max = 0;

    for estimate in data.iter() {
        // round to nearest 1/1000th
        let band = (*estimate as u32 / band_width) * band_width;

        let count = bands.get(&band).unwrap_or(&(0 as u32)) + 1;
        bands.insert(band, count);

        y_max = y_max.max(count);
    }

    // Set up the drawing area (800x600 pixels)
    let root = BitMapBackend::new(filename, (800, 600)).into_drawing_area();
    root.fill(&WHITE).unwrap();

    // Create a chart with labels for both axes
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("{} nodes, {} lookups", expected, lookups),
            ("sans-serif", 40),
        )
        .margin(10)
        .x_label_area_size(35)
        .y_label_area_size(40)
        .build_cartesian_2d(x_min as u32..x_max as u32, 0u32..y_max)
        .unwrap();

    chart
        .configure_mesh()
        .disable_y_mesh()
        .light_line_style(WHITE.mix(0.3))
        .bold_line_style(GREY.stroke_width(1))
        .y_desc("Count")
        .x_desc("Nodes")
        .axis_desc_style(("sans-serif", 20))
        .draw()
        .unwrap();

    chart
        .draw_series(
            Histogram::vertical(&chart)
                .style(RGBAColor(62, 106, 163, 1.0).filled())
                .data(bands.iter().map(|(x, y)| (*x, *y))),
        )
        .unwrap();

    // Save the result to file
    root.present().unwrap();
}

use std::{
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::Duration,
};

use clap::Parser;
use mainline::Dht;

const DEFAULT_SAMPLES: usize = 20;

#[derive(Parser)]
struct Cli {
    /// Number of samples to take across the keyspace
    #[arg(short, long, default_value_t = DEFAULT_SAMPLES)]
    samples: usize,
}

fn main() {
    let samples_count = Cli::parse().samples;

    println!("Calculating Dht size with ({samples_count}) samples..",);

    let samples = Arc::new(Mutex::new(vec![]));

    for _ in 0..samples_count {
        let samples = samples.clone();

        thread::spawn(move || {
            let dht = Dht::client().unwrap();
            // Wait for bootstrap
            sleep(Duration::from_secs(2));

            // Calculate the dht size estimate after bootstrap
            let table = dht.routing_table().unwrap();
            let estimate = table.estimate_dht_size();

            println!(
                "\tSample estimate ({}): {} nodes",
                table.id(),
                format_number(estimate)
            );

            // Add to the samples
            let mut s = samples.lock().unwrap();
            insert_sorted(&mut s, estimate);
        });
    }

    // Wait for bootstrap
    sleep(Duration::from_secs(4));

    let s = samples.lock().unwrap();
    let median = format_number(median(&s));
    println!(
        "\nMedian dht size estimate ({:>3} samples): {median} nodes",
        s.len()
    );

    // loop {
    //     let dht = Dht::client().unwrap();
    //
    //     // Wait for bootstrap
    //     sleep(Duration::from_secs(2));
    //
    //     // Calculate the dht size estimate after bootstrap
    //     let table = dht.routing_table().unwrap();
    //     let estimate = table.estimate_dht_size();
    //
    //     // Add to the samples
    //     insert_sorted(&mut samples, estimate);
    //
    //     let median = format_number(median(&samples));
    //     println!(
    //         "Median dht size estimate ({:>3} samples): {median} nodes",
    //         samples.len()
    //     );
    // }
}

fn format_number(num: usize) -> String {
    // Handle large numbers and format with suffixes
    if num >= 1_000_000_000 {
        return format!("{:.1}B", num as f64 / 1_000_000_000.0);
    } else if num >= 1_000_000 {
        return format!("{:.1}M", num as f64 / 1_000_000.0);
    } else if num >= 1_000 {
        return format!("{:.1}K", num as f64 / 1_000.0);
    }

    // Format with commas for thousands
    let num_str = num.to_string();
    let mut result = String::new();
    let len = num_str.len();

    for (i, c) in num_str.chars().enumerate() {
        // Add a comma before every three digits, except for the first part
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }

    result
}

fn insert_sorted(vec: &mut Vec<usize>, value: usize) {
    // Find the position to insert the value using binary search.
    let pos = vec
        .binary_search_by(|x| x.cmp(&value))
        .unwrap_or_else(|e| e);
    vec.insert(pos, value);
}

fn median(values: &[usize]) -> usize {
    let len = values.len();
    if len == 0 {
        0
    } else if len % 2 == 1 {
        values[len / 2]
    } else {
        (values[len / 2 - 1] + values[len / 2]) / 2
    }
}

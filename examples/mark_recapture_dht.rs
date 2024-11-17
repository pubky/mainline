//! # DHT Size Estimation using Mark-Recapture Method
//!
//! This example demonstrates how to estimate the size of a Distributed Hash Table (DHT)
//! using the Mark-Recapture method, specifically the Chapman estimator.
//!
//! The program continuously samples nodes from the DHT, marking and recapturing nodes
//! to estimate the total population size (i.e., the total number of nodes in the DHT).
//! It stops sampling when a minimum overlap between the marked and recaptured samples is achieved
//! or when a maximum number of random node ID lookups is reached.
//!
//! ## Why Run This Example?
//!
//! - **Educational Purpose**: To understand how the methods of the `mainline` crate can be applied
//!   to estimate the size of a DHT or other distributed systems without any implicit assumption of
//!   node id distributions.
//!
//! - **Alternative Estimation Method**: While there are more time and data-efficient methods
//!   to measure mainline DHT size (see `measure_dht.rs`), this example provides an alternative
//!   approach that might be useful in certain scenarios or for comparison purposes.
//!
//! - **Practical Application**: For developers and researchers working with DHTs who are
//!   interested in estimating the network size without requiring global knowledge of the network.
//!
//! ## Notes
//!
//! - The default parameters are set to take about ~2 hours to complete.
//! - Adjust the constants `MIN_OVERLAP`, `MAX_RANDOM_NODE_IDS`, and `BATCH_SIZE`
//!   as needed to balance between accuracy and computation time.
//!
//! ## How It Works
//!
//! 1. **Marking Phase**: Random node IDs are generated, and the nodes closest to these IDs
//!    are collected as the "marked" sample.
//!
//! 2. **Recapture Phase**: Another set of random node IDs are generated, and the nodes
//!    closest to these IDs are collected as the "recapture" sample.
//!
//! 3. **Estimation**: The overlap between the marked and recaptured samples is used to
//!    estimate the total population size using the Chapman estimator.
//!
//! ## Limitations
//!
//! - The estimation accuracy depends on the overlap between the samples.
//! - The method assumes that the DHT is stable during the sampling period.
//! - More efficient methods exist for measuring DHT size, and this method may not be suitable
//!   for large-scale or time-sensitive applications.
//!

use dashmap::DashSet;
use mainline::{Dht, Id};
use rayon::{prelude::*, ThreadPool, ThreadPoolBuilder};
use tracing::{debug, info, Level};

/// Adjust as needed. Default will take about ~2 hours
// Minimum number of overlapping nodes to stop sampling.
const MIN_OVERLAP: usize = 10_000;
// Maximum number of sampling rounds before stopping. Avoid infinite loops.
const MAX_RANDOM_NODE_IDS: usize = 100_000;
// Number of parallel lookups. Ideally not bigger than number of threads available. Display progress every N.
const BATCH_SIZE: usize = 16;
const Z_SCORE: f64 = 1.96;

/// Represents the DHT size estimation result.
struct EstimateResult {
    estimate: f64,
    standard_error: f64,
    lower_bound: f64,
    upper_bound: f64,
}

impl EstimateResult {
    fn display(&self) {
        println!("\nFinal Estimate:");
        println!(
            "Estimated DHT Size: {} nodes",
            format_number(self.estimate as usize)
        );
        println!(
            "95% Confidence Interval: {} - {} nodes",
            format_number(self.lower_bound as usize),
            format_number(self.upper_bound as usize)
        );
        println!(
            "Standard Error: {} nodes",
            format_number(self.standard_error as usize)
        );
    }
}

fn main() {
    // Initialize the logger.
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("Estimating DHT size using Mark-Recapture with continuous sampling...");
    println!(
        "Stopping at {} overlapped nodes or {} max random node id lookup iterations.",
        format_number(MIN_OVERLAP),
        format_number(MAX_RANDOM_NODE_IDS)
    );

    // Configure the Rayon thread pool with a same num of threads as batch size
    let pool = ThreadPoolBuilder::new()
        .num_threads(BATCH_SIZE)
        .build()
        .expect("Failed to build Rayon thread pool");

    // Initialize the DHT client.
    let dht = Dht::client().expect("Failed to create DHT client");

    // Collect samples from the DHT.
    let (marked_sample, recapture_sample) =
        collect_samples(&dht, pool, MIN_OVERLAP, MAX_RANDOM_NODE_IDS);

    // Display the final statistics.
    if let Some(estimate) = compute_estimate(&marked_sample, &recapture_sample) {
        estimate.display();
    } else {
        println!("Unable to calculate the DHT size estimate due to insufficient overlap.");
    }
}

fn collect_samples(
    dht: &Dht,
    pool: ThreadPool,
    min_overlap: usize,
    max_unique_random_node_ids: usize,
) -> (DashSet<Id>, DashSet<Id>) {
    let marked_sample = DashSet::new();
    let recapture_sample = DashSet::new();
    let mut total_iterations = 0;

    let mut size = 0.0;
    let mut confidence = 0.0;

    loop {
        if total_iterations >= max_unique_random_node_ids {
            println!("Reached maximum number of random node ID lookups.");
            break;
        }

        let mark_random_ids: Vec<_> = (0..BATCH_SIZE).map(|_| Id::random()).collect();
        let recapture_random_ids: Vec<_> = (0..BATCH_SIZE).map(|_| Id::random()).collect();

        // Perform sampling in the thread pool
        pool.install(|| {
            // Sample for marked_sample in parallel.
            mark_random_ids.par_iter().for_each(|random_id| {
                if let Ok(nodes) = dht.find_node(*random_id) {
                    for node in nodes {
                        marked_sample.insert(*node.id());
                    }
                }
            });

            // Sample for recapture_sample in parallel.
            recapture_random_ids.par_iter().for_each(|random_id| {
                if let Ok(nodes) = dht.find_node(*random_id) {
                    for node in nodes {
                        recapture_sample.insert(*node.id());
                    }
                }
            });
        });

        total_iterations += BATCH_SIZE;

        // Compute overlap.
        let overlap = marked_sample
            .iter()
            .filter(|id| recapture_sample.contains(id))
            .count();

        if let Some(estimate) = compute_estimate(&marked_sample, &recapture_sample) {
            size = estimate.estimate;
            confidence = estimate.standard_error * Z_SCORE;
        }

        info!(
            "Sampled {}/{} random IDs. Found {}/{} overlapping nodes. Estimate is {}±{}",
            format_number(total_iterations),
            format_number(max_unique_random_node_ids),
            format_number(overlap),
            format_number(min_overlap),
            format_number(size as usize),
            format_number(confidence as usize)
        );

        if overlap >= min_overlap {
            println!("Sufficient overlap achieved.");
            break;
        }
    }

    (marked_sample, recapture_sample)
}

/// Computes the DHT size estimate using the Chapman estimator.
///
/// # Arguments
///
/// * `marked_sample` - The marked sample as a DashSet.
/// * `recapture_sample` - The recapture sample as a DashSet.
///
/// # Returns
///
/// An `Option<EstimateResult>` containing the estimate and statistical data.
fn compute_estimate(
    marked_sample: &DashSet<Id>,
    recapture_sample: &DashSet<Id>,
) -> Option<EstimateResult> {
    let n1 = marked_sample.len() as f64;
    let n2 = recapture_sample.len() as f64;

    // Compute overlap (m).
    let m = marked_sample
        .iter()
        .filter(|id| recapture_sample.contains(id))
        .count() as f64;

    debug!("\nComputing estimate with:");
    debug!("Marked sample size (n1): {}", n1);
    debug!("Recapture sample size (n2): {}", n2);
    debug!("Overlap size (m): {}", m);

    if m > 0.0 {
        // Chapman estimator formula.
        let estimate = ((n1 + 1.0) * (n2 + 1.0) / (m + 1.0)) - 1.0;

        // Calculate variance and standard error.
        let variance =
            ((n1 + 1.0) * (n2 + 1.0) * (n1 - m) * (n2 - m)) / ((m + 1.0).powi(2) * (m + 2.0));
        let standard_error = variance.sqrt();

        // 95% confidence interval.
        let margin_of_error = Z_SCORE * standard_error;
        let lower_bound = (estimate - margin_of_error).max(0.0);
        let upper_bound = estimate + margin_of_error;

        Some(EstimateResult {
            estimate,
            standard_error,
            lower_bound,
            upper_bound,
        })
    } else {
        None
    }
}

fn format_number(num: usize) -> String {
    if num >= 1_000_000_000 {
        format!("{:.1}B", num as f64 / 1_000_000_000.0)
    } else if num >= 1_000_000 {
        format!("{:.1}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{:.1}K", num as f64 / 1_000.0)
    } else {
        num.to_string()
    }
}

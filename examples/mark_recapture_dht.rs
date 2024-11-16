use dashmap::DashSet;
use mainline::{Dht, Id};
use rayon::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info, Level};

/// Adjust as needed. Default will take about ~6 hours
const MIN_OVERLAP: usize = 10_000; // Minimum number of overlapping nodes to stop sampling.
const MAX_RANDOM_NODE_IDS: usize = 100_000; // Maximum number of sampling rounds before stopping. Avoid infinite loops.
const BATCH_SIZE: usize = 16; // Number of parallel lookups. Display progress every N.

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

    // Initialize the DHT client wrapped in Arc for thread safety.
    let dht = Arc::new(Dht::client().expect("Failed to create DHT client"));

    // Collect samples from the DHT.
    let (marked_sample, recapture_sample) = collect_samples(&dht, MIN_OVERLAP, MAX_RANDOM_NODE_IDS);

    // Display the final statistics.
    if let Some(estimate) = compute_estimate(&marked_sample, &recapture_sample) {
        estimate.display();
    } else {
        println!("Unable to calculate the DHT size estimate due to insufficient overlap.");
    }
}

fn collect_samples(
    dht: &Arc<Dht>,
    min_overlap: usize,
    max_unique_random_node_ids: usize,
) -> (HashSet<Id>, HashSet<Id>) {
    let marked_sample = DashSet::new();
    let recapture_sample = DashSet::new();
    let mut total_iterations = 0;

    let mut size = 0.0;
    let mut confidence = 0.0;

    loop {
        if total_iterations >= max_unique_random_node_ids {
            break;
        }

        // Sample for marked_sample in parallel.
        let random_ids: Vec<_> = (0..BATCH_SIZE).map(|_| Id::random()).collect();
        random_ids.par_iter().for_each(|random_id| {
            if let Ok(nodes) = dht.find_node(*random_id) {
                for node in nodes {
                    marked_sample.insert(*node.id());
                }
            }
        });

        // Sample for recapture_sample in parallel.
        let random_ids: Vec<_> = (0..BATCH_SIZE).map(|_| Id::random()).collect();
        random_ids.par_iter().for_each(|random_id| {
            if let Ok(nodes) = dht.find_node(*random_id) {
                for node in nodes {
                    recapture_sample.insert(*node.id());
                }
            }
        });

        total_iterations += BATCH_SIZE;

        // Compute overlap.
        let overlap = marked_sample
            .iter()
            .filter(|id| recapture_sample.contains(id))
            .count();

        if let Some(estimate) = compute_estimate(
            &marked_sample.clone().into_iter().collect(),
            &recapture_sample.clone().into_iter().collect(),
        ) {
            size = estimate.estimate;
            confidence = estimate.standard_error * 1.96;
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

    // Convert DashSets to HashSets.
    let marked_sample: HashSet<Id> = marked_sample.into_iter().collect();
    let recapture_sample: HashSet<Id> = recapture_sample.into_iter().collect();

    (marked_sample, recapture_sample)
}

/// Computes the DHT size estimate using the Chapman estimator.
///
/// # Arguments
///
/// * `marked_sample` - The marked sample.
/// * `recapture_sample` - The recapture sample.
///
/// # Returns
///
/// An `Option<EstimateResult>` containing the estimate and statistical data.
fn compute_estimate(
    marked_sample: &HashSet<Id>,
    recapture_sample: &HashSet<Id>,
) -> Option<EstimateResult> {
    let n1 = marked_sample.len() as f64;
    let n2 = recapture_sample.len() as f64;
    let m = marked_sample.intersection(recapture_sample).count() as f64;

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
        let z_score = 1.96;
        let margin_of_error = z_score * standard_error;
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

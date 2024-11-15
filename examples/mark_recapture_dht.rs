use mainline::{Dht, Id};
use std::collections::HashSet;
use tracing::Level;

/// Configuration parameters for the DHT size estimation.
struct Config {
    /// Incremental sample size to add in each iteration.
    incremental_sample_size: usize,
    /// Minimum required overlap to proceed with estimation.
    min_overlap: usize,
    /// Maximum number of iterations to prevent infinite loops.
    max_iterations: usize,
    /// Progress display interval.
    progress_interval: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            incremental_sample_size: 100,
            min_overlap: 100, // Adjust. A bigger overlap leads to more accurate estimates, but sampling will take longer.
            max_iterations: 100,
            progress_interval: 10,
        }
    }
}

fn main() {
    // Initialize the logger.
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    // Set up the configuration.
    let config = Config::default();

    println!("Estimating DHT size using Mark-Recapture with iterative sampling for both sets...");

    // Initialize the DHT client.
    let dht = Dht::client().expect("Failed to create DHT client");

    // Collect samples from the DHT.
    let (marked_sample, recapture_sample) = collect_iterative_samples(&dht, &config);

    // Compute DHT size estimate from the accumulated samples.
    let estimate = compute_estimate(&marked_sample, &recapture_sample);

    // Display the final statistics.
    display_estimate(estimate);
}

/// Represents the DHT size estimation result.
struct EstimateResult {
    estimate: f64,
    standard_error: f64,
    lower_bound: f64,
    upper_bound: f64,
}

/// Collects and accumulates samples for both marked and recapture sets iteratively until sufficient overlap is found.
///
/// # Arguments
///
/// * `dht` - Reference to the DHT client.
/// * `config` - Configuration parameters.
///
/// # Returns
///
/// A tuple containing the marked sample and the recapture sample.
fn collect_iterative_samples(dht: &Dht, config: &Config) -> (HashSet<Id>, HashSet<Id>) {
    let mut marked_sample = HashSet::new();
    let mut recapture_sample = HashSet::new();

    // Step 1: Collect initial samples for both marked and recapture sets.
    println!("\nCollecting initial marked sample...");
    perform_sampling(
        dht,
        config.incremental_sample_size,
        &mut marked_sample,
        config.progress_interval,
    );

    println!("\nCollecting initial recapture sample...");
    perform_sampling(
        dht,
        config.incremental_sample_size,
        &mut recapture_sample,
        config.progress_interval,
    );

    // Step 2: Iteratively add to both samples.
    let mut iterations = 0;
    let mut overlap = marked_sample.intersection(&recapture_sample).count();

    println!("Initial overlap size: {}", overlap);

    while iterations < config.max_iterations && overlap < config.min_overlap {
        iterations += 1;
        println!(
            "\nIteration {}: Adding more samples to both sets...",
            iterations
        );

        // Add to marked sample.
        perform_sampling(
            dht,
            config.incremental_sample_size,
            &mut marked_sample,
            config.progress_interval,
        );

        // Add to recapture sample.
        perform_sampling(
            dht,
            config.incremental_sample_size,
            &mut recapture_sample,
            config.progress_interval,
        );

        // Update overlap.
        overlap = marked_sample.intersection(&recapture_sample).count();
        println!("Current overlap size: {}", overlap);

        if overlap >= config.min_overlap {
            println!("Sufficient overlap achieved.");
            break;
        } else {
            println!("Overlap not sufficient yet. Continuing sampling...");
        }
    }

    if iterations >= config.max_iterations {
        println!("Maximum number of iterations reached.");
    }

    (marked_sample, recapture_sample)
}

/// Performs sampling by querying the DHT with random target IDs and adds node IDs to the provided sample.
///
/// # Arguments
///
/// * `dht` - Reference to the DHT client.
/// * `sample_size` - Number of random IDs to query.
/// * `sample` - Mutable reference to the sample to which node IDs will be added.
/// * `progress_interval` - Interval at which progress is displayed.
fn perform_sampling(
    dht: &Dht,
    sample_size: usize,
    sample: &mut HashSet<Id>,
    progress_interval: usize,
) {
    for i in 0..sample_size {
        if i % progress_interval == 0 {
            println!("Sampled {} random IDs of {}", i, sample_size);
        }
        let random_id = Id::random();
        if let Ok(nodes) = dht.find_node(random_id) {
            for node in nodes {
                sample.insert(*node.id());
            }
        }
    }
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
    let m2 = marked_sample.intersection(recapture_sample).count() as f64;

    println!("\nComputing estimate with:");
    println!("Marked sample size (n1): {}", n1);
    println!("Recapture sample size (n2): {}", n2);
    println!("Overlap size (m2): {}", m2);

    if m2 > 0.0 {
        // Chapman estimator formula.
        let estimate = ((n1 + 1.0) * (n2 + 1.0) / (m2 + 1.0)) - 1.0;

        // Calculate variance and standard error.
        let variance =
            ((n1 + 1.0) * (n2 + 1.0) * (n1 - m2) * (n2 - m2)) / ((m2 + 1.0).powi(2) * (m2 + 2.0));
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

fn display_estimate(estimate: Option<EstimateResult>) {
    if let Some(estimate) = estimate {
        println!("\nFinal Estimate:");
        println!(
            "Estimated DHT Size: {} nodes",
            format_number(estimate.estimate as usize)
        );
        println!(
            "95% Confidence Interval: {} - {} nodes",
            format_number(estimate.lower_bound as usize),
            format_number(estimate.upper_bound as usize)
        );
        println!("Standard Error: {:.2}", estimate.standard_error);
    } else {
        println!("Unable to calculate the DHT size estimate due to insufficient overlap.");
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

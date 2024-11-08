use clap::Parser;
use mainline::{Dht, Id};
use tracing::Level;

const DEFAULT_SAMPLES: usize = 20;

#[derive(Parser)]
struct Cli {
    /// Number of samples to take across the keyspace
    #[arg(short, long, default_value_t = DEFAULT_SAMPLES)]
    samples: usize,
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let dht = Dht::client().unwrap();

    println!("Calculating Dht size by sampling random lookup queries..",);

    for lookups in 1.. {
        let _ = dht.find_node(Id::random()).unwrap();

        let info = dht.info().unwrap();

        println!(
            "Dht size esttimate after {} lookups: {} +-{:.0}% nodes",
            lookups,
            format_number(info.dht_size_estimate()),
            (info.dht_size_estimate_standard_deviation() * 2.0) * 100.0
        );
    }
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

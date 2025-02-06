//! Demonstrates caching and reusing bootstrapping nodes from the running
//! node's routing table.
//!
//! Saves the bootstrapping nodes in `examples/bootstrapping_nodes.toml` relative to
//! the script's directory, regardless of where the script is run from.

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

use mainline::Dht;

use tracing::Level;
use tracing_subscriber;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let mut builder = Dht::builder();

    let examples_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let nodes_file = examples_dir.join("bootstrapping_nodes");
    if nodes_file.exists() {
        let mut file =
            fs::File::open(&nodes_file).expect("Failed to open bootstrapping nodes file");
        let mut content = String::new();
        file.read_to_string(&mut content)
            .expect("Failed to read bootstrapping nodes file");

        let cached_nodes = content
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        // To confirm that these old nodes are still viable,
        // try `builder.bootstrap(&cached_nodes)` instead,
        // this way you don't rely on default bootstrap nodes.
        builder.extra_bootstrap(&cached_nodes);
    };

    let client = builder.build().unwrap();

    client.bootstrapped();

    let bootstrap = client.to_bootstrap();

    let bootstrap_content = bootstrap.join("\n");
    let mut file = fs::File::create(&nodes_file).expect("Failed to save bootstrapping nodes");
    file.write(bootstrap_content.as_bytes())
        .expect("Failed to write bootstrapping nodes");
}

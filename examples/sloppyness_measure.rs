use std::{collections::HashSet, net::IpAddr};

use flume::Iter;
use mainline::{Dht, Id, Node};
use tracing::Level;


fn distance(target: Id, node: &Node) -> u8 {
    target.distance(node.id())
}

fn print_distances(nodes: &Vec<Node>, target: Id) {
    for node in nodes.iter() {
        let distance = target.distance(node.id());
        println!("Distance: {}, {}", distance, node.id());
    }
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let k = 20; // Not really k but we take the k closest nodes into account.
    let target = Id::random();
    let mut all_ips: HashSet<IpAddr> = HashSet::new();

    println!("Sloppyness test - Counting all IP addresses around a random target_key={target} k={k}. Each lookup round starts with a clear routing table and a new DHT Id.");

    for lookups in 1.. {
        let mut dht = Dht::client().unwrap();
        let nodes = dht.find_node(target).unwrap();
        let closest_nodes: Vec<Node> = nodes.into_iter().take(k).collect();
        let ips = closest_nodes.iter().map(|node| node.address().ip());
        for ip in ips {
            all_ips.insert(ip);
        }

        let closest_node = closest_nodes.first().expect("Closest node not found.");
        let closest_distance = distance(target, closest_node);
        println!(
            "lookup={} Ips found {}. Closest node distance: {}",
            lookups,
            all_ips.len(),
            closest_distance
        );
        dht.shutdown();
    }
}

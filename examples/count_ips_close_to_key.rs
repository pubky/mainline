use std::{collections::HashSet, net::IpAddr};

use mainline::{rpc::DEFAULT_BOOTSTRAP_NODES, Dht, Id, Node};
use rand::{seq::SliceRandom, thread_rng};
use tracing::Level;


fn get_random_boostrap_nodes(all: &HashSet<String>) -> Vec<String> {
    if all.len() < 4 {
        let nodes = DEFAULT_BOOTSTRAP_NODES;
        let nodes: Vec<String> = nodes.into_iter().map(|node| node.to_string()).collect();
        return nodes;
    }
    let mut all: Vec<String> = all.clone().into_iter().collect();
    let mut rng = thread_rng();
    all.shuffle(&mut rng);
    let slice: Vec<String> = all[..4].into_iter().map(|va| va.clone()).collect();
    slice
}


fn main() {
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let k = 20; // Not really k but we take the k closest nodes into account.
    let target = Id::random();
    let mut all_ips: HashSet<IpAddr> = HashSet::new();
    let mut all_sockets: HashSet<String> = HashSet::new();

    println!("Count all IP addresses around a random target_key={target} k={k}. Each lookup round starts with a clear routing table and a new DHT Id.");

    for lookups in 1.. {
        let bootstrap = get_random_boostrap_nodes(&all_sockets);
        let mut dht = Dht::builder().bootstrap(&bootstrap).build().unwrap();
        let nodes = dht.find_node(target).unwrap();
        let closest_nodes: Vec<Node> = nodes.into_iter().take(k).collect();
        let sockets = closest_nodes.iter().map(|node| node.address());
        for socket in sockets {
            all_ips.insert(socket.ip());
            let socket_str = socket.to_string();
            all_sockets.insert(socket_str);
        }

        let closest_node = closest_nodes.first().expect("Closest node not found.");
        let closest_distance = target.distance(closest_node.id());
        println!(
            "lookup={} Ips found {}. Closest node distance: {}",
            lookups,
            all_ips.len(),
            closest_distance
        );
        dht.shutdown();
    }
}

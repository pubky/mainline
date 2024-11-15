use std::{collections::HashSet, net::IpAddr};

use mainline::{rpc::DEFAULT_BOOTSTRAP_NODES, Dht, Id, Node};
use rand::{random, seq::SliceRandom, thread_rng};
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

fn get_random_boostrap_nodes2() -> Vec<String> {
    let mut dht = Dht::client().unwrap();
    let nodes = dht.find_node(Id::random()).unwrap();
    dht.shutdown();
    let mut addrs: Vec<String> = nodes.into_iter().map(|node| node.address().to_string()).collect();
    let slice: Vec<String> = addrs[..8].into_iter().map(|va| va.clone()).collect();
    slice
}


fn main() {
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let k = 20; // Not really k but we take the k closest nodes into account.
    let target = Id::random();
    let mut all_ips: HashSet<IpAddr> = HashSet::new();
    let MAX_DISTANCE = 150;

    println!("Count all IP addresses around a random target_key={target} k={k} max_distance={MAX_DISTANCE}.");

    let mut last_nodes: HashSet<IpAddr> = HashSet::new();
    for lookups in 1.. {
        let bootstrap = get_random_boostrap_nodes2();
        let mut dht = Dht::builder().bootstrap(&bootstrap).build().unwrap();
        let nodes = dht.find_node(target).unwrap();
        let nodes: Vec<Node> = nodes.into_iter().filter(|node| target.distance(node.id()) < MAX_DISTANCE).collect();
        let closest_nodes: Vec<Node> = nodes.into_iter().take(k).collect();
        let sockets: HashSet<IpAddr> = closest_nodes.iter().map(|node| node.address().ip()).collect();
        for socket in sockets.iter() {
            all_ips.insert(socket.clone());
        }

        if closest_nodes.is_empty() {
            continue;
        }
        let closest_node = closest_nodes.first().unwrap();
        let closest_distance = target.distance(closest_node.id());
        let furthest_node = closest_nodes.last().unwrap();
        let furthest_distance = target.distance(furthest_node.id());

        let overlap_with_last_lookup: HashSet<IpAddr> = sockets.intersection(&last_nodes).map(|ip| ip.clone()).collect();
        let overlap = overlap_with_last_lookup.len() as f64/k as f64;
        last_nodes = sockets;
        println!(
            "lookup={} Ips found {}. Closest node distance: {}, furthest node distance: {}, overlap with previous lookup {}%",
            lookups,
            all_ips.len(),
            closest_distance,
            furthest_distance,
            (overlap*100 as f64) as usize
        );
        dht.shutdown();
    }
}

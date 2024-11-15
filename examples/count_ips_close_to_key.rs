use std::{collections::HashSet, net::IpAddr};
use mainline::{Dht, Id, Node};
use tracing::Level;



fn get_random_boostrap_nodes2() -> Vec<String> {
    let mut dht = Dht::client().unwrap();
    let nodes = dht.find_node(Id::random()).unwrap();
    dht.shutdown();
    let mut addrs: Vec<String> = nodes.into_iter().map(|node| node.address().to_string()).collect();
    let slice: Vec<String> = addrs[..8].into_iter().map(|va| va.clone()).collect();
    slice
}

fn init_dht(use_random_boostrap_nodes: bool) -> Dht {
    if use_random_boostrap_nodes {
        let bootstrap = get_random_boostrap_nodes2();
        return Dht::builder().bootstrap(&bootstrap).build().unwrap();
    } else {
        Dht::client().unwrap()
    }
}


fn main() {
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let k = 20; // Not really k but we take the k closest nodes into account.
    const MAX_DISTANCE: u8 = 150; // Health check to not include outrageously distant nodes.
    const USE_RANDOM_BOOTSTRAP_NODES: bool = true;
    let target = Id::random();
    let mut all_ips: HashSet<IpAddr> = HashSet::new();

    println!("Count all IP addresses around a random target_key={target} k={k} max_distance={MAX_DISTANCE} random_boostrap={USE_RANDOM_BOOTSTRAP_NODES}.");
    println!();
    
    let mut last_nodes: HashSet<IpAddr> = HashSet::new();
    for lookups in 1.. {
        let mut dht = init_dht(USE_RANDOM_BOOTSTRAP_NODES);
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

use std::{collections::{HashMap, HashSet}, net::IpAddr, sync::mpsc::channel};
use histo::Histogram;
use mainline::{Dht, Id, Node};
use tracing::Level;


const k: usize = 20; // Not really k but we take the k closest nodes into account.
const MAX_DISTANCE: u8 = 150; // Health check to not include outrageously distant nodes.
const USE_RANDOM_BOOTSTRAP_NODES: bool = false;


fn main() {
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let target = Id::random();
    let mut ip_hits: HashMap<IpAddr, u16> = HashMap::new();
    let (tx_interrupted, rx_interrupted) = channel();


    println!("Count all IP addresses around a random target_key={target} k={k} max_distance={MAX_DISTANCE} random_boostrap={USE_RANDOM_BOOTSTRAP_NODES}.");
    println!("Press CTRL+C to show the histogram");
    println!();

    ctrlc::set_handler(move || {
        println!();
        println!("Received Ctrl+C! Finishing current lookup. Hold on...");
        tx_interrupted.send(()).unwrap();
    }).expect("Error setting Ctrl-C handler");
    
    let mut last_nodes: HashSet<IpAddr> = HashSet::new();
    let mut lookup_count = 0;
    while rx_interrupted.try_recv().is_err() {
        lookup_count += 1;
        let mut dht = init_dht(USE_RANDOM_BOOTSTRAP_NODES);
        let nodes = dht.find_node(target).unwrap();
        let nodes: Vec<Node> = nodes.into_iter().filter(|node| target.distance(node.id()) < MAX_DISTANCE).collect();
        let closest_nodes: Vec<Node> = nodes.into_iter().take(k).collect();
        let sockets: HashSet<IpAddr> = closest_nodes.iter().map(|node| node.address().ip()).collect();
        for socket in sockets.iter() {
            let previous = ip_hits.get(socket);
            match previous {
                Some(val) => {
                    ip_hits.insert(socket.clone(), val + 1);
                }
                None => {
                    ip_hits.insert(socket.clone(), 1);
                }
            };
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
            "lookup={:02} Ips found {}. Closest node distance: {}, furthest node distance: {}, overlap with previous lookup {}%",
            lookup_count,
            ip_hits.len(),
            closest_distance,
            furthest_distance,
            (overlap*100 as f64) as usize
        );
        dht.shutdown();
    };

    println!();
    println!("Histogram");
    print_histogram(ip_hits, lookup_count);
}


fn get_random_boostrap_nodes2() -> Vec<String> {
    let mut dht = Dht::client().unwrap();
    let nodes = dht.find_node(Id::random()).unwrap();
    dht.shutdown();
    let addrs: Vec<String> = nodes.into_iter().map(|node| node.address().to_string()).collect();
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


/*
Prints a histogram with the collected nodes
First column are the buckets indicate the hit rate. 84 .. 93 summerizes the nodes that get hit 3 to 12% of each lookup.
Second column indicates the number of nodes that this bucket contains. [19] means 19 nodes got hit with a probability of 3 to 12%.
Third column is a visualization of the number of nodes [19].

Example1: 
84 .. 93 [ 15 ]: ∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎
Within one lookup, 15 nodes got hit in 84 to 93% of the cases. These nodes are therefore foud in almost all lookups.

Example2:
 3 .. 12 [ 19 ]: ∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎
Within one lookup, 19 nodes got hit in 3 to 12% of the cases. These are rarely found therefore.

Full example:
 3 .. 12 [ 19 ]: ∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎
12 .. 21 [  2 ]: ∎∎
21 .. 30 [  3 ]: ∎∎∎
30 .. 39 [  2 ]: ∎∎
39 .. 48 [  3 ]: ∎∎∎
48 .. 57 [  0 ]: 
57 .. 66 [  0 ]: 
66 .. 75 [  0 ]: 
75 .. 84 [  1 ]: ∎
84 .. 93 [ 15 ]: ∎∎∎∎∎∎∎∎∎∎∎∎∎∎∎
*/
fn print_histogram(hits: HashMap<IpAddr, u16>, lookup_count: usize)  {
    /*

     */
    let mut histogram = Histogram::with_buckets(10);
    let percents: HashMap<IpAddr, u64> = hits.into_iter().map(|(ip, hits)| {
        let percent = (hits as f32 / lookup_count as f32) * 100 as f32;
        (ip, percent as u64)
    }).collect();


    for (_, percent) in percents.iter() {
        histogram.add(percent.clone());
    };

    println!("{}", histogram);
}
use mainline::{Dht, Id, Node};
use std::{
    collections::{HashMap, HashSet}, convert::TryInto, net::{IpAddr, Ipv4Addr}, sync::mpsc::channel

};
use tracing::Level;
use std::io::{self, Write};

/**
 * This script looks up random IDs and counts the number of times, the found nodes share the same IP /8 subnet (the first byte of the IP).
 * Here is an example after 160 lookups:
    1 IPs: 14.55
    2 IPs: 4.11
    3 IPs: 1.06
    4 IPs: 0.24
 * On average looking up k=20 nodes (November 2024), you can expect
 * - 14.55x unique subnets
 * -  2.05x 2 identical subnets (=4.11)
 * -  0.35x 3 identical subnets (=1.06)
 * -  0.06x 4 indentical subnets (=0.24)
 * 
 * In theory, if subnets are uniformly distributed, this problem is similar to the [birthday problem](https://www.bdayprob.com/).
 * Looking at these measurements, it does not seem to be uniformly distributed though.
 * 
 * 
 * Another run
 */

const K: usize = 20; // Not really k but we take the k closest nodes into account.

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let dht = Dht::client().unwrap();
    let mut counts: HashMap<usize, usize> = HashMap::new();

    let (tx_interrupted, rx_interrupted) = channel();

    println!("Lookup random /8 subnet distributions.");
    println!("Press CTRL+C to see the result");
    println!();

    ctrlc::set_handler(move || {
        println!();
        println!("Received Ctrl+C! Finishing current lookup. Hold on...");
        tx_interrupted.send(()).unwrap();
    })
    .expect("Error setting Ctrl-C handler");

    let mut i = 0;
    
    while rx_interrupted.try_recv().is_err() {
        println!("Lookup {}", i + 1);
        io::stdout().flush().unwrap();
        let mut blocks_ip: HashMap<u8, Vec<NodeIp>> = HashMap::new();
        let target = Id::random();
        let nodes = dht.find_node(target).unwrap();
        let nodes: Vec<Node> = nodes.into_iter().take(K).collect();
        let ips: HashSet<NodeIp> = nodes.into_iter().map(|node| {
            NodeIp::new(node)
        }).collect();
        for ip in ips {
            let first_byte = ip.ip.octets()[0];
            if !blocks_ip.contains_key(&first_byte) {
                blocks_ip.insert(first_byte, vec![]);
            }
            blocks_ip.get_mut(&first_byte).unwrap().push(ip);
        }

        for (_, vect) in blocks_ip.iter() {
            let key = vect.len();
            let current = counts.get(&key).map(|val| val.clone()).unwrap_or(0);
            counts.insert(key, current + 1);
        }
        i += 1;
    };

    let avg_occurance: HashMap<usize, f64> = counts.into_iter().map(|(count, amount)| (count, amount as f64 / i as f64)).collect();
    let mut ordered: Vec<(usize, f64)> = avg_occurance.into_iter().collect();
    ordered.sort_by_key(|(count, _)| count.clone());


    println!();
    println!("In a k={K} bucket, similar /8 subnets have an average occurance of:");
    for (count, occurance) in ordered {
        let expectation = occurance* (count as f64);
        println!("{count} IPs: {expectation:.2}");
        // println!("- {:5.1}% chance that {count} similar /8 subnets are found in a k={K} bucket.", occurance*100.0/(K as f64)*(count as f64))
    }



}


const BEP42_MASK: [u8; 4] = [0x03, 0x0f, 0x3f, 0xff];

#[derive(Clone, PartialEq, Debug, Eq, Hash)]
struct NodeIp {
    ip: Ipv4Addr
}

impl NodeIp {
    pub fn new(node: Node) -> Self {
        let ip = if let IpAddr::V4(value) = node.address().ip() {
            value
        } else {
            panic!("No IPv4")
        };
        Self {
            ip
        }
    }

    /**
     * IP address masked with the inverted bep0042 mask. This leaves the remaining interestings bits for the IP diversity check.
     */
    pub fn inverted_bep0042_masked_ip(&self) -> Ipv4Addr {
        let inverted_mask: [u8;4] = BEP42_MASK.iter().map(|val| val^0xff).collect::<Vec<_>>().try_into().unwrap(); // BEP_0042 mask flipped to get the remaining bits.
        let masked: [u8;4] = self.ip.octets().iter().zip(inverted_mask).map(|(ip, mask)| ip & mask)
        .collect::<Vec<_>>().try_into().unwrap();
        Ipv4Addr::new(masked[0], masked[1], masked[2], masked[3])
    }

    pub fn shifted_masked_ip(&self) -> Ipv4Addr {
        let leading_bits: [u8;4] = BEP42_MASK.iter().map(|mask| u8::count_ones(*mask) as u8).collect::<Vec<_>>().try_into().unwrap();
        let shifted: [u8;4] = self.inverted_bep0042_masked_ip().octets().iter().zip(leading_bits).map(|(ip, mask)| {
            io::stdout().flush().unwrap();
            let res = if mask == 8 {
                0
            } else {
                ip >> mask
            };
            res
        })
        .collect::<Vec<_>>().try_into().unwrap();
        Ipv4Addr::new(shifted[0], shifted[1], shifted[2], shifted[3])
    }

    pub fn first_byte(&self) -> u8 {
        self.shifted_masked_ip().octets()[0]
    }

    pub fn second_byte(&self) -> u8 {
        self.shifted_masked_ip().octets()[1]
    }

    pub fn third_byte(&self) -> u8 {
        self.shifted_masked_ip().octets()[2]
    }



}



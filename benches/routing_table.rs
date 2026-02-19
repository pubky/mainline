//! Micro-benchmarks for RoutingTable operations: add, closest, and remove
//! at varying table sizes. Reports nanoseconds-per-operation.
//!
//! Catches regressions in the hot-path data structure that underlies every
//! query and maintenance cycle.
//!
//! Run: `cargo run --release --features full --bin routing_table`

use mainline::{Id, Node, RoutingTable};
use std::net::SocketAddrV4;
use std::str::FromStr;
use std::time::Instant;

fn main() {
    println!("routing_table\n");

    bench_add();
    bench_closest();
    bench_remove();
}

fn bench_add() {
    println!("add");

    let addr = SocketAddrV4::from_str("127.0.0.1:6881").unwrap();

    // Pre-generate random nodes outside the timed section
    let nodes: Vec<_> = (0..1000).map(|_| Node::new(Id::random(), addr)).collect();

    // Empty table
    {
        let mut table = RoutingTable::new(Id::random());
        let start = Instant::now();
        for node in nodes.iter().cloned() {
            table.add(node);
        }
        let per_op = start.elapsed().as_nanos() / nodes.len() as u128;
        println!(
            "empty table:     {per_op}ns/op (final size: {})",
            table.size()
        );
    }

    // Pre-filled table (100 nodes)
    {
        let mut table = RoutingTable::new(Id::random());
        for node in nodes[..100].iter().cloned() {
            table.add(node);
        }

        let fresh: Vec<_> = (0..1000).map(|_| Node::new(Id::random(), addr)).collect();
        let start = Instant::now();
        for node in fresh {
            table.add(node);
        }
        let per_op = start.elapsed().as_nanos() / 1000;
        println!("half-full table: {per_op}ns/op");
    }

    // Saturated table (400 nodes)
    {
        let mut table = RoutingTable::new(Id::random());
        for _ in 0..400 {
            table.add(Node::new(Id::random(), addr));
        }

        let fresh: Vec<_> = (0..1000).map(|_| Node::new(Id::random(), addr)).collect();
        let start = Instant::now();
        for node in fresh {
            table.add(node);
        }
        let per_op = start.elapsed().as_nanos() / 1000;
        println!("full table:      {per_op}ns/op");
    }

    println!();
}

fn bench_closest() {
    println!("closest");

    let addr = SocketAddrV4::from_str("127.0.0.1:6881").unwrap();
    let targets: Vec<_> = (0..1000).map(|_| Id::random()).collect();

    for size in [50, 100, 200, 400] {
        let mut table = RoutingTable::new(Id::random());
        for _ in 0..size {
            table.add(Node::new(Id::random(), addr));
        }

        let start = Instant::now();
        for target in &targets {
            let _ = table.closest(*target);
        }
        let per_op = start.elapsed().as_nanos() / targets.len() as u128;
        println!("{size:>3} nodes: {per_op}ns/op");
    }

    println!();
}

fn bench_remove() {
    println!("remove");

    let addr = SocketAddrV4::from_str("127.0.0.1:6881").unwrap();
    let mut table = RoutingTable::new(Id::random());

    let mut ids = Vec::new();
    for _ in 0..200 {
        let id = Id::random();
        ids.push(id);
        table.add(Node::new(id, addr));
    }

    let n = ids.len();
    let start = Instant::now();
    for id in &ids {
        table.remove(id);
    }
    let per_op = start.elapsed().as_nanos() / n as u128;
    println!("{per_op}ns/op ({n} removals)");
    println!();
}

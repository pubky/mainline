use std::{
    net::ToSocketAddrs,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::Duration,
};

use crate::{
    common::Id,
    messages::{MessageType, ResponseSpecific},
    routing_table::RoutingTable,
    rpc::Rpc,
};

const DEFAULT_BOOTSTRAP_NODES: [&str; 7] = [
    "dht.anacrolix.link:42069",
    "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401", // @arvidn's
    // Above work reliably in home network.
    "router.bittorrent.com:6881",
    "router.utorrent.com:6881",
    "dht.aelitis.com:6881", // Vuze
    // "router.bittorrent.cloud:42069", // Seems to be read-only.
    "router.silotis.us:6881", // IPv6
];

#[derive(Debug, Clone)]
pub struct Dht {
    id: Id,
    rpc: Rpc,
    routing_table: Arc<Mutex<RoutingTable>>,
}

impl Default for Dht {
    fn default() -> Self {
        Self::new()
    }
}

impl Dht {
    pub fn new() -> Self {
        let id = Id::random();
        let mut rpc = Rpc::new()
            .unwrap()
            .with_id(id)
            .with_request_timout(10000)
            .unwrap();

        let mut routing_table = RoutingTable::new().with_id(id);

        Self {
            id,
            rpc,
            routing_table: Mutex::new(routing_table).into(),
        }
    }

    pub fn bootstrap(&mut self) {}

    pub fn tick(&mut self) {
        let mut routing_table = self.routing_table.lock().unwrap();

        let incoming_message = self.rpc.tick();

        // TODO: Add node to the routing table.
        if let Ok(Some((message, from))) = incoming_message {
            println!(
                "Incoming message: from({:?})\nmessage:{:?}\n",
                from, message
            );
            dbg!(&routing_table);

            match &message.message_type {
                MessageType::Request(request) => {
                    // TODO: Handle requests.
                }
                MessageType::Response(response_specific) => match response_specific {
                    ResponseSpecific::PingResponse(find_node_response) => {
                        // TODO: how should we handle ping response?
                    }
                    ResponseSpecific::FindNodeResponse(find_node_response) => {
                        // TODO add the bootstrap node.

                        for node in find_node_response.nodes.iter() {
                            routing_table.add(node.clone());
                        }
                    }
                    _ => {}
                },
                MessageType::Error(error) => {
                    // TODO: use tracing.
                    println!("Got Error from {:?}: {:?}", from, error);
                }
            }

            // TODO: Add the node to the routing table if it makes sense.
        };

        // TODO: Verify Id according to bep42

        // TODO: If isolated try bootstrapping again.
        if routing_table.is_empty() {
            dbg!("Routing table is empty so we are bootstrapping");

            for bootstrap_node in DEFAULT_BOOTSTRAP_NODES {
                if let Ok(iter) = bootstrap_node.to_socket_addrs() {
                    for address in iter {
                        // TODO: Support IPv6, and don't call same node twice.
                        self.rpc.find_node(address, self.id);
                    }
                }
            }
        } else {
            dbg!("finding nodes from routing table");
            for node in routing_table.nodes() {
                self.rpc.find_node(node.address, self.id);
            }
        }

        thread::sleep(Duration::from_millis(500));
    }

    pub fn run(mut self) -> thread::JoinHandle<()> {
        thread::spawn(move || loop {
            self.tick();
        })
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;

    #[test]
    fn bootstrap() {
        let dht = Dht::default();

        dht.clone().run();

        // dht.bootstrap();
        thread::sleep(Duration::from_secs(3600));
    }
}

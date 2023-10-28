use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::common::Id;
use crate::messages::{
    FindNodeRequestArguments, Message, MessageType, PingRequestArguments, PingResponseArguments,
    RequestSpecific, ResponseSpecific,
};

use crate::socket::KrpcSocket;
use crate::Result;

const DEFAULT_PORT: u16 = 6881;
const DEFAULT_TIMEOUT_MILLIS: u64 = 2000;
const VERSION: &[u8] = "RS".as_bytes(); // The Mainline rust implementation.
const MTU: usize = 2048;

#[derive(Debug, Clone)]
pub struct Rpc {
    id: Id,
    socket: KrpcSocket,
}

impl Rpc {
    pub fn new() -> Result<Self> {
        // TODO: One day I might implement BEP42.
        let id = Id::random();

        let socket = KrpcSocket::new()?;

        Ok(Rpc { id, socket })
    }

    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.socket.read_only = read_only;
        self
    }

    /// Sets requests timeout in milliseconds
    pub fn with_request_timout(mut self, timeout: u64) -> Self {
        self.socket.request_timeout = Duration::from_millis(timeout);
        self
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn server_addr(&self) -> SocketAddr {
        self.socket.local_addr()
    }

    pub fn tick(&mut self) -> Result<Option<(Message, SocketAddr)>> {
        self.socket.tick()
    }

    /// === Responses methods ===

    pub fn ping(&mut self, address: SocketAddr) -> Result<Receiver<Message>> {
        self.socket.request(
            address,
            &RequestSpecific::PingRequest(PingRequestArguments {
                requester_id: self.id,
            }),
        )
    }

    pub fn find_node(&mut self, address: SocketAddr, target: Id) -> Result<Receiver<Message>> {
        self.socket.request(
            address,
            &RequestSpecific::FindNodeRequest(FindNodeRequestArguments {
                target,
                requester_id: self.id,
            }),
        )
    }

    /// Helper function to spawn a background thread
    fn run(mut self) -> thread::JoinHandle<()> {
        thread::spawn(move || loop {
            self.socket.tick();
        })
    }
}

#[cfg(test)]
mod test {
    use std::thread;

    use crate::messages::{FindNodeResponseArguments, PingResponseArguments};

    use super::*;
    use std::net::{SocketAddr, ToSocketAddrs};

    // #[test]
    // fn test_ping() {
    //     let server = Rpc::new().unwrap();
    //
    //     let mut server_clone = server.clone();
    //     thread::spawn(move || loop {
    //         if let Ok(Some((message, from))) = server_clone.tick() {
    //             match message.message_type {
    //                 MessageType::Request(request_specific) => match request_specific {
    //                     RequestSpecific::PingRequest(_) => {
    //                         server_clone
    //                             .respond(
    //                                 message.transaction_id,
    //                                 from,
    //                                 ResponseSpecific::PingResponse(PingResponseArguments {
    //                                     responder_id: server_clone.id,
    //                                 }),
    //                             )
    //                             .unwrap();
    //                     }
    //                     _ => {}
    //                 },
    //                 _ => {}
    //             }
    //         }
    //     });
    //
    //     let server_addr = server.server_addr();
    //
    //     // Start the client.
    //     let mut client = Rpc::new().unwrap();
    //
    //     client.clone().run();
    //
    //     let pong = client.ping(server_addr).unwrap().recv().unwrap();
    //     let pong2 = client.ping(server_addr).unwrap().recv().unwrap();
    //
    //     assert_eq!(pong.transaction_id, 0);
    //     assert_eq!(pong.version, Some(VERSION.into()), "local version 'rs'");
    //     assert_eq!(
    //         pong.message_type,
    //         MessageType::Response(ResponseSpecific::PingResponse(PingResponseArguments {
    //             responder_id: server.id
    //         }))
    //     );
    //     assert_eq!(
    //         pong.requester_ip.unwrap().port(),
    //         client.server_addr().port()
    //     );
    //     assert_eq!(pong2.transaction_id, 1);
    //     assert_eq!(
    //         client.outstanding_requests.lock().unwrap().len(),
    //         0,
    //         "Outstandng requests should be empty after receiving a response"
    //     );
    // }
    //
    // #[test]
    // fn test_find_node() {
    //     let server = Rpc::new().unwrap();
    //
    //     let mut server_clone = server.clone();
    //     thread::spawn(move || loop {
    //         if let Ok(Some((message, from))) = server_clone.tick() {
    //             match message.message_type {
    //                 MessageType::Request(request_specific) => match request_specific {
    //                     RequestSpecific::FindNodeRequest(_) => {
    //                         server_clone
    //                             .respond(
    //                                 message.transaction_id,
    //                                 from,
    //                                 ResponseSpecific::FindNodeResponse(FindNodeResponseArguments {
    //                                     responder_id: server_clone.id,
    //                                     nodes: vec![],
    //                                 }),
    //                             )
    //                             .unwrap();
    //                     }
    //                     _ => {}
    //                 },
    //                 _ => {}
    //             }
    //         }
    //     });
    //
    //     let server_addr = server.server_addr();
    //
    //     // Start the client.
    //     let mut client = Rpc::new().unwrap();
    //
    //     client.clone().run();
    //
    //     let find_node_response = client
    //         .find_node(server_addr, client.id)
    //         .unwrap()
    //         .recv()
    //         .unwrap();
    //
    //     assert_eq!(find_node_response.transaction_id, 0);
    //     assert_eq!(
    //         find_node_response.message_type,
    //         MessageType::Response(ResponseSpecific::FindNodeResponse(
    //             FindNodeResponseArguments {
    //                 responder_id: server.id,
    //                 nodes: vec![]
    //             }
    //         ))
    //     );
    //     assert_eq!(
    //         client.outstanding_requests.lock().unwrap().len(),
    //         0,
    //         "Outstandng requests should be empty after receiving a response"
    //     );
    // }
    //
    // // Live interoperability tests, should be removed before CI.
    // #[test]
    // fn test_live_ping() {
    //     let mut client = Rpc::new().unwrap();
    //     client.clone().run();
    //
    //     // TODO: resolve the address from DNS.
    //     let address: SocketAddr = "router.pkarr.org:6881"
    //         .to_socket_addrs()
    //         .unwrap()
    //         .next()
    //         .unwrap();
    //
    //     let _ = client.ping(address);
    // }
    //
    // // Live interoperability tests, should be removed before CI.
    // #[test]
    // fn test_live_find_node() {
    //     let mut client = Rpc::new().unwrap();
    //     client.clone().run();
    //
    //     // TODO: resolve the address from DNS.
    //     let address: SocketAddr = "router.bittorrent.com:6881"
    //         .to_socket_addrs()
    //         .unwrap()
    //         .next()
    //         .unwrap();
    //
    //     let _ = client.find_node(address, client.id);
    // }
}

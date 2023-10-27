use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::common::Id;
use crate::messages::{
    FindNodeRequestArguments, Message, MessageType, PingRequestArguments, RequestSpecific,
    ResponseSpecific,
};

use crate::Result;

const DEFAULT_PORT: u16 = 6881;
const DEFAULT_TIMEOUT_MILLIS: u64 = 2000;
const VERSION: &[u8] = "RS".as_bytes(); // The Mainline rust implementation.

#[derive(Debug)]
pub struct Rpc {
    id: Id,
    socket: UdpSocket,
    next_tid: u16,
    request_timeout: Duration,
    read_only: bool,
    // TODO: Use oneshot instead of mpsc sender?
    outstanding_requests: Arc<Mutex<HashMap<u16, OutstandingRequest>>>,
}

#[derive(Debug)]
struct OutstandingRequest {
    sent_at: Instant,
    sender: Sender<Message>,
}

impl Clone for Rpc {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            socket: self.socket.try_clone().unwrap(),
            next_tid: self.next_tid,
            request_timeout: self.request_timeout,
            read_only: self.read_only,
            outstanding_requests: self.outstanding_requests.clone(),
        }
    }
}

impl Rpc {
    pub fn new() -> Result<Self> {
        // TODO: One day I might implement BEP42.
        let id = Id::random();

        let socket = match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT))) {
            Ok(socket) => Ok(socket),
            Err(_) => UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))),
        }?;

        // TICK interval
        socket.set_read_timeout(Some(Duration::from_millis(DEFAULT_TIMEOUT_MILLIS / 4)))?;

        Ok(Rpc {
            id,
            socket: socket.into(),
            next_tid: 0,
            request_timeout: Duration::from_millis(DEFAULT_TIMEOUT_MILLIS),
            read_only: false,
            outstanding_requests: Mutex::new(HashMap::new()).into(),
        })
    }

    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Sets requests timeout in milliseconds
    pub fn with_request_timout(mut self, timeout: u64) -> Result<Self> {
        self.request_timeout = Duration::from_millis(timeout);
        // TICK interval
        self.socket
            .set_read_timeout(Some(Duration::from_millis(timeout / 4)))?;
        Ok(self)
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn server_addr(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }

    /// === Responses methods ===

    pub fn response_message(
        &self,
        transaction_id: u16,
        to: SocketAddr,
        response: ResponseSpecific,
    ) -> Message {
        Message {
            transaction_id,
            message_type: MessageType::Response(response),
            version: Some(VERSION.into()),
            read_only: Some(self.read_only),
            requester_ip: Some(to),
        }
    }

    pub fn respond(
        &self,
        transaction_id: u16,
        to: SocketAddr,
        response: ResponseSpecific,
    ) -> Result<()> {
        let socket = self.socket.try_clone()?;

        let response = self.response_message(transaction_id, to, response);

        let bytes = &response.clone().to_bytes()?;
        socket.send_to(bytes, to)?;

        Ok(())
    }

    fn try_recv_from(&self) -> Result<Option<(Message, SocketAddr)>> {
        let mut buf = [0u8; 1024];
        match self.socket.recv_from(&mut buf) {
            Ok((amt, from)) => {
                let message = Message::from_bytes(&buf[..amt])?;
                Ok(Some((message, from)))
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No data received within the timeout
                // println!("No data received");
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Performs multiple tasks:
    /// - Receives a message from the udp socket.
    /// - Sends responses or errors to outstanding_requests.
    /// - Cleans up timeout outstanding_requests.
    ///
    /// Because it awaits a message from the udp socket for 1/4 of the request timeout, calling
    /// this in a loop will behave as a tick function with that interval.
    pub fn tick(&mut self) -> Result<Option<(Message, SocketAddr)>> {
        let request = self.try_recv_from()?;

        let mut lock = self.outstanding_requests.lock().unwrap();

        if let Some((message, from)) = request {
            match message.message_type {
                // Requests
                MessageType::Request(_) => {
                    // Return requests to be handled by the caller, if the RPC is not read_only.
                    if self.read_only {
                        return Ok(None);
                    };
                    return Ok(Some((message, from)));
                }
                // Responses and errors
                _ => {
                    // Send responses or errors to outstanding_requests.
                    if let Some(outstanding_request) = lock.remove(&message.transaction_id) {
                        let _ = outstanding_request.sender.send(message.clone());
                    };
                }
            }
        };

        // Use the locked reference to iterate and remove timed-out requests
        lock.retain(|_, outstanding_request| {
            outstanding_request.sent_at.elapsed() <= self.request_timeout
        });

        Ok(None)
    }

    /// === Requests methods ===

    /// Increments self.next_tid and returns the previous value.
    /// Wraps around at u16::max_value();
    fn tid(&mut self) -> u16 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    /// Create a new request message with the given request specific arguments.
    fn request_message(&mut self, transaction_id: u16, request: RequestSpecific) -> Message {
        Message {
            transaction_id,
            message_type: MessageType::Request(request),
            version: Some(VERSION.into()),
            read_only: Some(self.read_only),
            // Only relevant in responses.
            requester_ip: None,
        }
    }

    /// Blocks until a response is received for a given transaction_id.
    /// Times out after the configured self.timeout.
    fn response(&mut self, transaction_id: u16) -> Receiver<Message> {
        let (response_tx, response_rx) = mpsc::channel();
        self.outstanding_requests.lock().unwrap().insert(
            transaction_id,
            OutstandingRequest {
                sent_at: Instant::now(),
                sender: response_tx,
            },
        );

        response_rx
    }

    pub fn ping(&mut self, address: SocketAddr) -> Result<Receiver<Message>> {
        let transaction_id = self.tid();
        let message = self.request_message(
            transaction_id,
            RequestSpecific::PingRequest(PingRequestArguments {
                requester_id: self.id,
            }),
        );

        // Send a message to the server.
        self.socket.send_to(&message.to_bytes()?, address)?;
        Ok(self.response(transaction_id))
    }

    pub fn find_node(&mut self, address: SocketAddr, target: Id) -> Result<Receiver<Message>> {
        let transaction_id = self.tid();
        let message = self.request_message(
            transaction_id,
            RequestSpecific::FindNodeRequest(FindNodeRequestArguments {
                target,
                requester_id: self.id,
            }),
        );

        // Send a message to the server.
        self.socket.send_to(&message.to_bytes()?, address)?;
        Ok(self.response(transaction_id))
    }

    /// Helper function to spawn a background thread
    fn run(mut self) -> thread::JoinHandle<()> {
        thread::spawn(move || loop {
            self.tick();
        })
    }
}

#[cfg(test)]
mod test {
    use std::thread;

    use crate::messages::{FindNodeResponseArguments, PingResponseArguments};

    use super::*;
    use std::net::{SocketAddr, ToSocketAddrs};

    #[test]
    fn test_tid() {
        let mut rpc = Rpc::new().unwrap();

        assert_eq!(rpc.tid(), 0);
        assert_eq!(rpc.tid(), 1);
        assert_eq!(rpc.tid(), 2);

        rpc.next_tid = u16::max_value();

        assert_eq!(rpc.tid(), 65535);
        assert_eq!(rpc.tid(), 0);
    }

    #[test]
    fn test_with_readonly() {
        let mut rpc = Rpc::new().unwrap();
        assert_eq!(rpc.read_only, false);
        let request = rpc.request_message(
            0,
            RequestSpecific::PingRequest(PingRequestArguments {
                requester_id: Id::random(),
            }),
        );
        assert_eq!(request.read_only, Some(false));

        let response = rpc.response_message(
            0,
            SocketAddr::from(([0, 0, 0, 0], 0)),
            ResponseSpecific::PingResponse(PingResponseArguments {
                responder_id: Id::random(),
            }),
        );
        assert_eq!(response.read_only, Some(false));

        let mut rpc = rpc.with_read_only(true);
        assert_eq!(rpc.read_only, true);

        let request = rpc.request_message(
            0,
            RequestSpecific::PingRequest(PingRequestArguments {
                requester_id: Id::random(),
            }),
        );
        assert_eq!(request.read_only, Some(true));

        let response = rpc.response_message(
            0,
            SocketAddr::from(([0, 0, 0, 0], 0)),
            ResponseSpecific::PingResponse(PingResponseArguments {
                responder_id: Id::random(),
            }),
        );
        assert_eq!(response.read_only, Some(true));
    }

    #[test]
    fn test_tick() {
        let server = Rpc::new().unwrap().with_read_only(true);

        let server_addr = server.server_addr();

        let mut server_clone = server.clone();
        thread::spawn(move || loop {
            let incoming = server_clone.tick().unwrap();
            assert_eq!(incoming, None, "read_only server does not receive requests")
        });

        // Start the client.
        let mut client = Rpc::new().unwrap().with_request_timout(100).unwrap();

        client.clone().run();

        // Test that tick does not return responses;
        client.respond(
            0,
            server_addr,
            ResponseSpecific::PingResponse(PingResponseArguments {
                responder_id: client.id,
            }),
        );

        client.ping(server_addr).unwrap();
        assert_eq!(client.outstanding_requests.lock().unwrap().len(), 1);

        thread::sleep(Duration::from_millis(200));
        assert_eq!(client.outstanding_requests.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_ping() {
        let server = Rpc::new().unwrap();

        let mut server_clone = server.clone();
        thread::spawn(move || loop {
            if let Ok(Some((message, from))) = server_clone.tick() {
                match message.message_type {
                    MessageType::Request(request_specific) => match request_specific {
                        RequestSpecific::PingRequest(_) => {
                            server_clone
                                .respond(
                                    message.transaction_id,
                                    from,
                                    ResponseSpecific::PingResponse(PingResponseArguments {
                                        responder_id: server_clone.id,
                                    }),
                                )
                                .unwrap();
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        });

        let server_addr = server.server_addr();

        // Start the client.
        let mut client = Rpc::new().unwrap();

        client.clone().run();

        let pong = client.ping(server_addr).unwrap().recv().unwrap();
        let pong2 = client.ping(server_addr).unwrap().recv().unwrap();

        assert_eq!(pong.transaction_id, 0);
        assert_eq!(pong.version, Some(VERSION.into()), "local version 'rs'");
        assert_eq!(
            pong.message_type,
            MessageType::Response(ResponseSpecific::PingResponse(PingResponseArguments {
                responder_id: server.id
            }))
        );
        assert_eq!(
            pong.requester_ip.unwrap().port(),
            client.server_addr().port()
        );
        assert_eq!(pong2.transaction_id, 1);
        assert_eq!(
            client.outstanding_requests.lock().unwrap().len(),
            0,
            "Outstandng requests should be empty after receiving a response"
        );
    }

    #[test]
    fn test_find_node() {
        let server = Rpc::new().unwrap();

        let mut server_clone = server.clone();
        thread::spawn(move || loop {
            if let Ok(Some((message, from))) = server_clone.tick() {
                match message.message_type {
                    MessageType::Request(request_specific) => match request_specific {
                        RequestSpecific::FindNodeRequest(_) => {
                            server_clone
                                .respond(
                                    message.transaction_id,
                                    from,
                                    ResponseSpecific::FindNodeResponse(FindNodeResponseArguments {
                                        responder_id: server_clone.id,
                                        nodes: vec![],
                                    }),
                                )
                                .unwrap();
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        });

        let server_addr = server.server_addr();

        // Start the client.
        let mut client = Rpc::new().unwrap();

        client.clone().run();

        let find_node_response = client
            .find_node(server_addr, client.id)
            .unwrap()
            .recv()
            .unwrap();

        assert_eq!(find_node_response.transaction_id, 0);
        assert_eq!(
            find_node_response.message_type,
            MessageType::Response(ResponseSpecific::FindNodeResponse(
                FindNodeResponseArguments {
                    responder_id: server.id,
                    nodes: vec![]
                }
            ))
        );
        assert_eq!(
            client.outstanding_requests.lock().unwrap().len(),
            0,
            "Outstandng requests should be empty after receiving a response"
        );
    }

    // Live interoperability tests, should be removed before CI.
    #[test]
    fn test_live_ping() {
        let mut client = Rpc::new().unwrap();
        client.clone().run();

        // TODO: resolve the address from DNS.
        let address: SocketAddr = "router.pkarr.org:6881"
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();

        let _ = client.ping(address);
    }

    // Live interoperability tests, should be removed before CI.
    #[test]
    fn test_live_find_node() {
        let mut client = Rpc::new().unwrap();
        client.clone().run();

        // TODO: resolve the address from DNS.
        let address: SocketAddr = "router.bittorrent.com:6881"
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();

        let _ = client.find_node(address, client.id);
    }
}

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::common::Id;
use crate::messages::{
    FindNodeRequestArguments, Message, MessageType, PingRequestArguments, PingResponseArguments,
    RequestSpecific, ResponseSpecific,
};
use crate::{Error, Result};

const DEFAULT_PORT: u16 = 6881;
const DEFAULT_TIMEOUT_MILLIS: u64 = 2000;

#[derive(Debug, Clone)]
struct Rpc {
    id: Id,
    socket: Arc<UdpSocket>,
    next_tid: u16,
    request_timeout: Duration,
    // TODO: Use oneshot instead of mpsc sender?
    outstanding_requests: Arc<Mutex<HashMap<u16, OutstandingRequest>>>,
}

#[derive(Debug)]
struct OutstandingRequest {
    sent_at: Instant,
    sender: Sender<Message>,
}

impl Rpc {
    pub fn new() -> Result<Rpc> {
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
            outstanding_requests: Arc::new(Mutex::new(HashMap::new())),
        })
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

    pub fn respond(
        &self,
        transaction_id: u16,
        to: SocketAddr,
        response: ResponseSpecific,
    ) -> Result<()> {
        let socket = self.socket.try_clone()?;

        let response = Message {
            transaction_id,
            message_type: MessageType::Response(response),
            // TODO: define our version.
            version: None,
            // TODO: configure. if false, we should disable respond or make it noop.
            read_only: Some(false),
            // Only relevant in responses.
            requester_ip: Some(to),
        };

        let bytes = &response.clone().to_bytes()?;
        socket.send_to(bytes, to)?;

        Ok(())
    }

    fn try_recv_from(&self) -> Result<Option<(Message, SocketAddr)>> {
        let mut buf = [0u8; 1024];
        match self.socket.recv_from(&mut buf) {
            Ok((amt, from)) => {
                let mut message = Message::from_bytes(&buf[..amt])?;
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
    fn tick(&mut self) -> Result<Option<(Message, SocketAddr)>> {
        let request = self.try_recv_from()?;

        let mut lock = self.outstanding_requests.lock().unwrap();

        // Send responses or errors to outstanding_requests.
        if let Some((message, _)) = &request {
            match message.message_type {
                MessageType::Request(_) => {}
                _ => {
                    if let Some(outstanding_request) = lock.remove(&message.transaction_id) {
                        outstanding_request.sender.send(message.clone());
                    }
                }
            }
        }

        // Use the locked reference to iterate and remove timed-out requests
        lock.retain(|_, outstanding_request| {
            outstanding_request.sent_at.elapsed() <= self.request_timeout
        });

        Ok(request)
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
            // TODO: define our version.
            version: None,
            // TODO: configure
            read_only: Some(true),
            // Only relevant in responses.
            requester_ip: None,
        }
    }

    /// Blocks until a response is received for a given transaction_id.
    /// Times out after the configured self.timeout.
    fn response(&mut self, transaction_id: u16) -> Receiver<Message> {
        // // TODO: Add timeout here!
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

    fn ping(&mut self, address: SocketAddr) -> Result<Receiver<Message>> {
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

    fn find_node(&mut self, address: SocketAddr, target: Id) -> Result<Receiver<Message>> {
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
}

#[cfg(test)]
mod test {
    use super::*;

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
    fn test_tick() {
        let server = Rpc::new().unwrap();

        let mut server_clone = server.clone();
        thread::spawn(move || loop {
            if let Ok(Some((message, from))) = server_clone.tick() {
                match message.message_type {
                    MessageType::Request(request_specific) => match request_specific {
                        RequestSpecific::PingRequest(args) => {
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

        let mut client_clone = client.clone();
        thread::spawn(move || loop {
            client_clone.tick();
            // Do nothing ... responses will be sent to the outstanding_requests senders.
        });

        let pong = client.ping(server_addr).unwrap().recv().unwrap();
        let pong2 = client.ping(server_addr).unwrap().recv().unwrap();

        assert_eq!(pong.transaction_id, 0);
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
    fn test_request_timeout() {
        let server = Rpc::new().unwrap();
        let server_addr = server.server_addr();

        // Start the client.
        let mut client = Rpc::new().unwrap().with_request_timout(100).unwrap();

        let mut client_clone = client.clone();
        thread::spawn(move || loop {
            client_clone.tick();
        });

        client.ping(server_addr).unwrap();
        assert_eq!(client.outstanding_requests.lock().unwrap().len(), 1);

        thread::sleep(Duration::from_millis(100));
        assert_eq!(client.outstanding_requests.lock().unwrap().len(), 0);
    }

    // Live interoperability tests, should be removed before CI.
    #[test]
    fn test_live_ping() {
        let mut client = Rpc::new().unwrap();
        let mut client_clone = client.clone();
        thread::spawn(move || loop {
            client_clone.tick();
        });

        // TODO: resolve the address from DNS.
        let address: SocketAddr = "67.215.246.10:6881".parse().unwrap();

        client.ping(address);
    }

    // Live interoperability tests, should be removed before CI.
    #[test]
    fn test_live_find_node() {
        let mut client = Rpc::new().unwrap();
        let mut client_clone = client.clone();
        thread::spawn(move || loop {
            client_clone.tick();
        });

        // TODO: resolve the address from DNS.
        let address: SocketAddr = "67.215.246.10:6881".parse().unwrap();

        client.find_node(address, client.id);
    }
}

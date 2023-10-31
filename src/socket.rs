use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::messages::{ErrorSpecific, Message, MessageType, RequestSpecific, ResponseSpecific};

use crate::Result;

const DEFAULT_PORT: u16 = 6881;
const DEFAULT_TIMEOUT_MILLIS: u64 = 2000;
const VERSION: &[u8] = "RS".as_bytes(); // The Mainline rust implementation.
const MTU: usize = 2048;

/// A UdpSocket wrapper that manages inflight requests and receive incoming messages.
#[derive(Debug)]
pub struct KrpcSocket {
    socket: UdpSocket,
    next_tid: u16,
    pub request_timeout: Duration,
    pub read_only: bool,
    inflight_requests: Arc<Mutex<HashMap<u16, InflightRequest>>>,
}

/// A registered Sender for an inflight request waiting for a response or be removed after a
/// timeout period since the `sent_at` Instant.
#[derive(Debug)]
struct InflightRequest {
    sent_at: Instant,
    sender: Sender<Message>,
}

impl Clone for KrpcSocket {
    fn clone(&self) -> Self {
        Self {
            socket: self.socket.try_clone().unwrap(),
            next_tid: self.next_tid,
            request_timeout: self.request_timeout,
            read_only: self.read_only,
            inflight_requests: self.inflight_requests.clone(),
        }
    }
}

impl KrpcSocket {
    pub fn new() -> Result<Self> {
        let socket = match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT))) {
            Ok(socket) => Ok(socket),
            Err(_) => UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))),
        }?;
        socket.set_nonblocking(true)?;

        Ok(Self {
            socket,
            next_tid: 0,
            request_timeout: Duration::from_millis(DEFAULT_TIMEOUT_MILLIS),
            read_only: false,
            inflight_requests: Mutex::new(HashMap::new()).into(),
        })
    }

    // === Options ===

    /// Set read-only mode
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Set request timeout
    pub fn with_request_timeout(mut self, timeout: u64) -> Self {
        self.request_timeout = Duration::from_millis(timeout);
        self
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }

    // === Public Methods ===

    /// Send a request to the given address and return a receiver for the response.
    pub fn request(&mut self, address: SocketAddr, request: &RequestSpecific) -> Receiver<Message> {
        let message = self.request_message(request.clone());

        let (response_tx, response_rx) = mpsc::channel();

        self.inflight_requests.lock().unwrap().insert(
            message.transaction_id,
            InflightRequest {
                sent_at: Instant::now(),
                sender: response_tx,
            },
        );

        self.send(address, message);

        // TODO: reconsider if we need an mpsc channel in this level of abstraction vs in Dht.
        response_rx
    }

    /// Send a response to the given address.
    pub fn response(
        &mut self,
        address: SocketAddr,
        transaction_id: u16,
        response: ResponseSpecific,
    ) -> Result<()> {
        let message =
            self.response_message(MessageType::Response(response), address, transaction_id);
        self.send(address, message)
    }

    /// Send an error to the given address.
    pub fn error(
        &mut self,
        address: SocketAddr,
        transaction_id: u16,
        error: ErrorSpecific,
    ) -> Result<()> {
        let message = self.response_message(MessageType::Error(error), address, transaction_id);
        self.send(address, message)
    }

    /// Receives a single krpc message on the socket.
    /// On success, returns the dht message and the origin.
    pub fn recv_from(&mut self) -> Option<(Message, SocketAddr)> {
        let mut buf = [0u8; MTU];

        let mut lock = self.inflight_requests.lock().unwrap();

        if let Ok((amt, from)) = self.socket.recv_from(&mut buf) {
            match Message::from_bytes(&buf[..amt]) {
                Ok(message) => {
                    match message.message_type {
                        // Requests
                        MessageType::Request(_) => {
                            // Return requests to be handled by the caller,
                            // if the RPC is not read_only.
                            if self.read_only {
                                return None;
                            };
                        }
                        // Responses and errors
                        _ => {
                            // Send responses or errors to inflight_requests.
                            if let Some(inflight_request) = lock.remove(&message.transaction_id) {
                                let _ = inflight_request.sender.send(message.clone());
                            };
                        }
                    }

                    return Some((message, from));
                }
                Err(err) => {
                    println!("Error parsing incoming message from {:?}: {:?}", from, err);
                }
            };
        };

        // Clean up timed out requests.
        lock.retain(|_, inflight_request| {
            inflight_request.sent_at.elapsed() <= self.request_timeout
        });

        None
    }

    // === Private Methods ===

    /// Increments self.next_tid and returns the previous value.
    fn tid(&mut self) -> u16 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    /// Set transactin_id, version and read_only
    fn request_message(&mut self, message: RequestSpecific) -> Message {
        let transaction_id = self.tid();

        Message {
            transaction_id,
            message_type: MessageType::Request(message),
            version: Some(VERSION.into()),
            read_only: Some(self.read_only),
            requester_ip: None,
        }
    }

    /// Same as request_message but with request transaction_id and the requester_ip.
    fn response_message(
        &mut self,
        message: MessageType,
        requester_ip: SocketAddr,
        request_tid: u16,
    ) -> Message {
        Message {
            transaction_id: request_tid,
            message_type: message,
            version: Some(VERSION.into()),
            read_only: Some(self.read_only),
            // BEP0042 Only relevant in responses.
            requester_ip: Some(requester_ip),
        }
    }

    /// Send a raw dht message
    fn send(&mut self, address: SocketAddr, message: Message) -> Result<()> {
        self.socket.send_to(&message.to_bytes()?, address)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::thread;

    use crate::{
        common::Id,
        messages::{PingRequestArguments, PingResponseArguments},
    };

    use super::*;

    #[test]
    fn tid() {
        let mut socket = KrpcSocket::new().unwrap();

        assert_eq!(socket.tid(), 0);
        assert_eq!(socket.tid(), 1);
        assert_eq!(socket.tid(), 2);

        socket.next_tid = u16::max_value();

        assert_eq!(socket.tid(), 65535);
        assert_eq!(socket.tid(), 0);
    }

    #[test]
    fn request_response() {
        let server = KrpcSocket::new().unwrap();
        let server_id = Id::random();

        let mut server_clone = server.clone();
        thread::spawn(move || loop {
            if let Some((message, from)) = server_clone.recv_from() {
                match message.message_type {
                    MessageType::Request(request_specific) => match request_specific {
                        RequestSpecific::PingRequest(_) => match message.transaction_id {
                            10 => {
                                server_clone.response(
                                    from,
                                    message.transaction_id,
                                    ResponseSpecific::PingResponse(PingResponseArguments {
                                        responder_id: server_id,
                                    }),
                                );
                            }
                            11 => {
                                server_clone.error(
                                    from,
                                    message.transaction_id,
                                    ErrorSpecific {
                                        code: 201,
                                        description: "Generic Error".to_string(),
                                    },
                                );
                            }
                            _ => {}
                        },
                        _ => {}
                    },
                    _ => {}
                }
            }
        });

        let server_addr = server.local_addr();

        // Start the client.
        let mut client = KrpcSocket::new().unwrap();
        client.next_tid = 10; // Just to make sure we get the correct tid in response

        let mut clone = client.clone();
        thread::spawn(move || loop {
            clone.recv_from();
        });

        let request = RequestSpecific::PingRequest(PingRequestArguments {
            requester_id: Id::random(),
        });

        let response = client.request(server_addr, &request).recv().unwrap();

        assert_eq!(
            response.requester_ip.unwrap().port(),
            client.local_addr().port()
        );
        assert_eq!(response.transaction_id, 10);
        assert_eq!(response.version, Some(VERSION.into()), "local version 'rs'");
        assert_eq!(
            response.message_type,
            MessageType::Response(ResponseSpecific::PingResponse(PingResponseArguments {
                responder_id: server_id
            }))
        );

        let error = client.request(server_addr, &request).recv().unwrap();

        assert_eq!(error.transaction_id, 11);
        assert_eq!(error.version, Some(VERSION.into()), "local version 'rs'");
        assert_eq!(
            error.message_type,
            MessageType::Error(ErrorSpecific {
                code: 201,
                description: "Generic Error".to_string(),
            })
        );

        assert_eq!(
            client.inflight_requests.lock().unwrap().len(),
            0,
            "inflight requests should be empty after receiving a response"
        );
    }

    #[test]
    fn timeout() {
        let server = KrpcSocket::new().unwrap();
        let server_addr = server.local_addr();

        let mut client = KrpcSocket::new().unwrap().with_request_timeout(10);

        let mut clone = client.clone();
        thread::spawn(move || loop {
            clone.recv_from();
        });

        let request = RequestSpecific::PingRequest(PingRequestArguments {
            requester_id: Id::random(),
        });

        let response = client.request(server_addr, &request).recv();

        assert!(response.is_err(), "timeout")
    }
}

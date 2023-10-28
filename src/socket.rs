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

    /// Send a request to the given address and return a receiver for the response.
    pub fn request(
        &mut self,
        address: SocketAddr,
        request: &RequestSpecific,
    ) -> Result<Receiver<Message>> {
        let message = self.wrap_message(MessageType::Request(request.clone()), None);

        let (response_tx, response_rx) = mpsc::channel();
        self.inflight_requests.lock().unwrap().insert(
            message.transaction_id,
            InflightRequest {
                sent_at: Instant::now(),
                sender: response_tx,
            },
        );

        self.send(address, message);

        Ok(response_rx)
    }

    /// Send a response to the given address.
    pub fn response(&mut self, address: SocketAddr, response: ResponseSpecific) -> Result<()> {
        let message = self.wrap_message(MessageType::Response(response), Some(address));
        self.send(address, message)
    }

    /// Send an error to the given address.
    pub fn error(&mut self, address: SocketAddr, error: ErrorSpecific) -> Result<()> {
        let message = self.wrap_message(MessageType::Error(error), None);
        self.send(address, message)
    }

    /// Process incoming messages and manage outstanding requests.
    ///
    /// This method performs the following tasks:
    /// 1. Tries to receive a message from the UDP socket.
    /// 2. If the message is a response, sends it to the corresponding inflight request.
    /// 3. Cleans up timed-out inflight requests.
    /// 4. Returns the incoming message and its sender(requester/responder) address.
    pub fn tick(&mut self) -> Result<Option<(Message, SocketAddr)>> {
        let request = self.try_recv_from()?;

        let mut lock = self.inflight_requests.lock().unwrap();

        if let Some((message, _)) = &request {
            match message.message_type {
                // Requests
                MessageType::Request(_) => {
                    // Return requests to be handled by the caller, if the RPC is not read_only.
                    if self.read_only {
                        return Ok(None);
                    };
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

        Ok(request)
    }

    /// Try to receive a message from the socket.
    fn try_recv_from(&self) -> Result<Option<(Message, SocketAddr)>> {
        let mut buf = [0u8; MTU];
        match self.socket.recv_from(&mut buf) {
            Ok((amt, from)) => {
                let message = Message::from_bytes(&buf[..amt])?;
                Ok(Some((message, from)))
            }
            // Windows
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(None),
            // Unix
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Increments self.next_tid and returns the previous value.
    fn tid(&mut self) -> u16 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }
    /// Set transactin_id, version and read_only
    fn wrap_message(&mut self, message: MessageType, requester_ip: Option<SocketAddr>) -> Message {
        let transaction_id = self.tid();

        Message {
            transaction_id,
            message_type: message,
            version: Some(VERSION.into()),
            read_only: Some(self.read_only),
            // BEP0042 Only relevant in responses.
            requester_ip,
        }
    }

    /// Send a raw dht message
    fn send(&mut self, address: SocketAddr, message: Message) -> Result<()> {
        self.socket.send_to(&message.to_bytes()?, address)?;
        Ok(())
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
    fn read_only() {
        let mut socket = KrpcSocket::new().unwrap();

        let request = socket.wrap_message(
            MessageType::Request(RequestSpecific::PingRequest(PingRequestArguments {
                requester_id: Id::random(),
            })),
            None,
        );

        assert_eq!(request.transaction_id, 0);
        assert_eq!(request.read_only, Some(false));

        let mut socket = socket.with_read_only(true);

        let request = socket.wrap_message(
            MessageType::Request(RequestSpecific::PingRequest(PingRequestArguments {
                requester_id: Id::random(),
            })),
            None,
        );

        assert_eq!(request.transaction_id, 1);
        assert_eq!(request.read_only, Some(true));
    }

    #[test]
    fn request_response() {
        let server = KrpcSocket::new().unwrap();
        let server_id = Id::random();

        let mut server_clone = server.clone();
        thread::spawn(move || loop {
            if let Ok(Some((message, from))) = server_clone.tick() {
                match message.message_type {
                    MessageType::Request(request_specific) => match request_specific {
                        RequestSpecific::PingRequest(_) => match message.transaction_id {
                            0 => {
                                server_clone.response(
                                    from,
                                    ResponseSpecific::PingResponse(PingResponseArguments {
                                        responder_id: server_id,
                                    }),
                                );
                            }
                            1 => {
                                server_clone.error(
                                    from,
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

        client.clone().run();

        let request = RequestSpecific::PingRequest(PingRequestArguments {
            requester_id: Id::random(),
        });

        let response = client
            .request(server_addr, &request)
            .unwrap()
            .recv()
            .unwrap();

        assert_eq!(
            response.requester_ip.unwrap().port(),
            client.local_addr().port()
        );
        assert_eq!(response.transaction_id, 0);
        assert_eq!(response.version, Some(VERSION.into()), "local version 'rs'");
        assert_eq!(
            response.message_type,
            MessageType::Response(ResponseSpecific::PingResponse(PingResponseArguments {
                responder_id: server_id
            }))
        );

        let error = client
            .request(server_addr, &request)
            .unwrap()
            .recv()
            .unwrap();

        assert_eq!(error.transaction_id, 1);
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
            "Outstandng requests should be empty after receiving a response"
        );
    }
}

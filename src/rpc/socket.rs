//! UDP socket layer managing incoming/outgoing requests and responses.

mod inflight_requests;

use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};
use tracing::{debug, error, trace};

use inflight_requests::InflightRequests;

use crate::common::{ErrorSpecific, Message, MessageType, RequestSpecific, ResponseSpecific};

use super::config::Config;

const VERSION: [u8; 4] = [82, 83, 0, 5]; // "RS" version 05
const MTU: usize = 8192; // Increased buffer for better throughput

pub const DEFAULT_PORT: u16 = 6881;
/// Default request timeout before abandoning an inflight request to a non-responding node.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_millis(1000); // 1 second for faster retries
/// Cleanup interval for expired inflight requests to avoid overhead on every recv
const INFLIGHT_CLEANUP_INTERVAL: Duration = Duration::from_millis(200);

/// A UdpSocket wrapper that formats and correlates DHT requests and responses.
#[derive(Debug)]
pub struct KrpcSocket {
    next_tid: u32,
    socket: UdpSocket,
    pub(crate) server_mode: bool,
    request_timeout: Duration,
    inflight_requests: InflightRequests,
    last_cleanup: Instant,

    local_addr: SocketAddrV4,
}


impl KrpcSocket {
    pub(crate) fn new(config: &Config) -> Result<Self, std::io::Error> {
        let request_timeout = config.request_timeout;
        let port = config.port;

        let socket = if let Some(port) = port {
            UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], port)))?
        } else {
            match UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT))) {
                Ok(socket) => Ok(socket),
                Err(_) => UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))),
            }?
        };

        let local_addr = match socket.local_addr()? {
            SocketAddr::V4(addr) => addr,
            SocketAddr::V6(_) => unimplemented!("KrpcSocket does not support Ipv6"),
        };

        socket.set_nonblocking(true)?;

        Ok(Self {
            socket,
            next_tid: 0,
            server_mode: config.server_mode,
            request_timeout,
            inflight_requests: InflightRequests::new(),
            last_cleanup: Instant::now(),

            local_addr,
        })
    }

    #[cfg(test)]
    pub(crate) fn server() -> Result<Self, std::io::Error> {
        Self::new(&Config {
            server_mode: true,
            ..Default::default()
        })
    }

    #[cfg(test)]
    pub(crate) fn client() -> Result<Self, std::io::Error> {
        Self::new(&Config::default())
    }

    // === Getters ===

    /// Returns the address the server is listening to.
    #[inline]
    pub fn local_addr(&self) -> SocketAddrV4 {
        self.local_addr
    }

    // === Public Methods ===

    /// Returns true if this message's transaction_id is still inflight
    pub fn inflight(&self, transaction_id: u32) -> bool {
        self.inflight_requests.contains(transaction_id)
    }

    /// Send a request to the given address and return the transaction_id
    pub fn request(&mut self, address: SocketAddrV4, request: RequestSpecific) -> u32 {
        let message = self.request_message(request);
        trace!(context = "socket_message_sending", message = ?message);

        let tid = message.transaction_id;
        self.inflight_requests.add(tid, address);
        let _ = self.send(address, message).map_err(|e| {
            debug!(?e, "Error sending request message");
        });
        tid
    }

    /// Send a response to the given address.
    pub fn response(
        &mut self,
        address: SocketAddrV4,
        transaction_id: u32,
        response: ResponseSpecific,
    ) {
        let message =
            self.response_message(MessageType::Response(response), address, transaction_id);
        trace!(context = "socket_message_sending", message = ?message);
        let _ = self.send(address, message).map_err(|e| {
            debug!(?e, "Error sending response message");
        });
    }

    /// Send an error to the given address.
    pub fn error(&mut self, address: SocketAddrV4, transaction_id: u32, error: ErrorSpecific) {
        let message = self.response_message(MessageType::Error(error), address, transaction_id);
        let _ = self.send(address, message).map_err(|e| {
            debug!(?e, "Error sending error message");
        });
    }

    /// Receives krpc messages from the socket.
    /// On success, returns the dht message and the origin.
    pub fn recv_from(&mut self) -> Option<(Message, SocketAddrV4)> {
        let mut buf = [0u8; MTU];

        // Lazy cleanup of expired requests - only occasionally for performance
        let now = Instant::now();
        if now.duration_since(self.last_cleanup) > INFLIGHT_CLEANUP_INTERVAL {
            self.last_cleanup = now;
            self.inflight_requests.cleanup(self.request_timeout);
        }

        match self.socket.recv_from(&mut buf) {
            Ok((amt, SocketAddr::V4(from))) => {
                let bytes = &buf[..amt];

                if from.port() == 0 {
                    trace!(
                        context = "socket_validation",
                        message = "Response from port 0"
                    );
                    return None;
                }

                match Message::from_bytes(bytes) {
                    Ok(message) => {
                        let should_return = match message.message_type {
                            MessageType::Request(_) => {
                                trace!(
                                    context = "socket_message_receiving",
                                    ?message,
                                    ?from,
                                    "Received request message"
                                );
                                true
                            }
                            MessageType::Response(_) => {
                                trace!(
                                    context = "socket_message_receiving",
                                    ?message,
                                    ?from,
                                    "Received response message"
                                );
                                self.is_expected_response(&message, &from)
                            }
                            MessageType::Error(_) => {
                                trace!(
                                    context = "socket_message_receiving",
                                    ?message,
                                    ?from,
                                    "Received error message"
                                );
                                self.is_expected_response(&message, &from)
                            }
                        };

                        if should_return {
                            return Some((message, from));
                        }
                    }
                    Err(error) => {
                        trace!(
                            context = "socket_error",
                            ?error,
                            ?from,
                            message = ?String::from_utf8_lossy(bytes),
                            "Received invalid Bencode message."
                        );
                    }
                }
            }
            Ok((_, SocketAddr::V6(_))) => {
                trace!(
                    context = "socket_validation",
                    message = "Received IPv6 packet"
                );
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Don't sleep here - let the actor loop control timing
            }
            Err(e) => {
                trace!(
                    context = "socket_error",
                    ?e,
                    "recv_from failed unexpectedly"
                );
            }
        }

        None
    }

    // === Private Methods ===

    fn is_expected_response(&mut self, message: &Message, from: &SocketAddrV4) -> bool {
        // Find and remove the matching inflight request
        if let Some(_request) = self.inflight_requests.remove(message.transaction_id, from) {
            return true;
        } else {
            trace!(
                context = "socket_validation",
                message = "Unexpected response id or wrong address"
            );
        }
        false
    }

    /// Increments self.next_tid and returns the previous value.
    fn tid(&mut self) -> u32 {
        // We don't bother much with reusing freed transaction ids,
        // since the timeout is so short we are unlikely to run out
        // of 4 billion ids in 1 second.
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
            version: Some(VERSION),
            read_only: !self.server_mode,
            requester_ip: None,
        }
    }

    /// Same as request_message but with request transaction_id and the requester_ip.
    fn response_message(
        &mut self,
        message: MessageType,
        requester_ip: SocketAddrV4,
        request_tid: u32,
    ) -> Message {
        Message {
            transaction_id: request_tid,
            message_type: message,
            version: Some(VERSION),
            read_only: !self.server_mode,
            // BEP_0042 Only relevant in responses.
            requester_ip: Some(requester_ip),
        }
    }

    /// Send a raw dht message
    fn send(&mut self, address: SocketAddrV4, message: Message) -> Result<(), SendMessageError> {
        self.socket.send_to(&message.to_bytes()?, address)?;
        trace!(context = "socket_message_sending", message = ?message);
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
/// Mainline crate error enum.
pub enum SendMessageError {
    /// Errors related to parsing DHT messages.
    #[error("Failed to parse packet bytes: {0}")]
    BencodeError(#[from] serde_bencode::Error),

    #[error(transparent)]
    /// Transparent [std::io::Error]
    IO(#[from] std::io::Error),
}


#[cfg(test)]
mod test {
    use std::thread;

    use crate::common::{Id, PingResponseArguments, RequestTypeSpecific};

    use super::*;

    #[test]
    fn tid() {
        let mut socket = KrpcSocket::server().unwrap();

        assert_eq!(socket.tid(), 0);
        assert_eq!(socket.tid(), 1);
        assert_eq!(socket.tid(), 2);

        socket.next_tid = u32::MAX;

        assert_eq!(socket.tid(), 4294967295);
        assert_eq!(socket.tid(), 0);
    }

    #[test]
    fn recv_request() {
        let mut server = KrpcSocket::server().unwrap();
        let server_address = server.local_addr();

        let mut client = KrpcSocket::client().unwrap();
        client.next_tid = 120;

        let client_address = client.local_addr();
        let request = RequestSpecific {
            requester_id: Id::random(),
            request_type: RequestTypeSpecific::Ping,
        };

        let expected_request = request.clone();

        let server_thread = thread::spawn(move || loop {
            if let Some((message, from)) = server.recv_from() {
                assert_eq!(from.port(), client_address.port());
                assert_eq!(message.transaction_id, 120);
                assert!(message.read_only, "Read-only should be true");
                assert_eq!(message.version, Some(VERSION), "Version should be 'RS'");
                assert_eq!(message.message_type, MessageType::Request(expected_request));
                break;
            }
        });

        client.request(server_address, request);

        server_thread.join().unwrap();
    }

    #[test]
    fn recv_response() {
        let (tx, rx) = flume::bounded(1);

        let mut client = KrpcSocket::client().unwrap();
        let client_address = client.local_addr();

        let responder_id = Id::random();
        let response = ResponseSpecific::Ping(PingResponseArguments { responder_id });

        let server_thread = thread::spawn(move || {
            let mut server = KrpcSocket::client().unwrap();
            let server_address = server.local_addr();
            tx.send(server_address).unwrap();

            loop {
                server.inflight_requests.add(8, client_address);

                if let Some((message, from)) = server.recv_from() {
                    assert_eq!(from.port(), client_address.port());
                    assert_eq!(message.transaction_id, 8);
                    assert!(message.read_only, "Read-only should be true");
                    assert_eq!(message.version, Some(VERSION), "Version should be 'RS'");
                    assert_eq!(
                        message.message_type,
                        MessageType::Response(ResponseSpecific::Ping(PingResponseArguments {
                            responder_id,
                        }))
                    );
                    break;
                }
            }
        });

        let server_address = rx.recv().unwrap();

        client.response(server_address, 8, response);

        server_thread.join().unwrap();
    }

    #[test]
    fn ignore_response_from_wrong_address() {
        let mut server = KrpcSocket::client().unwrap();
        let server_address = server.local_addr();

        let mut client = KrpcSocket::client().unwrap();

        let client_address = client.local_addr();

        server.inflight_requests.add(
            8,
            SocketAddrV4::new([127, 0, 0, 1].into(), client_address.port() + 1),
        );

        let response = ResponseSpecific::Ping(PingResponseArguments {
            responder_id: Id::random(),
        });

        let _ = response.clone();

        let server_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(5));
            assert!(
                server.recv_from().is_none(),
                "Should not receive a response from wrong address"
            );
        });

        client.response(server_address, 8, response);

        server_thread.join().unwrap();
    }
}

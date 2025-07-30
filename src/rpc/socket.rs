//! UDP socket layer managing incoming/outgoing requests and responses.
pub mod inflight_requests;

use super::config::Config;
use crate::common::{ErrorSpecific, Message, MessageType, RequestSpecific, ResponseSpecific};
use inflight_requests::InflightRequests;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::time::Duration;
use tracing::{debug, error, trace};

const VERSION: [u8; 4] = [82, 83, 0, 5]; // "RS" version 05
const MTU: usize = 2048;

pub const DEFAULT_PORT: u16 = 6881;
/// Timeouts before abandoning an inflight request to a non-responding node.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_millis(2000);
pub const MIN_REQUEST_TIMEOUT: Duration = Duration::from_millis(500);

/// A UdpSocket wrapper that formats and correlates DHT requests and responses.
#[derive(Debug)]
pub struct KrpcSocket {
    socket: UdpSocket,
    pub(crate) server_mode: bool,
    local_addr: SocketAddrV4,
    inflight_requests: InflightRequests,
    current_timeout: Duration,
}

impl KrpcSocket {
    pub(crate) fn new(config: &Config) -> Result<Self, std::io::Error> {
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
            server_mode: config.server_mode,
            local_addr,
            inflight_requests: InflightRequests::new(),
            current_timeout: DEFAULT_REQUEST_TIMEOUT,
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

    /// Returns true if this message's transaction_id is still inflight
    pub fn inflight(&self, transaction_id: u32) -> bool {
        self.inflight_requests
            .get(transaction_id, self.current_timeout)
            .is_some()
    }

    /// Send a request to the given address and return the transaction_id
    pub fn request(&mut self, address: SocketAddrV4, request: RequestSpecific) -> u32 {
        let transaction_id = self.inflight_requests.add(address);
        let message = self.request_message(transaction_id, request);
        trace!(context = "socket_message_sending", message = ?message);

        let tid = message.transaction_id;
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

    /// Receives a single krpc message on the socket.
    /// On success, returns the dht message and the origin.
    pub fn recv_from(&mut self) -> Option<(Message, SocketAddrV4)> {
        let mut buf = [0u8; MTU];

        // Use adaptive timeout for better performance
        let has_inflight = !self.inflight_requests.is_empty();
        self.current_timeout = if has_inflight {
            MIN_REQUEST_TIMEOUT
        } else {
            DEFAULT_REQUEST_TIMEOUT
        };

        // Cleanup inflight requests with current timeout
        self.inflight_requests.cleanup(self.current_timeout);

        // Try one non-blocking read, then fall through to timeout if it fails
        match self.socket.recv_from(&mut buf) {
            Ok((amt, SocketAddr::V4(from))) => {
                return self.process_packet(&mut buf, amt, from);
            }
            Ok((_, SocketAddr::V6(_))) => return None,
            Err(_) => {
                // Non-blocking read failed (no data), fall through to blocking timeout
            }
        }

        // Fall back to blocking timeout
        let _ = self.socket.set_read_timeout(Some(self.current_timeout));
        let result = match self.socket.recv_from(&mut buf) {
            Ok((amt, SocketAddr::V4(from))) => self.process_packet(&mut buf, amt, from),
            Ok((_, SocketAddr::V6(_))) => None,
            Err(_) => None,
        };
        let _ = self.socket.set_nonblocking(true);
        result
    }

    fn process_packet(
        &mut self,
        buf: &[u8; MTU],
        amt: usize,
        from: SocketAddrV4,
    ) -> Option<(Message, SocketAddrV4)> {
        if from.port() == 0 {
            trace!(
                context = "socket_validation",
                message = "Response from port 0"
            );
            return None;
        }

        let bytes = &buf[..amt];

        let message = match Message::from_bytes(bytes) {
            Ok(m) => m,
            Err(err) => {
                trace!(
                    context = "socket_error",
                    ?err,
                    ?from,
                    message = ?String::from_utf8_lossy(bytes),
                    "Invalid Bencode message"
                );
                return None;
            }
        };

        let ok = match &message.message_type {
            MessageType::Request(_) => true,
            MessageType::Response(_) | MessageType::Error(_) => {
                self.is_expected_response(&message, &from)
            }
        };

        if !ok {
            return None;
        }

        trace!(
            context = "socket_message_receiving",
            ?message,
            ?from,
            "Received message"
        );

        Some((message, from))
    }

    // === Private Methods ===

    fn is_expected_response(&mut self, message: &Message, from: &SocketAddrV4) -> bool {
        // Positive or an error response or to an inflight request.
        if let Some(request) = self.inflight_requests.remove(message.transaction_id) {
            if compare_socket_addr(&request.to, from) {
                return true;
            } else {
                trace!(
                    context = "socket_validation",
                    message = "Response from wrong address"
                );
            }
        } else {
            trace!(
                context = "socket_validation",
                message = "Unexpected response id"
            );
        }
        false
    }

    /// Set transactin_id, version and read_only
    fn request_message(&mut self, transaction_id: u32, message: RequestSpecific) -> Message {
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

// Same as SocketAddr::eq but ignores the ip if it is unspecified for testing reasons.
fn compare_socket_addr(a: &SocketAddrV4, b: &SocketAddrV4) -> bool {
    if a.port() != b.port() {
        return false;
    }

    if a.ip().is_unspecified() {
        return true;
    }

    a.ip() == b.ip()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::common::{Id, PingResponseArguments, RequestTypeSpecific};
    use inflight_requests::InflightRequest;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn tid() {
        let mut socket = KrpcSocket::server().unwrap();

        assert_eq!(socket.inflight_requests.get_next_tid(), 0);
        assert_eq!(socket.inflight_requests.get_next_tid(), 1);
        assert_eq!(socket.inflight_requests.get_next_tid(), 2);

        socket.inflight_requests.next_tid = u32::MAX;

        assert_eq!(socket.inflight_requests.get_next_tid(), 4294967295);
        assert_eq!(socket.inflight_requests.get_next_tid(), 0);
    }

    #[test]
    fn recv_request() {
        let mut server = KrpcSocket::server().unwrap();
        let server_address = server.local_addr();

        let mut client = KrpcSocket::client().unwrap();
        client.inflight_requests.next_tid = 120;

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

        std::thread::sleep(Duration::from_secs(2));
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

            server
                .inflight_requests
                .requests_by_tid
                .push(InflightRequest {
                    tid: 8,
                    to: client_address,
                    sent_at: Instant::now(),
                });

            loop {
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
    fn inflight_request_timeout() {
        let mut server = KrpcSocket::client().unwrap();

        let tid = 8;
        let sent_at = Instant::now();

        server
            .inflight_requests
            .requests_by_tid
            .push(InflightRequest {
                tid,
                to: SocketAddrV4::new([0, 0, 0, 0].into(), 0),
                sent_at,
            });

        std::thread::sleep(DEFAULT_REQUEST_TIMEOUT);

        assert!(!server.inflight(tid));
    }

    #[test]
    fn ignore_response_from_wrong_address() {
        let mut server = KrpcSocket::client().unwrap();
        let server_address = server.local_addr();

        let mut client = KrpcSocket::client().unwrap();

        let client_address = client.local_addr();

        server
            .inflight_requests
            .requests_by_tid
            .push(InflightRequest {
                tid: 8,
                to: SocketAddrV4::new([127, 0, 0, 1].into(), client_address.port() + 1),
                sent_at: Instant::now(),
            });

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

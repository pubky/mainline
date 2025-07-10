//! UDP socket layer managing incoming/outgoing requests and responses.

use libc::{setsockopt, SOL_SOCKET, SO_RCVBUF, SO_SNDBUF};
use std::collections::BTreeMap;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};
use tracing::{debug, error, trace};

use crate::common::{ErrorSpecific, Message, MessageType, RequestSpecific, ResponseSpecific};

use super::config::Config;

const VERSION: [u8; 4] = [82, 83, 0, 5]; // "RS" version 05
const MTU: usize = 2048;
const UDP_SOCKET_BUFFER_SIZE: i32 = 2 * 1024 * 1024; // 2MB

pub const DEFAULT_PORT: u16 = 6881;
/// Default request timeout before abandoning an inflight request to a non-responding node.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_millis(2000); // 2 seconds

/// A UdpSocket wrapper that formats and correlates DHT requests and responses.
#[derive(Debug)]
pub struct KrpcSocket {
    next_tid: u16,
    socket: UdpSocket,
    pub(crate) server_mode: bool,
    request_timeout: Duration,
    inflight_requests: BTreeMap<u16, InflightRequest>,

    local_addr: SocketAddrV4,
}

#[derive(Debug)]
pub struct InflightRequest {
    to: SocketAddrV4,
    sent_at: Instant,
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

        // Increase OS-level UDP socket buffers to prevent packet loss under high throughput.
        // The default buffer size (~128KB) is often too small for DHT traffic at scale.
        // This sets the size for both SO_RCVBUF and SO_SNDBUF.
        set_socket_buffers(&socket, UDP_SOCKET_BUFFER_SIZE)?;

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
            inflight_requests: BTreeMap::new(),

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
    pub fn inflight(&self, transaction_id: &u16) -> bool {
        self.inflight_requests.contains_key(transaction_id)
    }

    /// Send a request to the given address and return the transaction_id
    pub fn request(&mut self, address: SocketAddrV4, request: RequestSpecific) -> u16 {
        let message = self.request_message(request);
        trace!(context = "socket_message_sending", message = ?message);

        let tid = message.transaction_id;
        self.inflight_requests.insert(
            tid,
            InflightRequest {
                to: address,
                sent_at: Instant::now(),
            },
        );
        let _ = self.send(address, message).map_err(|e| {
            debug!(?e, "Error sending request message");
        });
        tid
    }

    /// Send a response to the given address.
    pub fn response(
        &mut self,
        address: SocketAddrV4,
        transaction_id: u16,
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
    pub fn error(&mut self, address: SocketAddrV4, transaction_id: u16, error: ErrorSpecific) {
        let message = self.response_message(MessageType::Error(error), address, transaction_id);
        let _ = self.send(address, message).map_err(|e| {
            debug!(?e, "Error sending error message");
        });
    }

    /// Receives a single krpc message on the socket.
    /// On success, returns the dht message and the origin.
    pub fn recv_from(&mut self) -> Option<(Message, SocketAddrV4)> {
        let mut buf = [0u8; MTU];

        let timeout = self.request_timeout;
        self.inflight_requests
            .retain(|_, req| req.sent_at.elapsed() <= timeout);

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
                std::thread::sleep(Duration::from_micros(100)); // yield for a bit
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
        // Positive or an error response or to an inflight request.
        if let Some(request) = self.inflight_requests.remove(&message.transaction_id) {
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

    /// Increments self.next_tid and returns the previous value.
    fn tid(&mut self) -> u16 {
        // We don't bother much with reusing freed transaction ids,
        // since the timeout is so short we are unlikely to run out
        // of 65535 ids in 2 seconds.
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
        request_tid: u16,
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

#[cfg(unix)]
pub fn set_socket_buffers(socket: &UdpSocket, size: i32) -> std::io::Result<()> {
    use std::io::Error;

    // Extract raw file descriptor for FFI calls
    let fd = socket.as_raw_fd();

    // Increase receive buffer to reduce packet drops under high load.
    let recv = unsafe {
        setsockopt(
            fd,
            SOL_SOCKET,
            SO_RCVBUF,
            &size as *const _ as *const _,
            std::mem::size_of_val(&size) as u32,
        )
    };

    // OS may clamp the size or reject large values depending on sysctl limits.
    if recv != 0 {
        return Err(Error::last_os_error());
    }

    // Increase send buffer; typically less critical for DHT than receive but still helpful for bursts.
    let send = unsafe {
        setsockopt(
            fd,
            SOL_SOCKET,
            SO_SNDBUF,
            &size as *const _ as *const _,
            std::mem::size_of_val(&size) as u32,
        )
    };
    if send != 0 {
        return Err(Error::last_os_error());
    }

    Ok(())
}

#[cfg(not(unix))]
fn set_socket_buffers(_socket: &UdpSocket, _size: i32) -> std::io::Result<()> {
    // No-op on non-Unix platforms; add implementation if needed (e.g., Windows WSA).
    Ok(())
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

        socket.next_tid = u16::MAX;

        assert_eq!(socket.tid(), 65535);
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
                server.inflight_requests.insert(
                    8,
                    InflightRequest {
                        to: client_address,
                        sent_at: Instant::now(),
                    },
                );

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

        server.inflight_requests.insert(
            8,
            InflightRequest {
                to: SocketAddrV4::new([127, 0, 0, 1].into(), client_address.port() + 1),
                sent_at: Instant::now(),
            },
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

    #[test]
    fn packet_loss_blocking() {
        let num_clients = 10;
        let requests_per_client = 1000;
        let client_request_rate = std::time::Duration::from_micros(500); // 2000 req/sec per client
        let test_timeout = std::time::Duration::from_secs(10);

        let server = std::sync::Arc::new(std::sync::Mutex::new(KrpcSocket::server().unwrap()));
        let server_addr = server.lock().unwrap().local_addr();

        let server_clone = std::sync::Arc::clone(&server);
        let server_handle = std::thread::spawn(move || {
            let mut responses = 0;
            let start = std::time::Instant::now();
            while start.elapsed() < test_timeout {
                let mut server = server_clone.lock().unwrap();
                if let Some((msg, from)) = server.recv_from() {
                    if let MessageType::Request(_) = msg.message_type {
                        let response = ResponseSpecific::Ping(PingResponseArguments {
                            responder_id: Id::random(),
                        });
                        server.response(from, msg.transaction_id, response);
                        responses += 1;
                    }
                }
                drop(server);
            }
            println!("Server sent {} responses", responses);
        });

        let mut client_handles = Vec::new();
        for _ in 0..num_clients {
            let server_addr = server_addr.clone();
            let handle = std::thread::spawn(move || {
                let mut client = KrpcSocket::client().unwrap();
                let request = RequestSpecific {
                    requester_id: Id::random(),
                    request_type: RequestTypeSpecific::Ping,
                };
                let mut sent = 0;
                let mut received = 0;
                for _ in 0..requests_per_client {
                    client.request(server_addr, request.clone());
                    sent += 1;
                    std::thread::sleep(client_request_rate);
                    // Try to receive responses quickly
                    for _ in 0..2 {
                        if let Some((msg, _)) = client.recv_from() {
                            if let MessageType::Response(_) = msg.message_type {
                                received += 1;
                            }
                        }
                    }
                }
                let response_timeout = std::time::Duration::from_secs(1);
                let response_start = std::time::Instant::now();
                while response_start.elapsed() < response_timeout && received < sent {
                    if let Some((msg, _)) = client.recv_from() {
                        if let MessageType::Response(_) = msg.message_type {
                            received += 1;
                        }
                    }
                }
                println!(
                    "Client sent {}, received {} (loss: {:.1}%)",
                    sent,
                    received,
                    100.0 * (sent - received) as f64 / sent as f64
                );
            });
            client_handles.push(handle);
        }

        for handle in client_handles {
            handle.join().unwrap();
        }
        server_handle.join().unwrap();
    }

    #[test]
    fn packet_loss_non_blocking() {
        let num_clients = 10;
        let requests_per_client = 1000;
        let client_request_rate = std::time::Duration::from_micros(500); // 2000 req/sec per client
        let test_timeout = std::time::Duration::from_secs(10);

        let server = std::sync::Arc::new(std::sync::Mutex::new(KrpcSocket::server().unwrap()));
        let server_addr = server.lock().unwrap().local_addr();

        let server_clone = std::sync::Arc::clone(&server);
        let server_handle = std::thread::spawn(move || {
            let mut responses = 0;
            let start = std::time::Instant::now();
            while start.elapsed() < test_timeout {
                let mut server = server_clone.lock().unwrap();
                for _ in 0..10 {
                    if let Some((msg, from)) = server.recv_from() {
                        if let MessageType::Request(_) = msg.message_type {
                            let response = ResponseSpecific::Ping(PingResponseArguments {
                                responder_id: Id::random(),
                            });
                            server.response(from, msg.transaction_id, response);
                            responses += 1;
                        }
                        break;
                    } else {
                        std::thread::sleep(Duration::from_micros(50));
                    }
                }
                drop(server);
            }
            println!("Server sent {} responses", responses);
        });

        let mut client_handles = Vec::new();
        for _ in 0..num_clients {
            let server_addr = server_addr.clone();
            let handle = std::thread::spawn(move || {
                let mut client = KrpcSocket::client().unwrap();
                let request = RequestSpecific {
                    requester_id: Id::random(),
                    request_type: RequestTypeSpecific::Ping,
                };
                let mut sent = 0;
                let mut received = 0;
                for _ in 0..requests_per_client {
                    client.request(server_addr, request.clone());
                    sent += 1;
                    std::thread::sleep(client_request_rate);
                    // Try to receive responses quickly
                    for _ in 0..10 {
                        if let Some((msg, _)) = client.recv_from() {
                            if let MessageType::Response(_) = msg.message_type {
                                received += 1;
                            }
                        } else {
                            std::thread::sleep(Duration::from_micros(50));
                        }
                    }
                }
                let response_timeout = std::time::Duration::from_secs(1);
                let response_start = std::time::Instant::now();
                while response_start.elapsed() < response_timeout && received < sent {
                    if let Some((msg, _)) = client.recv_from() {
                        if let MessageType::Response(_) = msg.message_type {
                            received += 1;
                        }
                    }
                }
                println!(
                    "Client sent {}, received {} (loss: {:.1}%)",
                    sent,
                    received,
                    100.0 * (sent - received) as f64 / sent as f64
                );
            });
            client_handles.push(handle);
        }

        for handle in client_handles {
            handle.join().unwrap();
        }
        server_handle.join().unwrap();
    }

    // Saturate a single core with UDP requests to validate reliability under stress
    #[test]
    fn max_cpu_no_loss_single_core() {
        use std::sync::Arc;

        let test_duration = Duration::from_secs(10);
        let timeout = Duration::from_secs(2);

        let server = Arc::new(std::sync::Mutex::new(KrpcSocket::server().unwrap()));
        let server_addr = server.lock().unwrap().local_addr();

        let server_clone = Arc::clone(&server);
        let server_handle = std::thread::spawn(move || {
            let start = Instant::now();
            while start.elapsed() < test_duration + timeout {
                let mut server = server_clone.lock().unwrap();
                while let Some((msg, from)) = server.recv_from() {
                    if let MessageType::Request(_) = msg.message_type {
                        let response = ResponseSpecific::Ping(PingResponseArguments {
                            responder_id: Id::random(),
                        });
                        server.response(from, msg.transaction_id, response);
                    }
                }
            }
        });

        let mut client = KrpcSocket::client().unwrap();
        let mut sent = 0;
        let mut received = 0;
        let mut inflight = BTreeMap::new();
        let start = Instant::now();

        // Simulate client sending as many requests as possible in a single thread
        while start.elapsed() < test_duration {
            let tid = client.request(
                server_addr,
                RequestSpecific {
                    requester_id: Id::random(),
                    request_type: RequestTypeSpecific::Ping,
                },
            );
            inflight.insert(tid, Instant::now());
            sent += 1;

            // Drain responses if available
            while let Some((msg, _)) = client.recv_from() {
                if let MessageType::Response(_) = msg.message_type {
                    inflight.remove(&msg.transaction_id);
                    received += 1;
                }
            }

            inflight.retain(|_, t| t.elapsed() < timeout);
        }

        // Give extra time to receive late responses
        let end_wait = Instant::now();
        while end_wait.elapsed() < timeout {
            while let Some((msg, _)) = client.recv_from() {
                if let MessageType::Response(_) = msg.message_type {
                    inflight.remove(&msg.transaction_id);
                    received += 1;
                }
            }
        }

        let lost = inflight.len();
        println!(
            "Sent: {}, Received: {}, Lost (timeouts): {} (loss {:.2}%)",
            sent,
            received,
            lost,
            (lost as f64 / sent as f64) * 100.0
        );

        server_handle.join().unwrap();
    }

    // Full-core saturation test to validate zero-loss under parallel load
    #[test]

    fn max_cpu_no_loss_multi_core() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        let test_duration = Duration::from_secs(10);
        let timeout = Duration::from_secs(2);
        let num_threads = num_cpus::get();

        let server = Arc::new(Mutex::new(KrpcSocket::server().unwrap()));
        let server_addr = server.lock().unwrap().local_addr();

        let server_clone = Arc::clone(&server);
        let server_handle = std::thread::spawn(move || {
            let start = Instant::now();
            while start.elapsed() < test_duration + timeout {
                let mut server = server_clone.lock().unwrap();
                while let Some((msg, from)) = server.recv_from() {
                    if let MessageType::Request(_) = msg.message_type {
                        let response = ResponseSpecific::Ping(PingResponseArguments {
                            responder_id: Id::random(),
                        });
                        server.response(from, msg.transaction_id, response);
                    }
                }
            }
        });

        let sent_total = Arc::new(AtomicUsize::new(0));
        let received_total = Arc::new(AtomicUsize::new(0));
        let lost_total = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::with_capacity(num_threads);

        // Launch N threads to simulate max core saturation
        for _ in 0..num_threads {
            let server_addr = server_addr.clone();
            let sent_total = Arc::clone(&sent_total);
            let received_total = Arc::clone(&received_total);
            let lost_total = Arc::clone(&lost_total);

            let handle = std::thread::spawn(move || {
                let mut client = KrpcSocket::client().unwrap();
                let mut inflight = BTreeMap::new();
                let start = Instant::now();

                while start.elapsed() < test_duration {
                    let tid = client.request(
                        server_addr,
                        RequestSpecific {
                            requester_id: Id::random(),
                            request_type: RequestTypeSpecific::Ping,
                        },
                    );
                    inflight.insert(tid, Instant::now());
                    sent_total.fetch_add(1, Ordering::Relaxed);

                    while let Some((msg, _)) = client.recv_from() {
                        if let MessageType::Response(_) = msg.message_type {
                            if inflight.remove(&msg.transaction_id).is_some() {
                                received_total.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }

                    inflight.retain(|_, t| t.elapsed() < timeout);
                }

                let end_wait = Instant::now();
                while end_wait.elapsed() < timeout {
                    while let Some((msg, _)) = client.recv_from() {
                        if let MessageType::Response(_) = msg.message_type {
                            if inflight.remove(&msg.transaction_id).is_some() {
                                received_total.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }

                let lost = inflight.len();
                lost_total.fetch_add(lost, Ordering::Relaxed);
            });

            handles.push(handle);
        }

        for h in handles {
            h.join().unwrap();
        }

        server_handle.join().unwrap();

        let sent = sent_total.load(Ordering::Relaxed);
        let received = received_total.load(Ordering::Relaxed);
        let lost = lost_total.load(Ordering::Relaxed);
        println!(
            "Threads: {}, Sent: {}, Received: {}, Lost: {} (loss {:.2}%)",
            num_threads,
            sent,
            received,
            lost,
            100.0 * lost as f64 / sent as f64
        );
    }
}

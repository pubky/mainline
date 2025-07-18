//! UDP socket layer managing incoming/outgoing requests and responses.

use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};
use tracing::{debug, error, trace};

use crate::common::{ErrorSpecific, Message, MessageType, RequestSpecific, ResponseSpecific};

use super::config::Config;

const VERSION: [u8; 4] = [82, 83, 0, 5]; // "RS" version 05
const MTU: usize = 2048;

pub const DEFAULT_PORT: u16 = 6881;
/// Default request timeout before abandoning an inflight request to a non-responding node.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_millis(2000); // 2 seconds
/// The maximum duration to backoff checking the [UdpSocket] buffer after it is empty.
/// Lower values increases CPU usage, but reduces latency, and drains the buffer faster,
/// reducing the risk of packet loss.
// TODO: Either add as an option to [Config] and [DhtBuilder],
//       Or see if refactoring [Rpc::tick] makes cpu usage nigligble for very low values.
pub const MAX_THREAD_BLOCK_DURATION: Duration = Duration::from_millis(10);

/// A UdpSocket wrapper that formats and correlates DHT requests and responses.
#[derive(Debug)]
pub struct KrpcSocket {
    next_tid: u16,
    socket: UdpSocket,
    pub(crate) server_mode: bool,
    local_addr: SocketAddrV4,
    inflight_requests: InflightRequestsMap,
}

#[derive(Debug, Clone)]
pub struct InflightRequest {
    to: SocketAddrV4,
    sent_at: Instant,
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
            next_tid: 0,
            server_mode: config.server_mode,
            local_addr,
            inflight_requests: InflightRequestsMap::new(config.request_timeout, Strategy::Legacy),
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
    pub fn inflight(&self, transaction_id: u16) -> bool {
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

        self.inflight_requests.cleanup();

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
                std::thread::sleep(MAX_THREAD_BLOCK_DURATION);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Legacy,
    Vec,
    HashMap,
}

/// InflightRequestsMap with selectable cleanup strategy.
#[derive(Debug)]
pub struct InflightRequestsMap {
    request_timeout: Duration,
    strategy: Strategy,
    // legacy fields
    requests: Vec<(u16, InflightRequest)>,
    // double-vec fields
    by_tid: Vec<(u16, InflightRequest)>, // sorted by TID
    by_time: Vec<(Instant, u16)>,        // sorted by timestamp
    // kevin fields
    map: HashMap<u16, InflightRequest>,
    queue: VecDeque<u16>, // FIFO of TIDs
}

impl InflightRequestsMap {
    /// Create a new map with the given timeout and strategy.
    pub fn new(request_timeout: Duration, strategy: Strategy) -> Self {
        Self {
            request_timeout,
            strategy,
            requests: Vec::new(),
            by_tid: Vec::new(),
            by_time: Vec::new(),
            map: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    pub fn insert(&mut self, tid: u16, req: InflightRequest) {
        match self.strategy {
            Strategy::Legacy => {
                self.requests.push((tid, req));
            }
            Strategy::Vec => {
                match self.by_tid.binary_search_by_key(&tid, |&(k, _)| k) {
                    Ok(_) => panic!("duplicate TID inserted"),
                    Err(idx) => self.by_tid.insert(idx, (tid, req.clone())),
                }
                let ts = req.sent_at;
                match self.by_time.binary_search_by_key(&ts, |&(ts, _)| ts) {
                    Ok(idx) | Err(idx) => self.by_time.insert(idx, (ts, tid)),
                }
            }
            Strategy::HashMap => {
                self.map.insert(tid, req.clone());
                self.queue.push_back(tid);
            }
        }
    }

    pub fn contains_key(&self, tid: u16) -> bool {
        let now = Instant::now();
        let timeout = self.request_timeout;
        match self.strategy {
            Strategy::Legacy => self
                .requests
                .iter()
                .any(|&(k, ref r)| k == tid && now.duration_since(r.sent_at) < timeout),
            Strategy::Vec => {
                if let Ok(idx) = self.by_tid.binary_search_by_key(&tid, |&(k, _)| k) {
                    let req = &self.by_tid[idx].1;
                    now.duration_since(req.sent_at) < timeout
                } else {
                    false
                }
            }
            Strategy::HashMap => {
                if let Some(req) = self.map.get(&tid) {
                    now.duration_since(req.sent_at) < timeout
                } else {
                    false
                }
            }
        }
    }

    pub fn remove(&mut self, tid: u16) -> Option<InflightRequest> {
        match self.strategy {
            Strategy::Legacy => {
                if let Some(pos) = self.requests.iter().position(|&(k, _)| k == tid) {
                    Some(self.requests.remove(pos).1)
                } else {
                    None
                }
            }
            Strategy::Vec => {
                if let Ok(idx) = self.by_tid.binary_search_by_key(&tid, |&(k, _)| k) {
                    let (_k, req) = self.by_tid.remove(idx);
                    if let Some(pos) = self.by_time.iter().position(|&(_, t)| t == tid) {
                        self.by_time.remove(pos);
                    }
                    Some(req)
                } else {
                    None
                }
            }
            Strategy::HashMap => {
                if let Some(req) = self.map.remove(&tid) {
                    self.queue.retain(|&x| x != tid);
                    Some(req)
                } else {
                    None
                }
            }
        }
    }

    pub fn cleanup(&mut self) {
        let now = Instant::now();
        let timeout = self.request_timeout;
        match self.strategy {
            Strategy::Legacy => {
                // exactly as original: only prune when full
                if self.requests.len() < self.requests.capacity() {
                    return;
                }
                let index = match self
                    .requests
                    .binary_search_by(|(_, request)| timeout.cmp(&request.sent_at.elapsed()))
                {
                    Ok(idx) => idx,
                    Err(idx) => idx,
                };
                self.requests = self.requests[index..].to_vec();
            }
            Strategy::Vec => {
                let idx = self
                    .by_time
                    .iter()
                    .position(|&(ts, _)| now.duration_since(ts) < timeout)
                    .unwrap_or(self.by_time.len());
                for &(_ts, tid) in &self.by_time[..idx] {
                    if let Ok(pos) = self.by_tid.binary_search_by_key(&tid, |&(k, _)| k) {
                        self.by_tid.remove(pos);
                    }
                }
                self.by_time.drain(0..idx);
            }
            Strategy::HashMap => {
                while let Some(&tid) = self.queue.front() {
                    let req = &self.map[&tid];
                    if now.duration_since(req.sent_at) < timeout {
                        break;
                    }
                    self.queue.pop_front();
                    self.map.remove(&tid);
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        match self.strategy {
            Strategy::Legacy => self.requests.len(),
            Strategy::Vec => self.by_tid.len(),
            Strategy::HashMap => self.map.len(),
        }
    }
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

            // Expect the request
            server.inflight_requests.insert(
                8,
                InflightRequest {
                    to: client_address,
                    sent_at: Instant::now(),
                },
            );

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
                    assert!(
                        server.inflight_requests.requests.is_empty(),
                        "receiving removes the inflight request"
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

        server.inflight_requests.insert(
            tid,
            InflightRequest {
                to: SocketAddrV4::new([0, 0, 0, 0].into(), 0),
                sent_at,
            },
        );

        std::thread::sleep(DEFAULT_REQUEST_TIMEOUT);

        assert!(!server.inflight(tid));
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
    fn test_cleanup() {
        let mut map = InflightRequestsMap::new(Duration::from_secs(5), Strategy::Legacy); // 5 second timeout

        // Add expired requests
        for i in 0..3 {
            let req = InflightRequest {
                to: SocketAddrV4::new([0, 0, 0, 0].into(), 0),
                sent_at: Instant::now() - Duration::from_secs(10),
            };
            map.insert(i as u16, req);
        }

        // Add fresh requests that should NOT be removed
        for i in 3..6 {
            let req = InflightRequest {
                to: SocketAddrV4::new([0, 0, 0, 0].into(), 0),
                sent_at: Instant::now(),
            };
            map.insert(i as u16, req);
        }

        println!(
            "Before cleanup: {} requests (3 expired, 3 fresh)",
            map.requests.len()
        );

        map.cleanup();

        println!("After cleanup: {} requests", map.requests.len());

        // Should have 3 fresh requests remaining, but binary search approach will fail this
        assert_eq!(
            map.requests.len(),
            3,
            "Cleanup should only remove the 3 expired requests, keeping the 3 fresh ones"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;
    use std::net::SocketAddrV4;
    use std::time::Instant;

    #[test]
    fn test_insert_contains_remove_cleanup() {
        for &strategy in &[Strategy::Legacy, Strategy::Vec, Strategy::HashMap] {
            let timeout = Duration::from_millis(10);
            let mut map = InflightRequestsMap::new(timeout, strategy);
            // insert one fresh and one expired
            let now = Instant::now();
            map.insert(1, make_req(now));
            map.insert(2, make_req(now - Duration::from_secs(1)));
            assert!(map.contains_key(1), "fresh entry should be present");
            assert!(!map.contains_key(2), "expired entry should not be present");
            // remove the fresh one
            assert!(map.remove(1).is_some());
            assert!(
                !map.contains_key(1),
                "removed entry should no longer be present"
            );
            // cleanup should drop the expired entry
            map.cleanup();
            assert!(!map.contains_key(2), "cleanup should remove expired entry");
        }
    }

    #[test]
    fn test_legacy_strategy() {
        let timeout = Duration::from_millis(10);
        let mut map = InflightRequestsMap::new(timeout, Strategy::Legacy);
        map.insert(42, make_req(Instant::now()));
        assert!(map.contains_key(42));
        assert_eq!(map.remove(42).unwrap().to.port(), 0);
        map.insert(7, make_req(Instant::now() - Duration::from_secs(1)));
        map.cleanup();
        assert!(!map.contains_key(7));
    }

    #[test]
    fn test_doublevec_strategy() {
        let timeout = Duration::from_millis(10);
        let mut map = InflightRequestsMap::new(timeout, Strategy::Vec);
        map.insert(42, make_req(Instant::now()));
        assert!(map.contains_key(42));
        assert_eq!(map.remove(42).unwrap().to.port(), 0);
        map.insert(7, make_req(Instant::now() - Duration::from_secs(1)));
        map.cleanup();
        assert!(!map.contains_key(7));
    }

    #[test]
    fn test_hashmap_strategy() {
        let timeout = Duration::from_millis(10);
        let mut map = InflightRequestsMap::new(timeout, Strategy::HashMap);
        map.insert(42, make_req(Instant::now()));
        assert!(map.contains_key(42));
        assert_eq!(map.remove(42).unwrap().to.port(), 0);
        map.insert(7, make_req(Instant::now() - Duration::from_secs(1)));
        map.cleanup();
        assert!(!map.contains_key(7));
    }

    fn make_req(sent_at: Instant) -> InflightRequest {
        InflightRequest {
            to: SocketAddrV4::new([0, 0, 0, 0].into(), 0),
            sent_at,
        }
    }

    const N: usize = 2_000;

    fn benchmark_insert(map: &mut InflightRequestsMap) {
        let start = Instant::now();
        for i in 0..N {
            map.insert(i as u16, make_req(Instant::now()));
        }
        println!("insert: {:?}", start.elapsed());
    }

    fn benchmark_contains(map: &InflightRequestsMap) {
        let start = Instant::now();
        for i in 0..N {
            let _ = map.contains_key(i as u16);
        }
        println!("contains_key: {:?}", start.elapsed());
    }

    fn benchmark_remove(map: &mut InflightRequestsMap) {
        let start = Instant::now();
        for i in 0..N {
            let _ = map.remove(i as u16);
        }
        println!("remove: {:?}", start.elapsed());
    }

    fn benchmark_cleanup(map: &mut InflightRequestsMap) {
        let start = Instant::now();
        map.cleanup();
        println!("cleanup: {:?}", start.elapsed());
    }

    fn run_bench(strategy: Strategy) {
        println!(
            "
Benchmarking strategy: {:?}",
            strategy
        );
        let mut map = InflightRequestsMap::new(Duration::from_secs(60), strategy);
        // insert
        benchmark_insert(&mut map);
        // contains
        benchmark_contains(&map);
        // remove half
        benchmark_remove(&mut map);
        // cleanup
        benchmark_cleanup(&mut map);
    }

    #[test]
    fn bench_all() {
        run_bench(Strategy::Legacy);
        run_bench(Strategy::Vec);
        run_bench(Strategy::HashMap);
    }

    #[test]
    fn bench_vec_vs_hashmap_realistic() {
        const N: usize = 1_000; // typical inflight load
        const RUNS: usize = 5;
        let timeout = Duration::from_secs(60);
        let dummy_addr = SocketAddrV4::new([0, 0, 0, 0].into(), 0);

        // 1) Generate N unique sequential TIDs
        let tids: Vec<u16> = (0u16..).take(N).collect();

        // 2) Generate N strictly increasing timestamps (10µs apart)
        let base = Instant::now();
        let timestamps: Vec<Instant> = (0..N)
            .map(|i| base + Duration::from_micros((i as u64) * 10))
            .collect();

        let run_bench = |strategy: Strategy| -> Duration {
            // build fresh map and pre‑reserve
            let mut map = InflightRequestsMap::new(timeout, strategy);
            match strategy {
                Strategy::Vec => {
                    map.by_tid.reserve(N);
                    map.by_time.reserve(N);
                }
                Strategy::HashMap => {
                    map.map.reserve(N);
                    map.queue.reserve(N);
                }
                _ => unreachable!(),
            }

            let mut total = Duration::ZERO;
            for _ in 0..RUNS {
                let start = Instant::now();

                // bulk insert
                for (&tid, &ts) in tids.iter().zip(&timestamps) {
                    map.insert(
                        black_box(tid),
                        black_box(InflightRequest {
                            to: dummy_addr,
                            sent_at: ts,
                        }),
                    );
                }
                // contains
                for &tid in &tids {
                    black_box(map.contains_key(black_box(tid)));
                }
                // remove half
                for &tid in tids.iter().step_by(2) {
                    black_box(map.remove(black_box(tid)));
                }
                // cleanup
                map.cleanup();

                total += start.elapsed();

                // reset for next iteration
                map = InflightRequestsMap::new(timeout, strategy);
                if strategy == Strategy::Vec {
                    map.by_tid.reserve(N);
                    map.by_time.reserve(N);
                } else {
                    map.map.reserve(N);
                    map.queue.reserve(N);
                }
            }
            total / (RUNS as u32)
        };

        let dvec = run_bench(Strategy::Vec);
        let hmap = run_bench(Strategy::HashMap);

        println!("Dual‑Vec avg: {:?}", dvec);
        println!("HashMap+VecDeque avg: {:?}", hmap);

        // you can assert whichever you expect to win:
        assert!(
            dvec < hmap,
            "Expected Vec strategy faster; got {:?} vs {:?}",
            dvec,
            hmap
        );
    }
}

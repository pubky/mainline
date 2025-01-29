//! Dht node.

use std::{
    collections::HashMap,
    fmt::Formatter,
    net::{Ipv4Addr, SocketAddrV4},
    thread,
    time::Duration,
};

use flume::{Receiver, Sender};

use tracing::info;

use crate::{
    common::{
        hash_immutable, AnnouncePeerRequestArguments, FindNodeRequestArguments,
        GetPeersRequestArguments, GetValueRequestArguments, Id, MutableItem,
        PutImmutableRequestArguments, PutMutableRequestArguments, PutRequestSpecific,
    },
    rpc::{ConcurrencyError, GetRequestSpecific, Info, PutError, PutQueryError, Response, Rpc},
    server::Server,
    Node,
};

pub use crate::rpc::Config;

#[derive(Debug, Clone)]
/// Mainline Dht node.
pub struct Dht(pub(crate) Sender<ActorMessage>);

#[derive(Debug, Default)]
pub struct DhtBuilder(Config);

impl DhtBuilder {
    /// Set [Config::server_mode].
    pub fn server_mode(mut self) -> Self {
        self.0.server_mode = true;

        self
    }

    /// Set [Config::server]
    pub fn custom_server(mut self, custom_server: Box<dyn Server>) -> Self {
        self.0.server = Some(custom_server);

        self
    }

    /// Set [Config::bootstrap]
    pub fn bootstrap(mut self, bootstrap: &[String]) -> Self {
        self.0.bootstrap = bootstrap.to_vec();

        self
    }

    /// Add more bootstrap nodes to [Config::bootstrap].
    ///
    /// Useful when you want to augment the default bootstrapping nodes with
    /// dynamic list of nodes you have seen in previous sessions.
    pub fn extra_bootstrap(mut self, extra_bootstrap: &[String]) -> Self {
        for node in extra_bootstrap {
            self.0.bootstrap.push(node.clone());
        }

        self
    }

    /// Set [Config::port]
    pub fn port(mut self, port: u16) -> Self {
        self.0.port = Some(port);

        self
    }

    /// Set [Config::public_ip]
    pub fn public_ip(mut self, public_ip: Ipv4Addr) -> Self {
        self.0.public_ip = Some(public_ip);

        self
    }

    /// Set [Config::request_timeout]
    pub fn request_timeout(mut self, request_timeout: Duration) -> Self {
        self.0.request_timeout = request_timeout;

        self
    }

    /// Create a Dht node.
    pub fn build(self) -> Result<Dht, std::io::Error> {
        Dht::new(self.0)
    }
}

impl Dht {
    /// Create a new Dht node.
    ///
    /// Could return an error if it failed to bind to the specified
    /// port or other io errors while binding the udp socket.
    pub fn new(config: Config) -> Result<Self, std::io::Error> {
        let (sender, receiver) = flume::unbounded();

        thread::Builder::new()
            .name("Mainline Dht actor thread".to_string())
            .spawn(move || run(config, receiver))?;

        let (tx, rx) = flume::bounded(1);

        sender
            .send(ActorMessage::Check(tx))
            .expect("actor thread unexpectedly shutdown");

        rx.recv().expect("infallible")?;

        Ok(Dht(sender))
    }

    /// Returns a builder to edit settings before creating a Dht node.
    pub fn builder() -> DhtBuilder {
        DhtBuilder::default()
    }

    /// Create a new DHT client with default bootstrap nodes.
    pub fn client() -> Result<Self, std::io::Error> {
        Dht::builder().build()
    }

    /// Create a new DHT node that is running in [Server mode][Config::server_mode] as
    /// soon as possible.
    ///
    /// You shouldn't use this option unless you are sure your
    /// DHT node is publicly accessible (not firewalled) _AND_ will be long running,
    /// and/or you are running your own local network for testing.
    ///
    /// If you are not sure, use [Self::client] and it will switch
    /// to server mode when/if these two conditions are met.
    pub fn server() -> Result<Self, std::io::Error> {
        Dht::builder().server_mode().build()
    }

    // === Getters ===

    /// Information and statistics about this [Dht] node.
    pub fn info(&self) -> Result<Info, DhtWasShutdown> {
        let (sender, receiver) = flume::bounded::<Info>(1);

        self.0
            .send(ActorMessage::Info(sender))
            .map_err(|_| DhtWasShutdown)?;

        receiver.recv().map_err(|_| DhtWasShutdown)
    }

    /// Turn this node's routing table to a list of bootstraping nodes.   
    pub fn to_bootstrap(&self) -> Result<Vec<String>, DhtWasShutdown> {
        let (sender, receiver) = flume::bounded::<Vec<String>>(1);

        self.0
            .send(ActorMessage::ToBootstrap(sender))
            .map_err(|_| DhtWasShutdown)?;

        receiver.recv().map_err(|_| DhtWasShutdown)
    }

    // === Public Methods ===

    /// Shutdown the actor thread loop.
    pub fn shutdown(&mut self) {
        let (sender, receiver) = flume::bounded::<()>(1);

        let _ = self.0.send(ActorMessage::Shutdown(sender));
        let _ = receiver.recv();
    }

    /// Block until the bootstraping query is done.
    ///
    /// Returns true if the bootstraping was successful.
    pub fn bootstrapped(&self) -> Result<bool, DhtWasShutdown> {
        let info = self.info()?;

        let nodes = self.find_node(*info.id())?;

        Ok(!nodes.is_empty())
    }

    // === Find nodes ===

    /// Returns the closest 20 [secure](Node::is_secure) nodes to a target [Id].
    ///
    /// Mostly useful to crawl the DHT. You might need to ping them to confirm they exist,
    /// and responsive, or if you want to learn more about them like the client they are using,
    /// or if they support a given BEP.
    pub fn find_node(&self, target: Id) -> Result<Box<[Node]>, DhtWasShutdown> {
        let (sender, receiver) = flume::bounded::<Box<[Node]>>(1);

        let request = GetRequestSpecific::FindNode(FindNodeRequestArguments { target });

        self.0
            .send(ActorMessage::Get(
                target,
                request,
                ResponseSender::ClosestNodes(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver
            .recv()
            .expect("Query was dropped before sending a response, please open an issue."))
    }

    // === Peers ===

    /// Get peers for a given infohash.
    ///
    /// Note: each node of the network will only return a _random_ subset (usually 20)
    /// of the total peers it has for a given infohash, so if you are getting responses
    /// from 20 nodes, you can expect up to 400 peers in total, but if there are more
    /// announced peers on that infohash, you are likely to miss some, the logic here
    /// for Bittorrent is that any peer will introduce you to more peers through "peer exchange"
    /// so if you are implementing something different from Bittorrent, you might want
    /// to implement your own logic for gossipping more peers after you discover the first ones.
    pub fn get_peers(
        &self,
        info_hash: Id,
    ) -> Result<GetIterator<Vec<SocketAddrV4>>, DhtWasShutdown> {
        // Get requests use unbounded channels to avoid blocking in the run loop.
        // Other requests like put_* and getters don't need that and is ok with
        // bounded channel with 1 capacity since it only ever sends one message back.
        //
        // So, if it is a ResponseMessage<_>, it should be unbounded, otherwise bounded.
        let (sender, receiver) = flume::unbounded::<Vec<SocketAddrV4>>();

        let request = GetRequestSpecific::GetPeers(GetPeersRequestArguments { info_hash });

        self.0
            .send(ActorMessage::Get(
                info_hash,
                request,
                ResponseSender::Peers(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(GetIterator(receiver.into_iter()))
    }

    /// Announce a peer for a given infohash.
    ///
    /// The peer will be announced on this process IP.
    /// If explicit port is passed, it will be used, otherwise the port will be implicitly
    /// assumed by remote nodes to be the same ase port they recieved the request from.
    pub fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<Id, AnnouncePeerError> {
        let (sender, receiver) = flume::bounded::<Result<Id, PutError>>(1);

        let (port, implied_port) = match port {
            Some(port) => (port, None),
            None => (0, Some(true)),
        };

        let request = PutRequestSpecific::AnnouncePeer(AnnouncePeerRequestArguments {
            info_hash,
            port,
            implied_port,
        });

        self.0
            .send(ActorMessage::Put(info_hash, request, sender))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver
            .recv()
            .expect("Query was dropped before sending a response, please open an issue.")
            .map_err(|error| match error {
                PutError::Query(error) => error,
                PutError::Concurrency(_) => {
                    unreachable!("should not receieve a concurrency error from announce peer query")
                }
            })?)
    }

    // === Immutable data ===

    /// Get an Immutable data by its sha1 hash.
    pub fn get_immutable(&self, target: Id) -> Result<Option<Box<[u8]>>, DhtWasShutdown> {
        let (sender, receiver) = flume::unbounded::<Box<[u8]>>();

        let request = GetRequestSpecific::GetValue(GetValueRequestArguments {
            target,
            seq: None,
            salt: None,
        });

        self.0
            .send(ActorMessage::Get(
                target,
                request,
                ResponseSender::Immutable(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver.recv().map(Some).unwrap_or(None))
    }

    /// Put an immutable data to the DHT.
    pub fn put_immutable(&self, value: &[u8]) -> Result<Id, PutImmutableError> {
        let target: Id = hash_immutable(value).into();

        let (sender, receiver) = flume::bounded::<Result<Id, PutError>>(1);

        let request = PutRequestSpecific::PutImmutable(PutImmutableRequestArguments {
            target,
            v: value.into(),
        });

        self.0
            .send(ActorMessage::Put(target, request, sender))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver
            .recv()
            .expect("Query was dropped before sending a response, please open an issue.")
            .map_err(|error| match error {
                PutError::Query(error) => error,
                PutError::Concurrency(_) => {
                    unreachable!("should not receieve a concurrency error from put immutable query")
                }
            })?)
    }

    // === Mutable data ===

    /// Get a mutable data by its `public_key` and optional `salt`.
    ///
    /// You can specify the exact `seq` you are looking for, otherwise,
    /// nodes will respond with any item with the same `public_key` and `salt`.
    ///
    /// # Order
    ///
    /// The order of [MutableItem]s returned by this iterator is not guaranteed to
    /// reflect their `seq` value. You should not assume that the later items are
    /// more recent than earlier ones.
    ///
    /// Consider using [Self::get_mutable_most_recent] if that is what you need.
    pub fn get_mutable(
        &self,
        public_key: &[u8; 32],
        salt: Option<&[u8]>,
        seq: Option<i64>,
    ) -> Result<GetIterator<MutableItem>, DhtWasShutdown> {
        let salt = salt.map(|s| s.into());

        let target = MutableItem::target_from_key(public_key, salt.as_deref());

        let (sender, receiver) = flume::unbounded::<MutableItem>();

        let request = GetRequestSpecific::GetValue(GetValueRequestArguments { target, seq, salt });

        self.0
            .send(ActorMessage::Get(
                target,
                request,
                ResponseSender::Mutable(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(GetIterator(receiver.into_iter()))
    }

    /// Get the most recent [MutableItem] from the network.
    pub fn get_mutable_most_recent(
        &self,
        public_key: &[u8; 32],
        salt: Option<&[u8]>,
    ) -> Result<Option<MutableItem>, DhtWasShutdown> {
        let mut most_recent: Option<MutableItem> = None;

        let iter = self.get_mutable(public_key, salt, None)?;

        for item in iter {
            if let Some(mr) = &most_recent {
                if item.seq() == mr.seq && item.value() > &mr.value {
                    most_recent = Some(item)
                }
            } else {
                most_recent = Some(item);
            }
        }

        Ok(most_recent)
    }

    /// Put a mutable data to the DHT.
    ///
    /// # Lost Update Problem
    ///
    /// As mainline DHT is a distributed system, it is vulnerable to [Write–write conflict](https://en.wikipedia.org/wiki/Write-write_conflict).
    ///
    /// ## Read first
    ///
    /// To mitigate the risk of lost updates, you should call the [Self::get_mutable_most_recent] method
    /// then start authoring the new [MutableItem] based on the most recent as in the following example:
    ///
    ///```rust
    /// use mainline::{Dht, MutableItem, SigningKey, Testnet};
    ///
    /// let testnet = Testnet::new(3).unwrap();
    /// let dht = Dht::builder().bootstrap(&testnet.bootstrap).build().unwrap();
    ///
    /// let signing_key = SigningKey::from_bytes(&[0; 32]);
    /// let key = signing_key.verifying_key().to_bytes();
    /// let salt = Some(b"salt".as_ref());
    ///
    /// let (item, cas) = if let Some(most_recent) = dht
    ///     .get_mutable_most_recent(&key, salt)
    ///     .unwrap()
    /// {
    ///     // 1. Optionally Create a new value to take the most recent's value in consideration.
    ///     let mut new_value = most_recent.value().to_vec();
    ///     new_value.extend_from_slice(b" more data");
    ///
    ///     // 2. Increment the sequence number to be higher than the most recent's.
    ///     let most_recent_seq = most_recent.seq();
    ///     let new_seq = most_recent_seq + 1;
    ///
    ///     (
    ///         MutableItem::new(signing_key, &new_value, new_seq, salt),
    ///         // 3. Use the most recent [MutableItem::seq] as a `CAS`.
    ///         Some(most_recent_seq)
    ///     )
    /// } else {
    ///     (MutableItem::new(signing_key, b"first value", 1, salt), None)
    /// };
    ///
    /// dht.put_mutable(item, cas).unwrap();
    /// ```
    ///
    /// ## Errors
    ///
    /// In addition to the [PutQueryError] common with all PUT queries, PUT mutable item
    /// query has other [Concurrency errors][ConcurrencyError], that try to detect write conflict
    /// risks or obvious conflicts.
    ///
    /// If you are lucky to get one of these errors (which is not guaranteed), then you should
    /// read the most recent item again, and repeat the steps in the previous example.
    pub fn put_mutable(&self, item: MutableItem, cas: Option<i64>) -> Result<Id, PutMutableError> {
        let (sender, receiver) = flume::bounded::<Result<Id, PutError>>(1);

        let target = *item.target();

        let request = PutRequestSpecific::PutMutable(PutMutableRequestArguments::from(item, cas));

        self.0
            .send(ActorMessage::Put(target, request, sender))
            .map_err(|_| DhtWasShutdown)?;

        receiver
            .recv()
            .expect("Query was dropped before sending a response, please open an issue.")
            .map_err(|error| match error {
                PutError::Query(err) => PutMutableError::Query(err),
                PutError::Concurrency(err) => PutMutableError::Concurrency(err),
            })
    }
}

pub struct GetIterator<T>(flume::IntoIter<T>);

impl<T> Iterator for GetIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

fn run(config: Config, receiver: Receiver<ActorMessage>) {
    match Rpc::new(config) {
        Ok(mut rpc) => {
            let address = rpc.local_addr();
            info!(?address, "Mainline DHT listening");

            let mut put_senders = HashMap::new();
            let mut get_senders = HashMap::new();

            loop {
                if let Ok(actor_message) = receiver.try_recv() {
                    match actor_message {
                        ActorMessage::Shutdown(sender) => {
                            drop(receiver);
                            let _ = sender.send(());
                            break;
                        }
                        ActorMessage::Check(sender) => {
                            let _ = sender.send(Ok(()));
                        }
                        ActorMessage::Info(sender) => {
                            let _ = sender.send(rpc.info());
                        }
                        ActorMessage::Put(target, request, sender) => {
                            if let Err(error) = rpc.put(request) {
                                let _ = sender.send(Err(error));
                            } else {
                                let senders = put_senders.entry(target).or_insert(vec![]);

                                senders.push(sender);
                            };
                        }
                        ActorMessage::Get(target, request, sender) => {
                            if let Some(responses) = rpc.get(request, None) {
                                for response in responses {
                                    send(&sender, response);
                                }
                            };

                            let senders = get_senders.entry(target).or_insert(vec![]);

                            senders.push(sender);
                        }
                        ActorMessage::ToBootstrap(sender) => {
                            let _ = sender.send(rpc.routing_table().to_bootstrap());
                        }
                    }
                }

                let report = rpc.tick();

                // Response for an ongoing GET query
                if let Some((target, response)) = report.query_response {
                    if let Some(senders) = get_senders.get(&target) {
                        for sender in senders {
                            send(sender, response.clone());
                        }
                    }
                }

                // Response for finished FIND_NODE query
                for (id, closest_nodes) in report.done_find_node_queries {
                    if let Some(senders) = get_senders.remove(&id) {
                        for sender in senders {
                            if let ResponseSender::ClosestNodes(sender) = sender {
                                let _ = sender.send(closest_nodes.clone());
                            }
                        }
                    }
                }

                // Cleanup done PUT query and send a resulting error if any.
                for (id, error) in report.done_put_queries {
                    if let Some(senders) = put_senders.remove(&id) {
                        let result = if let Some(error) = error {
                            Err(error)
                        } else {
                            Ok(id)
                        };

                        for sender in senders {
                            let _ = sender.send(result.clone());
                        }
                    }
                }

                // Cleanup done GET queries
                for id in report.done_get_queries {
                    get_senders.remove(&id);
                }
            }
        }
        Err(err) => {
            if let Ok(ActorMessage::Check(sender)) = receiver.try_recv() {
                let _ = sender.send(Err(err));
            }
        }
    };
}

fn send(sender: &ResponseSender, response: Response) {
    match (sender, response) {
        (ResponseSender::Peers(s), Response::Peers(r)) => {
            let _ = s.send(r);
        }
        (ResponseSender::Mutable(s), Response::Mutable(r)) => {
            let _ = s.send(r);
        }
        (ResponseSender::Immutable(s), Response::Immutable(r)) => {
            let _ = s.send(r);
        }
        _ => {}
    }
}

pub(crate) enum ActorMessage {
    Info(Sender<Info>),
    Put(Id, PutRequestSpecific, Sender<Result<Id, PutError>>),
    Get(Id, GetRequestSpecific, ResponseSender),
    Shutdown(Sender<()>),
    Check(Sender<Result<(), std::io::Error>>),
    ToBootstrap(Sender<Vec<String>>),
}

#[derive(Debug, Clone)]
pub enum ResponseSender {
    ClosestNodes(Sender<Box<[Node]>>),
    Peers(Sender<Vec<SocketAddrV4>>),
    Mutable(Sender<MutableItem>),
    Immutable(Sender<Box<[u8]>>),
}

/// Create a testnet of Dht nodes to run tests against instead of the real mainline network.
#[derive(Debug)]
pub struct Testnet {
    pub bootstrap: Vec<String>,
    pub nodes: Vec<Dht>,
}

impl Testnet {
    pub fn new(count: usize) -> Result<Testnet, std::io::Error> {
        let mut nodes: Vec<Dht> = vec![];
        let mut bootstrap = vec![];

        for i in 0..count {
            if i == 0 {
                let node = Dht::builder().server_mode().bootstrap(&[]).build()?;

                let info = node.info().expect("node should not be shutdown in Testnet");
                let addr = info.local_addr();

                bootstrap.push(format!("127.0.0.1:{}", addr.port()));

                nodes.push(node)
            } else {
                let node = Dht::builder().server_mode().bootstrap(&bootstrap).build()?;
                nodes.push(node)
            }
        }

        Ok(Self { bootstrap, nodes })
    }
}

#[derive(thiserror::Error, Debug)]
/// Announce Peer errors.
pub enum AnnouncePeerError {
    #[error(transparent)]
    Query(#[from] PutQueryError),

    #[error(transparent)]
    Shutdown(#[from] DhtWasShutdown),
}

#[derive(thiserror::Error, Debug)]
/// Put Immutable errors.
pub enum PutImmutableError {
    #[error(transparent)]
    Query(#[from] PutQueryError),

    #[error(transparent)]
    Shutdown(#[from] DhtWasShutdown),
}

#[derive(thiserror::Error, Debug)]
/// Put MutableItem errors.
pub enum PutMutableError {
    #[error(transparent)]
    Query(#[from] PutQueryError),

    #[error(transparent)]
    Concurrency(#[from] ConcurrencyError),

    #[error(transparent)]
    Shutdown(#[from] DhtWasShutdown),
}

#[derive(Debug)]
pub struct DhtWasShutdown;

impl std::error::Error for DhtWasShutdown {}

impl std::fmt::Display for DhtWasShutdown {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "The Dht was shutdown")
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use ed25519_dalek::SigningKey;

    use crate::rpc::ConcurrencyError;

    use super::*;

    #[test]
    fn shutdown() {
        let mut dht = Dht::client().unwrap();

        let a = dht.clone();

        dht.shutdown();

        let result = a.get_immutable(Id::random());

        assert!(matches!(result, Err(DhtWasShutdown)))
    }

    #[test]
    fn bind_twice() {
        let a = Dht::client().unwrap();
        let result = Dht::builder()
            .port(a.info().unwrap().local_addr().port())
            .server_mode()
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn announce_get_peer() {
        let testnet = Testnet::new(10).unwrap();

        let a = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();
        let b = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let info_hash = Id::random();

        a.announce_peer(info_hash, Some(45555))
            .expect("failed to announce");

        let peers = b.get_peers(info_hash).unwrap().next().expect("No peers");

        assert_eq!(peers.first().unwrap().port(), 45555);
    }

    #[test]
    fn put_get_immutable() {
        let testnet = Testnet::new(10).unwrap();

        let a = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();
        let b = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let value = b"Hello World!";
        let expected_target = Id::from_str("e5f96f6f38320f0f33959cb4d3d656452117aadb").unwrap();

        let target = a.put_immutable(value).unwrap();
        assert_eq!(target, expected_target);

        let response = b.get_immutable(target).unwrap().unwrap();

        assert_eq!(response, value.to_vec().into_boxed_slice());
    }

    #[test]
    fn find_node_no_values() {
        let client = Dht::builder().bootstrap(&[]).build().unwrap();

        client.find_node(Id::random()).unwrap();
    }

    #[test]
    fn put_get_immutable_no_values() {
        let client = Dht::builder().bootstrap(&[]).build().unwrap();

        assert_eq!(client.get_immutable(Id::random()).unwrap(), None);
    }

    #[test]
    fn put_get_mutable() {
        let testnet = Testnet::new(10).unwrap();

        let a = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();
        let b = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let signer = SigningKey::from_bytes(&[
            56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
            228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
        ]);

        let seq = 1000;
        let value = b"Hello World!";

        let item = MutableItem::new(signer.clone(), value, seq, None);

        a.put_mutable(item.clone(), None).unwrap();

        let response = b
            .get_mutable(signer.verifying_key().as_bytes(), None, None)
            .unwrap()
            .next()
            .expect("No mutable values");

        assert_eq!(&response, &item);
    }

    #[test]
    fn put_get_mutable_no_more_recent_value() {
        let testnet = Testnet::new(10).unwrap();

        let a = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();
        let b = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let signer = SigningKey::from_bytes(&[
            56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
            228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
        ]);

        let seq = 1000;
        let value = b"Hello World!";

        let item = MutableItem::new(signer.clone(), value, seq, None);

        a.put_mutable(item.clone(), None).unwrap();

        let response = b
            .get_mutable(signer.verifying_key().as_bytes(), None, Some(seq))
            .unwrap()
            .next();

        assert!(&response.is_none());
    }

    #[test]
    fn repeated_put_query() {
        let testnet = Testnet::new(10).unwrap();

        let a = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let id = a.put_immutable(&[1, 2, 3]).unwrap();

        assert_eq!(a.put_immutable(&[1, 2, 3]).unwrap(), id);
    }

    #[test]
    fn concurrent_get_mutable() {
        let testnet = Testnet::new(10).unwrap();

        let a = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();
        let b = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let signer = SigningKey::from_bytes(&[
            56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
            228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
        ]);

        let key = signer.verifying_key().to_bytes();
        let seq = 1000;
        let value = b"Hello World!";

        let item = MutableItem::new(signer.clone(), value, seq, None);

        a.put_mutable(item.clone(), None).unwrap();

        let _response_first = b
            .get_mutable(&key, None, None)
            .unwrap()
            .next()
            .expect("No mutable values");

        let response_second = b
            .get_mutable(&key, None, None)
            .unwrap()
            .next()
            .expect("No mutable values");

        assert_eq!(&response_second, &item);
    }

    #[test]
    fn concurrent_put_mutable_same() {
        let testnet = Testnet::new(10).unwrap();

        let client = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let signer = SigningKey::from_bytes(&[
            56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
            228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
        ]);

        let seq = 1000;
        let value = b"Hello World!";

        let item = MutableItem::new(signer.clone(), value, seq, None);

        let mut handles = vec![];

        for _ in 0..2 {
            let client = client.clone();
            let item = item.clone();

            let handle = std::thread::spawn(move || client.put_mutable(item, None).unwrap());

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn concurrent_put_mutable_different() {
        let testnet = Testnet::new(10).unwrap();

        let client = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let mut handles = vec![];

        for i in 0..2 {
            let client = client.clone();

            let signer = SigningKey::from_bytes(&[
                56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
                228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
            ]);

            let seq = 1000;

            let mut value = b"Hello World!".to_vec();
            value.push(i);

            let item = MutableItem::new(signer.clone(), &value, seq, None);

            let handle = std::thread::spawn(move || {
                let result = client.put_mutable(item, None);
                if i == 0 {
                    assert!(matches!(result, Ok(_)))
                } else {
                    assert!(matches!(
                        result,
                        Err(PutMutableError::Concurrency(ConcurrencyError::ConflictRisk))
                    ))
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn concurrent_put_mutable_different_with_cas() {
        let testnet = Testnet::new(10).unwrap();

        let client = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let signer = SigningKey::from_bytes(&[
            56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
            228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
        ]);

        // First
        {
            let item = MutableItem::new(signer.clone(), &[], 1000, None);

            let (sender, _) = flume::bounded::<Result<Id, PutError>>(1);
            let target = *item.target();
            let request =
                PutRequestSpecific::PutMutable(PutMutableRequestArguments::from(item, None));
            client
                .0
                .send(ActorMessage::Put(target, request, sender))
                .map_err(|_| DhtWasShutdown)
                .unwrap();
        }

        std::thread::sleep(Duration::from_millis(100));

        // Second
        {
            let item = MutableItem::new(signer, &[], 1001, None);

            let most_recent = client.get_mutable_most_recent(item.key(), None).unwrap();

            if let Some(cas) = most_recent.map(|item| item.seq()) {
                client.put_mutable(item, Some(cas)).unwrap();
            } else {
                client.put_mutable(item, None).unwrap();
            }
        }
    }

    #[test]
    fn conflict_302_seq_less_than_current() {
        let testnet = Testnet::new(10).unwrap();

        let client = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let signer = SigningKey::from_bytes(&[
            56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
            228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
        ]);

        client
            .put_mutable(MutableItem::new(signer.clone(), &[], 1001, None), None)
            .unwrap();

        assert!(matches!(
            client.put_mutable(MutableItem::new(signer, &[], 1000, None), None),
            Err(PutMutableError::Concurrency(
                ConcurrencyError::NotMostRecent
            ))
        ));
    }

    #[test]
    fn conflict_301_cas() {
        let testnet = Testnet::new(10).unwrap();

        let client = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap();

        let signer = SigningKey::from_bytes(&[
            56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
            228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
        ]);

        client
            .put_mutable(MutableItem::new(signer.clone(), &[], 1001, None), None)
            .unwrap();

        assert!(matches!(
            client.put_mutable(MutableItem::new(signer, &[], 1002, None), Some(1000)),
            Err(PutMutableError::Concurrency(ConcurrencyError::CasFailed))
        ));
    }
}

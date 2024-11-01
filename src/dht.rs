//! Dht node.

use std::{fmt::Formatter, net::SocketAddr, thread, time::Duration};

use bytes::Bytes;
use flume::{Receiver, Sender};

use tracing::info;

use crate::{
    common::{
        hash_immutable, AnnouncePeerRequestArguments, FindNodeRequestArguments,
        GetPeersRequestArguments, GetValueRequestArguments, Id, MutableItem,
        PutImmutableRequestArguments, PutMutableRequestArguments, PutRequestSpecific,
        RequestTypeSpecific,
    },
    rpc::{PutError, ReceivedFrom, ReceivedMessage, ResponseSender, Rpc},
    server::{DefaultServer, Server},
    Node,
};

#[derive(Debug, Clone)]
/// Mainline Dht node.
pub struct Dht(pub(crate) Sender<ActorMessage>);

#[derive(Debug, Default)]
/// Dht settings
pub struct Settings {
    /// Defaults to [crate::rpc::DEFAULT_BOOTSTRAP_NODES]
    pub(crate) bootstrap: Option<Vec<String>>,
    /// Defaults to None
    pub(crate) server: Option<Box<dyn Server>>,
    /// Defaults to [crate::rpc::DEFAULT_PORT]
    pub(crate) port: Option<u16>,
    /// Defaults to [crate::rpc::DEFAULT_REQUEST_TIMEOUT]
    pub(crate) request_timeout: Option<Duration>,
}

impl Settings {
    /// Create a Dht node.
    pub fn build(self) -> Result<Dht, std::io::Error> {
        Dht::new(self)
    }

    /// Build an [Rpc] instance that you intend to manage in an Actor thread yourself.
    pub fn build_rpc(self) -> Result<Rpc, std::io::Error> {
        Rpc::new(&self)
    }

    /// Create a full DHT node that accepts requests, and acts as a routing and storage node.
    pub fn server(mut self) -> Self {
        self.server = Some(Box::<DefaultServer>::default());
        self
    }

    pub fn custom_server(mut self, custom_server: Box<dyn Server>) -> Self {
        self.server = Some(custom_server);
        self
    }

    /// Set bootstrapping nodes
    pub fn bootstrap(mut self, bootstrap: &[String]) -> Self {
        self.bootstrap = Some(bootstrap.to_vec());
        self
    }

    /// Set the port to listen on.
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the the duration a request awaits for a response.
    ///
    /// The longer this duration is, the longer queries take until they are deemeed "done".
    /// The shortet this duration is, the more responses from busy nodes we miss out on,
    /// which affects the accuracy of queries trying to find closest nodes to a target.
    ///
    /// Defaults to 2 seconds.
    pub fn request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = Some(request_timeout);
        self
    }
}

impl Dht {
    /// Returns a builder to edit settings before creating a Dht node.
    pub fn builder() -> Settings {
        Settings::default()
    }

    /// Create a new DHT client with default bootstrap nodes.
    pub fn client() -> Result<Self, std::io::Error> {
        Dht::builder().build()
    }

    /// Create a new DHT server that serves as a routing node and accepts storage requests
    /// for peers and other arbitrary data.
    ///
    /// Note: this is only useful if the node has a public IP address and is able to receive
    /// incoming udp packets.
    pub fn server() -> Result<Self, std::io::Error> {
        Dht::builder().server().build()
    }

    /// Create a new Dht node.
    ///
    /// Could return an error if it failed to bind to the specified
    /// port or other io errors while binding the udp socket.
    pub(crate) fn new(settings: Settings) -> Result<Self, std::io::Error> {
        let (sender, receiver) = flume::bounded(32);

        let rpc = Rpc::new(&settings)?;

        let address = rpc.local_addr()?;

        info!(?address, "Mainline DHT listening");

        let mut server = settings.server;

        thread::spawn(move || run(rpc, &mut server, receiver));

        Ok(Dht(sender))
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

    // === Public Methods ===

    /// Shutdown the actor thread loop.
    pub fn shutdown(&mut self) {
        let (sender, receiver) = flume::bounded::<()>(1);

        let _ = self.0.send(ActorMessage::Shutdown(sender));
        let _ = receiver.recv();
    }

    // === Find nodes ===

    pub fn find_node(&self, target: Id) -> Result<Vec<Node>, DhtWasShutdown> {
        let (sender, receiver) = flume::bounded::<Vec<Node>>(1);

        let request = RequestTypeSpecific::FindNode(FindNodeRequestArguments { target });

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
    ) -> Result<flume::IntoIter<Vec<SocketAddr>>, DhtWasShutdown> {
        // Get requests use unbounded channels to avoid blocking in the run loop.
        // Other requests like put_* and getters don't need that and is ok with
        // bounded channel with 1 capacity since it only ever sends one message back.
        //
        // So, if it is a ResponseMessage<_>, it should be unbounded, otherwise bounded.
        let (sender, receiver) = flume::unbounded::<Vec<SocketAddr>>();

        let request = RequestTypeSpecific::GetPeers(GetPeersRequestArguments { info_hash });

        self.0
            .send(ActorMessage::Get(
                info_hash,
                request,
                ResponseSender::Peers(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver.into_iter())
    }

    /// Announce a peer for a given infohash.
    ///
    /// The peer will be announced on this process IP.
    /// If explicit port is passed, it will be used, otherwise the port will be implicitly
    /// assumed by remote nodes to be the same ase port they recieved the request from.
    pub fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<Id, DhtPutError> {
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
            .expect("Query was dropped before sending a response, please open an issue.")?)
    }

    // === Immutable data ===

    /// Get an Immutable data by its sha1 hash.
    pub fn get_immutable(&self, target: Id) -> Result<Option<Bytes>, DhtWasShutdown> {
        let (sender, receiver) = flume::unbounded::<Bytes>();

        let request = RequestTypeSpecific::GetValue(GetValueRequestArguments {
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
    pub fn put_immutable(&self, value: Bytes) -> Result<Id, DhtPutError> {
        let target: Id = hash_immutable(&value).into();

        let (sender, receiver) = flume::bounded::<Result<Id, PutError>>(1);

        let request = PutRequestSpecific::PutImmutable(PutImmutableRequestArguments {
            target,
            v: value.clone().into(),
        });

        self.0
            .send(ActorMessage::Put(target, request, sender))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver
            .recv()
            .expect("Query was dropped before sending a response, please open an issue.")?)
    }

    // === Mutable data ===

    /// Get a mutable data by its public_key and optional salt.
    pub fn get_mutable(
        &self,
        public_key: &[u8; 32],
        salt: Option<Bytes>,
        seq: Option<i64>,
    ) -> Result<flume::IntoIter<MutableItem>, DhtWasShutdown> {
        let target = MutableItem::target_from_key(public_key, &salt);

        let (sender, receiver) = flume::unbounded::<MutableItem>();

        let request = RequestTypeSpecific::GetValue(GetValueRequestArguments { target, seq, salt });

        self.0
            .send(ActorMessage::Get(
                target,
                request,
                ResponseSender::Mutable(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver.into_iter())
    }

    /// Put a mutable data to the DHT.
    pub fn put_mutable(&self, item: MutableItem) -> Result<Id, DhtPutError> {
        let (sender, receiver) = flume::bounded::<Result<Id, PutError>>(1);

        let request = PutRequestSpecific::PutMutable(PutMutableRequestArguments {
            target: *item.target(),
            v: item.value().clone().into(),
            k: item.key().to_vec(),
            seq: *item.seq(),
            sig: item.signature().to_vec(),
            salt: item.salt().clone().map(|s| s.to_vec()),
            cas: *item.cas(),
        });

        self.0
            .send(ActorMessage::Put(*item.target(), request, sender))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver
            .recv()
            .expect("Query was dropped before sending a response, please open an issue.")?)
    }
}

fn run(mut rpc: Rpc, server: &mut Option<Box<dyn Server>>, receiver: Receiver<ActorMessage>) {
    loop {
        if let Ok(actor_message) = receiver.try_recv() {
            match actor_message {
                ActorMessage::Shutdown(sender) => {
                    drop(receiver);
                    let _ = sender.send(());
                    break;
                }
                ActorMessage::Info(sender) => {
                    let local_address = rpc.local_addr();

                    let _ = sender.send(Info {
                        id: *rpc.id(),
                        local_address,
                        dht_size_estimate: rpc.dht_size_estimate(),
                    });
                }
                ActorMessage::Put(target, request, sender) => {
                    rpc.put(target, request, Some(sender));
                }
                ActorMessage::Get(target, request, sender) => {
                    rpc.get(target, request, Some(sender), None)
                }
            }
        }

        let report = rpc.tick();

        // Handle incoming request with the default Server logic.
        if let Some(ReceivedFrom {
            from,
            message: ReceivedMessage::Request((transaction_id, request_specific)),
        }) = report.received_from
        {
            if let Some(server) = server.as_mut() {
                server.handle_request(&mut rpc, from, transaction_id, &request_specific);
            }
        };
    }
}

pub enum ActorMessage {
    Info(Sender<Info>),
    Put(Id, PutRequestSpecific, Sender<Result<Id, PutError>>),
    Get(Id, RequestTypeSpecific, ResponseSender),
    Shutdown(Sender<()>),
}

/// Information and statistics about this [Dht] node.
pub struct Info {
    id: Id,
    local_address: Result<SocketAddr, std::io::Error>,
    dht_size_estimate: (usize, f64),
}

impl Info {
    /// This Node's [Id]
    pub fn id(&self) -> &Id {
        &self.id
    }
    /// Local UDP Ipv4 socket address that this node is listening on.
    pub fn local_addr(&self) -> Result<&SocketAddr, &std::io::Error> {
        self.local_address.as_ref()
    }
    /// Dht size estimate
    pub fn dht_size_estimate(&self) -> usize {
        self.dht_size_estimate.0
    }
    /// The standard deviation (fraction) from the the [Self::dht_size_estimate]
    pub fn dht_size_estimate_standard_deviation(&self) -> f64 {
        self.dht_size_estimate.1
    }
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
                let node = Dht::builder().server().bootstrap(&[]).build()?;

                let addr = node
                    .info()
                    .expect("node should not be shutdown in Testnet")
                    .local_address
                    .expect("node should not be shutdown in Testnet");
                bootstrap.push(format!("127.0.0.1:{}", addr.port()));

                nodes.push(node)
            } else {
                let node = Dht::builder().server().bootstrap(&bootstrap).build()?;
                nodes.push(node)
            }
        }

        Ok(Self { bootstrap, nodes })
    }
}

#[derive(thiserror::Error, Debug)]
/// Dht Actor errors
pub enum DhtPutError {
    #[error(transparent)]
    PutError(#[from] PutError),

    #[error(transparent)]
    DhtWasShutdown(#[from] DhtWasShutdown),
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

    use super::*;

    #[test]
    fn shutdown() {
        let mut dht = Dht::client().unwrap();

        dht.info().unwrap().local_address.unwrap();

        let a = dht.clone();

        dht.shutdown();

        let result = a.get_immutable(Id::random());

        assert!(matches!(result, Err(DhtWasShutdown)))
    }

    #[test]
    fn bind_twice() {
        let a = Dht::client().unwrap();
        let result = Dht::builder()
            .port(a.info().unwrap().local_address.unwrap().port())
            .server()
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

        let value: Bytes = "Hello World!".into();
        let expected_target = Id::from_str("e5f96f6f38320f0f33959cb4d3d656452117aadb").unwrap();

        let target = a.put_immutable(value.clone()).unwrap();
        assert_eq!(target, expected_target);

        let response = b.get_immutable(target).unwrap().unwrap();

        assert_eq!(response, value);
    }

    #[test]
    fn find_node_no_values() {
        let client = Dht::builder().bootstrap(&vec![]).build().unwrap();

        client.find_node(Id::random()).unwrap();
    }

    #[test]
    fn put_get_immutable_no_values() {
        let client = Dht::builder().bootstrap(&vec![]).build().unwrap();

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
        let value: Bytes = "Hello World!".into();

        let item = MutableItem::new(signer.clone(), value, seq, None);

        a.put_mutable(item.clone()).unwrap();

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
        let value: Bytes = "Hello World!".into();

        let item = MutableItem::new(signer.clone(), value, seq, None);

        a.put_mutable(item.clone()).unwrap();

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

        let id = a.put_immutable(vec![1, 2, 3].into()).unwrap();

        assert_eq!(a.put_immutable(vec![1, 2, 3].into()).unwrap(), id);
    }
}

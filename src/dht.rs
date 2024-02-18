//! Dht node.

use std::{
    net::SocketAddr,
    thread::{self, JoinHandle},
};

use bytes::Bytes;
use flume::{Receiver, Sender};

use crate::{
    common::{hash_immutable, target_from_key, Id, MutableItem, Node, RoutingTable},
    rpc::{
        GetImmutableResponse, GetMutableResponse, GetPeerResponse, Response, ResponseMessage,
        ResponseSender, Rpc, StoreQueryMetdata,
    },
    Result,
};

#[derive(Debug)]
pub struct Dht {
    handle: Option<JoinHandle<()>>,
    pub(crate) sender: Sender<ActorMessage>,
}

impl Clone for Dht {
    /// Cloning a Dht node returns an actor that can be used to send
    /// commands to the main Dht actor that contains the active thread.
    ///
    /// This is useful for moving an actor to another thread.
    ///
    /// # Example
    ///
    /// ```
    /// use mainline::{Dht, Id, Testnet};
    /// use std::thread;
    ///
    /// // Create a testnet for this example to avoid spamming the mainnet.
    /// let testnet = Testnet::new(10);
    ///
    /// let dht = Dht::builder().bootstrap(&testnet.bootstrap).build();
    ///
    /// let actor = dht.clone();
    /// thread::spawn(move || {
    ///     let info_hash: Id = [0;20].into();
    ///
    ///     actor.announce_peer(info_hash, Some(9090));
    /// });
    /// ```
    fn clone(&self) -> Self {
        Dht {
            handle: None,
            sender: self.sender.clone(),
        }
    }
}

pub struct Builder {
    settings: DhtSettings,
}

impl Builder {
    pub fn build(&self) -> Dht {
        Dht::new(self.settings.clone())
    }

    /// Create a full DHT node that accepts requests, and acts as a routing and storage node.
    pub fn as_server(mut self) -> Self {
        self.settings.read_only = false;
        self
    }

    /// Set bootstrapping nodes
    pub fn bootstrap(mut self, bootstrap: &[String]) -> Self {
        self.settings.bootstrap = Some(bootstrap.to_owned());
        self
    }

    /// Set the port to listen on.
    pub fn port(mut self, port: u16) -> Self {
        self.settings.port = Some(port);
        self
    }
}

#[derive(Debug, Clone)]
pub struct DhtSettings {
    pub bootstrap: Option<Vec<String>>,
    pub read_only: bool,
    pub port: Option<u16>,
}

impl Default for DhtSettings {
    fn default() -> Self {
        DhtSettings {
            bootstrap: None,
            read_only: true,
            port: None,
        }
    }
}

impl Dht {
    pub fn builder() -> Builder {
        Builder {
            settings: DhtSettings::default(),
        }
    }

    /// Create a new DHT client with default bootstrap nodes.
    pub fn client() -> Self {
        Dht::default()
    }

    /// Create a new DHT server that serves as a routing node and accepts storage requests
    /// for peers and other arbitrary data.
    ///
    /// Note: this is only useful if the node has a public IP address and is able to receive
    /// incoming udp packets.
    pub fn server() -> Self {
        Dht::builder().as_server().build()
    }

    pub fn new(settings: DhtSettings) -> Self {
        let (sender, receiver) = flume::bounded(32);

        let mut dht = Dht {
            sender,
            handle: None,
        };

        let mut clone = dht.clone();

        let handle = thread::spawn(move || dht.run(settings, receiver));

        clone.handle = Some(handle);

        clone
    }

    // === Getters ===

    /// Returns the local address of the udp socket this node is listening on.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        let (sender, receiver) = flume::bounded::<SocketAddr>(1);

        let _ = self.sender.send(ActorMessage::LocalAddress(sender));

        receiver.recv().map_err(|e| e.into())
    }

    /// Returns a clone of the routing table of this node.
    pub fn routing_table(&self) -> Result<RoutingTable> {
        let (sender, receiver) = flume::bounded::<RoutingTable>(1);

        let _ = self.sender.send(ActorMessage::RoutingTable(sender));

        receiver.recv().map_err(|e| e.into())
    }

    // === Public Methods ===

    pub fn shutdown(&self) {
        let _ = self.sender.send(ActorMessage::Shutdown).ok();
    }

    // === Peers ===

    /// Get peers for a given infohash.
    ///
    /// Returns a blocking iterator over responses as they are received.
    ///
    /// Note: each node of the network will only return a _random_ subset (usually 20)
    /// of the total peers it has for a given infohash, so if you are getting responses
    /// from 20 nodes, you can expect up to 400 peers in total, but if there are more
    /// announced peers on that infohash, you are likely to miss some, the logic here
    /// for Bittorrent is that any peer will introduce you to more peers through "peer exchange"
    /// so if you are implementing something different from Bittorrent, you might want
    /// to implement your own logic for gossipping more peers after you discover the first ones.
    ///
    /// # Eaxmples
    ///
    /// ```
    /// use mainline::{Dht, Id, Testnet};
    ///
    /// // Create a testnet for this example to avoid spamming the mainnet.
    /// let testnet = Testnet::new(10);
    ///
    /// let dht = Dht::builder().bootstrap(&testnet.bootstrap).build();
    ///
    /// let info_hash: Id = [0; 20].into();
    ///
    /// let mut response = dht.get_peers(info_hash);
    /// for res in &mut response {
    ///     println!("Got peer: {:?} | from: {:?}", res.peer, res.from)
    /// }
    ///
    /// println!(
    ///     "Visited {:?} nodes, found {:?} closest nodes",
    ///     response.visited,
    ///     &response.closest_nodes.len()
    /// );
    /// ```
    pub fn get_peers(&self, info_hash: Id) -> Response<GetPeerResponse> {
        // Get requests use unbounded channels to avoid blocking in the run loop.
        // Other requests like put_to and getters don't need that and is ok with
        // bounded channel with 1 capacity since it only ever sends one message back.
        //
        // So, if it is a ResponseMessage<_>, it should be unbounded, otherwise bounded.
        let (sender, receiver) = flume::unbounded::<ResponseMessage<GetPeerResponse>>();

        let _ = self.sender.send(ActorMessage::GetPeers(info_hash, sender));

        Response::new(receiver)
    }

    /// Announce a peer for a given infohash.
    ///
    /// The peer will be announced on this process IP.
    /// If explicit port is passed, it will be used, otherwise the port will be implicitly
    /// assumed by remote nodes to be the same ase port they recieved the request from.
    pub fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::unbounded::<ResponseMessage<GetPeerResponse>>();

        let _ = self.sender.send(ActorMessage::GetPeers(info_hash, sender));

        let mut response = Response::new(receiver);

        // Block until we got a Done response!
        for _ in &mut response {}

        self.announce_peer_to(info_hash, response.closest_nodes, port)
    }

    /// Announce a peer for a given infhoash to a specific set of nodes.
    ///
    /// Useful if you already have the list of closest_nodes from previous queries.
    /// Note that tokens are only valid within a window of 5-10 minutes, so if your
    /// list of nodes are older than 5 minutes, you should use [announce_peer](Dht::announce_peer) instead.
    pub fn announce_peer_to(
        &self,
        info_hash: Id,
        nodes: Vec<Node>,
        port: Option<u16>,
    ) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<StoreQueryMetdata>(1);

        let _ = self
            .sender
            .send(ActorMessage::AnnouncePeer(info_hash, nodes, port, sender));

        receiver.recv().map_err(|e| e.into())
    }

    // === Immutable data ===

    /// Get an Immutable data by its sha1 hash.
    pub fn get_immutable(&self, target: Id) -> Response<GetImmutableResponse> {
        let (sender, receiver) = flume::unbounded::<ResponseMessage<GetImmutableResponse>>();

        let _ = self.sender.send(ActorMessage::GetImmutable(target, sender));

        Response::new(receiver)
    }

    /// Put an immutable data to the DHT.
    pub fn put_immutable(&self, value: Bytes) -> Result<StoreQueryMetdata> {
        let target = Id::from_bytes(hash_immutable(&value)).unwrap();

        let (sender, receiver) = flume::unbounded::<ResponseMessage<GetImmutableResponse>>();

        let _ = self.sender.send(ActorMessage::GetImmutable(target, sender));

        let mut response = Response::new(receiver);

        // Block until we got a Done response!
        for _ in &mut response {}

        self.put_immutable_to(target, value, response.closest_nodes)
    }

    /// Put an immutable data to specific nodes.
    pub fn put_immutable_to(
        &self,
        target: Id,
        value: Bytes,
        nodes: Vec<Node>,
    ) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<StoreQueryMetdata>(1);

        let _ = self
            .sender
            .send(ActorMessage::PutImmutable(target, value, nodes, sender));

        receiver.recv().map_err(|e| e.into())
    }

    // === Mutable data ===

    /// Get a mutable data by its public_key and optional salt.
    pub fn get_mutable(
        &self,
        public_key: &[u8; 32],
        salt: Option<Bytes>,
    ) -> Response<GetMutableResponse> {
        let target = target_from_key(public_key, &salt);

        let (sender, receiver) = flume::unbounded::<ResponseMessage<GetMutableResponse>>();

        let _ = self
            .sender
            .send(ActorMessage::GetMutable(target, salt, sender));

        Response::new(receiver)
    }

    /// Put an immutable data to the DHT.
    pub fn put_mutable(&self, item: MutableItem) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::unbounded::<ResponseMessage<GetMutableResponse>>();

        let _ = self.sender.send(ActorMessage::GetMutable(
            *item.target(),
            item.salt().clone(),
            sender,
        ));

        let mut response = Response::new(receiver);

        // Block until we got a Done response!
        for _ in &mut response {}

        self.put_mutable_to(item, response.closest_nodes)
    }

    /// Put an immutable data to specific nodes.
    pub fn put_mutable_to(&self, item: MutableItem, nodes: Vec<Node>) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<StoreQueryMetdata>(1);

        let _ = self
            .sender
            .send(ActorMessage::PutMutable(item, nodes, sender));

        receiver.recv().map_err(|e| e.into())
    }

    // === Private Methods ===

    #[cfg(test)]
    fn block_until_shutdown(self) {
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
    }

    fn run(&mut self, settings: DhtSettings, receiver: Receiver<ActorMessage>) {
        let mut rpc = Rpc::new().unwrap().with_read_only(settings.read_only);

        if let Some(bootstrap) = settings.bootstrap {
            rpc = rpc.with_bootstrap(bootstrap);
        }

        if let Some(port) = settings.port {
            rpc = rpc.with_port(port).unwrap();
        }

        loop {
            if let Ok(actor_message) = receiver.try_recv() {
                match actor_message {
                    ActorMessage::Shutdown => {
                        break;
                    }
                    ActorMessage::LocalAddress(sender) => {
                        let _ = sender.send(rpc.local_addr());
                    }
                    ActorMessage::RoutingTable(sender) => {
                        let _ = sender.send(rpc.routing_table());
                    }
                    ActorMessage::GetPeers(info_hash, sender) => {
                        rpc.get_peers(info_hash, ResponseSender::GetPeer(sender))
                    }
                    ActorMessage::AnnouncePeer(info_hash, nodes, port, sender) => {
                        rpc.announce_peer(info_hash, nodes, port, ResponseSender::StoreItem(sender))
                    }
                    ActorMessage::GetImmutable(target, sender) => {
                        rpc.get_immutable(target, ResponseSender::GetImmutable(sender))
                    }
                    ActorMessage::PutImmutable(target, value, nodes, sender) => {
                        rpc.put_immutable(target, value, nodes, ResponseSender::StoreItem(sender))
                    }
                    ActorMessage::GetMutable(target, salt, sender) => {
                        rpc.get_mutable(target, salt, ResponseSender::GetMutable(sender))
                    }
                    ActorMessage::PutMutable(item, nodes, sender) => {
                        rpc.put_mutable(item, nodes, ResponseSender::StoreItem(sender))
                    }
                }
            }

            rpc.tick();
        }
    }
}

impl Default for Dht {
    /// Create a new DHT client with default bootstrap nodes.
    fn default() -> Self {
        Dht::builder().build()
    }
}

pub(crate) enum ActorMessage {
    Shutdown,
    LocalAddress(Sender<SocketAddr>),
    RoutingTable(Sender<RoutingTable>),

    GetPeers(Id, Sender<ResponseMessage<GetPeerResponse>>),
    AnnouncePeer(Id, Vec<Node>, Option<u16>, Sender<StoreQueryMetdata>),

    GetImmutable(Id, Sender<ResponseMessage<GetImmutableResponse>>),
    PutImmutable(Id, Bytes, Vec<Node>, Sender<StoreQueryMetdata>),

    GetMutable(
        Id,
        Option<Bytes>,
        Sender<ResponseMessage<GetMutableResponse>>,
    ),
    PutMutable(MutableItem, Vec<Node>, Sender<StoreQueryMetdata>),
}

/// Create a testnet of Dht nodes to run tests against instead of the real mainline network.
#[derive(Debug)]
pub struct Testnet {
    pub bootstrap: Vec<String>,
    pub nodes: Vec<Dht>,
}

impl Testnet {
    pub fn new(count: usize) -> Self {
        let mut nodes: Vec<Dht> = vec![];
        let mut bootstrap = vec![];

        for i in 0..count {
            if i == 0 {
                let node = Dht::builder().as_server().bootstrap(&[]).build();

                let addr = node.local_addr().unwrap();
                bootstrap.push(format!("127.0.0.1:{}", addr.port()));

                nodes.push(node)
            } else {
                let node = Dht::builder().as_server().bootstrap(&bootstrap).build();
                nodes.push(node)
            }
        }

        Self { bootstrap, nodes }
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;
    use std::time::Duration;

    use ed25519_dalek::SigningKey;

    use super::*;

    #[test]
    fn shutdown() {
        let dht = Dht::default();

        let clone = dht.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));

            clone.shutdown();
        });

        dht.block_until_shutdown();
    }

    #[test]
    fn bind_twice() {
        let a = Dht::default();
        let b = Dht::builder()
            .port(a.local_addr().unwrap().port())
            .as_server()
            .build();

        let result = b.handle.unwrap().join();
        assert!(result.is_err());
    }

    #[test]
    fn announce_get_peer() {
        let testnet = Testnet::new(10);

        let a = Dht::builder().bootstrap(&testnet.bootstrap).build();
        let b = Dht::builder().bootstrap(&testnet.bootstrap).build();

        let info_hash = Id::random();

        match a.announce_peer(info_hash, Some(45555)) {
            Ok(_) => {
                let responses: Vec<_> = b.get_peers(info_hash).collect();

                match responses.first() {
                    Some(r) => {
                        assert_eq!(r.peer.port(), 45555);
                    }
                    None => {
                        panic!("No respnoses")
                    }
                }
            }
            Err(_) => {}
        };
    }

    #[test]
    fn put_get_immutable() {
        let testnet = Testnet::new(10);

        let a = Dht::builder().bootstrap(&testnet.bootstrap).build();
        let b = Dht::builder().bootstrap(&testnet.bootstrap).build();

        let value: Bytes = "Hello World!".into();
        let expected_target = Id::from_str("e5f96f6f38320f0f33959cb4d3d656452117aadb").unwrap();

        match a.put_immutable(value.clone()) {
            Ok(result) => {
                assert_ne!(result.stored_at().len(), 0);
                assert_eq!(result.target(), expected_target);

                let responses: Vec<_> = b.get_immutable(result.target()).collect();

                match responses.first() {
                    Some(r) => {
                        assert_eq!(r.value, value);
                    }
                    None => {
                        panic!("No respnoses")
                    }
                }
            }
            Err(_) => {
                panic!("Expected put_immutable to succeeed")
            }
        };
    }

    #[test]
    fn put_get_mutable() {
        let testnet = Testnet::new(10);

        let a = Dht::builder().bootstrap(&testnet.bootstrap).build();
        let b = Dht::builder().bootstrap(&testnet.bootstrap).build();

        let signer = SigningKey::from_bytes(&[
            56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
            228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
        ]);

        let seq = 1000;
        let value: Bytes = "Hello World!".into();

        let item = MutableItem::new(signer.clone(), value, seq, None);

        match a.put_mutable(item.clone()) {
            Ok(result) => {
                assert_ne!(result.stored_at().len(), 0);

                let responses: Vec<_> = b
                    .get_mutable(signer.verifying_key().as_bytes(), None)
                    .collect();

                match responses.first() {
                    Some(r) => {
                        assert_eq!(&r.item, &item);
                    }
                    None => {
                        panic!("No respnoses")
                    }
                }
            }
            Err(_) => {
                panic!("Expected put_immutable to succeeed")
            }
        };
    }
}

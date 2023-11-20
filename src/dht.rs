//! Dht node.

use std::{
    net::SocketAddr,
    thread::{self, JoinHandle},
};

use flume::{Receiver, Sender};

use crate::{
    common::{
        GetImmutableResponse, GetPeerResponse, Id, Node, Response, ResponseMessage, ResponseSender,
        StoreQueryMetdata,
    },
    routing_table::RoutingTable,
    rpc::Rpc,
    Result,
};

#[derive(Debug)]
pub struct Dht {
    handle: Option<JoinHandle<()>>,
    sender: Sender<ActorMessage>,
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

    #[cfg(feature = "async")]
    pub async fn local_addr_async(&self) -> Result<SocketAddr> {
        let (sender, receiver) = flume::bounded::<SocketAddr>(1);

        let _ = self.sender.send(ActorMessage::LocalAddress(sender));

        receiver.recv_async().await.map_err(|e| e.into())
    }

    /// Returns a clone of the routing table of this node.
    pub fn routing_table(&self) -> Result<RoutingTable> {
        let (sender, receiver) = flume::bounded::<RoutingTable>(1);

        let _ = self.sender.send(ActorMessage::RoutingTable(sender));

        receiver.recv().map_err(|e| e.into())
    }

    #[cfg(feature = "async")]
    pub async fn routing_table_async(&self) -> Result<RoutingTable> {
        let (sender, receiver) = flume::bounded::<RoutingTable>(1);

        let _ = self.sender.send(ActorMessage::RoutingTable(sender));

        receiver.recv_async().await.map_err(|e| e.into())
    }

    // === Public Methods ===

    pub fn shutdown(&self) {
        let _ = self.sender.send(ActorMessage::Shutdown).ok();
    }

    /// Get peers for a given infohash.
    ///
    /// Returns an blocking iterator over responses as they are received.
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
    /// for value in &mut response {
    ///     println!("Got peer: {:?} | from: {:?}", value.peer, value.from)
    /// }
    /// ```
    pub fn get_peers(&self, info_hash: Id) -> Response<GetPeerResponse> {
        let (sender, receiver) = flume::bounded::<ResponseMessage<GetPeerResponse>>(1);

        let _ = self.sender.send(ActorMessage::GetPeers(info_hash, sender));

        Response::new(receiver)
    }

    /// Announce a peer for a given infohash.
    ///
    /// The peer will be announced on this process IP.
    /// If explicit port is passed, it will be used, otherwise the port will be implicitly
    /// assumed by remote nodes to be the same ase port they recieved the request from.
    pub fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<ResponseMessage<GetPeerResponse>>(1);

        let _ = self.sender.send(ActorMessage::GetPeers(info_hash, sender));

        let mut response = Response::new(receiver);

        // Block until we got a Done response!
        for _ in &mut response {}

        self.announce_peer_to(info_hash, response.closest_nodes, port)
    }

    /// Async version of [announce_peer](Dht::announce_peer).
    #[cfg(feature = "async")]
    pub async fn announce_peer_async(
        &self,
        info_hash: Id,
        port: Option<u16>,
    ) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<ResponseMessage<GetPeerResponse>>(1);

        let _ = self.sender.send(ActorMessage::GetPeers(info_hash, sender));

        let mut response = Response::new(receiver);

        // Block until we got a Done response!
        while let Some(_) = response.next_async().await {}

        self.announce_peer_to_async(info_hash, response.closest_nodes, port)
            .await
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

    /// Async version of [announce_peer_to](Dht::announce_peer_to).
    #[cfg(feature = "async")]
    pub async fn announce_peer_to_async(
        &self,
        info_hash: Id,
        nodes: Vec<Node>,
        port: Option<u16>,
    ) -> Result<StoreQueryMetdata> {
        let (sender, receiver) = flume::bounded::<StoreQueryMetdata>(1);

        let _ = self
            .sender
            .send(ActorMessage::AnnouncePeer(info_hash, nodes, port, sender));

        receiver.recv_async().await.map_err(|e| e.into())
    }

    pub fn get_immutable(&self, info_hash: Id) -> Response<GetImmutableResponse> {
        let (sender, receiver) = flume::bounded::<ResponseMessage<GetImmutableResponse>>(1);

        let _ = self
            .sender
            .send(ActorMessage::GetImmutable(info_hash, sender));

        Response::new(receiver)
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

enum ActorMessage {
    Shutdown,
    LocalAddress(Sender<SocketAddr>),
    RoutingTable(Sender<RoutingTable>),

    GetPeers(Id, Sender<ResponseMessage<GetPeerResponse>>),
    AnnouncePeer(Id, Vec<Node>, Option<u16>, Sender<StoreQueryMetdata>),

    GetImmutable(Id, Sender<ResponseMessage<GetImmutableResponse>>),
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
    use std::time::Duration;

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
    fn bind_twice() {
        let a = Dht::default();
        let b = Dht::builder()
            .port(a.local_addr().unwrap().port())
            .as_server()
            .build();

        let result = b.handle.unwrap().join();
        assert!(result.is_err());
    }
}

//! AsyncDht node.

use bytes::Bytes;
use std::{net::SocketAddr, thread::JoinHandle};

use crate::{
    common::{
        hash_immutable, target_from_key, Id, MutableItem, PutResult, ResponseSender, RoutingTable,
    },
    dht::{ActorMessage, Dht},
    messages::{
        AnnouncePeerRequestArguments, GetPeersRequestArguments, GetValueRequestArguments,
        PutImmutableRequestArguments, PutMutableRequestArguments, PutRequestSpecific,
        RequestTypeSpecific,
    },
    Result,
};

impl Dht {
    pub fn as_async(self) -> AsyncDht {
        AsyncDht(self)
    }
}

#[derive(Debug, Clone)]
pub struct AsyncDht(Dht);

impl AsyncDht {
    // === Getters ===

    /// Returns the local address of the udp socket this node is listening on.
    pub async fn local_addr(&self) -> Result<SocketAddr> {
        let (sender, receiver) = flume::bounded::<SocketAddr>(1);

        self.0.sender.send(ActorMessage::LocalAddress(sender))?;

        Ok(receiver.recv_async().await?)
    }

    /// Returns a clone of the [RoutingTable] table of this node.
    pub async fn routing_table(&self) -> Result<RoutingTable> {
        let (sender, receiver) = flume::bounded::<RoutingTable>(1);

        self.0.sender.send(ActorMessage::RoutingTable(sender))?;

        Ok(receiver.recv_async().await?)
    }

    /// Returns the size of the [RoutingTable] without cloning the entire table.
    pub async fn routing_table_size(&self) -> Result<usize> {
        let (sender, receiver) = flume::bounded::<usize>(1);

        self.0.sender.send(ActorMessage::RoutingTableSize(sender))?;

        Ok(receiver.recv_async().await?)
    }

    /// Returns the `JoinHandle` of the actor thread
    pub fn handle(self) -> Option<JoinHandle<()>> {
        self.0.handle()
    }

    // === Public Methods ===

    pub fn shutdown(&self) -> Result<()> {
        self.0.shutdown()
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
    pub fn get_peers(&self, info_hash: Id) -> Result<flume::r#async::RecvStream<SocketAddr>> {
        // Get requests use unbounded channels to avoid blocking in the run loop.
        // Other requests like put_* and getters don't need that and is ok with
        // bounded channel with 1 capacity since it only ever sends one message back.
        //
        // So, if it is a ResponseMessage<_>, it should be unbounded, otherwise bounded.
        let (sender, receiver) = flume::unbounded::<SocketAddr>();

        let request = RequestTypeSpecific::GetPeers(GetPeersRequestArguments { info_hash });

        self.0.sender.send(ActorMessage::Get(
            info_hash,
            request,
            ResponseSender::Peer(sender),
        ))?;

        Ok(receiver.into_stream())
    }

    /// Announce a peer for a given infohash.
    ///
    /// The peer will be announced on this process IP.
    /// If explicit port is passed, it will be used, otherwise the port will be implicitly
    /// assumed by remote nodes to be the same ase port they recieved the request from.
    pub async fn announce_peer(&self, info_hash: Id, port: Option<u16>) -> Result<Id> {
        let (sender, receiver) = flume::bounded::<PutResult>(1);

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
            .sender
            .send(ActorMessage::Put(info_hash, request, sender))?;

        receiver.recv_async().await?
    }

    // === Immutable data ===

    /// Get an Immutable data by its sha1 hash.
    pub async fn get_immutable(&self, target: Id) -> Result<Bytes> {
        let (sender, receiver) = flume::unbounded::<Bytes>();

        let request = RequestTypeSpecific::GetValue(GetValueRequestArguments {
            target,
            seq: None,
            salt: None,
        });

        self.0.sender.send(ActorMessage::Get(
            target,
            request,
            ResponseSender::Immutable(sender),
        ))?;

        Ok(receiver.recv_async().await?)
    }

    /// Put an immutable data to the DHT.
    pub async fn put_immutable(&self, value: Bytes) -> Result<Id> {
        let target = Id::from_bytes(hash_immutable(&value)).unwrap();

        let (sender, receiver) = flume::bounded::<PutResult>(1);

        let request = PutRequestSpecific::PutImmutable(PutImmutableRequestArguments {
            target,
            v: value.clone().into(),
        });

        self.0
            .sender
            .send(ActorMessage::Put(target, request, sender))?;

        receiver.recv_async().await?
    }

    // === Mutable data ===

    /// Get a mutable data by its public_key and optional salt.
    pub fn get_mutable(
        &self,
        public_key: &[u8; 32],
        salt: Option<Bytes>,
    ) -> Result<flume::r#async::RecvStream<MutableItem>> {
        let target = target_from_key(public_key, &salt);

        let (sender, receiver) = flume::unbounded::<MutableItem>();

        let request = RequestTypeSpecific::GetValue(GetValueRequestArguments {
            target,
            seq: None,
            salt,
        });

        let _ = self.0.sender.send(ActorMessage::Get(
            target,
            request,
            ResponseSender::Mutable(sender),
        ));

        Ok(receiver.into_stream())
    }

    /// Put a mutable data to the DHT.
    pub async fn put_mutable(&self, item: MutableItem) -> Result<Id> {
        let (sender, receiver) = flume::bounded::<PutResult>(1);

        let request = PutRequestSpecific::PutMutable(PutMutableRequestArguments {
            target: *item.target(),
            v: item.value().clone().into(),
            k: item.key().to_vec(),
            seq: *item.seq(),
            sig: item.signature().to_vec(),
            salt: item.salt().clone().map(|s| s.to_vec()),
            cas: *item.cas(),
        });

        let _ = self
            .0
            .sender
            .send(ActorMessage::Put(*item.target(), request, sender));

        receiver.recv_async().await?
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use ed25519_dalek::SigningKey;
    use futures::StreamExt;

    use crate::Testnet;

    use super::*;

    #[test]
    fn shutdown() {
        async fn test() {
            let dht = Dht::default().as_async();

            dht.local_addr().await.unwrap();

            let a = dht.clone();

            dht.shutdown().unwrap();
            dht.handle().map(|h| h.join());

            let local_addr = a.local_addr().await;
            assert!(local_addr.is_err());
        }
        futures::executor::block_on(test());
    }

    #[test]
    fn bind_twice() {
        async fn test() {
            let a = Dht::default().as_async();
            let b = Dht::builder()
                .port(a.local_addr().await.unwrap().port())
                .as_server()
                .build()
                .as_async();

            let result = b.handle().unwrap().join();
            assert!(result.is_err());
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn announce_get_peer() {
        async fn test() {
            let testnet = Testnet::new(10);

            let a = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .as_async();
            let b = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .as_async();

            let info_hash = Id::random();

            a.announce_peer(info_hash, Some(45555))
                .await
                .expect("failed to announce");

            let peer = b
                .get_peers(info_hash)
                .unwrap()
                .next()
                .await
                .expect("No peers");

            assert_eq!(peer.port(), 45555);
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn put_get_immutable() {
        async fn test() {
            let testnet = Testnet::new(10);

            let a = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .as_async();
            let b = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .as_async();

            let value: Bytes = "Hello World!".into();
            let expected_target = Id::from_str("e5f96f6f38320f0f33959cb4d3d656452117aadb").unwrap();

            let target = a.put_immutable(value.clone()).await.unwrap();
            assert_eq!(target, expected_target);

            let response = b.get_immutable(target).await.unwrap();
            assert_eq!(response, value);
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn put_get_mutable() {
        async fn test() {
            let testnet = Testnet::new(10);

            let a = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .as_async();
            let b = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .as_async();

            let signer = SigningKey::from_bytes(&[
                56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
                228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
            ]);

            let seq = 1000;
            let value: Bytes = "Hello World!".into();

            let item = MutableItem::new(signer.clone(), value, seq, None);

            a.put_mutable(item.clone()).await.unwrap();

            let response = b
                .get_mutable(signer.verifying_key().as_bytes(), None)
                .unwrap()
                .next()
                .await
                .expect("No mutable values");

            assert_eq!(&response, &item);
        }

        futures::executor::block_on(test());
    }
}
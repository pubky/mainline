//! AsyncDht node.

use std::net::SocketAddrV4;

use futures_lite::StreamExt;

use crate::{
    common::{
        hash_immutable, AnnouncePeerRequestArguments, FindNodeRequestArguments,
        GetPeersRequestArguments, GetValueRequestArguments, Id, MutableItem, Node,
        PutImmutableRequestArguments, PutMutableRequestArguments, PutRequestSpecific,
    },
    dht::{
        ActorMessage, AnnouncePeerError, Dht, DhtWasShutdown, PutImmutableError, PutMutableError,
        ResponseSender,
    },
    rpc::{GetRequestSpecific, Info, PutError},
};

impl Dht {
    /// Return an async version of the Dht client.
    pub fn as_async(self) -> AsyncDht {
        AsyncDht(self)
    }
}

#[derive(Debug, Clone)]
/// Async version of the Dht node.
pub struct AsyncDht(Dht);

impl AsyncDht {
    // === Getters ===

    /// Information and statistics about this [Dht] node.
    pub async fn info(&self) -> Result<Info, DhtWasShutdown> {
        let (sender, receiver) = flume::bounded::<Info>(1);

        self.0
             .0
            .send(ActorMessage::Info(sender))
            .map_err(|_| DhtWasShutdown)?;

        receiver.recv_async().await.map_err(|_| DhtWasShutdown)
    }

    /// Turn this node's routing table to a list of bootstraping nodes.   
    pub async fn to_bootstrap(&self) -> Result<Vec<String>, DhtWasShutdown> {
        let (sender, receiver) = flume::bounded::<Vec<String>>(1);

        self.0
             .0
            .send(ActorMessage::ToBootstrap(sender))
            .map_err(|_| DhtWasShutdown)?;

        receiver.recv_async().await.map_err(|_| DhtWasShutdown)
    }

    // === Public Methods ===

    /// Shutdown the actor thread loop.
    pub async fn shutdown(&mut self) {
        let (sender, receiver) = flume::bounded::<()>(1);

        let _ = self.0 .0.send(ActorMessage::Shutdown(sender));
        let _ = receiver.recv_async().await;
    }

    /// Wait until the bootstraping query is done.
    ///
    /// Returns true if the bootstraping was successful.
    pub async fn bootstrapped(&self) -> Result<bool, DhtWasShutdown> {
        let info = self.info().await?;

        let nodes = self.find_node(*info.id()).await?;

        Ok(!nodes.is_empty())
    }

    // === Find nodes ===

    pub async fn find_node(&self, target: Id) -> Result<Vec<Node>, DhtWasShutdown> {
        let (sender, receiver) = flume::bounded::<Vec<Node>>(1);

        let request = GetRequestSpecific::FindNode(FindNodeRequestArguments { target });

        self.0
             .0
            .send(ActorMessage::Get(
                target,
                request,
                ResponseSender::ClosestNodes(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver
            .recv_async()
            .await
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
    ) -> Result<flume::r#async::RecvStream<Vec<SocketAddrV4>>, DhtWasShutdown> {
        // Get requests use unbounded channels to avoid blocking in the run loop.
        // Other requests like put_* and getters don't need that and is ok with
        // bounded channel with 1 capacity since it only ever sends one message back.
        //
        // So, if it is a ResponseMessage<_>, it should be unbounded, otherwise bounded.
        let (sender, receiver) = flume::unbounded::<Vec<SocketAddrV4>>();

        let request = GetRequestSpecific::GetPeers(GetPeersRequestArguments { info_hash });

        self.0
             .0
            .send(ActorMessage::Get(
                info_hash,
                request,
                ResponseSender::Peers(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver.into_stream())
    }

    /// Announce a peer for a given infohash.
    ///
    /// The peer will be announced on this process IP.
    /// If explicit port is passed, it will be used, otherwise the port will be implicitly
    /// assumed by remote nodes to be the same ase port they recieved the request from.
    pub async fn announce_peer(
        &self,
        info_hash: Id,
        port: Option<u16>,
    ) -> Result<Id, AnnouncePeerError> {
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
             .0
            .send(ActorMessage::Put(info_hash, request, sender))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver
            .recv_async()
            .await
            .expect("Query was dropped before sending a response, please open an issue.")
            .map_err(|error| match error {
                PutError::Query(err) => err,
                PutError::Concurrency(_) => {
                    unreachable!("should not receieve a concurrency error from announce peer query")
                }
            })?)
    }

    // === Immutable data ===

    /// Get an Immutable data by its sha1 hash.
    pub async fn get_immutable(&self, target: Id) -> Result<Option<Box<[u8]>>, DhtWasShutdown> {
        let (sender, receiver) = flume::unbounded::<Box<[u8]>>();

        let request = GetRequestSpecific::GetValue(GetValueRequestArguments {
            target,
            seq: None,
            salt: None,
        });

        self.0
             .0
            .send(ActorMessage::Get(
                target,
                request,
                ResponseSender::Immutable(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver.recv_async().await.map(Some).unwrap_or(None))
    }

    /// Put an immutable data to the DHT.
    pub async fn put_immutable(&self, value: &[u8]) -> Result<Id, PutImmutableError> {
        let target: Id = hash_immutable(value).into();

        let (sender, receiver) = flume::bounded::<Result<Id, PutError>>(1);

        let request = PutRequestSpecific::PutImmutable(PutImmutableRequestArguments {
            target,
            v: value.into(),
        });

        self.0
             .0
            .send(ActorMessage::Put(target, request, sender))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver
            .recv_async()
            .await
            .expect("Query was dropped before sending a response, please open an issue.")
            .map_err(|error| match error {
                PutError::Query(err) => err,
                PutError::Concurrency(_) => {
                    unreachable!("should not receieve a concurrency error from announce peer query")
                }
            })?)
    }

    // === Mutable data ===

    /// Get a mutable data by its `public_key` and optional `salt`.
    ///
    /// You can specify the exact `seq` you are looking for, otherwise,
    /// nodes will respond with any item with the same `public_key` and `salt`.
    pub fn get_mutable(
        &self,
        public_key: &[u8; 32],
        salt: Option<&[u8]>,
        seq: Option<i64>,
    ) -> Result<flume::r#async::RecvStream<MutableItem>, DhtWasShutdown> {
        let target = MutableItem::target_from_key(public_key, salt);

        let (sender, receiver) = flume::unbounded::<MutableItem>();

        let request = GetRequestSpecific::GetValue(GetValueRequestArguments {
            target,
            seq,
            salt: salt.map(|s| s.into()),
        });

        self.0
             .0
            .send(ActorMessage::Get(
                target,
                request,
                ResponseSender::Mutable(sender),
            ))
            .map_err(|_| DhtWasShutdown)?;

        Ok(receiver.into_stream())
    }

    /// Get the most recent [MutableItem] from the network.
    pub async fn get_mutable_most_recent(
        &self,
        public_key: &[u8; 32],
        salt: Option<&[u8]>,
    ) -> Result<Option<MutableItem>, DhtWasShutdown> {
        let mut most_recent: Option<MutableItem> = None;

        let mut stream = self.get_mutable(public_key, salt, None)?;

        while let Some(item) = stream.next().await {
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
    /// let dht = Dht::builder().bootstrap(&testnet.bootstrap).build().unwrap().as_async();
    ///
    /// let signing_key = SigningKey::from_bytes(&[0; 32]);
    /// let key = signing_key.verifying_key().to_bytes();
    /// let salt = Some(b"salt".as_ref());
    ///
    /// futures::executor::block_on(async move {
    ///     let (item, cas) = if let Some(most_recent) = dht
    ///         .get_mutable_most_recent(&key, salt)
    ///         .await
    ///         .unwrap()
    ///     {
    ///         // 1. Optionally Create a new value to take the most recent's value in consideration.
    ///         let mut new_value = most_recent.value().to_vec();
    ///         new_value.extend_from_slice(b" more data");
    ///
    ///         // 2. Increment the sequence number to be higher than the most recent's.
    ///         let most_recent_seq = most_recent.seq();
    ///         let new_seq = most_recent_seq + 1;
    ///
    ///         (
    ///             MutableItem::new(signing_key, &new_value, new_seq, salt),
    ///             // 3. Use the most recent [MutableItem::seq] as a `CAS`.
    ///             Some(most_recent_seq)
    ///         )
    ///     } else {
    ///         (MutableItem::new(signing_key, b"first value", 1, salt), None)
    ///     };
    ///
    ///     dht.put_mutable(item, cas).await.unwrap();
    /// });
    /// ```
    ///
    /// ## Errors
    ///
    /// In addition to the [PutQueryError][crate::errors::PutQueryError] common with all PUT queries, PUT mutable item
    /// query has other [Concurrency errors][crate::errors::ConcurrencyError], that try to detect write conflict
    /// risks or obvious conflicts.
    ///
    /// If you are lucky to get one of these errors (which is not guaranteed), then you should
    /// read the most recent item again, and repeat the steps in the previous example.
    pub async fn put_mutable(
        &self,
        item: MutableItem,
        cas: Option<i64>,
    ) -> Result<Id, PutMutableError> {
        let (sender, receiver) = flume::bounded::<Result<Id, PutError>>(1);

        let target = *item.target();

        let request = PutRequestSpecific::PutMutable(PutMutableRequestArguments::from(item, cas));

        self.0
             .0
            .send(ActorMessage::Put(target, request, sender))
            .map_err(|_| DhtWasShutdown)?;

        receiver
            .recv_async()
            .await
            .expect("Query was dropped before sending a response, please open an issue.")
            .map_err(|error| match error {
                PutError::Query(err) => PutMutableError::Query(err),
                PutError::Concurrency(err) => PutMutableError::Concurrency(err),
            })
    }
}

#[cfg(test)]
mod test {
    use std::{str::FromStr, time::Duration};

    use ed25519_dalek::SigningKey;
    use futures::StreamExt;

    use crate::{dht::Testnet, rpc::ConcurrencyError};

    use super::*;

    #[test]
    fn shutdown() {
        async fn test() {
            let mut dht = Dht::client().unwrap().as_async();

            let a = dht.clone();

            dht.shutdown().await;

            let result = a.get_immutable(Id::random()).await;

            assert!(matches!(result, Err(DhtWasShutdown)))
        }
        futures::executor::block_on(test());
    }

    #[test]
    fn announce_get_peer() {
        async fn test() {
            let testnet = Testnet::new(10).unwrap();

            let a = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();
            let b = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();

            let info_hash = Id::random();

            a.announce_peer(info_hash, Some(45555))
                .await
                .expect("failed to announce");

            let peers = b
                .get_peers(info_hash)
                .unwrap()
                .next()
                .await
                .expect("No peers");

            assert_eq!(peers.first().unwrap().port(), 45555);
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn put_get_immutable() {
        async fn test() {
            let testnet = Testnet::new(10).unwrap();

            let a = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();
            let b = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();

            let value = b"Hello World!";
            let expected_target = Id::from_str("e5f96f6f38320f0f33959cb4d3d656452117aadb").unwrap();

            let target = a.put_immutable(value).await.unwrap();
            assert_eq!(target, expected_target);

            let response = b.get_immutable(target).await.unwrap();
            assert_eq!(response, Some(value.to_vec().into_boxed_slice()));
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn put_get_mutable() {
        async fn test() {
            let testnet = Testnet::new(10).unwrap();

            let a = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();
            let b = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();

            let signer = SigningKey::from_bytes(&[
                56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
                228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
            ]);

            let seq = 1000;
            let value = b"Hello World!";

            let item = MutableItem::new(signer.clone(), value, seq, None);

            a.put_mutable(item.clone(), None).await.unwrap();

            let response = b
                .get_mutable(signer.verifying_key().as_bytes(), None, None)
                .unwrap()
                .next()
                .await
                .expect("No mutable values");

            assert_eq!(&response, &item);
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn put_get_mutable_no_more_recent_value() {
        async fn test() {
            let testnet = Testnet::new(10).unwrap();

            let a = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();
            let b = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();

            let signer = SigningKey::from_bytes(&[
                56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
                228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
            ]);

            let seq = 1000;
            let value = b"Hello World!";

            let item = MutableItem::new(signer.clone(), value, seq, None);

            a.put_mutable(item.clone(), None).await.unwrap();

            let response = b
                .get_mutable(signer.verifying_key().as_bytes(), None, Some(seq))
                .unwrap()
                .next()
                .await;

            assert!(&response.is_none());
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn repeated_put_query() {
        async fn test() {
            let testnet = Testnet::new(10).unwrap();

            let a = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();

            let first = a.put_immutable(&[1, 2, 3]);
            let second = a.put_immutable(&[1, 2, 3]);

            assert_eq!(first.await.unwrap(), second.await.unwrap());
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn concurrent_get_mutable() {
        async fn test() {
            let testnet = Testnet::new(10).unwrap();

            let a = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();
            let b = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();

            let signer = SigningKey::from_bytes(&[
                56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
                228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
            ]);

            let seq = 1000;
            let value = b"Hello World!";

            let item = MutableItem::new(signer.clone(), value, seq, None);

            a.put_mutable(item.clone(), None).await.unwrap();

            let _response_first = b
                .get_mutable(signer.verifying_key().as_bytes(), None, None)
                .unwrap()
                .next()
                .await
                .expect("No mutable values");

            let response_second = b
                .get_mutable(signer.verifying_key().as_bytes(), None, None)
                .unwrap()
                .next()
                .await
                .expect("No mutable values");

            assert_eq!(&response_second, &item);
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn concurrent_put_mutable_same() {
        let testnet = Testnet::new(10).unwrap();

        let dht = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap()
            .as_async();

        let signer = SigningKey::from_bytes(&[
            56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
            228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
        ]);

        let seq = 1000;
        let value = b"Hello World!";

        let item = MutableItem::new(signer.clone(), value, seq, None);

        let mut handles = vec![];

        for _ in 0..2 {
            let dht = dht.clone();
            let item = item.clone();

            let handle = std::thread::spawn(move || {
                futures::executor::block_on(async { dht.put_mutable(item, None).await.unwrap() });
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn concurrent_put_mutable_different() {
        let testnet = Testnet::new(10).unwrap();

        let dht = Dht::builder()
            .bootstrap(&testnet.bootstrap)
            .build()
            .unwrap()
            .as_async();

        let mut handles = vec![];

        for i in 0..2 {
            let dht = dht.clone();

            let signer = SigningKey::from_bytes(&[
                56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
                228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
            ]);

            let seq = 1000;

            let mut value = b"Hello World!".to_vec();
            value.push(i);

            let item = MutableItem::new(signer.clone(), &value, seq, None);

            let handle = std::thread::spawn(move || {
                futures::executor::block_on(async {
                    let result = dht.put_mutable(item, None).await;
                    if i == 0 {
                        assert!(matches!(result, Ok(_)))
                    } else {
                        assert!(matches!(
                            result,
                            Err(PutMutableError::Concurrency(ConcurrencyError::ConflictRisk))
                        ))
                    }
                })
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn concurrent_put_mutable_different_with_cas() {
        async fn test() {
            let testnet = Testnet::new(10).unwrap();

            let dht = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();

            let signer = SigningKey::from_bytes(&[
                56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
                228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
            ]);
            let value = b"Hello World!".to_vec();

            // First
            {
                let item = MutableItem::new(signer.clone(), &value, 1000, None);

                let (sender, _) = flume::bounded::<Result<Id, PutError>>(1);
                let target = *item.target();
                let request =
                    PutRequestSpecific::PutMutable(PutMutableRequestArguments::from(item, None));
                dht.0
                     .0
                    .send(ActorMessage::Put(target, request, sender))
                    .map_err(|_| DhtWasShutdown)
                    .unwrap();
            }

            std::thread::sleep(Duration::from_millis(100));

            // Second
            {
                let item = MutableItem::new(signer, &value, 1001, None);

                let most_recent = dht.get_mutable_most_recent(item.key(), None).await.unwrap();

                if let Some(cas) = most_recent.map(|item| item.seq()) {
                    dht.put_mutable(item, Some(cas)).await.unwrap();
                } else {
                    dht.put_mutable(item, None).await.unwrap();
                }
            }
        }

        futures::executor::block_on(test());
    }

    #[test]
    fn conflict_301_cas() {
        async fn test() {
            let testnet = Testnet::new(10).unwrap();

            let dht = Dht::builder()
                .bootstrap(&testnet.bootstrap)
                .build()
                .unwrap()
                .as_async();

            let signer = SigningKey::from_bytes(&[
                56, 171, 62, 85, 105, 58, 155, 209, 189, 8, 59, 109, 137, 84, 84, 201, 221, 115, 7,
                228, 127, 70, 4, 204, 182, 64, 77, 98, 92, 215, 27, 103,
            ]);
            let value = b"Hello World!".to_vec();

            dht.put_mutable(MutableItem::new(signer.clone(), &value, 1001, None), None)
                .await
                .unwrap();

            assert!(matches!(
                dht.put_mutable(MutableItem::new(signer, &value, 1002, None), Some(1000))
                    .await,
                Err(PutMutableError::Concurrency(ConcurrencyError::CasFailed))
            ));
        }

        futures::executor::block_on(test());
    }
}

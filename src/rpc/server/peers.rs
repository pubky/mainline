//! Manage announced peers for info_hashes

use std::{collections::HashMap, net::SocketAddr};

use rand::{rngs::ThreadRng, seq::SliceRandom, thread_rng};

use crate::{common::MAX_BUCKET_SIZE_K, Id};

const MAX_PEERS: usize = 10000;
const MAX_PEERS_PER_INFO_HASH: usize = 100;

#[derive(Debug)]
pub struct PeersStore {
    rng: ThreadRng,
    lru: Vec<(Id, SocketAddr)>,
    peers: HashMap<Id, Vec<SocketAddr>>,
}

impl PeersStore {
    pub fn new() -> Self {
        Self {
            rng: thread_rng(),
            lru: Vec::new(),
            peers: HashMap::new(),
        }
    }

    pub fn add_peer(&mut self, info_hash: Id, peer: SocketAddr) {
        let incoming = (info_hash, peer);

        // If the item is already in the LRU, bring it to the end.
        // This is where we ensure unique (info_hash, peer) tuple in the store.
        if let Some(index) = self.lru.iter().position(|i| i == &incoming) {
            // Bring the item to the end of the LRU
            self.lru.remove(index);
            self.lru.push(incoming);
            return;
        }

        // If the LRU is full, remove the oldest item.
        if self.lru.len() >= MAX_PEERS {
            let (info_hash, _) = self.lru.remove(0);
            let peers = self.peers.get_mut(&info_hash).unwrap();
            peers.remove(0);
        }

        let info_hash_peers = self.peers.entry(info_hash).or_default();

        // If the info_hash is full of peers, remove the oldest one.
        if info_hash_peers.len() >= MAX_PEERS_PER_INFO_HASH {
            let peer = info_hash_peers.remove(0);

            self.lru.retain(|i| i != &(info_hash, peer))
        }

        // Add the new item to the info_hash_peers
        info_hash_peers.push(peer);
        self.lru.push(incoming);
    }

    pub fn get_random_peers(&mut self, info_hash: &Id) -> Option<Vec<SocketAddr>> {
        if let Some(peers) = self.peers.get(info_hash) {
            let peers = peers.clone();

            let random_20: Vec<SocketAddr> = peers
                .choose_multiple(&mut self.rng, MAX_BUCKET_SIZE_K)
                .cloned()
                .collect();

            return Some(random_20);
        }

        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn lru() {
        let mut store = PeersStore::new();

        let info_hash_a = Id::random();
        let info_hash_b = Id::random();

        store.add_peer(info_hash_a, SocketAddr::from(([127, 0, 1, 1], 0)));
        store.add_peer(info_hash_a, SocketAddr::from(([127, 0, 1, 1], 0)));
        store.add_peer(info_hash_a, SocketAddr::from(([127, 0, 1, 1], 0)));

        store.add_peer(info_hash_b, SocketAddr::from(([127, 0, 2, 1], 0)));

        store.add_peer(info_hash_a, SocketAddr::from(([127, 0, 1, 2], 0)));
        store.add_peer(info_hash_b, SocketAddr::from(([127, 0, 2, 2], 0)));
        store.add_peer(info_hash_a, SocketAddr::from(([127, 0, 1, 2], 0)));

        let mut order: Vec<(Id, SocketAddr)> = Vec::new();

        for item in &store.lru {
            order.push(*item);
        }

        let expected = vec![
            (info_hash_a, SocketAddr::from(([127, 0, 1, 1], 0))),
            (info_hash_b, SocketAddr::from(([127, 0, 2, 1], 0))),
            (info_hash_b, SocketAddr::from(([127, 0, 2, 2], 0))),
            (info_hash_a, SocketAddr::from(([127, 0, 1, 2], 0))),
        ];

        assert_eq!(order, expected);
    }
}

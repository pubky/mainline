//! Simplified Kademlia routing table

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::slice::Iter;

use crate::common::{Id, Node, MAX_DISTANCE};

/// K = the default maximum size of a k-bucket.
pub const MAX_BUCKET_SIZE_K: usize = 20;

#[derive(Debug, Clone)]
/// Simplified Kademlia routing table
pub struct RoutingTable {
    id: Id,
    buckets: BTreeMap<u8, KBucket>,
}

impl RoutingTable {
    pub fn new() -> Self {
        let buckets = BTreeMap::new();

        RoutingTable {
            id: Id::random(),
            buckets,
        }
    }

    // === Options ===

    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    // === Public Methods ===

    pub fn add(&mut self, node: Node) -> bool {
        let distance = self.id.distance(&node.id);

        if distance == 0 {
            // Do not add self to the routing_table
            return false;
        }

        let bucket = self.buckets.entry(distance).or_default();

        bucket.add(node)
    }

    pub fn remove(&mut self, node_id: &Id) {
        let distance = self.id.distance(node_id);

        if let Some(bucket) = self.buckets.get_mut(&distance) {
            bucket.remove(node_id)
        }
    }

    /// Return the closest nodes to the target while prioritizing secure nodes,
    /// as defined in [BEP_0042](https://www.bittorrent.org/beps/bep_0042.html)
    pub fn closest(&self, target: &Id) -> Vec<Node> {
        let mut result = Vec::with_capacity(MAX_BUCKET_SIZE_K);
        let mut unsecure = Vec::with_capacity(MAX_BUCKET_SIZE_K);

        let distance = self.id.distance(target);

        for i in
            // First search in closest nodes
            (distance..=MAX_DISTANCE)
                // if we don't have enough close nodes, populate from other buckets
                .chain((0..distance).rev())
        {
            match &self.buckets.get(&i) {
                Some(bucket) => {
                    for node in bucket.iter() {
                        if result.len() < 20 {
                            if node.is_secure() {
                                result.push(node.clone());
                            } else {
                                unsecure.push(node.clone())
                            }
                        } else {
                            return result;
                        }
                    }
                }
                None => continue,
            }
        }

        if result.len() < 20 {
            for node in unsecure {
                if result.len() < 20 {
                    result.push(node);
                } else {
                    break;
                }
            }
        }

        result
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.values().all(|bucket| bucket.is_empty())
    }

    pub fn size(&self) -> usize {
        self.buckets
            .values()
            .fold(0, |acc, bucket| acc + bucket.nodes.len())
    }

    /// Returns all nodes in the routing_table.
    pub fn to_vec(&self) -> Vec<Node> {
        let mut nodes: Vec<Node> = vec![];

        for bucket in self.buckets.values() {
            for node in &bucket.nodes {
                nodes.push(node.clone());
            }
        }

        nodes
    }

    // === Private Methods ===

    #[cfg(test)]
    fn contains(&self, node_id: &Id) -> bool {
        let distance = self.id.distance(node_id);

        if let Some(bucket) = self.buckets.get(&distance) {
            if bucket.contains(node_id) {
                return true;
            }
        }
        false
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Kbuckets are similar to LRU caches that checks and evicts unresponsive nodes,
/// without dropping any responsive nodes in the process.
#[derive(Debug, Clone)]
pub struct KBucket {
    /// Nodes in the k-bucket, sorted by the least recently seen.
    nodes: Vec<Node>,
}

impl KBucket {
    pub fn new() -> Self {
        KBucket {
            nodes: Vec::with_capacity(MAX_BUCKET_SIZE_K),
        }
    }

    // === Public Methods ===

    pub fn add(&mut self, incoming: Node) -> bool {
        if let Some(index) = self.iter().position(|n| n.id == incoming.id) {
            // If it is the same socket address too, remove the old node,
            // and add the incoming one, effectively updating the node's
            // `last_seen` and moving it to the end of the bucket.
            if self.nodes[index].same_adress(&incoming) {
                self.nodes.remove(index);
                self.nodes.push(incoming);

                true
            } else {
                false
            }
        } else if self.nodes.len() < MAX_BUCKET_SIZE_K {
            self.nodes.push(incoming);
            true
        } else if self.nodes[0].is_stale() {
            // Remove the least recently seen node and add the new one
            self.nodes.remove(0);
            self.nodes.push(incoming);

            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, node_id: &Id) {
        self.nodes.retain(|node| node.id != *node_id);
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, Node> {
        self.nodes.iter()
    }

    #[cfg(test)]
    fn contains(&self, id: &Id) -> bool {
        self.iter().any(|node| node.id == *id)
    }
}

impl Default for KBucket {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use std::net::SocketAddr;
    use std::str::FromStr;

    use crate::common::{Id, KBucket, Node, RoutingTable, MAX_BUCKET_SIZE_K};

    #[test]
    fn table_is_empty() {
        let mut table = RoutingTable::new();
        assert!(table.is_empty());

        table.add(Node::random());
        assert!(!table.is_empty());
    }

    #[test]
    fn to_vec() {
        let mut table = RoutingTable::new();

        let mut expected_nodes: Vec<Node> = vec![];

        for _ in 0..20 {
            expected_nodes.push(Node::random());
        }

        for node in &expected_nodes {
            table.add(node.clone());
        }

        let mut sorted_table = table.to_vec();
        sorted_table.sort_by(|a, b| a.id.cmp(&b.id));

        let mut sorted_expected = expected_nodes.to_vec();
        sorted_expected.sort_by(|a, b| a.id.cmp(&b.id));

        assert_eq!(sorted_table, sorted_expected);
    }

    #[test]
    fn contains() {
        let mut table = RoutingTable::new();

        let node = Node::random();

        assert!(!table.contains(&node.id));

        table.add(node.clone());
        assert!(table.contains(&node.id));
    }

    #[test]
    fn remove() {
        let mut table = RoutingTable::new();

        let node = Node::random();

        table.add(node.clone());
        assert!(table.contains(&node.id));

        table.remove(&node.id);
        assert!(!table.contains(&node.id));
    }

    #[test]
    fn buckets_are_sets() {
        let mut table = RoutingTable::new();

        let node1 = Node::random();
        let node2 = Node::new(node1.id, node1.address);

        table.add(node1);
        table.add(node2);

        assert_eq!(table.to_vec().len(), 1);
    }

    #[test]
    fn should_not_add_self() {
        let mut table = RoutingTable::new();
        let node = Node::random().with_id(table.id);

        table.add(node.clone());

        assert!(!table.add(node));
        assert!(table.is_empty())
    }

    #[test]
    fn should_not_add_more_than_k() {
        let mut bucket = KBucket::new();

        for i in 0..MAX_BUCKET_SIZE_K {
            let node = Node::random();
            assert!(bucket.add(node), "Failed to add node {}", i);
        }

        let node = Node::random();

        assert!(!bucket.add(node));
    }

    #[test]
    fn should_update_existing_node() {
        // Same address
        {
            let mut bucket = KBucket::new();

            let node1 = Node::random();
            let node2 = Node::new(node1.id, node1.address);

            bucket.add(node1.clone());
            bucket.add(Node::random());

            assert_ne!(bucket.nodes[1].id, node1.id);

            bucket.add(node2);

            assert_eq!(bucket.nodes.len(), 2);
            assert_eq!(bucket.nodes[1].id, node1.id);
        }

        // Different address
        {
            let mut bucket = KBucket::new();

            let node1 = Node::random();
            let node2 = Node::new(node1.id, SocketAddr::from(([0, 0, 0, 0], 1)));

            bucket.add(node1.clone());
            bucket.add(Node::random());

            assert_ne!(bucket.nodes[1].id, node1.id);

            bucket.add(node2.clone());

            assert_eq!(bucket.nodes.len(), 2);
            assert_ne!(bucket.nodes[1].id, node1.id);
            assert_ne!(bucket.nodes[1].address, node2.address);
        }
    }

    #[test]
    fn closest() {
        let ids = [
            "fb449c17f6c34fadea26a5a83e1952e815e001ea",
            "e63b72f95aacee40ad087f83afb475645739f669",
            "58c65677e3833cb0f15733a6363cc4cb1352f90a",
            "fd042ff1404b495720ad8345404ff5f25acd02a8",
            "dbed34a2c8db568fe59c10adcca9e81825b3dcfd",
            "079d40b746b5721f59972ebde423429739844914",
            "094f1d2fb4b95ba2c3250b014a9f06d13cd9eb9a",
            "98805a55523458c56d59339266bdcecc82370ecd",
            "0a1d6cce47c60f2c7357e9fec2910192de6eb336",
            "fb689ce0e18c2c22f316976d3ae524aed4137773",
            "0d01c32b4cf386b0b784b718b999d0e9dac07876",
            "9465e80d80f707b222c4ae6ee81c02b62f607629",
            "6cdc012328cc7a3a9a5b967e93387686e19c9f75",
            "99719dfc220b145e2aac71d6b3e276731d85be1c",
            "94d2037bbc534a5f1d672ce3e3350576c2b78ed1",
            "b48d0aeb94cd3766f23d2ac098bbccf01485dc20",
            "3b6e1c05f199edd7dee87d3cc8422c8f0ed02358",
            "d9b50c6ca730c89f8fc9f518136cef6139dd2252",
            "15827c92e6efbc4f56e507e548409c4bc04360bf",
            "3c8ff1e484c21132f8e6b8112a2feab984536f57",
            "c9a8163fa3e85065d46567bfac39b5452cfb3ae8",
            "ef79f77e9eed9ad51094ce2747e2c4fdc3a81326",
            "81f038cabb8a845f39da0d40716bf0707da55187",
            "907fdf0aa137200b395bc210763ed947b03dfc2e",
            "b0bce9873042aee29cbc7ec395647f6cc7a482f8",
            "e6b8d5567bc05d9b68f23d562645bc030729abc9",
            "74667cb7c629fb7e63749134b16e27446984c517",
            "cdc7f4d5825dc316de20d998bc0f1c5e91e36a5e",
            "701e7b5af5fabcf0bc3de97cb05a7c00da3e53c6",
            "36eb09b1db4af2b11312742faa2bb42621fce753",
            "9e4923966754c02b036698e95f95cec8fc40a9d2",
            "0e43d66e9da1bfc7e2581155dfd1b8f4be57d3f1",
            "647679a0d8816d2f62200e7b6ef6171297756dd5",
            "c03d9008add37f8414cb41549448bb2dcb5c6c9b",
            "dff82b028a6ec033e00b387df8e386417b92a47c",
            "42e8b38494b0ee11003592da11b5cbe43332190e",
            "03161976385301ac9b965202e8f3922cef840790",
            "7d598e5726fb58501d8cc65faf6b676bab7cb4bc",
            "54ddde105d3f2c6ea7a5e7641ff24522eea2e784",
            "3a75532b5916c772c1b7a18627bf170cf915aeb3",
            "fa2b38321419e63cb890f8a8b5c53a1c4728a10a",
            "a3ba598bee9da287092f4f2f3864322af38e1824",
            "a94df01f21d870a006748b6ab3c04d31428c959d",
            "396aabc66c603617f376409053d1e2cec3813101",
            "a7b4becc2304da63792eb6c33f95677b2e7c9f8c",
            "58b1623af15a9828ccf41b8cee47d123c5cfe8b6",
            "3cb7eeac7be3a0195a9243537d452f790ccf1ca9",
            "e0296cfc4726d91a1f7f041e24638a1276a08bed",
            "aeb03edad3edc7c54a3c5f7916ecba981e65ce91",
            "4a81a4596b7c4b8706fd8b5c88ddfde18ca72293",
            "0d4e9ae7c486e5a0361bd4e3b918b6bdca89cfcb",
            "81d394b44403315f9845c3da6f018b8daedd89ef",
            "345630675ff0f319c8f2bb355edf59f9bd93072f",
            "b61fbd992a13af05feba939f597b5f6ee61188e3",
            "5ea45447e2e79a5f3b3d8c2f68aebdabf71c42f9",
            "84325dd6fbd9a93f4ab61d091a9562a6c6111df4",
            "e7c796aeecd47cfd01a2d62fd3fb1d41aafa2464",
            "897457b33c4eb1ffcab08331877108cbf3fac6de",
            "833843b1f33e720c17bccfb75647a49040861b4c",
            "06b49c253d3fc9800cfd75605d26426f8ccb89af",
            "5024212c42bed9f45e48c450147fecb3e934fc4e",
            "5a9de8041b045a7a4f85b71a6dc6a794a7fcd4ea",
            "70cad33774ddacb51ed1918adedeb67ff13a3b1e",
            "840d201e3c213c01b4ab85983efaac44f0671552",
            "aa7ffc7999a1b1bb79ce19b61c37f70331f492d6",
            "e2ec0c07e15411564292b5fa75246e4c385f4411",
            "38c1a0d14f548d4d81655920ec564b08e9fcf5e6",
            "1d128b8343569c7e9a8985879fafd325d458d31c",
            "4fcd30cbe02b74cece57babac93aded26ecdc893",
            "57d8a6d782ee1df62ceebd5d10884805ed382336",
            "54443ed3476d1d542f37bf069973bbd2b64c1b27",
            "0e7ba6c5e4c29cf4fff25733892b63cf2a6efdfc",
            "18824378226a6d33bcdbe39dd3bc9ee656ce20a2",
            "93b0cb01befc90b65a0026acf85bea2fefec7d44",
            "d65e378a1ec70cc79ae5b4469ae7f0e8939033fe",
            "9230a2f8ac81e73f16c63dd60adb030328fbc983",
            "302de797c9d73275ea184d7f6a8bf77364a8fd52",
            "cea92f6e6612ef408d8c22ad5c1ed602bb2aedbf",
            "353f2ff278f4ee038e7b217276a82d6ed0617130",
            "e962e3a1946afa0d3ee97f3a0418cb3489a5f84c",
            "a4e42b6cf98e957684aa4e7006940d31bcb76b1f",
            "57af8f960b2450ffa0dc5bc7314fece53996d4d0",
            "28e73f73084bc8e91fe9ec0a5581b583ef468d8c",
            "9481589ddec9a6d9ad2cee7f73e8319aab3f1e95",
            "edec09cc7476cd019560874def4af852bfeaffe3",
            "6c3ae2cf5f9452d5176788e15635c5958581c931",
            "f547b9717e84036c3d5eefec6d6ee3bfa5af89cb",
            "87b51f4bf1ccd41cda3aa85c71da5de56aeeda33",
            "e743092a576b92c8c05e04d5d2b23f2838825fd1",
            "e713b84894b761e2a4e20fd0e5a81ae48a6b6f9d",
            "6b1abca34099d2436bac8ab25aa17a57cbfe1564",
            "93cb2977e536a680c043b158345254c14b946d52",
            "8c2754fa9e93cbf1cccfd9241ebe0cc141199cfe",
            "13e4abf95a8a9e6525419b4db7b1704ed0a2789d",
            "8d53d453d7cfbb9bc386e128fa68aca388a5ddc6",
            "caebf39e9c9b48d87277f2a13faa5931a24819a4",
            "5025ca6cda98f31bc3ef321dd9a015b7f06b8bfa",
            "531fbf18fdf3e513091614f20d65e920a505ca41",
            "2f81e6159f7de0bc90c8a1db661b33bffbee85fd",
            "85d4d9954f3a28228a2786b320ad58a46a13f37b",
        ];

        let nodes: Vec<Node> = ids
            .iter()
            .map(|str| {
                let id = Id::from_str(str).unwrap();
                Node::random().with_id(id)
            })
            .collect();

        let local_id = Id::from_str("ba3042eb2d373b19e7c411ce6826e31b37be0b2e").unwrap();

        let mut table = RoutingTable::new().with_id(local_id);

        for node in nodes {
            table.add(node);
        }

        {
            let expected_closest_ids: Vec<_> = [
                "81d394b44403315f9845c3da6f018b8daedd89ef",
                "81f038cabb8a845f39da0d40716bf0707da55187",
                "833843b1f33e720c17bccfb75647a49040861b4c",
                "84325dd6fbd9a93f4ab61d091a9562a6c6111df4",
                "897457b33c4eb1ffcab08331877108cbf3fac6de",
                "907fdf0aa137200b395bc210763ed947b03dfc2e",
                "9465e80d80f707b222c4ae6ee81c02b62f607629",
                "94d2037bbc534a5f1d672ce3e3350576c2b78ed1",
                "98805a55523458c56d59339266bdcecc82370ecd",
                "99719dfc220b145e2aac71d6b3e276731d85be1c",
                "9e4923966754c02b036698e95f95cec8fc40a9d2",
                "a3ba598bee9da287092f4f2f3864322af38e1824",
                "a4e42b6cf98e957684aa4e7006940d31bcb76b1f",
                "a7b4becc2304da63792eb6c33f95677b2e7c9f8c",
                "a94df01f21d870a006748b6ab3c04d31428c959d",
                "aa7ffc7999a1b1bb79ce19b61c37f70331f492d6",
                "aeb03edad3edc7c54a3c5f7916ecba981e65ce91",
                "b0bce9873042aee29cbc7ec395647f6cc7a482f8",
                "b48d0aeb94cd3766f23d2ac098bbccf01485dc20",
                "b61fbd992a13af05feba939f597b5f6ee61188e3",
            ]
            .iter()
            .map(|id| Id::from_str(id).unwrap())
            .collect();

            let target = local_id;
            let closest = table.closest(&target);

            let mut closest_ids: Vec<Id> = closest.iter().map(|n| n.id).collect();
            closest_ids.sort();

            assert_eq!(closest_ids, expected_closest_ids);
        }

        {
            let expected_closest_ids: Vec<_> = [
                "c03d9008add37f8414cb41549448bb2dcb5c6c9b",
                "c9a8163fa3e85065d46567bfac39b5452cfb3ae8",
                "cdc7f4d5825dc316de20d998bc0f1c5e91e36a5e",
                "cea92f6e6612ef408d8c22ad5c1ed602bb2aedbf",
                "d65e378a1ec70cc79ae5b4469ae7f0e8939033fe",
                "d9b50c6ca730c89f8fc9f518136cef6139dd2252",
                "dbed34a2c8db568fe59c10adcca9e81825b3dcfd",
                "dff82b028a6ec033e00b387df8e386417b92a47c",
                "e0296cfc4726d91a1f7f041e24638a1276a08bed",
                "e2ec0c07e15411564292b5fa75246e4c385f4411",
                "e63b72f95aacee40ad087f83afb475645739f669",
                "e6b8d5567bc05d9b68f23d562645bc030729abc9",
                "e7c796aeecd47cfd01a2d62fd3fb1d41aafa2464",
                "e962e3a1946afa0d3ee97f3a0418cb3489a5f84c",
                "edec09cc7476cd019560874def4af852bfeaffe3",
                "ef79f77e9eed9ad51094ce2747e2c4fdc3a81326",
                "fa2b38321419e63cb890f8a8b5c53a1c4728a10a",
                "fb449c17f6c34fadea26a5a83e1952e815e001ea",
                "fb689ce0e18c2c22f316976d3ae524aed4137773",
                "fd042ff1404b495720ad8345404ff5f25acd02a8",
            ]
            .iter()
            .map(|str| Id::from_str(str).unwrap())
            .collect();

            let target = Id::from_str("d1406a3d3a8354d566f21dba8bd06c537cde2a20").unwrap();
            let closest = table.closest(&target);

            let mut closest_ids: Vec<Id> = closest.iter().map(|n| n.id).collect();
            closest_ids.sort();

            assert_eq!(closest_ids, expected_closest_ids);
        }
    }
}

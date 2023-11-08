//! Simplified Kademlia routing table

use std::collections::btree_map::Values;
use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::iter::Take;
use std::slice::Iter;
use std::time::Instant;

use crate::common::{Id, Node, MAX_DISTANCE};

/// K = the default maximum size of a k-bucket.
const MAX_BUCKET_SIZE_K: usize = 20;
/// The age of a node's last_seen time before it is considered stale and removed from a full bucket
/// on inserting a new node.
// const

#[derive(Debug)]
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

        self.buckets.entry(distance).or_insert(KBucket::new());

        let bucket = self.buckets.get_mut(&distance).unwrap();

        bucket.add(node)
    }

    pub fn remove(&mut self, node_id: &Id) {
        let distance = self.id.distance(node_id);

        if let Some(bucket) = self.buckets.get_mut(&distance) {
            bucket.remove(node_id)
        }
    }

    pub fn closest(&self, target: &Id) -> Vec<Node> {
        let mut result = Vec::with_capacity(20);
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
                            result.push(node.clone());
                        } else {
                            return result;
                        }
                    }
                }
                None => continue,
            }
        }

        result
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.values().all(|bucket| bucket.is_empty())
    }

    pub fn contains(&self, node_id: &Id) -> bool {
        let distance = self.id.distance(node_id);

        if let Some(bucket) = self.buckets.get(&distance) {
            if bucket.contains(node_id) {
                return true;
            }
        }
        false
    }

    /// Returns all nodes in the routing_table.
    pub fn to_vec(&self) -> Vec<&Node> {
        let mut nodes: Vec<&Node> = vec![];

        for bucket in self.buckets.values() {
            for node in &bucket.nodes {
                nodes.push(node);
            }
        }

        nodes
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Kbuckets are similar to LRU caches that checks and evicts unresponsive nodes,
/// without dropping any responsive nodes in the process.
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

    pub fn add(&mut self, node: Node) -> bool {
        if self.contains(&node.id) {
            return false;
        }

        if self.nodes.len() < MAX_BUCKET_SIZE_K {
            self.nodes.push(node);
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

    pub fn contains(&self, id: &Id) -> bool {
        self.iter().any(|node| node.id == *id)
    }

    pub fn iter(&self) -> Iter<'_, Node> {
        self.nodes.iter()
    }
}

impl Debug for KBucket {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Bucket{{\n");
        for node in &self.nodes {
            write!(f, "  {:?}\n", node);
        }
        write!(f, "}}")
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;
    use std::net::SocketAddr;

    use crate::{
        common::{Id, Node},
        routing_table::{KBucket, RoutingTable},
    };

    #[test]
    fn table_is_empty() {
        let mut table = RoutingTable::new();
        assert_eq!(table.is_empty(), true);

        table.add(Node::random());
        assert_eq!(table.is_empty(), false);
    }

    #[test]
    fn to_vec() {
        let mut table = RoutingTable::new();

        let mut expected_nodes: Vec<Node> = vec![];

        for i in 0..20 {
            expected_nodes.push(Node::random());
        }

        for node in &expected_nodes {
            table.add(node.clone());
        }

        assert_eq!(table.to_vec().sort(), expected_nodes.sort());
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

        assert_eq!(table.add(node), false);
        assert_eq!(table.is_empty(), true)
    }

    #[test]
    fn should_not_add_more_than_k() {
        let id = Id::random();
        let mut table = RoutingTable::new().with_id(id);

        for i in 0..20 {
            let mut cloned = id.to_vec();
            cloned[19] = i;
            let mutated = Id::from_bytes(cloned).unwrap();

            let node = Node::new(mutated, SocketAddr::from(([0, 0, 0, 0], 0)));
            assert!(table.add(node));
        }

        let mut cloned = id.to_vec();
        cloned[19] = 21;
        let mutated = Id::from_bytes(cloned).unwrap();
        let node = Node::new(mutated, SocketAddr::from(([0, 0, 0, 0], 0)));

        assert!(!table.add(node));
    }

    #[test]
    fn closest() {
        let ids = [
            [
                201, 70, 67, 200, 246, 70, 16, 38, 0, 236, 216, 87, 116, 239, 39, 11, 106, 128,
                101, 90,
            ],
            [
                243, 1, 91, 100, 255, 108, 49, 49, 122, 242, 133, 118, 68, 21, 0, 6, 27, 26, 54, 19,
            ],
            [
                49, 77, 141, 49, 37, 34, 125, 4, 3, 202, 105, 227, 99, 63, 59, 214, 51, 193, 67,
                223,
            ],
            [
                168, 225, 156, 203, 79, 65, 215, 251, 172, 96, 48, 59, 2, 23, 218, 12, 76, 66, 222,
                176,
            ],
            [
                170, 204, 0, 215, 170, 174, 200, 71, 83, 77, 217, 165, 87, 247, 199, 109, 47, 44,
                102, 73,
            ],
            [
                235, 153, 116, 42, 159, 145, 132, 237, 232, 111, 29, 215, 6, 188, 13, 238, 39, 87,
                250, 75,
            ],
            [
                35, 72, 247, 238, 239, 1, 98, 211, 202, 100, 95, 234, 37, 22, 229, 154, 115, 4,
                189, 33,
            ],
            [
                171, 47, 22, 53, 59, 161, 19, 123, 115, 94, 180, 242, 191, 147, 74, 18, 63, 198,
                228, 51,
            ],
            [
                190, 73, 17, 132, 55, 150, 216, 209, 198, 6, 156, 204, 66, 98, 128, 118, 131, 108,
                137, 45,
            ],
            [
                105, 242, 97, 157, 47, 111, 153, 190, 40, 170, 104, 88, 80, 148, 169, 254, 124, 81,
                136, 124,
            ],
            [
                71, 91, 187, 15, 18, 155, 221, 117, 140, 228, 72, 121, 179, 211, 229, 249, 138,
                244, 66, 3,
            ],
            [
                60, 52, 23, 181, 245, 29, 59, 34, 85, 129, 28, 217, 154, 41, 106, 111, 180, 62,
                223, 198,
            ],
            [
                145, 29, 158, 179, 233, 233, 100, 59, 36, 191, 43, 114, 13, 241, 21, 164, 120, 217,
                5, 93,
            ],
            [
                120, 43, 115, 248, 107, 101, 184, 136, 150, 254, 252, 187, 202, 56, 156, 136, 246,
                197, 26, 22,
            ],
            [
                160, 217, 94, 238, 82, 89, 185, 22, 184, 114, 159, 76, 156, 94, 203, 150, 143, 164,
                83, 238,
            ],
            [
                33, 104, 9, 5, 146, 30, 62, 218, 18, 118, 218, 53, 54, 162, 110, 123, 143, 189,
                208, 171,
            ],
            [
                179, 233, 172, 40, 206, 103, 33, 113, 105, 67, 62, 14, 146, 254, 141, 233, 166,
                159, 179, 181,
            ],
            [
                146, 50, 26, 210, 146, 252, 40, 161, 7, 136, 223, 152, 138, 95, 64, 71, 216, 238,
                108, 119,
            ],
            [
                97, 132, 193, 109, 91, 137, 139, 151, 224, 190, 136, 186, 156, 245, 74, 217, 105,
                105, 112, 167,
            ],
            [
                89, 245, 92, 6, 188, 249, 5, 85, 55, 210, 210, 101, 172, 154, 230, 87, 142, 157,
                145, 190,
            ],
            [
                128, 54, 141, 150, 217, 125, 36, 35, 108, 93, 72, 99, 141, 100, 96, 7, 5, 146, 230,
                25,
            ],
            [
                191, 166, 185, 78, 244, 210, 69, 210, 140, 76, 223, 60, 27, 91, 149, 211, 61, 130,
                158, 194,
            ],
            [
                211, 152, 2, 51, 215, 119, 115, 11, 164, 64, 114, 99, 102, 255, 186, 151, 240, 244,
                68, 36,
            ],
            [
                228, 181, 41, 209, 107, 220, 177, 243, 147, 100, 67, 252, 163, 98, 128, 10, 248,
                11, 38, 115,
            ],
            [
                60, 55, 207, 21, 8, 172, 206, 21, 170, 148, 32, 225, 214, 141, 74, 141, 84, 35,
                186, 161,
            ],
        ];

        let nodes: Vec<Node> = ids
            .iter()
            .map(|id| Node::random().with_id(Id::from_bytes(id.to_owned()).unwrap()))
            .collect();

        let expected_closest_ids = [
            [
                201, 70, 67, 200, 246, 70, 16, 38, 0, 236, 216, 87, 116, 239, 39, 11, 106, 128,
                101, 90,
            ],
            [
                243, 1, 91, 100, 255, 108, 49, 49, 122, 242, 133, 118, 68, 21, 0, 6, 27, 26, 54, 19,
            ],
            [
                235, 153, 116, 42, 159, 145, 132, 237, 232, 111, 29, 215, 6, 188, 13, 238, 39, 87,
                250, 75,
            ],
            [
                211, 152, 2, 51, 215, 119, 115, 11, 164, 64, 114, 99, 102, 255, 186, 151, 240, 244,
                68, 36,
            ],
            [
                228, 181, 41, 209, 107, 220, 177, 243, 147, 100, 67, 252, 163, 98, 128, 10, 248,
                11, 38, 115,
            ],
            [
                49, 77, 141, 49, 37, 34, 125, 4, 3, 202, 105, 227, 99, 63, 59, 214, 51, 193, 67,
                223,
            ],
            [
                35, 72, 247, 238, 239, 1, 98, 211, 202, 100, 95, 234, 37, 22, 229, 154, 115, 4,
                189, 33,
            ],
            [
                105, 242, 97, 157, 47, 111, 153, 190, 40, 170, 104, 88, 80, 148, 169, 254, 124, 81,
                136, 124,
            ],
            [
                71, 91, 187, 15, 18, 155, 221, 117, 140, 228, 72, 121, 179, 211, 229, 249, 138,
                244, 66, 3,
            ],
            [
                60, 52, 23, 181, 245, 29, 59, 34, 85, 129, 28, 217, 154, 41, 106, 111, 180, 62,
                223, 198,
            ],
            [
                120, 43, 115, 248, 107, 101, 184, 136, 150, 254, 252, 187, 202, 56, 156, 136, 246,
                197, 26, 22,
            ],
            [
                33, 104, 9, 5, 146, 30, 62, 218, 18, 118, 218, 53, 54, 162, 110, 123, 143, 189,
                208, 171,
            ],
            [
                97, 132, 193, 109, 91, 137, 139, 151, 224, 190, 136, 186, 156, 245, 74, 217, 105,
                105, 112, 167,
            ],
            [
                89, 245, 92, 6, 188, 249, 5, 85, 55, 210, 210, 101, 172, 154, 230, 87, 142, 157,
                145, 190,
            ],
            [
                60, 55, 207, 21, 8, 172, 206, 21, 170, 148, 32, 225, 214, 141, 74, 141, 84, 35,
                186, 161,
            ],
            [
                145, 29, 158, 179, 233, 233, 100, 59, 36, 191, 43, 114, 13, 241, 21, 164, 120, 217,
                5, 93,
            ],
            [
                146, 50, 26, 210, 146, 252, 40, 161, 7, 136, 223, 152, 138, 95, 64, 71, 216, 238,
                108, 119,
            ],
            [
                128, 54, 141, 150, 217, 125, 36, 35, 108, 93, 72, 99, 141, 100, 96, 7, 5, 146, 230,
                25,
            ],
            [
                190, 73, 17, 132, 55, 150, 216, 209, 198, 6, 156, 204, 66, 98, 128, 118, 131, 108,
                137, 45,
            ],
            [
                179, 233, 172, 40, 206, 103, 33, 113, 105, 67, 62, 14, 146, 254, 141, 233, 166,
                159, 179, 181,
            ],
        ]
        .iter()
        .map(|u| Id::from_bytes(*u).unwrap())
        .collect::<Vec<Id>>();

        let local_id: Id = "aefb7fac689c1122107dfcde08f6fa2ec4cfec66"
            .try_into()
            .unwrap();

        let target: Id = "d1406a3d3a8354d566f21dba8bd06c537cde2a20"
            .try_into()
            .unwrap();

        let mut table = RoutingTable::new().with_id(local_id);

        for node in nodes {
            table.add(node);
        }

        let closest = table.closest(&target);

        let closest_ids: Vec<Id> = closest.iter().map(|n| n.id).collect();

        assert_eq!(closest_ids, expected_closest_ids);
    }
}

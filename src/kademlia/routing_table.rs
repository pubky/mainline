//! Kademlia routing table based on the simplifications described [here](https://web.archive.org/web/20191122230423/https://github.com/ethereum/wiki/wiki/Kademlia-Peer-Selection)

use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::net::IpAddr;

use super::{
    id::{Id, ID_LENGTH, MAX_DISTANCE},
    node::Node,
};
use crate::Result;

/// The capacity of each row in the routing table.
const K: usize = 20;

#[derive(Debug)]
pub struct RoutingTable {
    rows: BTreeMap<u8, Row>,
    id: Id,
}

struct Row {
    nodes: Vec<Node>,
}

impl Row {
    fn new() -> Self {
        Row {
            nodes: Vec::with_capacity(K),
        }
    }

    fn add(&mut self, node: Node) -> bool {
        if self.nodes.len() < K {
            // TODO: revisit the ordering of this row!
            let index = match self.nodes.binary_search_by(|a| a.id.cmp(&node.id)) {
                Ok(existing_index) => existing_index,
                Err(insertion_index) => insertion_index,
            };

            self.nodes.insert(index, node);
            true
        } else {
            false
        }
    }
}

impl Debug for Row {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Row{{ nodes: {} }}", &self.nodes.len())
    }
}

impl RoutingTable {
    pub fn new() -> Self {
        let mut rows = BTreeMap::new();

        RoutingTable {
            rows,
            id: Id::random(),
        }
    }

    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn add(&mut self, node: Node) -> bool {
        // TODO: Verify the type of ip v4 or v6?

        let distance = self.id.distance(&node.id);

        let row = self.rows.get_mut(&distance);

        if row.is_none() {
            let mut row = Row::new();
            self.rows.insert(distance, row);
        }

        let row = self.rows.get_mut(&distance).unwrap();

        row.add(node)
    }

    pub fn closest(&self, target: &Id) -> Vec<&Node> {
        let mut result = Vec::with_capacity(K);
        let distance = self.id.distance(target);

        for i in
            // First search in closest nodes
            (distance..=MAX_DISTANCE)
                // if we don't have enough close nodes, populate from other rows
                .chain((0..distance).rev())
        {
            match self.rows.get(&i) {
                Some(row) => {
                    for node in row.nodes.iter() {
                        if result.len() < K {
                            result.push(node)
                        } else {
                            return result;
                        }
                    }
                }
                None => continue,
            }
        }

        return result;
    }
}

mod test {
    use std::mem;

    use crate::kademlia::{
        id::Id,
        routing_table::{K, MAX_DISTANCE},
    };

    use super::{Node, RoutingTable};

    #[test]
    fn distance_to_self() {
        let id = Id::random();
        let distance = id.distance(&id);
        assert_eq!(distance, 0)
    }

    #[test]
    fn distance_to_id() {
        let id = Id([
            6, 57, 161, 226, 79, 187, 138, 178, 119, 223, 3, 52, 118, 171, 13, 225, 15, 171, 59,
            220,
        ]);
        let target = Id([
            3, 91, 26, 235, 151, 55, 173, 225, 168, 9, 51, 89, 79, 64, 93, 63, 119, 42, 160, 142,
        ]);

        let distance = id.distance(&target);

        assert_eq!(distance, 155)
    }

    #[test]
    fn distance_to_random_id() {
        let id = Id::random();
        let target = Id::random();

        let distance = id.distance(&target);

        assert_ne!(distance, 0)
    }

    #[test]
    fn distance_to_furthest() {
        let id = Id::random();

        let mut opposite = [0_u8; 20];
        for (i, &value) in id.0.iter().enumerate() {
            opposite[i] = value ^ 0xff;
        }
        let target = Id(opposite);

        let distance = id.distance(&target);

        assert_eq!(distance, MAX_DISTANCE)
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
            .map(|id| Node {
                id: Id(id.to_owned()),
                ip: "0.0.0.0".parse().unwrap(),
                port: 0,
            })
            .collect();

        let mut expected_closest_ids = [
            [
                201, 70, 67, 200, 246, 70, 16, 38, 0, 236, 216, 87, 116, 239, 39, 11, 106, 128,
                101, 90,
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
                235, 153, 116, 42, 159, 145, 132, 237, 232, 111, 29, 215, 6, 188, 13, 238, 39, 87,
                250, 75,
            ],
            [
                243, 1, 91, 100, 255, 108, 49, 49, 122, 242, 133, 118, 68, 21, 0, 6, 27, 26, 54, 19,
            ],
            [
                33, 104, 9, 5, 146, 30, 62, 218, 18, 118, 218, 53, 54, 162, 110, 123, 143, 189,
                208, 171,
            ],
            [
                35, 72, 247, 238, 239, 1, 98, 211, 202, 100, 95, 234, 37, 22, 229, 154, 115, 4,
                189, 33,
            ],
            [
                49, 77, 141, 49, 37, 34, 125, 4, 3, 202, 105, 227, 99, 63, 59, 214, 51, 193, 67,
                223,
            ],
            [
                60, 52, 23, 181, 245, 29, 59, 34, 85, 129, 28, 217, 154, 41, 106, 111, 180, 62,
                223, 198,
            ],
            [
                60, 55, 207, 21, 8, 172, 206, 21, 170, 148, 32, 225, 214, 141, 74, 141, 84, 35,
                186, 161,
            ],
            [
                71, 91, 187, 15, 18, 155, 221, 117, 140, 228, 72, 121, 179, 211, 229, 249, 138,
                244, 66, 3,
            ],
            [
                89, 245, 92, 6, 188, 249, 5, 85, 55, 210, 210, 101, 172, 154, 230, 87, 142, 157,
                145, 190,
            ],
            [
                97, 132, 193, 109, 91, 137, 139, 151, 224, 190, 136, 186, 156, 245, 74, 217, 105,
                105, 112, 167,
            ],
            [
                105, 242, 97, 157, 47, 111, 153, 190, 40, 170, 104, 88, 80, 148, 169, 254, 124, 81,
                136, 124,
            ],
            [
                120, 43, 115, 248, 107, 101, 184, 136, 150, 254, 252, 187, 202, 56, 156, 136, 246,
                197, 26, 22,
            ],
            [
                128, 54, 141, 150, 217, 125, 36, 35, 108, 93, 72, 99, 141, 100, 96, 7, 5, 146, 230,
                25,
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
                179, 233, 172, 40, 206, 103, 33, 113, 105, 67, 62, 14, 146, 254, 141, 233, 166,
                159, 179, 181,
            ],
            [
                190, 73, 17, 132, 55, 150, 216, 209, 198, 6, 156, 204, 66, 98, 128, 118, 131, 108,
                137, 45,
            ],
        ]
        .iter()
        .map(|u| Id(*u))
        .collect::<Vec<Id>>();

        let local_id = Id([
            174, 251, 127, 172, 104, 156, 17, 34, 16, 125, 252, 222, 8, 246, 250, 46, 196, 207,
            236, 102,
        ]);

        let target = Id([
            209, 64, 106, 61, 58, 131, 84, 213, 102, 242, 29, 186, 139, 208, 108, 83, 124, 222, 42,
            32,
        ]);

        let mut table = RoutingTable::new().with_id(local_id);

        for node in nodes {
            table.add(node);
        }

        let closest = table.closest(&target);

        let mut closest_ids: Vec<Id> = closest.iter().map(|n| n.id).collect();

        assert_eq!(closest_ids, expected_closest_ids)
    }
}

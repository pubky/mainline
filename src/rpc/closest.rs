use std::time::{Duration, Instant};

use crate::Node;

use super::{query::Query, server::ROTATE_INTERVAL};

/// Similar to the token rotation interval described in BEP_0005
const CLOSEST_NODES_EXPIRY_DURATION: Duration = ROTATE_INTERVAL;

#[derive(Debug)]
pub struct ClosestNodes {
    pub(crate) nodes: Vec<Node>,
    last_seen: Instant,
}

impl ClosestNodes {
    pub fn expired(&self) -> bool {
        Instant::now().duration_since(self.last_seen) > CLOSEST_NODES_EXPIRY_DURATION
    }
}

impl From<&Query> for ClosestNodes {
    fn from(query: &Query) -> Self {
        ClosestNodes {
            nodes: query.closest(),
            last_seen: Instant::now(),
        }
    }
}

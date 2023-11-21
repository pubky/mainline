//! A struct to iterate on incoming responses for a query.
use flume::{Receiver, Sender};
use std::net::SocketAddr;

use super::{Id, MutableItem, Node};

#[derive(Debug)]
pub struct Response<T> {
    receiver: Receiver<ResponseMessage<T>>,
    pub closest_nodes: Vec<Node>,
    pub visited: usize,
}

impl<T> Response<T> {
    pub fn new(receiver: Receiver<ResponseMessage<T>>) -> Self {
        Self {
            receiver,
            visited: 0,
            closest_nodes: Vec::new(),
        }
    }
}

impl<T> Response<T> {
    /// Next item, async.
    ///
    /// We do not implement futures::stream::Stream to avoid the dependency,
    /// and to avoid having to deal with lifetime and pinning issues.
    #[cfg(feature = "async")]
    pub async fn next_async(&mut self) -> Option<T> {
        match self.receiver.recv_async().await {
            Ok(item) => match item {
                ResponseMessage::ResponseValue(value) => Some(value),
                ResponseMessage::ResponseDone(ResponseDone {
                    visited,
                    closest_nodes,
                }) => {
                    self.visited = visited;
                    self.closest_nodes = closest_nodes;

                    None
                }
            },
            _ => None,
        }
    }
}

impl<T> Iterator for &mut Response<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.receiver.recv() {
            Ok(item) => match item {
                ResponseMessage::ResponseValue(value) => Some(value),
                ResponseMessage::ResponseDone(ResponseDone {
                    visited,
                    closest_nodes,
                }) => {
                    self.visited = visited;
                    self.closest_nodes = closest_nodes;

                    None
                }
            },
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum ResponseSender {
    GetPeer(Sender<ResponseMessage<GetPeerResponse>>),
    GetImmutable(Sender<ResponseMessage<GetImmutableResponse>>),
    GetMutable(Sender<ResponseMessage<GetMutableResponse>>),

    StoreItem(Sender<StoreQueryMetdata>),
}

pub enum ResponseMessage<T> {
    ResponseValue(T),
    ResponseDone(ResponseDone),
}

#[derive(Clone, Debug)]
pub enum ResponseValue {
    Peer(GetPeerResponse),
    Immutable(GetImmutableResponse),
    Mutable(GetMutableResponse),
}

#[derive(Clone, Debug)]
pub struct GetPeerResponse {
    pub from: Node,
    pub peer: SocketAddr,
}

#[derive(Clone, Debug)]
pub struct GetImmutableResponse {
    pub from: Node,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct GetMutableResponse {
    pub from: Node,
    // TODO: detail this
    pub item: MutableItem,
}

#[derive(Clone, Debug)]
pub struct ResponseDone {
    /// Number of nodes visited.
    pub visited: usize,
    /// Closest nodes in the routing tablue.
    pub closest_nodes: Vec<Node>,
}

#[derive(Clone, Debug)]
pub struct StoreQueryMetdata {
    target: Id,
    stored_at: Vec<Id>,
    closest_nodes: Vec<Node>,
}

impl StoreQueryMetdata {
    pub fn new(target: Id, closest_nodes: Vec<Node>, stored_at: Vec<Id>) -> Self {
        Self {
            target,
            closest_nodes,
            stored_at,
        }
    }

    /// Return the target (or info_hash) for this query.
    pub fn target(&self) -> Id {
        self.target
    }

    /// Return the set of nodes that confirmed storing the value.
    pub fn stored_at(&self) -> Vec<&Node> {
        self.closest_nodes
            .iter()
            .filter(|node| self.stored_at.contains(&node.id))
            .collect()
    }

    /// Return closest nodes. Useful to repeat the store operation without repeating the lookup.
    pub fn closest_nodes(&self) -> Vec<Node> {
        self.closest_nodes.clone()
    }
}

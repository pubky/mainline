use std::{
    net::{IpAddr, SocketAddr},
    sync::mpsc::{Receiver, Sender},
};

use super::{Id, Node};

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

#[derive(Debug)]
pub enum ResponseSender {
    GetPeer(Sender<ResponseMessage<GetPeerResponse>>),
    StoreItem(Sender<StoreResponse>),
}

#[derive(Clone, Debug)]
pub enum ResponseValue {
    GetPeer(GetPeerResponse),
}

#[derive(Clone, Debug)]
pub struct GetPeerResponse {
    pub from: Node,
    pub peer: SocketAddr,
}

#[derive(Clone, Debug)]
pub struct StoreResponse {
    stored_at: Vec<Id>,
    closest_nodes: Vec<Node>,
}

impl StoreResponse {
    pub fn new(closest_nodes: Vec<Node>, stored_at: Vec<Id>) -> Self {
        Self {
            closest_nodes,
            stored_at,
        }
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

pub enum ResponseMessage<T> {
    ResponseValue(T),
    ResponseDone(ResponseDone),
}

#[derive(Clone, Debug)]
pub struct ResponseDone {
    /// Number of nodes visited.
    pub visited: usize,
    /// Closest nodes in the routing tablue.
    pub closest_nodes: Vec<Node>,
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

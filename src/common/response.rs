use std::{
    net::SocketAddr,
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
    AnnouncePeer(Sender<AnnouncePeerResponse>),
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
pub struct AnnouncePeerResponse {
    /// Nodes that responded with success.
    pub success: Vec<Id>,
    /// Ids of the nodes that successfully stored the peer.
    pub closest_nodes: Vec<Node>,
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

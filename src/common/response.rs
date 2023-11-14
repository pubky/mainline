use std::{
    net::SocketAddr,
    sync::mpsc::{Receiver, Sender},
};

use super::{Id, Node};

pub struct Response<T> {
    receiver: Receiver<ResponseMessage<T>>,
}

impl<T> Response<T> {
    pub fn new(receiver: Receiver<ResponseMessage<T>>) -> Self {
        Self { receiver }
    }
}

#[derive(Debug)]
pub enum ResponseSender {
    Peer(Sender<ResponseMessage<GetPeerResponse>>),
}

#[derive(Clone, Debug)]
pub enum ResponseValue {
    Peer(GetPeerResponse),
}

#[derive(Clone, Debug)]
pub struct GetPeerResponse {
    pub from: ResponseFrom,
    pub peer: SocketAddr,
}

#[derive(Clone, Debug)]
pub struct ResponseFrom {
    pub id: Id,
    pub address: SocketAddr,
    /// Mainline implementation version, useful for debugging.
    pub version: Option<Vec<u8>>,
    /// Opaque token used to authunticate our IP when we try to PUT a value in that node.
    pub token: Vec<u8>,
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

impl<T> Iterator for Response<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.receiver.recv() {
            Ok(item) => match item {
                ResponseMessage::ResponseValue(value) => Some(value),
                ResponseMessage::ResponseDone(ResponseDone {
                    visited,
                    closest_nodes,
                }) => {
                    // Do stuff;
                    None
                }
            },
            _ => None,
        }
    }
}

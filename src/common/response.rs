use bytes::Bytes;
use flume::Sender;
use std::net::SocketAddr;

use crate::{Id, Result};

use super::MutableItem;

#[derive(Clone, Debug)]
pub enum Response {
    Peer(SocketAddr),
    Immutable(Bytes),
    Mutable(MutableItem),
}

#[derive(Clone, Debug)]
pub enum ResponseSender {
    Peer(Sender<SocketAddr>),
    Mutable(Sender<MutableItem>),
    Immutable(Sender<Bytes>),
}

/// Returns the info_hash or target of the operation.
/// Useful for put_immutable.
pub type PutResult = Result<Id>;

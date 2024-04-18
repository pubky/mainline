use bytes::Bytes;
use flume::Sender;
use std::net::SocketAddr;

use crate::{Id, Result};

use super::MutableItem;

#[derive(Clone, Debug)]
pub enum Response {
    Peers(Vec<SocketAddr>),
    Immutable(Bytes),
    Mutable(MutableItem),
}

#[derive(Clone, Debug)]
pub enum ResponseSender {
    Peers(Sender<Vec<SocketAddr>>),
    Mutable(Sender<MutableItem>),
    Immutable(Sender<Bytes>),
}

/// Returns the info_hash or target of the operation.
/// Useful for put_immutable.
pub type PutResult = Result<Id>;

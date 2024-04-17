//! Main Crate Error

use crate::{common::messages::ErrorSpecific, dht::ActorMessage};

#[derive(thiserror::Error, Debug)]
/// Mainline crate error enum.
pub enum Error {
    /// For starter, to remove as code matures.
    #[error("Generic error: {0}")]
    Generic(String),
    /// For starter, to remove as code matures.
    #[error("Static error: {0}")]
    Static(&'static str),

    #[error(transparent)]
    /// Transparent [std::io::Error]
    IO(#[from] std::io::Error),

    // Id
    /// Id is expected to by 20 bytes.
    #[error("Invalid Id size, expected 20, got {0}")]
    InvalidIdSize(usize),

    /// hex encoding issue
    #[error("Invalid Id encoding: {0}")]
    InvalidIdEncoding(String),

    // DHT messages
    /// Errors related to parsing DHT messages.
    #[error("Failed to parse packet bytes: {0}")]
    BencodeError(#[from] serde_bencode::Error),

    /// Indicates that the message transaction_id is not two bytes.
    #[error("Invalid transaction_id: {0:?}")]
    InvalidTransactionId(Vec<u8>),

    #[error(transparent)]
    /// Transparent [flume::RecvError]
    Receive(#[from] flume::RecvError),

    #[error(transparent)]
    /// The dht was shutdown.
    DhtIsShutdown(#[from] flume::SendError<ActorMessage>),

    #[error("Invalid mutable item signature")]
    InvalidMutableSignature,

    #[error("Invalid mutable item public key")]
    InvalidMutablePublicKey,

    /// Failed to find any nodes close, usually means dht node failed to bootstrap,
    /// so the routing table is empty. Check the machine's access to UDP socket,
    /// or find better bootstrapping nodes.
    #[error("Failed to find any nodes close to store value at")]
    NoClosestNodes,

    #[error("Query Error")]
    QueryError(ErrorSpecific),
}

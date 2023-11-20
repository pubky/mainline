//! Main Crate Error

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
    /// Transparent [std::io::Error]
    Receive(#[from] flume::RecvError),
}

//! Main Crate Error

#[derive(thiserror::Error, Debug)]
/// Pkarr crate error enum.
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

    #[error("Failed to parse packet bytes: {0}")]
    BencodeError(#[from] serde_bencode::Error),

    /// Indicates that the Message type you're trying to build requires more information.
    #[error("{0} is required")]
    BuilderMissingFieldError(&'static str),

    /// Indicates that the builder is in an invalid/ambiguous state to build the desired
    /// Message type.
    #[error("Builder state invalid: {0}")]
    BuilderInvalidComboError(&'static str),

    /// Indicates that the message transaction_id is not two bytes.
    #[error("Invalid transaction_id: {0:?}")]
    InvalidTransactionId(Vec<u8>),
}

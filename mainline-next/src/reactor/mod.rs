//! Library-owned thread and IPv4 UDP socket lifecycle.
//!
//! A reactor starts fully initialized or returns an I/O error. Explicit
//! shutdown affects every handle and waits for the worker. Dropping the final
//! handle requests shutdown without blocking the dropping thread.

mod handle;
mod worker;

pub use handle::{ReactorHandle, ShutdownError};

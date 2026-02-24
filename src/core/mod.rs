//! Core DHT logic - pure computation with no direct I/O.
//!
//! Contains query drivers (`iterative_query`, `put_query`), the `server` request
//! handler, and stateful helpers (`statistics`, `routing_maintenance`).
//! All I/O orchestration lives in `actor/`, which calls into this module.

pub(crate) mod iterative_query;
pub(crate) mod put_query;
pub(crate) mod routing_maintenance;
pub(crate) mod server;
pub(crate) mod statistics;

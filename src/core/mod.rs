//! Core DHT logic extracted from the actor layer.
//!
//! Contains query drivers (`iterative_query`, `put_query`), the `server` request
//! handler, and stateful helpers (`statistics`, `routing_maintenance`) that are
//! free of direct socket I/O.

pub(crate) mod iterative_query;
pub(crate) mod put_query;
pub(crate) mod routing_maintenance;
pub(crate) mod server;
pub(crate) mod statistics;

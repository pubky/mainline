# Changelog

All notable changes to mainline dht will be documented in this file.

##  [3.0.0](https://github.com/pubky/mainline/compare/3a4c3312410e69201a287e40cb7b6dbb30c663f2..v3.0.0) - 2024-09-27

### Added

- Export `errors` module containing `PutError` as a part of the response of `Rpc::put`.
- `Dht::id()` and `AsyncDht::id()` to get the node's Id.
- `Dht::find_node()` and `AsyncDht::find_node()` to lookup a certain target, without calling `get_peers` and the closest responding nodes.
- `Dht::info()` and `AsyncDht::info()` some internal information about the node from one method.
- `Info::dht_size_estimate` to get the ongoing dht size estimate resulting from watching results of all queries.
- `measure_dht` example to estimate the DHT size.

### Changed

- Removed all internal panic `#![deny(clippy::unwrap_used)]`.
- `Testnet::new(size)` returns a `Result<Testnet>`.
- `Dht::local_addr()` and `AsyncDht::local_addr()` replaced with `::info()`.
- `Dht::shutdown()` and `AsyncDht::shutdown()` are now idempotent, and returns `()`.
- `Rpc::drop` uses `tracing::debug!()` to log dropping the Rpc.
- `Id::as_bytes()` instead of exposing internal `bytes` property.
- Replace crate `Error` with more granular errors.
- Replace Flume's `RecvError` with `expect()` message, since the sender should never be dropped to soon.
- `DhtWasShutdown` error is a standalone error.
- `InvalidIdSize` error is a standalone error.
- Rename `DhtSettings` to `Settings`
- Rename `DhtServer` to `DefaultServer`
- `Dht::get_immutable()` and `AsyncDht::get_immutable()` return `Result<Option<bytes::Bytes>, DhtWasShutdown>`
- `Node` fields are now all private, with `id()` and `address()` getters.
- Changed `Settings` to be a the Builder, and make fields private.

### Removed

- Removed `mainline::error::Error` and `mainline::error::Result`.

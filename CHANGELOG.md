# Changelog

All notable changes to mainline dht will be documented in this file.

## [Unreleased]

### Added

- `DhtBuilder` wrapper around `Config`, instead of `Settings` doubling as a builder.

### Changed

- Rename `Settings` to `Config`

##  [4.2.0](https://github.com/pubky/mainline/compare/v4.1.0...v4.2.0) - 2024-12-13

### Added

- Make MutableItem de/serializable (mikedilger)

##  [4.1.0](https://github.com/pubky/mainline/compare/v3.0.0...v4.1.0) - 2024-11-29

### Added

- Export `errors` module containing `PutError` as a part of the response of `Rpc::put`.
- `Dht::find_node()` and `AsyncDht::find_node()` to find the closest nodes to a certain target.
- `Dht::info()` and `AsyncDht::info()` some internal information about the node from one method.
- `Info::dht_size_estimate` to get the ongoing dht size estimate resulting from watching results of all queries.
- `Info::id` to get the Id of the node.
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
- Replaced `Rpc::new()` with `Settings::build_rpc()`.
- Update the client version from `RS01` to `RS04`

### Removed

- Removed `mainline::error::Error` and `mainline::error::Result`.

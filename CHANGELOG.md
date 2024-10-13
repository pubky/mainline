# Changelog

All notable changes to mainline dht will be documented in this file.

##  [3.0.0](https://github.com/pubky/mainline/compare/3a4c3312410e69201a287e40cb7b6dbb30c663f2..v3.0.0) - 2024-09-27

### Added

- Export `errors` module containing `PutError` as a part of the response of `Rpc::put`.

### Changed

- Removed all internal panic `#![deny(clippy::unwrap_used)]`.
- `Testnet::new(size)` returns a `Result<Testnet>`.
- `Dht::local_addr()` returns a `Result<SocketAddr>`.
- `AsyncDht::local_addr()` returns a `Result<SocketAddr>`.
- `Dht::shutdown()` is now idempotent, and returns `()`.
- `AsyncDht::shutdown()` is now idempotent, and returns `()`.
- `Rpc::drop` uses `tracing::debug!()` to log dropping the Rpc.
- `Id::as_bytes()` instead of exposing internal `bytes` property.
- Replace crate `Error` with more granular errors.
- Replace Flume's `RecvError` with `expect()` message, since the sender should never be dropped to soon.
- `DhtWasShutdown` error is a standalone error.
- `InvalidIdSize` error is a standalone error.

### Removed

- Removed `mainline::error::Error` and `mainline::error::Result`.

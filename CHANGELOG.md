# Changelog

All notable changes to mainline dht will be documented in this file.

## [Unreleased]

### Changed

- Removed all internal panic `#![deny(clippy::unwrap_used)]`
- `Testnet::new(size)` returns a `Result<Testnet>`.
- `Dht::local_addr()` returns a `Result<SocketAddr>`.
 `AsyncDht::local_addr()` returns a `Result<SocketAddr>`.

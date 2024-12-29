# Changelog

All notable changes to mainline dht will be documented in this file.

## [Unreleased]

### Added

- `DhtBuilder` wrapper around `Config`, instead of `Settings` doubling as a builder.
- Support `BEP0042 DHT Security extension` when running in server mode. 
- Add `Config::public_ip` for manually setting the node's public ip to generate secure node `Id` from.
- Add `Config::server_mode` to force server mode.
- Add [adaptive mode](https://github.com/pubky/mainline?tab=readme-ov-file#adaptive-mode).
- Export `RoutingTable`.
- Add `DhtBuilder::extra_bootstrap()` to add more bootstrapping nodes from previous sessions.
- Add `Dht::bootstrapped` and `AsyncDht::bootstrapped` to wait for the routing table to be bootstrapped.
- Add `RoutingTable::to_bootstrap()`, `Dht::to_bootstrap()`, and `AsyncDht::to_bootstrap()` to export the addresses nodes in the routing table.
- Add `Rpc::public_address()` and `Info::public_address()` which returns the best estimate for this node's public address.
- Add `Rpc::firewalled()` and `Info::firewalled()` which returns whether or not this node is firewalled, or publicly accessible.
- Add `Rpc::server_mode()` and `Info::server_mode()` which returns whether or not this node is running in server mode.
- Add `Rpc::info()` to export a thread safe and lightweight summary of the node's information and statistics.
- Add `cache_bootstrap.rs` example to show how you can store your routing table to disk and use it for subsequent bootstrapping.
- Add `Id::from_ipv4()`.
- Add `Id::is_valid_for_ipv4`.
- Add `RoutingTable::nodes()` iterator.

### Removed

- Remove `bytes` dependency.
- Remove `ipv6` optionality and commit to `ipv4`.
- Remove `Id::to_vec()`.

### Changed

- `Dht` is now behind a feature flag `node`, so you can include the `Rpc` only and build your own node.
- Rename `Settings` to `Config`.
- `Rpc::new()` takes `Config` as input.
- `Server::handle_request()` signature change, to avoid circular dependency on `Rpc`.
- Make `DefaultServer` properties public.
- `Rpc::id()` returns `Id` instead of `&Id`.
- `Rpc::get`, and `Rpc::put` don't take a `target` argument, as it is included in the request arguments.
- `Rpc::get()`, and `Rpc::put()` don't take a sender any more.
- `RpcTickReport` returned from `Rpc::tick()` is changed, `RpcTickReport::received_from` is removed, and `RpcTickReport::done_find_node_queries`, 
  and `RpcTickReport::qurey_response` are added.
- `Info::local_addr()` is infallible.
- `MutableItem::seq()` returns `i64` instead of a refernece.
- `Dht::put_immutable()` and `AsyncDh::put_immutable()` take `Box<[u8]>` instead of `bytes::Bytes` 
- `Dht::get_immutable()` and `AsyncDh::get_immutable()` return boxed slice `Box<[u8]>` instead of `bytes::Bytes` 

### Removed

- Exported `ClosestNodes`, you have to use it from `mainline::rpc`.

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

# Changelog

All notable changes to mainline dht will be documented in this file.

## [Unreleased]

### Added

- `DhtBuilder` wrapper around `Config`.
- Support `BEP0042 DHT Security extension` when running in server mode. 
- Add `Config::public_ip` for manually setting the node's public ip to generate secure node `Id` from.
- Add `Config::server_mode` to force server mode.
- Add [adaptive mode](https://github.com/pubky/mainline?tab=readme-ov-file#adaptive-mode).
- Add `DhtBuilder::extra_bootstrap()` to add more bootstrapping nodes from previous sessions.
- Add `Dht::bootstrapped()` and `AsyncDht::bootstrapped()` to wait for the routing table to be bootstrapped.
- Add `RoutingTable::to_bootstrap()`, `Dht::to_bootstrap()`, and `AsyncDht::to_bootstrap()` to export the addresses nodes in the routing table.
- Add `Rpc::public_address()` and `Info::public_address()` which returns the best estimate for this node's public address.
- Add `Rpc::firewalled()` and `Info::firewalled()` which returns whether or not this node is firewalled, or publicly accessible.
- Add `Rpc::server_mode()` and `Info::server_mode()` which returns whether or not this node is running in server mode.
- Add `Rpc::info()` to export a thread safe and lightweight summary of the node's information and statistics.
- Add `cache_bootstrap.rs` example to show how you can store your routing table to disk and use it for subsequent bootstrapping.
- Add `Id::from_ipv4()`.
- Add `Id::is_valid_for_ipv4`.
- Add `RoutingTable::nodes()` iterator.
- Add `Dht::get_mutable_most_recent()` and `AsyncDht::get_mutable_most_recent()` to get the most recent mutable item from the network.
- Add `PutQueryError::Timeout` in case put query is terminated unsuccessfully, but no error responses.
- Add `PutMutableError::Concurrrency(ConcurrrencyError)` for all cases where a `Lost Update Problem` may occur (read `Dht::put_mutable` documentation for more details).
- Export `Dhtbuilder`.
- Export `RoutingTable`.

### Removed

- Remove `bytes` dependency.
- Remove `ipv6` optionality and commit to `ipv4`.
- Remove `Id::to_vec()`.
- Exported `ClosestNodes`, you have to use it from `mainline::rpc`.
- Removed `Node::unique()`, `Node::with_id()`, `Node::with_address()`, and `Node::with_token()`.
- Removed `RoutingTable::default()`.

### Changed

- Rename `Settings` to `Config`.
- `Dht` is now behind a feature flag `node`, so you can include the `Rpc` only and build your own node.
- Rename `Settings` to `Config`.
- `Rpc::new()` takes `Config` as input.
- `Rpc::id()` returns `Id` instead of `&Id`.
- `Rpc::get`, and `Rpc::put` don't take a `target` argument, as it is included in the request arguments.
- `Rpc::get()`, and `Rpc::put()` don't take a sender any more.
- `RpcTickReport` returned from `Rpc::tick()` is changed, `RpcTickReport::received_from` is removed, and `RpcTickReport::done_find_node_queries`, 
- Enable calling `Rpc::put()` multiple times concurrently except for put mutable that may return `PutMutableError::Concurrrency(ConcurrrencyError)`.
  and `RpcTickReport::qurey_response` are added.
- `Info::local_addr()` is infallible.
- `MutableItem::seq()` returns `i64` instead of a refernece.
- `Dht::put_immutable()` and `AsyncDh::put_immutable()` take `&[u8]` instead of `bytes::Bytes`.
- `Dht::get_immutable()` and `AsyncDh::get_immutable()` return boxed slice `Box<[u8]>` instead of `bytes::Bytes`.
- `Dht::put_immutable()` and `AsyncDh::put_immutable()` return `PutImmutableError`.
- `Dht::announce_peer()` and `AsyncDh::announce_peer()` return `AnnouncePeerError`.
- `Dht::put_mutable()` and `AsyncDh::put_mutable()` return `PutMutableError`.
- All tracing logs are either `TRACE` (for krpcsocket), `DEBUG`, or `INFO` only for rare and singular events, 
  like starting the node, updating the node Id, or switching to server mode (from adaptive mode).
- Change `PutError` to contain transparent elements for generic `PutQueryError`, and more specialized `ConcurrrencyError`.
- Remove `MutableItem::cas` field, and add optional `CAS` parameter to `Dht::put_mutable` and `AsyncDht::put_mutable`.
- `Dht::find_node()` and `AsyncDht::find_node()` return `Box<[Node]>` instead of `Vec<Node>`.
- `Node` is `Send` and `Sync`, and cheap to clone using an internal `Arc`.
- `Node::new()` take `Id` and `SocketAddrV4`.
- `RoutingTable::new()` takes an `Id`.
- Return `GetIterator<T>` and `GetStream<T>` from `get_` methods from `Dht` and `AsyncDht` instead of exposing `flume`.
- `Server::handle_request()` signature change, to avoid circular dependency on `Rpc`.
- Make `DefaultServer` properties public.
- Trait `Server` needs to implement `Clone`, but no longer needs to implement `Sync`.

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

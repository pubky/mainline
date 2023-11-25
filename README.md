# Mainline

Simple, robust, BitTorrent's [Mainline](https://en.wikipedia.org/wiki/Mainline_DHT) DHT implementation.

This library is focused on being the best and simplest Rust client for Mainline, especially focused on reliable and fast time-to-first-response.

It should work as a routing / storing node as well, but if you want to run a reliable node to support the network, you might be better off running [libtorrent](https://libtorrent.org/) for now.

## Get started

Check the [Examples](./examples).

## Features

### Client

Running as a client, means you can store and query for values on the DHT, but not accept any incoming requests.

```rust
use mainline::Dht;

let dht = Dht::client(); // or Dht::default();
```

Supported BEPs:
- [x] [BEP0005 DHT Protocol](https://www.bittorrent.org/beps/bep_0005.html)
- [X] [BEP0042 DHT Security extension](https://www.bittorrent.org/beps/bep_0042.html)
- [x] [BEP0043 Read-only DHT Nodes](https://www.bittorrent.org/beps/bep_0043.html)
- [x] [BEP0044 Storing arbitrary data in the DHT](https://www.bittorrent.org/beps/bep_0044.html)

### Server

Running as a server is the same as a client, but you also respond to incoming requests and serve as a routing and storing node, supporting the general routing of the DHT, and contributing to the storage capacity of the DHT.

```rust
use mainline::Dht;

let dht = Dht::server(); // or Dht::builder::as_server().build();
```

Supported BEPs:
- [x] [BEP0005 DHT Protocol](https://www.bittorrent.org/beps/bep_0005.html)
- [ ] [BEP0042 DHT Security extension](https://www.bittorrent.org/beps/bep_0042.html)
- [x] [BEP0043 Read-only DHT Nodes](https://www.bittorrent.org/beps/bep_0043.html)
- [ ] [BEP0044 Storing arbitrary data in the DHT](https://www.bittorrent.org/beps/bep_0044.html)


## Acknowledgment

This implementation was possible thanks to [Webtorrent's Bittorrent-dht](https://github.com/webtorrent/bittorrent-dht) as a reference, and [Rustydht-lib](https://github.com/raptorswing/rustydht-lib) that saved me a lot of time, especially at the serialization and deserialization of Bencode messages.

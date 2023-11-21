# Mainline

Simple, robust, BitTorrent's [Mainline](https://en.wikipedia.org/wiki/Mainline_DHT) DHT implementation.

This library is focused on being the best and simplest Rust client for Mainline, especially focused on reliable and fast time-to-first-response.

It should work as a routing / storing node as well, but if you want to run a reliable node to support the network, you might be better off running [libtorrent](https://libtorrent.org/) for now.

## Get started

Check the [Examples](./examples).

## Features

### Client

Supported BEPs:
- [x] [BEP0005 DHT Protocol](https://www.bittorrent.org/beps/bep_0005.html)
- [ ] [BEP0042 DHT Security extension](https://www.bittorrent.org/beps/bep_0042.html)
- [x] [BEP0043 Read-only DHT Nodes](https://www.bittorrent.org/beps/bep_0043.html)
- [x] [BEP0044 Storing arbitrary data in the DHT](https://www.bittorrent.org/beps/bep_0044.html)

### Server

Supported BEPs:
- [x] [BEP0005 DHT Protocol](https://www.bittorrent.org/beps/bep_0005.html)
- [ ] [BEP0042 DHT Security extension](https://www.bittorrent.org/beps/bep_0042.html)
- [x] [BEP0043 Read-only DHT Nodes](https://www.bittorrent.org/beps/bep_0043.html)
- [ ] [BEP0044 Storing arbitrary data in the DHT](https://www.bittorrent.org/beps/bep_0044.html)

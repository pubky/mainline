# Examples

## Announce peer

```sh
cargo run --example announce_peer <40 bytes hex info_hash>
```

## Get peers

```sh
cargo run --example get_peers <40 bytes hex info_hash>
```

## Put Immutable

```sh
cargo run --example put_immutable <string>
```

## Get Immutable

```sh
cargo run --example get_immutable <40 bytes hex target from put_immutable>
```

## Put Mutable

```sh
cargo run --example put_mutable <64 bytes hex secret_key> <string>
```

## Get Mutable

```sh
cargo run --example get_mutable <40 bytes hex target from put_mutable>
```

## Request Filter

Example showing how to implement a custom request filter for the DHT server:

```sh
cargo run --example request_filter
```

## Cache Bootstrap

Example demonstrating how to cache and reuse bootstrapping nodes:

```sh
cargo run --example cache_bootstrap
```

## Count IPs Close to Key

Count all IP addresses around a random target ID and analyze hit rates:

```sh
cargo run --example count_ips_close_to_key
```

## Mark Recapture DHT

Estimate DHT size using Mark-Recapture method:

```sh
cargo run --example mark_recapture_dht
```

## Measure DHT

Measure the size of the DHT network:

```sh
cargo run --example measure_dht
```

## Bootstrap

Basic example of bootstrapping a DHT node:

```sh
cargo run --example bootstrap
```

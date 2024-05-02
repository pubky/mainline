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

## Custom Server

```sh
cargo run --example custom_server
````

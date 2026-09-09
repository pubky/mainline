# Mainline Next

Unpublished workspace crate for the Mainline DHT refactor, developed alongside
the existing `mainline` crate. Requires Rust 1.85 or newer. Currently only the
crate setup is implemented; there is no runtime or public API yet.

## Development

Run from the repository root:

```sh
cargo check -p mainline-next
cargo test -p mainline-next
cargo +1.85.0 check -p mainline-next
cargo fmt --all -- --check
```

CI checks, builds, tests, lints, and documents both workspace crates.

## Design

This proposal separates low-level DHT event streams from high-level estimates
and conclusions derived from those streams. The roadmap describes the staged
implementation.

- [Problem statement](docs/problem-statement.md)
- [Design principles](docs/design-principles.md)
- [Roadmap](docs/roadmap.md)
- [Wishlist](docs/wishlist.md)
- [Implementation strategy](docs/implementation-strategy.md)

## Mutable API Sketches

- [Low-level mutable API](docs/ll-mutable-api.rs)
- [High-level mutable API](docs/hl-mutable-api.rs)

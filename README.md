# Tsonic Rust Runtime

Core Rust runtime crate for Tsonic-emitted Rust.

The crate has one feature-layered source tree:

- `--no-default-features` proves the `core` foundation;
- `--no-default-features --features alloc` enables allocator-backed carriers;
- the default `std` feature enables hosted execution support such as
  `block_on`.

`alloc` and `std` are additive Cargo features; no alternate runtime source
tree or compatibility implementation exists.

The npm artifact `@tsonic/rust-runtime` owns the canonical shared Rust runtime
source tree. Installed Rust targets reference the crate directly from
`crates/tsonic_rust_runtime`; target packages do not copy this source.

## Crate

- Package/crate: `tsonic_rust_runtime`

# Tsonic Rust Runtime

Core Rust runtime crate for Tsonic-emitted Rust.

The crate has one feature-layered source tree:

- `--no-default-features` proves the `core` foundation;
- `--no-default-features --features alloc` enables allocator-backed carriers;
- the default `std` feature enables hosted execution support such as
  `block_on`.

`alloc` and `std` are additive Cargo features; no alternate runtime source
tree or compatibility implementation exists.

The allocator-backed layer includes `TsValue`, an opaque passive carrier for
target-proven closed TypeScript values. It supports retention and forwarding
only; it does not expose runtime type discovery, dynamic member access, or
arbitrary-object projection.

The npm artifact `@tsonic/rust-runtime` owns the canonical shared Rust runtime
source tree. Installed Rust targets reference the crate directly from
`crates/tsonic_rust_runtime`; target packages do not copy this source.

## Crate

- Package/crate: `tsonic_rust_runtime`

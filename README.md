# `@tsonic/rust-runtime`

Base Rust runtime substrate for Tsonic-generated Rust. One canonical crate,
`tsonic_rust_runtime`, is feature-layered for the `core`, `alloc`, and `std`
foundations and owns closed carriers needed independently of JS and Node.

Canonical product documentation:

- [Rust projects and foundations](https://github.com/tsoniclang/tsonic/blob/main/docs/manual/targets/rust/projects-and-output.md)
- [Rust type mapping](https://github.com/tsoniclang/tsonic/blob/main/docs/reference/targets/rust/type-mapping.md)
- [Provider and runtime ownership](https://github.com/tsoniclang/tsonic/blob/main/docs/architecture/provider-and-runtime-ownership.md)

## Development

```sh
npm test
```

The bounded gate proves the crate with `core`, `alloc`, and default `std`
feature selections. The npm artifact owns `crates/tsonic_rust_runtime`; target
packages reference it directly.

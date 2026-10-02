# `@tsonic/rust-runtime`

Base Rust runtime substrate for Tsonic-generated Rust. One canonical crate,
`tsonic_rust_runtime`, is feature-layered for the `core`, `alloc`, and `std`
foundations and owns closed carriers needed independently of JS and Node.

`TsValue` retains native scalar widths and owned strings directly. Primitive
comparisons borrow their buffers and integer values; admission never rounds
integers through floating point. Shared objects retain their existing owner and
identity without allocating another wrapper. Opaque owned payloads use closed
storage, not reflection. Source null and undefined use one native absence state.

Open indexed records use `Record<Key, Value>`, a native `HashMap` with shared
reference identity. Reference copies preserve aliased mutation without copying
the table; lookups accept borrowed native keys. Missing optional values produce
native absence, while missing required values fail like native dictionary
indexing. Enumeration uses native hash-table order, not emulated JavaScript
property order. Finite named records remain generated native record types.

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

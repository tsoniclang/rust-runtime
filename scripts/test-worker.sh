#!/usr/bin/env bash
set -euo pipefail
cargo check --locked -p tsonic_rust_runtime --no-default-features
cargo check --locked -p tsonic_rust_runtime --no-default-features --features alloc
cargo test --locked --workspace --all-features "$@"

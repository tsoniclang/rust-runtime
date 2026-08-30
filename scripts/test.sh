#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"

cargo check -p tsonic_rust_runtime --no-default-features
cargo check -p tsonic_rust_runtime --no-default-features --features alloc
cargo test --workspace --all-features

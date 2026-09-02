#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"

cargo check --locked -p tsonic_rust_runtime --no-default-features
cargo check --locked -p tsonic_rust_runtime --no-default-features --features alloc
cargo test --locked --workspace --all-features

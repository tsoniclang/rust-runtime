#!/usr/bin/env bash
set -euo pipefail
cargo check --locked -p tsonic_rust_runtime --no-default-features
cargo check --locked -p tsonic_rust_runtime --no-default-features --features alloc
exec node "${TSONIC_ROOT:-../tsonic}/scripts/certification/capture-tests.mjs" cargo cargo test --locked --workspace --all-features "$@"

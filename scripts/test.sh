#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
if (( $# == 0 )); then
  exec node "${TSONIC_ROOT:-../tsonic}/scripts/certification/run.mjs" rust-runtime
fi
exec bash "${TSONIC_ROOT:-../tsonic}/test/scripts/bounded-run.sh" native bash scripts/test-worker.sh "$@"

#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
exec bash "${TSONIC_ROOT:-../tsonic}/test/scripts/bounded-run.sh" native bash scripts/test-worker.sh "$@"

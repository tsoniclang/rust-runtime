#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
exec bash "${TSONIC_ROOT:-../tsonic}/test/scripts/bounded-run.sh" rust bash scripts/test-worker.sh "$@"

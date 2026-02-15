#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

echo "[collector-load] running concurrent collector load harness"
cargo test -p tau-training-runner regression_collector_load_harness_reports_metrics_and_no_drop -- --nocapture

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-target-fast-fuzz}"

cd "$ROOT_DIR"

echo "[fuzz-contract] target dir: $TARGET_DIR"
echo "[fuzz-contract] running tau-runtime parser conformance fuzz tests"
CARGO_TARGET_DIR="$TARGET_DIR" cargo test -p tau-runtime spec_c01_rpc_raw_fuzz_conformance_no_panic_for_10000_inputs
CARGO_TARGET_DIR="$TARGET_DIR" cargo test -p tau-runtime spec_c02_rpc_ndjson_fuzz_conformance_no_panic_for_10000_inputs

echo "[fuzz-contract] running tau-gateway websocket conformance fuzz test"
CARGO_TARGET_DIR="$TARGET_DIR" cargo test -p tau-gateway spec_c03_gateway_ws_parse_fuzz_conformance_no_panic_for_10000_inputs

echo "[fuzz-contract] PASS"

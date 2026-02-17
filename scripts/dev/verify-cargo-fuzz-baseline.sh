#!/usr/bin/env bash
set -euo pipefail

# Verifies baseline cargo-fuzz coverage for high-risk untrusted parser surfaces.
#
# Usage:
#   scripts/dev/verify-cargo-fuzz-baseline.sh
#
# Optional:
#   TAU_CARGO_FUZZ_RUNS=200 scripts/dev/verify-cargo-fuzz-baseline.sh
#   CARGO_TARGET_DIR=target-fast-cargo-fuzz scripts/dev/verify-cargo-fuzz-baseline.sh

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fuzz_dir="${root_dir}/fuzz"
target_dir="${CARGO_TARGET_DIR:-target-fast-cargo-fuzz}"
runs="${TAU_CARGO_FUZZ_RUNS:-200}"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/tau-cargo-fuzz.XXXXXX")"
tmp_corpus_root="${tmp_dir}/corpus"

cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is required to run nightly cargo-fuzz targets." >&2
  exit 1
fi

if [ -d "${HOME}/.cargo/bin" ]; then
  export PATH="${HOME}/.cargo/bin:${PATH}"
fi

if ! RUSTUP_TOOLCHAIN=nightly cargo fuzz --help >/dev/null 2>&1; then
  cat >&2 <<'EOF'
cargo-fuzz is not installed for the nightly toolchain.
Install it with:
  RUSTUP_TOOLCHAIN=nightly cargo install cargo-fuzz
EOF
  exit 1
fi

# Run against a temporary corpus copy to avoid writing generated entries to the
# repository's tracked seed corpus directories.
cp -R "${fuzz_dir}/corpus" "${tmp_corpus_root}"

run_fuzz_target() {
  local target="$1"
  local corpus_dir="${tmp_corpus_root}/${target}"
  if [ ! -d "${corpus_dir}" ]; then
    echo "missing corpus directory for target '${target}': ${corpus_dir}" >&2
    exit 1
  fi
  echo "==> RUSTUP_TOOLCHAIN=nightly cargo fuzz run --fuzz-dir ${fuzz_dir} ${target} -- -runs=${runs}"
  CARGO_TARGET_DIR="${target_dir}" RUSTUP_TOOLCHAIN=nightly cargo fuzz run \
    --fuzz-dir "${fuzz_dir}" \
    "${target}" \
    "${corpus_dir}" \
    -- \
    -runs="${runs}"
}

run_fuzz_target "rpc_raw_dispatch"
run_fuzz_target "rpc_ndjson_dispatch"
run_fuzz_target "gateway_ws_parse"

echo "cargo-fuzz baseline verification complete: all targets ran for ${runs} iterations."

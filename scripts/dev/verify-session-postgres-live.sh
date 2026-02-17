#!/usr/bin/env bash
set -euo pipefail

# Verifies tau-session PostgreSQL persistence using an ephemeral Docker-backed
# database and existing integration tests.
#
# Usage:
#   scripts/dev/verify-session-postgres-live.sh
#
# Optional environment overrides:
#   CARGO_TARGET_DIR=target-fast-postgres-live
#   TAU_POSTGRES_IMAGE=postgres:16-alpine
#   TAU_POSTGRES_WAIT_SECONDS=45

target_dir="${CARGO_TARGET_DIR:-target-fast-postgres-live}"
postgres_image="${TAU_POSTGRES_IMAGE:-postgres:16-alpine}"
wait_seconds="${TAU_POSTGRES_WAIT_SECONDS:-30}"
postgres_db="${TAU_POSTGRES_DB:-tau}"
postgres_user="${TAU_POSTGRES_USER:-tau}"
postgres_password="${TAU_POSTGRES_PASSWORD:-tau}"
container_name="tau-postgres-live-verify-${RANDOM}-$$"

require_cmd() {
  local name="$1"
  command -v "${name}" >/dev/null 2>&1 || {
    echo "required command not found: ${name}" >&2
    exit 1
  }
}

cleanup() {
  docker rm -f "${container_name}" >/dev/null 2>&1 || true
}

run_test() {
  local test_name="$1"
  echo "==> cargo test -p tau-session ${test_name}"
  TAU_TEST_POSTGRES_DSN="${session_dsn}" \
    CARGO_TARGET_DIR="${target_dir}" \
    cargo test -p tau-session "${test_name}" -- --nocapture
}

require_cmd docker
require_cmd cargo
docker info >/dev/null 2>&1 || {
  echo "docker daemon is unavailable" >&2
  exit 1
}

trap cleanup EXIT INT TERM

echo "==> docker run ${postgres_image}"
docker run -d --rm \
  --name "${container_name}" \
  -e POSTGRES_DB="${postgres_db}" \
  -e POSTGRES_USER="${postgres_user}" \
  -e POSTGRES_PASSWORD="${postgres_password}" \
  -p 127.0.0.1::5432 \
  "${postgres_image}" >/dev/null

postgres_port="$(docker port "${container_name}" 5432/tcp | awk -F: '{print $NF}' | head -n1)"
if [[ -z "${postgres_port}" ]]; then
  echo "failed to resolve mapped postgres port" >&2
  docker logs "${container_name}" || true
  exit 1
fi

echo "==> waiting for postgres readiness (${wait_seconds}s max)"
ready=false
for _ in $(seq 1 "${wait_seconds}"); do
  if docker exec "${container_name}" pg_isready -U "${postgres_user}" -d "${postgres_db}" >/dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 1
done

if [[ "${ready}" != "true" ]]; then
  echo "postgres did not become ready in ${wait_seconds}s" >&2
  docker logs "${container_name}" || true
  exit 1
fi

session_dsn="postgres://${postgres_user}:${postgres_password}@127.0.0.1:${postgres_port}/${postgres_db}?sslmode=disable"
echo "==> using TAU_TEST_POSTGRES_DSN=postgres://...@127.0.0.1:${postgres_port}/${postgres_db}"

run_test "integration_spec_c02_postgres_round_trip_preserves_lineage_when_dsn_provided"
run_test "integration_spec_c03_postgres_usage_summary_persists_when_dsn_provided"
run_test "integration_spec_c04_postgres_session_paths_are_isolated_when_dsn_provided"

echo "session postgres live verification complete: all mapped tests passed."

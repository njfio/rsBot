#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SCRIPT_UNDER_TEST="${SCRIPT_DIR}/gateway-api-route-inventory.sh"

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}', got '${actual}'" >&2
    exit 1
  fi
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected content to include '${needle}'" >&2
    exit 1
  fi
}

if [[ ! -x "${SCRIPT_UNDER_TEST}" ]]; then
  echo "assertion failed (script exists): missing executable ${SCRIPT_UNDER_TEST}" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-gateway-api-route-inventory.sh" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

router_path="${tmp_dir}/gateway_openresponses.rs"
api_doc_path="${tmp_dir}/gateway-api-reference.md"
output_json="${tmp_dir}/gateway-api-route-inventory.json"
output_md="${tmp_dir}/gateway-api-route-inventory.md"

cat >"${router_path}" <<'RS'
fn router() {
    Router::new()
        .route("/a", get(a))
        .route("/b", post(b))
        .route("/c", get(c).put(c).delete(c));
}
RS

cat >"${api_doc_path}" <<'DOC'
# Gateway API Reference

**Route inventory:** 3 router route calls (`.route(...)` entries in `gateway_openresponses.rs`).

## Endpoint Inventory (4 method-path entries)

| Method | Path | Auth | Policy Gate | Notes |
| --- | --- | --- | --- | --- |
| GET | `/a` | Protected | - | A |
| POST | `/b` | Protected | - | B |
| GET | `/c` | Protected | - | C |
| PUT | `/c` | Protected | - | C |
DOC

stdout_capture="$(${SCRIPT_UNDER_TEST} \
  --router "${router_path}" \
  --api-doc "${api_doc_path}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "2026-02-21T00:00:00Z")"

assert_contains "${stdout_capture}" "ok=true" "stdout ok marker"
assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "json schema version"
assert_equals "2026-02-21T00:00:00Z" "$(jq -r '.generated_at' "${output_json}")" "json generated at"
assert_equals "3" "$(jq -r '.actual_counts.route_calls' "${output_json}")" "json route calls"
assert_equals "4" "$(jq -r '.actual_counts.method_path_rows' "${output_json}")" "json method rows"
assert_equals "true" "$(jq -r '.drift.ok' "${output_json}")" "json drift ok"
assert_contains "$(cat "${output_md}")" "| route_calls | 3 |" "markdown route rows"

cat >"${api_doc_path}" <<'DOC'
# Gateway API Reference

**Route inventory:** 99 router route calls (`.route(...)` entries in `gateway_openresponses.rs`).

## Endpoint Inventory (4 method-path entries)

| Method | Path | Auth | Policy Gate | Notes |
| --- | --- | --- | --- | --- |
| GET | `/a` | Protected | - | A |
| POST | `/b` | Protected | - | B |
| GET | `/c` | Protected | - | C |
| PUT | `/c` | Protected | - | C |
DOC

if ${SCRIPT_UNDER_TEST} --router "${router_path}" --api-doc "${api_doc_path}" --output-json "${output_json}" --output-md "${output_md}" >/tmp/gateway-inventory-out.log 2>/tmp/gateway-inventory-err.log; then
  echo "assertion failed (mismatch should fail): script unexpectedly succeeded" >&2
  exit 1
fi

assert_contains "$(cat /tmp/gateway-inventory-err.log)" "route inventory marker mismatch" "mismatch stderr"

echo "gateway-api-route-inventory tests passed"

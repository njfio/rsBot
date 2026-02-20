#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_SCRIPT="${SCRIPT_DIR}/operator-readiness-live-check.sh"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected output to contain '${needle}'" >&2
    echo "actual output:" >&2
    echo "${haystack}" >&2
    exit 1
  fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

cat > "${tmp_dir}/curl" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail

url=""
for arg in "$@"; do
  if [[ "${arg}" == http* ]]; then
    url="${arg}"
  fi
done

if [[ "${url}" == *"/gateway/status" ]]; then
  gateway_health="${MOCK_GATEWAY_HEALTH:-healthy}"
  gateway_rollout="${MOCK_GATEWAY_ROLLOUT:-pass}"
  gateway_reason="${MOCK_GATEWAY_REASON:-healthy_cycle}"
  cat <<OUT
{"health_state":"${gateway_health}","rollout_gate":"${gateway_rollout}","rollout_reason_code":"${gateway_reason}"}
OUT
  exit 0
fi

if [[ "${url}" == *"/cortex/status" ]]; then
  cortex_health="${MOCK_CORTEX_HEALTH:-healthy}"
  cortex_rollout="${MOCK_CORTEX_ROLLOUT:-pass}"
  cortex_reason="${MOCK_CORTEX_REASON:-cortex_ready}"
  cat <<OUT
{"health_state":"${cortex_health}","rollout_gate":"${cortex_rollout}","reason_code":"${cortex_reason}","event_type_counts":{"cortex.chat.request":1}}
OUT
  exit 0
fi

echo "unexpected url: ${url}" >&2
exit 1
MOCK
chmod +x "${tmp_dir}/curl"

cat > "${tmp_dir}/cargo" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail

operator_health="${MOCK_OPERATOR_HEALTH:-healthy}"
operator_rollout="${MOCK_OPERATOR_ROLLOUT:-pass}"
operator_reason="${MOCK_OPERATOR_REASON:-healthy_cycle}"
cat <<OUT
{"health_state":"${operator_health}","rollout_gate":"${operator_rollout}","reason_codes":["${operator_reason}"]}
OUT
MOCK
chmod +x "${tmp_dir}/cargo"

set +e
missing_token_output="$(PATH="${tmp_dir}:${PATH}" \
  "${CHECK_SCRIPT}" --base-url "http://127.0.0.1:8787" --quiet 2>&1)"
missing_token_code=$?
set -e

if [[ ${missing_token_code} -eq 0 ]]; then
  echo "assertion failed (missing token): expected non-zero exit" >&2
  exit 1
fi
assert_contains "${missing_token_output}" "auth token required" "missing token"

PATH="${tmp_dir}:${PATH}" \
TAU_OPERATOR_AUTH_TOKEN="secret" \
"${CHECK_SCRIPT}" --base-url "http://127.0.0.1:8787" --quiet

set +e
degraded_output="$(PATH="${tmp_dir}:${PATH}" \
  TAU_OPERATOR_AUTH_TOKEN="secret" \
  MOCK_CORTEX_HEALTH="degraded" \
  "${CHECK_SCRIPT}" --base-url "http://127.0.0.1:8787" --quiet 2>&1)"
degraded_code=$?
set -e

if [[ ${degraded_code} -eq 0 ]]; then
  echo "assertion failed (degraded cortex): expected non-zero exit" >&2
  exit 1
fi
assert_contains "${degraded_output}" "cortex status health_state" "degraded cortex health"

set +e
hold_output="$(PATH="${tmp_dir}:${PATH}" \
  TAU_OPERATOR_AUTH_TOKEN="secret" \
  MOCK_OPERATOR_ROLLOUT="hold" \
  MOCK_OPERATOR_REASON="gateway_service_stopped" \
  "${CHECK_SCRIPT}" --base-url "http://127.0.0.1:8787" --quiet 2>&1)"
hold_code=$?
set -e

if [[ ${hold_code} -eq 0 ]]; then
  echo "assertion failed (operator hold): expected non-zero exit" >&2
  exit 1
fi
assert_contains "${hold_output}" "operator control summary rollout_gate='hold'" "operator hold gate"

echo "operator-readiness-live-check tests passed"

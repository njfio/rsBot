#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_SCRIPT="${SCRIPT_DIR}/cortex-readiness-live-check.sh"

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

if [[ "${url}" == *"/cortex/chat" ]]; then
  cat <<'OUT'
event: cortex.response.created
data: {"response_id":"cortex_resp"}

event: cortex.response.output_text.done
data: {"text":"ok"}

event: done
data: [DONE]
OUT
  exit 0
fi

if [[ "${url}" == *"/cortex/status" ]]; then
  status_health="${MOCK_STATUS_HEALTH:-healthy}"
  status_rollout="${MOCK_STATUS_ROLLOUT:-pass}"
  status_reason="${MOCK_STATUS_REASON:-cortex_ready}"
  status_chat_count="${MOCK_STATUS_CHAT_COUNT:-1}"
  cat <<OUT
{"health_state":"${status_health}","rollout_gate":"${status_rollout}","reason_code":"${status_reason}","event_type_counts":{"cortex.chat.request":${status_chat_count}}}
OUT
  exit 0
fi

echo "unexpected url: ${url}" >&2
exit 1
MOCK
chmod +x "${tmp_dir}/curl"

PATH="${tmp_dir}:${PATH}" \
TAU_CORTEX_AUTH_TOKEN="secret" \
"${CHECK_SCRIPT}" --base-url "http://127.0.0.1:8787" --quiet

set +e
failing_output="$(PATH="${tmp_dir}:${PATH}" \
  TAU_CORTEX_AUTH_TOKEN="secret" \
  MOCK_STATUS_HEALTH="degraded" \
  MOCK_STATUS_ROLLOUT="hold" \
  MOCK_STATUS_REASON="cortex_chat_activity_missing" \
  "${CHECK_SCRIPT}" --base-url "http://127.0.0.1:8787" --expect-health-state healthy --quiet 2>&1)"
failing_code=$?
set -e

if [[ ${failing_code} -eq 0 ]]; then
  echo "assertion failed (health mismatch): expected non-zero exit" >&2
  exit 1
fi

assert_contains "${failing_output}" "expected health_state='healthy'" "health mismatch"

echo "cortex-readiness-live-check tests passed"

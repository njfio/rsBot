#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "gateway-auth" "Run deterministic gateway auth posture demo commands without starting external services." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

demo_state_dir=".tau/demo-gateway-auth"

tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_run_step \
  "gateway-remote-profile-token-mode" \
  --gateway-remote-profile-inspect \
  --gateway-remote-profile proxy-remote \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token demo-gateway-token \
  --gateway-state-dir "${demo_state_dir}" \
  --gateway-remote-profile-json

tau_demo_common_run_step \
  "gateway-remote-profile-password-session-mode" \
  --gateway-remote-profile-inspect \
  --gateway-remote-profile password-remote \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode password-session \
  --gateway-openresponses-auth-password demo-gateway-password \
  --gateway-openresponses-session-ttl-seconds 900 \
  --gateway-state-dir "${demo_state_dir}" \
  --gateway-remote-profile-json

tau_demo_common_finish

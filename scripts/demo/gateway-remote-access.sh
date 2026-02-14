#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "gateway-remote-access" "Run deterministic remote-access profile inspect + fail-closed guardrail demo commands." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

demo_state_dir=".tau/demo-gateway-remote-access"
trace_log_path="${TAU_DEMO_REPO_ROOT}/${demo_state_dir}/trace.log"

tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"
mkdir -p "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"
export TAU_DEMO_TRACE_LOG="${trace_log_path}"
: >"${trace_log_path}"

tau_demo_common_run_step \
  "gateway-remote-profile-inspect-local-only" \
  --gateway-remote-profile-inspect \
  --gateway-remote-profile-json \
  --gateway-state-dir "${demo_state_dir}"

tau_demo_common_run_step \
  "gateway-remote-profile-inspect-tailscale-serve-token" \
  --gateway-remote-profile-inspect \
  --gateway-remote-profile tailscale-serve \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token demo-tailscale-token \
  --gateway-remote-profile-json \
  --gateway-state-dir "${demo_state_dir}"

tau_demo_common_run_step \
  "gateway-remote-plan-export-tailscale-serve" \
  --gateway-remote-plan \
  --gateway-remote-plan-json \
  --gateway-remote-profile tailscale-serve \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token demo-tailscale-token \
  --gateway-state-dir "${demo_state_dir}"

tau_demo_common_run_expect_failure \
  "gateway-remote-plan-fails-closed-for-missing-password" \
  1 \
  "tailscale_funnel_missing_password" \
  --gateway-remote-plan \
  --gateway-remote-profile tailscale-funnel \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:8787 \
  --gateway-openresponses-auth-mode password-session \
  --gateway-state-dir "${demo_state_dir}"

tau_demo_common_finish

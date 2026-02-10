#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "multi-channel" "Run deterministic multi-channel runtime and health-inspection demo commands against checked-in fixtures." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

fixture_path="${TAU_DEMO_REPO_ROOT}/crates/tau-multi-channel/testdata/multi-channel-contract/baseline-three-channel.json"
live_fixture_dir="${TAU_DEMO_REPO_ROOT}/crates/tau-multi-channel/testdata/multi-channel-live-ingress"
demo_state_dir=".tau/demo-multi-channel"
live_ingress_dir="${demo_state_dir}/live-ingress"

tau_demo_common_require_file "${fixture_path}"
tau_demo_common_require_file "${live_fixture_dir}/raw/telegram-update.json"
tau_demo_common_require_file "${live_fixture_dir}/raw/discord-message.json"
tau_demo_common_require_file "${live_fixture_dir}/raw/whatsapp-message.json"
tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"
mkdir -p "${TAU_DEMO_REPO_ROOT}/${live_ingress_dir}"

tau_demo_common_run_step \
  "multi-channel-channel-login-telegram" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-live-ingress-dir "${live_ingress_dir}" \
  --multi-channel-channel-login telegram \
  --multi-channel-telegram-bot-token demo-telegram-token \
  --multi-channel-channel-login-json

tau_demo_common_run_step \
  "multi-channel-channel-login-discord" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-live-ingress-dir "${live_ingress_dir}" \
  --multi-channel-channel-login discord \
  --multi-channel-discord-bot-token demo-discord-token \
  --multi-channel-channel-login-json

tau_demo_common_run_step \
  "multi-channel-channel-login-whatsapp" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-live-ingress-dir "${live_ingress_dir}" \
  --multi-channel-channel-login whatsapp \
  --multi-channel-whatsapp-access-token demo-whatsapp-token \
  --multi-channel-whatsapp-phone-number-id 15551230000 \
  --multi-channel-channel-login-json

tau_demo_common_run_step \
  "multi-channel-channel-status-telegram" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-live-ingress-dir "${live_ingress_dir}" \
  --multi-channel-channel-status telegram \
  --multi-channel-telegram-bot-token demo-telegram-token \
  --multi-channel-channel-status-json

tau_demo_common_run_step \
  "multi-channel-channel-probe-whatsapp" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-live-ingress-dir "${live_ingress_dir}" \
  --multi-channel-channel-probe whatsapp \
  --multi-channel-whatsapp-access-token demo-whatsapp-token \
  --multi-channel-whatsapp-phone-number-id 15551230000 \
  --multi-channel-channel-probe-json

tau_demo_common_run_step \
  "multi-channel-channel-logout-discord" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-live-ingress-dir "${live_ingress_dir}" \
  --multi-channel-channel-logout discord \
  --multi-channel-channel-logout-json

tau_demo_common_run_step \
  "multi-channel-runner" \
  --multi-channel-contract-runner \
  --multi-channel-fixture ./crates/tau-multi-channel/testdata/multi-channel-contract/baseline-three-channel.json \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-queue-limit 64 \
  --multi-channel-processed-event-cap 10000 \
  --multi-channel-retry-max-attempts 4 \
  --multi-channel-retry-base-delay-ms 0 \
  --multi-channel-outbound-mode dry-run \
  --multi-channel-outbound-max-chars 512

tau_demo_common_run_step \
  "transport-health-inspect-multi-channel" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --transport-health-inspect multi-channel \
  --transport-health-json

tau_demo_common_run_step \
  "multi-channel-status-inspect" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-status-inspect \
  --multi-channel-status-json

tau_demo_common_run_step \
  "multi-channel-live-ingest-telegram" \
  --multi-channel-live-ingest-file ./crates/tau-multi-channel/testdata/multi-channel-live-ingress/raw/telegram-update.json \
  --multi-channel-live-ingest-transport telegram \
  --multi-channel-live-ingest-provider telegram-bot-api \
  --multi-channel-live-ingest-dir "${live_ingress_dir}"

tau_demo_common_run_step \
  "multi-channel-live-ingest-discord" \
  --multi-channel-live-ingest-file ./crates/tau-multi-channel/testdata/multi-channel-live-ingress/raw/discord-message.json \
  --multi-channel-live-ingest-transport discord \
  --multi-channel-live-ingest-provider discord-gateway \
  --multi-channel-live-ingest-dir "${live_ingress_dir}"

tau_demo_common_run_step \
  "multi-channel-live-ingest-whatsapp" \
  --multi-channel-live-ingest-file ./crates/tau-multi-channel/testdata/multi-channel-live-ingress/raw/whatsapp-message.json \
  --multi-channel-live-ingest-transport whatsapp \
  --multi-channel-live-ingest-provider whatsapp-cloud-api \
  --multi-channel-live-ingest-dir "${live_ingress_dir}"

tau_demo_common_run_step \
  "multi-channel-live-runner" \
  --multi-channel-live-runner \
  --multi-channel-live-ingress-dir "${live_ingress_dir}" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-queue-limit 64 \
  --multi-channel-processed-event-cap 10000 \
  --multi-channel-retry-max-attempts 4 \
  --multi-channel-retry-base-delay-ms 0 \
  --multi-channel-outbound-mode dry-run \
  --multi-channel-outbound-max-chars 512

tau_demo_common_run_step \
  "multi-channel-route-inspect-telegram" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-route-inspect-file ./crates/tau-multi-channel/testdata/multi-channel-live-ingress/telegram-valid.json \
  --multi-channel-route-inspect-json

tau_demo_common_run_step \
  "transport-health-inspect-multi-channel-live" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --transport-health-inspect multi-channel \
  --transport-health-json

tau_demo_common_run_step \
  "multi-channel-status-inspect-live" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-status-inspect \
  --multi-channel-status-json

tau_demo_common_run_step \
  "channel-store-inspect-telegram" \
  --channel-store-root "${demo_state_dir}/channel-store" \
  --channel-store-inspect telegram/chat-100

tau_demo_common_finish

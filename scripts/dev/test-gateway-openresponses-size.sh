#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ROOT_MODULE="${REPO_ROOT}/crates/tau-gateway/src/gateway_openresponses.rs"
EVENTS_MODULE="${REPO_ROOT}/crates/tau-gateway/src/gateway_openresponses/events_status.rs"
MAX_LINES=260

if [[ ! -f "${ROOT_MODULE}" ]]; then
  echo "assertion failed (root module exists): ${ROOT_MODULE}" >&2
  exit 1
fi

if [[ ! -f "${EVENTS_MODULE}" ]]; then
  echo "assertion failed (events module exists): ${EVENTS_MODULE}" >&2
  exit 1
fi

line_count="$(wc -l < "${ROOT_MODULE}")"
if (( line_count > MAX_LINES )); then
  echo "assertion failed (gateway_openresponses.rs size): ${line_count} > ${MAX_LINES}" >&2
  exit 1
fi

if ! rg -q '^mod events_status;' "${ROOT_MODULE}"; then
  echo "assertion failed (events status module wiring): missing 'mod events_status;'" >&2
  exit 1
fi

if ! rg -q '^mod status_runtime;' "${ROOT_MODULE}"; then
  echo "assertion failed (status runtime module wiring): missing 'mod status_runtime;'" >&2
  exit 1
fi

if ! rg -q '^mod compat_state_runtime;' "${ROOT_MODULE}"; then
  echo "assertion failed (compat state runtime module wiring): missing 'mod compat_state_runtime;'" >&2
  exit 1
fi

for type_name in \
  GatewayMultiChannelStatusReport \
  GatewayMultiChannelRuntimeStateFile \
  GatewayMultiChannelConnectorsStateFile; do
  if rg -q "^struct ${type_name}" "${ROOT_MODULE}"; then
    echo "assertion failed (multi-channel types moved): found '${type_name}' in root module" >&2
    exit 1
  fi
done

for type_name in \
  GatewayAuthRuntimeState \
  GatewaySessionTokenState \
  GatewayRateLimitBucket \
  GatewayAuthStatusReport; do
  if rg -q "^struct ${type_name}" "${ROOT_MODULE}"; then
    echo "assertion failed (auth types moved): found '${type_name}' in root module" >&2
    exit 1
  fi
done

if rg -q '^pub trait GatewayToolRegistrar' "${ROOT_MODULE}"; then
  echo "assertion failed (tool registrar api moved): found 'GatewayToolRegistrar' trait in root module" >&2
  exit 1
fi

for type_name in NoopGatewayToolRegistrar GatewayToolRegistrarFn; do
  if rg -q "^pub struct ${type_name}" "${ROOT_MODULE}"; then
    echo "assertion failed (tool registrar api moved): found '${type_name}' in root module" >&2
    exit 1
  fi
done

for const_name in OPENRESPONSES_ENDPOINT GATEWAY_STATUS_ENDPOINT OPS_DASHBOARD_ENDPOINT; do
  if rg -q "^const ${const_name}:" "${ROOT_MODULE}"; then
    echo "assertion failed (endpoint constants moved): found '${const_name}' in root module" >&2
    exit 1
  fi
done

for type_name in GatewayOpenResponsesServerConfig GatewayOpenResponsesServerState; do
  if rg -q "^(pub\\s+)?struct ${type_name}\\b" "${ROOT_MODULE}"; then
    echo "assertion failed (server state types moved): found '${type_name}' in root module" >&2
    exit 1
  fi
done

for function_name in run_gateway_openresponses_server build_gateway_openresponses_router; do
  if rg -q "^(pub\\s+)?(async\\s+)?fn ${function_name}\\b" "${ROOT_MODULE}"; then
    echo "assertion failed (bootstrap/router functions moved): found '${function_name}' in root module" >&2
    exit 1
  fi
done

if rg -q '^macro_rules! define_ops_shell_handler' "${ROOT_MODULE}"; then
  echo "assertion failed (ops shell handlers moved): found 'define_ops_shell_handler' macro in root module" >&2
  exit 1
fi

if rg -q '^define_ops_shell_handler!' "${ROOT_MODULE}"; then
  echo "assertion failed (ops shell handlers moved): found macro invocations in root module" >&2
  exit 1
fi

for function_name in \
  handle_ops_dashboard_agent_detail_shell_page \
  handle_ops_dashboard_session_detail_shell_page; do
  if rg -q "^async fn ${function_name}\\b" "${ROOT_MODULE}"; then
    echo "assertion failed (ops shell handlers moved): found '${function_name}' in root module" >&2
    exit 1
  fi
done

for function_name in \
  handle_webchat_page \
  handle_dashboard_shell_page \
  handle_gateway_auth_bootstrap; do
  if rg -q "^async fn ${function_name}\\b" "${ROOT_MODULE}"; then
    echo "assertion failed (entry handlers moved): found '${function_name}' in root module" >&2
    exit 1
  fi
done

for function_name in \
  authorize_and_enforce_gateway_limits \
  validate_gateway_request_body_size \
  enforce_policy_gate \
  system_time_to_unix_ms; do
  if rg -q "^fn ${function_name}\\b" "${ROOT_MODULE}"; then
    echo "assertion failed (preflight helpers moved): found '${function_name}' in root module" >&2
    exit 1
  fi
done

if rg -q '^fn parse_gateway_json_body<' "${ROOT_MODULE}"; then
  echo "assertion failed (preflight helpers moved): found 'parse_gateway_json_body' in root module" >&2
  exit 1
fi

for function_name in \
  handle_gateway_ws_upgrade \
  run_dashboard_stream_loop; do
  if rg -q "^async fn ${function_name}\\b" "${ROOT_MODULE}"; then
    echo "assertion failed (ws/stream handlers moved): found '${function_name}' in root module" >&2
    exit 1
  fi
done

if rg -q '^async fn stream_openresponses\\b' "${ROOT_MODULE}"; then
  echo "assertion failed (stream handler moved): found 'stream_openresponses' in root module" >&2
  exit 1
fi

if rg -q '^async fn handle_gateway_auth_session\\b' "${ROOT_MODULE}"; then
  echo "assertion failed (auth session handler moved): found 'handle_gateway_auth_session' in root module" >&2
  exit 1
fi

if rg -q '^async fn handle_openresponses\\b' "${ROOT_MODULE}"; then
  echo "assertion failed (openresponses entry handler moved): found 'handle_openresponses' in root module" >&2
  exit 1
fi

if rg -q '^async fn execute_openresponses_request\\b' "${ROOT_MODULE}"; then
  echo "assertion failed (openresponses execution handler moved): found 'execute_openresponses_request' in root module" >&2
  exit 1
fi

echo "gateway-openresponses size guard passed (${line_count} <= ${MAX_LINES})"

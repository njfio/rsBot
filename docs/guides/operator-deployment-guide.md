# Operator Deployment Guide

Run all commands from repository root.

## Purpose

This is the operator entrypoint for deploying and validating Tau gateway/dashboard runtime.
It provides a linear flow for prerequisites, startup, health validation, troubleshooting, and rollback.

Canonical promotion gate procedure:

- `docs/guides/ops-readiness-live-validation.md`

## Prerequisites

Required tools:

- `cargo`
- `curl`
- `jq`

Required credentials/config:

- Provider credential for the selected model path (for local OpenAI model flow: `OPENAI_API_KEY`)
- Gateway auth mode selection (`localhost-dev`, `token`, or `password-session`)
- Writable state directory (default examples use `.tau/gateway`)

Recommended local shell setup:

```bash
export OPENAI_API_KEY="local-dev-placeholder"
export TAU_GATEWAY_PORT="8791"
```

## Auth Modes and Tokens

- `localhost-dev`: local-only bootstrap mode, no bearer required for protected routes.
- `token`: bearer token required on protected routes (`Authorization: Bearer <token>`).
- `password-session`: obtain bearer token via `/gateway/auth/session`, then use bearer on protected routes.

For endpoint-specific details, see `docs/guides/gateway-api-reference.md`.

## Quick Start (Localhost-Dev)

## Step 1: Start gateway in local smoke posture

Use `localhost-dev` auth for local bring-up validation:

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-state-dir .tau/gateway \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:${TAU_GATEWAY_PORT:-8791} \
  --gateway-openresponses-auth-mode localhost-dev \
  --gateway-openresponses-max-input-chars 32000
```

Production-facing token mode baseline (recommended outside local-only environments):

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --gateway-state-dir .tau/gateway \
  --gateway-openresponses-server \
  --gateway-openresponses-bind 127.0.0.1:${TAU_GATEWAY_PORT:-8791} \
  --gateway-openresponses-auth-mode token \
  --gateway-openresponses-auth-token local-dev-token \
  --gateway-openresponses-max-input-chars 32000
```

## Step 2: Verify gateway and cortex health endpoints

`localhost-dev` auth mode (no bearer required):

```bash
curl -sS http://127.0.0.1:${TAU_GATEWAY_PORT:-8791}/gateway/status | jq
curl -sS http://127.0.0.1:${TAU_GATEWAY_PORT:-8791}/cortex/status | jq
```

Expected local smoke posture (fresh state):

- Gateway: `health_state=healthy`, `rollout_gate=pass`
- Cortex: `health_state=failing`, `rollout_gate=hold`, `reason_code=cortex_observer_events_missing`

For token mode, add:

```bash
-H "Authorization: Bearer local-dev-token"
```

## Step 3: Verify dashboard/webchat access

Open the browser endpoint:

- `http://127.0.0.1:${TAU_GATEWAY_PORT:-8791}/webchat`

Auth expectations:

- `localhost-dev`: no token required
- `token`: paste configured bearer token
- `password-session`: issue session token first using `/gateway/auth/session`, then paste returned token

## Step 4: Run fail-closed readiness validation

For sparse local environments, keep gateway strict and override cortex/operator expectations to known
local hold posture:

```bash
scripts/dev/operator-readiness-live-check.sh \
  --base-url http://127.0.0.1:${TAU_GATEWAY_PORT:-8791} \
  --auth-mode none \
  --expect-gateway-health-state healthy \
  --expect-gateway-rollout-gate pass \
  --expect-cortex-health-state failing \
  --expect-cortex-rollout-gate hold \
  --expect-operator-health-state failing \
  --expect-operator-rollout-gate hold
```

If this command returns successfully, local operator validation passes for the expected sparse posture.

## Step 5: Troubleshooting checklist

| Symptom | Check | Command |
|---|---|---|
| Gateway status unavailable | Gateway process not running/bind conflict | `lsof -iTCP:${TAU_GATEWAY_PORT:-8791} -sTCP:LISTEN` |
| Gateway auth failures in token mode | Wrong/missing bearer token | `curl -sS http://127.0.0.1:${TAU_GATEWAY_PORT:-8791}/gateway/status -H "Authorization: Bearer <token>"` |
| Cortex gate held (`cortex_observer_events_missing`) | No cortex chat activity yet | `curl -sS http://127.0.0.1:${TAU_GATEWAY_PORT:-8791}/cortex/status | jq` |
| Readiness script fails operator gate | Operator summary defaults to hold locally | rerun with explicit `--expect-operator-*` overrides above |
| Readiness script fails gateway gate | Inspect reason code and health fields | `curl -sS http://127.0.0.1:${TAU_GATEWAY_PORT:-8791}/gateway/status | jq` |

## Step 6: Rollback and stop procedures

Stop gateway service posture explicitly:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --gateway-service-stop \
  --gateway-service-stop-reason emergency_rollback
```

If rollback is code-related:

```bash
git revert <commit>
```

Then rerun this guide from Step 1 through Step 4 before re-promotion.

## Rollback Procedure

1. Stop gateway service posture:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --gateway-service-stop \
  --gateway-service-stop-reason emergency_rollback
```

2. Revert code if needed:

```bash
git revert <commit>
```

3. Re-run bring-up and readiness checks:

```bash
scripts/dev/operator-readiness-live-check.sh \
  --base-url http://127.0.0.1:${TAU_GATEWAY_PORT:-8791} \
  --auth-mode none \
  --expect-gateway-health-state healthy \
  --expect-gateway-rollout-gate pass
```

## Related Runbooks

- `docs/guides/gateway-ops.md`
- `docs/guides/dashboard-ops.md`
- `docs/guides/deployment-ops.md`
- `docs/guides/ops-readiness-live-validation.md`
- `docs/provider-auth/provider-auth-capability-matrix.md`

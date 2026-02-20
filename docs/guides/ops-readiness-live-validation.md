# Operator Readiness Live Validation

Run all commands from repository root.

## Purpose

This runbook is the canonical P0 go/no-go procedure for runtime promotion. It combines:

- gateway transport health and rollout guardrails
- cortex readiness contract checks
- operator control summary posture checks
- deployment rollout guardrails
- rollback triggers and evidence capture

Use this runbook before production promotion and during incident recovery validation.

## Prerequisites

- Local build is available (`cargo build -p tau-coding-agent` completed once).
- Gateway server is running and reachable at `http://127.0.0.1:8787` (or custom base URL).
- Auth token is available for token-mode gateway auth.
- `jq`, `curl`, and `cargo` are available in `PATH`.

## Step 1: Canonical readiness gate command

```bash
TAU_OPERATOR_AUTH_TOKEN=local-dev-token \
scripts/dev/operator-readiness-live-check.sh \
  --base-url http://127.0.0.1:8787 \
  --expect-rollout-gate pass
```

This command fails closed when:

- `/gateway/status` is not healthy or gate is held
- `/cortex/status` is not healthy or gate is held
- `--operator-control-summary` reports a hold gate or missing reason codes

For sparse local environments where operator summary is expected to hold, override only operator expectations while keeping gateway/cortex strict:

```bash
TAU_OPERATOR_AUTH_TOKEN=local-dev-token \
scripts/dev/operator-readiness-live-check.sh \
  --base-url http://127.0.0.1:8787 \
  --expect-operator-health-state failing \
  --expect-operator-rollout-gate hold
```

## Step 2: Gateway and cortex detail inspection

```bash
curl -sS http://127.0.0.1:8787/gateway/status \
  -H "Authorization: Bearer local-dev-token" | jq

curl -sS http://127.0.0.1:8787/cortex/status \
  -H "Authorization: Bearer local-dev-token" | jq
```

Expected posture:

- `health_state=healthy`
- `rollout_gate=pass`
- non-empty reason code fields (`rollout_reason_code`/`reason_code`)

## Step 3: Operator control-plane posture

```bash
cargo run -p tau-coding-agent -- \
  --operator-control-summary \
  --operator-control-summary-json | jq
```

Expected posture:

- `health_state=healthy`
- `rollout_gate=pass`
- `reason_codes[]` present and non-empty

## Step 4: Deployment guardrail checks

```bash
cargo run -p tau-coding-agent -- \
  --deployment-state-dir .tau/deployment \
  --transport-health-inspect deployment \
  --transport-health-json | jq

cargo run -p tau-coding-agent -- \
  --deployment-state-dir .tau/deployment \
  --deployment-status-inspect \
  --deployment-status-json | jq
```

Expected posture:

- deployment transport health is `healthy`
- deployment status `rollout_gate=pass`

## Step 5: Promotion decision

Promote only when all checks are true:

- gateway readiness command returns `status=pass`
- gateway status gate is `pass`
- cortex status gate is `pass`
- operator control summary gate is `pass`
- deployment status gate is `pass`

If any gate is `hold`, stop promotion and treat as incident triage input.

## Rollback triggers and commands

Trigger rollback when any of the following is true:

- readiness validator fails after remediation retry
- repeated `health_state=failing` on gateway/deployment/operator summary
- cortex readiness reason code indicates missing/malformed observer artifact after repair attempt

Gateway rollback posture:

```bash
cargo run -p tau-coding-agent -- \
  --gateway-state-dir .tau/gateway \
  --gateway-service-stop \
  --gateway-service-stop-reason emergency_rollback
```

Deployment rollback posture:

```bash
git revert <commit>
```

Then re-run this runbook end to end before re-promotion.

## Evidence capture checklist

Store the following artifacts per validation run:

- readiness command output (`status=pass` or error)
- gateway status JSON
- cortex status JSON
- operator control summary JSON
- deployment transport/deployment status JSON

Suggested output path:

- `.tau/reports/operator-readiness/<timestamp>/`

## Related runbooks

- `docs/guides/gateway-ops.md`
- `docs/guides/deployment-ops.md`
- `docs/guides/operator-control-summary.md`
- `docs/guides/dashboard-ops.md`

## Ownership

Primary ownership surfaces:

- `scripts/dev/operator-readiness-live-check.sh`
- `crates/tau-coding-agent`
- `crates/tau-gateway`
- `crates/tau-deployment`
- `docs/guides/runbook-ownership-map.md`

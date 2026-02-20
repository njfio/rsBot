# Deployment Operations Runbook

Run all commands from repository root.

## Scope

This runbook covers the fixture-driven deployment runtime (`--deployment-contract-runner`) for
cloud and WASM rollout validation.

Canonical promotion gate procedure:

- `docs/guides/ops-readiness-live-validation.md`

## Fly.io gateway deployment baseline

Repository default manifest: `fly.toml` (repo root).

1. Set a unique app name in `fly.toml` (`app = "..."`).
2. Bootstrap app metadata from the manifest:

```bash
fly launch --copy-config --no-deploy
```

3. Configure provider credentials as Fly secrets:

```bash
fly secrets set OPENAI_API_KEY=... ANTHROPIC_API_KEY=...
```

4. Deploy the gateway service:

```bash
fly deploy
```

5. Verify service status and health:

```bash
fly status
fly logs
curl -sS https://<your-app-name>.fly.dev/gateway/status
```

Fly manifest contract defaults in this repository:
- Runs the existing `Dockerfile` image build.
- Forces gateway transport mode via `TAU_TRANSPORT_MODE=gateway`.
- Enables OpenResponses HTTP server on Fly internal port routing
  (`TAU_GATEWAY_OPENRESPONSES_BIND=0.0.0.0:8080`).
- Configures an HTTP check on `/gateway/status`.

## Health and observability signals

Primary transport health signal:

```bash
cargo run -p tau-coding-agent -- \
  --deployment-state-dir .tau/deployment \
  --transport-health-inspect deployment \
  --transport-health-json
```

Primary operator status/guardrail signal:

```bash
cargo run -p tau-coding-agent -- \
  --deployment-state-dir .tau/deployment \
  --deployment-status-inspect \
  --deployment-status-json
```

Primary state files:

- `.tau/deployment/state.json`
- `.tau/deployment/runtime-events.jsonl`
- `.tau/deployment/wasm-artifacts/<blueprint_id>/<sha>.manifest.json`
- `.tau/deployment/wasm-artifacts/<blueprint_id>/<sha>.wasm`
- `.tau/deployment/channel-store/deployment/<blueprint_id>/...`

`runtime-events.jsonl` reason codes:

- `healthy_cycle`
- `queue_backpressure_applied`
- `duplicate_cases_skipped`
- `malformed_inputs_observed`
- `retry_attempted`
- `retryable_failures_observed`
- `case_processing_failed`
- `cloud_rollout_applied`
- `wasm_rollout_applied`

Guardrail interpretation:

- `rollout_gate=pass`: health is `healthy`, promotion can continue.
- `rollout_gate=hold`: health is `degraded` or `failing`, pause promotion and investigate.

## WASM deliverable packaging

Package one module into a verifiable WASM artifact + manifest:

```bash
cargo run -p tau-coding-agent -- \
  --deployment-state-dir .tau/deployment \
  --deployment-wasm-package-module ./crates/tau-coding-agent/testdata/deployment-wasm/edge-runtime.wasm \
  --deployment-wasm-package-blueprint-id edge-wasm \
  --deployment-wasm-package-runtime-profile wasm-wasi \
  --deployment-wasm-package-output-dir .tau/deployment/wasm-artifacts \
  --deployment-wasm-package-json
```

Package channel-automation runtime profile artifact:

```bash
cargo run -p tau-coding-agent -- \
  --deployment-state-dir .tau/deployment \
  --deployment-wasm-package-module ./crates/tau-coding-agent/testdata/deployment-wasm/edge-runtime.wasm \
  --deployment-wasm-package-blueprint-id channel-automation-wasm \
  --deployment-wasm-package-runtime-profile channel-automation-wasi \
  --deployment-wasm-package-output-dir .tau/deployment/wasm-artifacts \
  --deployment-wasm-package-json
```

Manifest guarantees:

- deterministic SHA-256 hash (`artifact_sha256`)
- deterministic size (`artifact_size_bytes`)
- runtime profile compatibility (`runtime_profile`)
- control-plane runtime constraint profile (`runtime_constraints.profile_id`)
- import-module observation for ABI/compliance checks (`observed_import_modules`)
- capability constraints (`capability_constraints`)
- deployment state tracking (`state.json` `wasm_deliverables` entry)

Compatibility matrix:

- Supported `deploy_target`: `wasm`
- Supported runtime profile(s): `wasm_wasi`, `channel_automation_wasi`
- Runtime constraint profile (`wasm_wasi`): `control_plane_gateway_v1`
- Runtime constraint profile (`channel_automation_wasi`): `channel_automation_runtime_v1`
- Required WASI ABI family: `wasi:*` (preview2 namespace pattern)
- Forbidden legacy WASI imports: `wasi_snapshot_preview1`, `wasi_unstable`
- Required module format: valid WASM binary magic header (`\\0asm`)
- Unsupported in this path: native/container image packaging, host capability negotiation, and runtime sandbox policy synthesis

## WASM deliverable inspect/report

Inspect an existing manifest/artifact pair and emit compliance status:

```bash
cargo run -p tau-coding-agent -- \
  --deployment-wasm-inspect-manifest .tau/deployment/wasm-artifacts/edge-wasm/<sha>.manifest.json \
  --deployment-wasm-inspect-json
```

Inspect report fields include:

- `constraint_profile_id`
- `constraint_target_role`
- `required_runtime_profile`
- `required_abi`
- `compliant`
- `reason_codes`
- `observed_import_modules`
- `required_feature_gates`
- `max_artifact_size_bytes`

## Browser-native DID bootstrap (KAMN)

Initialize a browser/edge DID payload and persist it to deployment state:

```bash
cargo run -p tau-coding-agent -- \
  --deployment-wasm-browser-did-init \
  --deployment-wasm-browser-did-method key \
  --deployment-wasm-browser-did-network tau-devnet \
  --deployment-wasm-browser-did-subject browser-agent \
  --deployment-wasm-browser-did-entropy local-dev-seed \
  --deployment-wasm-browser-did-output .tau/deployment/browser-did.json \
  --deployment-wasm-browser-did-json
```

Output guarantees:

- deterministic DID derivation for identical `(method, network, subject, entropy)` input
- persisted bootstrap payload at `--deployment-wasm-browser-did-output`
- portability across native and wasm builds through `kamn-core`/`kamn-sdk`

## WASM compile + edge smoke harness

Run the consolidated smoke harness:

```bash
./scripts/dev/wasm-smoke.sh
```

Harness coverage:

- `cargo check --target wasm32-unknown-unknown` for `kamn-core`, `kamn-sdk`, `tau-access`, `tau-deployment`
- Cloudflare probe (`scripts/edge/cloudflare-wasm-smoke.sh`)
- Deno probe (`scripts/edge/deno-wasm-smoke.sh`)

Edge runtime matrix:

- Cloudflare Workers: probe passes when `wrangler` is installed, otherwise explicit skip
- Deno: probe passes when `deno` is installed, otherwise explicit skip

## Deterministic demo path

```bash
./scripts/demo/deployment.sh
```

## Rollout plan with guardrails

1. Validate fixture contract and runtime locally:
   `cargo test -p tau-coding-agent deployment_contract -- --test-threads=1`
2. Validate runtime behavior coverage:
   `cargo test -p tau-coding-agent deployment_runtime -- --test-threads=1`
3. Validate WASM packaging + manifest verification:
   `cargo test -p tau-coding-agent deployment_wasm -- --test-threads=1`
4. Run deterministic demo:
   `./scripts/demo/deployment.sh`
5. Verify transport health and status gate:
   `--transport-health-inspect deployment --transport-health-json`
   `--deployment-status-inspect --deployment-status-json`
6. Promote by increasing fixture complexity gradually while monitoring:
   `failure_streak`, `last_cycle_failed`, `queue_depth`, `rollout_gate`,
   `wasm_rollout_count`, and `cloud_rollout_count`.

## Rollback plan

1. Stop invoking `--deployment-contract-runner`.
2. Preserve `.tau/deployment/` for incident analysis.
3. Revert to last known-good revision:
   `git revert <commit>`
4. Re-run validation matrix before re-enable.

## Troubleshooting

- Symptom: health state `degraded` with `case_processing_failed`.
  Action: inspect `runtime-events.jsonl`, then verify fixture expectations and runtime support.
- Symptom: health state `degraded` with `malformed_inputs_observed`.
  Action: inspect fixture records for invalid deploy target/runtime combinations.
- Symptom: health state `degraded` with `retry_attempted` or `retryable_failures_observed`.
  Action: verify retryable failure simulation and retry policy settings.
- Symptom: health state `failing` (`failure_streak >= 3`).
  Action: treat as rollout gate failure; pause promotion and investigate repeated failures.
- Symptom: `rollout_gate=hold` with stale state.
  Action: run deterministic demo and re-check `deployment-status-inspect` freshness fields.
- Symptom: non-zero `queue_depth`.
  Action: reduce per-cycle fixture volume or increase `--deployment-queue-limit`.

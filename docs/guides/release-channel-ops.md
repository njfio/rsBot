# Release Channel Ops

Run commands from repository root.

## Cross-Surface Rollout Contract

Use this rollout profile for all live surfaces:

- voice
- browser automation
- dashboard
- custom command
- memory

| Phase | Canary % | Minimum Soak | Promotion Gates (all required) |
| --- | --- | --- | --- |
| preflight | 0% | 1 full run | `./scripts/demo/live-run-unified.sh --skip-build --timeout-seconds 180` reports `total=5 passed=5 failed=0`; CI `live-smoke-matrix` is green; rollback target is recorded via `/release-channel show`. |
| canary-1 | 5% | 30 minutes | Every surface is `health_state=healthy`; `failure_streak=0`; `last_cycle_failed=false`; `queue_depth<=1`; no new `case_processing_failed` or `malformed_inputs_observed`. |
| canary-2 | 25% | 60 minutes | canary-1 gates continue to hold; browser timeline has no non-`ok` step and no `error_code`; no rollback trigger fired. |
| canary-3 | 50% | 120 minutes | canary-2 gates continue to hold; retry pressure is stable (`retry_attempted` and `retryable_failures_observed` are flat). |
| general-availability | 100% | 24 hours | canary-3 gates hold for full window; release sign-off checklist is complete with evidence links. |

### Health Threshold Collection Commands

Run these checks at each phase boundary:

```bash
# voice
cargo run -p tau-coding-agent -- \
  --voice-state-dir .tau/voice \
  --voice-status-inspect \
  --voice-status-json

# dashboard
cargo run -p tau-coding-agent -- \
  --dashboard-state-dir .tau/dashboard \
  --dashboard-status-inspect \
  --dashboard-status-json

# custom command
cargo run -p tau-coding-agent -- \
  --custom-command-state-dir .tau/custom-command \
  --custom-command-status-inspect \
  --custom-command-status-json

# memory
cargo run -p tau-coding-agent -- \
  --memory-state-dir .tau/memory \
  --transport-health-inspect memory \
  --transport-health-json

# browser live summary
cat .tau/demo-browser-automation-live/browser-live-summary.json
```

## Rollback Trigger Matrix

| Trigger | Threshold | Immediate Action |
| --- | --- | --- |
| Health regression | Any surface reports `health_state=degraded` or `health_state=failing` for 2 consecutive checks | Freeze promotion at current phase and start rollback workflow. |
| Failure streak breach | Any surface reaches `failure_streak>=3` | Trigger rollback immediately, do not continue canary. |
| Hard runtime errors | New `case_processing_failed` or `malformed_inputs_observed` appears during canary windows | Trigger rollback, capture runtime events and artifacts for incident review. |
| Browser execution failure | Browser timeline step is non-`ok` or has non-empty `error_code` | Trigger rollback and switch to previous known-good browser lane. |
| Validation gate failure | Unified harness or CI `live-smoke-matrix` fails | Trigger rollback and block merge/release sign-off. |

## Rollback Execution Steps

1. Freeze promotion and record incident timestamp.
2. Capture evidence before cleanup:
   - `.tau/live-run-unified/report.json`
   - `.tau/live-run-unified/manifest.json`
   - per-surface logs under `.tau/live-run-unified/surfaces/`
3. Inspect rollback target metadata:

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel show
/quit
EOF
```

4. Apply rollback plan to target version metadata:

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel apply --target <rollback_version>
/quit
EOF
```

5. Revert/redeploy to the last known-good revision:
   `git revert <release_commit_sha>`
6. Re-run unified validation before reopening rollout:
   `./scripts/demo/live-run-unified.sh --skip-build --timeout-seconds 180`

## Inspect channel and rollback metadata

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel show
/quit
EOF
```

The output includes persisted rollback metadata fields:
- `rollback_channel`
- `rollback_version`
- `rollback_reason`
- `rollback_reference_unix_ms`

## Switch channel policy

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel set stable
/release-channel set beta
/release-channel set dev
/quit
EOF
```

Channel changes persist rollback hints automatically.

## Plan an update

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel plan
/release-channel plan --target v0.1.5 --dry-run
/quit
EOF
```

Planning writes `.tau/release-update-state.json` with:
- target version
- guard decision and reason code
- action (`upgrade|noop|blocked`)
- dry-run mode and lookup source

## Apply an update plan

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel apply
/release-channel apply --target v0.1.5 --dry-run
/quit
EOF
```

`apply` enforces fail-closed guards:
- malformed version fields
- prerelease target blocked on `stable`
- unsafe major-version jumps

Current implementation records deterministic apply metadata and rollback hints.
Binary replacement remains an operator-managed step.

## Cache maintenance

```bash
cargo run -p tau-coding-agent -- --model openai/gpt-4o-mini <<'EOF'
/release-channel cache show
/release-channel cache refresh
/release-channel cache prune
/release-channel cache clear
/quit
EOF
```

## Release Sign-Off Checklist Template

Use the checklist guide before any 100% rollout:

- [Release Sign-Off Checklist](release-signoff-checklist.md)

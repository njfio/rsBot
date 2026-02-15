# Prompt Optimization Operations Guide

This guide covers rollout-based prompt optimization mode with durable SQLite state.

This mode executes and evaluates rollouts. It is not a full policy-learning
training pipeline.

Future true RL policy-learning work is tracked separately in
[Epic #1657](https://github.com/njfio/Tau/issues/1657) and
[Milestone #24](https://github.com/njfio/Tau/milestone/24)
(`True RL Wave 2026-Q3: Policy Learning in Production`).
Staged roadmap details: [`docs/planning/true-rl-roadmap-skeleton.md`](../planning/true-rl-roadmap-skeleton.md).

## Run Prompt Optimization Mode

From repository root:

```bash
cargo run -p tau-coding-agent -- \
  --model openai/gpt-4o-mini \
  --prompt-optimization-config .tau/prompt-optimization.json \
  --prompt-optimization-store-sqlite .tau/training/store.sqlite \
  --prompt-optimization-json
```

`--prompt-optimization-config` switches Tau into prompt optimization mode and exits after
completion.

## Prompt Optimization Config JSON

Prompt optimization mode expects a JSON object:

```json
{
  "optimize": [
    { "prompt": "What is 2 + 2?", "expected": "4" }
  ],
  "validate": [
    { "prompt": "What is 3 + 3?", "expected": "6" }
  ],
  "resources": {
    "system_prompt": "You are a concise assistant."
  },
  "worker_count": 2,
  "poll_interval_ms": 50,
  "heartbeat_interval_ms": 1000,
  "completion_poll_interval_ms": 60,
  "completion_timeout_secs": 30
}
```

Fields:

- `optimize`: optimization rollout inputs (array of JSON objects)
- `validate`: validation rollout inputs (array of JSON objects)
- `resources`: optional initial resource snapshot persisted before execution
- `worker_count`: optional runner worker count (`> 0`)
- `poll_interval_ms`: optional rollout dequeue polling interval (`> 0`)
- `heartbeat_interval_ms`: optional worker heartbeat interval (`> 0`)
- `completion_poll_interval_ms`: optional trainer completion poll interval (`> 0`)
- `completion_timeout_secs`: optional trainer timeout (`> 0`)

At least one of `optimize` or `validate` must be non-empty.

Legacy config keys remain accepted for compatibility:

- `train` aliases `optimize`
- `val` aliases `validate`

## SQLite Store

`--prompt-optimization-store-sqlite` controls persistent state location. The store records:

- rollout queue + lifecycle status
- attempts and worker heartbeats
- execution spans
- immutable resource versions

Re-running with the same SQLite path keeps prior state for audit/inspection.

## Dashboard Metrics Export

After each successful prompt optimization run, Tau writes `.tau/training/status.json` next to the
SQLite store. This status file includes model identity and rollout outcome counters
(`total_rollouts`, `succeeded`, `failed`, `cancelled`) for dashboard/gateway status surfaces.

Gateway dashboard endpoints (`/dashboard/health`, `/dashboard/widgets`,
`/dashboard/queue-timeline`, `/dashboard/alerts`) include this status under the `training` field.

Resume/recovery lifecycle controls and guardrail procedures are documented in:
`docs/guides/prompt-optimization-recovery-runbook.md`.

## Flag Notes

Canonical flags for current automation:

- `--prompt-optimization-config`
- `--prompt-optimization-store-sqlite`
- `--prompt-optimization-json`
- `--prompt-optimization-proxy-*`

Compatibility aliases (temporary migration path):

- `--train-config` -> `--prompt-optimization-config`
- `--train-store-sqlite` -> `--prompt-optimization-store-sqlite`
- `--train-json` -> `--prompt-optimization-json`
- `--training-proxy-server` -> `--prompt-optimization-proxy-server`
- `--training-proxy-bind` -> `--prompt-optimization-proxy-bind`
- `--training-proxy-upstream-url` -> `--prompt-optimization-proxy-upstream-url`
- `--training-proxy-state-dir` -> `--prompt-optimization-proxy-state-dir`
- `--training-proxy-timeout-ms` -> `--prompt-optimization-proxy-timeout-ms`

When a compatibility alias is used, Tau emits a deterministic deprecation
warning to stderr with the canonical replacement.

To generate gate evidence for alias compatibility:

```bash
scripts/dev/m22-compatibility-alias-validation.sh \
  --repo-root . \
  --output-json tasks/reports/m22-compatibility-alias-validation.json \
  --output-md tasks/reports/m22-compatibility-alias-validation.md
```

Boundary decisions and staged consolidation sets:
- `docs/guides/training-crate-boundary-plan.md`

## M24 Live-Run RL Benchmark Protocol

This section defines the benchmark proof protocol for M24 true RL work.
It standardizes baseline-vs-trained evidence so maintainers can compare runs.

Use this protocol with:

- [Milestone #24](https://github.com/njfio/Tau/milestone/24)
- [Issue #1697](https://github.com/njfio/Tau/issues/1697) benchmark fixtures
- [Issue #1674](https://github.com/njfio/Tau/issues/1674) significance reporting

### Protocol Steps

1. Freeze benchmark inputs:
   use the same benchmark fixture file, model/provider, and episode count for
   both baseline and trained runs.
2. Run baseline checkpoint:
   execute benchmark suite against baseline policy/checkpoint and persist
   `tasks/reports/m24-benchmark-baseline.json`.
3. Run trained checkpoint:
   execute benchmark suite against trained policy/checkpoint and persist
   `tasks/reports/m24-benchmark-trained.json`.
4. Compute significance:
   produce `tasks/reports/m24-benchmark-significance.json` with p-value and
   confidence-level fields.
5. Publish consolidated proof artifact:
   fill `scripts/demo/m24-rl-benchmark-proof-template.json` into a run-scoped
   artifact (for example `tasks/reports/m24-benchmark-proof-<run_id>.json`).

### Required Artifacts

- baseline report JSON
- trained report JSON
- significance report JSON
- consolidated benchmark proof JSON using the M24 template

Validate the consolidated artifact:

```bash
scripts/demo/validate-m24-rl-benchmark-proof-template.sh \
  tasks/reports/m24-benchmark-proof-<run_id>.json
```

### Pass/Fail Significance Criteria

The benchmark proof is a pass only if all conditions hold:

- reward improvement: `trained.mean_reward - baseline.mean_reward >= criteria.min_reward_delta`
- safety regression bound:
  `trained.mean_safety_penalty - baseline.mean_safety_penalty <= criteria.max_safety_regression`
- statistical significance: `significance.p_value <= criteria.max_p_value`
- confidence floor: `significance.confidence_level >= 0.95`

If any criterion fails, mark `significance.pass=false` and treat the run as a
regression or non-significant improvement.

### Interpretation Limits For Significance Reports

Significance reports are guardrails, not causal proofs.

- seed reproducibility checks compare bounded ranges of p-values and reward
  deltas at fixed sample size
- sample-size sensitivity checks compare adjacent sample-size drift for p-value
  and reward delta
- passing these checks means conclusions are stable within configured bands, not
  universally true for all task distributions
- failing checks means "insufficiently stable evidence", not necessarily "model
  regression"

Reference implementation for reproducibility checks:
`tau_trainer::benchmark_significance`.

### Benchmark Report Publication And Archival Policy

Use `scripts/demo/m24-rl-benchmark-report-template.json` for each published
benchmark report (`baseline`, `trained`, `significance`, `summary`).

Required schema groups:

- run metadata: `schema_version`, `report_kind`, `run_id`, `generated_at`
- benchmark metadata: `benchmark_suite.name`, `benchmark_suite.version`
- metrics: `episodes`, `mean_reward`, `mean_safety_penalty`
- publication paths: `publication.report_path`, `publication.archive_path`
- retention policy: `retention.policy`, `retention.retain_days`,
  `retention.archive_after_days`, `retention.purge_after`

`.tau/reports` naming conventions:

- active report:
  `.tau/reports/m24/<run_id>/m24-benchmark-report-<report_kind>.json`
- archived report:
  `.tau/reports/archive/m24/YYYY/MM/m24-benchmark-report-<run_id>-<report_kind>.json`

Retention policy rules:

- `retention.policy` must be `archive-then-purge` or `retain-only`
- `retention.retain_days > 0`
- `0 <= retention.archive_after_days <= retention.retain_days`

Validate publication reports with:

```bash
scripts/demo/validate-m24-rl-benchmark-report.sh \
  .tau/reports/m24/<run_id>/m24-benchmark-report-<report_kind>.json
```

## M24 Resume-After-Crash Drill Playbook

This drill proves runtime restart recovery for in-flight jobs and captures an
operator-ready artifact with exact commands and evidence paths.

### Drill Procedure

1. Run controlled crash-recovery integration proof:

```bash
cargo test -p tau-runtime \
  integration_background_job_runtime_recovers_running_manifest_after_restart
```

2. Capture operator command transcript to a log file, for example:
   `tasks/reports/m24-recovery-operator.log`.
3. Populate the template:
   `scripts/demo/m24-rl-resume-after-crash-playbook-template.json`.
4. Save run-scoped artifact, for example:
   `tasks/reports/m24-resume-after-crash-playbook-<run_id>.json`.
5. Validate artifact:

```bash
scripts/demo/validate-m24-rl-resume-after-crash-playbook.sh \
  tasks/reports/m24-resume-after-crash-playbook-<run_id>.json
```

### Required Evidence Fields

- crash drill metadata:
  `crash_drill.state_dir`, `crash_drill.running_manifest_path`,
  `crash_drill.crash_simulation_command`
- resume metadata:
  `resume_drill.restart_command`, `resume_drill.resumed_job_id`,
  `resume_drill.checkpoint_path`
- evidence paths:
  `evidence.events_log_path`, `evidence.health_snapshot_path`,
  `evidence.operator_log_path`
- pass outcome:
  `outcome.resume_status == succeeded` and
  `outcome.recovery_reason_code == job_recovered_after_restart`

## Ownership

Primary ownership surfaces:
- `crates/tau-trainer` (top-level orchestration lifecycle)
- `crates/tau-training-runner` and `crates/tau-training-store` (rollout execution + persistence)
- `crates/tau-algorithm` (prompt optimization strategy)
- `crates/tau-coding-agent` (CLI flag wiring and startup dispatch)

Ownership map: `docs/guides/runbook-ownership-map.md`.

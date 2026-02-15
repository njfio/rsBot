# Doc Density Scorecard

This scorecard tracks public API documentation density across Tau crates and defines CI guard targets.

## Method

Measured by `.github/scripts/rust_doc_density.py`:

- Scans `crates/*/src/**/*.rs`.
- Counts public API items declared with `pub` (`fn`, `struct`, `enum`, `trait`, `mod`, `const`, `type`).
- Marks an item documented when an immediate preceding line-doc comment (`///`) is present.
- Excludes test modules (`tests.rs`, `src/**/tests/**`).

## Baseline Snapshot (2026-02-14)

Before this wave:

- Overall: `648 / 1913` documented public items (`33.87%`).
- `tau-core`: `0 / 6` (`0.00%`).
- `tau-github-issues-runtime`: `0 / 5` (`0.00%`).
- `tau-slack-runtime`: `0 / 5` (`0.00%`).
- `tau-startup`: `3 / 21` (`14.29%`).

After this wave:

- Overall: `671 / 1913` documented public items (`35.08%`).
- `tau-core`: `6 / 6` (`100.00%`).
- `tau-github-issues-runtime`: `5 / 5` (`100.00%`).
- `tau-slack-runtime`: `5 / 5` (`100.00%`).
- `tau-startup`: `10 / 21` (`47.62%`).

## CI Targets

Targets are defined in `docs/guides/doc-density-targets.json` and enforced in CI:

- Global minimum: `35.0%`.
- Crate minima (anchor crates):
- `tau-core >= 100.0%`
- `tau-github-issues-runtime >= 100.0%`
- `tau-slack-runtime >= 100.0%`
- `tau-startup >= 45.0%`
- `tau-onboarding >= 23.0%`
- `tau-runtime >= 28.0%`
- `tau-provider >= 27.0%`
- `tau-tools >= 37.0%`
- `tau-multi-channel >= 45.0%`
- `tau-session >= 23.0%`

## CI Artifact

When Rust scope changes, CI uploads artifact `rust-doc-density` containing `ci-artifacts/rust-doc-density.json`.

### JSON Schema (v1)

- `schema_version` (`1`)
- `repo_root`
- `crate_count`
- `overall_public_items`
- `overall_documented_items`
- `overall_percent`
- `global_min_percent`
- `crate_targets` (object: crate -> threshold percent)
- `reports[]`:
- `crate`
- `total_public_items`
- `documented_public_items`
- `percent`
- `issues[]`:
- `kind`
- `detail`

## Recompute

```bash
python3 .github/scripts/rust_doc_density.py \
  --repo-root . \
  --targets-file docs/guides/doc-density-targets.json

python3 .github/scripts/rust_doc_density.py \
  --repo-root . \
  --targets-file docs/guides/doc-density-targets.json \
  --json-output-file ci-artifacts/rust-doc-density.json

python3 .github/scripts/rust_doc_density.py \
  --repo-root . \
  --targets-file docs/guides/doc-density-targets.json \
  --json
```

## Raw Marker Count (M23 Gate)

Use the marker-count command to measure raw rustdoc markers (`///`, `//!`)
across crate `src` trees for `>=3,000` trajectory tracking:

```bash
scripts/dev/rustdoc-marker-count.sh \
  --repo-root . \
  --scan-root crates \
  --output-json tasks/reports/m23-rustdoc-marker-count.json \
  --output-md tasks/reports/m23-rustdoc-marker-count.md
```

Outputs:

- `tasks/reports/m23-rustdoc-marker-count.json`
- `tasks/reports/m23-rustdoc-marker-count.md`

### Threshold Verification Artifact

Compare current marker totals against a persisted baseline snapshot and record
gate status (`PASS`/`FAIL`) plus per-crate deltas:

```bash
scripts/dev/rustdoc-marker-threshold-verify.sh \
  --repo-root . \
  --baseline-json tasks/reports/m23-rustdoc-marker-count-baseline.json \
  --current-json tasks/reports/m23-rustdoc-marker-count.json \
  --threshold 3000 \
  --output-json tasks/reports/m23-rustdoc-marker-threshold-verify.json \
  --output-md tasks/reports/m23-rustdoc-marker-threshold-verify.md
```

Outputs:

- `tasks/reports/m23-rustdoc-marker-threshold-verify.json`
- `tasks/reports/m23-rustdoc-marker-threshold-verify.md`

### Ratchet Floor Check (CI Guardrail)

M23 also enforces a non-regression ratchet floor for raw marker totals:

Policy:

- `tasks/policies/m23-doc-ratchet-policy.json`

Command:

```bash
scripts/dev/rustdoc-marker-ratchet-check.sh \
  --repo-root . \
  --policy-file tasks/policies/m23-doc-ratchet-policy.json \
  --current-json tasks/reports/m23-rustdoc-marker-count.json \
  --output-json tasks/reports/m23-rustdoc-marker-ratchet-check.json \
  --output-md tasks/reports/m23-rustdoc-marker-ratchet-check.md
```

Outputs:

- `tasks/reports/m23-rustdoc-marker-ratchet-check.json`
- `tasks/reports/m23-rustdoc-marker-ratchet-check.md`

### PR File-Level Hints

When crate density thresholds fail in CI, file-level hints are emitted for
changed Rust files with undocumented public items:

- script: `.github/scripts/doc_density_annotations.py`
- artifact: `ci-artifacts/rust-doc-density-annotations.json`

This complements (does not replace) `rust_doc_density.py`, which measures
documented-public-API coverage percentage.

## Allocation Quotas (M23)

Crate-level quota planning from baseline to `>=3,000` markers is tracked in:

- `tasks/policies/m23-doc-allocation-plan.json`
- `tasks/reports/m23-doc-allocation-plan.md`
- guide: `docs/guides/doc-density-allocation-plan.md`

Owner-domain cadence metadata is mirrored in
`docs/guides/doc-density-targets.json` under
`owner_domain_review_cadence_days`.

## Gate Reproducibility Artifact (M23)

For milestone gate reviews, generate a standardized artifact bundle that records
the exact doc-density command, tool versions, and execution context:

```bash
scripts/dev/doc-density-gate-artifact.sh \
  --repo-root . \
  --targets-file docs/guides/doc-density-targets.json \
  --output-json tasks/reports/m23-doc-density-gate-artifact.json \
  --output-md tasks/reports/m23-doc-density-gate-artifact.md
```

### Artifact Outputs

- `tasks/reports/m23-doc-density-gate-artifact.json`
- `tasks/reports/m23-doc-density-gate-artifact.md`

## Artifact Template

Gate artifact payload fields (JSON):

- `schema_version`
- `generated_at`
- `repo_root`
- `command` (`script`, `targets_file`, `rendered`)
- `versions` (`python3`, `rustc`, `cargo`, `gh`, `jq`)
- `context` (`os`, `git_commit`, `git_branch`, `git_dirty`)
- `density_report` (embedded output from `rust_doc_density.py --json`)
- `troubleshooting` (operator diagnostics checklist entries)

Markdown template sections:

- Command
- Versions
- Context
- Summary
- Crate Breakdown
- Troubleshooting
- Reproduction Command

## Troubleshooting

1. Count mismatch vs CI artifact:
   Confirm `command.rendered` and `command.targets_file` match the CI run.
2. Unexpected count movement after toolchain updates:
   Compare `versions` between local and CI artifacts and rerun with aligned
   tool versions.
3. Local/remote divergence:
   Verify `context.git_commit` and `context.git_dirty` before concluding the
   counter behavior regressed.

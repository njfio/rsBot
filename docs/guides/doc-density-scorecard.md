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

## Recompute

```bash
python3 .github/scripts/rust_doc_density.py \
  --repo-root . \
  --targets-file docs/guides/doc-density-targets.json

python3 .github/scripts/rust_doc_density.py \
  --repo-root . \
  --targets-file docs/guides/doc-density-targets.json \
  --json
```

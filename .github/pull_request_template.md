## Summary

- Behavior changes:
- Risks / compatibility notes:

## Lane Boundary Contract

- Lane (`structural` | `docs` | `rl`):
- Boundary Map (`tasks/policies/pr-batch-lane-boundaries.json`):
- Boundary Paths (list concrete files/paths touched):
- Hotspot Mitigation (`none` if no hotspot path touched):
- Batch Size (`<open PR count in lane>` / `<lane max>`):
- Review SLA (first review and merge target windows):
- Exception Reference (`none` or `tasks/policies/pr-batch-exceptions.json#<exception_id>`):
- Branch Freshness (`age_days`, `behind_commits`, `ok|warning|critical`):
- Stale Alert Acknowledgement (`none` or link to acknowledgement comment/workflow update):

## Validation Evidence

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Additional issue-specific tests (list):

Closes #<issue-id>

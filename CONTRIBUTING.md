# Contributing to Tau

Thanks for contributing to Tau.

## Development Workflow

1. Create or identify a GitHub issue for the change.
2. Create a branch from `master` using the project naming convention (`codex/issue-<id>-<slug>`).
3. Add or update spec artifacts for the issue under `specs/<issue-id>/`.
4. Implement in small, reviewable commits.
5. Update docs/specs in the same PR when behavior changes.

## Testing and Quality Gates

Run these before opening a PR:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test -p <crate>
cargo check -q
```

For docs/scripts-only slices, run the relevant conformance scripts in `scripts/dev/`.

## Pull Request Expectations

Every PR should include:

- A short summary of what changed and why.
- Links to milestone/issue/spec artifacts.
- Acceptance criteria mapping (AC -> tests).
- RED/GREEN/REGRESSION evidence for test-driven slices.
- Risk/rollback notes.

Keep scope focused. Avoid unrelated edits in the same PR.

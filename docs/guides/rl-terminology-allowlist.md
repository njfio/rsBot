# RL Terminology Allowlist

This guide defines which RL terms are approved in future-planning contexts and
how the scanner distinguishes those from stale wording in current capability
docs.

## Policy And Scanner

- Policy: `tasks/policies/rl-terms-allowlist.json`
- Scanner: `scripts/dev/rl-terminology-scan.sh`

Run scan:

```bash
scripts/dev/rl-terminology-scan.sh \
  --repo-root . \
  --scan-root . \
  --allowlist-file tasks/policies/rl-terms-allowlist.json \
  --output-json tasks/reports/m22-rl-terminology-scan.json \
  --output-md tasks/reports/m22-rl-terminology-scan.md
```

## Approved Examples

Approved future-RL references are limited to planning/research paths and
specific context phrases:

- term: `reinforcement learning`
- allowed_paths: `docs/planning/`, `docs/research/`
- required_context: `future true-RL roadmap`, `future RL roadmap`, `Q3`, `planned`

Example approved usage:

- "We plan reinforcement learning experiments in Q3" within
  `docs/planning/future-true-rl-roadmap.md`.

## Non-Examples

The scanner should classify these as stale wording:

- `reinforcement learning` in operational guides like `docs/guides/` without
  future-roadmap context.
- phrases from `disallowed_defaults` such as `current RL mode` and
  `RL training loop` in current-state docs/help text.

## Why This Exists

The allowlist prevents over-cleanup while still enforcing naming alignment for
current prompt optimization functionality.

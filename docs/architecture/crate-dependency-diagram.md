# Crate Dependency Architecture Diagram

Run all commands from repository root.

## Purpose

This document provides the canonical workspace crate dependency map used for onboarding, impact analysis,
and architecture reviews.

## Generate and Validate

Generate deterministic dependency graph artifacts:

```bash
scripts/dev/crate-dependency-graph.sh \
  --output-json tasks/reports/crate-dependency-graph.json \
  --output-md tasks/reports/crate-dependency-graph.md
```

Deterministic timestamp mode:

```bash
scripts/dev/crate-dependency-graph.sh \
  --output-json tasks/reports/crate-dependency-graph.json \
  --output-md tasks/reports/crate-dependency-graph.md \
  --generated-at 2026-02-21T00:00:00Z
```

## Artifacts

- `tasks/reports/crate-dependency-graph.json`
- `tasks/reports/crate-dependency-graph.md`

The markdown artifact includes a Mermaid graph, crate inventory, and workspace edge table.

## Operational Cadence

1. Regenerate artifacts after dependency-boundary changes.
2. Include updated artifacts in PRs that add/remove workspace crate relationships.
3. Review edge deltas during architecture and release-readiness checks.

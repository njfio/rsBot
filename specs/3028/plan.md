# Plan: Issue #3028 - Publish crate dependency architecture diagram

## Approach
1. Add RED conformance test for missing script and output/schema expectations.
2. Implement dependency graph script that consumes `cargo metadata` (or fixture metadata via flag), extracts workspace-only dependency edges, and emits deterministic JSON/Markdown (with Mermaid graph).
3. Publish architecture documentation with generation/validation command contract.
4. Generate artifacts and run conformance + baseline checks.

## Affected Paths
- `scripts/dev/crate-dependency-graph.sh` (new)
- `scripts/dev/test-crate-dependency-graph.sh` (new)
- `docs/architecture/crate-dependency-diagram.md` (new)
- `tasks/reports/crate-dependency-graph.json`
- `tasks/reports/crate-dependency-graph.md`
- `specs/milestones/m185/index.md`
- `specs/3028/spec.md`
- `specs/3028/plan.md`
- `specs/3028/tasks.md`

## Risks and Mitigations
- Risk: workspace metadata parsing complexity.
  - Mitigation: use stable `cargo metadata --format-version 1` schema and fixture-driven test.
- Risk: non-deterministic output ordering.
  - Mitigation: sort crate and edge lists lexicographically before writing artifacts.
- Risk: oversized markdown graph.
  - Mitigation: include complete Mermaid graph and separate concise summary section.

## Interfaces / Contracts
Script interface:
- `--metadata <path>` optional metadata JSON override
- `--output-json <path>` output JSON report path
- `--output-md <path>` output markdown report path
- `--generated-at <iso>` deterministic timestamp
- `--quiet` suppress stdout summary

Output schema contract:
- `schema_version`
- `generated_at`
- `inputs` (`metadata_source`)
- `summary` (`workspace_crates`, `workspace_edges`)
- `crates` (`name`, `manifest_path`)
- `edges` (`from`, `to`)

## ADR
Not required (docs/script quality and architecture-visibility slice).

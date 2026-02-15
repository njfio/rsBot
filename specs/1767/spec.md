# Issue 1767 Spec

Status: Accepted

Issue: `#1767`  
Milestone: `#21`  
Parent: `#1761`

## Problem Statement

Roadmap execution currently lacks a deterministic extractor that emits a
machine-readable issue hierarchy graph plus a human-readable tree for the
`#1678` execution hierarchy. This reduces observability of parent/child drift
and orphaned nodes during milestone operations.

## Scope

In scope:

- script to extract hierarchy graph from GitHub issues with retry handling
- normalized JSON graph output (nodes/edges + orphan/missing-link signals)
- human-readable Markdown tree output
- deterministic fixture mode for local/CI validation

Out of scope:

- dashboard/UI visualization
- retention scheduling and publication policy (handled by `#1768`)
- automated issue mutation/fixes

## Acceptance Criteria

AC-1 (API + retry):
Given live repository access and a root issue id,
when the extractor runs in live mode,
then it fetches issues via GitHub API with bounded retry handling and emits
outputs without manual intervention.

AC-2 (JSON graph):
Given issue hierarchy data,
when extraction completes,
then JSON output contains normalized nodes and parent-child edges and includes
explicit missing-link and orphan-node signals.

AC-3 (Markdown tree):
Given the same extracted hierarchy,
when Markdown output is generated,
then it contains a readable tree rooted at the target issue and separate
sections for missing links/orphans.

AC-4 (deterministic validation):
Given fixture input data,
when extractor tests are run,
then outputs are deterministic and regression checks fail on contract drift.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given fixture/live options, when extractor runs, then required inputs parse and run mode executes successfully. |
| C-02 | AC-2 | Conformance | Given fixture with parent-child chain, when JSON is emitted, then nodes/edges match expected counts and ids. |
| C-03 | AC-2 | Regression | Given fixture with missing parent links, when JSON is emitted, then missing-link and orphan arrays contain expected condition records. |
| C-04 | AC-3 | Functional | Given extracted graph, when Markdown is emitted, then tree and anomaly sections are present and reference expected issues. |
| C-05 | AC-4 | Regression | Given malformed fixture input, when extractor runs, then it exits non-zero with deterministic validation error. |

## Success Metrics

- extractor runs deterministically in fixture mode and passes test suite
- live-mode command and artifact paths are documented for tracker updates
- anomaly surfacing (`missing_links`, `orphan_nodes`) is machine-readable

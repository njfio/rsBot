# Issue 1768 Spec

Status: Accepted

Issue: `#1768`  
Milestone: `#21`  
Parent: `#1761`

## Problem Statement

`#1767` introduced hierarchy graph extraction, but there is no standardized
publication and retention workflow for those artifacts. Without a contract,
historical graph snapshots are inconsistent and difficult to discover during
tracker updates for `#1678`.

## Scope

In scope:

- machine-readable publication and naming convention policy for hierarchy graph
  artifacts
- publication script to persist timestamped historical snapshots and index data
- retention-window enforcement for historical snapshots
- operator documentation for publication steps used in tracker updates

Out of scope:

- dashboard visualization of history artifacts
- external object-store publication
- automatic GitHub issue comment posting

## Acceptance Criteria

AC-1 (naming convention policy):
Given hierarchy graph artifacts,
when publication policy is evaluated,
then naming conventions and retention defaults are explicitly defined in a
versioned policy document.

AC-2 (publication workflow):
Given current hierarchy graph JSON + Markdown outputs,
when the publication workflow runs,
then timestamped snapshot artifacts and a discoverability index are emitted to
history storage.

AC-3 (retention enforcement):
Given historical snapshots older than the retention window,
when publication runs,
then expired snapshots are pruned and index entries are updated accordingly.

AC-4 (tracker documentation):
Given roadmap operators updating `#1678`,
when they follow the roadmap status guide,
then publication + retention steps are documented with concrete commands and
policy references.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given publication policy JSON, when loaded, then naming and retention fields are present with valid values. |
| C-02 | AC-2 | Functional | Given valid graph JSON/Markdown inputs, when publish script runs, then snapshot artifacts and index are created. |
| C-03 | AC-2 | Integration | Given repeated publication runs, when snapshots are listed, then history index remains deterministic and discoverable. |
| C-04 | AC-3 | Regression | Given history containing expired snapshots, when publish script runs, then expired artifacts are pruned and index no longer references them. |
| C-05 | AC-4 | Regression | Given roadmap status sync docs, when docs tests run, then publication script/policy references are present. |

## Success Metrics

- historical snapshots are discoverable through a single committed index format
- retention pruning is deterministic and covered by regression tests
- roadmap operator docs contain end-to-end extract + publish commands

# Issue 1719 Spec

Status: Implemented

Issue: `#1719`  
Milestone: `#22`  
Parent: `#1700`

## Problem Statement

M22 requires user-facing training docs to use accurate dual-track language:
current implemented capability is prompt optimization, while true RL policy
learning remains a future roadmap track. Current docs use prompt-optimization
terms but do not consistently cross-link operators to the future true-RL track.

## Scope

In scope:

- add explicit dual-track wording in top-level and operator-facing docs
- add explicit cross-links to future true-RL planning artifacts
- verify docs link checks remain green

Out of scope:

- implementing true RL runtime features
- changing prompt-optimization runtime behavior or CLI flags

## Acceptance Criteria

AC-1 (dual-track wording):
Given user-facing training docs,
when operators read capability descriptions,
then docs explicitly distinguish current prompt optimization from future true RL.

AC-2 (future-track cross-links):
Given updated docs,
when training roadmap context is referenced,
then links to Epic `#1657` and Milestone `#24` are present.

AC-3 (docs verification):
Given documentation updates,
when docs checks run,
then link/integrity checks pass without regressions.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given `README.md`, when capability section is read, then current prompt-optimization-only status is explicit. |
| C-02 | AC-1 | Functional | Given `docs/guides/training-ops.md`, when scope text is read, then true RL is marked future/planned. |
| C-03 | AC-2 | Conformance | Given updated docs, when searching for roadmap references, then links to `#1657` and Milestone `#24` exist. |
| C-04 | AC-3 | Regression | Given docs checks, when `test_docs_link_check.py` runs, then suite passes. |

## Success Metrics

- Training docs present explicit current-vs-future track wording
- Operators can navigate from docs to true RL roadmap artifacts in one hop
- Docs link checks remain green

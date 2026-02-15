# Issue 1732 Spec

Status: Implemented

Issue: `#1732`  
Milestone: `#22`  
Parent: `#1700`

## Problem Statement

M22 requires roadmap taxonomy to distinguish current prompt-optimization
delivery from future true-RL work. A stale milestone taxonomy artifact still
labels the completed Agent Lightning port as "RL" even though this lane now
represents prompt optimization, creating naming drift in GitHub metadata.

## Scope

In scope:

- audit milestone/issue taxonomy artifacts for stale RL wording
- rename stale metadata artifacts in GitHub where wording is inaccurate
- update roadmap docs with corrected taxonomy references and links
- publish audit/rename evidence under `tasks/reports/`

Out of scope:

- changing true-RL roadmap artifacts in milestone `#24`
- runtime code behavior changes

## Acceptance Criteria

AC-1 (taxonomy audit):
Given current roadmap metadata,
when taxonomy audit runs,
then stale RL-labeled artifacts are identified with before/after evidence.

AC-2 (metadata rename):
Given stale artifacts,
when rename pass completes,
then inaccurate RL taxonomy names/descriptions are replaced with prompt
optimization wording while keeping links valid.

AC-3 (docs cross-link integrity):
Given taxonomy updates,
when roadmap docs are refreshed,
then links resolve and docs checks pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given GitHub milestone list, when audit runs, then stale taxonomy entries are recorded in report artifacts. |
| C-02 | AC-2 | Conformance | Given stale milestone metadata, when rename is applied, then milestone title/description reflect prompt optimization terminology. |
| C-03 | AC-3 | Regression | Given roadmap docs updates, when docs link checks run, then suite passes. |
| C-04 | AC-3 | Functional | Given roadmap execution docs, when read, then naming-alignment milestones and true-RL track remain clearly separated. |

## Success Metrics

- No inaccurate RL taxonomy names remain in active prompt-optimization milestone lane
- Audit evidence committed with before/after snapshot
- Docs links remain green after updates

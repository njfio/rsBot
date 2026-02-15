# Issue 1734 Spec

Status: Implemented

Issue: `#1734`  
Milestone: `#23`  
Parent: `#1656`

## Problem Statement

M23 quality gates require objective checks for low-value rustdoc comments.
Current workflow includes remediation policy/template but no executable helper
to detect anti-patterns consistently across crates.

## Scope

In scope:

- define anti-pattern heuristics in a checked-in policy file
- implement reproducible audit helper command
- emit JSON + Markdown findings artifacts
- document false-positive suppression handling
- add contract tests for detection and suppression behavior

Out of scope:

- automatic rewriting of doc comments
- CI workflow integration changes

## Acceptance Criteria

AC-1 (heuristics policy):
Given repository policy files,
when audit helper runs,
then anti-pattern heuristics are loaded from a checked-in policy artifact.

AC-2 (helper output):
Given repository docs/comments,
when helper runs,
then JSON + Markdown outputs include findings with path/line/pattern metadata.

AC-3 (false-positive handling):
Given known acceptable wording contexts,
when helper evaluates comments,
then suppressions are applied via policy rules and documented.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given policy JSON, when helper executes, then pattern list and suppression list are loaded successfully. |
| C-02 | AC-2 | Conformance | Given fixture comments, when helper runs, then JSON report includes deterministic findings and summary counts. |
| C-03 | AC-3 | Regression | Given suppressed fixture lines, when helper runs, then findings are omitted and suppression counts are reported. |
| C-04 | AC-3 | Integration | Given remediation guide docs, when reviewed, then false-positive handling workflow is documented with command examples. |

## Success Metrics

- one-command reproducible doc-quality scan for low-value comment anti-patterns
- deterministic findings contract with suppression support
- remediation guide updated with suppression workflow

# Issue 1755 Spec

Status: Accepted

Issue: `#1755`  
Milestone: `#22`  
Parent: `#1705`

## Problem Statement

Terminology cleanup for M22 risks deleting legitimate future-RL references
(e.g., roadmap-only forward-looking content). Without an explicit allowlist and
scanner integration, stale RL term scans produce noisy false positives and
operators cannot distinguish approved vs disallowed usage consistently.

## Scope

In scope:

- machine-readable RL-term allowlist schema and examples/non-examples
- terminology scanner integration that classifies findings as approved/stale
- documentation for policy usage and expected scanner output
- contract tests for schema and scanner classification behavior

Out of scope:

- editing all stale RL occurrences across the repository
- compatibility alias runtime behavior changes
- milestone gate closure decisions

## Acceptance Criteria

AC-1 (allowlist schema):
Given future-RL approved contexts,
when policy is authored,
then allowlist schema encodes approved terms, context patterns, and rationale.

AC-2 (examples and non-examples):
Given policy and docs,
when maintainers review terminology rules,
then concrete approved and disallowed examples are documented.

AC-3 (scanner integration):
Given a repository scan run,
when scanner evaluates RL terms,
then output distinguishes approved allowlisted matches from stale findings.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given allowlist policy file, when parsed, then required schema keys and non-empty entries exist. |
| C-02 | AC-2 | Functional | Given documentation guide, when read, then approved and non-approved examples are explicit. |
| C-03 | AC-3 | Conformance | Given fixture content, when scanner runs, then approved hits and stale findings are separated in JSON output. |
| C-04 | AC-3 | Regression | Given unknown CLI option or missing allowlist file, when scanner runs, then deterministic non-zero error is emitted. |
| C-05 | AC-1, AC-3 | Regression | Given allowlisted term in non-allowed path/context, when scanner runs, then finding remains stale. |

## Success Metrics

- stale RL terminology scans provide actionable, low-noise findings
- approved future-RL references are explicitly documented and machine-readable
- policy + scanner behavior is deterministic and test-backed

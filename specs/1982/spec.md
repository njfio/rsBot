# Issue 1982 Spec

Status: Accepted

Issue: `#1982`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Directory-level gate report summaries now exist, but there is no deterministic
quality-gate evaluator for those summaries. Operators need a stable pass/fail
decision with reason codes before accepting benchmark batches.

## Scope

In scope:

- add summary quality policy and decision models
- add evaluator helper consuming summary-manifest counters
- add machine-readable decision JSON projection

Out of scope:

- CI workflow wiring
- dashboard rendering
- remote report transport

## Acceptance Criteria

AC-1 (pass path):
Given a summary manifest that meets policy thresholds,
when quality evaluator runs,
then pass=true and deterministic counters are preserved.

AC-2 (fail reasons):
Given a summary manifest violating policy thresholds,
when quality evaluator runs,
then deterministic reason codes are emitted.

AC-3 (machine-readable serialization):
Given a quality decision,
when serialized,
then JSON payload is machine-readable.

AC-4 (invalid policy fail closed):
Given policy ratios outside `[0.0, 1.0]`,
when evaluator runs,
then deterministic validation error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given summary(pass=2, fail=0, invalid=0) and permissive policy, when evaluator runs, then pass=true with counters preserved. |
| C-02 | AC-2 | Integration | Given summary(pass=0, fail=2, invalid=1), when evaluator runs, then reason codes include `below_min_pass_entries` and threshold violations. |
| C-03 | AC-3 | Conformance | Given quality decision, when serialized, then payload has deterministic scalar fields and `reason_codes` array. |
| C-04 | AC-4 | Unit | Given policy with ratio >1.0, when evaluator runs, then deterministic out-of-range error is returned. |

## Success Metrics

- one deterministic helper yields pass/fail quality decisions for summary
  manifests
- decision reasons are machine-readable and actionable
- invalid policy inputs fail closed before decisioning

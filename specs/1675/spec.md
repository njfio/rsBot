# Issue 1675 Spec

Status: Implemented

Issue: `#1675`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

M24 requires a reproducible live-run benchmark proof artifact showing baseline
vs trained policy outcomes and significance-gated pass/fail decisions. Current
components exist (templates/validators/significance generator), but there is no
single command that produces the final proof bundle end-to-end.

## Scope

In scope:

- add one-shot live benchmark proof generator command
- generate baseline/trained benchmark report artifacts from sample vectors
- generate significance report artifact
- assemble final benchmark proof artifact and validate it
- include failure-analysis details when significance gate fails

Out of scope:

- distributed training execution orchestration
- external dashboard publishing
- policy-update algorithm changes

## Acceptance Criteria

AC-1 (end-to-end proof generation):
Given baseline and trained sample vectors,
when live proof command runs,
then baseline/trained/significance/proof artifacts are generated.

AC-2 (statistical gain gate):
Given clear trained reward gain,
when proof is generated,
then proof significance status is pass and thresholds are satisfied.

AC-3 (validator compatibility):
Given generated proof artifact,
when proof validator executes,
then artifact passes schema/contract checks.

AC-4 (failure analysis):
Given non-significant or regressed trained metrics,
when proof is generated,
then command still emits artifact with failure analysis summary and non-zero exit.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given valid baseline/trained sample arrays, when generator runs, then baseline/trained/significance/proof files are created. |
| C-02 | AC-2 | Functional | Given trained samples with clear gain, when generator runs, then proof `significance.pass=true` and command exits 0. |
| C-03 | AC-3 | Integration | Given generated proof artifact, when `validate-m24-rl-benchmark-proof-template.sh` runs, then validation succeeds. |
| C-04 | AC-4 | Regression | Given trained samples without gain, when generator runs, then output includes failure analysis fields and command exits non-zero. |

## Success Metrics

- maintainers can produce full M24 benchmark proof artifacts with one command
- proof validation is automated and deterministic
- failing benchmark gain scenarios include actionable failure analysis

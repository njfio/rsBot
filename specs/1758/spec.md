# Issue 1758 Spec

Status: Accepted

Issue: `#1758`  
Milestone: `#23`  
Parent: `#1708`

## Problem Statement

Doc-quality audit findings are currently tracked inconsistently, which makes it
hard to prioritize remediation, enforce response windows, and prove closure
quality during milestone gate reviews.

## Scope

In scope:

- remediation severity class definitions
- SLA and checklist guidance for remediation workflow
- standardized closure-proof field set
- docs + tasks artifacts that operators can reuse directly
- contract tests validating required sections and policy/template coherence

Out of scope:

- automated issue creation from audit outputs
- CI enforcement of remediation SLAs
- milestone closure decisions for M23 gate story

## Acceptance Criteria

AC-1 (severity classes):
Given a doc-quality finding,
when triaging begins,
then severity classes and their meanings are documented in a machine-readable
policy artifact and reflected in template guidance.

AC-2 (SLA + checklist):
Given a finding severity,
when remediation is tracked,
then SLA targets and checklist steps are explicit and reusable.

AC-3 (closure proof):
Given a finding marked ready to close,
when closure is recorded,
then required proof fields are present so closure evidence is standardized.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given policy artifact, when parsed, then severity classes include name/definition/SLA metadata. |
| C-02 | AC-2 | Functional | Given remediation template, when used for a finding, then checklist + SLA fields are present and unambiguous. |
| C-03 | AC-3 | Conformance | Given closure-ready finding, when template closure section is filled, then required proof fields are enumerated. |
| C-04 | AC-1, AC-2 | Regression | Given policy/template updates, when contract tests run, then severity lists stay aligned between policy and template guide. |
| C-05 | AC-2, AC-3 | Regression | Given docs updates, when docs contract test runs, then workflow references policy + template + closure proof fields. |

## Success Metrics

- doc-quality findings can be triaged with one standard severity taxonomy
- remediation tracking includes SLA/checklist expectations by default
- closure evidence becomes consistent across findings

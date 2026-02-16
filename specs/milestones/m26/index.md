# Milestone M26: Oversized Policy Hardening + Exemption Burn-Down

Status: Implemented

## Objective

Strengthen oversized-file governance after M25 by ensuring exemption metadata is
actively accurate and by removing stale exemptions for files now below the
default production threshold.

## Scope

In scope:

- stale exemption cleanup in `tasks/policies/oversized-file-exemptions.json`
- fail-closed validation rules that reject exemptions for non-oversized files
- deterministic shell/Python regression coverage for exemption drift

Out of scope:

- new large-scale runtime decomposition work outside policy metadata drift
- unrelated performance or protocol changes

## Success Signals

- M26 hierarchy exists and is active with epic/story/task/subtask labels.
- Exemption metadata only contains actively oversized files (or none).
- Policy and guardrail tests fail closed on stale exemption metadata.
- CI oversized-file guard remains green without temporary stale exceptions.

## Issue Hierarchy

Milestone: GitHub milestone `M26 Oversized Policy Hardening + Exemption Burn-Down`

Epic:

- `#2086` Epic: M26 Oversized Policy Hardening + Exemption Burn-Down

Story:

- `#2087` Story: M26.1 Oversized Exemption Contract Enforcement

Task:

- `#2088` Task: M26.1.1 Remove stale oversized-file exemptions and enforce active-size eligibility

Subtask:

- `#2089` Subtask: M26.1.1a Add stale-exemption regression checks and clean policy file

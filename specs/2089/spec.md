# Spec #2089

Status: Implemented
Milestone: specs/milestones/m26/index.md
Issue: https://github.com/njfio/Tau/issues/2089

## Problem Statement

The oversized-file exemption list currently contains stale entries for files
that are no longer above the default threshold. This weakens governance signal
because obsolete exemptions can linger without enforcement. We need a fail-closed
rule that rejects exemptions unless the referenced file is actively oversized,
then clean stale exemption metadata.

## Acceptance Criteria

- AC-1: Oversized-file policy validation fails when an exemption references a
  file at or below the default threshold.
- AC-2: Repository exemption metadata removes stale entries and remains valid.
- AC-3: Guardrail tests cover stale-exemption regression and stay green.

## Scope

In:

- update `scripts/dev/oversized-file-policy.sh` to verify active-size
  eligibility for each exemption path
- extend `scripts/dev/test-oversized-file-policy.sh` with stale-exemption
  fail-closed regression case
- clean `tasks/policies/oversized-file-exemptions.json` stale rows
- ensure related oversized-file guardrail contracts pass

Out:

- new runtime module decomposition unrelated to policy metadata drift
- changing default threshold values

## Conformance Cases

- C-01 (AC-1, functional): policy script exits non-zero with explicit stale
  exemption error when exemption file is <= default threshold.
- C-02 (AC-2, integration): repository exemptions JSON validates and reflects
  stale-entry removal.
- C-03 (AC-3, regression): shell policy tests include stale-exemption case and
  pass.
- C-04 (AC-3, regression): oversized-file guardrail contract test remains green.

## Success Metrics

- `tasks/policies/oversized-file-exemptions.json` no longer contains stale
  entries.
- policy script has deterministic fail-closed stale-exemption behavior.
- conformance suites for C-01..C-04 pass.

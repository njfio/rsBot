# Spec #2262

Status: Implemented
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2262

## Problem Statement

Deployment WASM constraint profiles still encode `wasi_snapshot_preview1` as the
required ABI. Gap-9 requires migration to WASI preview2 posture with validation
that compile/runtime compatibility checks still pass.

## Scope

In scope:

- Update deployment WASM runtime constraint defaults from preview1 to preview2
  ABI posture.
- Preserve deterministic compliance/inspect reporting behavior.
- Validate deployment and wasm compile smoke checks.
- Document preview2 constraint posture in deployment runbook.

Out of scope:

- Replacing Wasmtime runtime architecture.
- New wasm runtime feature design beyond ABI profile migration.

## Acceptance Criteria

- AC-1: Given deployment runtime constraint defaults, when packaging/inspect
  flows run, then required ABI is preview2-oriented (not preview1).
- AC-2: Given import-module compliance checks, when non-empty import modules are
  evaluated, then required-ABI/allowlist matching supports preview2 pattern
  semantics and rejects preview1-only posture.
- AC-3: Given compile/runtime compatibility validation, when deployment tests and
  wasm smoke harness run, then checks pass.
- AC-4: Given operator documentation, when deployment runbook is reviewed, then
  preview2 ABI posture is documented.

## Conformance Cases

- C-01 (AC-1, unit/integration): `tau-deployment` tests assert inspect reports
  emit preview2 ABI requirement for both runtime profiles.
- C-02 (AC-2, unit): compliance matcher handles wildcard/prefix profile entries
  for required/allowed/forbidden import modules.
- C-03 (AC-3, integration): `cargo test -p tau-deployment` passes after
  migration.
- C-04 (AC-3, integration): `scripts/dev/wasm-smoke.sh` passes.
- C-05 (AC-4, documentation): `docs/guides/deployment-ops.md` documents preview2
  ABI expectation.

## Success Metrics / Observable Signals

- No `wasi_snapshot_preview1` required ABI defaults remain in deployment profiles.
- Deployment inspect reports show preview2 ABI requirement.
- Deployment crate tests and wasm smoke checks pass.

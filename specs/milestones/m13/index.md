# M13: P3 Sandbox

Milestone: `Gap List P3 Sandbox` (`#13`)

## Scope

Harden execution sandboxing:

- docker-level sandbox backend
- WASM runtime sandbox with fuel metering
- isolation and capability controls

## Active Spec-Driven Issues (current lane)

- `#1425` Epic: P3 Sandbox Hardening (1.5, 1.8)
- `#1438` Story: 1.5 Docker-Level Sandbox Backend
- `#1444` Story: 1.8 WASM Sandbox Runtime with Fuel Metering

## Contract

Each implementation issue under this milestone must maintain:

- `specs/<issue-id>/spec.md`
- `specs/<issue-id>/plan.md`
- `specs/<issue-id>/tasks.md`

No implementation is considered complete until acceptance criteria are mapped to
conformance tests and verified in PR evidence.

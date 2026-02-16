# M21 Scaffold Inventory and Ownership Map

- Generated: 2026-02-16T00:00:00Z
- Source candidates: `tasks/reports/m21-scaffold-merge-remove-decision-matrix.json`
- Schema: `tasks/schemas/m21-scaffold-inventory.schema.json`

## Summary

| Metric | Value |
| --- | ---: |
| Total candidates | 13 |
| Existing crate paths | 12 |
| Missing owners | 0 |
| Runtime reference hits | 4 |
| Test touchpoint hits | 0 |

## Inventory

| Candidate | Action | Owner | Crate Path | Exists | Rust Files | Rust LOC | Runtime Hits | Test Hits |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: |
| `tau-algorithm` | keep | `training-runtime` | `crates/tau-algorithm` | yes | 7 | 3249 | 0 | 0 |
| `tau-browser-automation` | remove | `tools-runtime` | `crates/tau-browser-automation` | yes | 3 | 1465 | 0 | 0 |
| `tau-contract-runner-remnants` | remove | `runtime-core` | `-` | no | 0 | 0 | 0 | 0 |
| `tau-custom-command` | keep | `events-runtime` | `crates/tau-custom-command` | yes | 4 | 3178 | 0 | 0 |
| `tau-dashboard-widget-contracts` | merge | `gateway-ui` | `crates/tau-dashboard` | yes | 3 | 1910 | 0 | 0 |
| `tau-memory-postgres-backend` | remove | `memory-runtime` | `crates/tau-memory` | yes | 3 | 3406 | 4 | 0 |
| `tau-trainer` | keep | `training-runtime` | `crates/tau-trainer` | yes | 6 | 5691 | 0 | 0 |
| `tau-training-proxy` | keep | `training-runtime` | `crates/tau-training-proxy` | yes | 1 | 534 | 0 | 0 |
| `tau-training-runner` | keep | `training-runtime` | `crates/tau-training-runner` | yes | 1 | 2323 | 0 | 0 |
| `tau-training-store` | keep | `training-runtime` | `crates/tau-training-store` | yes | 2 | 2276 | 0 | 0 |
| `tau-training-tracer` | keep | `training-runtime` | `crates/tau-training-tracer` | yes | 1 | 493 | 0 | 0 |
| `tau-training-types` | keep | `training-runtime` | `crates/tau-training-types` | yes | 1 | 1227 | 0 | 0 |
| `tau-voice-runtime` | remove | `multi-channel-runtime` | `crates/tau-voice` | yes | 4 | 3111 | 0 | 0 |

## Update Instructions

Regenerate inventory artifacts with:

```bash
scripts/dev/scaffold-inventory.sh
```

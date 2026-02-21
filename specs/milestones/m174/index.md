# M174 - Gateway OpenResponses Module Split Phase 4 (Config Runtime)

## Context
`crates/tau-gateway/src/gateway_openresponses.rs` remains a hotspot after phase 3. This milestone extracts gateway config runtime handlers and helper plumbing into a dedicated module while preserving endpoint and hot-reload contracts.

## Scope
- Extract `/gateway/config` handlers from `gateway_openresponses.rs`.
- Move tightly-coupled config override/policy helper functions.
- Preserve response schema, route bindings, and telemetry behavior.

## Linked Issues
- Epic: #2982
- Story: #2983
- Task: #2984

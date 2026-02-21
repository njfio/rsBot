# M184 - Gateway API Route Inventory Drift Guard

## Context
Gateway route inventory markers in docs can drift from actual router declarations over time. Operators depend on accurate route counts and method/path inventory claims.

## Scope
- Add deterministic script to extract route inventory from gateway router source.
- Validate docs inventory markers against extracted counts.
- Emit machine-readable and markdown report artifacts for CI/review traceability.
- Add conformance tests and wire into docs quality checks.

## Linked Issues
- Epic: #3023
- Story: #3022
- Task: #3024

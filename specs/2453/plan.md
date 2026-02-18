# Plan #2453

1. Establish milestone container + hierarchy artifacts for #2453/#2454/#2455/#2456.
2. Implement #2455 via TDD in `tau-memory` runtime/query layers.
3. Verify scoped quality gates and map ACs in PR.

## Risks

- Over-pruning useful memories.
  Mitigation: identity exemption + soft-delete only + deterministic conformance tests.

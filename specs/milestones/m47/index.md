# M47 — OpenRouter Dynamic Catalog Discovery

Milestone: [GitHub milestone #47](https://github.com/njfio/Tau/milestone/47)

## Objective

Implement dynamic OpenRouter model discovery via `/api/v1/models`, map discovered metadata into Tau's `ModelCatalogEntry`, and merge discovered entries with built-in catalog data using deterministic precedence and fallback behavior.

## Scope

- Add OpenRouter payload parsing path in `tau-provider` model catalog loader.
- Add deterministic merge policy between built-in and remote catalogs.
- Preserve cache/offline behavior and explicit source diagnostics.
- Add conformance and regression tests for mapping, merge, and fallback semantics.

## Out of Scope

- Full OpenRouter routing preferences (provider ordering, route hints).
- OpenRouter OAuth flows.
- Non-catalog provider transport changes.

## Linked Hierarchy

- Epic: #2296
- Story: #2297
- Task: #2298
- Subtask: #2299

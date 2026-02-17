# M60 — Spacebot G8 Local Embeddings (FastEmbed Slice)

Milestone: [GitHub milestone #60](https://github.com/njfio/Tau/milestone/60)

## Objective

Implement the `tasks/spacebot-comparison.md` `G8` gap by adding first-class local
embeddings support in `tau-memory` so memory save/search can run without remote
embedding API calls.

## Scope

- Add a local embedding mode that can be selected from runtime/tool policy
  configuration.
- Preserve current provider-backed embedding behavior as opt-in/override.
- Keep deterministic fallback embedding behavior for failure paths.
- Add conformance, integration, and regression coverage for local mode and
  fallback behavior.

## Out of Scope

- Bulk memory ingestion (`G9`).
- Typed memory graph/lifecycle enhancements (`G5`, `G6`, `G7`).
- Provider billing/caching optimizations unrelated to embeddings selection.

## Linked Hierarchy

- Epic: #2364
- Story: #2365
- Task: #2366
- Subtask: #2367

# M75 - Spacebot G6 Memory Graph Relations (Phase 1)

Milestone: [GitHub milestone #75](https://github.com/njfio/Tau/milestone/75)

## Objective

Deliver the first production slice of `tasks/spacebot-comparison.md` gap `G6` by
adding persisted memory relations and graph-aware relevance contribution in memory
search ranking.

## Scope

- Add relation edge persistence between memory records.
- Expose relation metadata through write/read/search pathways.
- Add graph signal contribution to existing search ranking flow.
- Preserve backward compatibility for relation-less legacy records.

## Out of Scope

- Full lifecycle maintenance features from `G7` (decay/prune/dedup).
- UI visualization (`G19`).
- Cross-session Cortex synthesis (`G3`).

## Linked Hierarchy

- Epic: #2442
- Story: #2443
- Task: #2444
- Subtask: #2445

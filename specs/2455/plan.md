# Plan #2455

## Approach

1. Introduce:
   - `MemoryLifecycleMaintenancePolicy`
   - `MemoryLifecycleMaintenanceResult`
   - `FileMemoryStore::run_lifecycle_maintenance(...)`
2. Compute latest active records and graph edge-presence map.
3. For each active record:
   - skip identity from decay/prune/orphan soft-delete paths
   - decay stale importance
   - soft-delete when below floor
   - soft-delete low-importance orphan when enabled
4. Append updated records to preserve audit/history semantics.

## Risks

- Threshold math drift in tests.
  Mitigation: deterministic explicit policy inputs and fixed timestamps in tests.

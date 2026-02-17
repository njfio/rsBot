# Spec #2299

Status: Implemented
Milestone: specs/milestones/m47/index.md
Issue: https://github.com/njfio/Tau/issues/2299

## Problem Statement

`tau-provider` can refresh a remote model catalog only when the payload already matches Tau's schema. OpenRouter's `/api/v1/models` payload shape is not ingested, and remote refresh currently replaces the full catalog rather than merging with built-in entries. This blocks dynamic model discovery and creates brittle startup behavior when remote payloads are partial.

## Scope

In scope:

- Parse OpenRouter `/api/v1/models` payloads into `ModelCatalogFile` entries.
- Map discovered models into `provider=openrouter`, `model=<openrouter_model_id>` entries.
- Convert token pricing to per-million pricing where available.
- Merge remote catalog entries with built-in catalog entries using deterministic precedence.
- Preserve explicit cache/offline fallback behavior.
- Add tests covering mapping, merge, and fallback conformance.

Out of scope:

- OpenRouter transport routing preferences.
- Provider auth flow redesign.
- Runtime model auto-selection policies.

## Acceptance Criteria

- AC-1: Given an OpenRouter models payload, when catalog payload parsing runs, then parsing succeeds and produces normalized `ModelCatalogEntry` values for OpenRouter models.
- AC-2: Given a successful remote refresh, when startup catalog load completes, then built-in entries remain available and discovered remote entries are added.
- AC-3: Given key collisions between built-in and remote catalogs, when merge runs, then remote entries deterministically override matching keys.
- AC-4: Given a prior merged cache, when offline mode is enabled, then loader returns cached merged entries and explicit `Cache` source.
- AC-5: Given remote refresh failure and a readable cache, when loader runs, then loader returns `CacheFallback` source without dropping cached entries.

## Conformance Cases

- C-01 (AC-1, unit): OpenRouter payload with `data[]` parses into `provider=openrouter` entries with mapped context/cost fields.
- C-02 (AC-2, integration): Remote refresh from OpenRouter-shaped payload returns catalog containing both built-in and discovered entries.
- C-03 (AC-3, unit): Merge helper on duplicate key returns remote-overridden entry.
- C-04 (AC-4, integration): Offline load reuses merged cache and includes both built-in and discovered keys.
- C-05 (AC-5, regression): Remote failure with existing cache returns `ModelCatalogSource::CacheFallback` and preserves cached key set.

## Success Metrics / Observable Signals

- `/models-list --provider openrouter` can list discovered OpenRouter IDs from refresh payload.
- Startup diagnostics retain explicit source line and do not regress fallback visibility.
- Existing model-catalog tests remain green.

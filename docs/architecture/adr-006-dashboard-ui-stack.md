# ADR-006: Dashboard UI Architecture and Stack Selection

## Context
Tau's dashboard capabilities are now delivered through gateway-served web surfaces:

- `/webchat` for operator workflows.
- `/dashboard` embedded shell with API-backed Overview/Sessions/Memory/Configuration views.

The G18 checklist still left two unresolved decisions:

1. Implementation location (consolidated in gateway scope vs separate frontend repo).
2. Preferred stack for richer SPA evolution.

Without a formal decision, roadmap and contribution expectations stay ambiguous.

## Decision
1. **Architecture location:** keep dashboard UI implementation consolidated with gateway in the main Tau repository and served by `tau-gateway` endpoints (`/dashboard`, `/webchat`), not a separate frontend repository.
2. **Selected stack for rich UI evolution:** adopt **React + TypeScript + Vite** as the primary stack for future interactive dashboard increments, with assets embedded/served by gateway runtime.
3. **Migration posture:** preserve the current embedded shell route (`/dashboard`) and incrementally replace sections with richer components while maintaining gateway-hosted deployment and operator endpoint contracts.

## Consequences
### Positive
- Single deployment boundary: gateway runtime owns API + UI delivery.
- Lower operational overhead than multi-repo split.
- Clear contributor target stack for new dashboard features.
- Incremental migration keeps existing operator workflows stable.

### Negative
- Build and packaging complexity increases as frontend assets grow.
- Requires disciplined frontend asset/version handling inside gateway delivery.

### Neutral / Follow-on
- Current plain embedded shell remains valid baseline; React/TypeScript/Vite adoption can phase in view-by-view.
- Additional ADR(s) may refine asset build pipeline details when full SPA bundling is introduced.

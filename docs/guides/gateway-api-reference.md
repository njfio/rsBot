# Gateway API Reference

Run all commands from repository root.

## Scope and Source of Truth

This reference documents the HTTP/SSE/WS routes wired in:

- `crates/tau-gateway/src/gateway_openresponses.rs`
- `build_gateway_openresponses_router(...)`

If this document and the router differ, treat the router as canonical and update this file.

## Auth and Policy Contracts

### Auth modes

- `token`: `Authorization: Bearer <configured-token>` required on protected routes.
- `password-session`: call `POST /gateway/auth/session` first, then use returned bearer token on protected routes.
- `localhost-dev`: protected-route auth checks resolve to localhost-dev principal (no bearer token required), but rate limits still apply.

### Protection helpers

- `authorize_and_enforce_gateway_limits(...)`: bearer auth (mode-dependent) + rate limit.
- `authorize_dashboard_request(...)`: same auth + rate limit contract, used by dashboard-oriented APIs.

### Policy gates for mutating routes

- `allow_session_write`: required for session append/reset APIs.
- `allow_memory_write`: required for memory write/update/delete APIs.

## Endpoint Inventory

Auth column values:

- `Protected`: gateway auth + rate limits enforced.
- `Bootstrap`: no bearer auth; endpoint-specific constraints still apply.
- `Unprotected`: no gateway auth check in handler.

### LLM and compatibility APIs

| Method | Path | Auth | Policy Gate | Notes |
| --- | --- | --- | --- | --- |
| POST | `/v1/responses` | Protected | - | OpenResponses-compatible request/response (+ SSE when `stream=true`) |
| POST | `/v1/chat/completions` | Protected | - | OpenAI-compatible chat adapter |
| POST | `/v1/completions` | Protected | - | OpenAI-compatible completions adapter |
| GET | `/v1/models` | Protected | - | OpenAI-compatible model listing |

### Session and memory APIs

| Method | Path | Auth | Policy Gate | Notes |
| --- | --- | --- | --- | --- |
| GET | `/gateway/sessions` | Protected | - | Session list |
| GET | `/gateway/sessions/{session_key}` | Protected | - | Session detail |
| POST | `/gateway/sessions/{session_key}/append` | Protected | `allow_session_write` | Manual message append |
| POST | `/gateway/sessions/{session_key}/reset` | Protected | `allow_session_write` | Session reset |
| GET | `/gateway/memory/{session_key}` | Protected | - | Memory read |
| PUT | `/gateway/memory/{session_key}` | Protected | `allow_memory_write` | Memory upsert |
| GET | `/gateway/memory/{session_key}/{entry_id}` | Protected | - | Memory entry read |
| PUT | `/gateway/memory/{session_key}/{entry_id}` | Protected | `allow_memory_write` | Memory entry upsert |
| DELETE | `/gateway/memory/{session_key}/{entry_id}` | Protected | `allow_memory_write` | Memory entry delete |
| GET | `/gateway/memory-graph/{session_key}` | Protected | - | Session memory graph |
| GET | `/api/memories/graph` | Protected | - | API memory graph view |

### Gateway control, safety, audit, training, tools, jobs, deploy, cortex

| Method | Path | Auth | Policy Gate | Notes |
| --- | --- | --- | --- | --- |
| POST | `/gateway/channels/{channel}/lifecycle` | Protected | - | Multi-channel lifecycle action |
| GET | `/gateway/config` | Protected | - | Runtime config snapshot |
| PATCH | `/gateway/config` | Protected | - | Runtime config override |
| GET | `/gateway/safety/policy` | Protected | - | Safety policy read |
| PUT | `/gateway/safety/policy` | Protected | - | Safety policy update |
| GET | `/gateway/safety/rules` | Protected | - | Safety rules read |
| PUT | `/gateway/safety/rules` | Protected | - | Safety rules update |
| POST | `/gateway/safety/test` | Protected | - | Safety scan test |
| GET | `/gateway/audit/summary` | Protected | - | Audit summary |
| GET | `/gateway/audit/log` | Protected | - | Audit log records |
| GET | `/gateway/training/status` | Protected | - | Training status |
| GET | `/gateway/training/rollouts` | Protected | - | Training rollouts |
| PATCH | `/gateway/training/config` | Protected | - | Training config patch |
| GET | `/gateway/tools` | Protected | - | Tool inventory |
| GET | `/gateway/tools/stats` | Protected | - | Tool usage stats |
| GET | `/gateway/jobs` | Protected | - | Job list |
| POST | `/gateway/jobs/{job_id}/cancel` | Protected | - | Job cancel |
| POST | `/gateway/deploy` | Protected | - | Deployment action |
| POST | `/gateway/agents/{agent_id}/stop` | Protected | - | Agent stop action |
| POST | `/gateway/ui/telemetry` | Protected | - | UI telemetry ingest |
| POST | `/cortex/chat` | Protected | - | Cortex chat |
| GET | `/cortex/status` | Protected | - | Cortex status |

### External coding-agent bridge APIs

| Method | Path | Auth | Policy Gate | Notes |
| --- | --- | --- | --- | --- |
| POST | `/gateway/external-coding-agent/sessions` | Protected | - | Open bridge session |
| GET | `/gateway/external-coding-agent/sessions/{session_id}` | Protected | - | Session detail |
| POST | `/gateway/external-coding-agent/sessions/{session_id}/progress` | Protected | - | Append progress event |
| POST | `/gateway/external-coding-agent/sessions/{session_id}/followups` | Protected | - | Queue follow-up |
| POST | `/gateway/external-coding-agent/sessions/{session_id}/followups/drain` | Protected | - | Drain follow-ups |
| GET | `/gateway/external-coding-agent/sessions/{session_id}/stream` | Protected | - | SSE progress stream |
| POST | `/gateway/external-coding-agent/sessions/{session_id}/close` | Protected | - | Close session |
| POST | `/gateway/external-coding-agent/reap` | Protected | - | Reap idle sessions |

### Dashboard data APIs

| Method | Path | Auth | Policy Gate | Notes |
| --- | --- | --- | --- | --- |
| GET | `/dashboard/health` | Protected | - | Dashboard health payload |
| GET | `/dashboard/widgets` | Protected | - | Dashboard widgets payload |
| GET | `/dashboard/queue-timeline` | Protected | - | Queue timeline payload |
| GET | `/dashboard/alerts` | Protected | - | Alert payload |
| POST | `/dashboard/actions` | Protected | - | Dashboard action mutation |
| GET | `/dashboard/stream` | Protected | - | Dashboard SSE stream |

### Shell and UI routes

| Method | Path | Auth | Policy Gate | Notes |
| --- | --- | --- | --- | --- |
| GET | `/dashboard` | Unprotected | - | Dashboard shell HTML |
| GET | `/webchat` | Unprotected | - | Webchat shell HTML |
| GET | `/ops` | Unprotected | - | Ops dashboard shell |
| GET | `/ops/agents` | Unprotected | - | Ops shell route |
| GET | `/ops/agents/{agent_id}` | Unprotected | - | Ops shell route |
| GET | `/ops/chat` | Unprotected | - | Ops shell route |
| POST | `/ops/chat/new` | Unprotected | - | Creates/initializes session via form |
| POST | `/ops/chat/send` | Unprotected | - | Appends chat message via form |
| GET | `/ops/sessions` | Unprotected | - | Ops shell route |
| POST | `/ops/sessions/branch` | Unprotected | - | Session branch via form |
| GET | `/ops/sessions/{session_key}` | Unprotected | - | Ops shell route |
| POST | `/ops/sessions/{session_key}` | Unprotected | - | Ops session reset via form |
| GET | `/ops/memory` | Unprotected | - | Ops shell route |
| POST | `/ops/memory` | Unprotected | - | Memory create/edit via form |
| GET | `/ops/memory-graph` | Unprotected | - | Ops shell route |
| GET | `/ops/tools-jobs` | Unprotected | - | Ops shell route |
| GET | `/ops/channels` | Unprotected | - | Ops shell route |
| GET | `/ops/config` | Unprotected | - | Ops shell route |
| GET | `/ops/training` | Unprotected | - | Ops shell route |
| GET | `/ops/safety` | Unprotected | - | Ops shell route |
| GET | `/ops/diagnostics` | Unprotected | - | Ops shell route |
| GET | `/ops/deploy` | Unprotected | - | Ops shell route |
| GET | `/ops/login` | Unprotected | - | Ops login shell route |

### Bootstrap, status, and websocket

| Method | Path | Auth | Policy Gate | Notes |
| --- | --- | --- | --- | --- |
| GET | `/gateway/auth/bootstrap` | Bootstrap | - | Returns auth-mode bootstrap metadata; rate-limited |
| POST | `/gateway/auth/session` | Bootstrap | - | Password-session token issuance only; requires `--gateway-openresponses-auth-mode=password-session` |
| GET | `/gateway/status` | Protected | - | Gateway status/report payload |
| GET | `/gateway/ws` | Protected | - | WebSocket upgrade endpoint |

## Route-Coverage Validation Procedure

Use this to confirm documentation stays aligned with routed endpoint constants:

```bash
perl -0777 -ne 'while(/const\\s+([A-Z0-9_]+(?:ENDPOINT|ENDPOINT_TEMPLATE)):\\s*&str\\s*=\\s*"([^"]+)";/g){print "$1 $2\\n"}' \
  crates/tau-gateway/src/gateway_openresponses.rs | sort
```

Then verify each documented path exists in this file and include the literal route:

- `/ops/sessions/branch`


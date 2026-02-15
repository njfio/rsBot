# Background Jobs Operations Guide

Date: 2026-02-15  
Story: #1452  
Task: #1453

## Scope

Tau now ships persisted background job tooling through built-in tools:

- `jobs_create`
- `jobs_list`
- `jobs_status`
- `jobs_cancel`

The runtime persists job manifests, bounded health counters, and event logs in `--jobs-state-dir`.

## CLI/Policy Controls

- `--jobs-enabled` (`TAU_JOBS_ENABLED`)
- `--jobs-state-dir` (`TAU_JOBS_STATE_DIR`)
- `--jobs-list-default-limit` (`TAU_JOBS_LIST_DEFAULT_LIMIT`)
- `--jobs-list-max-limit` (`TAU_JOBS_LIST_MAX_LIMIT`)
- `--jobs-default-timeout-ms` (`TAU_JOBS_DEFAULT_TIMEOUT_MS`)
- `--jobs-max-timeout-ms` (`TAU_JOBS_MAX_TIMEOUT_MS`)

Related trace sinks:

- `--channel-store-root`
- `--session` / `--no-session`

## Runtime Layout

Under `--jobs-state-dir`:

- `state.json`  
  Aggregated counters (`created_total`, `started_total`, `succeeded_total`, `failed_total`, `cancelled_total`), queue depth, running jobs, recent reason-codes, and diagnostics.
- `events.jsonl`  
  Append-only lifecycle events (`created`, `started`, `succeeded`, `failed`, `cancelled`, `trace_error`).
- `jobs/<job_id>.json`  
  Per-job manifest and terminal status.
- `jobs/<job_id>.stdout.log` / `jobs/<job_id>.stderr.log`  
  Command output artifacts consumed by `jobs_status` preview.

## Reason Codes

Primary lifecycle reason-codes:

- `job_queued`
- `job_started`
- `job_succeeded`
- `job_non_zero_exit`
- `job_spawn_failed`
- `job_timeout`
- `job_cancelled_before_start`
- `job_cancelled_during_run`
- `job_recovered_after_restart`
- `job_runtime_error`
- `job_trace_write_failed`

Tool-level reason-codes:

- `jobs_disabled`
- `jobs_invalid_arguments`
- `jobs_runtime_unavailable`
- `jobs_runtime_error`
- `jobs_list_ok`
- `jobs_status_ok`
- `jobs_cancel_ok`
- `job_not_found`

## Tracing Integration

Each job transition can emit:

- ChannelStore log entries when `channel_transport` + `channel_id` are supplied.
- Session trace messages appended to the resolved session path.

This allows operators to correlate asynchronous execution with transport/session audit flows.

## Operational Notes

- Jobs run asynchronously via runtime worker loops.
- Cancelling a queued job transitions it directly to `cancelled`.
- Cancelling a running job sets a cancellation signal and the worker terminates the process.
- Running jobs recovered after process restart are re-queued with `job_recovered_after_restart`.

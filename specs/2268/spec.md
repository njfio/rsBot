# Spec #2268

Status: Accepted
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2268

## Problem Statement

Tau appends operational JSONL logs across runtime components (runtime cycle
events, heartbeat events, background job events, tool audit, and prompt
telemetry) but currently has no built-in size-based rotation. Long-running
deployments accumulate unbounded files and create disk-growth and operator
reliability risk.

## Scope

In scope:

- Add shared runtime log rotation policy and append helper for JSONL line writes.
- Add operator controls for retention via environment variables.
- Wire rotation into runtime event appenders and telemetry/audit appenders.
- Add conformance tests for rotation behavior and control handling.
- Document runtime log rotation controls and retained-file behavior.

Out of scope:

- Compression of rotated files.
- Time-based rotation schedules.
- Rotation of session transcript/history stores used as conversational memory.

## Acceptance Criteria

- AC-1: Given runtime JSONL appenders, when append would exceed configured
  max-bytes, then active log rotates and append continues on a fresh active
  file.
- AC-2: Given configured max-files retention, when repeated rotation happens,
  then only configured retained files remain (`active + numbered backups`) and
  oldest backups are pruned.
- AC-3: Given operator environment controls, when values are set, then runtime
  log rotation policy uses those values; when unset/invalid, safe defaults are
  used.
- AC-4: Given runtime and ops documentation, when operators review deployment
  guidance, then the log rotation controls and retained-file naming are
  documented.

## Conformance Cases

- C-01 (AC-1, unit): append helper rotates active file when
  `current_size + incoming_line > max_bytes`.
- C-02 (AC-2, unit): repeated rotations keep only `max_files - 1` backups and
  prune older files.
- C-03 (AC-3, functional): environment control parser applies valid values and
  falls back for invalid/empty values.
- C-04 (AC-1, integration): runtime appenders (heartbeat + runtime events +
  telemetry/audit) continue writing records after rotation.
- C-05 (AC-4, documentation): operator docs include env vars, defaults, and
  backup naming convention.

## Success Metrics / Observable Signals

- Rotation helper tests pass for threshold, retention, and env policy controls.
- Runtime crate tests demonstrate appenders keep writing after rotation.
- Runtime docs list controls and retention naming (`*.jsonl`, `*.jsonl.1`, ...).

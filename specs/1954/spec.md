# Issue 1954 Spec

Status: Implemented

Issue: `#1954`  
Milestone: `#24`  
Parent: `#1659`

## Problem Statement

`SpansToTrajectories` currently emits variable-length trajectories directly from
span counts. Experience collection for PPO windows needs deterministic fixed-size
window controls (truncate/pad) so batch shape is predictable.

## Scope

In scope:

- add configurable window policy for `SpansToTrajectories`
- support truncation to trailing `window_size` steps
- support optional right-padding with synthetic zero-reward terminal-safe steps
- preserve deterministic `step_index` and valid trajectory schema after windowing

Out of scope:

- runner/store persistence schema changes
- PPO optimizer math changes
- padding mask tensors (future issue)

## Acceptance Criteria

AC-1 (default behavior compatibility):
Given no window policy,
when spans are adapted,
then output trajectory length and values match current behavior.

AC-2 (truncate tail window):
Given `window_size=N` and more than `N` spans,
when spans are adapted,
then trajectory contains the last `N` transitions with reindexed `step_index`.

AC-3 (pad to fixed length):
Given `window_size=N` and fewer than `N` spans with padding enabled,
when spans are adapted,
then trajectory length is exactly `N` and padded steps are deterministic.

AC-4 (invalid config fails closed):
Given invalid window policy (`window_size=0`),
when adapter runs,
then it returns deterministic validation error.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given adapter default config and two spans, when adapted, then length stays 2 and schema validates. |
| C-02 | AC-2 | Conformance | Given 5 spans and `window_size=3`, when adapted, then only tail 3 remain with `step_index` 0..2. |
| C-03 | AC-3 | Conformance | Given 1 span and `window_size=3` with padding enabled, when adapted, then output length is 3 with deterministic padded metadata. |
| C-04 | AC-4 | Unit | Given `window_size=0`, when adapting, then deterministic config error is returned. |

## Success Metrics

- fixed-length trajectory shaping is available without breaking default behavior
- output trajectories remain `EpisodeTrajectory::validate()` clean after windowing
- errors are deterministic for invalid configs

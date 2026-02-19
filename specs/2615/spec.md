# Spec: Issue #2615 - Integrate RL loop with live agent decision-making path

Status: Implemented

## Problem Statement
Tau has rollout, trajectory, PPO, and GAE components, but live interactive agent decisions are not currently persisted into RL experience rollouts nor fed into scheduled PPO/GAE updates. This leaves the RL loop disconnected from production decision traces.

## Acceptance Criteria

### AC-1 Live decision experiences are collected from agent runtime events
Given live runtime prompt execution with RL live loop enabled,
When the agent emits decision/runtime events,
Then a training rollout + attempt-scoped spans are persisted with deterministic rollout ids and terminal status updates.

### AC-2 PPO/GAE updates are scheduled from collected live rollouts
Given successful collected live rollouts,
When the configured update interval is reached,
Then trajectory collection and PPO/GAE update math execute against the collected live rollout set and publish an optimizer status report.

### AC-3 Guarded rollout controls prevent unsafe RL loop behavior
Given live RL loop runtime errors,
When consecutive failures exceed the configured threshold,
Then the live RL gate transitions to `hold` and further collection/update work is skipped until reset.

### AC-4 Feature is disabled by default and opt-in only
Given default runtime startup,
When no live RL enable env flag is set,
Then no live rollout collection/update hooks are attached.

### AC-5 Scoped verification gates are green
Given this issue scope,
When formatting, linting, and targeted live-RL/runtime tests run,
Then all checks pass.

## Scope

### In Scope
- Add live RL runtime bridge in `tau-coding-agent` that subscribes to `AgentEvent` during local runtime.
- Persist live rollout metadata/spans to training store and mark rollout lifecycle transitions.
- Schedule PPO/GAE update math on configured intervals using collected live rollouts.
- Add guarded failure gate controls and runtime snapshot visibility.
- Add unit/functional/regression tests for collection, scheduling, and failure gating behavior.

### Out of Scope
- Applying optimizer updates to production model weights.
- Distributed/multi-host RL orchestration.
- New external dependencies.

## Conformance Cases
- C-01 (functional): live event stream persists rollout + spans and terminal status.
- C-02 (functional): scheduled PPO/GAE update executes when interval threshold is reached.
- C-03 (regression): failure streak transitions live RL gate from pass -> hold.
- C-04 (unit): disabled-by-default env configuration leaves live RL bridge unregistered.
- C-05 (verify): `cargo fmt --check`, `cargo clippy -p tau-coding-agent -- -D warnings`, and targeted tests pass.

## Success Metrics / Observable Signals
- Live decision rollouts appear in training store with deterministic ids.
- PPO/GAE update reports are generated on schedule from live-collected trajectories.
- Gate transitions to hold under repeated failures instead of destabilizing runtime path.

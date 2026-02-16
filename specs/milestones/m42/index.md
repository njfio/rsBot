# Milestone M42: Resolution Roadmap P0 Provider Execution

Status: Implemented

## Objective

Execute the first P0 implementation wave from `tasks/resolution-roadmap.md`:
refresh provider model catalog metadata/entries, add DeepSeek alias wiring, and
ship a local-safe live validation harness for provider keys.

## Scope

In scope:

- model catalog schema expansion and built-in frontier/legacy entry refresh
- DeepSeek alias/auth candidate plumbing for OpenAI-compatible routing
- local-safe provider key template + smoke-run script for live validation
- verification tests for catalog parsing/lookup and provider alias resolution

Out of scope:

- full first-class OpenRouter provider variant rollout
- PPO pipeline architecture changes
- broad non-provider roadmap sections

## Success Signals

- M42 hierarchy exists and is active with epic/story/task/subtask linkage.
- P0 roadmap slice lands with test evidence and no regressions in touched crates.
- Local operators can inject provider keys without committing secrets.

## Issue Hierarchy

Milestone: GitHub milestone `M42 Resolution Roadmap P0 Provider Execution`

Epic:

- `#2216` Epic: M42 Resolution Roadmap P0 provider/catalog execution

Story:

- `#2217` Story: M42.1 Refresh provider model catalog and live validation flow

Task:

- `#2218` Task: M42.1.1 Implement roadmap P0 provider/catalog + local validation slice

Subtask:

- `#2219` Subtask: M42.1.1a Implement provider catalog/alias update and live key harness

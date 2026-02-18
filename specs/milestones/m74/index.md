# M74 — CI Roadmap Freshness and Fast Preflight

Milestone: [GitHub milestone #74](https://github.com/njfio/Tau/milestone/74)

## Objective

Keep roadmap freshness quality gates green while improving local developer
velocity with a single fast preflight command that mirrors core CI blockers.

## Scope

- Regenerate roadmap status sections required by CI freshness checks.
- Add a fast preflight script that runs roadmap check + rust validation with
  existing `fast-validate` logic.
- Add script tests for argument passthrough and strict failure behavior.

## Out of Scope

- Application runtime behavior changes.
- CI workflow architecture changes.

## Linked Hierarchy

- Epic: #2436
- Story: #2437
- Task: #2438
- Subtask: #2439

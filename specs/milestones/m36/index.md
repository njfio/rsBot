# Milestone M36: Split Module Documentation Wave 9

Status: Draft

## Objective

Advance split-module rustdoc coverage for `tau-session` and `tau-memory` helper
modules while extending guard assertions.

## Scope

In scope:

- rustdoc additions for selected wave-9 session/memory modules
- guard script expansion with wave-9 marker assertions
- scoped compile/test verification for touched crates

Out of scope:

- broad repository-wide documentation parity in one milestone
- behavior changes unrelated to documentation baseline

## Success Signals

- M36 hierarchy exists and is active with epic/story/task/subtask linkage.
- wave-9 session/memory modules gain rustdoc marker coverage.
- split-module guard and scoped crate validations remain green.

## Issue Hierarchy

Milestone: GitHub milestone `M36 Split Module Documentation Wave 9`

Epic:

- `#2168` Epic: M36 Split-Module Rustdoc Coverage Wave 9

Story:

- `#2169` Story: M36.1 Document session/memory split helper modules

Task:

- `#2170` Task: M36.1.1 Add rustdoc to session/memory split modules and extend guard

Subtask:

- `#2171` Subtask: M36.1.1a Document session locking/storage/integrity and memory backend modules

# Milestone M27: CLI Args Maintainability Decomposition

Status: Draft

## Objective

Reduce `tau-cli` CLI argument-surface complexity by extracting coherent domain
slices from `crates/tau-cli/src/cli_args.rs` while preserving flag behavior and
compatibility.

## Scope

In scope:

- execution-domain flag extraction from `cli_args.rs`
- clap compatibility regression checks for migrated slices
- maintainability-driven decomposition with no user-facing flag removals

Out of scope:

- unrelated runtime feature additions
- protocol/wire format changes

## Success Signals

- M27 hierarchy exists and is active with epic/story/task/subtask labels.
- `cli_args.rs` complexity is reduced via modular extraction.
- regression suites confirm no CLI contract drift.

## Issue Hierarchy

Milestone: GitHub milestone `M27 CLI Args Maintainability Decomposition`

Epic:

- `#2094` Epic: M27 CLI Args Domain Decomposition

Story:

- `#2095` Story: M27.1 Extract CLI execution-domain flag groups

Task:

- `#2096` Task: M27.1.1 Extract execution-domain flags from cli_args.rs into dedicated module

Subtask:

- `#2097` Subtask: M27.1.1a Scaffold execution-domain module and migrate first flag slice with regression checks

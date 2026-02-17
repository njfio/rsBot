# Spec #2265

Status: Accepted
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2265

## Problem Statement

Tau currently ships binary artifacts but does not ship first-party shell
completion scripts. Operators using bash/zsh/fish must discover and type large
flag surfaces manually, reducing CLI usability and increasing command mistakes.

## Scope

In scope:

- Add CLI completion rendering for bash/zsh/fish.
- Add release automation to generate and publish completion assets.
- Add contract tests for generation scripts and release workflow wiring.
- Add operator documentation for completion download/install usage.

Out of scope:

- Installing completions automatically into user shell config files.
- Supporting additional shells beyond bash/zsh/fish in this slice.
- Changing runtime command behavior unrelated to completion generation.

## Acceptance Criteria

- AC-1: Given `tau-coding-agent --shell-completion <shell>`, when shell is one
  of `bash|zsh|fish`, then valid completion content is emitted to stdout.
- AC-2: Given release workflow execution for a tag, when artifacts are staged,
  then bash/zsh/fish completion files are generated and published as release
  assets.
- AC-3: Given completion script/workflow changes, when CI release-helper scope
  runs, then completion generation contract tests and workflow contract tests
  pass.
- AC-4: Given operator docs, when onboarding shell completion usage, then docs
  include deterministic download/install commands for bash/zsh/fish.

## Conformance Cases

- C-01 (AC-1, conformance): CLI completion flag emits deterministic script
  output for bash/zsh/fish using command name `tau-coding-agent`.
- C-02 (AC-2, integration): release workflow generates completion assets and
  includes them in release publish files.
- C-03 (AC-3, functional): release helper contract scripts verify completion
  generator behavior and workflow step/file assertions.
- C-04 (AC-4, documentation): docs include completion asset paths and install
  command examples for bash/zsh/fish.

## Success Metrics / Observable Signals

- `scripts/release/test-shell-completions.sh` passes locally and in CI.
- `scripts/release/test-release-workflow-contract.sh` includes shell completion
  checks and passes.
- Release docs and README include shell completion usage paths and commands.

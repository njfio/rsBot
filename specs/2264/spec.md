# Spec #2264

Status: Accepted
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2264

## Problem Statement

Tau release automation publishes native archives but does not produce a Homebrew
formula. macOS/Linux operators using Homebrew cannot install/upgrade/uninstall
Tau through a standard package manager workflow tied to release artifacts.

## Scope

In scope:

- Add deterministic Homebrew formula rendering from release checksums.
- Publish rendered formula artifact from release workflow.
- Add contract tests validating formula generation and release workflow wiring.
- Add operator documentation for install/upgrade/uninstall flows.

Out of scope:

- Hosting a separate `homebrew-tap` repository.
- Supporting unsupported Homebrew targets beyond Linux/macOS amd64/arm64.
- Replacing existing install/update scripts.

## Acceptance Criteria

- AC-1: Given a release tag and checksum manifest, when rendering formula, then
  generated `tau.rb` includes macOS/Linux amd64/arm64 URLs and sha256 digests.
- AC-2: Given generated formula, when validating contract checks, then formula
  includes install path mapping to `tau-coding-agent` and a `test do` smoke.
- AC-3: Given release workflow execution for a tag, when checksums are produced,
  then Homebrew formula is rendered and published as a release asset.
- AC-4: Given operator docs, when using Homebrew, then install/upgrade/uninstall
  commands are documented with deterministic artifact references.

## Conformance Cases

- C-01 (AC-1, conformance): render script generates formula with all supported
  platform URLs + checksums.
- C-02 (AC-2, functional): contract test validates formula structure (`class`,
  `install`, `test do`, binary mapping).
- C-03 (AC-3, integration): release workflow contract asserts formula render and
  publish steps.
- C-04 (AC-4, documentation): docs include explicit install/upgrade/uninstall
  commands and generated formula path.

## Success Metrics / Observable Signals

- `scripts/release/test-homebrew-formula.sh` passes in local/CI runs.
- Release workflow contract tests pass with explicit formula publish assertions.
- Release guide and README mention Homebrew availability and usage commands.

# Spec #2263

Status: Accepted
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2263

## Problem Statement

Tau currently ships native release archives but no maintained container image.
Operators that standardize on container deployment cannot consume official Tau
artifacts, and release automation has no container publish path or runtime smoke
gate for image integrity.

## Scope

In scope:

- Add a production Docker image definition for `tau-coding-agent`.
- Add container smoke validation that proves image runtime entrypoint works.
- Add release automation to publish container images for release tags.
- Add documentation for build/run/pull flows and expected image tags.

Out of scope:

- Kubernetes manifests/Helm packaging.
- Non-GHCR registries.
- Runtime orchestration features unrelated to image packaging.

## Acceptance Criteria

- AC-1: Given repo source, when building the container image, then image build
  succeeds from first-party Dockerfile using release binary packaging.
- AC-2: Given a built image, when running container smoke command, then
  `tau-coding-agent --help` exits successfully.
- AC-3: Given pull requests that modify Docker packaging artifacts, when CI
  runs, then Docker build + smoke validation execute as a required check.
- AC-4: Given release tags (`v*`), when release workflow runs, then GHCR image
  tags are published for the release version and `latest`.

## Conformance Cases

- C-01 (AC-1, conformance): Dockerfile builds `tau-coding-agent` via multi-stage
  build and produces runnable runtime image.
- C-02 (AC-2, functional): smoke command runs container and validates `--help`
  output exit status.
- C-03 (AC-3, integration): CI workflow executes Docker package smoke path when
  Docker artifacts change.
- C-04 (AC-4, integration): release workflow publishes GHCR image tags derived
  from release tag + latest.

## Success Metrics / Observable Signals

- `docker build` + smoke script pass locally/CI for packaging changes.
- Release workflow includes successful image publish stage for tags.
- Repo docs include explicit container install/run guidance.

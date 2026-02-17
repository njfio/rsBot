# Plan #2263

Status: Reviewed
Spec: specs/2263/spec.md

## Approach

1. Create Docker packaging artifacts:
   - root `Dockerfile` (multi-stage build, release binary copy, non-root runtime)
   - `.dockerignore` to reduce build context and avoid leaking local artifacts.
2. Add deterministic smoke runner scripts for local/CI validation:
   - build image with a stable local tag
   - run `tau-coding-agent --help` inside container and assert success.
3. Add CI and release workflow wiring:
   - CI job for Docker build/smoke on relevant path changes.
   - Release workflow stage publishing GHCR tags (`vX.Y.Z`, `latest`) for amd64/arm64.
4. Update user/operator documentation with container usage and release behavior.

## Affected Modules

- `Dockerfile`
- `.dockerignore`
- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `scripts/dev/*` (container smoke helper)
- `docs/guides/release-automation-ops.md`
- `README.md`
- `specs/2263/*`

## Risks and Mitigations

- Risk: container image uses wrong runtime dependencies and fails to launch.
  - Mitigation: enforce smoke command gate in CI and release workflows.
- Risk: release publish tags drift from binary release tags.
  - Mitigation: derive image tags from existing `RELEASE_TAG` workflow env and
    add conformance assertions in workflow logic.
- Risk: CI runtime cost increase.
  - Mitigation: scope Docker CI execution to Docker/release path filters.

## Interfaces / Contracts

- CLI runtime contract unchanged (`tau-coding-agent` binary behavior preserved).
- New delivery contract:
  - Docker image build is first-party and reproducible from repo.
  - GHCR publish executes on release tags with stable tag mapping.

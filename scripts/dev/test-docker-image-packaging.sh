#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

if [[ ! -f Dockerfile ]]; then
  echo "error: missing Dockerfile" >&2
  exit 1
fi

if [[ ! -f scripts/dev/docker-image-smoke.sh ]]; then
  echo "error: missing scripts/dev/docker-image-smoke.sh" >&2
  exit 1
fi

bash -n scripts/dev/docker-image-smoke.sh

if ! rg -q '^FROM rust:.* AS builder$' Dockerfile; then
  echo "error: Dockerfile must define a rust builder stage" >&2
  exit 1
fi

if ! rg -q '^FROM .*debian.*$' Dockerfile; then
  echo "error: Dockerfile must define a debian runtime stage" >&2
  exit 1
fi

if ! rg -q 'tau-coding-agent --help' scripts/dev/docker-image-smoke.sh; then
  echo "error: docker smoke script must verify tau-coding-agent --help" >&2
  exit 1
fi

if ! rg -q 'docker_packaging' .github/workflows/ci.yml; then
  echo "error: ci workflow missing docker_packaging change scope" >&2
  exit 1
fi

if ! rg -q 'ghcr.io' .github/workflows/release.yml; then
  echo "error: release workflow missing ghcr publish wiring" >&2
  exit 1
fi

echo "docker image packaging contract tests passed"

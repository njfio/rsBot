#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

usage() {
  cat <<'USAGE'
Usage: scripts/dev/docker-image-smoke.sh [--tag <image-tag>] [--no-cache]

Builds Tau Docker image from repo Dockerfile and validates runtime entrypoint by
running tau-coding-agent --help in the built container.
USAGE
}

IMAGE_TAG="${TAU_DOCKER_SMOKE_TAG:-tau-coding-agent:dev-smoke}"
NO_CACHE="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      IMAGE_TAG="${2:-}"
      if [[ -z "${IMAGE_TAG}" ]]; then
        echo "error: --tag requires a value" >&2
        exit 1
      fi
      shift 2
      ;;
    --no-cache)
      NO_CACHE="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker is required for docker-image-smoke.sh" >&2
  exit 1
fi

docker_build_args=(build --pull -t "${IMAGE_TAG}" -f Dockerfile .)
if [[ "${NO_CACHE}" == "true" ]]; then
  docker_build_args=(build --pull --no-cache -t "${IMAGE_TAG}" -f Dockerfile .)
fi

echo "==> docker build ${IMAGE_TAG}"
docker "${docker_build_args[@]}"

echo "==> docker run smoke: tau-coding-agent --help"
docker run --rm --entrypoint tau-coding-agent "${IMAGE_TAG}" --help >/dev/null

echo "docker image smoke summary: status=pass image=${IMAGE_TAG}"

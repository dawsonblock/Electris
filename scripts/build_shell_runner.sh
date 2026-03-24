#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_TAG="${1:-electro-shell-runner:local}"
ENGINE="${CONTAINER_ENGINE:-docker}"

if ! command -v "$ENGINE" >/dev/null 2>&1; then
  echo "Container engine '$ENGINE' not found on PATH." >&2
  exit 1
fi

exec "$ENGINE" build \
  -f "$ROOT_DIR/docker/shell-runner.Dockerfile" \
  -t "$IMAGE_TAG" \
  "$ROOT_DIR"

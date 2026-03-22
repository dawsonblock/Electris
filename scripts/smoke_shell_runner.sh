#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_TAG="${1:-electro-shell-runner:local}"
ENGINE="${CONTAINER_ENGINE:-docker}"
WORKDIR="${2:-$ROOT_DIR}"

if ! command -v "$ENGINE" >/dev/null 2>&1; then
  echo "Container engine '$ENGINE' not found on PATH." >&2
  exit 1
fi

exec "$ENGINE" run --rm \
  --network none \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --read-only \
  --tmpfs /tmp:rw,noexec,nosuid,nodev,size=64m \
  --workdir /workspace \
  --mount "type=bind,src=$WORKDIR,dst=/workspace,rw" \
  "$IMAGE_TAG" \
  bash -lc 'set -e; printf "shell runner smoke test\n"; git --version; python3 --version; node --version; jq --version; rg --version; zip -v >/dev/null; unzip -v >/dev/null; sqlite3 --version; test -d /workspace'

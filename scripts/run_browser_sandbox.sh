#!/usr/bin/env bash
set -euo pipefail
docker compose -f docker-compose.browser-sandbox.yml up -d --build browser-proxy browser-sandbox
echo "browser sandbox listening on http://127.0.0.1:9223"

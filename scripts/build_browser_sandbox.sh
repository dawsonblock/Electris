#!/usr/bin/env bash
set -euo pipefail
docker compose -f docker-compose.browser-sandbox.yml build browser-proxy browser-sandbox

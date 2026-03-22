#!/usr/bin/env bash
set -euo pipefail

CHROME_BIN="${CHROME_BIN:-/usr/bin/chromium}"
USER_DATA_DIR="${CHROME_USER_DATA_DIR:-/home/chrome/profile}"
PROXY_SERVER="${CHROME_PROXY_SERVER:-http://browser-proxy:3128}"
PROXY_BYPASS="${CHROME_PROXY_BYPASS:-}"
HEADLESS_FLAG="${CHROME_HEADLESS_FLAG:---headless=new}"
REMOTE_ADDR="${CHROME_REMOTE_DEBUGGING_ADDRESS:-0.0.0.0}"
REMOTE_PORT="${CHROME_REMOTE_DEBUGGING_PORT:-9222}"

mkdir -p "$USER_DATA_DIR"

EXTRA_FLAGS=()
if [[ "${CHROME_NO_SANDBOX:-0}" == "1" ]]; then
  EXTRA_FLAGS+=(--no-sandbox)
fi

if [[ -n "$PROXY_SERVER" ]]; then
  EXTRA_FLAGS+=("--proxy-server=${PROXY_SERVER}")
fi
if [[ -n "$PROXY_BYPASS" ]]; then
  EXTRA_FLAGS+=("--proxy-bypass-list=${PROXY_BYPASS}")
fi

exec "$CHROME_BIN"       "$HEADLESS_FLAG"       --remote-debugging-address="$REMOTE_ADDR"       --remote-debugging-port="$REMOTE_PORT"       --user-data-dir="$USER_DATA_DIR"       --disable-gpu       --disable-dev-shm-usage       --disable-background-networking       --disable-sync       --metrics-recording-only       --disable-default-apps       --no-default-browser-check       --no-first-run       --no-pings       --disable-component-update       --disable-features=AutofillServerCommunication,CertificateTransparencyComponentUpdater,DialMediaRouteProvider,MediaRouter       "${EXTRA_FLAGS[@]}"       about:blank

#!/usr/bin/env bash
set -euo pipefail
curl --fail --silent http://127.0.0.1:9223/json/version | python3 - <<'PY'
import json, sys
data = json.load(sys.stdin)
ws = data.get("webSocketDebuggerUrl")
browser = data.get("Browser")
if not ws:
    raise SystemExit("missing webSocketDebuggerUrl")
print("remote browser ok")
print(browser)
print(ws)
PY

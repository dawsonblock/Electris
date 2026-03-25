#!/usr/bin/env bash
set -e

BASE_URL="${BASE_URL:-http://localhost:8080}"

echo "=== Testing Spine API ==="
echo "Base URL: $BASE_URL"
echo ""

# Health check
echo "1. Health check"
curl -s "$BASE_URL/health" | jq .
echo ""

# Execute simple task
echo "2. Execute task"
curl -s -X POST "$BASE_URL/execute" \
  -H "Content-Type: application/json" \
  -d '{
    "payload": {
      "action": "test",
      "data": "hello world"
    }
  }' | jq .
echo ""

# Execute with different actions
echo "3. Execute with action 'analyze'"
curl -s -X POST "$BASE_URL/execute" \
  -H "Content-Type: application/json" \
  -d '{
    "payload": {
      "action": "analyze",
      "target": "src/main.rs"
    }
  }' | jq .
echo ""

echo "=== Done ==="

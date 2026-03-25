#!/usr/bin/env bash
set -e

echo "== Spine Verification =="

echo ""
echo "== cargo check =="
cargo check --workspace

echo ""
echo "== cargo test =="
cargo test --workspace

echo ""
echo "== clippy =="
cargo clippy --workspace -- -D warnings

echo ""
echo "== Single entrypoint check =="
RUNTIME_PUB=$(grep -r "^pub async fn\|^pub fn" crates/spine-runtime/src/*.rs 2>/dev/null | grep -v "pub(crate)" | wc -l)
echo "Public functions in spine-runtime: $RUNTIME_PUB"
if [ "$RUNTIME_PUB" -eq 1 ]; then
    echo "✅ Single entrypoint verified (submit_intent)"
else
    echo "⚠️  Expected 1 public function in runtime"
fi

echo ""
echo "== Done =="

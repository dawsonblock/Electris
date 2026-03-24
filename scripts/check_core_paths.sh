#!/usr/bin/env bash
set -euo pipefail

# Core integrity checks for Electris runtime stabilization
# Usage: bash scripts/check_core_paths.sh

printf '\n== Git Conflict Markers (exact match) ==\n'
# Use exact Git conflict marker patterns (anchored)
rg -n '^(<<<<<<< |=======$|>>>>>>> )' . --type rust --type toml 2>/dev/null || echo "No conflict markers found - OK"

printf '\n== Agent Execution Calls ==\n'
echo "Active process_message calls (should only be in worker, worker-node, tests, agent internals):"
rg -n 'process_message\(' src crates --type rust | grep -v "test" | grep -v "#\[test" | head -20 || true

printf '\n== Direct Host Command Execution ==\n'
echo "Command::new occurrences (audit for sandbox bypasses):"
rg -n 'Command::new' src crates --type rust | head -20 || true

printf '\n== Gateway Router Check ==\n'
if rg -n 'process_message' crates/electro-gateway/src --type rust > /dev/null 2>&1; then
    echo "WARNING: electro-gateway still contains process_message calls"
    rg -n 'process_message' crates/electro-gateway/src --type rust || true
else
    echo "OK: electro-gateway has no direct process_message calls"
fi

printf '\n== TUI Bridge Check ==\n'
if rg -n 'process_message' crates/electro-tui/src --type rust > /dev/null 2>&1; then
    echo "WARNING: electro-tui still contains process_message calls"
    rg -n 'process_message' crates/electro-tui/src --type rust || true
else
    echo "OK: electro-tui has no direct process_message calls"
fi

printf '\nDone.\n'

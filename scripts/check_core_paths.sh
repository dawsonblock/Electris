#!/usr/bin/env bash
set -euo pipefail

printf '\n== merge markers ==\n'
rg -n '<<<<<<<|=======|>>>>>>>' . || true

printf '\n== agent execution calls ==\n'
rg -n 'process_message\(' src crates || true

printf '\n== direct host command execution ==\n'
rg -n 'Command::new' src crates || true

printf '\n== direct output paths ==\n'
rg -n 'println!|eprintln!|send_message' src/app crates/electro-agent crates/electro-runtime crates/electro-gateway || true

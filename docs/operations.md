# Operations Notes

## Before calling this build stable

Run on a machine with Rust installed:

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features
```

## Core audits

```bash
rg -n '<<<<<<<|=======|>>>>>>>' .
rg -n 'process_message\(' src crates
rg -n 'Command::new' src crates
rg -n 'println!|eprintln!|send_message' src/app crates/electro-agent crates/electro-runtime crates/electro-gateway
```

## Acceptance gates

- no merge conflict markers in source files
- successful build
- core tests passing
- live runtime warning count is low and explainable
- one gateway, one execution path, one event model, one sandbox boundary

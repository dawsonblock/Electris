# Build Verification TODO

This archive was produced in an environment without Rust tooling, so it was not compile-verified here.

## Required on a Rust-enabled machine

1. `cargo check`
2. `cargo test`
3. `cargo clippy --all-targets --all-features`
4. run `scripts/check_core_paths.sh`
5. review remaining warnings in the live core only

## If build breaks

Prioritize fixes in this order:
- unresolved imports
- signature mismatches
- duplicate definitions
- references to parked or duplicate surfaces
- warning cleanup after successful build

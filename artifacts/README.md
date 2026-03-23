# artifacts/ — Historical Build Logs

> **These files are snapshots from a prior version of the repository and do not reflect the current source tree.**

The log files in this directory (`fmt.log`, `check.log`, `clippy.log`, `test.log`, etc.) were captured during an earlier phase of development. They should not be used to assess the current build health or code quality.

## What these files are

| File | Description |
|------|-------------|
| `fmt.log` | Output from `cargo fmt --check` against a previous source snapshot |
| `check.log` | Output from `cargo check` against a previous source snapshot |
| `clippy.log` | Output from `cargo clippy` against a previous source snapshot |
| `test.log` | Output from `cargo test` against a previous source snapshot |

## How to get current build status

Run against the live source tree:

```bash
# Format check
cargo fmt --all -- --check

# Type check
cargo check --workspace

# Lint
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Tests
cargo test --workspace
```

CI results from the current tree are authoritative. See `.github/workflows/ci.yml` for the full configuration.

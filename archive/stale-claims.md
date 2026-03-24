# Stale Claims Archive

**Claims that have been removed or corrected from the codebase.**

## Removed Claims

### "Production-grade"
**Status:** Removed from README
**Reason:** The codebase is operational but not production-ready. Tool sandbox is partial, TUI bypasses queue, and several features are experimental.

### "Zero warnings"
**Status:** Corrected
**Reason:** Build currently has style warnings (clippy). Core compiles with zero errors.

### "Fully sandboxed"
**Status:** Corrected to "Partial"
**Reason:** Host execution is still available (behind validation). Full sandbox requires Docker/Podman backend.

### "Fully unified runtime"
**Status:** Corrected
**Reason:** TUI agent bridge still calls `process_message` directly instead of using queue/dispatcher/worker spine.

### "Stable release"
**Status:** Removed
**Reason:** This is a stabilization branch, not a release. See `docs/status.md` for actual capability status.

### "Browser automation included"
**Status:** Disabled
**Reason:** htmd dependency requires Rust 1.88+, currently on 1.85.0.

## Outdated Documentation

The following documents may contain stale information:
- `COMPLETE_FIX_PLAN.md` - May reference completed work
- `CORRECTED_BUILD_NOTES.md` - Historical context, not current state
- `docs/operations.md` - May have outdated conflict scan instructions

## Verification

Current truth sources:
- `docs/status.md` - Operational status
- `docs/ownership.md` - Architecture ownership
- `scripts/check_core_paths.sh` - Verification script
- `cargo check` - Build status
- `cargo test` - Test status

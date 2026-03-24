# Stale Claims Archive

This file tracks claims that were previously made about Electris but have been corrected or removed.

## Removed Claims

### "Stable" / "Production-Grade"
**Previous claim**: Version badges and documentation described the project as "Stable" or "production-grade".

**Status**: Removed.

**Current status**: "Operational Beta" - the core runtime is functional and tested, but the project is still evolving.

**Rationale**: The term "stable" implies a level of maturity and API stability that wasn't accurate. The runtime is operational but still under active development.

---

### "Clippy Warnings: 0"
**Previous claim**: Documentation claimed zero clippy warnings.

**Status**: Corrected.

**Current status**: "Core clean, experimental has warnings"

**Rationale**: While the core stabilization set is relatively clean, experimental modules and some edge cases generate warnings. Claiming zero was inaccurate.

---

### "Fully Sandboxed"
**Previous claim**: Implied all tool execution was fully sandboxed.

**Status**: ✅ RESOLVED.

**Current status**: "All live tool execution goes through canonical sandbox"

**Rationale**: Shell and git tools now route through `runner.rs` with policy → validation → sandbox runner → output cap → audit event chain. The only remaining `Command::new` calls are in admin utilities (outside request path) and container engines (which provide their own isolation).

---

### "Hive: Operational"
**Previous claim**: Hive (multi-agent swarm) was listed as operational.

**Status**: Corrected.

**Current status**: "⚠️ Feature-gated" - Experimental, not in default build

**Rationale**: Hive is functional but not part of the core stabilization set. It's now feature-gated and requires explicit opt-in.

---

## Corrected Architecture Claims

### TUI Execution
**Previous claim**: TUI was a normal adapter.

**Issue**: TUI was calling `agent.process_message()` directly in a spawned task, bypassing the queue.

**Correction**: Refactored to pure adapter pattern:
- TUI creates `InboundMessage`
- Enqueues via `RuntimeHandle`
- Worker executes agent
- TUI subscribes to `OutboundEvent` for UI updates

**Verification**: `rg 'process_message' crates/electro-tui/src --type rust` now only shows the worker task.

---

### Output Unification
**Previous claim**: Output was unified through events.

**Issue**: Multiple direct output paths existed:
- `commands.rs` used `sender.send_message()` directly
- `dispatcher.rs` used `sender.send_message()` for overload and busy messages
- `router.rs` used `sender.send_message()` for stop confirmations

**Correction**: All converted to use `OutboundEvent`:
- `commands.rs` → `OutboundEvent::Completed`
- `dispatcher.rs` → `OutboundEvent::Failed` (overload)
- `router.rs` → `OutboundEvent::Completed` (stop)
- Busy interceptor → `OutboundEvent::Completed`

**Verification**: `rg 'send_message' src/app/server --type rust` now only shows trait definitions and adapter implementations.

---

## Why This Archive Exists

Transparency about past claims helps:
1. **Track evolution**: Show how the project has matured
2. **Prevent regression**: Document what was fixed and why
3. **Build trust**: Honest accounting of past overclaims
4. **Guide future**: Remind contributors to verify claims

## Adding New Entries

When correcting a claim:
1. Document the previous claim
2. Explain why it was inaccurate
3. State the current corrected status
4. Provide verification method
5. Date the correction

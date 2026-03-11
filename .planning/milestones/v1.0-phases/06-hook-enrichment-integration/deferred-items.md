# Deferred Items — Phase 06

## Pre-existing Test Failures (Out of Scope)

### init_integration.rs (3 tests)

- `test_init_writes_hooks`
- `test_init_idempotent`
- `test_init_preserves_other_hooks`

**Status:** Failing before Plan 06-03 work began. Verified by stashing changes and running the tests independently.

**Error:** Tests expect PostToolUse hooks to have 3 entries (Read, Edit|Write, Grep) but the current init logic only registers 1 entry. This suggests the multi-hook registration logic in `src/cli/init.rs` is not yet implementing the 3-entry merge that the tests expect.

**Scope:** These failures are in `init_integration.rs` and relate to hook registration logic that is not part of Plan 06-03's scope (integration tests for hook enrichment HTTP handlers and token savings wiring).

**Action needed:** Investigate and fix `cli/init.rs` hook registration to write 3 PostToolUse entries (Read, Edit|Write, Grep). This may be part of a future plan in this phase or a follow-up.

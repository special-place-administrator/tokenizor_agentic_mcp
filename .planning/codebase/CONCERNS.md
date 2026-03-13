# Codebase Concerns

**Analysis Date:** 2026-03-14

## Tech Debt

**V1 Feature Gate — Legacy Test Compatibility:**
- Issue: `Cargo.toml` maintains a `v1` feature gate for backward compatibility with `retrieval_conformance.rs` tests, which depend on old domain types not present in v2
- Files: `Cargo.toml` (line 49-51), `tests/retrieval_conformance.rs`
- Impact: Code includes dead/unused feature gate that adds maintenance burden. Tests will need rewriting for v2 response format in Phase 2
- Fix approach: Complete Phase 2 milestone (Intelligence) which rewrites retrieval_conformance tests. Then remove the `v1` feature gate and code paths that depend on it

**RwLock Poison Handling — Silent Failures:**
- Issue: All `RwLock` access throughout the codebase uses `.lock().unwrap()` or `.read().unwrap()` patterns (`src/live_index/store.rs`, `src/daemon.rs`). Poisoned locks will panic instead of gracefully degrading
- Files: `src/live_index/store.rs` (line 5, 349, 357), `src/daemon.rs` (widespread use), `src/sidecar/handlers.rs` (lock interactions)
- Impact: Single-threaded panic in any RwLock holder brings down the entire MCP server. In long-running daemon scenarios, this is a production reliability issue
- Fix approach: Wrap all `.unwrap()` calls with explicit poison handling that logs and returns error state. Consider using `parking_lot::RwLock` which doesn't poison on panic, or implement custom recovery logic that treats poisoned locks as stale state

**Loose Parsing Error Strategy — Silent Data Loss:**
- Issue: File parse failures record only a summary message in `ParseStatus::Failed` but discard the actual parse result tree. References may be extracted from a partially-corrupt AST
- Files: `src/live_index/store.rs` (line 66-75), `src/domain/index.rs` (FileOutcome enum), `src/parsing/mod.rs`
- Impact: In files with syntax errors, extracted references may include false positives or miss real references. Tools like `find_references` could return misleading results
- Fix approach: Retain the partial AST even on failure, store syntax error details alongside symbols, and explicitly mark references extracted from error states. Consider adding a confidence score to ReferenceRecord

## Known Bugs

**Definition-Site Reference Filtering — Byte Range Collision:**
- Symptoms: References whose byte range exactly matches a symbol's byte range are filtered out as "definition sites", but in languages with complex syntax (decorators, annotations, visibility modifiers), this filter may over-aggressively exclude valid references
- Files: `src/live_index/store.rs` (line 91-106, "Pitfall 1" comment)
- Trigger: Parse a Python class with decorators or Java class with annotations. The decorator/annotation location may have the same byte_range as the symbol definition
- Workaround: Manually inspect the bytewise symbol ranges if results seem incomplete
- Fix: Refine the filter to check not just byte_range equality but also context (line number, enclosing node type) to distinguish true definitions from decorated definitions

**Git Temporal Computation Timing — No Deadline:**
- Symptoms: `spawn_git_temporal_computation` spawns an async background task that may run indefinitely on large repos with deep history. No timeout. If git2 library hangs, the task hangs forever
- Files: `src/live_index/git_temporal.rs` (line 32-73, `spawn_git_temporal_computation`)
- Trigger: Clone a large monorepo (>1M commits). Run `git log` analysis
- Workaround: Set `git2` to read-only shallow clones or limit commits server-side
- Fix: Add a timeout (`tokio::time::timeout`) around the spawn_blocking call. Default 30 seconds. Make configurable via env var

**Missing Synchronization in Symbol Search — Race on Published State:**
- Symptoms: `PublishedIndexState` is updated independently from the live index. Between write lock release and published_state update, queries may see inconsistent state (new files indexed but old symbol counts)
- Files: `src/live_index/store.rs` (PublishedIndexState management), `src/daemon.rs` (session state updates)
- Trigger: Multiple edits in quick succession while queries are in flight
- Workaround: Single-threaded workloads or coordinated client-side throttling
- Fix: Wrap the entire "update live index → recompute published_state → swap" in a single write lock scope

## Security Considerations

**File Path Traversal — No Normalization on User Input:**
- Risk: Tool handlers like `get_file_content`, `search_symbols` accept user-supplied file paths. Paths like `../../etc/passwd` are not explicitly normalized
- Files: `src/protocol/tools.rs` (handlers that take `path` parameter), `src/live_index/query.rs` (path resolution)
- Current mitigation: Index is scoped to repo root, so `../../` would only escape within the repo boundary. Filesystem operations use `std::fs` which rejects absolute paths. But there's no explicit path canonicalization
- Recommendations:
  1. Add `Path::canonicalize()` on all user-supplied paths before use
  2. Verify that canonical path starts with repo root, reject if not
  3. Document this assumption in tool descriptions

**HTTP Sidecar — No Authentication:**
- Risk: The sidecar (`src/sidecar/server.rs`) binds to `127.0.0.1` and serves HTTP endpoints like `/outline`, `/symbol-context`. Any local process can query
- Files: `src/sidecar/server.rs` (line ~100, server bind), `src/sidecar/handlers.rs` (endpoint handlers)
- Current mitigation: Binds to localhost only, port dynamically allocated and written to `.tokenizor/daemon.port`. No public internet exposure
- Recommendations: Consider adding HMAC-based request signing if the sidecar is ever used in multi-tenant environments. Document that sidecar is local-only

**Git Credentials Exposure — libgit2 May Read SSH Keys:**
- Risk: The git2 crate uses libgit2 which may attempt to load SSH keys from `~/.ssh` when accessing git repositories. No way to prevent this
- Files: `src/git.rs` (GitRepo implementation), `Cargo.toml` (git2 dependency with vendored-libgit2)
- Current mitigation: SSH/HTTPS features disabled in git2 to remove OpenSSL dependency (see v0.20.1 fix), so git operations are read-only to local refs
- Recommendations: If SSH key access becomes needed, carefully audit git2 credential handling and consider running in a sandboxed environment

## Performance Bottlenecks

**In-Memory Symbol Storage — Unbounded Memory Growth:**
- Problem: All file content bytes are stored in `IndexedFile::content` (Vec<u8>). For large repositories (50K+ files), this can cause multi-gigabyte memory usage
- Files: `src/live_index/store.rs` (line 36-37, IndexedFile struct), `docs/ROADMAP-v2.md` (line 527 acknowledges this)
- Cause: Design trade-off for zero-copy, zero-disk-I/O retrieval. Every byte of every indexed file stays in RAM
- Improvement path:
  1. Short-term: Add `max_file_size` filter (skip files >10MB) and soft memory budget with per-file eviction (keep symbols, drop content for untouched files)
  2. Medium-term: Implement file-level eviction policy (LRU) for content bytes while keeping symbol indices
  3. Long-term: Move to tiered storage (hot/cold files) or SpacetimeDB backend

**Cross-Reference Query — No Index on Reference Kind:**
- Problem: `find_references` and `find_dependents` iterate through all references in all files. For large repos, this is O(all_references)
- Files: `src/live_index/query.rs` (line ~500+, find_references implementation), `src/live_index/store.rs` (reverse_index HashMaps)
- Cause: No secondary index by ReferenceKind or symbol name to narrow result set
- Improvement path: Add a `references_by_kind` index (ReferenceKind → HashMap of name → ReferenceLocations) to skip scanning irrelevant references

**Git Temporal Computation — Blocks on I/O:**
- Problem: `git_temporal.rs` spawns a background task using `spawn_blocking`, which runs on a separate thread pool. For very large histories, this can starve other blocking tasks
- Files: `src/live_index/git_temporal.rs` (line 48-51)
- Cause: No priority or queueing system for long-running background tasks
- Improvement path: Add task queue with priority levels, or make git temporal computation incremental (compute top N hotspots first, backfill rest asynchronously)

## Fragile Areas

**Symbol Selector Matching — Line-Based Ambiguity:**
- Files: `src/live_index/query.rs` (resolve_symbol_selector), `src/protocol/edit.rs` (resolve_or_error)
- Why fragile: Resolves symbols by name + optional kind + optional line. Line numbers are 0-indexed internally but 1-indexed in user-facing output. Off-by-one errors are common
- Safe modification: Always test with multi-line symbol definitions (classes, functions with decorators) and verify line number consistency through the entire stack (user input → internal representation → output). Add integration tests that specifically exercise line boundary conditions
- Test coverage: `tests/sidecar_integration.rs` covers basic cases but lacks edge case scenarios (nested symbols, decorator lines, inline comments)

**Edit Operations — Byte Range Calculations:**
- Files: `src/protocol/edit.rs` (apply_splice, line 17-25), indentation detection (line 91-100)
- Why fragile: All edits work at the byte level. Multi-byte UTF-8 characters, CR/LF line endings, and tabs can cause misalignment between user-facing line:column and internal byte ranges
- Safe modification: Before modifying any edit operation, add fuzz testing with mixed line endings and non-ASCII characters. Test on Windows (CRLF) and Unix (LF)
- Test coverage: Basic ASCII testing in `tests/sidecar_integration.rs` but no UTF-8, CRLF, or mixed-encoding tests

**Reference Extraction — Language-Specific Patterns:**
- Files: `src/parsing/xref.rs` (tree-sitter query extraction), `src/parsing/languages/*.rs` (per-language patterns)
- Why fragile: Each language has custom tree-sitter query patterns. Changes to a single language's extractor can silently break reference detection
- Safe modification: After any change to xref extraction, run `tests/xref_integration.rs` on real codebases. Manually spot-check a few key symbols to ensure references are found
- Test coverage: Integration tests exist but only test basic cases. No regression tests for known false negatives (e.g., dynamic imports, reflection-based calls)

**Watcher Event Handling — Platform-Specific Timing:**
- Files: `src/watcher/mod.rs` (event processing loop), `src/live_index/mod.rs` (watcher integration)
- Why fragile: File watcher relies on OS filesystem events which have different guarantees across Windows/macOS/Linux. Events can be coalesced, delayed, or missed under heavy I/O load
- Safe modification: Test on multiple platforms. Add delays/retries for file stat checks if event-based update fails
- Test coverage: `tests/watcher_integration.rs` tests basic scenarios but doesn't cover high-concurrency writes or network filesystems

## Scaling Limits

**Memory Usage — Current Limit ~50MB Source:**
- Current capacity: Comfortably indexes ~10,000 files (~50MB source code)
- Limit: At ~100,000 files (~500MB source), memory usage becomes problematic. OOM likely at 1,000,000+ files
- Scaling path:
  1. Add file-level eviction: keep symbols, drop content bytes for files not accessed in last hour
  2. Implement lazy-loading: load file content on first access, not at startup
  3. Consider SpacetimeDB backend for very large repos (deferred to Phase 5+)

**Query Latency — Not Measured Under Load:**
- Current assumption: All queries <1ms from in-memory index (per ROADMAP-v2.md line 50)
- Scaling issue: This hasn't been benchmarked with 100K+ symbols or 50K+ files. Lock contention under heavy concurrent queries could push this higher
- Scaling path: Add latency instrumentation. Profile under simulated load (1000+ concurrent requests). If needed, switch to lock-free data structures or shard the index

**Git History Analysis — No Bounds on Commits:**
- Current limit: `MAX_COMMITS = 500`, `WINDOW_DAYS = 90` (src/live_index/git_temporal.rs line 77-78)
- Issue: These are hard-coded constants. For very active repos (>500 commits/month), the window is too short to capture meaningful patterns
- Scaling path: Make these configurable via env vars. Consider adaptive windowing based on commit frequency

## Dependencies at Risk

**tree-sitter Grammar Versions — Pinned, May Fall Behind:**
- Risk: All tree-sitter language grammars are pinned to specific versions (`tree-sitter-rust = "0.24"`, etc. in Cargo.toml). If upstream fixes a critical bug, we won't see it
- Impact: Symbol extraction or cross-references could be incorrect for newly-written code if grammar lags
- Migration plan: Monitor grammar repositories monthly. Update quarterly or on critical bug fixes. Add regression tests when updating

**git2 Vendored Libgit2 — Build Complexity:**
- Risk: `git2` with `vendored-libgit2` feature compiles a C library from source. This adds build time and introduces platform-specific compilation issues
- Impact: CI/CD becomes slower. Windows MSVC builds are particularly fragile
- Migration plan: Monitor git2 releases. If vendored libgit2 causes too many issues, consider replacing with `gitoxide` (pure Rust) or simple `std::process::Command` calls to system git

## Missing Critical Features

**No Symbol Type Information — References Are String-Based:**
- Problem: Cross-references match by symbol name only. Can't distinguish between a function `foo()` and a variable `foo`. This causes false positive matches
- Blocks: Precise impact analysis, type-aware refactoring, accurate dependency graphs
- Implementation: Would require integrating a language server (rust-analyzer, pyright) or implementing type inference. Large effort

**No Incremental Git Temporal Updates — Recomputes on Every Reload:**
- Problem: `git_temporal.rs` recomputes the entire temporal index on every index reload, even if only 1 file changed
- Blocks: Fast reload times for large repos with deep history
- Implementation: Cache temporal data per-file, only recompute files in the changeset

**No Pruning of Transient References — Symbol Aliasing Creates Duplicate Results:**
- Problem: In Rust, `use foo::bar; bar();` creates a reference to `bar` that also implicitly references `foo`. Both show up in `find_references` results
- Blocks: Clean reference lists, ability to remove unused imports
- Implementation: Add re-export tracking and prune transitive reference chains

## Test Coverage Gaps

**Integration Tests — Limited Real-World Scenarios:**
- What's not tested:
  - Large repos (>10K files)
  - Files with syntax errors during watcher updates
  - Concurrent edits while queries are in flight
  - Git histories with >1000 commits
  - Mixed line endings (CRLF on Windows)
  - Symlinks and unusual filesystem layouts
- Files: `tests/` directory (7 integration tests, ~7K lines total)
- Risk: High-probability bugs in real-world usage scenarios
- Priority: High — Add at least one "large repo" benchmark test. Add concurrent edit + query test. Add CRLF/UTF-8 edge case tests

**Unit Tests on Private Functions — Format/Query Logic Untested:**
- What's not tested:
  - `src/protocol/format.rs` functions (100+ formatter functions)
  - `src/live_index/search.rs` trigram and relevance logic
  - Error paths (what happens when lock is poisoned, file is deleted mid-query)
- Files: `src/protocol/format.rs` (4,835 lines, no visible unit tests), `src/live_index/search.rs` (1,764 lines, only inline test data)
- Risk: Format changes break tool output contracts silently
- Priority: Medium — Add doctests to formatter functions. Add parametric tests for search scoring

**End-to-End MCP Protocol Tests — Only Tool Inputs Tested:**
- What's not tested:
  - Full MCP request/response cycle (JSON serialization, tool schema validation)
  - MCP error responses (tools returning errors)
  - Prompt resource generation
  - Resource URI resolution
- Files: `tests/sidecar_integration.rs` tests HTTP endpoints, but no tests for MCP protocol layer (`src/protocol/mod.rs`)
- Risk: Tools may work in HTTP sidecar but fail when called via MCP protocol
- Priority: High — Add tests that call tools via rmcp library, verify schema compliance

---

*Concerns audit: 2026-03-14*

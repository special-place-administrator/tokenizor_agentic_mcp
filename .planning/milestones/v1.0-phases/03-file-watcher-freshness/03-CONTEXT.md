# Phase 3: File Watcher + Freshness - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

The LiveIndex always reflects current disk state — queries never return stale symbols after any file change. This phase adds a file watcher (notify crate), incremental single-file reparse, and adaptive debouncing. No cross-references, no hooks, no HTTP sidecar — those are later phases.

</domain>

<decisions>
## Implementation Decisions

### Debounce strategy
- Adaptive debounce: start at 200ms, extend to 500ms on burst detection
- Burst threshold: if >3 events for the same file within the 200ms window, extend that file's next debounce to 500ms
- Resets after a quiet period (e.g., 5s with no events for that file)
- Use `notify-debouncer-full` as the debouncing layer on top of `notify`
- content_hash always checked before re-parsing — if file bytes hash to same value, skip tree-sitter parse entirely (eliminates false re-indexes from metadata-only events or editor no-op writes)
- Rename events treated as delete + create (no special rename tracking). notify-debouncer-full already consolidates From+To events.

### Watcher lifecycle
- Watcher auto-starts immediately after initial index load completes
- Controlled by same TOKENIZOR_AUTO_INDEX env var — if auto-index is off, watcher is off too
- Calling index_folder also starts (or restarts) the watcher
- Watches repo root recursively (RecursiveMode::Recursive) — handles new directories automatically
- Only processes events for files with supported language extensions (6 languages)

### Failure behavior
- Auto-restart with 1s backoff on watcher errors (OS limit hit, watch path deleted, internal panic)
- After 3 consecutive restart failures, enter degraded mode — index is stale but queries still work
- health tool reports watcher state (active/degraded/off)
- index_folder resets watcher from degraded mode — attempts fresh watcher start. If fails again, back to degraded.
- File deletion during re-index (ENOENT): remove entry from LiveIndex silently, log warning. No crash, no error propagation to queries.

### Health stats extension
- health tool adds: watcher_state (active/degraded/off), events_processed count, last_event_at timestamp, debounce_window_ms
- Gives the model visibility into freshness status

### Change tracking
- Watcher updates LiveIndex's loaded_at_system timestamp on each file update
- what_changed(since) picks up watcher-triggered changes naturally via existing timestamp comparison

### Logging
- INFO level: watcher started/stopped/restarted, degraded mode entered/exited
- DEBUG level: individual file re-index events (path + duration + hash-skip/reparse)
- WARN level: watcher failures, restart attempts, file read errors
- Quiet at default info level — no noise during active editing sessions

### Claude's Discretion
- Exact notify-debouncer-full configuration and channel type (std::sync::mpsc vs crossbeam vs tokio channel)
- Internal structure of the watcher module (single file vs mod.rs + submodules)
- How the watcher task communicates with SharedIndex (channel vs direct Arc access)
- Windows path normalization implementation details (ReadDirectoryChangesW C:\ paths → forward-slash relative paths)
- Exact backoff strategy for watcher restart (linear vs exponential)

</decisions>

<specifics>
## Specific Ideas

- The adaptive debounce should feel invisible — the model never sees stale data, but also never gets duplicate re-index noise
- content_hash skip is the key optimization: most editor saves trigger 3-6 OS events, but only the first one with actual content changes needs parsing
- Windows path normalization is a known blocker from STATE.md — needs explicit handling and Windows-specific test

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `IndexedFile::from_parse_result(result, bytes)`: Creates indexed file from parse result + raw bytes — ready for single-file use
- `parsing::process_file(relative_path, bytes, language)`: Single-file parser — exactly what the watcher callback needs
- `IndexedFile.content_hash`: SHA-256 hash already computed during parsing — compare against new file bytes to skip no-op re-indexes
- `DiscoveredFile.language` + `LanguageId::from_extension()`: File classification by extension — reuse for filtering watcher events to supported languages only
- `discovery::find_git_root()`: Returns repo root path — use as watcher root

### Established Patterns
- `SharedIndex = Arc<RwLock<LiveIndex>>`: Watcher needs write lock for single-file updates. Current pattern: read lock for queries, write lock for reload. Single-file update is a new write-lock code path.
- `CircuitBreakerState`: Watcher-triggered parses should NOT count toward circuit breaker (CB is for initial load failures). Watcher failures are individual and recoverable.
- `tracing` on stderr with env-filter — continue for watcher logging

### Integration Points
- `LiveIndex` needs new method: `update_file(path, indexed_file)` and `remove_file(path)` — currently only has full `reload()`
- `LiveIndex` needs new method: `add_file(path, indexed_file)` for newly created files
- `HealthStats` struct needs watcher fields: state, events_processed, last_event_at, debounce_window_ms
- `src/main.rs` needs to spawn watcher task after initial load, passing SharedIndex
- `Cargo.toml` needs: `notify` (v8.x) and `notify-debouncer-full` (v0.7.x)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 03-file-watcher-freshness*
*Context gathered: 2026-03-10*

# Phase 6: Hook Enrichment Integration - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

After every native Read, Edit, Grep, and Write call, the model automatically receives symbol context injected by hook scripts without changing its behavior. Delivers PostToolUse hooks for Read/Edit/Write/Grep, a SessionStart hook for the repo map, token budget enforcement, token savings tracking, and stdin JSON parsing to replace the Phase 5 env var shim.

Requirements: HOOK-04, HOOK-05, HOOK-06, HOOK-07, HOOK-08, HOOK-09, INFR-01, INFR-04.

</domain>

<decisions>
## Implementation Decisions

### Read hook content (HOOK-04)
- Inject symbol outline (like get_file_outline) plus "Key references" section showing the 3-5 most-referenced symbols' callers
- Reference selection: rank symbols by caller count descending, take top 3-5 (fit within ~80 token sub-budget), show up to 3 callers per symbol, skip symbols with 0 callers
- Non-source files (`.json`, `.toml`, `.md`): silent no-op — output empty additionalContext, no noise
- One-line footer: `[~N tokens saved]` at the end of every hook response

### Edit hook impact (HOOK-05)
- Two-section output: "Changed symbols" diff + "Callers to review"
- Changed symbols: compare pre-edit and post-edit symbol lists by name+kind. Added/removed symbols shown explicitly. "Changed" = same name but different line range or byte count (signature/body changed)
- Callers to review: show callers only for Changed and Removed symbols
- Detection: sidecar caches per-file symbol list; on `/impact` call, re-indexes the file immediately (not waiting for watcher), computes diff against cached list, returns impact JSON
- Hook triggers re-index explicitly (POST to `/impact`), guaranteeing fresh data even if watcher hasn't caught up within its 200ms debounce window. ~30ms re-index + ~5ms diff = well within 50ms HTTP budget.

### Write hook (HOOK-06)
- Index confirmation only — new files have no callers yet
- Output: language detected, symbol count with kind breakdown, `[Indexed, 0 callers yet]` footer
- Separate from Edit hook — Write creates new files, Edit modifies existing ones

### Grep hook context (HOOK-07)
- Enclosing symbol name annotation: for each matched line, show which symbol (function/method/class) contains it
- Non-source file matches: skip silently — only annotate matches in indexed source files
- Input: parse tool_input only from stdin JSON (extract pattern + path), send to sidecar which searches its own index and returns symbol context
- Cap at 10 annotated matches, then `... (showing N of M matches)`. Keeps within ~100 token budget even for broad patterns.

### SessionStart repo map (HOOK-08)
- Directory tree + symbol counts: show directory structure (2 levels max) with file counts and symbol counts per directory
- Language breakdown header: project name, total source files, total symbols, language counts
- For dirs with >5 subdirs: aggregate (e.g., `12 subdirs, 89 files`)
- ~500 token budget

### Token budget enforcement (HOOK-09)
- Hardcoded defaults: 200 (Read), 150 (Edit), 100 (Grep), 500 (SessionStart)
- Enforcement happens in the sidecar (server-side) via `max_tokens` query parameter
- Truncation strategy: truncate at nearest logical boundary (end of symbol entry, end of reference group), append `... (truncated, N more)`
- Hook binary receives already-budgeted content, just formats into additionalContext

### Stdin JSON parsing
- Minimal serde parse: deserialize stdin JSON into small struct with just `tool_name` and `tool_input` fields. All fields `Option<T>`, fail-open if missing or schema changes.
- Extract file path from `tool_input.file_path` (Read/Edit/Write) or `tool_input.pattern` + `tool_input.path` (Grep)
- Stdin `tool_name` drives routing: single `tokenizor hook` command (no subcommands needed), routes to /outline, /impact, /symbol-context based on tool_name
- Old subcommands (tokenizor hook read/edit/grep) still work for manual testing — subcommand takes precedence over stdin if provided
- SessionStart uses dedicated `tokenizor hook session-start` subcommand

### tokenizor init (INFR-01)
- Registers 2 hook entries in `~/.claude/settings.json` (user-global): one PostToolUse (`tokenizor hook`) + one SessionStart (`tokenizor hook session-start`)
- Auto-migration: if old Phase 5 subcommand entries exist (tokenizor hook read/edit/grep), remove them and replace with single stdin-routed entry
- Idempotent: running twice produces identical result
- Preserves all non-tokenizor hooks

### Token savings tracking (INFR-04)
- Byte-ratio estimate: tokens = bytes / 4. Saved = (file_bytes - hook_output_bytes) / 4. Only counts when hook fires on source files with non-empty response.
- In-memory atomic counters on sidecar state: per-hook-type fire count + saved token count. Reset on process restart. No persistence needed — savings are per-session.
- Surfaced in two places: per-hook `[~N tokens saved]` footer + cumulative breakdown in health tool response (by hook type: fires + saved)
- SessionStart does NOT count toward savings (it's additive context, not a replacement)

### Claude's Discretion
- Exact serde struct layout for stdin JSON parsing
- How sidecar caches pre-edit symbol lists for diff (in-memory alongside LiveIndex or separate)
- Exact format of health tool's token savings section
- Write hook endpoint: reuse `/impact` with `new_file=true` param or separate `/index-new` endpoint
- How to handle the `max_tokens` parameter when not provided (use hardcoded default)

</decisions>

<specifics>
## Specific Ideas

- Output format follows the preview mockups from discussion — compact, indented, ripgrep-style headers with `──` delimiters
- Read hook: outline section then "Key references" section, one line per caller with `file.rs:symbol()  line N` format
- Edit hook: "Changed/Added/Removed" labels on symbols, then callers grouped by file
- Grep hook: grouped by file, `line N  in fn symbol_name` format
- SessionStart: directory tree indented with file/symbol counts right-aligned
- Phase 5 blocker re: `additionalContext` schema — the minimal serde parse with all `Option` fields handles this gracefully (fail-open on unknown schema)

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/cli/hook.rs` (342 lines): sync HTTP client, fail-open JSON, success JSON, url_encode — all reusable. Needs stdin parsing added and endpoint_for() updated for stdin-driven routing.
- `src/sidecar/handlers.rs` (545 lines): 5 endpoint handlers already exist (/health, /outline, /impact, /symbol-context, /repo-map). Need budget enforcement added and /impact needs re-index + diff logic.
- `src/sidecar/router.rs` (24 lines): axum router wiring all 5 endpoints. May need GET→POST change for /impact.
- `src/cli/init.rs` (398 lines): `merge_hooks_into_settings()` already public and tested. Needs update for new hook format (stdin-routed single entry + SessionStart).
- `src/protocol/format.rs`: Compact formatters for outline, repo_outline, etc. — sidecar can share formatting logic.
- `src/live_index/store.rs`: LiveIndex with all query methods — sidecar handlers already use Arc<SharedIndex>.

### Established Patterns
- Fail-open: hook binary catches all errors and outputs empty additionalContext JSON. Continue this pattern.
- Sync I/O in hook binary: no tokio runtime, raw TcpStream, 50ms timeout. Continue this.
- `loading_guard!` macro in MCP tools — sidecar handlers should use similar guards.
- Parse-before-lock for watcher updates — /impact re-index can follow same pattern.
- `Arc<RwLock<LiveIndex>>` shared ownership — sidecar already has a clone.

### Integration Points
- `src/cli/hook.rs`: Add stdin JSON parsing, update routing logic, keep old subcommands as fallback
- `src/cli/mod.rs`: Add `Write` to HookSubcommand enum, update CLI routing
- `src/sidecar/handlers.rs`: Add budget enforcement to all handlers, add re-index + diff logic to /impact, add token stats recording
- `src/sidecar/mod.rs`: Add TokenStats struct with atomic counters, wire into handler state
- `src/cli/init.rs`: Update hook entry format, add auto-migration of old entries, add SessionStart entry
- `src/protocol/tools.rs`: Update health tool to include token savings section from sidecar stats

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 06-hook-enrichment-integration*
*Context gathered: 2026-03-10*

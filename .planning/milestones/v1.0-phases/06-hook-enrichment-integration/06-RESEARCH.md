# Phase 6: Hook Enrichment Integration - Research

**Researched:** 2026-03-10
**Domain:** Claude Code hooks schema, Rust async atomics, sidecar handler augmentation, stdin JSON parsing
**Confidence:** HIGH

## Summary

Phase 6 completes the parasitic hook system by replacing the Phase 5 env-var shim with proper stdin JSON parsing, adding rich content (outline, impact diff, grep annotations, repo map) to each hook type, enforcing per-hook token budgets server-side, and wiring token savings tracking as in-memory atomic counters on the sidecar. All five hook events (PostToolUse/Read, PostToolUse/Edit, PostToolUse/Write, PostToolUse/Grep, SessionStart) are addressed. The init command is updated to write a single stdin-routed PostToolUse entry plus auto-migrate the three old subcommand entries.

The Claude Code hooks schema is confirmed against live documentation (March 2026). The output format that Phase 5 already uses — `{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"..."}}` — is the correct current schema for both PostToolUse and SessionStart. No breaking schema change is required. The known pre-Phase 6 concern in STATE.md is resolved: the existing format matches the official spec.

**Primary recommendation:** Augment existing handlers and hook binary in-place — the infrastructure from Phase 5 is correct. The work is additive: enrich handler responses, add budget truncation, add stdin parsing, add atomic counters, update init hook registration format.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Read hook content (HOOK-04)**
- Inject symbol outline (like get_file_outline) plus "Key references" section showing the 3-5 most-referenced symbols' callers
- Reference selection: rank symbols by caller count descending, take top 3-5 (fit within ~80 token sub-budget), show up to 3 callers per symbol, skip symbols with 0 callers
- Non-source files (.json, .toml, .md): silent no-op — output empty additionalContext, no noise
- One-line footer: `[~N tokens saved]` at the end of every hook response

**Edit hook impact (HOOK-05)**
- Two-section output: "Changed symbols" diff + "Callers to review"
- Changed symbols: compare pre-edit and post-edit symbol lists by name+kind. Added/removed symbols shown explicitly. "Changed" = same name but different line range or byte count (signature/body changed)
- Callers to review: show callers only for Changed and Removed symbols
- Detection: sidecar caches per-file symbol list; on /impact call, re-indexes the file immediately (not waiting for watcher), computes diff against cached list, returns impact JSON
- Hook triggers re-index explicitly (POST to /impact), guaranteeing fresh data even if watcher hasn't caught up within its 200ms debounce window. ~30ms re-index + ~5ms diff = well within 50ms HTTP budget.

**Write hook (HOOK-06)**
- Index confirmation only — new files have no callers yet
- Output: language detected, symbol count with kind breakdown, `[Indexed, 0 callers yet]` footer
- Separate from Edit hook — Write creates new files, Edit modifies existing ones

**Grep hook context (HOOK-07)**
- Enclosing symbol name annotation: for each matched line, show which symbol (function/method/class) contains it
- Non-source file matches: skip silently — only annotate matches in indexed source files
- Input: parse tool_input only from stdin JSON (extract pattern + path), send to sidecar which searches its own index and returns symbol context
- Cap at 10 annotated matches, then `... (showing N of M matches)`. Keeps within ~100 token budget even for broad patterns.

**SessionStart repo map (HOOK-08)**
- Directory tree + symbol counts: show directory structure (2 levels max) with file counts and symbol counts per directory
- Language breakdown header: project name, total source files, total symbols, language counts
- For dirs with >5 subdirs: aggregate (e.g., `12 subdirs, 89 files`)
- ~500 token budget

**Token budget enforcement (HOOK-09)**
- Hardcoded defaults: 200 (Read), 150 (Edit), 100 (Grep), 500 (SessionStart)
- Enforcement happens in the sidecar (server-side) via `max_tokens` query parameter
- Truncation strategy: truncate at nearest logical boundary (end of symbol entry, end of reference group), append `... (truncated, N more)`
- Hook binary receives already-budgeted content, just formats into additionalContext

**Stdin JSON parsing**
- Minimal serde parse: deserialize stdin JSON into small struct with just `tool_name` and `tool_input` fields. All fields `Option<T>`, fail-open if missing or schema changes.
- Extract file path from `tool_input.file_path` (Read/Edit/Write) or `tool_input.pattern` + `tool_input.path` (Grep)
- Stdin `tool_name` drives routing: single `tokenizor hook` command (no subcommands needed), routes to /outline, /impact, /symbol-context based on tool_name
- Old subcommands (tokenizor hook read/edit/grep) still work for manual testing — subcommand takes precedence over stdin if provided
- SessionStart uses dedicated `tokenizor hook session-start` subcommand

**tokenizor init (INFR-01)**
- Registers 2 hook entries in `~/.claude/settings.json` (user-global): one PostToolUse (`tokenizor hook`) + one SessionStart (`tokenizor hook session-start`)
- Auto-migration: if old Phase 5 subcommand entries exist (tokenizor hook read/edit/grep), remove them and replace with single stdin-routed entry
- Idempotent: running twice produces identical result
- Preserves all non-tokenizor hooks

**Token savings tracking (INFR-04)**
- Byte-ratio estimate: tokens = bytes / 4. Saved = (file_bytes - hook_output_bytes) / 4. Only counts when hook fires on source files with non-empty response.
- In-memory atomic counters on sidecar state: per-hook-type fire count + saved token count. Reset on process restart. No persistence needed — savings are per-session.
- Surfaced in two places: per-hook `[~N tokens saved]` footer + cumulative breakdown in health tool response (by hook type: fires + saved)
- SessionStart does NOT count toward savings (it's additive context, not a replacement)

**Output format follows the preview mockups from discussion:**
- Compact, indented, ripgrep-style headers with `──` delimiters
- Read hook: outline section then "Key references" section, one line per caller with `file.rs:symbol()  line N` format
- Edit hook: "Changed/Added/Removed" labels on symbols, then callers grouped by file
- Grep hook: grouped by file, `line N  in fn symbol_name` format
- SessionStart: directory tree indented with file/symbol counts right-aligned

### Claude's Discretion
- Exact serde struct layout for stdin JSON parsing
- How sidecar caches pre-edit symbol lists for diff (in-memory alongside LiveIndex or separate)
- Exact format of health tool's token savings section
- Write hook endpoint: reuse /impact with `new_file=true` param or separate `/index-new` endpoint
- How to handle the `max_tokens` parameter when not provided (use hardcoded default)

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| HOOK-04 | PostToolUse(Read) — inject symbol outline + key references for indexed files | Sidecar /outline handler exists; needs caller count ranking + "Key references" section + budget enforcement added |
| HOOK-05 | PostToolUse(Edit) — trigger re-index + inject impact analysis (callers to review) | Sidecar /impact handler exists (currently returns dependents); needs re-index-on-call + pre/post diff logic + sidecar symbol cache |
| HOOK-06 | PostToolUse(Write) — trigger index of new file + confirmation | Write tool sends file_path + content in stdin; sidecar needs new-file indexing endpoint/param + language/kind confirmation response |
| HOOK-07 | PostToolUse(Grep) — inject symbol context for matched lines | Sidecar /symbol-context handler exists; needs enclosing-symbol lookup per matched line, stdin pattern+path parsing |
| HOOK-08 | SessionStart — inject compact repo map (~500 tokens) | Sidecar /repo-map handler exists (JSON); needs formatted human-readable directory tree with symbol counts |
| HOOK-09 | Hook output token budget enforced (<200 tokens for Read, <100 for Grep) | All handlers need `max_tokens` query param + logical-boundary truncation logic |
| INFR-01 | tokenizor init writes PostToolUse hooks into settings.json (idempotent) | merge_hooks_into_settings() is public + tested; needs new single stdin-routed entry format + auto-migration of old 3-entry format |
| INFR-04 | Token savings calculation and tracking per session | Sidecar state needs TokenStats struct with AtomicU64 counters; health handler needs stats section; hook binary needs `[~N tokens saved]` footer |
</phase_requirements>

---

## Standard Stack

### Core (all already in Cargo.toml — no new dependencies needed)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde` | 1.0 | Stdin JSON deserialization | Already used throughout; `#[derive(Deserialize)]` on Option fields |
| `serde_json` | 1.0 | JSON parsing + formatting | Already used for all hook output |
| `axum` | 0.8 | Sidecar HTTP handlers | Already running; handlers just need augmentation |
| `std::sync::atomic` | stdlib | Token savings counters | AtomicU64 + AtomicUsize, no new crate needed |
| `tokio::sync::RwLock` or `std::sync::Arc` | stdlib / tokio | Sharing TokenStats with handlers | Already arc-shared pattern for SharedIndex |
| `anyhow` | 1.0 | Error handling in hook binary | Already used |

**Installation:** No new dependencies. All required crates are already in `Cargo.toml`.

### Confidence: HIGH — all dependencies confirmed present in Cargo.toml.

---

## Architecture Patterns

### Confirmed Hook Output Schema (HIGH confidence — verified against live docs 2026-03-10)

The official Claude Code hooks documentation confirms:

**PostToolUse `additionalContext` output:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "text injected into Claude context"
  }
}
```

**SessionStart `additionalContext` output:**
```json
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "text injected into Claude context"
  }
}
```

The format `hook.rs` already produces in Phase 5 is **correct**. The pre-Phase 6 concern in STATE.md is resolved. No schema migration needed.

**CRITICAL finding:** The official schema shows `additionalContext` as a field INSIDE `hookSpecificOutput`, which is exactly what Phase 5 implemented. The structure `{"hookSpecificOutput":{"hookEventName":"...","additionalContext":"..."}}` is current and correct as of March 2026.

### Confirmed PostToolUse Stdin Schema (HIGH confidence — from official docs)

Claude Code sends this JSON on stdin to PostToolUse hook commands:
```json
{
  "session_id": "abc123",
  "transcript_path": "/path/to/transcript.jsonl",
  "cwd": "/path/to/project",
  "permission_mode": "default",
  "hook_event_name": "PostToolUse",
  "tool_name": "Read",
  "tool_input": {
    "file_path": "/absolute/path/to/file.rs"
  },
  "tool_response": { ... },
  "tool_use_id": "toolu_01ABC123..."
}
```

**For Grep specifically:**
```json
{
  "tool_name": "Grep",
  "tool_input": {
    "pattern": "TODO.*fix",
    "path": "/path/to/dir",
    "glob": "*.ts"
  }
}
```

**For Write specifically:**
```json
{
  "tool_name": "Write",
  "tool_input": {
    "file_path": "/absolute/path/to/new_file.rs",
    "content": "file content here"
  }
}
```

**For Edit specifically:**
```json
{
  "tool_name": "Edit",
  "tool_input": {
    "file_path": "/absolute/path/to/file.rs",
    "old_string": "...",
    "new_string": "..."
  }
}
```

**Minimal serde struct (Claude's Discretion — recommended layout):**
```rust
// Source: design decision — fail-open on all Option fields
#[derive(Deserialize, Default)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<HookToolInput>,
    cwd: Option<String>,
}

#[derive(Deserialize, Default)]
struct HookToolInput {
    file_path: Option<String>,  // Read, Edit, Write
    pattern: Option<String>,    // Grep
    path: Option<String>,       // Grep optional scope
    content: Option<String>,    // Write (for byte size estimation)
}
```

**Path extraction note:** Claude Code sends absolute paths in `tool_input.file_path`. The hook needs to convert to relative path by stripping `cwd` prefix, since the sidecar index uses relative paths.

### Pattern 1: Single Stdin-Routed PostToolUse Entry

The new init format replaces three separate subcommand entries with one:
```json
{
  "PostToolUse": [
    {
      "matcher": "Read|Edit|Write|Grep",
      "hooks": [{"type": "command", "command": "/path/tokenizor hook", "timeout": 5}]
    }
  ]
}
```

The hook binary reads `tool_name` from stdin to route to the correct sidecar endpoint. Old subcommand entries (with `hook read`, `hook edit`, `hook grep` suffixes) are identified and replaced during migration via the existing `is_tokenizor_entry()` predicate.

### Pattern 2: Sidecar Token Budget via Query Parameter

All handlers accept `max_tokens: Option<u64>` in their query params. Truncation happens in the sidecar before returning text. Logical boundaries for truncation:
- Read outline: after a complete symbol entry line
- Read key references: after a complete caller group
- Edit impact: after the "Changed symbols" section OR after a complete "Callers to review" file group
- Grep: after a complete annotated match line
- SessionStart repo map: after a complete directory entry

Default values when `max_tokens` not supplied:
- `/outline`: 200 tokens (encoded as 800 bytes)
- `/impact`: 150 tokens (600 bytes)
- `/symbol-context`: 100 tokens (400 bytes)
- `/repo-map`: 500 tokens (2000 bytes)

### Pattern 3: Pre-Edit Symbol Snapshot for Impact Diff

The sidecar needs a per-file symbol snapshot cache to compute HOOK-05's "Changed symbols" diff. Since sidecar and watcher share `Arc<RwLock<LiveIndex>>`, the pre-edit snapshot can be stored separately in sidecar state:

```rust
// In sidecar state (Claude's Discretion — recommended layout)
struct SidecarState {
    index: SharedIndex,
    token_stats: Arc<TokenStats>,
    symbol_cache: Arc<RwLock<HashMap<String, Vec<SymbolSnapshot>>>>,
}
```

**Impact flow on POST /impact?path=src/foo.rs:**
1. Read pre-edit symbols from `symbol_cache` for `path` (populated on last outline call or lazy on first impact call)
2. Re-parse `path` from disk immediately via `LiveIndex::update_file` equivalent
3. Read post-edit symbols from freshly updated index
4. Compute diff (added/changed/removed)
5. For Changed+Removed symbols: look up callers via `find_references_for_name`
6. Update `symbol_cache[path]` to post-edit snapshot
7. Return formatted impact text

**Alternative (simpler):** Store snapshot in a `Mutex<HashMap>` field alongside the handler state rather than inside sidecar/mod.rs. Both work; the `Arc<RwLock<>>` approach is consistent with existing patterns.

### Pattern 4: TokenStats with Atomics

```rust
// In src/sidecar/mod.rs
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct TokenStats {
    pub read_fires: AtomicUsize,
    pub read_saved: AtomicU64,
    pub edit_fires: AtomicUsize,
    pub edit_saved: AtomicU64,
    pub write_fires: AtomicUsize,
    pub grep_fires: AtomicUsize,
    pub grep_saved: AtomicU64,
}
```

Savings calculation: `saved_tokens = (file_bytes.saturating_sub(output_bytes)) / 4`

The `TokenStats` instance is wrapped in `Arc` and passed to each handler via axum `State`. The health MCP tool accesses sidecar stats via the same sidecar port (HTTP call from the MCP tool to the sidecar's `/health` endpoint, or via a new `/stats` endpoint, or by passing `Arc<TokenStats>` into the `TokenizorServer` struct).

**Recommended approach (Claude's Discretion):** Add a `/stats` endpoint to the sidecar returning a JSON summary. The MCP health tool calls the sidecar's `/stats` endpoint at query time (same 50ms sync HTTP pattern as the hook binary). This avoids coupling `TokenizorServer` to `SidecarState`.

### Pattern 5: Write Hook Endpoint

**Recommended approach (Claude's Discretion):** Reuse `/impact` with a `new_file=true` query parameter. When `new_file=true`:
- Skip pre-edit snapshot comparison
- Index the file (call `update_file` on the shared LiveIndex)
- Return language + symbol count breakdown + `[Indexed, 0 callers yet]` footer

This avoids adding a new route to the axum router.

### Recommended Project Structure Changes

```
src/
├── cli/
│   ├── hook.rs          # Add stdin JSON parsing, update routing logic
│   └── init.rs          # Update hook entry format + auto-migration
├── sidecar/
│   ├── handlers.rs      # Add budget enforcement, impact diff, token stats recording
│   ├── mod.rs           # Add TokenStats struct + SidecarState
│   └── router.rs        # Possibly add /stats route; /impact may change GET→POST
└── protocol/
    └── tools.rs         # Update health tool to query sidecar /stats endpoint
```

### Anti-Patterns to Avoid

- **Holding RwLockReadGuard across re-index operations:** The impact handler must drop the read guard, acquire write for update_file, then re-acquire read for the diff. Established pattern: parse-before-lock.
- **Tokio runtime in hook binary:** hook.rs must stay sync (no `#[tokio::main]`). Established constraint from Phase 5.
- **Writing anything to stdout in hook binary except the final JSON line:** HOOK-10 requires pure JSON stdout. No debug prints.
- **GET with body for /impact re-index:** HTTP GET with a request body is undefined. Use POST for /impact when it triggers re-indexing, or keep GET and pass file path as query param (file path is already a query param, and re-indexing happens server-side as a side effect of the GET — this is acceptable since it's idempotent on the same content).

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Atomic counters | Custom mutex-wrapped counters | `std::sync::atomic::AtomicU64` | Lock-free, already in stdlib |
| Token estimation | Word/sentence tokenizer | bytes / 4 ratio (locked decision) | Simple, no dep, acceptable precision |
| Absolute→relative path | Custom path stripping logic | `path.strip_prefix(cwd)` from `std::path::Path` | Handles platform differences |
| JSON escape | Custom escaping | `serde_json::to_string()` or existing `json_escape()` in hook.rs | Edge cases in JSON string escaping |
| Symbol diff | Myers diff algorithm | Simple HashMap comparison by (name, kind) | Only need added/removed/changed, not edit distance |

---

## Common Pitfalls

### Pitfall 1: Absolute vs Relative Paths in Hook Input

**What goes wrong:** Claude Code sends absolute paths in `tool_input.file_path` (e.g., `/home/user/project/src/foo.rs`). The sidecar index stores relative paths (e.g., `src/foo.rs`). Passing absolute paths to `/outline?path=...` returns 404.

**Why it happens:** The hook binary's `endpoint_for()` in Phase 5 read from `TOKENIZOR_HOOK_FILE_PATH` env var (which was relative). With stdin JSON, the new path source is absolute.

**How to avoid:** In `hook.rs` stdin parsing, strip the `cwd` field from the stdin JSON as the prefix: `absolute_path.strip_prefix(&cwd).unwrap_or(absolute_path)`. URL-encode the resulting relative path.

**Warning signs:** `/outline` returning 404 in integration tests despite the file existing in the index.

### Pitfall 2: GET vs POST for /impact with Re-Index Side Effect

**What goes wrong:** Changing /impact from GET to POST breaks existing unit tests that use `routing::get(impact_handler)` and `raw_http_get(port, "/impact", ...)` in tests.

**Why it happens:** The CONTEXT.md says re-index is triggered by the hook, but the decision on HTTP method is Claude's Discretion.

**How to avoid:** Keep `/impact` as GET. The file path is passed as a query parameter. Re-indexing is a read-triggered side effect — acceptable since re-reading the same file gives the same result (idempotent). This preserves backward compatibility with Phase 5 tests.

### Pitfall 3: Symbol Cache Race in Impact Handler

**What goes wrong:** Two concurrent Edit calls for the same file update `symbol_cache[path]` interleaved, corrupting the pre-edit snapshot.

**Why it happens:** Without locking, the cache read and write are not atomic.

**How to avoid:** Use `Arc<RwLock<HashMap<String, Vec<SymbolSnapshot>>>>` for the cache, acquired write-exclusively during the snapshot-update step. The handler pattern already uses RwLock for SharedIndex.

### Pitfall 4: Non-Source Files Returning Noise

**What goes wrong:** Read hook fires on `.json`, `.toml`, `.md` files that aren't indexed. The sidecar returns 404, and the hook binary emits fail-open empty JSON — correct. But if the handler is changed to return something on non-found files, noise enters context.

**Why it happens:** Sidecar handlers currently return `StatusCode::NOT_FOUND` for unknown paths. Hook binary already fail-opens on non-200 responses. The invariant holds as long as handlers never return 200 with empty-but-present content for unknown files.

**How to avoid:** Preserve the 404 behavior for files not in the index. Document the non-source file no-op as an explicit invariant in the handler.

### Pitfall 5: Truncation at Wrong Boundary

**What goes wrong:** Token budget truncates mid-line, leaving incomplete symbol entries like `fn foo` without its line range, or a "Callers to review" header with no entries following.

**Why it happens:** Naive byte-count truncation doesn't respect line structure.

**How to avoid:** Build the output as a `Vec<String>` of logical units, count tokens after each append, stop before exceeding budget, then append `... (truncated, N more)`.

### Pitfall 6: Auto-Migration Removes Non-Tokenizor Entries

**What goes wrong:** The auto-migration loop in init.rs removes entries that aren't tokenizor-owned but happen to have the substring `"tokenizor hook"` in a description field or other field.

**Why it happens:** `is_tokenizor_entry()` checks `command` field only — this is already correct. Risk is minimal but worth documenting.

**How to avoid:** Keep `is_tokenizor_entry()` checking only the `command` field substring. The existing implementation is already safe.

---

## Code Examples

Verified patterns from official sources and existing codebase:

### Stdin JSON Parsing in hook.rs

```rust
// Recommended layout (Claude's Discretion)
use serde::Deserialize;
use std::io::BufRead;

#[derive(Deserialize, Default)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<HookToolInput>,
    cwd: Option<String>,
}

#[derive(Deserialize, Default)]
struct HookToolInput {
    file_path: Option<String>,
    pattern: Option<String>,
    path: Option<String>,
}

fn parse_stdin_input() -> HookInput {
    let stdin = std::io::stdin();
    let mut raw = String::new();
    for line in stdin.lock().lines() {
        match line {
            Ok(l) => raw.push_str(&l),
            Err(_) => break,
        }
    }
    serde_json::from_str(&raw).unwrap_or_default()
}

fn relative_path(absolute: &str, cwd: &str) -> String {
    std::path::Path::new(absolute)
        .strip_prefix(cwd)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| absolute.to_string())
}
```

### Routing by tool_name (in run_hook)

```rust
// Single entry point — subcommand takes precedence for manual testing
pub fn run_hook_auto(subcommand: Option<&HookSubcommand>) -> anyhow::Result<()> {
    let input = parse_stdin_input();

    // If explicit subcommand provided (manual test mode), use it.
    let tool_name = subcommand.map(|s| subcommand_to_tool_name(s))
        .or_else(|| input.tool_name.as_deref().map(|s| s.to_string()));

    let (path, query) = match tool_name.as_deref() {
        Some("Read") => { /* extract file_path, make relative, build /outline query */ }
        Some("Edit") => { /* extract file_path, build /impact query */ }
        Some("Write") => { /* extract file_path, build /impact?new_file=true query */ }
        Some("Grep") => { /* extract pattern, build /symbol-context query */ }
        _ => return Ok(println!("{}", fail_open_json("PostToolUse"))),
    };
    // ... rest of run_hook logic
}
```

### TokenStats struct

```rust
// In src/sidecar/mod.rs
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct TokenStats {
    pub read_fires: AtomicUsize,
    pub read_saved_tokens: AtomicU64,
    pub edit_fires: AtomicUsize,
    pub edit_saved_tokens: AtomicU64,
    pub write_fires: AtomicUsize,
    pub grep_fires: AtomicUsize,
    pub grep_saved_tokens: AtomicU64,
}

impl TokenStats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            read_fires: AtomicUsize::new(0),
            read_saved_tokens: AtomicU64::new(0),
            edit_fires: AtomicUsize::new(0),
            edit_saved_tokens: AtomicU64::new(0),
            write_fires: AtomicUsize::new(0),
            grep_fires: AtomicUsize::new(0),
            grep_saved_tokens: AtomicU64::new(0),
        })
    }

    pub fn record_read(&self, file_bytes: u64, output_bytes: u64) {
        self.read_fires.fetch_add(1, Ordering::Relaxed);
        let saved = file_bytes.saturating_sub(output_bytes) / 4;
        self.read_saved_tokens.fetch_add(saved, Ordering::Relaxed);
    }
}
```

### Budget Enforcement Pattern

```rust
// Sidecar handler pattern for budget-aware text building
fn build_with_budget(items: &[String], max_tokens: u64) -> (String, usize) {
    let max_bytes = max_tokens * 4;
    let mut result = Vec::new();
    let mut byte_count: u64 = 0;
    let total = items.len();

    for item in items {
        let item_bytes = item.len() as u64 + 1; // +1 for newline
        if byte_count + item_bytes > max_bytes {
            let remaining = total - result.len();
            result.push(format!("... (truncated, {} more)", remaining));
            break;
        }
        byte_count += item_bytes;
        result.push(item.clone());
    }
    (result.join("\n"), result.len())
}
```

### Init: New Hook Entry Format

```rust
// src/cli/init.rs — build_post_tool_use_entries replacement
fn build_post_tool_use_entries(binary_path: &str) -> Vec<Value> {
    // Single stdin-routed entry replaces the 3 subcommand entries from Phase 5
    vec![json!({
        "matcher": "Read|Edit|Write|Grep",
        "hooks": [{"type": "command", "command": format!("{binary_path} hook"), "timeout": 5}]
    })]
}
```

Auto-migration: the existing `is_tokenizor_entry()` predicate already identifies entries by `"tokenizor hook"` substring in the command field. Both old-format entries (`tokenizor hook read`) and new-format entry (`tokenizor hook`) match this predicate. Running merge twice produces identical results — idempotency preserved.

**Old entries that get removed (auto-migration):**
- `{binary_path} hook read`
- `{binary_path} hook edit`
- `{binary_path} hook grep`

**New single entry:**
- `{binary_path} hook`

---

## State of the Art

| Old Approach (Phase 5) | Current Approach (Phase 6) | Impact |
|------------------------|---------------------------|--------|
| Env var shim (`TOKENIZOR_HOOK_FILE_PATH`) | Stdin JSON parsing of Claude Code's native event payload | Correct file paths, access to all tool_input fields including content for Write |
| Three PostToolUse entries (hook read/edit/grep) | One stdin-routed PostToolUse entry (hook) | Simpler init, single binary entry point |
| Handler returns raw JSON array | Handler returns budgeted human-readable text | Fits additionalContext token budget |
| No pre-edit snapshot | Sidecar symbol cache for diff | Enables Changed/Added/Removed symbol detection |
| No token tracking | AtomicU64 counters on sidecar state | Session-scoped savings reporting |

**Deprecated/outdated:**
- `TOKENIZOR_HOOK_FILE_PATH` env var: Phase 6 replaces with stdin JSON; the env var shim code in `endpoint_for()` is removed (but old subcommands remain as fallback for manual testing)
- Three-entry PostToolUse format in init.rs: replaced by single stdin-routed entry

---

## Open Questions

1. **GET vs POST for /impact with re-index side effect**
   - What we know: Re-indexing is triggered server-side when /impact is called; query param carries the file path
   - What's unclear: Whether the HTTP method matters given re-indexing is idempotent on same-content file
   - Recommendation: Keep GET, trigger re-index as a side effect. This preserves existing test infrastructure and the result is idempotent (re-parsing same file gives same symbols).

2. **Where does the sidecar's stats reach the MCP health tool?**
   - What we know: `TokenizorServer` doesn't hold a reference to sidecar state; sidecar and MCP server share `SharedIndex` but not sidecar state
   - What's unclear: Best wiring without creating tight coupling
   - Recommendation: Add a `/stats` endpoint to the sidecar. The MCP health tool calls it synchronously via the same raw TcpStream pattern as the hook binary, using the port file. This keeps concerns separated.

3. **Write hook content source**
   - What we know: The Write tool's `tool_input.content` field contains the new file content. For token savings: we could parse content byte count from the Write tool_input rather than reading disk.
   - What's unclear: Whether to parse content from stdin or just index from disk after write completes.
   - Recommendation: Prefer reading from disk after indexing (cleaner, consistent with how other hooks work). The file_bytes for savings comes from the newly-indexed `IndexedFile.byte_len`.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust `cargo test` (built-in) + `tokio::test` for async |
| Config file | none — cargo test runner |
| Quick run command | `cargo test --lib 2>&1 \| tail -5` |
| Full suite command | `cargo test -- --test-threads=1 2>&1 \| tail -20` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HOOK-04 | Read hook injects outline + key refs within 200 tokens | integration | `cargo test test_read_hook_injects_outline -- --test-threads=1` | Wave 0 |
| HOOK-04 | Non-source files get empty additionalContext | unit | `cargo test test_read_hook_noop_on_non_source` | Wave 0 |
| HOOK-05 | Edit hook injects impact diff with callers | integration | `cargo test test_edit_hook_impact_diff -- --test-threads=1` | Wave 0 |
| HOOK-05 | Changed/Added/Removed symbol labels correct | unit | `cargo test test_symbol_diff_labels` | Wave 0 |
| HOOK-06 | Write hook confirms indexing with kind breakdown | integration | `cargo test test_write_hook_confirms_index -- --test-threads=1` | Wave 0 |
| HOOK-07 | Grep hook annotates matched lines with enclosing symbol | integration | `cargo test test_grep_hook_annotates_matches -- --test-threads=1` | Wave 0 |
| HOOK-07 | Grep hook caps at 10 annotated matches | unit | `cargo test test_grep_hook_caps_at_10` | Wave 0 |
| HOOK-08 | SessionStart injects directory tree under 500 tokens | integration | `cargo test test_session_start_repo_map -- --test-threads=1` | Wave 0 |
| HOOK-09 | Read hook output <= 200 tokens | integration | `cargo test test_read_hook_budget_enforced -- --test-threads=1` | Wave 0 |
| HOOK-09 | Truncation appends `... (truncated, N more)` at logical boundary | unit | `cargo test test_truncation_logical_boundary` | Wave 0 |
| INFR-01 | init writes single stdin-routed PostToolUse entry | unit | `cargo test test_init_single_post_tool_use_entry` | Wave 0 |
| INFR-01 | init auto-migrates old 3-entry format | unit | `cargo test test_init_migrates_old_entries` | Wave 0 |
| INFR-01 | init idempotent with new format | unit (existing passes) | `cargo test test_init_idempotent` | ✅ `tests/init_integration.rs` |
| INFR-04 | Token savings counters increment on source file hook fire | unit | `cargo test test_token_stats_increment` | Wave 0 |
| INFR-04 | Health endpoint includes savings breakdown | integration | `cargo test test_health_includes_savings -- --test-threads=1` | Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test --lib 2>&1 | tail -5`
- **Per wave merge:** `cargo test -- --test-threads=1 2>&1 | tail -20`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `tests/hook_enrichment_integration.rs` — covers HOOK-04 through HOOK-09 integration scenarios
- [ ] Unit tests for `build_with_budget()` / truncation logic in sidecar handlers
- [ ] Unit tests for `symbol_diff()` (pre/post snapshot diff for HOOK-05)
- [ ] Unit tests for `parse_stdin_input()` and `relative_path()` in hook.rs
- [ ] Unit tests for new init entry format and auto-migration (INFR-01)
- [ ] Unit tests for `TokenStats` counter increment logic (INFR-04)

*(Existing `tests/sidecar_integration.rs` covers HOOK-01..03 and HOOK-10 — all passing. These must remain green.)*
*(Existing `tests/init_integration.rs` covers idempotency — will need update for new single-entry format.)*

---

## Sources

### Primary (HIGH confidence)
- Official Claude Code hooks reference (https://code.claude.com/docs/en/hooks) — verified PostToolUse and SessionStart `additionalContext` schema, stdin JSON field names for Read/Edit/Write/Grep tools
- `src/cli/hook.rs` (342 lines) — Phase 5 hook binary implementation
- `src/sidecar/handlers.rs` (545 lines) — all 5 endpoint handlers
- `src/cli/init.rs` (398 lines) — `merge_hooks_into_settings()` and entry builders
- `src/sidecar/mod.rs`, `src/sidecar/server.rs`, `src/sidecar/router.rs` — sidecar architecture
- `src/protocol/mod.rs` — `TokenizorServer` struct and wiring
- `Cargo.toml` — confirmed no new dependencies needed
- `.planning/phases/06-hook-enrichment-integration/06-CONTEXT.md` — all locked decisions

### Secondary (MEDIUM confidence)
- `.planning/STATE.md` accumulated decisions for Phases 1-5 — established patterns confirmed in source code
- `.planning/REQUIREMENTS.md` — requirement descriptions for HOOK-04..09, INFR-01, INFR-04

### Tertiary (LOW confidence)
- None — all critical claims verified via official docs or direct source code reading

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all dependencies confirmed in Cargo.toml, no new crates needed
- Architecture: HIGH — existing handler interfaces read directly, hook schema verified against live docs
- Pitfalls: HIGH — absolute vs relative path issue verified against live hook schema; other pitfalls derived from existing code patterns
- Hook output schema: HIGH — verified against live Claude Code docs 2026-03-10; resolves pre-Phase 6 blocker concern

**Research date:** 2026-03-10
**Valid until:** 2026-06-10 (Claude Code hook schema is stable; re-verify if major version bump)

**Pre-Phase 6 blocker from STATE.md — RESOLVED:**
> `additionalContext` JSON schema path varies across Claude Code releases.

Resolution: The official docs (https://code.claude.com/docs/en/hooks, March 2026) confirm the schema that Phase 5 already uses is correct and current: `{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"..."}}` for PostToolUse and identical structure for SessionStart. No migration needed.

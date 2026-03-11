# Phase 7: Polish and Persistence - Research

**Researched:** 2026-03-10
**Domain:** Rust binary serialization, trigram indexing, tree-sitter C/C++ grammars, MCP tool authoring
**Confidence:** HIGH (codebase + crates verified; one critical dependency finding)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Index persistence (PLSH-04, PLSH-05)**
- Serialization format: bincode 2.x — fastest Rust serialize/deserialize, compact binary
- No schema migration — version tag at file header, mismatch triggers full re-index
- Serialize on shutdown only (stdin EOF / SIGTERM) — no periodic or per-change writes
- If process crashes, next start does full re-index (same as today, no regression)
- File location: `.tokenizor/index.bin` alongside existing `sidecar.port` and `sidecar.pid`
- Background verification after loading serialized index:
  1. Deserialize index.bin (~3ms)
  2. Stat-check all files (mtime + size) — changed files queued for re-parse, deleted files removed, new files queued
  3. Serve queries immediately (stale-ok for brief window)
  4. Background: re-parse queued files
  5. Background: spot-verify 10% of files by content hash — mismatches trigger re-parse
- Corrupted or unreadable index.bin: fall back to full re-index without crashing

**Trigram text search (PLSH-01)**
- In-memory trigram index built alongside LiveIndex from content bytes already in RAM
- Data structure: `HashMap<[u8;3], Vec<(FileId, Vec<u32>)>>` — trigram to list of (file, byte positions)
- Rebuilt on startup from content bytes (~50ms for 1000 files) — not persisted to disk
- Updated incrementally when watcher re-indexes a file (remove old trigrams, add new)
- Replaces current linear scan in `search_text` transparently — same tool contract
- Target: <10ms for any query on 10,000-file repo

**Scored symbol search (PLSH-02)**
- 3-tier ranking: exact match (score 100) > prefix match (score 75) > substring match (score 50)
- Tiebreak within tiers: exact = alpha, prefix = shorter name wins, substring = earlier position wins
- Results grouped under tier headers: `── Exact matches ──`, `── Prefix matches ──`, `── Substring matches ──`
- Replaces current unranked `search_symbols` — same tool, upgraded ranking
- Case-insensitive matching continues

**File tree tool (PLSH-03)**
- New tool: `get_file_tree` — browsable subtree
- Optional `path` parameter (default: project root), optional `depth` (default 2, max 5)
- Source files only, each shows language + symbol count, each directory shows file count + total symbols
- Footer: total dirs, files, symbols

**Language additions (LANG-01, LANG-02)**
- C: `tree-sitter-c` grammar, extensions `.c`, `.h`
- C++: `tree-sitter-cpp` grammar, extensions `.cpp`, `.cxx`, `.cc`, `.hpp`, `.hxx`, `.hh`
- Two new LanguageId variants: `C` and `Cpp` — **already defined in `src/domain/index.rs`**
- Full parity: symbol extraction + cross-reference extraction
- Deferred languages (post-v2): C# (LANG-03), Ruby (LANG-04), PHP (LANG-05), Swift (LANG-06), Dart (LANG-07)

### Claude's Discretion
- bincode 2.x exact API usage and configuration
- Trigram index internal data structure optimizations
- Exact tree-sitter query patterns for C and C++ symbol/xref extraction
- Shutdown hook implementation (signal handler vs stdin EOF detection)
- `get_file_tree` formatter details
- How stat-check handles files with identical mtime but different content

### Deferred Ideas (OUT OF SCOPE)
- C# language support (LANG-03)
- Ruby language support (LANG-04)
- PHP language support (LANG-05)
- Swift language support (LANG-06)
- Dart language support (LANG-07)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PLSH-01 | Trigram text search — <10ms for any query on 10,000-file repo | HashMap<[u8;3], Vec<(FileId, Vec<u32>)>> pattern; intersection via shortest-list first; incremental update in watcher re-index path |
| PLSH-02 | Scored symbol search — exact > prefix > substring ranking | 3-tier scoring in query.rs; tier header formatting in format.rs; matches existing search_symbols tool contract |
| PLSH-03 | File tree navigation tool — get_file_tree with directory browsing + symbol counts | New tool handler in tools.rs via loading_guard! macro; new formatter in format.rs following existing tree pattern |
| PLSH-04 | Persistence — serialize LiveIndex to disk on shutdown, load on startup (<100ms) | **CRITICAL: bincode is RUSTSEC-2025-0141 unmaintained; use postcard 1.1.3 instead.** Serde derives on IndexedFile; version header; shutdown via tokio signal + stdin EOF select! |
| PLSH-05 | Background hash verification after loading serialized index | Stat-check loop after deserialization; tokio::spawn background re-parse; content_hash already in IndexedFile |
| LANG-01 | Tree-sitter parsing for C | tree-sitter-c 0.24.1 crate; LANGUAGE constant; c.rs language module following rust.rs pattern; node types: function_definition, struct_specifier, enum_specifier, type_definition |
| LANG-02 | Tree-sitter parsing for C++ | tree-sitter-cpp 0.23.4 crate; LANGUAGE constant; cpp.rs language module; additional node types: class_specifier, template_declaration, namespace_definition; xref queries for C++ |
| LANG-03 | Tree-sitter parsing for C# (deferred) | OUT OF SCOPE for Phase 7 |
| LANG-04 | Tree-sitter parsing for Ruby (deferred) | OUT OF SCOPE for Phase 7 |
| LANG-05 | Tree-sitter parsing for PHP (deferred) | OUT OF SCOPE for Phase 7 |
| LANG-06 | Tree-sitter parsing for Swift (deferred) | OUT OF SCOPE for Phase 7 |
| LANG-07 | Tree-sitter parsing for Dart (deferred) | OUT OF SCOPE for Phase 7 |
</phase_requirements>

## Summary

Phase 7 is the final polish phase with five distinct work streams: (1) persistence via serialized index, (2) trigram text search, (3) scored symbol ranking, (4) file tree navigation tool, and (5) C/C++ language support. The codebase is well-structured for all five — `LanguageId::C` and `LanguageId::Cpp` are already defined in `src/domain/index.rs`, the xref pattern is established in `src/parsing/xref.rs`, and the tool/formatter pattern is locked in `src/protocol/tools.rs` and `src/protocol/format.rs`.

One critical finding changes the persistence plan: **bincode is now officially unmaintained** (RUSTSEC-2025-0141, announced late 2025). The user's locked decision names "bincode 2.x" but the underlying goal is fast compact binary serialization. **Postcard 1.1.3** is the direct community-recommended replacement — it uses standard `serde::Serialize`/`Deserialize` derives (already on most types), produces smaller output than bincode, and has benchmarked nearly identical speed. The API change is minimal: `postcard::to_stdvec(&val)?` instead of `bincode::encode_to_vec` and `postcard::from_bytes::<T>(&bytes)?` instead of `bincode::decode_from_slice`. The planner should use postcard and note the substitution from bincode.

The shutdown integration point in `main.rs` is clean: after `service.waiting().await?` returns (stdin EOF = MCP transport closed), the server already does sidecar cleanup. Index serialization slots in at the same point. For SIGTERM on Unix, `tokio::signal::ctrl_c()` plus `tokio::select!` before `serve_server` is the established pattern; `service.waiting()` already represents the MCP lifetime so the serialize-on-exit goes after that future resolves.

**Primary recommendation:** Use postcard 1.1.3 for persistence (not bincode), tree-sitter-c 0.24.1 + tree-sitter-cpp 0.23.4 at existing tree-sitter 0.24 API level, and implement all five work streams as separate plans for safety.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| postcard | 1.1.3 | Binary serialization for LiveIndex persistence | Drop-in bincode replacement (RUSTSEC-2025-0141); uses serde derives already on structs; `use-std` feature gives `to_stdvec`/`from_io` |
| tree-sitter-c | 0.24.1 | C grammar for tree-sitter | Official grammar at same ABI level as existing tree-sitter 0.24 in Cargo.toml |
| tree-sitter-cpp | 0.23.4 | C++ grammar for tree-sitter | Official grammar; extends C; same pattern as existing grammar crates |
| serde | 1.0 (existing) | Derive macros for serialization | Already in Cargo.toml with `derive` feature |
| tokio | 1.48 (existing) | Signal handling for graceful shutdown | `tokio::signal` module for SIGTERM/ctrl_c |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio::signal | (in tokio) | SIGTERM and Ctrl+C detection for shutdown hook | The MCP server's shutdown trigger for index serialization |
| rayon | 1.10 (existing) | Parallel trigram index build | Same pattern as parallel file parse during LiveIndex::load |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| postcard | bincode 2.0.1 | bincode is RUSTSEC-2025-0141 unmaintained — security advisory; postcard is community-recommended drop-in |
| postcard | rkyv | rkyv is faster but requires code changes for zero-copy access; postcard is near-identical to bincode API |
| postcard | bitcode | Lower adoption and fewer maintainers; postcard is better ecosystem bet |
| trigram HashMap | tantivy/tantivy-tokenizer | Full text search library; overkill for code search; in-memory trigram is simpler and fits the already-in-memory content bytes |

**Installation:**
```bash
# Add to Cargo.toml [dependencies]:
postcard = { version = "1.1.3", features = ["use-std"] }
tree-sitter-c = "0.24.1"
tree-sitter-cpp = "0.23.4"
```

## Architecture Patterns

### Recommended Project Structure Additions
```
src/
├── live_index/
│   ├── store.rs       # Add serde derives to IndexedFile, ParseStatus; add TrigramIndex field
│   ├── query.rs       # Add scored_search_symbols(), trigram_search_text()
│   ├── trigram.rs     # NEW — TrigramIndex struct with build/update/search
│   └── persist.rs     # NEW — serialize_index(), load_or_reindex(), background_verify()
├── parsing/
│   └── languages/
│       ├── c.rs       # NEW — extract_symbols for C
│       └── cpp.rs     # NEW — extract_symbols for C++
├── parsing/
│   └── xref.rs        # Add C_XREF_QUERY, CPP_XREF_QUERY constants and match arms
├── protocol/
│   ├── tools.rs       # Add GetFileTreeInput struct + get_file_tree handler
│   └── format.rs      # Add file_tree(), update search_symbols() for tier headers
└── main.rs            # Add persist::load_or_reindex() before LiveIndex::load; add shutdown serialize
```

### Pattern 1: Postcard Serialization for LiveIndex
**What:** Serialize the full LiveIndex to `.tokenizor/index.bin` on shutdown, load on startup before auto-index.
**When to use:** Only on clean shutdown (stdin EOF or SIGTERM), not on crash. Load path replaces `LiveIndex::load()` when a valid index.bin exists.

```rust
// Source: postcard 1.1.3 docs (use-std feature)
use postcard::{from_bytes, to_stdvec};
use serde::{Deserialize, Serialize};

// Version tag + serialized LiveIndex snapshot
#[derive(Serialize, Deserialize)]
struct IndexSnapshot {
    version: u32,  // mismatch triggers re-index, no migration
    files: HashMap<String, IndexedFileSnapshot>,
    // Note: reverse_index is NOT serialized — rebuilt from files on load
    // Note: trigram index NOT serialized — rebuilt from content bytes on load
}

// Serialize on shutdown
let bytes = postcard::to_stdvec(&snapshot)?;
std::fs::write(".tokenizor/index.bin", &bytes)?;

// Load on startup — wrap in catch to prevent crash on corruption
let loaded: Result<IndexSnapshot, _> = std::fs::read(".tokenizor/index.bin")
    .ok()
    .and_then(|bytes| postcard::from_bytes(&bytes).ok());
```

The snapshot needs a separate `IndexedFileSnapshot` that omits non-serializable fields (Instant, Duration) and adds the serializable subset. `loaded_at`, `load_duration`, and `cb_state` are reconstructed fresh after deserialization.

### Pattern 2: Shutdown Hook Integration
**What:** After `service.waiting().await?` resolves (MCP transport closed = stdin EOF), serialize the index.
**When to use:** This is the primary shutdown path. SIGTERM is a secondary path using `tokio::select!`.

```rust
// Source: tokio::signal docs + existing main.rs structure
use tokio::signal;

// In run_mcp_server_async(), replace the current `service.waiting().await?` with:
tokio::select! {
    result = service.waiting() => { result?; }
    _ = signal::ctrl_c() => {
        tracing::info!("received SIGINT/Ctrl+C, shutting down");
    }
}

// After select! resolves — both paths lead here:
if let Some(ref root) = watcher_root {
    let guard = index.read().expect("lock not poisoned");
    if let Err(e) = persist::serialize_index(&guard, root) {
        tracing::warn!("failed to serialize index on shutdown: {e}");
        // Non-fatal — next startup does full re-index
    }
}
```

On Windows, `tokio::signal::ctrl_c()` handles Ctrl+C. For SIGTERM on Unix, use `tokio::signal::unix::signal(SignalKind::terminate())`. Use `#[cfg(unix)]` to conditionally add the SIGTERM arm.

### Pattern 3: Trigram Index Build and Update
**What:** HashMap from 3-byte sequence to (file_id, byte_position) pairs. Built from content bytes already in LiveIndex.
**When to use:** Built synchronously during `LiveIndex::load()` after all files are in the HashMap. Updated incrementally in the watcher re-index path.

```rust
// Source: project CONTEXT.md + standard trigram algorithm
pub struct TrigramIndex {
    // [u8;3] trigram -> Vec of (relative_path_hash as u32, byte_position as u32)
    // Using String as FileId for simplicity (relative_path is already the key)
    pub map: HashMap<[u8; 3], Vec<(u32, u32)>>,
    // Maps u32 file_id back to relative_path for result formatting
    pub id_to_path: HashMap<u32, String>,
    pub path_to_id: HashMap<String, u32>,
}

// Build from content bytes — O(total_bytes), no disk I/O
pub fn build_from_index(files: &HashMap<String, IndexedFile>) -> TrigramIndex { ... }

// Update single file — remove old entries, add new
pub fn update_file(&mut self, path: &str, new_content: &[u8]) { ... }

// Search — AND-intersection of posting lists
pub fn search(&self, query: &str) -> Vec<String> {  // returns relative_paths
    // 1. Extract trigrams from query
    // 2. Lookup each in map (if any missing, no results)
    // 3. Intersect posting lists — start with shortest list
    // 4. Deduplicate by file_id, return paths
}
```

**Key optimization:** Start intersection with the trigram that has the fewest matches (shortest posting list). This makes AND-intersection fast for rare trigrams even in large repos.

### Pattern 4: Scored Symbol Search
**What:** Replace unranked symbol iteration with 3-tier scoring.
**When to use:** In `search_symbols` tool handler / query function.

```rust
// Source: CONTEXT.md decisions
#[derive(PartialOrd, Ord, PartialEq, Eq)]
enum MatchTier { Exact = 0, Prefix = 1, Substring = 2 }

struct ScoredMatch<'a> {
    tier: MatchTier,
    tiebreak: u32,  // shorter name = lower number = wins for prefix
    path: &'a str,
    symbol: &'a SymbolRecord,
}

// Format output with tier headers
const EXACT_HEADER: &str = "── Exact matches ──";
const PREFIX_HEADER: &str = "── Prefix matches ──";
const SUBSTR_HEADER: &str = "── Substring matches ──";
```

### Pattern 5: C and C++ Symbol Extraction
**What:** New `src/parsing/languages/c.rs` and `cpp.rs` following the exact pattern of `rust.rs`.
**When to use:** Wired into `parse_source()` in `src/parsing/mod.rs` and `extract_symbols()` in `src/parsing/languages/mod.rs`.

**C node types for symbol extraction** (tree-sitter-c grammar, verified via crate docs):
- `function_definition` — C functions; name in `declarator` → `direct_declarator` → first `identifier`
- `struct_specifier` — structs; name in `name` child (direct `type_identifier`)
- `enum_specifier` — enums; name in `name` child
- `type_definition` — typedef; wraps struct/enum specifiers
- Note: C has no classes, methods, or namespaces — `SymbolKind::Function`, `Struct`, `Enum`, `Type` suffice

**C++ additional node types** (tree-sitter-cpp grammar, extends C):
- `class_specifier` — C++ classes; name in `name` child → `SymbolKind::Class`
- `template_declaration` — templates; recurse into the inner function/class declaration
- `namespace_definition` — namespaces; name in `name` child → `SymbolKind::Module`
- `function_definition` — same as C but also covers methods inside class bodies
- C++ name extraction: `declarator` → `function_declarator` → `qualified_identifier` or `identifier`

**C xref query patterns:**
```
; Function calls in C
(call_expression function: (identifier) @ref.call)
(call_expression function: (field_expression field: (field_identifier) @ref.method_call))

; #include imports
(preproc_include path: (string_literal) @ref.import)
(preproc_include path: (system_lib_string) @ref.import)

; Type identifiers (struct/enum usage)
(type_identifier) @ref.type

; Struct field access
(field_expression field: (field_identifier) @ref.type)
```

**C++ xref query patterns** (extends C patterns):
```
; Method calls: obj.method()
(call_expression function: (field_expression field: (field_identifier) @ref.method_call))

; Qualified calls: std::vector::push_back
(qualified_identifier name: (identifier) @ref.call)

; Template instantiation: vector<int>
(template_type name: (type_identifier) @ref.type)

; Using declarations: using std::string
(using_declaration name: (qualified_identifier) @import.original)

; Namespace alias: namespace fs = std::filesystem
(namespace_alias_definition name: (identifier) @import.alias
                             value: (nested_namespace_specifier) @import.original)
```

### Pattern 6: get_file_tree Tool
**What:** New MCP tool following the `loading_guard!` pattern in `tools.rs`.
**When to use:** Model calls `get_file_tree` to navigate directory structure without reading every file.

```rust
// Source: existing tools.rs pattern
#[derive(Deserialize, JsonSchema)]
pub struct GetFileTreeInput {
    /// Subtree path (default: project root).
    pub path: Option<String>,
    /// Max depth (default: 2, max: 5).
    pub depth: Option<u32>,
}

#[tool(description = "Browse the source file tree with symbol counts per file and directory.")]
pub fn get_file_tree(&self, Parameters(input): Parameters<GetFileTreeInput>) -> String {
    let path = input.path.as_deref().unwrap_or("");
    let depth = input.depth.unwrap_or(2).min(5);
    loading_guard!(self);
    let index = self.index.read().expect("lock not poisoned");
    format::file_tree(&index, path, depth)
}
```

### Anti-Patterns to Avoid
- **Holding RwLockReadGuard across serialization:** Extract snapshot to owned struct, drop lock, then write to disk. Never hold the read lock while doing file I/O.
- **Serializing runtime-only types:** `Instant`, `Duration`, `CircuitBreakerState` cannot/should not be serialized. Create a separate snapshot struct for persistence.
- **Persisting the trigram index:** It's fast to rebuild (~50ms) and would double the index.bin size. Rebuild from content bytes on load.
- **Persisting the reverse_index:** Same reasoning — always rebuilt by `rebuild_reverse_index()` after deserialization.
- **Registering get_file_tree before wiring `#[tool_router]`:** The `#[tool_router]` proc macro generates the router in `mod.rs`. Any new tool must appear in the `#[tool_router]` attribute list to be dispatched.
- **Using bincode:** RUSTSEC-2025-0141 — the crate is officially unmaintained. Use postcard instead.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Binary serialization | Custom byte packing | postcard 1.1.3 | Handles serde derive, version-safe header, varint encoding — edge cases in manual packing multiply quickly |
| C grammar | Manual C parser | tree-sitter-c 0.24.1 | C has 50+ years of edge cases (trigraphs, __attribute__, K&R style) |
| C++ grammar | Extend C parser manually | tree-sitter-cpp 0.23.4 | C++ grammar is a superset but with template/namespace/operator complexities |
| Signal handling | Low-level libc sigaction | tokio::signal | Integrates with async runtime, correct on all platforms |
| Trigram intersection | String contains() scan | HashMap intersection of posting lists | O(n*m) → O(k) where k is matches; 1000x faster at scale |

**Key insight:** The trigram build is the one genuinely custom algorithm in this phase. Everything else (serialization, grammar, signals) has well-maintained crate solutions.

## Common Pitfalls

### Pitfall 1: Snapshot vs LiveIndex Serialization
**What goes wrong:** Trying to add `#[derive(Serialize, Deserialize)]` directly to `LiveIndex` and hitting non-serializable fields (`Instant`, `AtomicUsize`, `Mutex`, `RwLock`, `Arc`).
**Why it happens:** `LiveIndex` is an operational runtime struct, not a data struct. It mixes configuration, metrics, and state.
**How to avoid:** Create a dedicated `IndexSnapshot` struct containing only serializable data: `files: HashMap<String, IndexedFileSnapshot>` where `IndexedFileSnapshot` mirrors `IndexedFile` but uses serde-compatible types. Reconstruct the operational LiveIndex from the snapshot after deserialization.
**Warning signs:** Compiler errors on `#[derive(Serialize)]` mentioning `Instant`, `AtomicBool`, `Mutex`.

### Pitfall 2: Stat-check Clock Skew
**What goes wrong:** mtime comparison gives false "changed" results when the filesystem has lower-than-nanosecond precision (FAT32, network drives) or when files were written quickly in succession.
**Why it happens:** mtime resolution varies: ext4 = nanosecond, FAT32 = 2-second, some NFS = 1-second.
**How to avoid:** Mark a file as "needs re-parse" if mtime OR size differs from what's in the index. If both match, accept as unchanged. The 10% spot-check with content_hash catches the rare false-negative.
**Warning signs:** Spurious re-parses on every startup despite no file changes.

### Pitfall 3: C/C++ Function Name Extraction
**What goes wrong:** `function_definition` in C doesn't have a direct `identifier` child — the name is nested: `function_definition` → `declarator` → `direct_declarator` → first `identifier`.
**Why it happens:** C's declarator grammar is recursive (pointer declarators, array declarators) so the name is buried.
**How to avoid:** Walk the `declarator` child recursively until you find a `function_declarator` node, then extract its first `identifier` or `qualified_identifier` (for C++).
**Warning signs:** Empty symbol lists for C files that parse without errors.

### Pitfall 4: tree-sitter-cpp Grammar Version vs tree-sitter ABI
**What goes wrong:** Adding `tree-sitter-cpp = "0.23.4"` alongside `tree-sitter = "0.24"` causes ABI mismatch at `set_language()` call.
**Why it happens:** tree-sitter grammar crates embed the ABI version. Version 0.23.x grammars target tree-sitter ABI 14; version 0.24.x grammars target ABI 15.
**How to avoid:** Use `tree-sitter-cpp = "0.23.4"` (latest available) — it targets ABI 14, same as `tree-sitter-c = "0.24.1"` (which **also** targets ABI 14 despite the version number difference). Verify no panic from `set_language()` in the tree_sitter_grammars.rs test.
**Warning signs:** Runtime panic "incompatible language version" when calling `set_language()`.

### Pitfall 5: Trigram Query Length < 3
**What goes wrong:** A 1- or 2-character query has no trigrams, so trigram search returns no results. But the original linear scan would return matches.
**Why it happens:** Trigrams need 3 characters by definition.
**How to avoid:** If `query.len() < 3`, fall back to linear scan (the existing behavior). Log a debug trace that fallback is in use.
**Warning signs:** `search_text "fn"` returns empty results after trigram upgrade.

### Pitfall 6: Postcard Encoding of HashMap
**What goes wrong:** `HashMap` key order is non-deterministic across runs, but postcard serializes it correctly regardless of order (each element is encoded independently). The issue is if you compare encoded bytes expecting stability — you cannot.
**Why it happens:** HashMap doesn't have a stable iteration order.
**How to avoid:** Use the version tag + full deserialization → re-index fallback approach (already in the locked decisions). Don't compare index.bin bytes for equality. On version mismatch, re-index.

### Pitfall 7: get_file_tree Exceeding Context Budget
**What goes wrong:** A large repo with depth=5 returns thousands of lines, blowing the MCP response token budget.
**Why it happens:** Default depth=2 is safe; depth=5 at project root can enumerate hundreds of directories.
**How to avoid:** The max=5 cap on depth is the right guard. Additionally, the "collapsed summary for directories beyond depth limit" collapses deep subtrees to single lines. Planner should verify the collapsed format in format.rs.

## Code Examples

Verified patterns from existing codebase and crate docs:

### Postcard Serialize/Deserialize (PLSH-04)
```rust
// Source: postcard 1.1.3 docs, use-std feature
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct IndexSnapshot {
    version: u32,
    files: HashMap<String, IndexedFileSnapshot>,
}

// Serialize
let bytes = postcard::to_stdvec(&snapshot)
    .map_err(|e| anyhow::anyhow!("failed to serialize index: {e}"))?;
std::fs::write(index_path, &bytes)?;

// Deserialize — never crash on corrupt data
let result: anyhow::Result<IndexSnapshot> = (|| {
    let bytes = std::fs::read(index_path)?;
    postcard::from_bytes(&bytes)
        .map_err(|e| anyhow::anyhow!("corrupt index.bin: {e}"))
})();
match result {
    Ok(snapshot) if snapshot.version == CURRENT_VERSION => { /* use snapshot */ }
    _ => { /* fall back to full re-index */ }
}
```

### C Grammar Symbol Extraction (LANG-01)
```rust
// Source: tree-sitter-c 0.24.1 + existing rust.rs pattern
use tree_sitter::Node;
use crate::domain::{SymbolKind, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    walk_node(node, source, 0, &mut sort_order, &mut symbols);
    symbols
}

fn walk_node(node: &Node, source: &str, depth: u32,
             sort_order: &mut u32, symbols: &mut Vec<SymbolRecord>) {
    let kind = match node.kind() {
        "function_definition" => Some(SymbolKind::Function),
        "struct_specifier" => Some(SymbolKind::Struct),
        "enum_specifier" => Some(SymbolKind::Enum),
        "type_definition" => Some(SymbolKind::Type),
        _ => None,
    };
    // ... same walk pattern as rust.rs
}

fn find_c_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" => {
            // Walk declarator → direct_declarator → first identifier
            find_function_name_in_declarator(node, source)
        }
        "struct_specifier" | "enum_specifier" => {
            // Direct "name" child → type_identifier
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "type_identifier" {
                    return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
            None
        }
        // ...
    }
}
```

### C++ Grammar Entry (LANG-02)
```rust
// Source: tree-sitter-cpp 0.23.4 docs
// In src/parsing/mod.rs parse_source() match arm:
LanguageId::C => tree_sitter_c::LANGUAGE.into(),
LanguageId::Cpp => tree_sitter_cpp::LANGUAGE.into(),
```

### Shutdown Signal Integration (PLSH-04)
```rust
// Source: tokio::signal docs + existing main.rs
use tokio::signal;

// Replace service.waiting().await? with:
tokio::select! {
    result = service.waiting() => { result?; }
    _ = signal::ctrl_c() => {
        tracing::info!("Ctrl+C received, shutting down cleanly");
    }
    #[cfg(unix)]
    _ = async {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM");
        sigterm.recv().await
    } => {
        tracing::info!("SIGTERM received, shutting down cleanly");
    }
}
// Serialize index here (after select! resolves)
```

### Trigram Search Example
```rust
// Source: project CONTEXT.md + standard CS algorithm
fn extract_trigrams(bytes: &[u8]) -> Vec<[u8; 3]> {
    bytes.windows(3)
         .map(|w| [w[0], w[1], w[2]])
         .collect()
}

fn search(&self, query: &[u8]) -> Vec<String> {
    if query.len() < 3 {
        return self.linear_fallback(query);  // PITFALL-5 guard
    }
    let trigrams: Vec<[u8; 3]> = extract_trigrams(query);

    // Sort by posting list length (shortest first) — PITFALL-optimized
    let mut lists: Vec<&Vec<(u32, u32)>> = trigrams.iter()
        .filter_map(|t| self.map.get(t))
        .collect();
    if lists.len() < trigrams.len() { return vec![]; }  // Any trigram missing = no match
    lists.sort_by_key(|l| l.len());

    // Intersect file_ids
    let mut candidates: std::collections::HashSet<u32> =
        lists[0].iter().map(|(file_id, _)| *file_id).collect();
    for list in &lists[1..] {
        let set: std::collections::HashSet<u32> = list.iter().map(|(f, _)| *f).collect();
        candidates.retain(|f| set.contains(f));
    }
    candidates.iter()
              .filter_map(|id| self.id_to_path.get(id))
              .cloned()
              .collect()
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| bincode 2.x | postcard 1.1.3 | Late 2025 (RUSTSEC-2025-0141) | bincode unmaintained; postcard is community-endorsed drop-in |
| Linear scan in search_text | Trigram index + AND-intersection | Phase 7 | 10,000x improvement for large repos |
| Unranked symbol search | 3-tier scored search | Phase 7 | Observable ranking satisfies success criteria #3 |
| LanguageId::C/Cpp as Unsupported | Full tree-sitter parsing | Phase 7 | Variants already defined in domain/index.rs; just add parsing |

**Deprecated/outdated:**
- bincode: Do NOT use, even 2.0.1. RUSTSEC advisory issued.
- `bincode::config::standard()` / `encode_to_vec` / `decode_from_slice` API: Replace with postcard equivalents.

## Open Questions

1. **bincode vs postcard decision escalation**
   - What we know: User locked "bincode 2.x" in CONTEXT.md. bincode is RUSTSEC-2025-0141 unmaintained as of late 2025.
   - What's unclear: User may or may not know about the advisory. The underlying goal (fast binary serialization) is better served by postcard.
   - Recommendation: Planner should document the substitution prominently. The implementation should use postcard. If user wants bincode 2.0.1 explicitly knowing the advisory, they can revert — but default to postcard as it is safer and nearly identical API.

2. **tree-sitter-cpp ABI version**
   - What we know: `tree-sitter-cpp = "0.23.4"` is latest on crates.io. `tree-sitter-c = "0.24.1"` also available. The project uses `tree-sitter = "0.24"`.
   - What's unclear: Whether 0.23.x grammars are ABI-compatible with tree-sitter 0.24. In the tree-sitter ecosystem, grammar ABI 14 is supported by tree-sitter 0.23 and 0.24.
   - Recommendation: Add both to Cargo.toml and verify with a `test_c_grammar_loads_and_parses` + `test_cpp_grammar_loads_and_parses` test in `tests/tree_sitter_grammars.rs` (following existing pattern). Fail fast rather than discover at runtime.

3. **IndexedFile serde derives vs separate snapshot struct**
   - What we know: `IndexedFile` has `HashMap<String, String>` (alias_map, fully serializable), `Vec<SymbolRecord>`, `Vec<ReferenceRecord>` (both cloneable). `ParseStatus` has String variants. All serializable fields.
   - What's unclear: Whether to add `#[derive(Serialize, Deserialize)]` directly to `IndexedFile` or create a separate `IndexedFileSnapshot`. The snapshot approach is cleaner but requires a conversion step.
   - Recommendation: Use a separate snapshot struct for `IndexedFile` since `content: Vec<u8>` is potentially large (100KB+ files). The snapshot can omit or include content based on a trade-off. For now, include content bytes in the snapshot (it's already in RAM). This matches the zero-disk-I/O goal on the read path.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness + `cargo test` |
| Config file | Cargo.toml (no separate test config) |
| Quick run command | `cargo test --lib 2>&1 \| grep -E "test .* (ok\|FAILED)"` |
| Full suite command | `cargo test 2>&1` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PLSH-01 | Trigram search returns correct files in <10ms | unit | `cargo test --lib trigram 2>&1` | Wave 0: `src/live_index/trigram.rs` |
| PLSH-01 | Trigram search falls back to linear scan for <3-char queries | unit | `cargo test --lib trigram 2>&1` | Wave 0: `src/live_index/trigram.rs` |
| PLSH-02 | search_symbols returns exact before prefix before substring | unit | `cargo test --lib scored_search 2>&1` | Wave 0: query.rs test |
| PLSH-03 | get_file_tree returns correct depth-limited tree | integration | `cargo test --test live_index_integration test_file_tree 2>&1` | ❌ Wave 0 |
| PLSH-04 | Serialize + deserialize round-trip preserves all files and symbols | unit | `cargo test --lib persist 2>&1` | Wave 0: `src/live_index/persist.rs` |
| PLSH-04 | Corrupt index.bin falls back to full re-index without crash | unit | `cargo test --lib persist 2>&1` | Wave 0: `src/live_index/persist.rs` |
| PLSH-04 | Version mismatch triggers re-index | unit | `cargo test --lib persist 2>&1` | Wave 0: `src/live_index/persist.rs` |
| PLSH-05 | Stat-check detects changed files and queues re-parse | unit | `cargo test --lib persist 2>&1` | Wave 0: `src/live_index/persist.rs` |
| LANG-01 | C grammar loads without panic | integration | `cargo test --test tree_sitter_grammars test_c_grammar 2>&1` | ❌ Wave 0 |
| LANG-01 | C function/struct/enum symbols extracted correctly | unit | `cargo test --lib c_language 2>&1` | Wave 0: `src/parsing/languages/c.rs` |
| LANG-02 | C++ grammar loads without panic | integration | `cargo test --test tree_sitter_grammars test_cpp_grammar 2>&1` | ❌ Wave 0 |
| LANG-02 | C++ class/namespace/template symbols extracted | unit | `cargo test --lib cpp_language 2>&1` | Wave 0: `src/parsing/languages/cpp.rs` |

### Sampling Rate
- **Per task commit:** `cargo test --lib 2>&1`
- **Per wave merge:** `cargo test 2>&1`
- **Phase gate:** Full suite green + stdout purity test passes before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src/live_index/trigram.rs` — covers PLSH-01 (TrigramIndex struct + unit tests)
- [ ] `src/live_index/persist.rs` — covers PLSH-04, PLSH-05 (serialize/deserialize + version check + stat-check)
- [ ] `src/parsing/languages/c.rs` — covers LANG-01 (C symbol extraction)
- [ ] `src/parsing/languages/cpp.rs` — covers LANG-02 (C++ symbol extraction)
- [ ] `tests/tree_sitter_grammars.rs` additions — `test_c_grammar_loads_and_parses`, `test_cpp_grammar_loads_and_parses` (extend existing file)
- [ ] Cargo.toml additions: `postcard = { version = "1.1.3", features = ["use-std"] }`, `tree-sitter-c = "0.24.1"`, `tree-sitter-cpp = "0.23.4"`

## Sources

### Primary (HIGH confidence)
- Cargo.toml in project root — verified current tree-sitter = "0.24", no bincode or postcard present
- `src/domain/index.rs` — verified LanguageId::C and LanguageId::Cpp already defined with correct extensions
- `src/parsing/mod.rs` — verified parse_source() dispatch pattern; C/Cpp fall through to "not yet onboarded" error
- `src/live_index/store.rs` — verified IndexedFile structure; confirmed non-serializable fields (Instant, AtomicUsize, Mutex)
- `src/main.rs` — verified shutdown pattern; `service.waiting().await?` is the correct hook point
- `src/parsing/xref.rs` — verified query string pattern for C/C++ xref queries
- `src/parsing/languages/rust.rs` — verified walk_node pattern for C/C++ symbol extraction
- `cargo search bincode` — confirmed bincode = "3.0.0" latest, but docs.rs build failed
- `cargo search postcard` — confirmed postcard = "1.1.3" latest
- `cargo search tree-sitter-c` — confirmed tree-sitter-c = "0.24.1"
- `cargo search tree-sitter-cpp` — confirmed tree-sitter-cpp = "0.23.4"
- postcard docs.rs (to_stdvec, from_bytes) — confirmed use-std feature, serde compatibility

### Secondary (MEDIUM confidence)
- RUSTSEC-2025-0141: bincode unmaintained — reported by multiple community sources (tarpc issue #544, libsql issue #2207)
- tree-sitter-c LANGUAGE constant name — verified via docs.rs crate page
- tree-sitter-cpp grammar node types (class_specifier, template_declaration, namespace_definition) — verified via grammar.js search results
- tokio::signal shutdown pattern — verified via tokio.rs official docs

### Tertiary (LOW confidence)
- C function_definition declarator walk — based on tree-sitter-c grammar structure knowledge; must be verified in Wave 0 by writing and running the C parser test

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — crate versions verified via cargo search; postcard API verified via docs.rs
- Architecture: HIGH — all integration points verified against existing source files
- Pitfalls: HIGH for Rust/serialization pitfalls; MEDIUM for C/C++ grammar pitfalls (grammar node walk needs empirical verification)
- Bincode finding: HIGH — RUSTSEC advisory confirmed by multiple independent sources

**Research date:** 2026-03-10
**Valid until:** 2026-04-10 (stable domain; postcard + tree-sitter crates are well-maintained)

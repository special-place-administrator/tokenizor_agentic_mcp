# Explore Semantic Relevance Overhaul — Design Spec

## Problem

The `explore` tool is the entry point for codebase discovery but the weakest tool in the suite. Two independent reviews (both 8.5/10) identified it as the biggest improvement opportunity.

Specific failures:
- "error handling" returns `stopDaemonProcesses` (npm script) and `getInstalledVersion`, misses `src/error.rs` and `TokenizorError`
- "file watcher debounce" returns `OutputLimits` (output formatting struct), misses `BurstTracker` and `WatcherInfo`
- The tool finds code that *contains* patterns (try/catch, unwrap), not code *about* the concept

Root causes:
1. `CONCEPT_MAP` has only 7 static entries — no coverage for file watching, parsing, indexing, serialization, etc.
2. `match_concept` is all-or-nothing — "error handling in the watcher" matches "error handling" concept and ignores "watcher" entirely
3. No awareness of project structure — "watcher" as a query term doesn't know that `src/watcher/` is a module full of relevant symbols

## Design

### 1. Module-path boosting

Add a Phase 0 to the explore handler, before symbol and text search. **Phase 0 runs unconditionally** — both for concept matches and fallback queries. For concept matches, Phase 0 uses the remainder terms (if any). For fallback queries, it uses all terms.

For each query term, scan indexed file paths for path segments matching the term. Matching rules:
- **Exact segment match** (e.g., "watcher" == "watcher" in `src/watcher/mod.rs`): inject symbols with weight **+2**
- **Substring segment match** (e.g., "watch" in "watcher"): inject symbols with weight **+1**

This two-tier weighting prevents common directory names like "test" or "src" from overwhelming results — a query for "test" would give +2 to `tests/` symbols but only +1 to `src/testing_utils.rs` symbols, and the per-directory cap (below) limits total injection.

**Per-directory cap:** Inject at most `limit` symbols per matched directory to prevent a large directory (e.g., `tests/` with 174 symbols) from flooding `match_counts`. Within the cap, prefer symbols that already have matches from other phases (i.e., inject all, but the cap prevents extreme cases).

Effective ranking:
- A symbol from `src/watcher/mod.rs` gets +2 from exact module match for "watcher"
- A symbol named `FileWatcher` elsewhere from symbol search gets +1
- A symbol hitting both (e.g., `WatcherInfo` in `src/watcher/mod.rs`) gets +3
- Regular "file" matches get +1 each, losing to the +2/+3 module-boosted results

**Path matching:** Split the file path on `/` and `\`, compare each segment against the query term (case-insensitive). Exact match = +2, substring match = +1.

**Performance:** Iterating all file paths is O(files × terms). With 326 files and 3 terms, this is ~1000 string comparisons — negligible.

### 2. Concept + remainder merging

Change `match_concept` to return the matched concept AND the remaining query terms that weren't part of the concept key.

Current signature:
```rust
pub fn match_concept(query: &str) -> Option<&'static ConceptPattern>
```

New signature:
```rust
pub fn match_concept(query: &str) -> Option<(&'static str, &'static ConceptPattern)>
```

Returns the matched key and its pattern. The caller computes remainder terms from the query minus the matched key. This keeps `match_concept` a pure lookup with no allocation.

**Remainder computation (in the explore handler, not in match_concept):**
1. Remove the matched key's words from the query
2. Apply stopword filter + length filter (>= 3 chars)
3. Stopword set: `{"a", "an", "the", "in", "on", "of", "for", "to", "and", "or", "is", "it", "my", "at", "by", "do", "no", "so", "up", "if", "with", "from", "this", "that"}`

Example:
- Query: "error handling in the watcher"
- Concept match key: "error handling"
- Query minus key words: ["in", "the", "watcher"]
- After stopword + length filter: ["watcher"]

In the `explore` handler, when a concept matches:
1. Run concept's curated `symbol_queries` and `text_queries` into `match_counts` (existing behavior)
2. Compute remainder terms from query minus concept key
3. If remainder is non-empty, run remainder through the full fallback path (module boosting + symbol search + text search with enclosing symbol injection) into the same `match_counts`
4. Sort and truncate as normal

This means "error handling in the watcher" produces results from BOTH the error-handling concept (curated queries for Error, Result, unwrap) AND the watcher fallback (module boost for `src/watcher/`, symbol/text search for "watcher"). Symbols matching both rank highest.

### 3. Expand CONCEPT_MAP

Add ~10 universal concepts covering the gaps identified by reviews:

```rust
("file watching", ConceptPattern {
    label: "File Watching",
    symbol_queries: &["watcher", "notify", "debounce", "event"],
    text_queries: &["notify::Event", "DebouncedEvent", "file_event", "inotify"],
    kind_filters: &[],
}),
("parsing", ConceptPattern {
    label: "Parsing",
    symbol_queries: &["parse", "parser", "ast", "node", "tree_sitter"],
    text_queries: &["tree_sitter::", ".parse(", "syntax tree", "grammar"],
    kind_filters: &[],
}),
("serialization", ConceptPattern {
    label: "Serialization",
    symbol_queries: &["serialize", "deserialize", "serde", "json", "postcard"],
    text_queries: &["#[derive(Serialize", "#[derive(Deserialize", "serde_json", "postcard::"],
    kind_filters: &[],
}),
("indexing", ConceptPattern {
    label: "Indexing",
    symbol_queries: &["index", "reindex", "snapshot", "persist"],
    text_queries: &["LiveIndex", "index.bin", "reindex", "rebuild_reverse"],
    kind_filters: &[],
}),
("logging", ConceptPattern {
    label: "Logging / Observability",
    symbol_queries: &["log", "trace", "span", "metric", "telemetry"],
    text_queries: &["tracing::", "log::", "debug!", "warn!", "info!"],
    kind_filters: &[],
}),
("cli", ConceptPattern {
    label: "CLI / Command Line",
    symbol_queries: &["cli", "args", "command", "subcommand"],
    text_queries: &["clap", "structopt", "Arg::", "Command::new"],
    kind_filters: &[],
}),
("networking", ConceptPattern {
    label: "Networking",
    symbol_queries: &["socket", "listener", "bind", "connect", "server"],
    text_queries: &["TcpListener", "hyper", "axum", "reqwest", "tonic"],
    kind_filters: &[],
}),
("caching", ConceptPattern {
    label: "Caching",
    symbol_queries: &["cache", "lru", "memoize", "ttl", "expire"],
    text_queries: &["LruCache", "cache.get(", "cached::", "moka::"],
    kind_filters: &[],
}),
("permissions", ConceptPattern {
    label: "Permissions / Authorization",
    symbol_queries: &["permission", "role", "policy", "acl", "authorize"],
    text_queries: &["forbidden", "unauthorized", "access_control", "RBAC"],
    kind_filters: &[],
}),
("deployment", ConceptPattern {
    label: "Deployment / Release",
    symbol_queries: &["release", "deploy", "version", "publish", "migrate"],
    text_queries: &["npm publish", "cargo publish", "release-please", "changelog"],
    kind_filters: &[],
}),
```

Also add aliases so both "file watching" and "watcher" match:
```rust
("watcher", /* same pattern as "file watching" */),
("parser", /* same pattern as "parsing" */),
```

**Key matching precision:** Short concept keys like "cli" and "api" risk substring collisions ("public**cli**ent" matches "cli", "c**api**tal" matches "api"). Fix:
1. Sort `CONCEPT_MAP` entries by key length descending so longer, more specific keys match first ("authentication" before "api", "command line" before "cli")
2. Use word-boundary matching: split both the query and the concept key on `split_whitespace()`, then check if the key's words appear as a contiguous subsequence of the query's words. "cli tools" matches "cli", but "clinical" does not. "error handling" matches as a 2-word subsequence in "error handling in the watcher". Underscore-separated terms are programming identifiers, not natural language — `split_whitespace()` is sufficient.

**Call site updates for `match_concept` signature change:** 1 production call site (`tools.rs` explore handler) and 3 test call sites (`explore.rs` tests) need updating to destructure the new return type.

### 4. Files modified

| File | Change |
|------|--------|
| `src/protocol/explore.rs` | Expand CONCEPT_MAP (~10 entries + aliases), change `match_concept` return type to include remainder terms |
| `src/protocol/tools.rs` | Add Phase 0 module-path boosting in `explore` handler, implement concept+remainder merging |

### 5. Testing

- **Module boosting:** file at `src/watcher/mod.rs` with `BurstTracker` → query "watcher" → `BurstTracker` appears (module boost +2 beats unrelated +1 matches)
- **Concept + remainder:** query "error handling in the watcher" → results include both error-handling concept hits AND watcher module hits
- **Expanded concepts:** query "parsing" → finds tree-sitter related symbols via concept
- **Existing tests:** all current explore tests continue passing

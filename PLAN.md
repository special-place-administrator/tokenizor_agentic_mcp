# Tokenizor — Development Plan

Current version: v0.23.0 | 24 MCP tools | 91 files indexed | 3115 symbols | 975 tests

All v1.0 (7 phases) and v2.0 (5 phases) milestones are complete.
This plan covers remaining improvements and new features.

---

## Sprint 1: Quick Fixes

Small effort, high value. Can ship together in one release.

- [ ] **daemon_degraded periodic reset** — `daemon_degraded` flag in `src/protocol/mod.rs:56` is set permanently after one failed reconnect. Add periodic retry (e.g. every 5min) instead of permanent degradation. The reconnect logic already exists in `proxy_tool_call` (lines 131-201).
- [ ] **ParseStatus warning banner** — `PartialParse` status not surfaced in tool output. Users see no warning when working with incomplete parse data. Add warning banner to affected tool responses. Files: `src/protocol/format.rs`, `src/protocol/tools.rs`.
- [ ] **around_line out-of-range error** — `get_file_content` with `around_line=999999` returns silently empty. Return "Line N exceeds file length (M lines)" instead. LLMs retry endlessly on empty responses.
- [ ] **diff_symbols phantom file count** — Says "2 files changed" but only shows details for 1 when a file changed without symbol boundary modifications. Add "N files changed but no symbol boundaries affected" note.
- [ ] **get_symbol_context kind labels** — With `verbosity=signature`, callers section labels all symbols as "fn" (e.g. "in fn Result" should be "in type Result", "in fn TokenizorError" should be "in enum TokenizorError").
- [ ] **analyze_file_impact stale co-changes** — Git temporal data references deleted files from pre-v2 rewrite (e.g. `src/protocol/mcp.rs`). Filter out files that no longer exist in the working tree.

## Sprint 2: Explore Improvements

Medium effort, highest UX impact per two independent reviews (both rated explore as weakest tool).

- [ ] **Expand CONCEPT_MAP** — "error handling" misses `src/error.rs`, returns npm scripts instead. Biggest gap in the entry-point tool. File: `src/protocol/explore.rs`.
- [ ] **Multi-concept matching** — "error handling and retry" only finds error-related results, ignores retry. `match_concept` should return multiple concepts for compound queries, then merge results.
- [ ] **Word-boundary relevance tuning** — Pattern matching finds code that *contains* error-handling patterns, not code *about* error handling. Improve scoring to weight definition-site matches higher.

## Sprint 3: Unified Detail Levels

Medium-large effort. Prerequisite for Sprint 4 (session intelligence).

- [ ] **Add `detail: Option<u8>` param** — Single param (1-5) replacing per-tool noise flags (`include_tests`, `code_only`, `verbosity`, `compact`).
  - 1 = minimal (names + counts, ~100 tokens)
  - 2 = summary (signatures + hidden-result counts) — DEFAULT
  - 3 = standard (current behavior)
  - 4 = detailed (include tests, generated files, all categories)
  - 5 = everything (zero filtering, zero truncation)
- [ ] **Internal flag mapping** — `detail` maps to existing filter flags. Old flags still work, detail overrides when present.
- [ ] **Progressive disclosure hints** — Every level shows what's hidden: "Showing 5/23 references (detail=2). 18 in test modules."

## Sprint 4: Session Intelligence

Large effort, transformative. Moves Tokenizor from stateless tool to context-aware assistant.

- [ ] **set_focus** — Persistent focus state on `TokenizorServer` (path, symbol, intent). All tools auto-rank by proximity. Auto-set from recent tool calls.
- [ ] **Intent-driven prepare** — One call assembles context for a specific task:
  - `intent: "edit"` → bundle + find_references(compact) + git activity
  - `intent: "review"` → diff_symbols + callers + recent commits
  - `intent: "debug"` → call chain + error patterns + recent changes
- [ ] **follow_calls** — Walk callee/caller graph N levels deep with signature-only verbosity. `direction: "down"/"up"/"both"`, `depth: 1-5`, tree output.

## Sprint 5: Extended Capabilities

Various effort levels. Lower priority, tackle opportunistically.

- [ ] **Cross-crate/workspace dependency tracking** — `find_dependents` can't see cross-crate Cargo workspace dependencies. Needs Cargo.toml/package.json/go.mod parsing. Significant new feature.
- [ ] **Cross-language false positives in edit warnings** — `replace_symbol_body` on Rust `add` flags Python's `task_queue.py`. Add language-scoped reference filtering.
- [ ] **Error path test coverage** — No tests for `PartialParse`/`Failed` states, parse failures, corrupted snapshots, git temporal failures.
- [ ] **Tree-sitter grammar unification** — Python/JS/Go at ^0.25.8, Rust/TS at 0.24.x. PHP/Swift/Perl ABI 15 incompatible with tree-sitter 0.24 host. Coordinated upgrade needed.

## Sprint 11: Structured Config/Doc File Parsing (Tier 1)

**Goal**: Make JSON, TOML, YAML, Markdown, and .env files first-class in the LiveIndex via pseudo-extractors that produce symbols from file structure. All existing tools (search, navigation, edit) work on these files without modification.

### Task 11.1 — Pseudo-Extractor Framework

Create extraction path for config files that outputs the same `SymbolEntry` structs tree-sitter extractors produce. Register new extractors by file extension in the parsing pipeline.

**Symbol mapping:**
| Format | Symbol name | Kind | Byte range |
|--------|------------|------|------------|
| JSON | `scripts.build` (dot-joined key path) | `key` | value span |
| TOML | `dependencies.serde.version` | `key` | value span |
| YAML | `services.api.ports` | `key` | value span |
| Markdown | `Installation` (header text) | `section` | header to next same-level header |
| .env | `DATABASE_URL` | `variable` | full line span |

**Files**: `src/parsing/config_extractors.rs` (new), `src/parsing/mod.rs`, `src/live_index/`

### Task 11.2 — JSON Extractor

Parse with `serde_json`. Walk value tree, emit symbols for keys with dot-joined paths. Arrays: `key[0]`, `key[1]`. Depth limit: 6 levels. Dependency: `serde_json` (already in deps).

### Task 11.3 — TOML Extractor

Parse with `toml_edit` (already in deps). Walk tables, emit dot-joined key paths. Same pattern as JSON.

### Task 11.4 — YAML Extractor

Parse with `serde_yaml` or `yaml-rust2`. Walk mapping nodes, emit dot-joined key paths. New dependency needed.

### Task 11.5 — Markdown Extractor

Regex-based (no dependency). Scan for ATX headers (`# `, `## `, etc.). Each header → section symbol spanning to next same-or-higher-level header or EOF. Nesting via dot-path: `Section.Subsection`.

### Task 11.6 — .env Extractor

Line-by-line scan. `KEY=value` → symbol with kind `variable`. Skip comments (`#`). Byte range = full line.

### Task 11.7 — Edit Tool Verification

Verify `replace_symbol_body`, `edit_within_symbol`, `delete_symbol` work with config-file pseudo-symbols. Byte ranges must be splice-accurate. Watch for JSON validity (brackets/commas), YAML whitespace, Markdown section boundaries.

### Task 11.8 — PreToolUse Hook Update

Update `is_non_source_path` in `src/cli/hook.rs` to intercept config files once Tokenizor handles them. Currently skips `.md`, `.json`, `.toml` — after this sprint, those should trigger Tokenizor suggestions.

### Task 11.9 — Tests

Unit tests per extractor (symbol names, kinds, byte ranges). Integration tests (tools end-to-end on config files). Edge cases: empty files, deeply nested JSON, multi-document YAML, frontmatter Markdown.

**Acceptance criteria:**
- `search_symbols(name="dependencies")` finds TOML/JSON dependency keys
- `get_file_context(path="Cargo.toml")` returns structured outline of keys
- `get_file_content(path="README.md", around_symbol="Installation")` works
- `get_symbol(path="package.json", name="scripts.build")` returns the value
- File watcher re-indexes config files on change

## Sprint 12: Frontend Asset Parsing (Angular/CSS/SCSS)

**Goal**: Add tree-sitter-based parsing for Angular HTML templates, CSS, and SCSS. These use the existing tree-sitter pipeline (same as .ts/.js), not the config extractor path.

### Task 12.1 — Angular HTML Templates (.html)

**First-class support.** Use `tree-sitter-angular` crate (v0.6.1, ABI 14, compatible with 0.24.x host). Extends `tree-sitter-html` with full Angular syntax support through v21.

Add `LanguageId::AngularTemplate`. Detect Angular projects via `angular.json` presence — if found, treat `.html` as Angular templates; otherwise generic HTML.

**Symbol extraction:**
- Component/element blocks
- `<ng-template>` blocks
- Template reference variables (`#ref`)
- Interpolation regions (`{{ }}`)
- Property/event/two-way bindings (`[prop]`, `(event)`, `[(ngModel)]`)
- Control-flow blocks (`@if`, `@for`, `@switch`, `@defer`, `@let`)

**Edit capability:** `TextEditSafe`. Structural edits deferred — template regions can be syntactically valid HTML while Angular-fragile across bindings or control-flow blocks.

### Task 12.2 — CSS (.css)

**First-class support, pending ABI compatibility.** `tree-sitter-css` v0.25.0 targets ABI 15. Dedicated spike required: either pin an older compatible version or rebuild with `--abi=14`.

Add `LanguageId::Css`.

**Symbol extraction:**
- Selector blocks (`.class`, `#id`, `element`)
- At-rules (`@media`, `@keyframes`, `@import`)
- Custom property declarations (`--var-name`)

**Edit capability:** `TextEditSafe`, potentially `StructuralEditSafe` for full rule blocks after validation.

### Task 12.3 — SCSS (.scss)

**Provisional support — lower confidence.** `tree-sitter-scss` (serenadeai, v1.0.0) has open issues around interpolation and `@use` scoping. Community-maintained, not under tree-sitter org.

Add `LanguageId::Scss`.

**Symbol extraction:**
- Selector blocks
- Variables (`$var`)
- Mixins (`@mixin`)
- Includes (`@include`)
- At-rules

**Edit capability:** `TextEditSafe` only. Structural edit guarantees deferred until grammar behavior validated on real project files.

### Task 12.4 — ABI Compatibility Spike

Dedicated task: verify CSS (v0.25.0, ABI 15) and SCSS grammar compatibility with the 0.24.x tree-sitter host. Options: pin older grammar versions, regenerate with `--abi=14`, or upgrade host tree-sitter. Note: Sprint 5 already flags ABI 15 incompatibility for PHP/Swift/Perl — this spike may overlap with that coordinated upgrade.

### Task 12.5 — Tests

- Angular template: extract components, bindings, control-flow blocks, `ng-template`, template refs
- CSS: selectors, at-rules, custom properties, nested media queries
- SCSS: variables, mixins, includes, interpolation edge cases
- Angular detection: `angular.json` present → AngularTemplate; absent → generic HTML
- Edit capability gating per file type

## Sprint 21: Full-Text Search Extension (Tier 2 — Future)

**Goal**: Extend `search_text` FTS index to cover non-source files. One search covers code + docs + configs.

### Task 21.1 — Index non-source text files in FTS

Currently `search_text` only indexes tree-sitter-parsed files. Extend to index any text file under project root (exclude binaries, node_modules, .git, images). Reuse existing FTS infrastructure.

### Task 21.2 — Search result grouping

Add `group_by: "type"` mode that separates results into source vs config vs docs sections.

## Sprint 31: Semantic Embeddings (Tier 3 — Future)

**Goal**: Enable semantic search across all files using local embeddings. No external API calls.

### Task 31.1 — Local embedding model integration

Evaluate `gte-small` or `all-MiniLM-L6-v2` via ONNX runtime. ~30MB model, in-process inference.

### Task 31.2 — Chunk and embed on index

Chunk files into ~512 token segments, embed, store in in-memory vector index. Re-embed on file change.

### Task 31.3 — Semantic search tool

New mode on `explore`: `explore(query="deployment configuration", mode="semantic")` returns ranked file chunks by cosine similarity.

---

## Completed (for reference)

- v1.0: LiveIndex foundation, MCP tools parity, file watcher, cross-references, HTTP sidecar, hook enrichment, polish & persistence (7 phases, 22 plans)
- v2.0: Recursive type resolution, trait/impl mapping, enriched file context, dependency visualization, git temporal context (5 phases)
- Tool consolidation: 34 → 24 tools
- Doc comment ranges (16 languages), explore relevance overhaul, non-blocking cold-start, incremental reverse index
- find_references total limit, search_symbols file count fix, find_references cross-file type refs fix

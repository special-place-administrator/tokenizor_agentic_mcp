# Story 2.4: Extend Indexing Through a Repeatable Broader-Language Onboarding Pattern

Status: review

## Story

As a power user,
I want Tokenizor to extend indexing through a repeatable broader-language onboarding pattern,
so that additional languages can be added as implementation-sized follow-on slices instead of one oversized parity story.

## Acceptance Criteria

1. **Given** one explicitly named broader-language slice (Java) outside the initial quality-focus set is onboarded through the new pattern
   **When** an indexing run executes against a repository containing Java files
   **Then** Tokenizor discovers, processes, and persists usable file-level indexing outputs plus symbol/file metadata for the onboarded slice
   **And** the onboarded slice uses the same run, commit, and inspection contracts as the initial quality-focus stories without redesigning the overall indexing lifecycle

2. **Given** files in the onboarded broader-language slice (Java) fail parsing, extraction, or commit-time validation
   **When** processing continues
   **Then** Tokenizor records explicit per-file failure outcomes for those files
   **And** the onboarding pattern reuses the shared failure-isolation behavior rather than requiring one-off handling for that language

3. **Given** a repository also contains broader baseline languages that have not yet been onboarded through the pattern (e.g., C, C++, C#, Ruby, PHP, Swift, Dart, Perl, Elixir)
   **When** indexing completes
   **Then** Tokenizor reports only the explicitly onboarded slice as supported for that run
   **And** it preserves an inspectable not-yet-supported outcome for the remaining broader-language files instead of implying full baseline parity coverage

## Tasks / Subtasks

- [x] Task 1: Extend `LanguageId` enum with all broader baseline language variants (AC: #1, #3)
  - [x] 1.1: Add variants to `LanguageId`: `Java`, `C`, `Cpp`, `CSharp`, `Ruby`, `Php`, `Swift`, `Dart`, `Perl`, `Elixir`
  - [x] 1.2: Update `from_extension` — Java: `"java"`, C: `"c"`, `"h"`, Cpp: `"cpp"`, `"cxx"`, `"cc"`, `"hpp"`, `"hxx"`, `"hh"`, CSharp: `"cs"`, Ruby: `"rb"`, Php: `"php"`, Swift: `"swift"`, Dart: `"dart"`, Perl: `"pl"`, `"pm"`, Elixir: `"ex"`, `"exs"`
  - [x] 1.3: Update `extensions` — return correct extension slices for each new variant
  - [x] 1.4: Update `support_tier` — Java returns `SupportTier::Broader`, all others return `SupportTier::Unsupported`
  - [x] 1.5: Unit tests — `from_extension` for all new extensions, `support_tier` mapping correctness, serde round-trip for new variants

- [x] Task 2: Handle broader languages in parser (`src/parsing/mod.rs`) (AC: #1, #3)
  - [x] 2.1: Update `parse_source` match on `LanguageId` — add `Java => tree_sitter_java::LANGUAGE.into()`
  - [x] 2.2: Add catch-all arm for unsupported languages — return `Err("language not yet onboarded for parsing: {language:?}")`
  - [x] 2.3: Add `tree-sitter-java` dependency to `Cargo.toml` (use version `0.23` to match existing grammar crate versions)
  - [x] 2.4: Update `extract_symbols` dispatcher in `src/parsing/languages/mod.rs` — add `Java => java::extract_symbols(node, source)`, catch-all for unsupported returns empty `Vec`
  - [x] 2.5: Create `src/parsing/languages/java.rs` — Java symbol extraction following established Go/Rust extractor pattern
  - [x] 2.6: Register `mod java;` in `src/parsing/languages/mod.rs`

- [x] Task 3: Implement Java symbol extractor (`src/parsing/languages/java.rs`) (AC: #1)
  - [x] 3.1: Implement `extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord>` with `walk_node` recursive pattern
  - [x] 3.2: Map tree-sitter-java node kinds to `SymbolKind`:
    - `class_declaration` -> `SymbolKind::Class`
    - `interface_declaration` -> `SymbolKind::Interface`
    - `enum_declaration` -> `SymbolKind::Enum`
    - `method_declaration` -> `SymbolKind::Method`
    - `constructor_declaration` -> `SymbolKind::Function`
    - `field_declaration` -> `SymbolKind::Variable`
    - `constant_declaration` -> `SymbolKind::Constant`
    - `record_declaration` -> `SymbolKind::Class` (Java 16+ records)
    - `annotation_type_declaration` -> `SymbolKind::Interface`
  - [x] 3.3: Extract symbol names from `name` child node (identifier) — same pattern as other extractors
  - [x] 3.4: Handle depth tracking for nested classes, inner methods, etc.
  - [x] 3.5: Unit tests — parse sample Java class with methods, interface, enum, constructor, verify SymbolRecord output

- [x] Task 4: Update pipeline for multi-tier language handling (AC: #3)
  - [x] 4.1: In `process_discovered`, partition `files` into `indexable` (tier is `QualityFocus` or `Broader`) and `not_yet_supported` (tier is `Unsupported`)
  - [x] 4.2: Add `not_yet_supported: std::collections::BTreeMap<LanguageId, u64>` field to `PipelineResult` — counts per language
  - [x] 4.3: Count not-yet-supported files by language before processing indexable files
  - [x] 4.4: Log at `info!` level: `"discovered {count} not-yet-supported files across {n} languages"` (one line total, NOT per-file)
  - [x] 4.5: Process only `indexable` files through the existing pipeline loop (bounded concurrency, circuit breaker, CAS commit)
  - [x] 4.6: Update `total_files` progress counter to reflect only indexable files
  - [x] 4.7: Update pipeline finish summary to include not-yet-supported breakdown
  - [x] 4.8: Unit tests — pipeline with mixed supported/unsupported files, verify unsupported are counted but not processed

- [x] Task 5: Update commit guard for broader tier (AC: #1, #2)
  - [x] 5.1: In `commit_file_result`, change guard from `!= SupportTier::QualityFocus` to `== SupportTier::Unsupported` — allow both `QualityFocus` and `Broader`
  - [x] 5.2: Update error message to `"language {:?} is not onboarded for indexing"`
  - [x] 5.3: Unit test — commit with `Broader` tier succeeds, commit with `Unsupported` tier returns error

- [x] Task 6: Integration testing (AC: #1, #2, #3)
  - [x] 6.1: End-to-end test: create temp repo with `.java` files -> run pipeline -> verify FileRecords persisted in registry -> verify CAS blobs exist on disk
  - [x] 6.2: Test: Java file with valid class/method -> `Committed` outcome with correct symbols
  - [x] 6.3: Test: Java file with syntax errors -> `PartialParse` outcome, reuses shared failure-isolation
  - [x] 6.4: Test: mixed repo with Java + not-yet-supported files (e.g., `.rb`, `.cs`) -> Java processed, others reported as not-yet-supported with correct counts
  - [x] 6.5: Test: not-yet-supported files do NOT produce FileRecords or CAS blobs
  - [x] 6.6: Test: existing quality-focus languages still process correctly (regression)
  - [x] 6.7: Add Java grammar load + parse test in `tests/tree_sitter_grammars.rs`
  - [x] 6.8: Verify test count does not regress below 181 (Story 2.3 baseline)

## Dev Notes

### CRITICAL: Load project-context.md FIRST

MUST load `_bmad-output/project-context.md` BEFORE starting implementation. It contains 87 agent rules scoped to Epic 2 covering persistence architecture, type design, concurrency, error handling, testing, and anti-patterns. Failure to load this will cause architectural violations.

### Build Order (MANDATORY)

Follow the same build-then-test pattern established in Stories 2.2 and 2.3:

1. **Domain types** (Task 1) — extend `LanguageId` in `src/domain/index.rs`, fix all compiler errors from exhaustive match
2. **Java extractor** (Task 3) — `src/parsing/languages/java.rs` — pure function, test independently
3. **Parser wiring** (Task 2) — update `parse_source` and `extract_symbols` dispatcher, add `tree-sitter-java` dep
4. **Commit guard** (Task 5) — change tier check in `commit_file_result`
5. **Pipeline partition** (Task 4) — multi-tier handling in `process_discovered`
6. **Integration tests** (Task 6) — end-to-end verification

### Compiler-Driven Exhaustive Match Updates

Adding 10 new `LanguageId` variants will cause compiler errors in EVERY `match` on `LanguageId`. This is by design (project-context.md: "The compiler enforces exhaustive handling"). The agent MUST fix ALL compiler errors before running tests. Key match sites:

| File | Function | What to add |
|------|----------|-------------|
| `src/domain/index.rs` | `from_extension` | All new extension mappings |
| `src/domain/index.rs` | `extensions` | Extension slices per variant |
| `src/domain/index.rs` | `support_tier` | `Java => Broader`, rest => `Unsupported` |
| `src/parsing/mod.rs` | `parse_source` | `Java => tree_sitter_java::LANGUAGE.into()`, unsupported => `Err(...)` |
| `src/parsing/languages/mod.rs` | `extract_symbols` | `Java => java::extract_symbols(...)`, unsupported => `vec![]` |

### Critical Design Decision: Why ALL Broader Variants Now

AC3 requires inspectable not-yet-supported outcomes. If `LanguageId::from_extension` returns `None` for unrecognized languages, those files are silently skipped in discovery — no outcome is inspectable. Adding all broader variants means:
- Discovery finds `.cs`, `.rb`, `.php` etc. files
- Pipeline partitions them as not-yet-supported
- `PipelineResult` reports counts per unrecognized language
- The pattern to onboard the next language is: change `support_tier` to `Broader`, add tree-sitter grammar dep, add extractor module — no structural changes needed

### Extension Conflict: `.h` Files

C header files (`.h`) could also be C++ headers. Map `.h` to `LanguageId::C` (the simpler/more common case). This mirrors how most indexing tools handle the ambiguity. The developer should NOT attempt to heuristically detect C vs C++ from `.h` content — that's out of scope.

### Java Tree-sitter Grammar Notes

- Use `tree-sitter-java` version `0.23` to match existing grammar crate versions (tree-sitter core `0.24`, grammars `0.23`)
- Java grammar exposes `LANGUAGE` (not `LANGUAGE_JAVA`) — standard pattern, unlike TypeScript's `LANGUAGE_TYPESCRIPT`
- If `0.23` is unavailable, check crates.io for the latest version compatible with `tree-sitter = "0.24"`
- Java grammar node types: `class_declaration`, `interface_declaration`, `enum_declaration`, `method_declaration`, `constructor_declaration`, `field_declaration`

### Java Extractor Pattern

Follow the Go extractor pattern (`src/parsing/languages/go.rs`). Key structure:

```rust
pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    walk_node(node, source, 0, &mut sort_order, &mut symbols);
    symbols
}

fn walk_node(node: &Node, source: &str, depth: u32, sort_order: &mut u32, symbols: &mut Vec<SymbolRecord>) {
    // Map node.kind() to SymbolKind, extract name from child "name" node
    // Recurse into child nodes for nested symbols (inner classes, etc.)
}
```

### Pipeline Partition Design

The partition in `process_discovered` should be simple:

```rust
let (indexable, not_yet_supported): (Vec<_>, Vec<_>) = files
    .into_iter()
    .partition(|f| f.language.support_tier() != SupportTier::Unsupported);
```

Count not-yet-supported by language into `BTreeMap<LanguageId, u64>`, log one summary line, then process only `indexable` through the existing pipeline loop. The existing bounded-concurrency, circuit breaker, and CAS commit code is untouched.

### Previous Story Review Patterns (Apply Defensively)

Story 2.3 code review found 8 issues (2H, 3M, 3L). Patterns to prevent in this story:

| Pattern | Risk | Prevention |
|---------|------|------------|
| **Missing test for new path** | `Broader` tier flows through commit untested | Write explicit test: Java file -> commit_file_result -> success |
| **Wrong match exhaustiveness** | New variants missing from a match arm | `cargo test` will fail to compile — fix ALL errors before running tests |
| **Silent skip of unsupported** | Not-yet-supported files vanish without trace | Verify `PipelineResult.not_yet_supported` has correct counts in tests |
| **Backward compat** | New `PipelineResult` field breaks deserialization | `not_yet_supported` field: `#[serde(default)]` if PipelineResult is ever serialized |
| **Info-level per-file logging** | Logging each not-yet-supported file floods output | One summary `info!` line with total count, NOT per-file |

### What This Story Does NOT Implement

- Onboarding more than one broader language (future stories follow same pattern)
- C/C++ header file disambiguation beyond `.h` -> `C`
- Language-specific parsing quality improvements beyond basic symbol extraction
- Checkpointing / resume (Story 2.8)
- Search or retrieval of persisted data (Epic 3)

### Testing Standards

- Naming: `test_verb_condition` (e.g., `test_java_extraction_produces_class_and_method_symbols`)
- Assertions: plain `assert!`, `assert_eq!` — NO assertion crates
- `#[test]` by default; `#[tokio::test]` only for async
- Fakes: hand-written with `AtomicUsize` call counters — NO mock crates
- Temp directories for all file operations (CAS, registry)
- Current baseline: 181 tests — must not regress
- Logging: `debug!` for per-file outcomes, `info!` for run-level events — NEVER `info!` per-file

### Existing Code Locations

| Component | Path |
|-----------|------|
| LanguageId enum (extend) | `src/domain/index.rs` |
| SupportTier enum (already has Broader, Unsupported) | `src/domain/index.rs` |
| Domain re-exports | `src/domain/mod.rs` |
| Parser + parse_source (extend match) | `src/parsing/mod.rs` |
| Language extractors dispatcher (extend) | `src/parsing/languages/mod.rs` |
| Go extractor (pattern reference) | `src/parsing/languages/go.rs` |
| File discovery (auto-extends via from_extension) | `src/indexing/discovery.rs` |
| Commit guard (change tier check) | `src/indexing/commit.rs` |
| Pipeline (partition logic) | `src/indexing/pipeline.rs` |
| Grammar integration tests (add Java) | `tests/tree_sitter_grammars.rs` |
| Integration tests (extend) | `tests/indexing_integration.rs` |
| Cargo dependencies (add tree-sitter-java) | `Cargo.toml` |

### Tree-sitter / Parsing Notes (Inherited)

- `tree-sitter` 0.24 — `Node::is_null()` removed, use `node.kind().is_empty()`
- `tree-sitter-typescript` exposes `LANGUAGE_TYPESCRIPT` not `LANGUAGE`
- `tree-sitter-java` should expose `LANGUAGE` (standard pattern) — verify at implementation time
- `SymbolKind` has `Copy` derive
- `ignore` crate requires `.git/` directory to respect `.gitignore` in tests

### Project Structure Notes

Files to create:
- `src/parsing/languages/java.rs` — Java symbol extraction module

Files to modify:
- `Cargo.toml` — add `tree-sitter-java` dependency
- `src/domain/index.rs` — add 10 `LanguageId` variants, update all `impl` methods
- `src/domain/mod.rs` — no changes needed (re-exports `LanguageId` which auto-includes new variants)
- `src/parsing/mod.rs` — update `parse_source` match
- `src/parsing/languages/mod.rs` — register `java` module, update `extract_symbols` dispatcher
- `src/indexing/commit.rs` — change support tier guard
- `src/indexing/pipeline.rs` — add `not_yet_supported` field to `PipelineResult`, partition logic in `process_discovered`
- `tests/tree_sitter_grammars.rs` — add Java grammar test
- `tests/indexing_integration.rs` — add broader language integration tests

No conflicts with unified project structure detected. All new code follows established `mod.rs` module style and existing directory layout.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Epic-2-Story-2.4]
- [Source: _bmad-output/planning-artifacts/prd.md#Language-Matrix]
- [Source: _bmad-output/planning-artifacts/architecture.md#Indexing-Pipeline-Architecture]
- [Source: _bmad-output/project-context.md#Epic-2-Type-Design]
- [Source: _bmad-output/project-context.md#Tree-sitter-Rules]
- [Source: _bmad-output/implementation-artifacts/2-3-persist-file-level-indexing-outputs-and-symbol-file-metadata-for-the-initial-quality-focus-language-set.md]
- [Source: _bmad-output/implementation-artifacts/2-2-execute-indexing-for-the-initial-quality-focus-language-set.md]

## Change Log

- 2026-03-07: Implemented Story 2.4 — broader language onboarding pattern with Java as first onboarded slice, 10 new LanguageId variants, pipeline partition for multi-tier support, 30 new tests (211 total, up from 181)

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6

### Debug Log References

- tree-sitter-java v0.23.5 resolved and compiled successfully, exposes `LANGUAGE` as expected
- `LanguageId` required `PartialOrd + Ord` derives for `BTreeMap` usage in `not_yet_supported`
- Integration test `test_out_of_scope_files_not_persisted_as_file_records` updated — Java files now expected to produce FileRecords (Broader tier)

### Completion Notes List

- Task 1: Added 10 LanguageId variants (Java, C, Cpp, CSharp, Ruby, Php, Swift, Dart, Perl, Elixir) with all extension mappings, tier assignments, and 16 new unit tests
- Task 2: Wired Java into parse_source (tree-sitter-java) and extract_symbols dispatcher; catch-all arms for unsupported languages
- Task 3: Created Java symbol extractor (src/parsing/languages/java.rs) following Go extractor pattern — handles classes, interfaces, enums, methods, constructors, fields, nested classes, with 7 unit tests
- Task 4: Pipeline now partitions discovered files into indexable (QualityFocus/Broader) and not-yet-supported (Unsupported), counts by language, logs one summary line, processes only indexable files
- Task 5: Commit guard changed from `!= QualityFocus` to `== Unsupported` — Broader tier files now commit successfully
- Task 6: 5 new integration tests + 1 grammar test verifying Java end-to-end, syntax error handling, mixed-tier repos, unsupported file exclusion, and QualityFocus regression

### Implementation Plan

Build order followed (per Dev Notes): Domain types (T1) -> Java extractor (T3) -> Parser wiring (T2) -> Commit guard (T5) -> Pipeline partition (T4) -> Integration tests (T6). Compiler-driven exhaustive match ensured all sites updated before tests ran.

### File List

**New files:**
- `src/parsing/languages/java.rs` — Java symbol extractor module

**Modified files:**
- `Cargo.toml` — added `tree-sitter-java = "0.23"` dependency
- `src/domain/index.rs` — 10 new LanguageId variants, updated from_extension/extensions/support_tier, added PartialOrd+Ord derives, 16 new unit tests
- `src/parsing/mod.rs` — added Java arm and unsupported catch-all in parse_source
- `src/parsing/languages/mod.rs` — registered java module, added Java arm and unsupported catch-all in extract_symbols
- `src/indexing/commit.rs` — changed tier guard from != QualityFocus to == Unsupported, updated error message, 2 new tests
- `src/indexing/pipeline.rs` — added BTreeMap/LanguageId/SupportTier imports, added not_yet_supported field to PipelineResult, partition logic in process_discovered, updated finish summary, 1 new test
- `tests/tree_sitter_grammars.rs` — added Java grammar load+parse test
- `tests/indexing_integration.rs` — updated existing out-of-scope test, added 5 new Story 2.4 integration tests

# Discovery: Phase 0 Benchmark Scenarios

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [10-T-phase0-benchmark-scenarios.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/10-T-phase0-benchmark-scenarios.md)

Goal:

- define the smallest benchmark set that covers the Phase 0 hot paths before Phase 1 through Phase 5 change the query surface
- keep the set small enough for repeatable local runs while still exposing the main regression risks called out in the plan

Selection rules:

- benchmark the public tool path first, not just the inner query function
- keep one happy-path and one ambiguity or noise-path case for each lookup family where ranking matters
- use tempdir-backed synthetic repos first; convert only the stable cases into checked-in fixtures in the next tasks

## Minimum Scenario Set

### Path lookup

`PATH-01` unique exact path lookup

- why it matters: this is the baseline for the common "I know roughly which file I want" workflow and the future `resolve_path` fast path
- fixture shape: nested repo tree with unique filenames under `src/`, `tests/`, and `docs/`
- likely measure home: Phase 0 proxy in `tests/live_index_integration.rs`; post-Phase-2 public benchmark through `src/protocol/tools.rs` once `search_files` or `resolve_path` exists

`PATH-02` repeated basename disambiguation

- why it matters: the plan explicitly calls out ambiguous basenames and path-aware ranking as a core regression risk
- fixture shape: several `mod.rs`, `index.ts`, `main.rs`, and `README.md` files in different directories
- likely measure home: same harness as `PATH-01`, with a ranking assertion added when path tools land; candidate ranking micro-bench in `src/live_index/query.rs` if needed

### Text search

`TEXT-01` selective needle in a small code repo

- why it matters: establishes the low-noise baseline for `search_text` latency and output grouping
- fixture shape: 10 to 20 source files with one uncommon token appearing in 1 to 2 files
- likely measure home: `tests/live_index_integration.rs` for end-to-end behavior, with tool-level timing through `src/protocol/tools.rs`

`TEXT-02` noisy common token across mixed code and text files

- why it matters: this is the anchor for future path, glob, language, generated-file, and test-file filters
- fixture shape: mixed Rust, TypeScript, Markdown, and generated-like files containing tokens such as `new`, `config`, or `TODO`
- likely measure home: `tests/live_index_integration.rs` plus formatter or query isolation in `src/protocol/format.rs` if output rendering dominates

### Symbol lookup

`SYM-01` exact unique symbol lookup

- why it matters: captures the expected fast path for `search_symbols` and later exact-symbol chaining
- fixture shape: medium repo with a clearly unique function or type name
- likely measure home: public tool benchmark through `src/protocol/tools.rs`, backed by current result-shaping coverage in `src/protocol/format.rs`

`SYM-02` crowded prefix or substring symbol lookup

- why it matters: exposes ranking cost and output bloat when many nearby names compete, which is a known precision risk before scoped filters land
- fixture shape: many symbols sharing stems such as `parse`, `load`, `config`, or `new`
- likely measure home: `src/protocol/tools.rs` for public latency and `src/protocol/format.rs` for tier-order output stability

### Reference lookup

`REF-01` exact cross-file reference lookup

- why it matters: this is the main functional baseline for `find_references` and `get_context_bundle`
- fixture shape: 30 to 50 source files with one target symbol referenced across multiple callers and imports
- likely measure home: `tests/xref_integration.rs`, using the existing `get_context_bundle` performance style as the starting point

`REF-02` noisy common-name reference lookup

- why it matters: the plan explicitly calls out common-name floods and exact-symbol disambiguation as a major gap
- fixture shape: builtin-like names, generic names, and alias-heavy imports such as `new`, `string`, `T`, and `HashMap`
- likely measure home: `tests/xref_integration.rs` and `src/live_index/query.rs` for inner filtering behavior if regressions appear

### File reading

`READ-01` full-file read by exact path

- why it matters: preserves the simplest `get_file_content` behavior and detects accidental formatting or truncation regressions
- fixture shape: short source file and short text file with stable line endings
- likely measure home: `src/protocol/tools.rs` and `tests/live_index_integration.rs`

`READ-02` focused line-range read

- why it matters: line-bounded reads are the current precision control on the read path and the foundation for later `around_line` and `around_match` work
- fixture shape: 50 to 200 line file where the requested window sits in the middle
- likely measure home: `tests/live_index_integration.rs`, with formatter-level assertions in `src/protocol/format.rs`

## Current Repo Anchors

- `src/protocol/tools.rs` already exposes the public tool handlers for `search_symbols`, `search_text`, `get_file_content`, `find_references`, and `get_context_bundle`
- `src/protocol/format.rs` already carries output-shaping tests for `search_symbols`, `search_text`, `find_references`, and file-content formatting
- `src/live_index/query.rs` already contains the core `find_references_for_name` behavior and is the right isolation point if ranking or filtering regressions need deeper timing
- `tests/live_index_integration.rs` already has realistic tempdir-backed coverage for `search_text` and `get_file_content`
- `tests/xref_integration.rs` already has realistic cross-file coverage for `find_references`, `find_dependents`, and a `get_context_bundle` under-100ms check

## Phase 0 Recommendation

- treat the ten scenarios above as the minimum useful set; do not expand into a full matrix yet
- keep the first harness centered on tool-level timings plus snapshot-ready outputs
- leave path lookup as a proxy benchmark in Phase 0, then switch the same scenarios to direct `search_files` or `resolve_path` calls when those tools land

Carry-forward for task 11:

- snapshot outputs for `TEXT-01`, `TEXT-02`, `SYM-01`, `SYM-02`, `REF-01`, `REF-02`, `READ-01`, and `READ-02`
- include a path-lookup proxy snapshot only if the output is stable enough to survive the later `repo_outline` path-label upgrade

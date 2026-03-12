# Discovery: Phase 0 Baseline Output Snapshot Plan

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [10-D-phase0-benchmark-scenarios.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/10-D-phase0-benchmark-scenarios.md)
- [11-T-phase0-baseline-output-snapshot-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/11-T-phase0-baseline-output-snapshot-plan.md)

Goal:

- capture the current text outputs that later query-surface work may accidentally reorder, truncate, or over-expand
- keep the baseline focused on the current public tool behavior, not future output shapes from later phases

Current constraint:

- the repo does not currently carry a snapshot-specific test dependency such as `insta`
- Phase 0 should therefore use checked-in plain-text golden files rather than add a new framework during baseline capture

## Snapshot Targets

Must snapshot now:

- `search_text`
  - `TEXT-01` selective needle
  - `TEXT-02` noisy common token
- `search_symbols`
  - `SYM-01` exact unique symbol
  - `SYM-02` crowded prefix or substring symbol set
- `find_references`
  - `REF-01` exact cross-file reference case
  - `REF-02` noisy common-name reference case
- `get_file_content`
  - `READ-01` full-file read
  - `READ-02` focused line-range read
- `get_context_bundle`
  - one stable `REF-01` companion fixture preserving body, footer, and section headers
- `get_file_context`
  - one stable cross-file dependency case preserving header plus `Key references`

Optional or intentionally temporary in Phase 0:

- `get_repo_outline`
  - only as the current path-lookup proxy
  - keep this to a narrow baseline if captured at all because Phase 2 explicitly plans path-rich label changes

Do not spend Phase 0 snapshot budget on:

- `health`, `index_folder`, `what_changed`, `analyze_file_impact`
- `get_repo_map` and `get_file_tree` unless one is used only as setup for the path proxy case
- retrieval envelope JSON already covered by `tests/retrieval_conformance.rs`

## Normalization Rules

Normalize only environment noise:

- replace tempdir or machine-specific roots with `<ROOT>`
- normalize path separators to `/`
- write snapshot files with `\n` line endings

Do not normalize contract-bearing content:

- counts such as `2 matches in 1 files`
- line numbers
- section headers such as `Exact matches`, `Prefix matches`, `Key references`, `Callers`, `Callees`, and `Type usages`
- ordering of files, symbols, or references once emitted by the formatter
- exact text windows returned by current `get_file_content`

Important guardrail:

- snapshot the current output, not the planned future output
- for example, Phase 0 `get_file_content` fixtures should preserve today's full-file and line-range rendering and must not pre-bake future line-number or `around_line` formatting

## Deterministic Ordering Concerns

These should be asserted exactly, not hidden by harness-side sorting:

- `search_symbols` tier order and within-tier ranking
  - current contract already distinguishes exact, prefix, and substring matches
- `search_text` file grouping and in-file line order
- `find_references` grouped-by-file rendering
- `get_context_bundle` section ordering and caller grouping
- `get_file_context` header and `Key references` section ordering

Special case:

- `get_repo_outline` is currently deterministic, but it is also a known transition surface because later work will add richer path labels
- if Phase 0 captures it, treat it as a disposable compatibility baseline rather than a long-lived exact contract

## Recommended Layout

Snapshot files:

- `tests/snapshots/phase0/search_text/text-01-selective.txt`
- `tests/snapshots/phase0/search_text/text-02-noisy-common.txt`
- `tests/snapshots/phase0/search_symbols/sym-01-exact.txt`
- `tests/snapshots/phase0/search_symbols/sym-02-crowded.txt`
- `tests/snapshots/phase0/find_references/ref-01-exact.txt`
- `tests/snapshots/phase0/find_references/ref-02-common-name.txt`
- `tests/snapshots/phase0/get_file_content/read-01-full-file.txt`
- `tests/snapshots/phase0/get_file_content/read-02-line-range.txt`
- `tests/snapshots/phase0/get_context_bundle/ref-01-bundle.txt`
- `tests/snapshots/phase0/get_file_context/context-01-key-references.txt`
- `tests/snapshots/phase0/get_repo_outline/path-proxy-01.txt` only if the proxy output is stable enough to keep

Harness location:

- add a dedicated integration test such as `tests/phase0_output_snapshots.rs`
- load checked-in snapshot text there rather than adding fixture IO to `src/protocol/format.rs`
- drive snapshot generation through public handlers in `src/protocol/tools.rs` where possible, falling back to formatter-level setup only when the tool wrapper adds no extra behavior

Input fixture location for the next task:

- keep repository inputs separate under `tests/fixtures/phase0/`
- task 12 should decide which fixtures stay synthetic tempdir builders and which should become checked-in repos

Carry-forward for task 12:

- fixture repos must support the ten benchmark scenarios from task 10 plus the `get_context_bundle` and `get_file_context` compatibility snapshots
- unresolved question: whether the path-proxy baseline should snapshot `get_repo_outline` at all, or stay as assertion-based coverage until `search_files` or `resolve_path` exists

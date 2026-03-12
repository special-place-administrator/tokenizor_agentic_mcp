# Discovery: Phase 0 Regression Fixture Plan

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [10-D-phase0-benchmark-scenarios.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/10-D-phase0-benchmark-scenarios.md)
- [11-D-phase0-baseline-output-snapshot-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/11-D-phase0-baseline-output-snapshot-plan.md)
- [12-T-phase0-regression-fixture-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/12-T-phase0-regression-fixture-plan.md)

Goal:

- cover the four regression-risk categories named in Phase 0 without creating a sprawling fixture catalog
- choose fixtures that can serve both the benchmark scenarios from task 10 and the snapshot targets from task 11

Fixture design rules:

- prefer a few reusable fixture repos over one fixture per tool
- keep structural layout fixtures checked in on disk when path shape matters
- keep high-volume noise fixtures builder-backed when checked-in files would become churny or unreadable
- preserve exact bytes for any file that will later feed file-content or text-lane comparisons

## Minimum Fixture Set

`FIX-01` repeated-basenames repo

- purpose: exercise ambiguous filenames, module-style entry files, and future path-ranking behavior
- include:
  - repeated names such as `main.rs`, `mod.rs`, `index.ts`, and `README.md`
  - nested directories under `src/`, `tests/`, `docs/`, and one web-style subtree
  - at least one `mod.rs`, one `__init__.py`, and one `index.ts` so future module-path logic is covered too
- likely storage location: checked-in repo under `tests/fixtures/phase0/repeated-basenames/`
- primary consumers: path proxy checks, future `search_files` or `resolve_path`, `repo_outline` compatibility checks, and targeted read-path tests

`FIX-02` common-symbol-flood repo

- purpose: expose noisy symbol and reference queries before scoped filters or exact-symbol disambiguation land
- include:
  - repeated names such as `new`, `load`, `parse`, and `config`
  - builtin-like or generic names such as `string`, `T`, and `K`
  - one alias case such as `HashMap as Map`
  - one qualified-name case such as `Vec::new`
- likely storage location: builder-backed fixture helper for scale, with a tiny checked-in seed under `tests/fixtures/phase0/common-symbol-flood-seed/`
- primary consumers: `search_symbols`, `find_references`, `get_context_bundle`, and later exact-symbol disambiguation work

`FIX-03` generated-noise overlay

- purpose: create realistic result bloat from generated-looking content without bloating the repo with huge committed artifacts
- include:
  - paths like `generated/`, `dist/`, `vendor/`, or `bindings/`
  - repeated boilerplate identifiers and comments
  - at least one file large enough to stress text search and read-path truncation decisions
- likely storage location: small checked-in seed under `tests/fixtures/phase0/generated-noise-seed/` plus builder expansion in the test harness
- primary consumers: `search_text`, future generated-file suppression defaults, and read-path boundary tests

`FIX-04` mixed-code-text repo

- purpose: preserve the cross-lane reality that later phases must handle even though the current parser is still code-first
- include:
  - Rust or TypeScript source files
  - Markdown docs, changelog or README files, JSON or YAML config, and one plain-text note file
  - both selective needles and noisy shared tokens such as `TODO` or `config`
- likely storage location: checked-in repo under `tests/fixtures/phase0/mixed-code-text/`
- primary consumers: `search_text`, `get_file_content`, path proxy checks, and later non-code text-lane work

## Reuse Points In The Current Repo

- `src/live_index/query.rs` already has tests for builtin-name filtering, single-letter generic filtering, alias resolution, qualified-name matching, and `mod.rs` or `index.*` dependent resolution
- `tests/xref_integration.rs` already proves alias-heavy and common-name reference cases, so the common-symbol fixture should reuse those name patterns instead of inventing new ones
- `tests/watcher_integration.rs` already shows that non-source files such as `README.md` are ignored today, which makes the mixed-code-text fixture useful both for current behavior and for future text-lane changes
- `src/parsing/mod.rs` is still code-language focused, so text-heavy fixtures should be stored exactly on disk now even if only a subset of files are indexed today

## Recommended Storage Layout

Checked-in fixture repos:

- `tests/fixtures/phase0/repeated-basenames/`
- `tests/fixtures/phase0/common-symbol-flood-seed/`
- `tests/fixtures/phase0/generated-noise-seed/`
- `tests/fixtures/phase0/mixed-code-text/`

Builder-backed expansion:

- add a small helper later in a dedicated integration harness or support module to fan out:
  - extra common-symbol files for scale
  - extra generated-noise files for volume

Reason for the hybrid layout:

- path-sensitive and byte-sensitive structures are easier to reason about when checked in
- high-volume noise is cheaper to generate than to review in git

## Incremental Implementation Order

1. build `FIX-01` repeated-basenames because it supports the path proxy immediately
2. build `FIX-04` mixed-code-text because it feeds text and read-path snapshots next
3. add `FIX-02` common-symbol-flood builder once the symbol or reference harness is ready
4. add `FIX-03` generated-noise overlay last, because it is mostly about suppression and scaling pressure

Carry-forward for task 13:

- compatibility thresholds should assume these four fixtures are enough for Phase 0
- if a threshold cannot be expressed against one of these fixtures, that is a sign the fixture set needs a narrow addition rather than a broad new family

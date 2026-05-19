# SymForge Actionable Implementation Backlog

Last consolidated: 2026-05-20

This is the only retained docs artifact. It lists implementation or
test-hardening work still worth doing in SymForge. Historical plans, reviews,
ADRs, research notes, release evidence, and doc-only ideas were pruned after
their live implementation work was either captured here or discarded.

## Task-generation instructions for GPT-5.5 Pro

Convert the items below into ordered implementation tasks for SymForge. Each
generated task should be code-oriented and should include:

- objective;
- non-goals;
- allowed files or code areas;
- contracts and invariants;
- concrete acceptance criteria;
- evidence required before closing;
- stop conditions;
- verification commands.

Do not create tasks for historical cleanup, research refresh, documentation
cookbooks, or already-implemented work. Split broad items into smaller tasks
only when the split creates independently verifiable code or test work.

## 1. Windows libgit2 lockfile flake mitigation

Problem: tests that create many commits can intermittently fail on Windows when
libgit2 cannot rename `.git/refs/heads/*` lockfiles.

Implement:

- Add retry/backoff around affected git-test helpers, or replace the helper's
  libgit2 commit path with process `git commit` where appropriate.
- Keep the fix isolated to tests/helpers unless production code is proven to hit
  the same Windows lockfile race.

Accept:

- Affected frecency/persist tests pass repeatedly on Windows under parallel and
  serial cargo test runs.
- No `#[ignore]` workaround is needed.

## 2. Untracked-file search diagnostic

Problem: `what_changed` can see untracked files, but `search_files` and
`search_text` do not appear to emit an actionable empty-result diagnostic that
points users to `analyze_file_impact(path, new_file=true)`.

Implement:

- In `search_files` and `search_text`, when a query returns zero hits and a
  matching untracked file exists, append a diagnostic such as:
  `Note: 1 untracked file may match this query. Run analyze_file_impact("<path>", new_file=true) to index it.`
- Do not auto-index untracked files by default.

Accept:

- A regression test proves the diagnostic appears for a matching untracked file.
- Existing tracked-file search behavior is unchanged.

## 3. Sidecar PID/alive state in health output

Problem: health still reports hook adoption text, but does not surface an
explicit `Sidecar:` line with PID and alive/dead state.

Implement:

- Expose sidecar PID and liveness from the existing sidecar state/port-file path.
- Render this in both `health` and `health_compact`.
- Preserve existing hook-adoption counters.

Accept:

- Tests assert sidecar status appears in full and compact health output.
- Tests cover a down/dead sidecar state.

## 4. NoisePolicy classification for Obsidian internals

Problem: `.obsidian/` and `wiki/.obsidian/` can still pollute search or
coupling signals.

Implement:

- Extend the path-noise classifier so `.obsidian/` and `wiki/.obsidian/`
  classify as personal tooling.
- Do not exclude normal markdown/wiki content outside `.obsidian`.

Accept:

- Tests cover `.obsidian/`, `wiki/.obsidian/`, and
  `.obsidian/plugins/dataview/styles.css`.

## 5. External-evaluator regression coverage

Problem: historical external evaluations found bugs that ordinary tests did not
make obvious. Most specific fixes landed, but the test-surface gaps still need
hardening.

Implement:

- Audit fixed evaluator bugs and map each to the test category that should have
  caught it before release.
- Add the top regression tests directly, or turn each into a concrete backlog
  item in this file.

Accept:

- At least three new regression tests land, or each top gap is represented here
  as a concrete implementation item with file targets and verification.

## 6. Current partial-parse hygiene

Problem: current health no longer shows SymForge Rust source partials; the
remaining partial files are vendored SCSS parser C/header files.

Implement:

- Decide whether vendor partials should be fixed, suppressed as vendor noise, or
  surfaced as expected vendor parse limitations.
- Keep the old Rust `&raw` parser issue closed unless it reappears.

Accept:

- Health reports zero unexpected partials for the repo, or clearly marks vendor
  partials as expected/noise.

## 7. `search_text(group_by="usage")` doc/comment filter

Problem: `group_by="usage"` filters imports and ordinary comments, but doc
comments can still remain usage-visible. The intended product behavior needs to
be pinned in code.

Implement:

- If doc/markdown matches should be suppressed, update the usage filter and add
  regression tests.
- If current behavior is intentional, add tests that pin why doc comments remain
  usage-visible.

Accept:

- `group_by="usage"` behavior around doc comments and markdown is explicitly
  tested.

## 8. `replace_symbol_body` same-line inline doc preservation

Problem: when a doc comment and symbol signature live on the same source line,
for example `/** @deprecated */ export function legacy() { ... }`,
`replace_symbol_body` can replace from the start of the line and swallow the
inline doc if the replacement body has no docs.

Implement:

- Detect inline doc/comment text between `raw_line_start` and
  `sym.byte_range.0` before replacing.
- Preserve the inline doc prefix or adjust the splice start to begin after the
  inline doc marker when the new body does not provide its own docs.
- Add focused fixtures for TypeScript/JSDoc and one Rust-style inline comment
  case if the parser can represent it.

Accept:

- `replace_symbol_body` preserves a same-line inline doc when replacing a symbol
  with a docless `new_body`.
- Existing attached-doc and orphan-doc tests still pass.

## 9. Machine-readable result status semantics

Problem: many tool responses are optimized for readable text. Agents still need
stable machine-level outcome semantics for states such as found, not found,
ambiguous selector, invalid request, empty result, and internal failure.

Implement:

- Add a public result-status contract for MCP tool responses where the protocol
  can carry it without breaking existing text output.
- Keep human-readable messages, but expose stable machine truth separately.
- Prioritize `get_symbol`, `get_file_content`, `search_*`, `find_references`,
  `replace_symbol_body`, `batch_edit`, and `batch_insert`.

Accept:

- Contract tests cover found, not found, ambiguous, invalid request, and empty
  or no-match states.
- Existing human text remains understandable.

## 10. Runtime state identity and reset clarity

Problem: shared daemon/index state is useful, but hidden carry-over state makes
benchmarking, debugging, and reproductions harder.

Implement:

- Surface active project root, index/session identity, and whether the index was
  freshly built or reused in `health`, `health_compact`, or a dedicated status
  surface.
- Add or document a deterministic fresh-index/reset workflow for evaluations.
- Make context/session carry-over visible enough that callers do not infer a
  clean session incorrectly.

Accept:

- A fresh process, reused daemon session, and explicit `index_folder` reset are
  distinguishable in tool output.
- Evaluation harnesses can assert active project/index identity before running.

## 11. Replayable public-contract conformance suite

Problem: historical evaluations exposed contract-level issues that ordinary
implementation tests did not catch.

Implement:

- Add a versioned conformance corpus for public MCP contracts: canonical JSON
  requests, expected response class/status, expected recovery hint for invalid
  requests, and dry-run behavior for mutating tools.
- Include negative cases for malformed payloads and unsupported forms.
- Record schema/behavior deltas in release notes when public contracts change.

Accept:

- The conformance suite can replay core read/search/edit/dry-run cases against a
  built binary.
- At least one invalid-request case asserts a specific recovery message instead
  of generic deserialization fallout.

## 12. Guidance ranking and noise filtering

Problem: guidance tools are valuable, but they should avoid low-signal symbols,
doc-only code patterns, and unexplained suggestions.

Implement:

- Audit `investigation_suggest`, `ask`, and `explore` ranking for low-signal
  symbols such as builtins/common names and doc/comment-only pattern hits.
- Prefer project-owned symbols, changed files, loaded-context proximity,
  caller/reference depth, and explicit reason text.
- Keep outputs concise.

Accept:

- Focused tests prove trivial names are filtered unless strongly contextualized.
- At least one guidance response includes a concise reason for why a suggestion
  was made.

## 13. Co-change rerank calibration closure

Problem: query-level anchor-confidence gating remains provisional. Current code
has a conservative basename-tier floor and hardcoded chore-anchor defaults; the
remaining work is to close the empirical/configuration loop.

Implement:

- Add a query-level calibration or regression corpus for
  `search_files(rank_by="path+cochange", anchor_path=...)` that proves weak
  anchors do not degrade baseline path ordering.
- Promote, adjust, or remove the basename-tier anchor-confidence threshold based
  on measured outcomes.
- Decide whether the chore-anchor denylist should remain hardcoded or become
  workspace-configurable.

Accept:

- Tests cover the chosen weak-anchor behavior and chore-anchor behavior.
- The chosen constants/config are documented in code comments or test names.

## 14. `trace_symbol` compatibility alias cleanup

Problem: `trace_symbol` was kept as a compatibility alias for one release cycle.
It is still present in client allow-list guidance and daemon compatibility after
many later releases.

Implement:

- Remove `trace_symbol` from generated client allow lists and default tool-name
  guidance.
- Decide whether the daemon compatibility route should remain for one final
  release with an explicit deprecation warning or be removed in the same patch.
- Ensure `find_references` and `get_symbol_context` are the only documented
  paths.

Accept:

- Source search for `trace_symbol` returns only deliberate historical references
  or none at all, depending on the chosen compatibility policy.
- Client init tests still pass and do not grant the retired tool by default.

## 15. Rust raw-reference grammar upgrade

Problem: `Cargo.toml` still pins `tree-sitter-rust = "=0.24.2"`. Rust 2024
`&raw const` / `&raw mut` should parse without partial-parse fallout.

Implement:

- Bump `tree-sitter-rust` after checking compatibility with the pinned
  `tree-sitter` version.
- Add or retain a fixture proving raw-reference expressions parse cleanly.
- Re-check current partial Rust parse examples before and after the bump.

Accept:

- Raw-reference syntax no longer creates unexpected Rust partial parses.
- Full Rust parsing tests and the repo-wide test suite pass.

## 16. `validate_file_syntax` deepest-error diagnostics

Problem: current tree-sitter diagnostics still use a first-error walk. The
reported syntax location should prefer the deepest useful ERROR or MISSING node.

Implement:

- Replace first-error selection with deepest-error selection that preserves
  stable line/column/byte-span reporting.
- Add malformed-source fixtures where the outer parse error is less useful than
  the nested error.

Accept:

- `validate_file_syntax` reports the deepest actionable syntax error for the
  targeted fixtures.
- Existing config-language diagnostics remain unchanged unless the new location
  is strictly more specific.

## 17. Unified truncation phrasing

Problem: protocol and sidecar surfaces still use multiple truncation phrases,
which makes automated parsing and user guidance noisier than necessary.

Implement:

- Choose one canonical truncation footer/envelope phrase.
- Apply it consistently to protocol output and sidecar budgeted output.
- Add tests for at least one protocol path and one sidecar path.

Accept:

- No active output surface emits a second, contradictory truncation phrase.
- Tests pin the canonical phrasing.

## 18. Remaining language inline extractor tests

Problem: the inline extractor-test framework and first Rust/Python examples are
implemented, but the remaining language extractors do not each have a co-located
fixture that asserts expected symbol extraction.

Implement:

- Add focused `inline_test!` cases for the remaining language extractors in
  small batches.
- Keep each fixture minimal: one representative snippet, expected symbol names,
  expected kinds, and no broad parser refactor.

Accept:

- Every supported language extractor has at least one inline test.
- `cargo test --lib parsing -- --test-threads=1` passes.

## 19. Local SQLite analytics implementation

Problem: local persistent tool-call analytics is accepted as useful, but
production code still has only tracing and session-local counters.

Implement:

- Implement a versioned local SQLite analytics store.
- Preserve disabled-no-footprint behavior: disabled analytics must not create a
  database, especially for discovery-only tools.
- Record only bounded local metadata: tool name, surface, configured scope,
  response bytes, estimated tokens, duration, success, outcome class, and
  capability state where already computed.
- Keep writes off the hot path through a bounded queue and background writer.
- Add CLI status/summary/export/reset surfaces; do not add an MCP analytics tool
  without a separate decision.

Accept:

- Analytics storage has migration, retention, redaction, disabled-mode, and
  queue-failure tests.
- Enabled analytics records bounded metadata without synchronous SQLite writes
  in handler hot paths.
- Disabled mode creates no analytics database and reports explicit disabled
  status.

## 20. Non-code repository intelligence expansion

Problem: SymForge is strong for source symbols, but many real debugging tasks
depend on operational files that still behave too much like plain text: SQL
migrations, XML/MSBuild, YAML/CI, shell scripts, fixtures, logs, and large docs.

Implement:

- Add one file family at a time; do not attempt a broad parser rewrite.
- Start with SQL/migration facts or CI/YAML facts, whichever has the clearest
  user workflow and test corpus.
- Reuse existing config-extractor and metadata-only degradation patterns before
  inventing new public tools.
- Keep search, outline, explain, and edit behavior consistent with source-code
  files wherever possible.

Accept:

- The chosen file family becomes searchable, resolvable, and explainable through
  existing SymForge surfaces without raw shell fallback for normal inspection.
- Tests include normal, malformed, large, and empty/edge-case files.
- Any new edit behavior has dry-run or recovery evidence comparable to source
  edits.

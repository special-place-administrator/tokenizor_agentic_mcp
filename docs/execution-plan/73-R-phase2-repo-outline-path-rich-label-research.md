# Research: Phase 2 Repo Outline Path-Rich Label

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [02-P-workstreams-and-tool-surface.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)
- [11-D-phase0-baseline-output-snapshot-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/11-D-phase0-baseline-output-snapshot-plan.md)
- [51-T-phase1-repo-outline-published-query-snapshot-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/51-T-phase1-repo-outline-published-query-snapshot-shell.md)
- [69-R-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md)
- [71-R-phase2-search-files-output-and-ranking-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/71-R-phase2-search-files-output-and-ranking-research.md)
- [73-T-phase2-repo-outline-path-rich-label-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/73-T-phase2-repo-outline-path-rich-label-research.md)

Goal:

- choose the first safe path-rich label shape for `repo_outline` without turning a compact whole-index view into a full-path dump

## Current Evidence

- the Phase 2 plan explicitly calls for upgrading `repo_outline` away from basename-only output
- the current formatter still renders each entry by stripping to the final filename from `relative_path`
- the current repo already has repeated code-lane basenames:
  - `mod.rs`: 9 occurrences under `src/`
  - `lib.rs`: 2 occurrences under `src/`
- `search_files` and `resolve_path` now cover active path discovery, so `repo_outline` can stay an overview surface instead of becoming another free-form search result list

## Constraint: Preserve The Published Snapshot Seam

`get_repo_outline` and `get_file_tree` already consume the published immutable `RepoOutlineView` snapshot.

That snapshot already contains the only raw path data needed:

- `relative_path`
- `language`
- `symbol_count`

So the first label upgrade should avoid changing:

- `LiveIndex::capture_repo_outline_view()`
- `SharedIndexHandle::published_repo_outline()`
- `file_tree` snapshot consumption

If the label can be derived from `relative_path` at render time, the change stays formatter-local and keeps the publication boundary stable.

## Candidate Label Shapes

### Rejected: Full Relative Path For Every Entry

Example:

- `src/live_index/mod.rs`
- `src/protocol/mod.rs`

Why reject for the first shell:

- it is unambiguous, but too verbose for a whole-index summary that lists every file
- it duplicates the role already served by `search_files`
- it would create a larger visible output contract change than necessary

### Rejected: Basename Plus Immediate Parent

Example:

- `live_index/mod.rs`
- `protocol/mod.rs`

Why reject as the contract:

- it is only conditionally safe
- collisions can still survive when the same basename appears under the same parent name in different branches
- once that edge case appears, the formatter needs another extension rule anyway

This is acceptable as an intermediate computation step, but not as the final rule.

### Preferred: Shortest Unique Path Suffix

Rule:

- start with basename only
- for collided basenames, prepend one path component at a time from the right
- stop once the label is unique within the current repo-outline set
- fall back to full relative path if uniqueness still requires the entire path

Examples:

- unique file: `main.rs`
- repeated file: `protocol/mod.rs`
- deeper repeated file if needed: `parsing/languages/mod.rs`

Why this is the smallest safe choice:

- unique files stay compact
- repeated basenames become explicit and deterministic
- token cost grows only where ambiguity exists
- the algorithm can be computed entirely from the current `RepoOutlineView.files`

## Compatibility Posture With `file_tree`

Recommendation:

- keep `file_tree` unchanged in the first slice

Why:

- `file_tree` already uses hierarchical directory rendering, so it does not share the basename-only ambiguity problem
- both surfaces can continue consuming the same published `RepoOutlineView`
- changing both together would create unnecessary output churn and broaden the regression surface

## Recommended First Implementation Slice

- add a formatter-local helper that derives a display label from the full `RepoOutlineView.files` set
- update `repo_outline_view()` to render the derived label instead of raw basename-only output
- keep the existing published snapshot schema unchanged
- add focused tests covering:
  - unique basename stays basename-only
  - repeated basename expands to shortest unique suffix
  - deeper collisions expand beyond one parent when needed
  - `file_tree` output remains unchanged

Expected touch points:

- `src/protocol/format.rs`

Possible test-only collateral:

- `src/protocol/tools.rs` if a direct tool-output assertion is added

## Carry Forward

- keep `repo_outline` code-lane only until a real text registry exists
- do not widen or reshape the published repo-outline snapshot for this first label fix
- keep full relative paths as the payload for `search_files`, but use collision-aware compact labels for `repo_outline`

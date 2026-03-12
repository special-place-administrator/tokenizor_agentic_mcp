# Research: Phase 1 Path Index Options

Related plan:

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [20-D-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-D-phase1-query-duplication-discovery.md)
- [21-R-phase1-query-layer-shape-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/21-R-phase1-query-layer-shape-research.md)
- [22-T-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-T-phase1-path-index-options-research.md)

Goal:

- choose the lightest path-index substrate that can support Phase 2 path discovery without prematurely paying for a heavier fuzzy-path structure

## Current Code Constraints

- `LiveIndex` in `src/live_index/store.rs` currently maintains only:
- `files`
- `reverse_index`
- a content-oriented `trigram_index`
- `TrigramIndex::build_from_files` in `src/live_index/trigram.rs` indexes `file.content`, not paths
- `TrigramIndex::remove_file` currently removes a path by scanning all trigram posting lists with `retain`, which is acceptable once for content search but becomes more expensive if another trigram index is added for paths too
- path discovery today is effectively ad hoc:
- sidecar prompt path hints in `src/sidecar/handlers.rs` scan all indexed paths for exact path or basename matches
- there is no dedicated path index in `store.rs`
- current discovery still indexes only recognized source files via `src/discovery/mod.rs`, so any Phase 1 path substrate is initially a code-lane substrate and should not overfit the later non-code text lane before task 23 resolves that boundary

## Phase 2 Needs This Must Support

- exact or near-exact file resolution for `resolve_path`
- clean handling of repeated basenames such as `mod.rs`, `lib.rs`, `index.ts`, and `README.md`
- bounded candidate sets for `search_files`
- deterministic ranking inputs for code-first path discovery
- cheap enough updates for watcher-driven `update_file` and `remove_file`

## Candidate A: Basename Map

### Shape

- add `files_by_basename: HashMap<String, Vec<String>>`
- optionally index both full basename and basename stem if Phase 2 wants `store` and `store.rs` to behave similarly

### Advantages

- best exact-basename fast path
- naturally exposes ambiguity when one basename maps to many paths
- very low memory cost: roughly one posting per indexed file, or two if stem and full-name keys are both stored
- cheap mutation cost on add, update, and remove

### Weaknesses

- weak for directory-scoped queries such as `live_index store`
- weak for partial path or directory-fragment search
- insufficient on its own for a credible `search_files` surface

### Verdict

- necessary, but not sufficient alone

## Candidate B: Directory-Component Map

### Shape

- add `files_by_dir_component: HashMap<String, Vec<String>>`
- index normalized directory components such as `src`, `protocol`, `live_index`, `tests`
- maintain a small reverse mapping per path so removals touch only that path’s components

### Advantages

- good fit for common scoped path queries where users remember the directory but not the exact filename
- pairs well with basename lookup by intersecting basename and directory candidates
- memory cost stays modest because each file contributes only its path components, not all 3-byte windows
- update cost is proportional to component count, which is much smaller than path trigram count

### Weaknesses

- still not enough for arbitrary mid-token substring matching
- ranking behavior still needs a separate policy once multiple candidate files survive

### Verdict

- useful companion index, especially for repeated basename disambiguation

## Candidate C: Path Trigram Index

### Shape

- add a second trigram-like index over normalized relative paths instead of file contents
- use it to accelerate arbitrary substring search across full paths

### Advantages

- strongest support for fuzzy-ish path substring queries
- can match across basename and directory boundaries with one structure
- gives `search_files` the broadest matching surface immediately

### Weaknesses

- current `TrigramIndex` implementation is optimized for content search, not cheap path mutation
- adding a second trigram index means:
- another `build_from_files` pass at load and reload
- another per-file `update_file`
- another `remove_file` path that scans all trigram posting lists with `retain`
- more memory than basename or directory maps because each path contributes many trigrams and common trigrams such as `src`, `.rs`, and `/in` create large posting lists
- does not solve ranking by itself; it only improves candidate generation

### Verdict

- powerful, but heavier than necessary for the first Phase 2 path substrate

## Candidate D: Small Hybrid

### Shape

- add `files_by_basename`
- add `files_by_dir_component`
- keep a lightweight fallback over normalized full paths only after candidate narrowing
- defer a dedicated path trigram index unless benchmarks show the cheaper hybrid is not enough

### Why This Fits Phase 2 Best

- `resolve_path` wants exact basename resolution first, not fuzzy matching first
- repeated basenames are handled directly by the basename map
- directory components provide a cheap second dimension for disambiguation and scoped discovery
- the shared query layer chosen in task 21 can apply deterministic ranking over a much smaller candidate set without forcing a new heavyweight index immediately
- this aligns with the plan’s “prefer additive slices” rule and keeps before-and-after benchmark attribution clear

### Memory And Update Cost

- materially cheaper than a path trigram index
- watcher updates stay simple because basename and component maps only need to touch keys associated with that one path
- avoids doubling the current trigram maintenance cost before there is evidence that arbitrary path-substring matching is a real bottleneck

### Remaining Gap

- arbitrary partial substring path queries may still be weaker than a future path trigram design
- that is acceptable for the first implementation as long as Phase 2 covers the common workflows:
- exact basename resolution
- repeated-basename disambiguation
- directory-scoped narrowing
- deterministic path-rich output

## Preferred First Implementation

- add a small hybrid:
- `files_by_basename`
- `files_by_dir_component`
- a reverse mapping from path to indexed basename and directory-component keys for cheap removal
- keep path trigram deferred

## Suggested First Query Behavior Over That Substrate

- first try exact normalized path match
- then exact basename match
- then basename plus directory-component intersection
- then, only if needed, do a cheap normalized path prefix or substring scan over the narrowed candidate set
- keep final ranking rules for Phase 2, but ensure the substrate already exposes enough signal for:
- exact hits first
- basename hits before loose directory-only hits
- deterministic tie-break by path

## Why Not Start With Path Trigrams

- the current codebase does not yet prove that arbitrary substring path search is the first blocker
- the current trigram implementation has relatively expensive remove semantics for watcher mutations
- task 23 still needs to decide how far the non-code text lane expands the indexed path universe
- if the file universe grows after task 23, paying for a second trigram index before that decision would be premature

## Benchmark Notes

- benchmark validation should focus on candidate set size, latency, and ranking quality for repeated-basename repos
- the decision to add a future path trigram index should be triggered only if basename-plus-directory maps still leave too many candidates or too much linear fallback work on realistic fixture repos

## Carry-Forward

- preferred path substrate: basename map plus directory-component map
- defer dedicated path trigram until benchmark evidence shows the lighter hybrid is insufficient
- OPEN: whether basename stem indexing is needed in addition to full basename should be validated during Phase 2 implementation
- OPEN: final ranking heuristics remain a Phase 2 concern, not a blocker for the Phase 1 substrate choice

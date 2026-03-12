# Research: Phase 2 Search Files Output And Ranking

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md)
- [69-R-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md)
- [70-T-phase2-resolve-path-code-lane-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/70-T-phase2-resolve-path-code-lane-shell.md)
- [71-T-phase2-search-files-output-and-ranking-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/71-T-phase2-search-files-output-and-ranking-research.md)

Goal:

- define the first bounded `search_files` output shape and ranking policy for the current code lane

## Current Evidence

- the current repo already has real repeated-basename collisions in code-bearing files:
  - `mod.rs`: 10 occurrences
  - `lib.rs`: 2 occurrences
- the old sidecar prompt-hint helper only succeeds on:
  - exact path string inclusion
  - unique basename mention
- once a basename is ambiguous, that helper returns no hint at all

That confirms the first `search_files` shell must do more than “unique basename or nothing,” but it also does not need heavy fuzzy ranking to be immediately useful.

## Output Shape Decision

### Rejected: Per-Line Path Plus Reason Labels

Example:

- `[basename] src/live_index/mod.rs`
- `[component] src/protocol/mod.rs`

Why reject:

- full relative paths already provide the essential disambiguation
- repeating match reasons on every line burns tokens fast on the exact cases that matter most, such as many `mod.rs` hits
- the reason is more useful as a group-level ranking cue than as per-line noise

### Preferred: Tier Headers Plus One Full Path Per Line

Shape:

```text
N matches
── Strong path matches ──
  src/protocol/tools.rs
── Basename matches ──
  src/sidecar/tools.rs
── Loose path matches ──
  src/protocol/mod.rs
  src/live_index/mod.rs
  ... and X more
```

Why this is the smallest safe choice:

- the path itself is the payload the caller actually needs
- tier headers explain ranking without bloating every result row
- it matches the project’s existing style for bounded ranked outputs such as `search_symbols`
- it gives a clean handoff to `resolve_path` when the caller wants one exact path

## Ranking Tiers

### Tier 1: Strong Path Matches

Include:

- exact normalized path match
- exact path suffix match
- basename plus all provided directory components matched

Why:

- these are the closest equivalents to “I know roughly which file I want”
- they should surface before plain basename collisions

### Tier 2: Basename Matches

Include:

- exact basename matches without enough extra components to disambiguate

Why:

- repeated basenames are a central Phase 2 problem
- they are still stronger than generic component-only hits

### Tier 3: Loose Path Matches

Include:

- component-only matches
- partial path substring fallback matches

Why:

- these are still useful discovery results
- but they should not outrank clear basename or suffix evidence

## Within-Tier Ordering

Use deterministic tie-breakers:

1. shorter normalized path first
2. lexical path order

This is simple, stable, and easy to test. The first shell does not need a deeper scoring model yet.

## Bounded Result Policy

Recommended first public contract:

- input:
  - `query: String`
  - optional `limit`
- default limit:
  - `20`
- hard cap:
  - `50`

Why:

- `search_files` is discovery-oriented, so it needs a larger default than `resolve_path`
- path lines are longer than symbol result lines, so the default should still stay modest
- a hard cap avoids pathological output on repos with many repeated names

No-match behavior:

- `No indexed source files matching '{query}'`

Empty-query behavior:

- `Path search requires a non-empty query.`

## Ranking Inputs To Use In The First Shell

- basename map
- directory-component map
- cheap normalized full-path suffix / substring scan only after indexed candidate narrowing or basename miss

Do not add:

- path trigram index
- basename stem indexing
- mixed-lane text candidates

Those are still later concerns.

## Recommended Next Implementation Slice

- implement a code-lane `search_files` shell with:
  - `query` plus optional bounded `limit`
  - tiered full-path output
  - basename/component ranking over the existing path indices
  - cheap path scan fallback only when indexed narrowing is insufficient

This is the right next move because it:

- directly addresses the biggest remaining shell escape
- builds on the new `resolve_path` behavior instead of competing with it
- stays inside the proven Phase 1/Phase 2 substrate

## Carry Forward

- use full relative paths as result lines
- explain ranking through tier headers, not per-result reason labels
- keep the first `search_files` shell code-lane only
- defer heavier fuzzy-path machinery until real misses justify it

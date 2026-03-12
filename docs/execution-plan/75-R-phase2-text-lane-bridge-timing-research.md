# Research: Phase 2 Text-Lane Bridge Timing

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [64-R-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-R-phase1-file-classification-heuristics-research.md)
- [69-R-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md)
- [73-R-phase2-repo-outline-path-rich-label-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/73-R-phase2-repo-outline-path-rich-label-research.md)
- [75-T-phase2-text-lane-bridge-timing-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/75-T-phase2-text-lane-bridge-timing-research.md)

Goal:

- decide whether any more Phase 2 path-discovery implementation is justified before the text-lane registry exists

## Current State

Phase 2 code-lane work is now in place:

- `resolve_path`
- `search_files`
- upgraded `repo_outline` labels

The remaining future promise is narrower:

- eventually allow non-binary text resolution for read workflows

But the substrate for that promise still does not exist in production:

- discovery is code-only
- watcher ingestion is code-only
- `RepoOutlineView` is published from the semantic code lane only
- the preferred text-lane design is still a separate lightweight registry plus bounded cache
- `FileClass::Text` exists as a model seam, but current indexed files are still only semantic-lane `Code`

## Why An Interim Mixed-Lane Bridge Would Be Wrong

### Rejected: Ad Hoc Disk Scan Inside `resolve_path`

Possible idea:

- keep code-lane indices
- add a filesystem walk or lazy glob fallback for `README.md`, `*.json`, and similar text files

Why reject:

- it would introduce a second, non-authoritative source of truth beside the live in-memory state
- watcher and persistence would not own those results, so output could drift with on-disk changes outside the published state model
- ranking would become harder to reason about because semantic-lane hits would come from indexed memory while text hits would come from ad hoc disk inspection
- it would duplicate logic that the future text registry must own anyway

This would weaken the architecture for a short-term feature win.

### Rejected: Broadening The Semantic Index Just For Path Resolution

Possible idea:

- start admitting non-code text into `LiveIndex.files` only so `resolve_path` can see them

Why reject:

- task 23 already rejected extending the semantic `IndexedFile` path to arbitrary non-code text
- that would increase watcher mutation cost and memory residency for the wrong reason
- it would blur the semantic-vs-text lane boundary right before later phases need it to stay crisp

## Decision

- do not add any more Phase 2 path-discovery implementation before the text registry exists
- treat current Phase 2 path discovery as complete on the present substrate
- keep `resolve_path` as the first future mixed-lane candidate, but only after authoritative text-lane membership and update paths exist

## Why This Is Phase-Correct

- Phase 2 acceptance is already satisfied for the current code-first shell-escape workflows:
  - path search exists
  - path resolution exists
  - repeated basename ambiguity is handled cleanly
  - output is deterministic and bounded
- the remaining mixed-lane requirement is not actually a path-ranking problem anymore
- it is a registry, watcher, and retrieval-lane problem that belongs with the later text search and read work

## Recommended Next Task

- move to Phase 3 research rather than forcing one more Phase 2 implementation
- the best next slice is to define the minimum scoped `search_text` contract, including how future non-code text participation should work without harming code-first defaults

Why this next task:

- Phase 3 explicitly needs that research
- it is the first place where the future text lane needs a user-facing contract rather than a speculative path-only bridge
- it can guide the later text-registry and read-path work without regressing the current path tools

## Carry Forward

- Phase 2 should stop at code-lane path discovery on the current substrate
- future mixed-lane `resolve_path` should wait for the text registry, not invent an interim filesystem fallback
- the next justified work is Phase 3 research on scoped search contract and text-lane participation

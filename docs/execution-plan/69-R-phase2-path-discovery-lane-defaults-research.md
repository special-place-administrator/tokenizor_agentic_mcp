# Research: Phase 2 Path Discovery Lane Defaults

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [67-R-phase1-dual-lane-option-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/67-R-phase1-dual-lane-option-defaults-research.md)
- [68-T-phase1-explicit-current-tool-option-defaults-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/68-T-phase1-explicit-current-tool-option-defaults-shell.md)
- [69-T-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-T-phase2-path-discovery-lane-defaults-research.md)

Goal:

- decide how the first Phase 2 path discovery surfaces should behave across the semantic code lane and the future lightweight text lane without breaking the project’s code-first posture

## Current State

- path substrate already exists for the current semantic lane:
  - `files_by_basename`
  - `files_by_dir_component`
- current published discovery-ish surfaces are semantic-lane only:
  - `repo_outline`
  - `file_tree`
- the preferred text-lane design is still separate:
  - lightweight registry
  - bounded cache
  - lane-aware query routing

So there is no honest mixed-lane source of truth yet. Any Phase 2 default that pretends otherwise would be speculative.

## Decision: Discovery Surfaces Should Not All Widen At Once

### 1. `repo_outline`

Recommendation:

- keep `repo_outline` code-lane only until the text registry exists

Why:

- current implementation is published from the semantic repo-outline snapshot only
- the Phase 2 task for `repo_outline` is primarily about path-rich labeling and ambiguity reduction, not about widening the file universe first
- silently mixing text-lane files into the current outline before a real mixed-lane data source exists would be artificial

Conclusion:

- improve `repo_outline` labels first
- defer any mixed-lane mode until there is authoritative text-lane metadata to merge

### 2. `search_files`

Recommendation:

- first implementation should be code-lane only, but shaped so an `All`-lane mode can be added later

Why:

- Phase 2 needs to eliminate shell escapes quickly for code workflows
- basename and directory-component indices currently only cover the semantic lane
- mixed-lane search without a real text registry would produce misleading semantics and incomplete results

Conclusion:

- implement `search_files` over the current code-lane substrate first
- preserve a ranking/model boundary that can later merge text-lane candidates without changing the public tool shape again

### 3. `resolve_path`

Recommendation:

- make `resolve_path` the first future path discovery surface allowed to widen to text-lane candidates once the text registry exists

Why:

- the Phase 2 plan explicitly calls for allowing non-binary text resolution for read workflows
- that requirement is narrower than broad file search
- read workflows benefit from resolving `README.md`, config files, and similar text targets earlier than broad mixed-lane search needs to

Recommended future ordering once the text registry exists:

1. exact semantic code path match
2. exact semantic basename / basename+component winner
3. exact text-lane path match
4. exact text-lane basename / basename+component winner
5. ambiguous mixed-lane result list with code-first ordering

Conclusion:

- yes, `resolve_path` should be allowed to consider future text-lane candidates before `search_files` does

## Ranking Posture

Default ranking should stay code-first:

- exact path before basename before loose component matches
- semantic code hits before text-lane hits when both are plausible
- deterministic tie-break by normalized path

This matches:

- the project’s coding-first mission
- the Phase 2 acceptance criteria
- the separate-lane design from task 23

## Recommended Next Implementation Slice

- start Phase 2 with a code-lane `resolve_path` shell over the existing basename/component indices

Why this is the best first step:

- smallest user-visible shell-escape reduction
- directly exercises the path substrate chosen in task 22
- keeps mixed-lane expansion out of the first implementation
- gives later `search_files` and `repo_outline` work a concrete ranking/result shape to build on

Suggested first behavior:

- exact normalized path hit returns immediately
- exact basename match returns a deterministic result when unique
- basename plus directory-component narrowing resolves repeated names
- ambiguous results return bounded disambiguation output instead of guessing

## Carry Forward

- `repo_outline` stays code-lane only until a real text registry exists
- `search_files` should not pretend to be mixed-lane before there is authoritative text-lane metadata
- `resolve_path` is the right first future bridge to text-lane read workflows
- the next small implementation should be a code-lane `resolve_path` shell

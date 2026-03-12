# Research: Phase 1 File Classification Heuristics

Related plan:

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [63-R-phase1-remaining-substrate-priority-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/63-R-phase1-remaining-substrate-priority-research.md)

Goal:

- define the smallest deterministic classification model that can support Phase 1 without prematurely broadening the semantic index into an all-files registry

## Current Code Reality

- discovery is code-only today:
- `discover_files` admits only files whose extension maps to `LanguageId`
- watcher is code-only today:
- `supported_language()` gates `maybe_reindex()` through `LanguageId::from_extension`
- parsing only runs for files that already have a `LanguageId`
- current file metadata carriers have no room for classification:
- `DiscoveredFile`
- `FileProcessingResult`
- `IndexedFile`
- `IndexedFileSnapshot`
- the earlier text-lane research already concluded:
- do not extend the current semantic `IndexedFile` path to arbitrary non-code text files
- use a lightweight text registry later instead

That means the first classification slice must preserve the current semantic-lane boundary while still creating a model that future text-lane work can reuse.

## Candidate A: Six Independent Booleans Added Ad Hoc To Current File Structs

### Shape

- add `is_code`, `is_text`, `is_binary`, `is_generated`, `is_test`, and `is_vendor` directly to current structs
- derive them wherever convenient during load and watcher updates

### Advantages

- superficially small
- matches the plan wording literally

### Weaknesses

- allows invalid combinations such as `is_code` and `is_binary` both being true
- makes it too easy for different call sites to derive flags differently
- does not make the code-lane vs text-lane boundary explicit

### Verdict

- reject

## Candidate B: Broaden Discovery Immediately To All Files And Classify Everything Now

### Shape

- discovery walks all files
- classify every file as code, text, or binary immediately
- attach generated/test/vendor tags at the same time

### Advantages

- complete on paper
- would fully populate all six planned flags immediately

### Weaknesses

- collides with the small-slice rule
- forces text-lane and watcher expansion in the same task as classification
- risks accidental semantic-index broadening before the lightweight text registry exists

### Verdict

- reject for now

## Candidate C: Use A Mutually Exclusive File Class Plus Orthogonal Noise Tags

### Shape

- introduce a small internal classification model:
- `FileClass`:
- `Code`
- `Text`
- `Binary`
- orthogonal noise tags:
- `is_generated`
- `is_test`
- `is_vendor`
- expose `is_code`, `is_text`, and `is_binary` as derived helpers from `FileClass`

### Advantages

- prevents impossible combinations
- matches the planned dual-lane model cleanly
- lets the first implementation stay semantic-lane-only while reserving `Text` and `Binary` for the future registry
- gives later `NoisePolicy` and `SearchScope` real substrate instead of loosely related booleans

### Weaknesses

- slightly more design work than six raw fields
- requires small compatibility updates anywhere snapshots or tests construct file metadata directly

### Verdict

- preferred

## Preferred Classification Rules

### Class Axis

- `Code`
- file has a supported `LanguageId`
- this is decided before parse and remains true even if parsing later partially fails or fully fails
- `Text`
- reserved for future non-code text-lane entries
- should mean non-code, non-binary text, not “any file whose bytes happen to be textual”
- `Binary`
- reserved for future non-code binary entries or exclusions
- should be the complement of the future text sniff for non-code files

The important rule is:

- `Code`, `Text`, and `Binary` are mutually exclusive
- code files are not also marked as text

### Noise Tags

- `is_test`
- path and basename heuristics on normalized relative paths
- strong initial matches only:
- path segment `tests`, `test`, `__tests__`, or `spec`
- basename prefix `test_`
- basename suffix `_test`, `.test`, `_spec`, or `.spec`
- `is_vendor`
- strong path-segment heuristics only:
- `vendor`
- `third_party`
- `third-party`
- `node_modules`
- `.venv`
- `venv`
- `site-packages`
- `Pods`
- `is_generated`
- strong generated-artifact heuristics only in the first shell
- exact path segments such as `generated`, `__generated__`, or `generated-sources`
- filename patterns such as `.generated.`, `.g.dart`, `.pb.go`, `.designer.cs`, or `.min.js`
- do not start with content-banner parsing or weak guesses like any `build/` path in the first slice

These three tags are orthogonal and may all be false or overlap.

## Why The Rules Must Be Index-Time

- Refactor 6 explicitly says not to recompute generated/test/vendor noise on every query
- current load and watcher paths already have the needed inputs:
- normalized relative path
- supported language decision
- raw bytes when relevant
- computing classification before parse means parse failures still retain stable classification
- persisting the result avoids drift across snapshot restore and watcher/live reload paths

## First Implementation Boundary

- keep the first shell scoped to the current semantic lane
- add classification types and metadata to:
- `DiscoveredFile`
- `FileProcessingResult`
- `IndexedFile`
- `IndexedFileSnapshot`
- compute:
- `FileClass::Code` for current discovered/indexed files
- noise tags from the normalized relative path
- explicitly defer population of `Text` and `Binary` entries until the lightweight text registry exists

That gives the codebase a real classification substrate without broadening discovery or watcher behavior in the same slice.

## Ownership Boundary

- classification types should live with shared file metadata, not inside protocol or formatter code
- the best fit is the domain-layer metadata surface used by discovery, parsing, watcher updates, persistence, and query filters
- classification computation should be a pure reusable helper invoked by:
- discovery/load
- watcher reindex
- any future text-lane registry ingestion

## Why Not Detect Generated Files From File Contents Yet

- content-banner heuristics such as `DO NOT EDIT` can be useful later
- but they are easy to overfit and would increase classification ambiguity in the first shell
- strong path and filename rules are deterministic, cheap, and easy to test

## Watcher And Text-Lane Implications

- watcher should keep its current supported-language gate in the first shell
- the first shell should not begin indexing Markdown, JSON, or other non-code text into `LiveIndex.files`
- the later text-lane registry can reuse the same `FileClass` model to populate `Text` and `Binary` entries without redesigning the metadata again

## Recommended Next Implementation Slice

1. add the classification types and derived helpers
2. thread classification through discovery, parse result, live index, and snapshot persistence
3. keep current behavior unchanged except for new metadata availability
4. add focused tests for path-based generated/test/vendor classification and persistence round-trip

## Carry Forward

- preferred model: mutually exclusive `FileClass` plus orthogonal noise tags
- first implementation should classify only the current semantic-lane files as `Code`, with path-based generated/test/vendor tags
- explicitly deferred:
- text-lane ingestion
- binary sniff activation for broader discovery
- content-banner generated detection

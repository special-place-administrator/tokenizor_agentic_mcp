# Research: Phase 1 Text Lane Boundary

Related plan:

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md)
- [23-T-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-T-phase1-text-lane-boundary-research.md)

Goal:

- choose the lightest reliable boundary between the semantic code lane and the non-code text lane without turning the current semantic index into an all-files in-memory store

## Current Code Constraints

- discovery is source-only today:
- `src/discovery/mod.rs` admits only files with recognized language extensions
- watcher updates are source-only today:
- `src/watcher/mod.rs` gates reindexing through `LanguageId::from_extension`
- watcher integration tests explicitly assert that `README.md` and `config.json` are ignored
- `get_file_content` in `src/protocol/tools.rs` and `src/protocol/format.rs` only serves files present in `LiveIndex.files`
- `LiveIndex::update_file` in `src/live_index/store.rs` currently:
- updates the content trigram index
- inserts the file into `files`
- rebuilds the semantic `reverse_index`
- that update path is correct for code files, but it is the wrong mutation cost profile for large numbers of non-code text files
- there is no existing file classification metadata such as `is_code`, `is_text`, or `is_binary`

## What The Boundary Must Preserve

- semantic code files stay fully in-memory with symbols, references, and code-first ranking
- non-code text support becomes available for path discovery, file reading, and later plain-text search
- watcher costs do not explode because every Markdown, JSON, or TOML change rebuilds semantic structures
- memory use stays bounded and proportional to actual recent text-lane usage

## Candidate A: Pure Lazy Reads

### Shape

- do not track non-code text files in memory
- when a tool asks for one, resolve the path from disk and read the file on demand

### Advantages

- lowest immediate memory use
- very small code surface at first glance
- no cache invalidation logic

### Problems

- no authoritative in-memory registry for path discovery or filtering
- weak watcher story because create, modify, and remove events for non-code text do not update any runtime state
- repeated reads keep paying disk I/O
- text search over non-code files would require filesystem walks or repeated path scans outside the current index model
- result coherence is weaker because reads come straight from disk instead of from a runtime-owned view

### Verdict

- too weak as the boundary by itself
- acceptable only as an internal content-fetch mechanism behind a registry, not as the lane design

## Candidate B: Bounded Cache Only

### Shape

- add an LRU-like cache of recently read non-code text file content
- populate it only when reads happen

### Advantages

- repeated reads become cheaper than pure lazy reads
- memory can be capped explicitly

### Problems

- cache-only does not answer which non-code text files are eligible
- cache-only does not solve path discovery, watcher invalidation, or lane membership
- without a registry, every miss still needs ad hoc disk resolution rules
- the system would still lack a durable lane boundary, only a retrieval optimization

### Verdict

- useful as part of the design
- not sufficient as the design

## Candidate C: Lightweight Text Registry

### Shape

- add a separate non-code text registry in `src/live_index/store.rs`
- keep metadata only for eligible non-binary text files, for example:
- normalized path
- size
- modified time or content hash
- lightweight classification flags
- optional path-index participation
- read actual bytes lazily when a tool needs content
- optionally place recently used bytes or line tables in a bounded cache

### Advantages

- creates a real lane boundary without forcing semantic parsing or permanent full-content residency
- path discovery can include text-lane files without pretending they are semantic code files
- watcher updates can stay cheap:
- update metadata
- invalidate any cached content
- avoid semantic reverse-index rebuilds for text-only file changes
- memory profile stays controlled because metadata is cheap and content residency is capped
- aligns with the plan’s recommendation to prefer bounded caching, lazy reads, or a lightweight registry over full semantic expansion

### Problems

- requires clear classification rules for what counts as non-binary text
- adds a second read path that must stay deterministic
- uncached text search will still pay disk reads unless later caching or lightweight indexing is added

### Verdict

- best core boundary

## Preferred Boundary

- use a lightweight non-code text registry as the authoritative text-lane membership layer
- pair it with a bounded on-demand content cache
- allow lazy disk reads only on cache miss

## Why This Is The Smallest Safe Choice

- it avoids turning `LiveIndex.files` into an all-files container
- it avoids forcing `LiveIndex::update_file` and semantic `reverse_index` rebuilds on every non-code text edit
- it gives path discovery and later search/read features a stable source of truth for which non-code text files exist
- it preserves code-first behavior because semantic and text lanes remain separate data structures with separate ranking defaults

## Recommended Ownership Boundary

- `src/live_index/store.rs`
- semantic lane remains the current `files`, `reverse_index`, and content trigram index
- add a text-lane registry and cache state here, not inside `IndexedFile`
- `src/live_index/query.rs` or the shared query module chosen in task 21
- decide which lane a request should hit
- merge or prioritize results while keeping code-first ranking
- `src/protocol/tools.rs`
- stay lane-agnostic
- dispatch `get_file_content`, later `search_text`, and path tools into lane-aware query helpers instead of assuming everything lives in `LiveIndex.files`

## Watcher Implications

- watcher should not tree-sitter-parse non-code text files
- watcher should not rebuild semantic reverse indices for non-code text changes
- watcher should update text-lane metadata and invalidate cached content for modified text files
- this likely means a separate metadata update path instead of reusing the current source-file `maybe_reindex` path unchanged

## Memory Profile Implications

- semantic code lane memory stays tied to actual parsed code files
- text lane pays only:
- metadata for all tracked non-code text files
- bounded cache residency for recently used content
- this is materially safer than storing all Markdown, JSON, TOML, YAML, shell, and config bytes in the same always-hot structures as semantic code files

## Read And Search Implications

- `get_file_content` should first check the semantic lane, then fall back to the text-lane registry
- path discovery should include text-lane files once the registry exists, but rank semantic code results first by default
- later text-lane `search_text` can operate over registry-selected candidate files with lazy reads or cached content, rather than requiring all text content to be permanently indexed

## Why Not Put Non-Code Text Straight Into `LiveIndex.files`

- current `IndexedFile` is designed around semantic code concerns:
- language id
- parsed symbols
- references
- semantic-content trigram participation
- putting non-code text into that same structure would:
- muddy the code-first mental model
- increase watcher mutation cost
- pressure memory upward immediately
- make later ranking and filtering rules harder to reason about

## First Implementation Guidance After This Research

- first add the boundary, not the whole feature set
- the smallest follow-on slice should likely:
- define text-lane metadata structures
- define lane-selection rules for reads
- keep content caching bounded and optional
- defer richer non-code text search behavior until after the registry boundary is benchmarked

## Carry-Forward

- chosen boundary: lightweight text registry plus bounded content cache, with lazy reads only on cache miss
- do not extend the current semantic `IndexedFile` path to non-code text files
- unresolved risk: the exact file-classification rule for non-binary text still needs implementation-time definition
- unresolved risk: if text-lane reads become frequent enough, cache sizing and invalidation rules will need benchmarks rather than intuition

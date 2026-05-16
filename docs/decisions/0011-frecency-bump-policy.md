# 0011. Frecency bump policy: commitment tools only, never discovery

Date: 2026-04-18
Status: Accepted

## Context

SymForge introduces a per-workspace frecency signal to rank
[`search_files`](../../src/protocol/tools.rs) results. The signal lives in a
SQLite store
([`src/live_index/frecency.rs`](../../src/live_index/frecency.rs), opened at
`.symforge/frecency.db`) and decays on a 7-day half-life. Fusion with the
existing path-match and co-change signals happens inside the `RankSignal`
registry introduced by
[ADR 0012](./0012-edit-and-ranker-hook-architecture.md) — frecency registers
as one more signal, not as a special case of the ranker.

The decision this ADR records is not *how* frecency is stored or decayed —
the storage shape and decay formula are spec decisions, not architectural
ones. The load-bearing question is **which SymForge tools move a file's
frecency score.** Getting that answer wrong corrupts rankings for every
subsequent session.

### The positive-feedback-loop hazard

Every SymForge tool that reads a file is a candidate bump point. The
intuitive answer — "bump whenever the agent touches a file" — collapses on
inspection:

1. Agent searches for `cache` via `search_files`. The top 20 results bump.
2. Next session, the same `cache` search. The same 20 files now have higher
   frecency scores, so `rank_by="frecency"` pushes them even further up.
3. The bias compounds per search. Files that were surfaced for *any* reason
   become durably surfaced for *every* reason. The signal that was meant to
   reflect the agent's working set instead reflects the ranker's own
   history.

The fix is not tuning — it is a structural policy. Searches are
*discovery*; they surface candidates the agent hasn't decided to engage
with. Read tools that resolve to a specific known file are *commitment*;
the agent has already picked the file and is now doing work on it. Edits
are commitment by construction. Only commitment tools should bump.

### The bump surface, enumerated

The relevant handlers on `impl SymForgeServer` in
[`src/protocol/tools.rs`](../../src/protocol/tools.rs) partition cleanly:

**Commitment (bump ON):**

- `replace_symbol_body` (L6377-6532).
- `insert_symbol` (L6541-6666).
- `delete_symbol` (L6674-6790).
- `edit_within_symbol` (L6800-7021).
- `batch_edit` (L7031-7092).
- `batch_rename` (L7100-7139).
- `batch_insert` (L7146-7197).
- `get_file_context` (L2820-2890).
- `get_file_content` (L4482-4578).
- `get_symbol` (L2479-2646).
- `get_symbol_context` (L2902-3223).

**Discovery (bump OFF):**

- `search_files` (L3781-4034).
- `search_text` (L3439-3650).
- `search_symbols` (L3314-3428).

Edits reach the bump hook through
[`src/protocol/edit_hooks.rs`](../../src/protocol/edit_hooks.rs)'s
`after_edit_committed` per ADR 0012 — no inline bump call lives in an edit
handler body. Read-tool commitment paths call
`crate::live_index::frecency::bump(&paths)` directly at the end of the
happy path; those tools do not flow through `EditHook`, so the direct call
is the correct pattern.

### Why batch-tool dedup matters

A naive bump loop inside `batch_edit` would call `bump` once per edit.
Renaming a symbol across 20 call sites would bump each touched file once
per rewrite, skewing scores on files that happened to hold many matches.
The Implementation Notes § "Bump dedup per tool invocation" calls for a
`HashSet<PathBuf>` collected over the batch, drained into a single `bump`
call at the end. One invocation, one bump per distinct path — regardless
of how many internal edits ran.

### Three options considered

- **Option A — bump every tool that reads a file.** Rejected: the
  positive-feedback-loop collapse above.
- **Option B — bump only on edits.** Considered. Symmetric and obviously
  safe, but misses the case where an agent reads a file extensively
  during a commitment session (investigating a bug, reviewing a symbol
  before planning an edit). Frecency would lag real work by a full edit
  cycle, and read-only sessions (code review, exploration after commit)
  would leave zero trace.
- **Option C (chosen) — bump on edits + on read tools that resolve to a
  known single file; never on search.** Preserves the invariant that
  search is side-effect-free for rankings while capturing the real
  working set, including read-heavy sessions.

## Decision

Bump frecency on every **commitment** tool and on no **discovery** tool,
with per-invocation dedup for batch operations. Concretely:

1. **Every edit handler** bumps via the `EditHook::after_edit_committed`
   registration in
   [`src/live_index/frecency.rs`](../../src/live_index/frecency.rs). The
   seven edit handlers do not call `bump` inline — they route through
   [`src/protocol/edit_hooks.rs`](../../src/protocol/edit_hooks.rs) per
   ADR 0012 invariant 1, and the registered `FrecencyBumpHook` calls
   `bump` from `after_edit_committed`.
2. **The four commitment-read handlers**
   (`get_file_context`, `get_file_content`, `get_symbol`,
   `get_symbol_context`) call
   `crate::live_index::frecency::bump(&paths)` at the end of their
   happy path, after the operation has succeeded. These tools do not
   flow through `EditHook`, so a direct call is the right pattern.
3. **The three discovery handlers**
   (`search_files`, `search_text`, `search_symbols`) never bump. Each
   handler body carries an explicit
   `// intentionally no frecency bump — discovery tool, prevents feedback loop`
   comment above the final return so a future reader does not
   "helpfully" wire one in.
4. **Batch handlers** (`batch_edit`, `batch_insert`, `batch_rename`)
   collect affected paths into a `HashSet<PathBuf>` during the batch
   and emit one `bump` call with the full set at the end. The
   `FrecencyBumpHook` implementation is the natural place to apply
   dedup when the batch handler fires `after_edit_committed` once per
   sub-edit, since the hook owns the set and the handler owns only
   the lifecycle callback.
5. **Frecency collection policy stays inside `bump`, not at call
   sites.** The original v7.5 rollout used `SYMFORGE_FRECENCY=1` as an
   opt-in persistent-store gate. After
   [ADR 0016](./0016-call-time-capability-resolution.md), unset
   `SYMFORGE_FRECENCY` means session-scoped in-memory collection,
   truthy/persistent values use `.symforge/frecency.db`, and explicit
   false/off/disabled values disable collection. Call sites still invoke
   `bump`; policy remains centralized.

### Rollout status as of this ADR

This ADR lands together with partial implementation: the four
commitment-read handlers call `bump` directly (item 2); the three
discovery handlers carry the no-bump guards (item 3); the call-site
`bump` façade in
[`src/live_index/frecency.rs`](../../src/live_index/frecency.rs) originally
routed to a test-observability sink gated on `SYMFORGE_FRECENCY=1`.
The `FrecencyBumpHook` registration that wires item 1 — and the
store-backed sink that replaces the test-observability layer — land in
subsequent todos on this tentacle; the architectural policy this ADR
records does not change when they do.

The scoring and fusion math live in ADR 0012's `RankSignal` registry and
the implementation notes in the spec; this ADR is specifically about the
commitment-vs-discovery policy and the invariants that policy places on
future handlers.

## Consequences

**Easier**

- Search-only sessions do not poison rankings. An agent running a
  `search_files` sweep to map an unfamiliar codebase leaves no
  frecency footprint, which is the correct behavior — the sweep was
  discovery, not work.
- Agents that read-then-edit (the common case) see frecency reflect
  intent. Opening `get_symbol` to understand a function before editing
  it is the first commitment signal; the subsequent edit adds another.
  By the next session, the agent's actual working set is visible.
- The no-bump boundary is code-comment-discoverable. Each of the three
  discovery handlers carries the "no bump" comment above its return
  statement; grepping `intentionally no frecency bump` surfaces all
  three sites immediately.

**Harder**

- Seven edit handlers plus four commitment-read handlers is eleven
  distinct bump call sites to audit against centralized frecency policy.
  The policy lives once inside `bump`, so reviewers still need to know
  that "bump was added" and "bump is live" are two different statements.
- The `EditHook` + direct-call split (edits via the registry;
  commitment reads via direct `bump`) means there are two bump
  code paths to keep in sync. The split is deliberate per ADR 0012 —
  edit handlers are the only ones flowing through `EditHook` — but
  future tools that behave like commitment reads must choose the
  right path.
- Per-invocation dedup on batch tools is easy to regress: a loop
  that calls `bump(&[path])` once per internal edit is a plausible
  refactor that silently re-introduces score skew. Tests in
  `tests/frecency_ranking.rs` pin this behavior.

**New invariants future code must respect**

1. **Discovery tools MUST NOT bump.** Any new tool that behaves like a
   search — returns candidate files the caller has not yet committed
   to — must omit the bump call and document the omission inline.
   `search_files`, `search_text`, `search_symbols` are the canonical
   examples. Regressions re-introduce the positive-feedback-loop
   failure mode.
2. **Commitment tools MUST bump via the supported path.** Edits go
   through `EditHook::after_edit_committed` per ADR 0012; read-tool
   commitment handlers call
   `crate::live_index::frecency::bump(&paths)` directly. Inlining a
   bump into an edit-handler body violates ADR 0012 invariant 1;
   routing a commitment-read through `EditHook` misuses the edit
   lifecycle.
3. **Batch handlers MUST dedup within a single invocation.** Multiple
   `bump(&[path])` calls per invocation silently re-introduce the
   score-skew failure mode that per-invocation dedup exists to
   prevent.
4. **The `SYMFORGE_FRECENCY` policy MUST stay inside `bump`, not at
   call sites.** Centralizing the policy means no handler can forget it,
   and changing the environment/config default changes behavior
   uniformly. A call-site gate drifts; the central policy does not.
5. **`rank_by="frecency"` on `search_files` is the only user-visible
   API change this feature is allowed.** The spec's zero-new-tools
   constraint (§"Tool placement — zero new tools") is load-bearing;
   any proposal for a new MCP tool that exposes frecency data must
   go through the no-surprise rule in CONTEXT.md before landing.

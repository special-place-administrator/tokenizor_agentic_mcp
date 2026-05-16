# 0010. Worktree-aware edit routing via `working_directory`

Date: 2026-04-18
Status: Accepted

## Context

SymForge indexes one repo path and stores fully-qualified absolute paths in
its symbol index. When an edit tool resolves a symbol, it reads
`symbol.indexed_absolute_path` and writes there. That is the correct target
*only when the agent is operating inside the indexed repo copy*.

On 2026-04-16, an Octogent run on the GW2 Build Optimizer project exposed
the failure mode implied by that assumption. The operator spawned a tentacle
on branch `tentacle/core-domain-r2` inside a git worktree at
`../gw2-core-domain-r2` (a sibling of the indexed main checkout). The
agent called `edit_within_symbol` and `replace_symbol_body` against
symbols it had resolved via `get_symbol`. Each edit succeeded. Each edit
landed **in the indexed main checkout, not in the worktree** — because
`indexed_absolute_path` still pointed at the main repo. The worktree's
files were unchanged, the agent believed its work was done, and the
committed commit (`87e3db6`) was produced from the wrong tree. The only
way the operator noticed was a `git status` in the main checkout showing
unexpected dirty files.

The seven edit tools on `impl SymForgeServer` in
[`src/protocol/tools.rs`](../../src/protocol/tools.rs)
(`replace_symbol_body`, `insert_symbol`, `delete_symbol`,
`edit_within_symbol`, `batch_edit`, `batch_insert`, `batch_rename`) all
share this failure mode. Read tools (`get_symbol`, `search_symbols`,
`get_file_context`, etc.) are unaffected: reading the indexed copy is the
whole point of having an index. Writes are where the mismatch bites.

Three design options were considered:

- **Option A — per-call `working_directory` parameter.** The caller
  supplies the worktree root on every write; SymForge validates it against
  `git worktree list` and re-roots the indexed absolute path onto that
  worktree. Explicit, auditable, backward-compatible (old callers omit the
  field and get today's behaviour).
- **Option B — auto-detect the working directory from the MCP transport
  or from `$PWD`.** Zero caller effort at first glance, but conflates two
  distinct failure modes: (1) the agent is *in* a worktree and wants
  writes routed there; (2) the agent is *in* the indexed repo and the
  caller's `$PWD` is noise. MCP transports today do not carry a reliable
  client-side cwd, and `$PWD` on the server side is the SymForge process,
  not the agent. Silent magic would make the GW2 bug harder to diagnose
  next time, not easier.
- **Option C — per-worktree symbol indexes.** Give each worktree its own
  index at `.symforge/index.bin` rooted at the worktree. Fragments the
  index across every worktree (N × index cost), forces a reindex on every
  `git worktree add`, and still does not solve the case where the agent
  holds a cached symbol from the indexed repo and points it at a sibling.
  The cost is disproportionate to the benefit for the core failure mode.

Option A is also the minimum-surface change: it extends the public tool
contract by one field, touches no existing callers when omitted, and
composes cleanly with ADR
[0012](./0012-edit-and-ranker-hook-architecture.md)'s `EditHook` registry
— the worktree-aware behaviour lives in a registered `EditHook` impl, not
in the seven handler bodies.

## Decision

Adopt Option A. Every edit tool on `impl SymForgeServer` accepts an
optional `working_directory: Option<String>` field on its input struct.
When supplied, SymForge re-roots the indexed absolute path against the
caller's worktree root before writing. After ADR 0016, the parameter is
the call-time routing request; `SYMFORGE_WORKTREE_AWARE` is only a
policy/default knob. Unset or truthy policy allows explicit routing, while
false/off/disabled policy rejects requested routing before any write.

### Resolution algorithm (`src/worktree.rs`)

[`src/worktree.rs`](../../src/worktree.rs) owns the logic.
`resolve_target_path(indexed_abs, indexed_root, working_directory, cache)`
implements the algorithm from the source spec §2.2:

1. If `working_directory` is `None`, return `ResolvedTarget {
   target_path = indexed_abs, rerouted = false }` before any filesystem
   canonicalization. This is the byte-identical-to-pre-routing path.
2. Canonicalize `indexed_abs`, `indexed_root`, and `working_directory`
   with [`dunce::canonicalize`](https://docs.rs/dunce) — `std::fs::canonicalize`
   on Windows produces `\\?\` UNC prefixes that do not match what
   `git worktree list` prints.
3. If the canonical `working_directory` equals `indexed_root`, return
   `ResolvedTarget { target_path = indexed_abs, rerouted = false }`.
4. Otherwise look up the canonical working-directory path in a cached
   `git worktree list --porcelain` of the indexed root. On cache miss,
   shell out, repopulate, re-check. Unknown paths return
   `WorkingDirectoryNotARecognizedWorktree` with a hint pointing at
   `git worktree list`.
5. Strip the `indexed_root` prefix off `indexed_abs`, re-join against the
   canonical worktree path, verify the target file exists (worktrees at
   older commits may lack the file), and return
   `ResolvedTarget { target_path, indexed_path = indexed_abs,
   rerouted = true }`.

### Integration via `EditHook`

`WorktreeAwareEditHook` in
[`src/worktree.rs`](../../src/worktree.rs) implements the `EditHook`
trait defined by ADR 0012 and calls `resolve_target_path` from
`resolve_target_path(ctx)`. `register_if_feature_enabled()` now installs
the hook unconditionally; the hook resolves worktree policy at call time
so a supplied `working_directory` can route without restarting the MCP
server. The seven handler bodies do not branch on `working_directory`;
they route every path resolution through `edit_hooks::resolve()` per ADR
0012, and the hook decides.

### Response shape

Edit responses gain four additive fields rendered by
[`src/protocol/edit_format.rs`](../../src/protocol/edit_format.rs):

- `working_directory` — caller-supplied worktree root for this edit call.
- `wrote_to` — absolute path of the file that was actually written.
- `indexed_path` — absolute path the index believes the file lives at.
- `rerouted` — boolean, `true` iff `wrote_to != indexed_path`.

When `working_directory` is omitted, the resolved-target block is omitted
so existing response text remains unchanged. When it is supplied and equal
to the indexed root, the block reports `rerouted == false`.

### Visibility

The `health` handler in
[`src/protocol/tools.rs`](../../src/protocol/tools.rs) appends an
`edit tool calls without working_directory (last hour): N` line when the
transitional observability knob is on, so agents that forget to pass the
parameter show up in diagnostics rather than going silent. This counter
does not gate routing.

### Deferred

- **MCP-transport cwd auto-detection (Option B)** is an opt-in fallback
  (`SYMFORGE_AUTO_CWD_DETECT`) to revisit after the explicit path is
  stable. Out-of-scope for the first ship.
- **Per-worktree indexes (Option C)** are out-of-scope indefinitely.
- **File-level locking across worktrees** is out-of-scope; concurrent
  writes from different worktrees to the same relative path remain the
  caller's responsibility.

## Consequences

**Easier**

- Agents running inside a sibling worktree can now call SymForge edit
  tools without risk of silently writing to the indexed copy — the GW2
  failure mode is closed at the source. Pass `working_directory`, get
  `rerouted: true` in the response, verify the write landed in the
  intended tree.
- The contract is auditable. Every edit response with a supplied
  `working_directory` carries the resolved `wrote_to` path and the
  `rerouted` flag, so post-hoc log review can spot misrouted writes
  directly.
- Adoption is incremental. Callers that omit `working_directory` are
  byte-identical to pre-routing behaviour; the `health` misuse counter
  surfaces callers that forgot to adapt when the transitional
  observability knob is enabled, without breaking them.

**Harder**

- The edit-tool response shape gains four fields (`working_directory`,
  `wrote_to`, `indexed_path`, `rerouted`). Clients that depend on the
  exact response shape — for example, MCP clients that assert on a fixed JSON schema —
  must ignore unknown fields gracefully or pin the SymForge version.
- The `git worktree list` cache introduces a consistency window.
  `git worktree add` / `git worktree remove` calls made *outside*
  SymForge are not observed until the next cache miss triggers a refresh.
  For workflows that add a worktree and immediately call an edit tool
  with `working_directory` pointing at it, the miss-refresh loop absorbs
  the cost at the price of one extra `git worktree list` invocation.
- Shelling out to `git worktree list` on cache miss adds a subprocess
  spawn to the edit path. Expected cost is a single-digit-millisecond
  call, amortized across all subsequent edits for that cache lifetime;
  the alternative (linking `libgit2`) costs more in build surface.

**New invariants future code must respect**

1. The seven edit handlers MUST route path resolution through
   `edit_hooks::resolve()` (per ADR 0012 invariant 1). Inlining
   worktree-specific logic in a handler body re-introduces the failure
   mode this ADR fixes.
2. When `working_directory` is `None`, behaviour MUST be byte-identical
   to pre-routing releases. This is the backward-compat contract.
   Regression tests in `tests/worktree_awareness.rs` guard
   policy-disabled and omitted-field paths. A supplied indexed-root
   `working_directory` writes the same file but still emits the additive
   resolved-target evidence.
3. `SYMFORGE_WORKTREE_AWARE` is a policy/default knob, not the semantic
   trigger for a supplied `working_directory`.
4. `working_directory`, `wrote_to`, `indexed_path`, and `rerouted` are
   additive fields and MUST NOT be removed or renamed without a
   superseding ADR; clients may depend on them for auditability.
5. Windows canonicalization MUST use `dunce::canonicalize`, not
   `std::fs::canonicalize`. The latter emits `\\?\` UNC prefixes that
   do not match `git worktree list` output and silently break the
   cache lookup on Windows.

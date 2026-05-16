# 0016. Call-Time Capability Resolution

Date: 2026-05-16
Status: Accepted

## Context

SymForge advertises optional tool capabilities that are useful during an
agent session:

- `search_files(rank_by="frecency")`
- `search_files(rank_by="path+cochange", anchor_path=...)`
- edit tools with `working_directory`
- ranking diagnostics for explaining score contributions

The first shipped versions protected those capabilities with process
environment variables such as `SYMFORGE_FRECENCY`,
`SYMFORGE_COUPLING`, `SYMFORGE_WORKTREE_AWARE`, and
`SYMFORGE_DEBUG_RANKING`. That kept defaults deterministic and avoided
surprising startup work, but it created an MCP product problem: a client
can see an advertised tool parameter during a task but usually cannot
restart the already-running server with different environment variables.

The desired product contract is therefore parameter-first. If a call asks
for an advertised capability, SymForge must either honor that request at
call time or return concise evidence explaining why it did not.

This ADR establishes the contract only. It does not claim that the
production behavior has already migrated; that migration is assigned to
the call-time capability-resolution task pack.

## Decision

SymForge adopts a call-time capability-resolution contract for advertised
optional capabilities.

At minimum, a requested capability is applied, prepared with evidence,
marked unavailable, or marked disabled by policy.

When a request names a capability, the response must report one of these
outcomes:

- **applied** - the requested capability was used for this call.
- **preparing** - SymForge started or scheduled bounded local preparation
  and returned the deterministic fallback output for this call.
- **unavailable** - required local evidence or platform support is absent
  and no bounded preparation is possible for this call.
- **disabled by policy** - operator policy explicitly forbids the
  requested capability.
- **fallback used** - the call preserved default behavior while reporting
  the requested capability's state.
- **stale** - derived evidence exists but is stale enough that SymForge
  did not treat it as authoritative without saying so.

Environment variables and future config keys are policy/default knobs,
not the semantic trigger for advertised tool behavior. They may:

- disable a capability for this process or workspace;
- opt into background warmup;
- choose persistence policy;
- default diagnostics on for operators;
- cap cost or freshness thresholds.

They must not be the only path by which an MCP client can request an
advertised capability.

## Non-Goals

The first implementation pass will not build a multi-process router or a
multi-index SymForge swarm. One in-process server remains the execution
model.

The first pass will not add a broad generic `scope` parameter. Scoped
views may be revisited later, but call-time capability resolution starts
with the concrete capabilities already present in tool schemas or docs.

The first pass will not add a cloud control plane or external database for
query serving.

## Source Of Truth

`LiveIndex` remains authoritative for current file bytes, symbol spans,
references, parse status, and ordinary query results. Local derived stores
under `.symforge/` are advisory:

- frecency reflects agent commitment history;
- coupling reflects git-history relationships;
- edit-safety artifacts support recovery and audit;
- ranking diagnostics explain one call's scoring path.

Derived stores may improve ranking or evidence, but they must not silently
override current indexed bytes or symbol spans.

## Safety Rules

Write routing requires explicit call-time consent. For edit tools, a
supplied `working_directory` is the intended consent signal. SymForge must
validate the target before writing and include response evidence such as
the indexed path, resolved write target, and whether a reroute happened.
Unknown, unsafe, or policy-disabled routing must fail or report disabled
before any write.

Read-side capabilities must preserve deterministic default behavior when
the caller does not request them.

## Performance Rules

No heavy derived-store work runs on startup unless policy explicitly opts
into background warmup.

Call-time preparation must be bounded. If a requested capability needs
more work than the call can safely perform, SymForge returns fallback
results with `preparing`, `unavailable`, `stale`, or `disabled by policy`
evidence instead of hiding the condition.

The local-first, in-process read path remains a product requirement.

## Migration Order

1. Add a shared capability evidence and policy model.
2. Convert frecency so `rank_by="frecency"` resolves at call time and
   reports applied, empty-history, fallback, or disabled evidence.
3. Convert co-change so `rank_by="path+cochange"` lazily uses, prepares,
   or reports coupling evidence without eager startup work.
4. Convert worktree routing so validated `working_directory` is honored at
   call time unless policy disables it.
5. Add call-time ranking explanation, with environment variables only as
   defaults for operators.
6. Surface capability status in `health` or an equivalent status surface
   and add env-vars-unset integration coverage.

## Acceptance Criteria

- Advertised capabilities are resolved from the request at call time.
- Responses distinguish applied, preparing, unavailable, disabled by
  policy, fallback, and stale states where relevant.
- Environment variables are documented as policy/default overrides.
- Default calls that do not request advanced behavior keep current
  deterministic behavior.
- The first pass preserves one authoritative in-process `LiveIndex` and
  does not introduce a multi-process router.
- Env-vars-unset tests prove requested call-time behavior or explicit
  fallback evidence for frecency, co-change, worktree routing, and ranking
  diagnostics before the migration is considered shipped.

## Consequences

**Easier**

- MCP clients can request advertised behavior during a session instead of
  depending on server launch environment.
- Future capability work has a shared response vocabulary instead of
  feature-specific fallback text.
- Operators still retain policy controls for persistence, diagnostics,
  background warmup, and disablement.

**Harder**

- Tool responses must carry more evidence when requested capabilities
  cannot be applied.
- Derived-store lifecycle code must separate operator policy from request
  intent.
- Tests must cover both env-vars-unset request paths and policy-disabled
  paths.

**Implementation status**

This ADR is accepted as the product and architecture contract. The
production migration is pending the tasks in
`docs/plans/2026-05-16-call-time-capability-resolution/`.

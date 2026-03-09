# Sprint Change Proposal: SpacetimeDB-First Persistence Correction

Date: 2026-03-08
Trigger Story: 4.2-resume-interrupted-indexing-from-durable-checkpoints
Mode: Incremental
Recommended Scope Classification: Major

## 1. Issue Summary

Story 4.2 successfully implemented resumable indexing from durable checkpoints, but it also exposed a structural mismatch between the current implementation path and the product's stated architecture.

The project artifacts already define SpacetimeDB as the authoritative control plane for structured operational state. In practice, mutable operational durability for runs, checkpoints, per-run file records, and idempotency still flows through the local JSON `RegistryPersistence` path. Story 4.2 made the correct immediate correctness tradeoff on that path, but doing so surfaced early persistence debt and confirmed that continuing Epic 4 on the same write path would deepen the mismatch.

Evidence:
- The PRD defines SpacetimeDB-backed operational state as baseline product behavior, not aspirational future architecture.
- The architecture document still carries an interim decision that keeps structured durable state on the bootstrap registry JSON until a later epic.
- `project-context.md` still instructs implementation work not to wire SpacetimeDB writes and to keep using `RegistryPersistence` as the durable write path.
- Story 4.2 review findings showed the interim path already creates avoidable performance and robustness pressure for per-file durable writes during recovery-oriented indexing.

Problem statement:
Mutable operational state is still implemented on an interim local-only persistence path even though the product definition already treats SpacetimeDB as authoritative for that state. This now risks building more recovery and repair behavior on the wrong storage boundary.

## 2. Impact Analysis

### Epic Impact

Epic 4 remains valid in product intent, but not in its current implementation order.

- Story 4.1 remains valid and complete.
- Story 4.2 remains valid and complete.
- The current Story 4.3 through 4.6 backlog should not proceed unchanged on top of the interim `RegistryPersistence` write path.
- Epic 4 needs an enabling story inserted immediately after 4.2 to move mutable run durability to the SpacetimeDB-backed control plane.
- Existing recovery, repair, health, history, and action-classification stories should continue after that enabling story lands.

### Story Impact

Planned story resequencing:

- New Story 4.3: Move Mutable Run Durability to the SpacetimeDB Control Plane
- Existing 4.3 becomes 4.4: Trigger Deterministic Repair for Suspect or Incomplete State
- Existing 4.4 becomes 4.5: Inspect Repository Health and Repair-Required Conditions
- Existing 4.5 becomes 4.6: Preserve Operational History for Runs, Repairs, and Integrity Events
- Existing 4.6 becomes 4.7: Classify Action-Required States and Signal the Next Safe Action

### Artifact Conflicts

Affected artifacts:

- PRD: needs a sequencing clarification so baseline success explicitly requires authoritative SpacetimeDB-backed mutable operational state, not just conceptual alignment.
- Architecture: needs the interim persistence decision corrected so mutable run durability moves to the control plane now rather than in a vague future epic.
- Project context: needs implementation rules updated so future work stops extending `RegistryPersistence` for mutable run durability.
- Epics and sprint plan: need resequencing to add the enabling migration story before further Epic 4 recovery work.

### Technical Impact

The correction affects:

- run persistence
- checkpoint persistence
- per-run durable file metadata
- idempotency records
- typed recovery metadata / abort reasoning
- resume semantics and checkpoint replay boundaries
- recovery and repair story sequencing

Expected architectural direction:

- SpacetimeDB becomes authoritative for mutable operational metadata.
- Local CAS remains authoritative for raw file bytes and other byte-sensitive large artifacts.
- `RunManager` stops depending directly on `RegistryPersistence` for mutable run durability and instead depends on a run persistence boundary backed by the control plane.
- Resume no longer infers replay boundaries from fresh filesystem rediscovery; it uses a persisted discovery manifest for the run.

## 3. Recommended Approach

Recommended path: Direct Adjustment

Do not roll back Story 4.1 or 4.2. Do not reduce MVP scope. Correct the sequencing immediately by inserting a new Epic 4 enabling story that moves mutable run durability to the SpacetimeDB-backed control plane, then continue the remaining Epic 4 stories on that corrected foundation.

Rationale:

- preserves the correctness work already completed in 4.1 and 4.2
- stops extending the wrong persistence boundary
- aligns implementation with the PRD and architectural commitments already made
- removes current recovery-related persistence debt at the architectural source instead of patching around it
- gives later Epic 4 work a stable and intended control-plane foundation

Effort estimate: Medium-High
Risk assessment: Medium
Timeline impact: Moderate resequencing inside Epic 4, but lower long-term delivery risk than continuing on the interim path

Alternatives considered:

- Potential rollback: rejected because it would discard correct work without addressing the structural mismatch.
- MVP scope review: rejected because the problem is not over-scope; it is implementation sequencing against an already-approved architecture.

## 4. Detailed Change Proposals

### 4.1 Architecture Update

Artifact: `_bmad-output/planning-artifacts/architecture.md`
Section: Interim Persistence Decision

Before:

- Structured durable state stays on the local bootstrap registry JSON until a future epic wires SpacetimeDB writes.
- `RegistryPersistence` remains the interim durable write path for runs, checkpoints, and related state.

After:

- Starting in Epic 4, mutable operational state moves to the SpacetimeDB-backed control plane.
- `index_runs`, checkpoints, per-run durable file records, idempotency records, typed recovery metadata, and related operational history stop being planned around the local registry JSON path.
- The local registry is narrowed to bootstrap and compatibility responsibilities until separately migrated.
- Resume semantics are updated to rely on persisted discovery manifests rather than live rediscovery-derived checkpoint compatibility.

Justification:

This makes the architecture document match the intended product architecture and prevents more Epic 4 work from landing on an interim persistence boundary.

### 4.2 Project Context Update

Artifact: `_bmad-output/project-context.md`
Section: Architecture Decisions / Epic 2 Persistence Architecture

Before:

- SpacetimeDB is described as target-state only and not wired for writes.
- `RegistryPersistence` is explicitly the durable path for runs, checkpoints, idempotency, and file/symbol metadata.
- Implementation guidance tells agents not to wire SpacetimeDB writes yet.

After:

- SpacetimeDB is the authoritative control-plane state for mutable operational metadata.
- The bootstrap registry is narrowed to project/workspace compatibility and migration support.
- New mutable run/checkpoint/file-record/idempotency durability work must go through the control-plane boundary rather than direct `RegistryPersistence` expansion.
- Resume-capable runs must freeze discovery through a persisted manifest.

Justification:

This updates the agent-facing implementation rules so future work stops reinforcing the architecture mismatch.

### 4.3 Epic 4 Resequencing

Artifact: `_bmad-output/planning-artifacts/epics.md`
Section: Epic 4 story sequencing

Before:

- 4.3 Trigger Deterministic Repair for Suspect or Incomplete State
- 4.4 Inspect Repository Health and Repair-Required Conditions
- 4.5 Preserve Operational History for Runs, Repairs, and Integrity Events
- 4.6 Classify Action-Required States and Signal the Next Safe Action

After:

- 4.3 Move Mutable Run Durability to the SpacetimeDB Control Plane
- 4.4 Trigger Deterministic Repair for Suspect or Incomplete State
- 4.5 Inspect Repository Health and Repair-Required Conditions
- 4.6 Preserve Operational History for Runs, Repairs, and Integrity Events
- 4.7 Classify Action-Required States and Signal the Next Safe Action

New Story 4.3 acceptance direction:

1. Run, checkpoint, durable per-run file metadata, idempotency, and typed recovery writes go through the SpacetimeDB-backed control plane.
2. Per-file durability no longer requires full registry-file rewrites to preserve the checkpoint-before-durable-state invariant.
3. Resume uses a persisted discovery manifest for replay boundaries instead of fresh rediscovery.
4. Migration or compatibility handling for older registry-backed run state is explicit and safe.

Justification:

This converts the correction into an executable backlog change and prevents additional Epic 4 stories from being implemented on the wrong persistence model.

### 4.4 PRD Clarification

Artifact: `_bmad-output/planning-artifacts/prd.md`
Section: Technical Success / Baseline sequencing clarification

Before:

- Technical success requires SpacetimeDB-backed operational state to be real, durable, and useful in practice.

After:

- Technical success still requires SpacetimeDB-backed operational state to be real, durable, and useful in practice.
- Baseline implementation must route mutable run, checkpoint, recovery, repair, and idempotency state through the authoritative control plane instead of leaving those behaviors on an interim local-only write path.
- It is an unacceptable baseline failure if recovery and operational workflows ship while SpacetimeDB authority remains mostly conceptual for that mutable operational state.

Justification:

This tightens traceability without changing product scope.

## 5. Implementation Handoff

Scope classification: Major

Reason:

This is not just a story tweak. It changes the active architectural decision, requires Epic 4 resequencing, and changes the implementation rules that future stories must follow.

Handoff recipients and responsibilities:

- Product Manager / Architect
  - approve the persistence correction as the active architectural path
  - approve PRD, architecture, and project-context updates
  - confirm the discovery-manifest and run-store direction

- Scrum Master / Product Owner
  - resequence Epic 4 in planning artifacts
  - insert the new Story 4.3 before continuing existing Epic 4 backlog
  - update sprint tracking after approval

- Development Team
  - implement the new enabling story
  - move mutable run durability off direct `RegistryPersistence`
  - preserve CAS as the raw-byte authority
  - keep compatibility and migration behavior explicit and safe

Success criteria for implementation:

- mutable operational run state is durably persisted through the SpacetimeDB-backed control plane
- `RunManager` no longer treats `RegistryPersistence` as the primary source of truth for mutable run durability
- checkpoint advancement does not outrun durable prior state
- resume boundaries are based on a persisted discovery manifest
- remaining Epic 4 recovery stories proceed on the corrected persistence foundation

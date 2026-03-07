# Sprint Change Proposal

**Date:** 2026-03-07
**Workflow:** Correct Course
**Mode:** Batch
**Status:** Approved from explicit user-directed correction request

## 1. Issue Summary

The current Epic 2 planning still had one implementation-readiness blocker after the earlier observability split. `Story 2.3` remained too large and too vague because it mixed three different concerns:

- bounded execution of the initial quality-focus indexing slice
- persistence of usable indexing outputs for that slice
- future expansion toward broader baseline language parity

That structure prevented clean acceptance testing. It also left room for a false sense of coverage because the old story used parity-direction wording without a concrete deliverable boundary.

## 2. Impact Analysis

### Epic Impact

- **Epic 2** was tightened so indexing execution, persisted outputs, and broader-language extension are now separate delivery units.
- The new shape keeps the initial indexing path implementation-sized while preserving a clear mechanism for later broader-language growth.

### Story Impact

- Kept `Story 2.1` unchanged as the durable-run entrypoint.
- Kept `Story 2.2` as bounded indexing execution for the initial quality-focus language set, but tightened its boundaries so it only claims scope for `Rust`, `Python`, `JavaScript / TypeScript`, and `Go`.
- Replaced the oversized old `Story 2.3` with:
  - `Story 2.3: Persist File-Level Indexing Outputs and Symbol/File Metadata for the Initial Quality-Focus Language Set`
  - `Story 2.4: Extend Indexing Through a Repeatable Broader-Language Onboarding Pattern`
- Renumbered downstream Epic 2 stories from `2.4-2.10` to `2.5-2.11`.

### Artifact Conflicts

- **PRD:** No direct PRD edit required. The revised Epic 2 story structure still implements `FR9` while making the delivery path more concrete.
- **Architecture:** No direct architecture edit required. The change clarifies story boundaries inside the existing indexing lifecycle rather than changing system design.
- **Sprint tracking artifact:** No `sprint-status.yaml` artifact exists in the repository, so no separate sprint-status update was applied.

### Technical Impact

- No code changes were required for this planning correction.
- Future implementation can now test execution, persistence, and language-onboarding separately.
- Broader-language support is no longer implied as one monolithic parity delivery.

## 3. Recommended Approach

**Selected path:** Direct Adjustment

This is a planning-structure correction, not a product-direction reset. The correct response is to tighten story boundaries in `epics.md`, keep the PRD and architecture intact, and remove vague parity language from acceptance criteria.

**Effort:** Low
**Risk:** Low
**Timeline impact:** Minimal planning-only change

## 4. Detailed Change Proposals

### Stories

#### Story 2.2 tightening

**OLD**

- Bounded indexing execution for the initial quality-focus language set, but with boundaries not stated clearly enough.

**NEW**

- `Story 2.2` now remains execution-only.
- The story explicitly names its scope as `Rust`, `Python`, `JavaScript / TypeScript`, and `Go`.
- Acceptance criteria now state that languages outside that set are out of scope for this story.

**Rationale**

- This keeps execution separate from persistence.
- It prevents accidental over-claiming of support during the first indexing slice.

#### Story 2.3 replacement

**OLD**

- `Story 2.3: Extend Indexing Coverage Toward Baseline Language Parity`

**NEW**

- `Story 2.3: Persist File-Level Indexing Outputs and Symbol/File Metadata for the Initial Quality-Focus Language Set`

**Rationale**

- This creates a concrete first deliverable after execution: durable usable indexing outputs.
- The story now has explicit output, failure, and support-boundary behavior.

#### Story 2.4 replacement

**OLD**

- Broad parity expansion language without a concrete implementation-sized slice.

**NEW**

- `Story 2.4: Extend Indexing Through a Repeatable Broader-Language Onboarding Pattern`

**Rationale**

- This story now defines a reusable onboarding mechanism for one explicitly named broader-language slice at a time.
- It no longer implies that all remaining parity languages ship in one story.
- It makes support boundaries explicit for onboarded versus not-yet-onboarded languages.

#### Epic 2 renumbering

**OLD**

- Downstream stories started at `2.4` immediately after the oversized parity story.

**NEW**

- Downstream stories now shift by one:
  - `2.5` Inspect Run Status and Health
  - `2.6` Observe Live or Near-Live Indexing Progress
  - `2.7` Cancel an Active Indexing Run Safely
  - `2.8` Checkpoint Long-Running Indexing Work
  - `2.9` Re-index Managed Repository or Workspace State Deterministically
  - `2.10` Invalidate Indexed State So It Is No Longer Trusted
  - `2.11` Reject Conflicting Idempotent Replays

**Rationale**

- Renumbering is required to preserve an ordered, unambiguous Epic 2 sequence after the split.

## 5. Implementation Handoff

**Scope classification:** Moderate

The change is planning-only, but it alters story boundaries and downstream numbering. It should be treated as backlog reorganization with direct implications for future story creation and readiness review.

**Route to:** Product Owner / Scrum Master workflow ownership

**Responsibilities**

- use the updated `epics.md` as the new Epic 2 source of truth
- keep implementation work for `2.2`, `2.3`, and `2.4` separated by execution, persistence, and onboarding-pattern concerns
- carry forward the new Epic 2 numbering in any future sprint tracking artifact
- rerun implementation readiness when ready if you want the blocker formally re-evaluated

**Success criteria**

- no Epic 2 story claims broad parity coverage without a concrete, testable slice
- `2.2`, `2.3`, and `2.4` each define scope, outputs, failure behavior, and support boundaries
- future story creation and implementation follow the renumbered Epic 2 sequence

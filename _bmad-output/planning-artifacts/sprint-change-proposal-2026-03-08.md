# Sprint Change Proposal

**Date:** 2026-03-08
**Workflow:** Correct Course
**Mode:** Batch
**Status:** Approved from explicit user-directed correction request

## 1. Issue Summary

The Epic 3 retrospective introduced a mandatory Epic 4.0 hardening checkpoint, but that checkpoint only existed in the retrospective artifact. The active sprint artifacts still made Epic 4 look like a normal next-story sequence starting directly at `Story 4.1`, which created a planning mismatch between the approved retro and the working backlog.

That mismatch matters because the retro made four items hard blockers before `Story 4.1` can even be created, one additional policy item required before the first Epic 4 implementation starts, and one parallel debt-reduction task that must remain visible during Epic 4 execution.

## 2. Impact Analysis

### Epic Impact

- **Epic 4** product scope stays the same.
- **Epic 4 execution sequencing** changes: a formal `Epic 4.0 Hardening Checkpoint` now precedes Story 4.1 creation.
- **Epic 5** is unaffected except that it remains downstream of the Epic 4 gate.

### Story Impact

- Kept `Stories 4.1-4.6` unchanged as the product-scope recovery and repair sequence.
- Added six backlog-tracked hardening tasks derived from the Epic 3 retro:
  - `4.0.1` Establish Epic 4 Definition of Done
  - `4.0.2` Update `project-context.md` with Epic 4 Recovery Architecture rules
  - `4.0.3` Document Epic 4 agent selection policy
  - `4.0.4` Add full-chain MCP `call_tool` integration coverage
  - `4.0.5` Record registry-read performance benchmark evidence
  - `4.0.6` Extract read-only query interface from `RegistryPersistence`
- Added an explicit rule that `Story 4.1` may not be created or moved to `ready-for-dev` until `4.0.1`, `4.0.2`, `4.0.4`, and `4.0.5` are complete and written down.

### Artifact Conflicts

- **PRD:** No direct PRD edit required. Epic 4 scope and FR mapping remain valid.
- **Architecture:** No direct architecture document edit required as part of this backlog formalization.
- **Epics backlog:** `epics.md` needed an explicit checkpoint section before the Epic 4 stories.
- **Sprint tracking:** `sprint-status.yaml` needed separate gate tracking so the retro blockers are visible without falsely presenting them as normal product stories.

### Technical Impact

- No production code changes are made by this correction.
- The backlog now explicitly tracks the protocol-boundary, benchmark, process-hardening, and supervision work that the retrospective identified as mandatory before Epic 4 story execution.
- The correction reduces the risk of creating `Story 4.1` prematurely or forgetting the hardening work during sprint flow.

## 3. Recommended Approach

**Selected path:** Direct Adjustment

This is a planning and sequencing correction, not a product-direction reset. The correct response is to preserve the Epic 4 story set, insert a formal pre-story hardening checkpoint, and update sprint tracking so the gating work is visible and enforceable.

**Effort:** Low
**Risk:** Low
**Timeline impact:** Intentional short pause before Epic 4 story creation so the hard blockers are closed first

## 4. Detailed Change Proposals

### Backlog Structure

#### Epic 4 checkpoint insertion

**OLD**

- Epic 4 flowed directly from the retrospective into `Story 4.1` with no formal checkpoint artifact in the active backlog.

**NEW**

- `epics.md` now contains `Epic 4.0 Hardening Checkpoint` before `Story 4.1`.
- The checkpoint lists six concrete hardening tasks and the exact start-gate rules for Epic 4.

**Rationale**

- This makes the approved retrospective operational instead of advisory.
- It keeps product-scope stories separate from delivery-hardening work.

### Sprint Tracking

#### Gate tracking addition

**OLD**

- `sprint-status.yaml` showed Epic 4 stories as simple backlog items with no visible prerequisite gate.

**NEW**

- `sprint-status.yaml` now includes dedicated `epic_start_gates` and `hardening_checkpoint_status` sections.
- The file explicitly records which items block `Story 4.1` creation, which item blocks the first Epic 4 implementation, and which task may proceed in parallel.

**Rationale**

- The gate is now visible in the live sprint artifact without overloading normal story-status semantics.
- Future workflows can distinguish product stories from hardening tasks cleanly.

### Epic Sequencing Clarification

#### Story 4.1 readiness rule

**OLD**

- The active backlog implied `Story 4.1` could be the next normal created story.

**NEW**

- `Story 4.1` stays the first Epic 4 product story, but it is explicitly blocked until the required hardening tasks are complete and written down.

**Rationale**

- This matches the retrospective's "momentum is not readiness" decision.
- It prevents the team from starting recovery work on top of unresolved protocol, performance, and process gaps.

## 5. Implementation Handoff

**Scope classification:** Moderate

The change does not alter product requirements, but it does reorganize pre-Epic 4 execution and adds mandatory gate tracking that the Scrum Master and Product Owner must respect.

**Route to:** Product Owner / Scrum Master workflow ownership

**Responsibilities**

- use the updated `epics.md` and `sprint-status.yaml` as the new Epic 4 source of truth
- do not create `Story 4.1` until `4.0.1`, `4.0.2`, `4.0.4`, and `4.0.5` are complete and written down
- do not start the first Epic 4 implementation until `4.0.3` is complete
- keep `4.0.6` visible as parallel debt-reduction work until it is either completed or explicitly called out in story records
- preserve `Stories 4.1-4.6` as the product-scope order once the checkpoint closes

**Success criteria**

- the active backlog shows the Epic 4.0 checkpoint explicitly
- sprint tracking exposes the same gate rules as the Epic 3 retrospective
- no workflow can reasonably interpret Epic 4 as "start Story 4.1 immediately" anymore

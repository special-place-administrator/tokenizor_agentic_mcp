# Story {{epic_num}}.{{story_num}}: {{story_title}}

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a {{role}},
I want {{action}},
so that {{benefit}}.

## Acceptance Criteria

1. [Add acceptance criteria from epics/PRD]

## Tasks / Subtasks

- [ ] Task 1 (AC: #)
  - [ ] Subtask 1.1
- [ ] Task 2 (AC: #)
  - [ ] Subtask 2.1

## Dev Notes

- Relevant architecture patterns and constraints
- Source tree components to touch
- Testing standards summary

### Epic 4 Definition of Done (mandatory)

- Expected test delta: [Declare the minimum new or updated tests before implementation starts]
- Build/test evidence: [Record the exact `cargo test` command(s) and pass/fail summary]
- Acceptance-criteria traceability: [Map each AC to the implementing function(s) and verifying test(s)]
- Trust-boundary traceability: [Map each trust or recovery decision to the exact architecture or project-context source]
- State-transition evidence: [List both mutation-side and retrieval/inspection-side proof for every state change]

### Self-Audit Checklist (mandatory before requesting review)

_Run this checklist after all tasks are complete. This is a blocking step — do not request review until every item is verified._

#### Generic Verification
- [ ] For every task marked `[x]`, cite the specific test that verifies it
- [ ] For every new error variant or branch, confirm a test exercises it
- [ ] For every computed value, trace it to where it surfaces (log, return value, persistence)
- [ ] For every test, verify the assertion can actually fail (no `assert!(true)`, no conditionals that always pass)

#### Epic 4 Recovery Verification
- [ ] The declared expected test delta was met or exceeded by the actual implementation
- [ ] Build/test evidence is recorded with the exact command and outcome summary
- [ ] Every acceptance criterion is traced to concrete implementation code and at least one concrete test
- [ ] Every trust-boundary or recovery-policy decision cites the exact architecture or `project-context.md` source
- [ ] Every state transition is tested from both sides: the mutation itself and the resulting retrieval/inspection behavior

### Project Structure Notes

- Alignment with unified project structure (paths, modules, naming)
- Detected conflicts or variances (with rationale)

### References

- Cite all technical details with source paths and sections, e.g. [Source: docs/<file>.md#Section]

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List

---
goal_id: SFB14
title: Create replayable public-contract conformance corpus
chain_id: symforge-live-code-backlog
phase: Phase 2 - public contract hardening
status: "Completed"
depends_on: ["SFB09"]
target_branch: "backlog-implementation"
prohibited_branches: ["main", "master"]
worktree_hint: ".worktrees/backlog-implementation"
created_at: "2026-05-19"
started_at: "2026-05-20T21:11:04.3463551+02:00"
completed_at: "2026-05-20T21:28:41.1956057+02:00"
completion_commit: "a247c61faa7c0ef73c0f0b25cdab52ae1419a5c9"
blocked_reason: ""
gate: "implementation-ready"
risk_level: "high"
source_refs:
  - "docs/live-code-backlog.md#11"
---
# SFB14 - Create replayable public-contract conformance corpus

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-live-code-backlog/SFB14-create-replayable-public-contract-conformance-corpus.md
```

## Goal File Workflow

0. Treat this markdown file as the whole prompt. Do not ask the user for extra instructions. If the task cannot be completed safely, mark it `Blocked` and explain exactly why in the final report.
1. Run the Branch Guard before editing this file, source code, tests, npm files, docs, generated artifacts, or Cargo metadata.
2. After Branch Guard passes, update this file's frontmatter: set `status` to `In progress` and set `started_at` to an ISO-8601 timestamp.
3. Execute only this goal's mini-spec. Keep changes inside `allowed_files_or_area`. Do not expand into adjacent backlog items unless this file explicitly says so.
4. If a stop condition is hit, stop implementation, set `status` to `Blocked`, set `blocked_reason`, leave `completion_commit` empty, and commit the status update if committing is safe.
5. When acceptance criteria pass, run the verification command exactly as written unless the command is impossible for a documented pre-existing reason.
6. Commit the verified implementation work first. Then update this file: set `status` to `Completed`, set `completed_at`, and set `completion_commit` to the exact verified work commit hash.
7. Commit the goal-status update as a separate commit.

## Branch Guard

This goal belongs only to branch `backlog-implementation`.

Before making any change, run:

```bash
git branch --show-current
git status --short
```

If the branch is `backlog-implementation`, continue only if the working tree is clean or contains only this goal's already-started changes.

If the branch is `main`, `master`, or any other branch, do not edit there. Use or create the dedicated worktree:

```bash
if [ -d ".worktrees/backlog-implementation/.git" ] || [ -f ".worktrees/backlog-implementation/.git" ]; then
  cd .worktrees/backlog-implementation
else
  git fetch origin
  git worktree add -b backlog-implementation .worktrees/backlog-implementation origin/main || git worktree add .worktrees/backlog-implementation backlog-implementation
  cd .worktrees/backlog-implementation
fi
mkdir -p .agent/goals/symforge-live-code-backlog
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-live-code-backlog/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `backlog-implementation`.

## SymForge Goal Discipline

- Work from current code, not historical plans. Do not revive deleted historical docs, ADRs, RTK plans, old reports, or planning directories.
- Do not invent unrelated product features.
- Prefer small, reviewable Rust changes with focused tests.
- Preserve existing MCP behavior, public output contracts, npm packaging, CLI flags, tool names, schemas, and daemon behavior unless this goal explicitly changes one.
- Keep SymForge local-first: in-process `LiveIndex` and `.symforge/` local state remain the source of runtime truth.
- Maintain byte-exact source handling. Do not normalize line endings, rewrite source bytes casually, or serve stale spans silently.
- Never turn mock, stale, degraded, disabled, blocked, unavailable, or unknown state into success.
- If the target is already implemented, strengthen tests/evidence instead of duplicating code.
- If a public contract changes, add tests that pin the contract and note whether npm/client setup is affected.

## Dependency Guard

If `depends_on` is not empty, inspect the referenced goal file(s) under `.agent/goals/symforge-live-code-backlog/` when present. If a dependency is absent or not marked `Completed`, continue only if the code already contains the dependency's acceptance artifacts. Otherwise mark this goal `Blocked` with evidence.


## Mini-Spec

objective:
- Add a versioned conformance corpus for public MCP contracts with canonical JSON requests, expected response class/status, expected recovery hints for invalid requests, and dry-run behavior for mutating tools.

non_goals:
- Do not rewrite all integration tests.
- Do not change public contracts except through tests that expose existing behavior.
- Do not add network-dependent fixtures.

allowed_files_or_area:
- tests/conformance.rs
- tests/**
- src/protocol/**
- src/cli/** only if harness needs binary invocation
- CHANGELOG.md only if public contract delta is intentionally recorded

forbidden_files:
- src/live_index/** except fixture setup helpers
- src/daemon.rs except daemon invocation harness if required
- npm/** unless conformance includes npm wrapper invocation
- docs/**
- plans/**
- .planning/**
- openspec/**

contracts_or_interfaces:
- Corpus is versioned and replayable against a built binary or stable in-process harness.
- Each case has request JSON, expected status/outcome class, and optional expected recovery hint.
- At least one invalid-request case asserts a specific recovery message instead of generic deserialization fallout.

invariants:
- Mutating dry-run cases must not write files.
- Corpus failures are deterministic and isolated by temp directories.
- Schema roundtrip tests continue to pass.

acceptance_criteria:
- Conformance corpus covers core read, search, edit, and dry-run cases.
- Negative malformed/unsupported cases are included.
- At least one invalid request asserts a specific recovery hint.
- Release note delta is recorded only if this goal changes a public contract.

evidence_required:
- List of conformance cases.
- Conformance test output.
- Diff stat.
- Default verification output.

stop_conditions:
- SFB09 result-status contract is absent and no stable expected response class can be asserted.
- Harness requires starting external services or network; stop and use in-process/built-binary local harness.

verification_command:

```bash
cargo fmt --check
cargo check
cargo test --all-targets -- --test-threads=1
rg "conformance|expected_status|recovery" tests src
```

Default full verification, when task-specific verification passes and time permits:

```bash
git branch --show-current
git diff --check
cargo fmt --check
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
```

If this goal changes `npm/**`, also run:

```bash
cd npm && npm test
```


reviewer_checklist:
- Gate type is `implementation-ready` and was handled honestly.
- Branch evidence shows `backlog-implementation`.
- Changes stayed inside allowed files/areas.
- Forbidden historical docs/plans were not revived.
- Public MCP, CLI, npm, daemon, and output contracts did not regress unless this goal explicitly changed and tested them.
- Verification output is included in the final report.

## Task Prompt

Run only this goal. Follow the Branch Guard, update this file before and after work, keep edits inside the allowed files/areas, satisfy the mini-spec, run verification, commit verified work, then commit the status update. Report blockers instead of guessing.

## Final Report Format

Objective:
- <repeat this goal's objective>
Gate:
- <implementation-ready | evidence-gated | decision-gated>
Changes:
- <focused list of implementation changes>
Files changed:
- <paths>
Acceptance criteria:
- PASS/FAIL: <criterion> — <evidence>
Verification:
- PASS/FAIL: `<command>` — <summary>
Evidence:
- <branch evidence, test output summaries, rg output, before/after notes, status/output examples>
Commit:
- Verified work commit: `<hash>`
Known gaps / blockers:
- <none or explicit blocker with reason>
Next goal:
- SFB15 - Calibrate guidance ranking and noise filtering for investigation_suggest, ask, and explore

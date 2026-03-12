# Discovery: Phase Plan Task Slicing

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)

Purpose:

- enforce execution slices that fit comfortably within one working context window
- avoid mid-task compaction
- keep every change set reviewable, testable, and resumable

Core rule:

- no implementation slice should be large enough that a context compaction would be expected before the slice reaches a stable checkpoint

Default slice size:

- one public tool addition or one public tool upgrade
- one internal query/storage/refactor step that clearly supports a later public tool
- one ranking/read-path improvement with focused validation

Required checkpoint at slice end:

- code compiles or the remaining blocker is explicitly documented
- tests added or updated where practical
- benchmark note captured for hot-path changes, or explicit note that benchmarking is deferred to the next slice
- next step is obvious without rereading the whole plan

Split triggers:

- more than two split plan files need to stay active in working memory
- more than one major subsystem must change together
- the task includes multiple unrelated acceptance criteria
- the task cannot be verified with a focused test or benchmark run
- the task needs both architectural design and broad implementation at once

Preferred split pattern for this project:

1. discovery note
2. research note if architecture, performance, or API tradeoffs exist
3. small implementation slice
4. focused verification
5. next slice

Examples:

- good slice: add `path_prefix` support to `search_text` input parsing, query plumbing, and one focused test set
- good slice: add line-number rendering to `get_file_content` without also doing chunking and match-context support
- too large: implement `search_files`, `resolve_path`, path ranking, repo outline cleanup, and non-code text lane support in one pass

Phase implications:

- Phase 1 should likely be split into several substrate slices rather than treated as one coding task
- Phase 2 through Phase 5 should each be executed as multiple checkpoints, not single commits worth of work
- any watcher or memory-profile change deserves its own research-backed slice

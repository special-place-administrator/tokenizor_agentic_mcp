# Discovery: Phase 0 Compatibility Thresholds

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [10-D-phase0-benchmark-scenarios.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/10-D-phase0-benchmark-scenarios.md)
- [11-D-phase0-baseline-output-snapshot-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/11-D-phase0-baseline-output-snapshot-plan.md)
- [12-D-phase0-regression-fixture-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/12-D-phase0-regression-fixture-plan.md)
- [13-T-phase0-compatibility-thresholds.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/13-T-phase0-compatibility-thresholds.md)

Goal:

- turn Phase 0 "no regressions" intent into explicit pass or fail rules for later slices
- separate thresholds already backed by tests from provisional thresholds that need the benchmark harness to supply real numbers

## Compatibility Gate

A later slice should only claim compatibility if all three gates pass on the relevant Phase 0 fixtures:

- latency stays within the allowed envelope
- output ordering and structure remain deterministic
- output usefulness stays bounded and does not collapse into emptier or noisier results

## Latency Thresholds

Absolute thresholds already present in the repo:

- `get_context_bundle` must remain under 100ms on the existing 50-file synthetic case
- existing non-query performance guards remain in force even though they are not the main Phase 0 target:
  - 70-file `LiveIndex::load` under 500ms
  - 1000-file `LiveIndex::load` under 3s

Provisional hot-path thresholds until the benchmark harness exists:

- `search_text`
- `search_symbols`
- `find_references`
- `get_file_content`
- `get_file_context`
- Phase 0 path proxy behavior

Rule for provisional thresholds:

- record before and after timings on the same fixture, same machine, and same warm-state conditions
- compatibility means median latency does not regress by more than 20 percent from the captured Phase 0 baseline
- if a slice regresses more than 20 percent but materially improves precision or output usefulness, it must be called out as a deliberate tradeoff rather than reported as "no regression"
- once the benchmark harness lands, replace the provisional entries with concrete measured baselines and keep the same 20 percent envelope unless a stricter tool-specific cap is justified

## Deterministic Behavior Thresholds

Exact-match requirements after the normalization rules from task 11:

- all must-snapshot outputs keep exact text parity after temp-root, separator, and line-ending normalization
- `search_symbols` preserves exact, prefix, and substring tier order
- `find_references` preserves grouped-by-file ordering and enclosing-symbol annotations when available
- `get_context_bundle` preserves section order and keeps its caller-list cap
- `get_file_context` preserves header plus `Key references` section shape
- `get_file_content` preserves full-file and line-range slice semantics exactly as currently emitted

Fail conditions:

- harness-side sorting or filtering is required to make output "look stable"
- a tool now emits different ordering without an intentional contract update
- a previously stable section disappears, duplicates, or changes meaning silently

## Bounded Output And Usefulness Thresholds

These are compatibility failures even if latency still looks acceptable:

- a ready-index fixture that previously returned meaningful output now returns an empty or guard-style response
- noisy fixtures materially expand in output volume without a documented precision gain
- current compactness controls disappear, such as the caller cap in `get_context_bundle`
- a later slice bakes future formatting into Phase 0 comparison points, for example by silently changing `get_file_content` semantics before that phase is scheduled

Compatibility-friendly output changes:

- fewer results are acceptable when the change clearly improves relevance and the snapshot or benchmark note explains the precision win
- additive metadata is acceptable only when it stays compact and does not break the snapshot contract unexpectedly

Special case:

- the Phase 0 path proxy baseline is informative, but it is not yet a long-lived public contract because Phase 2 is expected to add dedicated path tools
- path-proxy comparisons should therefore block obvious regressions, but they should not prevent the planned path-surface redesign

## Evidence Required For Later Slices

Before a slice claims "compatible":

- benchmark evidence for every hot-path query it touched
- snapshot evidence for every output fixture it touched
- a note saying whether each threshold used was absolute or still provisional
- an explanation for any result that is slower but allegedly higher quality

Before a slice claims "improved":

- before and after timings from the same fixture and machine conditions
- before and after output comparison showing the improvement is not just a format drift
- explicit statement that deterministic ordering still holds

Review shorthand for future handoffs:

- `PASS`: within latency envelope, snapshots stable, usefulness preserved
- `PASS-WITH-TRADEOFF`: latency or verbosity changed, but the gain is explicit and justified
- `OPEN`: baseline missing or measurement method changed
- `FAIL`: compatibility threshold broken without an approved contract change

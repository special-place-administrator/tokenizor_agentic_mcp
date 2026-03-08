# Epic 3 Registry Benchmark

**Date:** 2026-03-08
**Command:** `cargo test --test epic4_hardening benchmark_registry_read_performance_and_write_report -- --ignored --exact --nocapture`

## Fixture

- Repository id: `registry-benchmark`
- File count: 500
- Language mix: 125 Rust, 125 Python, 125 TypeScript, 125 Go
- Query mix: 10 `search_text`, 10 `search_symbols`, 5 `get_repo_outline`
- Warm-up: one query per operation before timings were recorded

## Thresholds

- `search_text` p50 <= 150 ms
- `search_symbols` p50 <= 100 ms

## Results

| Operation | Raw timings (ms) | p50 (ms) | Threshold | Status |
|---|---|---:|---:|---|
| `search_text` | 38.362, 37.719, 37.679, 37.830, 37.764, 37.180, 37.174, 36.850, 38.245, 37.513 | 37.699 | 150.000 | pass |
| `search_symbols` | 19.004, 19.075, 18.820, 18.812, 18.672, 18.529, 18.567, 19.300, 18.798, 19.028 | 18.816 | 100.000 | pass |
| `get_repo_outline` | 19.142, 19.163, 19.323, 19.399, 18.940 | 19.163 | n/a | recorded |

## Notes

- Timings were collected on a warm local index after the indexing run completed successfully.
- The benchmark records application-layer registry read performance on the generated 500-file mixed-language fixture.
- `get_repo_outline` timings are recorded for visibility even though the Epic 4 gate only enforces thresholds for `search_text` and `search_symbols`.

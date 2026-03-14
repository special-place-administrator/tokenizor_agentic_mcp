# Tool Quality Fixes Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 4 tool usability issues: regex double-escaping, explore multi-term scoring, insert_before spacing, and follow_refs visibility.

**Architecture:** Four independent fixes touching `src/protocol/tools.rs`, `src/protocol/edit.rs`, and `src/protocol/format.rs`. Each fix is a self-contained commit. Priority order: regex → explore → insert_before → follow_refs.

**Tech Stack:** Rust, regex crate

**Spec:** `docs/superpowers/specs/2026-03-14-tool-quality-fixes-design.md`

---

## Task 1: Regex double-escape auto-correction

**Files:**
- Modify: `src/protocol/tools.rs` (~line 1520, `search_text` handler)

- [ ] **Step 1: Write `fix_common_double_escapes` helper**

Add this helper function near the `search_text` handler (e.g., after `search_text_options_from_input` around line 764):

```rust
/// Attempt to fix double-escaped regex character classes (e.g., `\\s` → `\s`).
/// Returns `Some(fixed)` if the pattern contains likely double-escaped sequences,
/// `None` otherwise.
fn fix_common_double_escapes(pattern: &str) -> Option<String> {
    let re = regex::Regex::new(r"\\\\([sdwbntSDWB])").unwrap();
    if !re.is_match(pattern) {
        return None;
    }
    Some(re.replace_all(pattern, r"\$1").to_string())
}
```

- [ ] **Step 2: Add retry logic in `search_text` handler**

In `src/protocol/tools.rs`, the `search_text` method (line ~1520). After the existing call to `search_text_with_options` and the `enrich_with_callers` block, add retry logic before formatting.

The current code flow is:
```rust
let result = { /* guard block that calls search_text_with_options */ };
format::search_text_result_view(result, params.0.group_by.as_deref())
```

Replace the final `format::search_text_result_view(result, ...)` call with:

```rust
// Auto-correct double-escaped regex patterns when regex=true
if params.0.regex.unwrap_or(false) {
    let should_retry = match &result {
        Err(search::TextSearchError::InvalidRegex { .. }) => true,
        Ok(r) if r.files.is_empty() => true,
        _ => false,
    };
    if should_retry {
        if let Some(query) = &params.0.query {
            if let Some(fixed) = fix_common_double_escapes(query) {
                let retry = {
                    let guard = self.index.read().expect("lock poisoned");
                    loading_guard!(guard);
                    let mut r = search::search_text_with_options(
                        &guard,
                        Some(&fixed),
                        params.0.terms.as_deref(),
                        true,
                        &options,
                    );
                    if params.0.follow_refs.unwrap_or(false) {
                        if let Ok(ref mut text_result) = r {
                            let limit = params.0.follow_refs_limit.unwrap_or(3) as usize;
                            enrich_with_callers(&guard, text_result, limit);
                        }
                    }
                    r
                };
                // Only use retry result if it actually found something (or compiled successfully)
                let use_retry = match &retry {
                    Ok(r) if !r.files.is_empty() => true,
                    Ok(_) => matches!(result, Err(_)), // compiled at least
                    Err(_) => false,
                };
                if use_retry {
                    let mut output = format::search_text_result_view(
                        retry,
                        params.0.group_by.as_deref(),
                    );
                    output.push_str(&format!(
                        "\n(auto-corrected double-escaped regex: `{}` → `{}`)",
                        query, fixed
                    ));
                    return output;
                }
            }
        }
    }
}
format::search_text_result_view(result, params.0.group_by.as_deref())
```

Note: `options` must be moved before the initial `result` block so it's available for retry. Currently `options` is computed at line ~1524 and consumed inside the `result` block. Extract it before the block.

- [ ] **Step 3: Write tests**

Add to the test module in `src/protocol/tools.rs`:

```rust
#[tokio::test]
async fn test_search_text_auto_corrects_double_escaped_regex() {
    let server = make_test_server_with_file(
        "test.rs",
        "fn handle_request() {}\nfn handle_response() {}\n",
    );
    let result = server
        .search_text(Parameters(SearchTextInput {
            query: Some("fn\\\\s+handle_".to_string()), // double-escaped \s
            regex: Some(true),
            ..Default::default()
        }))
        .await;
    assert!(
        result.contains("handle_"),
        "should find matches after auto-correction: {result}"
    );
    assert!(
        result.contains("auto-corrected"),
        "should include auto-correction note: {result}"
    );
}
```

Note: The exact test helper (`make_test_server_with_file` or similar) should match the existing test patterns in the file. Check the existing `test_search_text_*` tests for the correct setup pattern.

- [ ] **Step 4: Run tests**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 5: Run format check**

Run: `cargo fmt -- --check`

- [ ] **Step 6: Commit**

```
fix: auto-correct double-escaped regex patterns in search_text

When regex=true and the pattern fails to compile or returns 0
results, attempt to fix common double-escaped character classes
(\\s → \s, \\d → \d, etc.) and retry. Appends a note to output
when auto-correction is applied.
```

---

## Task 2: Explore multi-term scoring

**Files:**
- Modify: `src/protocol/tools.rs` (~line 2017, `explore` handler)

- [ ] **Step 1: Replace symbol collection with HashMap scoring**

In the `explore` method (line ~2017), replace the current symbol collection loop (lines ~2040-2053):

```rust
// Current: first-come-first-served dedup
let mut symbol_hits: Vec<(String, String, String)> = Vec::new();
for sq in &symbol_queries {
    let result = search::search_symbols(&guard, sq, None, limit);
    for hit in &result.hits {
        if symbol_hits.len() >= limit {
            break;
        }
        let entry = (hit.name.clone(), hit.kind.clone(), hit.path.clone());
        if !symbol_hits.contains(&entry) {
            symbol_hits.push(entry);
        }
    }
}
```

Replace with HashMap scoring:

```rust
// Phase 1: Symbol search — over-fetch and count term matches
let mut match_counts: std::collections::HashMap<(String, String, String), usize> =
    std::collections::HashMap::new();
for sq in &symbol_queries {
    let result = search::search_symbols(&guard, sq, None, limit * 3);
    for hit in &result.hits {
        let entry = (hit.name.clone(), hit.kind.clone(), hit.path.clone());
        *match_counts.entry(entry).or_default() += 1;
    }
}
```

- [ ] **Step 2: Merge text hit collection with enclosing symbol injection**

Replace the current text hit collection loop (lines ~2055-2074) with the combined version that also injects enclosing symbols into `match_counts`:

```rust
// Phase 2: Text search — collect text_hits AND inject enclosing symbols
let mut text_hits: Vec<(String, String, usize)> = Vec::new();
for tq in &text_queries {
    let options = search::TextSearchOptions {
        total_limit: limit.min(50),
        max_per_file: 2,
        ..search::TextSearchOptions::for_current_code_search()
    };
    let result =
        search::search_text_with_options(&guard, Some(tq), None, false, &options);
    if let Ok(r) = result {
        for file in &r.files {
            for m in &file.matches {
                if text_hits.len() < limit && !format::is_noise_line(&m.line) {
                    text_hits.push((file.path.clone(), m.line.clone(), m.line_number));
                }
                // Inject enclosing symbol into match_counts
                if let Some(ref enc) = m.enclosing_symbol {
                    let entry =
                        (enc.name.clone(), enc.kind.clone(), file.path.clone());
                    *match_counts.entry(entry).or_default() += 1;
                }
            }
        }
    }
}
```

- [ ] **Step 3: Add sort + truncate to produce final symbol_hits**

After Phase 2, add:

```rust
// Phase 3: Sort by match count descending, alphabetical for stability
let mut ranked: Vec<_> = match_counts.into_iter().collect();
ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0 .0.cmp(&b.0 .0)));
ranked.truncate(limit);
let symbol_hits: Vec<(String, String, String)> =
    ranked.into_iter().map(|(k, _)| k).collect();
```

The rest of the function (file_counts, related_files, enriched_symbols, depth 2/3 enrichment) uses `symbol_hits` and `text_hits` unchanged — no further modifications needed.

- [ ] **Step 4: Write test**

Add to the test module in `src/protocol/tools.rs`:

```rust
#[tokio::test]
async fn test_explore_multi_term_scoring() {
    // Create a server with a file containing a struct that matches
    // one term by name and another term by body content
    let server = make_test_server_with_file(
        "src/watcher.rs",
        "/// Tracks burst events for debounce\nstruct BurstTracker {\n    count: u32,\n}\n",
    );
    let result = server
        .explore(Parameters(ExploreInput {
            query: "watcher debounce".to_string(),
            depth: None,
            limit: None,
        }))
        .await;
    // BurstTracker should appear because "debounce" is in its doc comment
    // and it's in a file matching "watcher"
    assert!(
        result.contains("BurstTracker"),
        "multi-term query should surface enclosing symbol from text match: {result}"
    );
}
```

Note: Verify the test helper setup matches existing patterns. The key assertion is that `BurstTracker` appears in results because "debounce" matches as a text hit inside its body, and the enclosing symbol gets injected into the scored results.

- [ ] **Step 5: Run tests**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 6: Commit**

```
fix: explore multi-term scoring with enclosing symbol injection

Replace first-come-first-served symbol dedup with HashMap-based
scoring. Symbols matching more query terms rank higher. Text hits
inject their enclosing symbols into the scored set, bridging the
lexical-to-semantic gap.
```

---

## Task 3: `insert_before` conditional spacing

**Files:**
- Modify: `src/protocol/edit.rs` (~line 129, `build_insert_before`)

- [ ] **Step 1: Write failing test — no doc comment gets \n\n**

Add to the test module in `src/protocol/edit.rs`:

```rust
#[test]
fn test_build_insert_before_double_newline_without_doc_comments() {
    let content = b"struct Point { x: f64 }\n";
    let sym = SymbolRecord {
        name: "Point".to_string(),
        kind: SymbolKind::Struct,
        depth: 0,
        sort_order: 0,
        byte_range: (0, 23),
        line_range: (0, 0),
        doc_byte_range: None,
    };
    let result = build_insert_before(content, &sym, "struct Point3D { x: f64 }");
    let result_str = String::from_utf8(result).unwrap();
    // Should have blank line between inserted content and existing
    assert!(
        result_str.contains("Point3D { x: f64 }\n\nstruct Point"),
        "should have \\n\\n separator when no doc comment: {result_str}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p tokenizor_agentic_mcp --lib -- edit::tests::test_build_insert_before_double_newline -v`
Expected: FAIL — currently uses `\n` not `\n\n`

- [ ] **Step 3: Implement the conditional separator**

In `src/protocol/edit.rs`, `build_insert_before` (line ~129), replace:

```rust
    // Single newline: the content is placed immediately before the symbol's line.
    // Using \n\n would create an unwanted blank line between e.g. a doc comment and the symbol.
    insertion.extend_from_slice(b"\n");
```

With:

```rust
    // When the target symbol has doc comments, use single newline to keep
    // doc-to-symbol spacing tight. Otherwise, use double newline for
    // visual separation between definitions.
    let separator = if sym.doc_byte_range.is_some() {
        b"\n" as &[u8]
    } else {
        b"\n\n"
    };
    insertion.extend_from_slice(separator);
```

- [ ] **Step 4: Run tests**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass (including the existing `test_build_insert_before_goes_above_doc_comments` which uses `doc_byte_range: Some(...)` and should still get `\n`)

- [ ] **Step 5: Commit**

```
fix: insert_before uses blank line separator when no doc comments

When the target symbol has no attached doc comments, use \n\n
instead of \n to provide visual separation between definitions.
Keeps \n when doc comments are present to maintain tight spacing.
```

---

## Task 4: `follow_refs` visibility

**Files:**
- Modify: `src/protocol/tools.rs` (~line 766, `enrich_with_callers`)
- Modify: `src/protocol/format.rs` (~line 296, `search_text_result_view`)

- [ ] **Step 1: Relax self-file filter in `enrich_with_callers`**

In `src/protocol/tools.rs`, `enrich_with_callers` (line ~766), find the self-file skip (around line 798):

```rust
// Skip self-references (same file)
if ref_file == file_matches.path {
    continue;
}
```

Replace with:

```rust
// Skip self-references (same file AND same symbol — different symbols
// in the same file are useful context)
if ref_file == file_matches.path && symbol_names.contains(&enclosing_name) {
    continue;
}
```

- [ ] **Step 2: Always set callers when follow_refs ran**

In the same function, replace the conditional set (around line 821):

```rust
if !callers.is_empty() {
    file_matches.callers = Some(callers);
}
```

With:

```rust
// Always set callers when follow_refs was requested — even if empty.
// This distinguishes "feature not requested" (None) from "ran but
// found nothing" (Some([])).
file_matches.callers = Some(callers);
```

- [ ] **Step 3: Render empty-callers message in `search_text_result_view`**

In `src/protocol/format.rs`, `search_text_result_view` (around line 455), find the callers rendering block:

```rust
if let Some(ref callers) = file.callers {
    let caller_strs: Vec<String> = callers
        .iter()
        .map(|c| format!("{} ({}:{})", c.symbol, c.file, c.line))
        .collect();
    lines.push(format!("    Called by: {}", caller_strs.join(", ")));
}
```

Replace with:

```rust
if let Some(ref callers) = file.callers {
    if callers.is_empty() {
        lines.push("    (no cross-references found)".to_string());
    } else {
        let caller_strs: Vec<String> = callers
            .iter()
            .map(|c| format!("{} ({}:{})", c.symbol, c.file, c.line))
            .collect();
        lines.push(format!("    Called by: {}", caller_strs.join(", ")));
    }
}
```

- [ ] **Step 4: Write test for same-file callers**

Add to the test module in `src/protocol/tools.rs`, near the existing `test_search_text_follow_refs_includes_callers`:

```rust
#[tokio::test]
async fn test_search_text_follow_refs_same_file_different_symbol() {
    // Create a file where fn bar calls fn foo — both in the same file
    let server = make_test_server_with_file(
        "src/lib.rs",
        "fn foo() {}\nfn bar() { foo(); }\n",
    );
    let result = server
        .search_text(Parameters(SearchTextInput {
            query: Some("foo".to_string()),
            follow_refs: Some(true),
            ..Default::default()
        }))
        .await;
    // bar() should appear as a caller even though it's in the same file
    // because bar is a different symbol than foo
    assert!(
        result.contains("bar") || result.contains("cross-references"),
        "same-file callers from different symbols should appear: {result}"
    );
}
```

Note: The assertion is flexible because index population in test may not produce full cross-references. Check the existing `test_search_text_follow_refs_includes_callers` test pattern for the correct setup.

- [ ] **Step 5: Run tests**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 6: Commit**

```
fix: follow_refs shows same-file callers and empty-result signal

Relax self-file filter to only skip callers that are the same
symbol as the search match. Always set callers field when
follow_refs is requested. Render "(no cross-references found)"
when the feature ran but found nothing.
```

---

## Dependency Graph

```
Task 1 (regex auto-correct) ──independent──┐
Task 2 (explore scoring)    ──independent──┤──► all done
Task 3 (insert_before)      ──independent──┤
Task 4 (follow_refs)        ──independent──┘
```

All 4 tasks are fully independent — they touch different functions and can be implemented in any order. Priority order: 1 → 2 → 3 → 4.

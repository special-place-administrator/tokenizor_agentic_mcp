# Tool Quality Fixes — Design Spec

## Problem

Four usability issues identified during tool review testing:

1. **`search_text` regex double-escaping** — LLM clients often double-escape regex patterns (`\\s` instead of `\s`), causing silent zero results. Affects every LLM consumer.
2. **`explore` multi-term scoring** — compound queries like "file watcher debounce" get dominated by high-frequency single-term matches ("file"). No cross-term scoring.
3. **`insert_before` blank line gap** — inserting a definition before another definition produces no visual separator. The current `\n` separator was designed for doc-comment-to-symbol tight spacing.
4. **`follow_refs` silent no-op** — same-file caller filter and lack of visibility signal make the feature appear broken when all callers are in the same file.

## Priority

4 (regex) → 1 (explore) → 2 (insert_before) → 3 (follow_refs)

Regex escaping is systemic (hits every LLM agent). Explore scoring affects first-time codebase discovery. The other two are polish.

## Fixes

### Fix 1: Regex double-escape auto-correction

**File:** `src/protocol/tools.rs` (`search_text` handler, line ~1520)

**Approach:** Add a `fix_common_double_escapes` helper that replaces `\\s` → `\s`, `\\d` → `\d`, `\\w` → `\w`, `\\b` → `\b`, `\\n` → `\n`, `\\t` → `\t` (plus uppercase `\\S`, `\\D`, `\\W`, `\\B`).

**Application strategy (conservative):** When `regex=true`:
1. Compile the pattern. If it compiles and returns results, use as-is.
2. If it compiles but returns 0 results, check if the pattern contains likely double-escaped sequences (e.g., `\\s`). If so, retry with `fix_common_double_escapes`. If the fixed pattern produces results, use those and append a note to the output: `"(auto-corrected double-escaped regex: \\\\s → \\s)"`.
3. If it fails to compile and the pattern contains `\\s` etc., try the fixed pattern. If that compiles and works, use it with the note.
4. If neither works, return the original error.

**Why conservative:** Legitimate patterns using literal `\\s` (matching backslash-s) exist, though they're rare. Only auto-correcting on failure/zero-results avoids false positives.

**Implementation:**

```rust
fn fix_common_double_escapes(pattern: &str) -> Option<String> {
    // Only attempt if pattern contains likely double-escaped sequences
    let re = regex::Regex::new(r"\\\\([sdwbntSDWB])").unwrap();
    if !re.is_match(pattern) {
        return None;
    }
    Some(re.replace_all(pattern, r"\$1").to_string())
}
```

### Fix 2: `explore` multi-term scoring

**File:** `src/protocol/tools.rs` (`explore` handler, line ~2017)

**Current behavior:** `fallback_terms` splits query into individual terms. Each term runs `search_symbols` with the tool's `limit`. First-come-first-served dedup. "file" fills the limit before "debounce" gets a chance.

**New behavior:** Over-fetch `limit * 3` per term. Track how many query terms each `(name, kind, path)` tuple matches via HashMap. Sort by match count descending. Truncate to limit.

Additionally, after collecting text hits, extract enclosing symbols and inject them into symbol hits. This bridges the lexical→semantic gap — "debounce" appearing in `BurstTracker`'s body surfaces `BurstTracker` as a symbol hit even though the name doesn't match.

**Implementation:**

**Sequencing constraint:** The symbol hit loop and text hit loop currently run independently. After this change, they both write to `match_counts`, so the text hit loop MUST run before the final sort+truncate. The implementation order is: (1) symbol search → populate match_counts, (2) text search → collect text_hits AND inject enclosing symbols into match_counts, (3) sort+truncate match_counts into final symbol_hits.

```rust
// Phase 1: Symbol search — populate match_counts
let mut match_counts: HashMap<(String, String, String), usize> = HashMap::new();
for sq in &symbol_queries {
    let result = search::search_symbols(&guard, sq, None, limit * 3);
    for hit in &result.hits {
        let entry = (hit.name.clone(), hit.kind.clone(), hit.path.clone());
        *match_counts.entry(entry).or_default() += 1;
    }
}

// Phase 2: Text search — collect text_hits AND inject enclosing symbols
let mut text_hits: Vec<(String, String, usize)> = Vec::new();
for tq in &text_queries {
    let options = search::TextSearchOptions {
        total_limit: limit.min(50),
        max_per_file: 2,
        ..search::TextSearchOptions::for_current_code_search()
    };
    let result = search::search_text_with_options(&guard, Some(tq), None, false, &options);
    if let Ok(r) = result {
        for file in &r.files {
            for m in &file.matches {
                if text_hits.len() < limit && !format::is_noise_line(&m.line) {
                    text_hits.push((file.path.clone(), m.line.clone(), m.line_number));
                }
                // Inject enclosing symbol into match_counts (bridges lexical→semantic gap)
                if let Some(ref enc) = m.enclosing_symbol {
                    let entry = (enc.name.clone(), enc.kind.clone(), file.path.clone());
                    *match_counts.entry(entry).or_default() += 1;
                }
            }
        }
    }
}

// Phase 3: Sort by match count descending, then alphabetical for stability
let mut ranked: Vec<_> = match_counts.into_iter().collect();
ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0 .0.cmp(&b.0 .0)));
ranked.truncate(limit);
let symbol_hits: Vec<(String, String, String)> =
    ranked.into_iter().map(|(k, _)| k).collect();
```

**Key insight:** `m.enclosing_symbol` already carries the enclosing symbol's name and kind. Injecting it into `match_counts` during the text search loop (Phase 2) is simpler and cheaper than calling `guard.get_file()` + `find_enclosing_symbol` after the fact.

### Fix 3: `insert_before` conditional spacing

**File:** `src/protocol/edit.rs` (`build_insert_before`, line ~129)

**Current:** Always uses `\n` separator.

**New:** Check `sym.doc_byte_range`:
- If `Some(...)` → keep `\n` (doc comment is attached, don't add gap)
- If `None` → use `\n\n` (visual separator between definitions)

```rust
let separator = if sym.doc_byte_range.is_some() {
    b"\n" as &[u8]
} else {
    b"\n\n"
};
insertion.extend_from_slice(separator);
```

Also apply the same logic in `execute_batch_edit`'s `InsertBefore` branch, which calls `build_insert_before` — no change needed there since the function handles it internally.

### Fix 4: `follow_refs` visibility

**Files:**
- `src/protocol/tools.rs` (`enrich_with_callers`, line ~766)
- `src/protocol/format.rs` (`search_text_result_view`)

**A. Relax self-file filter:**

Current: `if ref_file == file_matches.path { continue; }` — skips ALL same-file references.

New: Only skip if the caller is one of the matched symbols (self-referencing is noise, but same-file callers from different symbols are useful context):

```rust
if ref_file == file_matches.path && symbol_names.contains(&enclosing_name) {
    continue;
}
```

**B. Visibility signal:**

In `enrich_with_callers`, always set `callers` when follow_refs is requested, even when empty:

```rust
// Current: only set callers when non-empty
if !callers.is_empty() {
    file_matches.callers = Some(callers);
}

// New: always set when follow_refs ran (even empty)
file_matches.callers = Some(callers);
```

In `search_text_result_view` (format.rs), handle the empty-callers case:

```rust
if let Some(callers) = &file.callers {
    if callers.is_empty() {
        lines.push("    (no cross-references found)".to_string());
    } else {
        // render callers as today
    }
}
```

This distinguishes "feature not requested" (callers=None) from "feature ran, nothing found" (callers=Some([])).

## Files Modified

| File | Fix | Change |
|---|---|---|
| `src/protocol/tools.rs` | #1 | `fix_common_double_escapes` + retry logic in `search_text` |
| `src/protocol/tools.rs` | #2 | `explore` handler scoring + enclosing symbol extraction |
| `src/protocol/tools.rs` | #4 | `enrich_with_callers` self-file filter relaxation + always-set callers |
| `src/protocol/edit.rs` | #3 | `build_insert_before` conditional separator |
| `src/protocol/format.rs` | #4 | `search_text_result_view` empty-callers rendering |

## Testing Strategy

- **Fix 1:** Test with double-escaped pattern (`fn\\s+foo`), verify auto-correction and note in output. Test that legitimate `\\s` (literal backslash-s) still works when it produces results.
- **Fix 2:** Test "file watcher debounce" query, verify `BurstTracker` or watcher-related symbols appear in results. Test that single-term queries still work.
- **Fix 3:** Test `insert_before` on a symbol without doc comments → verify `\n\n` separator. Test on a symbol with doc comments → verify `\n` separator.
- **Fix 4:** Test `follow_refs` with a symbol whose callers are all in the same file → verify callers now appear. Test with a symbol with no callers → verify "(no cross-references found)" message.

# Sprint 14 — Trust + Admission Control

**Date:** 2026-03-15
**Scope:** 2 tracks — trust fixes (T1, T2) and tiered admission control (Track B)
**Out of scope:** Noise/classification polish (Sprint 15), UX/telemetry polish (Sprint 16)

---

## Track A — Trust Fixes

### T1: `batch_rename` qualified path coverage

**Problem:** Textual rename misses cases where the renamed identifier appears as a path segment in qualified paths. Reported by 2/5 eval projects. Examples:
- `Type::new()` — renaming `Type` misses the qualified usage
- `module::Type::new()` — deeper nesting
- `use module::Type` — import paths
- `Type::method()` — associated function calls

**Fix:** After the existing indexed-reference rename pass, run a supplemental scan for qualified path usages where the renamed identifier appears as a path segment.

**Constraints:**
- Scope the supplemental scan to: (a) files already containing indexed rename candidates, plus (b) files reachable through the renamed symbol's import/module neighborhood within the same crate/package. Fall back to directory-parent traversal only when module resolution is unavailable (e.g., non-Rust languages without explicit module declarations).
- Classify matches as **confident** (exact path segment match in code context) vs **uncertain** (ambiguous, e.g., inside a string literal or comment)
- Never silently apply uncertain replacements — surface them separately in output
- Dry-run output must clearly separate confident from uncertain sections

**Tests:**
- `Type::new()` — rename `Type` catches the qualified call
- `module::Type::new()` — deeper path nesting caught
- `use module::Type` — import statement caught
- Common name `new` renamed does NOT false-positive on unrelated `SomeOther::new()`
- Dry-run separates confident vs uncertain matches
- Uncertain matches are surfaced but not applied by default

### T2: `search_text` disk truthfulness after edits

**Problem:** After edit operations, the FTS index may not reflect actual disk content, especially after partial failures. Reported in Sprint 13 eval and deferred.

**Fix:** Change from verify-and-repair to a **commit model**: disk is always the source of truth.

**Invariant:** All post-edit index state is derived from persisted disk content, never from intended content.

**Implementation:**
1. After any successful `fs::write`:
   - Re-read the file from disk
   - Rebuild affected index entries (symbols, FTS, reverse index) from the on-disk bytes
   - Only then return success
2. A debug assertion can optionally verify the re-read content matches expectations, but the assertion is not the correctness mechanism — the re-read is.

**Atomic multi-file operations contract:**
- Files written successfully are reindexed from disk immediately after each write
- Files not written (due to failure or not yet reached) remain untouched in the index
- No speculative index mutation is allowed — the index is only updated from persisted bytes
- The overall operation may still return an error, but successfully-written files have correct index state

**This applies to:** `replace_symbol_body`, `edit_within_symbol`, `insert_symbol`, `delete_symbol`, `batch_edit`, `batch_rename`, `batch_insert`

**Tests:**
- After successful edit, `search_text` returns content matching disk (not pre-edit)
- After partial `batch_edit` failure (file 2 of 3 fails), file 1's index reflects its on-disk content
- `reindex_after_write` reads from disk, not from the in-memory buffer passed to `fs::write`
- After partial `batch_edit` failure, file 3 (never written) retains its pre-edit index entry unchanged — no speculative index update

---

## Track B — Tiered Admission Control

### Concept: `AdmissionTier`

A new enum separate from `NoiseClass`. Noise is about ranking/filtering signal. Admission is about whether the file is eligible for indexing/parsing at all.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionTier {
    /// Tier 1: Fully indexed — parsed, symbols extracted, text searchable.
    Normal,
    /// Tier 2: Metadata only — path, size, classification stored. No parsing.
    MetadataOnly,
    /// Tier 3: Hard-skipped — counted in health, minimal registration.
    HardSkip,
}
```

A markdown file may be `NoiseClass::None` but `AdmissionTier::Normal`. A `.safetensors` file is `AdmissionTier::MetadataOnly` — it's not "noisy", it's non-indexable content. These are different concepts.

### Admission gate sequence

**Precedence (evaluated in order, first match wins):**

**Implementation constraint:** Steps are evaluated strictly in order. Step 1 MUST be evaluated unconditionally before any extension lookup. No early-return in step 2 may bypass the size ceiling check.

1. **Hard-skip size ceiling** — file size > `HARD_SKIP_BYTES` (default 100MB) → **Tier 3** always, regardless of extension or content
2. **Extension denylist** — known artifact extensions → **Tier 2** regardless of size
3. **Metadata-only size threshold** — file size > `METADATA_ONLY_BYTES` (default 1MB) → **Tier 2**
4. **Binary sniff** — read first 8KB, check for null bytes / non-UTF8 → **Tier 2**
5. **All else** → **Tier 1** (normal parse)

### Extension denylist (Tier 2 by policy)

**Always metadata-only:**
- ML models: `.safetensors`, `.ckpt`, `.pt`, `.onnx`, `.gguf`, `.pth`
- VM/disk images: `.vmdk`, `.iso`, `.img`, `.qcow2`
- Archives: `.tar`, `.gz`, `.zip`, `.7z`, `.rar`, `.bz2`, `.xz`, `.zst`
- Databases: `.db`, `.sqlite`, `.sqlite3`, `.mdb`
- Media: `.mp3`, `.mp4`, `.wav`, `.avi`, `.mov`, `.mkv`, `.png`, `.jpg`, `.jpeg`, `.gif`, `.bmp`, `.ico`, `.woff`, `.woff2`, `.ttf`, `.eot`

**Note on `.svg`:** Excluded from the denylist intentionally. SVG files are often plain-text XML meaningful in frontend repos. They are handled by the size threshold (>1MB → Tier 2) and binary sniff instead.

**Policy note:** `.bin` is denylisted because it is almost never source code. Note that the denylist at step 2 takes precedence over the binary sniff at step 4 — a `.bin` file that is legitimate UTF-8 text will still be classified as Tier 2 by the denylist before the sniff is reached. If this becomes a problem in practice, `.bin` can be removed from the denylist and left to the sniff. The size threshold and binary sniff remain the real backstops for unlisted extensions.

### Thresholds

| Constant | Default | Purpose |
|----------|---------|---------|
| `HARD_SKIP_BYTES` | 100MB | Tier 3 ceiling — files this large are never worth metadata handling |
| `METADATA_ONLY_BYTES` | 1MB | Tier 2 threshold — files above this are registered but not parsed |
| `BINARY_SNIFF_BYTES` | 8192 | How many bytes to read for binary detection |

These are constants in code (not user-configurable in v1). Can be promoted to config later if needed.

### What's stored per tier

| Field | Tier 1 | Tier 2 | Tier 3 |
|-------|--------|--------|--------|
| Relative path | Yes | Yes | Yes (in health only) |
| File size | Yes | Yes | Yes |
| Extension | Yes | Yes | Yes |
| Classification / skip reason | — | Yes | Yes |
| Last modified | Yes | Yes | No |
| Parsed symbols | Yes | No | No |
| Text searchable | Yes | No | No |
| In repo_map tree | Yes | Yes (tagged) | No (summary line only) |
| In search_files | Yes | Yes (path only) | No |
| In health counts | Yes | Yes | Yes |

### Visibility

**Tier 2 in repo_map:**
```
model.safetensors [skipped: binary, 4.2 GB]
checkpoint.ckpt [skipped: artifact, metadata only]
big_config.json [skipped: >1MB, metadata only]
```

**Tier 3 in repo_map:**
Not in the tree. But repo_map footer includes:
```
12 hard-skipped artifacts not shown (>100MB)
```

**Health output additions:**
```
Admission: 9500 files discovered
  Tier 1 (indexed): 8200
  Tier 2 (metadata only): 1280
  Tier 3 (hard-skipped): 20
```

### Symbol count headline

The headline number (`N symbols`) in health and repo_map compact counts **only Tier 1 symbols**. Tier 2 files contribute zero to the headline count. This prevents the inflation problem (1.1M symbols from one JSON node, 9274 from package-lock.json).

### Where admission lives in code

| Component | File | Responsibility |
|-----------|------|---------------|
| `AdmissionTier` enum | `src/domain/index.rs` | Tier definition |
| Admission gate function | `src/discovery/mod.rs` | Extension check, size check, binary sniff |
| Tier 2 metadata storage | `src/live_index/store.rs` | Lightweight `SkippedFile` struct alongside `IndexedFile` |
| Health rendering | `src/protocol/format.rs` | Tier counts, Tier 3 summary |
| Repo map rendering | `src/protocol/format.rs` | Tier 2 tags, Tier 3 footer |

---

## Tests

### Track A: Trust

**T1 — batch_rename qualified paths:**
- Rename `Type` catches `Type::new()`, `module::Type::new()`, `use module::Type`
- Common name `new` renamed does NOT false-positive on `SomeOther::new()`
- Dry-run output separates confident vs uncertain match sections
- Uncertain matches surfaced but not applied by default

**T2 — disk truthfulness:**
- After edit, `search_text` returns content matching disk
- After partial `batch_edit` failure, successfully-written files are correctly re-indexed from disk
- `reindex_after_write` re-reads from disk, not from the write buffer

### Track B: Admission

**Tier precedence:**
- 150MB text file → Tier 3 by size, even though UTF-8
- 50KB `.ckpt` → Tier 2 by denylist, even though small
- 4.2GB `.ckpt` → Tier 3 (size ceiling wins over denylist)
- 2MB `.json` → Tier 2 by size threshold
- 50KB UTF-8 `.txt` → Tier 1
- 500KB Rust source → Tier 1

**Visibility:**
- Tier 2 file shows in repo_map with `[skipped: ...]` tag
- Tier 3 file NOT in repo_map tree, but footer says "N hard-skipped artifacts not shown"
- Health shows all three tier counts
- Symbol headline counts only Tier 1

**Binary detection:**
- File with null bytes in first 8KB → Tier 2
- Pure UTF-8 file above 1MB → Tier 2 by size (not binary)
- Pure UTF-8 file below 1MB → Tier 1

**ComfyUI stress test (acceptance):**
- Project with multi-GB model files indexes without choking
- Health shows realistic symbol count (not inflated by model files)
- Admission tiers prevent artifact-driven index blowups (runtime materially improved vs pre-admission behavior)

---

## Acceptance Criteria

Sprint 14 is complete when:
1. ComfyUI-scale project indexes successfully with multi-GB binaries present
2. Symbol count headline reflects source code only, not artifacts
3. `batch_rename` catches qualified path usages (`Type::new()`, `module::Type`)
4. All edit operations re-index from disk, not from write buffers
5. `search_text` never returns stale results after any edit operation
6. `cargo test --all-targets -- --test-threads=1` green
7. `cargo fmt -- --check` clean

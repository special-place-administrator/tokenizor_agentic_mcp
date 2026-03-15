# Sprint 14 — Trust + Admission Control Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two trust bugs (batch_rename qualified paths, search_text disk truthfulness) and add tiered admission control to prevent large/binary files from choking the indexer.

**Architecture:** Three independent tracks that can be parallelized. Track A1 (batch_rename) adds a supplemental qualified-path scan after the existing rename pass. Track A2 (disk truthfulness) changes edit operations to re-read from disk after writes instead of trusting in-memory buffers. Track B (admission) adds a new `AdmissionTier` enum and gate function that classifies files before parsing.

**Tech Stack:** Rust, tokio, MCP protocol, tree-sitter parsers

---

## Chunk 1: Track B — Tiered Admission Control

This is the largest track and has no dependencies on Track A. It touches the most files but each change is self-contained.

### Task 1: Define `AdmissionTier` enum and constants

**Files:**
- Modify: `src/domain/index.rs` — add enum and threshold constants

- [ ] **Step 1: Write the failing test**

Create a test that imports `AdmissionTier` and checks the three variants exist:

```rust
// In src/domain/index.rs tests module
#[test]
fn test_admission_tier_variants() {
    let t1 = AdmissionTier::Normal;
    let t2 = AdmissionTier::MetadataOnly;
    let t3 = AdmissionTier::HardSkip;
    assert_ne!(t1, t2);
    assert_ne!(t2, t3);
    assert_ne!(t1, t3);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_admission_tier_variants -- --test-threads=1`
Expected: FAIL — `AdmissionTier` not found

- [ ] **Step 3: Implement AdmissionTier enum, SkipReason, AdmissionDecision, and constants**

Add to `src/domain/index.rs`:

```rust
/// Admission tier — whether a file is eligible for indexing/parsing at all.
/// Separate from NoiseClass (which is about ranking/filtering signal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdmissionTier {
    /// Tier 1: Fully indexed — parsed, symbols extracted, text searchable.
    Normal,
    /// Tier 2: Metadata only — path, size, classification stored. No parsing.
    MetadataOnly,
    /// Tier 3: Hard-skipped — counted in health, minimal registration.
    HardSkip,
}

/// Reason a file was placed in Tier 2 or Tier 3.
/// Carried inside `AdmissionDecision` so downstream code never re-derives the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// File exceeds HARD_SKIP_BYTES (Tier 3)
    SizeCeiling,
    /// Extension is on the denylist (Tier 2)
    DenylistedExtension,
    /// File exceeds METADATA_ONLY_BYTES (Tier 2)
    SizeThreshold,
    /// Binary content detected (Tier 2)
    BinaryContent,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkipReason::SizeCeiling => write!(f, ">100MB"),
            SkipReason::DenylistedExtension => write!(f, "artifact"),
            SkipReason::SizeThreshold => write!(f, ">1MB"),
            SkipReason::BinaryContent => write!(f, "binary"),
        }
    }
}

/// Structured result from the admission gate.
/// Contains both the tier and the reason, so consumers never have to re-derive
/// classification logic. Returned by `classify_admission()` and threaded through
/// discovery → store → formatting without decomposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionDecision {
    pub tier: AdmissionTier,
    /// `None` for Tier 1 (Normal) — no skip reason applies.
    pub reason: Option<SkipReason>,
}

impl AdmissionDecision {
    pub fn normal() -> Self {
        Self { tier: AdmissionTier::Normal, reason: None }
    }

    pub fn skip(tier: AdmissionTier, reason: SkipReason) -> Self {
        Self { tier, reason: Some(reason) }
    }
}

/// Files larger than this are always Tier 3 (hard-skipped), regardless of extension.
pub const HARD_SKIP_BYTES: u64 = 100 * 1024 * 1024; // 100 MB

/// Files larger than this (but below HARD_SKIP_BYTES) are Tier 2 (metadata only).
pub const METADATA_ONLY_BYTES: u64 = 1 * 1024 * 1024; // 1 MB

/// How many bytes to read for binary content detection.
pub const BINARY_SNIFF_BYTES: usize = 8192;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_admission_tier_variants -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/domain/index.rs
git commit -m "feat: add AdmissionTier enum and size threshold constants"
```

---

### Task 2: Extension denylist

**Files:**
- Modify: `src/domain/index.rs` — add denylist constant and lookup function

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_extension_is_denylisted() {
    assert!(is_denylisted_extension("safetensors"));
    assert!(is_denylisted_extension("ckpt"));
    assert!(is_denylisted_extension("zip"));
    assert!(is_denylisted_extension("mp4"));
    assert!(is_denylisted_extension("woff2"));
    assert!(is_denylisted_extension("png"));
    assert!(is_denylisted_extension("bin"));
}

#[test]
fn test_extension_not_denylisted() {
    assert!(!is_denylisted_extension("rs"));
    assert!(!is_denylisted_extension("ts"));
    assert!(!is_denylisted_extension("json"));
    assert!(!is_denylisted_extension("svg")); // SVG intentionally NOT denylisted
    assert!(!is_denylisted_extension("md"));
    assert!(!is_denylisted_extension("toml"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_extension_is_denylisted test_extension_not_denylisted -- --test-threads=1`
Expected: FAIL — `is_denylisted_extension` not found

- [ ] **Step 3: Implement denylist**

Add to `src/domain/index.rs`:

```rust
/// Extensions that are always Tier 2 (metadata only) regardless of file size.
const DENYLISTED_EXTENSIONS: &[&str] = &[
    // ML models
    "safetensors", "ckpt", "pt", "onnx", "gguf", "pth",
    // VM/disk images
    "vmdk", "iso", "img", "qcow2",
    // Archives
    "tar", "gz", "zip", "7z", "rar", "bz2", "xz", "zst",
    // Databases
    "db", "sqlite", "sqlite3", "mdb",
    // Media
    "mp3", "mp4", "wav", "avi", "mov", "mkv",
    "png", "jpg", "jpeg", "gif", "bmp", "ico",
    "woff", "woff2", "ttf", "eot",
    // Binary
    "bin",
];

/// Check if a file extension (without leading dot) is on the denylist.
pub fn is_denylisted_extension(ext: &str) -> bool {
    DENYLISTED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test test_extension_is_denylisted test_extension_not_denylisted -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/domain/index.rs
git commit -m "feat: add extension denylist for admission control"
```

---

### Task 3: Binary sniff function

**Files:**
- Modify: `src/discovery/mod.rs` — add binary content detection

**Design note:** NUL-byte-only detection undercatches. Many binary formats (e.g., protobuf,
MessagePack, some image formats) may not have NUL in the first 8KB. The sniff uses three
heuristics — any one triggers binary classification:

1. **NUL byte present** — cheapest check, catches most binaries
2. **UTF-8 decode failure** — catches non-UTF-8 encodings (binary data, non-text encodings)
3. **High control byte ratio** — if >30% of bytes are non-printable control characters
   (0x00–0x08, 0x0E–0x1F, 0x7F), it's almost certainly binary even if technically valid UTF-8

This is not MIME detection — it's a cheap gate to prevent wasting parse time.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_binary_sniff_detects_null_bytes() {
    let content = b"hello\x00world";
    assert!(is_binary_content(content));
}

#[test]
fn test_binary_sniff_allows_pure_utf8() {
    let content = b"fn main() { println!(\"hello\"); }";
    assert!(!is_binary_content(content));
}

#[test]
fn test_binary_sniff_empty_file() {
    assert!(!is_binary_content(b""));
}

#[test]
fn test_binary_sniff_detects_invalid_utf8() {
    // Invalid UTF-8 sequence (continuation byte without start byte)
    let content: &[u8] = &[0x80, 0x81, 0x82, 0x83, 0x84];
    assert!(is_binary_content(content));
}

#[test]
fn test_binary_sniff_detects_high_control_ratio() {
    // Mostly control characters but valid UTF-8 and no NUL
    let mut content = Vec::new();
    for _ in 0..80 {
        content.push(0x01); // SOH — control char
    }
    for _ in 0..20 {
        content.push(b'A'); // printable
    }
    // 80% control bytes > 30% threshold → binary
    assert!(is_binary_content(&content));
}

#[test]
fn test_binary_sniff_allows_low_control_ratio() {
    // Mostly printable with a few control chars (tabs, etc.)
    let content = b"line1\tvalue1\nline2\tvalue2\nline3\tvalue3\n";
    assert!(!is_binary_content(content));
}

#[test]
fn test_binary_sniff_allows_common_whitespace_controls() {
    // \t (0x09), \n (0x0A), \r (0x0D) are NOT counted as suspicious control bytes
    let content = b"col1\tcol2\tcol3\r\nval1\tval2\tval3\r\n";
    assert!(!is_binary_content(content));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_binary_sniff -- --test-threads=1`
Expected: FAIL — `is_binary_content` not found

- [ ] **Step 3: Implement binary sniff**

Add to `src/discovery/mod.rs`:

```rust
/// Check if content appears to be binary.
/// Examines up to BINARY_SNIFF_BYTES of the content using three heuristics:
/// 1. NUL byte present → binary
/// 2. UTF-8 decode failure → binary
/// 3. >30% suspicious control bytes (excluding \t, \n, \r) → binary
pub fn is_binary_content(content: &[u8]) -> bool {
    if content.is_empty() {
        return false;
    }
    let check_len = content.len().min(crate::domain::index::BINARY_SNIFF_BYTES);
    let window = &content[..check_len];

    // Heuristic 1: NUL byte
    if window.contains(&0) {
        return true;
    }

    // Heuristic 2: Invalid UTF-8
    if std::str::from_utf8(window).is_err() {
        return true;
    }

    // Heuristic 3: High control byte ratio
    // Control bytes: 0x00–0x08, 0x0E–0x1F, 0x7F
    // Excludes common text controls: \t (0x09), \n (0x0A), \r (0x0D)
    let suspicious_controls = window
        .iter()
        .filter(|&&b| matches!(b, 0x01..=0x08 | 0x0E..=0x1F | 0x7F))
        .count();
    let ratio = suspicious_controls as f64 / window.len() as f64;
    if ratio > 0.30 {
        return true;
    }

    false
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test test_binary_sniff -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/discovery/mod.rs
git commit -m "feat: add binary content sniff with NUL, UTF-8, and control-byte heuristics"
```

---

### Task 4: Admission gate function

**Files:**
- Modify: `src/discovery/mod.rs` — add `classify_admission` function

- [ ] **Step 1: Write failing tests for tier precedence**

```rust
#[cfg(test)]
mod admission_tests {
    use super::*;
    use crate::domain::index::{AdmissionDecision, AdmissionTier, SkipReason};
    use std::path::Path;

    #[test]
    fn test_huge_text_file_is_hard_skip() {
        // 150MB text file → Tier 3 by size, even though UTF-8
        let decision = classify_admission(Path::new("huge.txt"), 150 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::HardSkip);
        assert_eq!(decision.reason, Some(SkipReason::SizeCeiling));
    }

    #[test]
    fn test_small_ckpt_is_metadata_only() {
        // 50KB .ckpt → Tier 2 by denylist, even though small
        let decision = classify_admission(Path::new("model.ckpt"), 50 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::DenylistedExtension));
    }

    #[test]
    fn test_huge_ckpt_is_hard_skip() {
        // 4.2GB .ckpt → Tier 3 (size ceiling wins over denylist)
        let decision = classify_admission(Path::new("big.ckpt"), 4_200_000_000, None);
        assert_eq!(decision.tier, AdmissionTier::HardSkip);
        assert_eq!(decision.reason, Some(SkipReason::SizeCeiling));
    }

    #[test]
    fn test_large_json_is_metadata_only() {
        // 2MB .json → Tier 2 by size threshold
        let decision = classify_admission(Path::new("big.json"), 2 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::SizeThreshold));
    }

    #[test]
    fn test_small_txt_is_normal() {
        // 50KB UTF-8 .txt → Tier 1
        let decision = classify_admission(Path::new("readme.txt"), 50 * 1024, None);
        assert_eq!(decision, AdmissionDecision::normal());
    }

    #[test]
    fn test_medium_rust_source_is_normal() {
        // 500KB Rust source → Tier 1
        let decision = classify_admission(Path::new("big_module.rs"), 500 * 1024, None);
        assert_eq!(decision, AdmissionDecision::normal());
    }

    #[test]
    fn test_binary_content_is_metadata_only() {
        // File with null bytes → Tier 2
        let content = b"ELF\x00\x00\x00binary";
        let decision = classify_admission(Path::new("unknown_file"), 1024, Some(content));
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::BinaryContent));
    }

    #[test]
    fn test_svg_not_denylisted() {
        // SVG files are intentionally NOT denylisted
        let decision = classify_admission(Path::new("icon.svg"), 50 * 1024, None);
        assert_eq!(decision, AdmissionDecision::normal());
    }

    #[test]
    fn test_large_svg_is_metadata_only_by_size() {
        // Large SVG → Tier 2 by size threshold, not denylist
        let decision = classify_admission(Path::new("huge.svg"), 2 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::SizeThreshold));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test admission_tests -- --test-threads=1`
Expected: FAIL — `classify_admission` not found

- [ ] **Step 3: Implement admission gate**

Add to `src/discovery/mod.rs`:

```rust
use crate::domain::index::{
    AdmissionDecision, AdmissionTier, SkipReason,
    HARD_SKIP_BYTES, METADATA_ONLY_BYTES, is_denylisted_extension,
};

/// Classify a file's admission tier based on the gate sequence.
///
/// Returns an `AdmissionDecision` containing both the tier and the reason,
/// so downstream code (store, formatting) never re-derives classification logic.
///
/// Precedence (first match wins):
/// 1. Hard-skip size ceiling (>100MB) → Tier 3
/// 2. Extension denylist → Tier 2
/// 3. Metadata-only size threshold (>1MB) → Tier 2
/// 4. Binary sniff (null bytes in first 8KB) → Tier 2
/// 5. All else → Tier 1
///
/// `content_sample` is optional — if provided, used for binary sniff.
/// If not provided, binary sniff is skipped (caller must handle separately
/// or provide content after reading the file).
pub fn classify_admission(
    path: &std::path::Path,
    file_size: u64,
    content_sample: Option<&[u8]>,
) -> AdmissionDecision {
    // Step 1: Hard-skip size ceiling — ALWAYS checked first
    if file_size > HARD_SKIP_BYTES {
        return AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling);
    }

    // Step 2: Extension denylist
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if is_denylisted_extension(ext) {
            return AdmissionDecision::skip(
                AdmissionTier::MetadataOnly,
                SkipReason::DenylistedExtension,
            );
        }
    }

    // Step 3: Metadata-only size threshold
    if file_size > METADATA_ONLY_BYTES {
        return AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::SizeThreshold);
    }

    // Step 4: Binary sniff (if content available)
    if let Some(content) = content_sample {
        if is_binary_content(content) {
            return AdmissionDecision::skip(
                AdmissionTier::MetadataOnly,
                SkipReason::BinaryContent,
            );
        }
    }

    // Step 5: All else → normal indexing
    AdmissionDecision::normal()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test admission_tests -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/discovery/mod.rs
git commit -m "feat: implement admission gate with tiered file classification"
```

---

### Task 5: SkippedFile metadata struct and store integration

**Files:**
- Modify: `src/domain/index.rs` — add `SkippedFile` struct
- Modify: `src/live_index/store.rs` — add storage for Tier 2/3 files

- [ ] **Step 1: Write failing test for SkippedFile**

```rust
// In src/domain/index.rs tests
#[test]
fn test_skipped_file_creation() {
    let decision = AdmissionDecision::skip(
        AdmissionTier::MetadataOnly,
        SkipReason::DenylistedExtension,
    );
    let sf = SkippedFile {
        path: "model.safetensors".into(),
        size: 4_200_000_000,
        extension: Some("safetensors".into()),
        decision,
    };
    assert_eq!(sf.tier(), AdmissionTier::MetadataOnly);
    assert_eq!(sf.reason(), Some(SkipReason::DenylistedExtension));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_skipped_file_creation -- --test-threads=1`
Expected: FAIL

- [ ] **Step 3: Implement SkippedFile struct**

`SkipReason` and `AdmissionDecision` were already defined in Task 1. `SkippedFile` stores the
decision directly — no re-derivation of tier or reason anywhere downstream.

Add to `src/domain/index.rs`:

```rust
/// Metadata record for a file that was not fully indexed (Tier 2 or Tier 3).
/// Constructed from `AdmissionDecision` during discovery — the decision is
/// stored as-is so formatting code can access both tier and reason without
/// re-running classification logic.
#[derive(Debug, Clone)]
pub struct SkippedFile {
    pub path: String,
    pub size: u64,
    pub extension: Option<String>,
    pub decision: AdmissionDecision,
}

impl SkippedFile {
    /// Convenience accessors — delegates to the stored decision.
    pub fn tier(&self) -> AdmissionTier {
        self.decision.tier
    }

    pub fn reason(&self) -> Option<SkipReason> {
        self.decision.reason
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_skipped_file_creation -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Add skipped file storage to LiveIndex store**

Add to `src/live_index/store.rs`:
- A `skipped_files: Vec<SkippedFile>` field (or `HashMap<String, SkippedFile>` keyed by path)
- Methods: `add_skipped_file(&mut self, sf: SkippedFile)`, `skipped_files(&self) -> &[SkippedFile]`, `tier_counts(&self) -> (usize, usize, usize)` returning (tier1, tier2, tier3) counts

- [ ] **Step 6: Write test for tier counts**

```rust
#[test]
fn test_tier_counts() {
    // Setup store with some indexed files and some skipped files
    // Assert tier_counts returns correct breakdown
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test tier_counts -- --test-threads=1`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/domain/index.rs src/live_index/store.rs
git commit -m "feat: add SkippedFile struct and store integration for admission tiers"
```

---

### Task 6: Wire admission gate into discovery walk

**Files:**
- Modify: `src/discovery/mod.rs` — call `classify_admission` during file walk
- Modify: `src/live_index/store.rs` — route Tier 2/3 to skipped storage

- [ ] **Step 1: Identify the file walk entry point**

Search for the function in `src/discovery/mod.rs` that iterates directory entries and feeds files into the index. This is where admission classification will be inserted.

- [ ] **Step 2: Add admission check before parsing**

At the point where a discovered file would be sent to the parser:
1. Get file size from metadata (already available from the walk)
2. Call `classify_admission(path, size, None)` → returns `AdmissionDecision`
3. If `decision.tier == HardSkip` → create `SkippedFile { decision, .. }`, add to store, skip parsing
4. If `decision.tier == MetadataOnly` → create `SkippedFile { decision, .. }`, add to store, skip parsing
5. If `decision.tier == Normal` → proceed with existing parsing flow
6. For files that pass steps 1-3 but haven't been binary-sniffed yet: after reading content for parsing, call `classify_admission(path, size, Some(&content))` again with the content sample. If the decision changes to MetadataOnly (binary detected), create `SkippedFile` from the new decision and skip parsing.

**Key:** The `AdmissionDecision` flows through unchanged — discovery creates it, store stores it, formatting reads it. No re-derivation.

**Reclassification guard:** If a file passed the initial `classify_admission(path, size, None)` as Tier 1 (Normal) and is later reclassified to Tier 2 after the binary sniff with content, **no partial symbol/index state for that file may remain in the store**. The implementation must either:
- (a) Not add the file to the symbol index until after the binary sniff confirms Tier 1, or
- (b) Remove any partially-built index entries before adding the file to `skipped_files`

Option (a) is strongly preferred — do not start parsing/indexing until admission is finalized. This means the content read for binary sniffing happens *before* the content is passed to the parser, not after.

- [ ] **Step 3: Write integration test**

```rust
#[test]
fn test_discovery_skips_denylisted_extension() {
    // Create a temp dir with:
    // - a normal .rs file (50 bytes)
    // - a fake .safetensors file (small, but denylisted extension)
    // Run discovery/indexing
    // Assert: .rs file is indexed, .safetensors is in skipped_files
    // Assert: .safetensors has reason = DenylistedExtension
}

#[test]
fn test_discovery_deferred_binary_sniff_reclassifies() {
    // Exercises the two-phase admission path:
    // Phase 1: classify_admission(path, size, None) returns Normal
    //          (extension not denylisted, size < 1MB)
    // Phase 2: after reading content, reclassify with Some(&content)
    //          detects binary → MetadataOnly
    //
    // Setup:
    // - a normal .rs file
    // - a "custom.dat" file with NUL-heavy content (not on denylist, <1MB)
    let dir = tempdir().unwrap();
    create_file(&dir, "main.rs", "fn main() {}\n");
    create_binary_file(&dir, "custom.dat", &{
        let mut v = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic
        v.extend(vec![0x00; 1024]);
        v
    });

    let index = index_directory(dir.path());

    // .rs file is Tier 1 and indexed
    assert!(index.has_symbols_for("main.rs"));

    // custom.dat is Tier 2 via deferred binary sniff
    let skipped = index.skipped_files();
    let dat = skipped.iter().find(|f| f.path.contains("custom.dat"));
    assert!(dat.is_some(), "custom.dat should be in skipped_files");
    assert_eq!(dat.unwrap().reason(), Some(SkipReason::BinaryContent));

    // CRITICAL: no partial index state — custom.dat has zero symbols
    let symbols = index.symbols_for_file("custom.dat");
    assert!(
        symbols.is_empty(),
        "Binary file must have no symbols in index after reclassification"
    );
}
```

- [ ] **Step 4: Run test**

Run: `cargo test test_discovery_skips -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/discovery/mod.rs src/live_index/store.rs
git commit -m "feat: wire admission gate into discovery walk"
```

---

### Task 7: Update health output with tier counts

**Files:**
- Modify: `src/protocol/format.rs` — add tier breakdown to health rendering

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_health_shows_tier_counts() {
    // Create a mock health output with tier counts
    // Assert output contains:
    // "Admission: N files discovered"
    // "  Tier 1 (indexed): ..."
    // "  Tier 2 (metadata only): ..."
    // "  Tier 3 (hard-skipped): ..."
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_health_shows_tier_counts -- --test-threads=1`
Expected: FAIL

- [ ] **Step 3: Implement tier count rendering**

In the health formatting function in `src/protocol/format.rs`, add a new section after the existing file/symbol counts:

```rust
// Add admission tier breakdown
let (tier1, tier2, tier3) = store.tier_counts();
let total = tier1 + tier2 + tier3;
writeln!(out, "Admission: {} files discovered", total)?;
writeln!(out, "  Tier 1 (indexed): {}", tier1)?;
writeln!(out, "  Tier 2 (metadata only): {}", tier2)?;
writeln!(out, "  Tier 3 (hard-skipped): {}", tier3)?;
```

- [ ] **Step 4: Ensure symbol headline counts only Tier 1**

Verify the existing symbol count in health output already reflects only indexed files (Tier 1). If it sums across all files, restrict it. The headline `N symbols` must count only Tier 1 symbols.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test test_health_shows_tier_counts -- --test-threads=1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/protocol/format.rs
git commit -m "feat: show admission tier counts in health output"
```

---

### Task 8: Update repo_map with tier visibility

**Files:**
- Modify: `src/protocol/format.rs` — Tier 2 tags in tree, Tier 3 footer

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_repo_map_shows_tier2_tagged() {
    // Tier 2 file should appear in tree with skip tag:
    // "model.safetensors [skipped: artifact, 4.2 GB]"
}

#[test]
fn test_repo_map_tier3_footer_only() {
    // Tier 3 files should NOT appear in tree
    // Footer should say "N hard-skipped artifacts not shown (>100MB)"
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_repo_map_shows_tier -- --test-threads=1`
Expected: FAIL

- [ ] **Step 3: Implement Tier 2 tags**

In the repo_map tree rendering, when emitting a file entry:
- Check if the file is in `skipped_files`
- If Tier 2: append `[skipped: {reason}, {human_size}]` to the line
- If Tier 3: skip it from the tree entirely

- [ ] **Step 4: Implement Tier 3 footer**

After the tree, if there are any Tier 3 files:
```rust
if tier3_count > 0 {
    writeln!(out, "{} hard-skipped artifacts not shown (>100MB)", tier3_count)?;
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test test_repo_map_shows_tier -- --test-threads=1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/protocol/format.rs
git commit -m "feat: show Tier 2 tags and Tier 3 footer in repo_map"
```

---

## Chunk 2: Track A1 — batch_rename Qualified Path Coverage

### Task 9: Qualified path detection function

**Files:**
- Modify: `src/protocol/edit.rs` — add qualified path scanning

- [ ] **Step 1: Write failing tests for qualified path matching**

```rust
#[cfg(test)]
mod qualified_path_tests {
    use super::*;

    #[test]
    fn test_finds_type_new_qualified_call() {
        let source = "let x = MyType::new();";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_finds_deep_nested_qualified() {
        let source = "let x = module::MyType::new();";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_finds_use_import_path() {
        let source = "use crate::module::MyType;";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_scanner_finds_all_raw_occurrences_of_common_name() {
        // The scanner itself is context-free — it finds ALL qualified usages
        // of the identifier, regardless of scope. Scoping is the caller's job.
        let source = "let x = SomeOther::new();\nlet y = Target::new();";
        let matches = find_qualified_usages("new", source);
        // Scanner must find both occurrences
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|m| m.confident));
    }

    // Note: The corresponding integration test in Task 10 verifies that
    // batch_rename applies scope filtering — only renaming `new` within
    // the target symbol's neighborhood, not all `::new()` calls globally.

    #[test]
    fn test_uncertain_match_in_string() {
        let source = r#"let s = "MyType::new()";"#;
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident); // Inside string literal — uncertain
    }

    #[test]
    fn test_uncertain_match_in_comment() {
        let source = "// MyType::new() creates an instance";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident); // Inside comment — uncertain
    }

    #[test]
    fn test_finds_turbofish_qualified_call() {
        let source = "let x = MyType::<T>::new();";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_uncertain_match_in_block_comment() {
        let source = "/* MyType::new() creates an instance */";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident); // Inside block comment — uncertain
    }

    #[test]
    fn test_uncertain_match_in_multiline_string() {
        let source = r#"let s = r"
            MyType::new()
        ";"#;
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident); // Inside raw string — uncertain
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test qualified_path_tests -- --test-threads=1`
Expected: FAIL

- [ ] **Step 3: Implement qualified path scanner**

Add to `src/protocol/edit.rs`:

```rust
/// A qualified path match with confidence classification.
#[derive(Debug)]
pub struct QualifiedMatch {
    /// Byte offset of the match in the source.
    pub offset: usize,
    /// Line number (1-based).
    pub line: usize,
    /// The full matched segment (e.g., "MyType::new()").
    pub context: String,
    /// Whether the match is confident (code context) or uncertain (string/comment).
    pub confident: bool,
}

/// Find qualified path usages of `identifier` in `source`.
///
/// Looks for patterns like:
/// - `identifier::method()`
/// - `module::identifier::method()`
/// - `use path::identifier`
/// - `identifier::CONST`
///
/// Classifies matches as confident (in code) or uncertain (in strings/comments).
pub fn find_qualified_usages(identifier: &str, source: &str) -> Vec<QualifiedMatch> {
    let mut matches = Vec::new();
    let pattern = format!(r"(^|[^a-zA-Z0-9_]){}(\s*::|\s*;)", regex::escape(identifier));
    // Also match `::identifier` for use paths
    let use_pattern = format!(r"::\s*{}\b", regex::escape(identifier));

    // Implementation: scan each line, check if in string/comment context,
    // classify as confident or uncertain accordingly.
    // Use regex to find matches, then use simple heuristics for context:
    // - Line starts with // → comment → uncertain
    // - Match is inside quote delimiters → string → uncertain
    // - Otherwise → confident

    // ... (full regex-based implementation)

    matches
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test qualified_path_tests -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs
git commit -m "feat: add qualified path usage scanner for batch_rename"
```

---

### Task 10: Integrate qualified scan into batch_rename

**Files:**
- Modify: `src/protocol/edit.rs` — call supplemental scan after existing rename pass

- [ ] **Step 1: Write failing integration test**

```rust
#[test]
fn test_batch_rename_catches_qualified_call() {
    // Setup: create temp files with qualified usage `OldType::new()`
    // Run batch_rename to rename OldType → NewType
    // Assert: `OldType::new()` becomes `NewType::new()`
}

#[test]
fn test_batch_rename_dry_run_separates_confident_uncertain() {
    // Setup: file with code usage and comment usage
    // Run batch_rename dry_run
    // Assert: output has separate sections for confident and uncertain matches
}

#[test]
fn test_batch_rename_uncertain_not_applied_by_default() {
    // Setup: file with OldType only in a comment
    // Run batch_rename (not dry_run)
    // Assert: comment is NOT modified (uncertain matches skipped)
}

#[test]
fn test_batch_rename_scopes_common_name_to_target() {
    // Completes the contract from Task 9's scanner test:
    // Scanner finds ALL raw occurrences; batch_rename applies scope filtering.
    // Setup: file with `SomeOther::new()` and `Target::new()`
    // Run batch_rename renaming `new` scoped to `Target`
    // Assert: only `Target::new()` is renamed, `SomeOther::new()` is untouched
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_batch_rename_catches_qualified test_batch_rename_dry_run_separates test_batch_rename_uncertain_not_applied -- --test-threads=1`
Expected: FAIL

- [ ] **Step 3: Implement supplemental qualified scan in batch_rename**

In the `batch_rename` handler in `src/protocol/edit.rs`:

After the existing indexed-reference rename pass:
1. Determine the file scope: files with existing rename candidates + files in the import/module neighborhood
2. For each file in scope, call `find_qualified_usages(old_name, file_content)`
3. Confident matches: apply the rename (replace `old_name` with `new_name` at the matched positions)
4. Uncertain matches: collect but do NOT apply
5. In dry_run mode: render confident and uncertain sections separately in output
6. In live mode: apply only confident matches, surface uncertain matches in the response

- [ ] **Step 4: Run tests**

Run: `cargo test test_batch_rename_catches_qualified test_batch_rename_dry_run_separates test_batch_rename_uncertain_not_applied -- --test-threads=1`
Expected: PASS

- [ ] **Step 5: Update dry_run output format**

Ensure the dry_run output clearly separates:
```
── Confident matches (will be applied) ──
  src/main.rs:10  OldType::new() → NewType::new()

── Uncertain matches (NOT applied — review manually) ──
  src/main.rs:25  // OldType::new() creates an instance
```

- [ ] **Step 6: Commit**

```bash
git add src/protocol/edit.rs
git commit -m "feat: batch_rename supplemental qualified path scan with confidence classification"
```

---

## Chunk 3: Track A2 — Disk Truthfulness

### Task 11: Implement reindex-from-disk after write

**Files:**
- Modify: `src/protocol/edit.rs` — change post-write reindexing to re-read from disk

- [ ] **Step 1: Write failing test for reindex from disk**

```rust
#[test]
fn test_reindex_after_write_reads_from_disk() {
    // Setup: create a temp file, index it
    // Perform an edit via replace_symbol_body or edit_within_symbol
    // After the edit, verify the index was rebuilt from disk content
    // (not from the in-memory buffer passed to fs::write)
    //
    // Strategy: mock or intercept to verify fs::read is called after fs::write
    // OR: verify search_text returns the on-disk content
}

#[test]
fn test_search_text_matches_disk_after_edit() {
    // Setup: index a file containing "old_content"
    // Edit the file to contain "new_content"
    // search_text("old_content") → no results
    // search_text("new_content") → finds the file
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_reindex_after_write test_search_text_matches_disk -- --test-threads=1`
Expected: FAIL

- [ ] **Step 3: Implement reindex-from-disk**

In `src/protocol/edit.rs`, find the existing write-and-reindex flow. Change it to:

```rust
/// Write content to a file and fully reindex from disk.
///
/// INVARIANT: **All** derived index state is rebuilt from the persisted
/// on-disk bytes, never from the in-memory buffer passed to `fs::write`.
///
/// "All derived index state" means every search-relevant structure:
///   - **Symbol index** — parsed symbols, kinds, ranges, parent links
///   - **FTS / text index** — trigram index used by `search_text`
///   - **Reverse-reference index** — caller/callee/import edges
///   - **Per-file caches** — line offsets, content hashes, anything
///     derived from file bytes that query paths may read
///
/// If any of these are skipped, `search_text` or `find_references`
/// can return stale results — which is the exact bug this function fixes.
fn write_and_reindex(
    path: &Path,
    content: &[u8],
    index: &mut LiveIndex,
) -> Result<()> {
    // 1. Write to disk
    std::fs::write(path, content)?;

    // 2. Re-read from disk (NOT from `content` parameter)
    let on_disk = std::fs::read(path)?;

    // 3. Rebuild ALL index entries from on-disk bytes:
    //    symbols, FTS/trigrams, reverse refs, per-file caches.
    //    This is the same path used by initial indexing — not a
    //    partial "update symbols only" shortcut.
    index.reindex_file_full(path, &on_disk)?;

    // 4. Optional debug assertion
    debug_assert_eq!(
        content, &on_disk[..],
        "write_and_reindex: disk content differs from written buffer"
    );

    Ok(())
}
```

**Implementation note:** The call is `reindex_file_full`, not `reindex_file`. If the existing codebase has a partial reindex function (e.g., symbol-only), the implementer must use or create one that rebuilds **all** derived structures. Verify by confirming that after `write_and_reindex`, both `search_text` (FTS) and `find_references` (reverse index) return results consistent with the new on-disk content.

- [ ] **Step 4: Apply to all edit operations**

Ensure `write_and_reindex` is called by all edit tool handlers:
- `replace_symbol_body`
- `edit_within_symbol`
- `insert_symbol`
- `delete_symbol`
- `batch_edit`
- `batch_rename`
- `batch_insert`

Search for all `fs::write` calls in `src/protocol/edit.rs` and replace with `write_and_reindex`.

- [ ] **Step 5: Run tests**

Run: `cargo test test_reindex_after_write test_search_text_matches_disk -- --test-threads=1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/protocol/edit.rs
git commit -m "fix: reindex from disk after writes, not from in-memory buffer"
```

---

### Task 12: Partial failure correctness and atomicity semantics

**Files:**
- Modify: `src/protocol/edit.rs` — ensure partial batch failures leave correct index state

**Atomicity policy (decide before coding):**

Before implementing, audit the existing batch operations and classify each:

| Operation | Current contract | Sprint 14 policy |
|-----------|-----------------|------------------|
| `batch_edit` | Best-effort (edits are independent) | **Best-effort** — each file is written+reindexed independently. Partial success is reported. |
| `batch_rename` | Atomic (rename is meaningless if partial) | **Atomic** — stage all file contents first, validate all will succeed, then write+reindex all. On any failure, rollback already-written files to their pre-edit content and reindex from disk. |
| `batch_insert` | Best-effort (inserts are independent) | **Best-effort** — same as `batch_edit`. |

**Critical:** Do not quietly downgrade `batch_rename` from atomic to best-effort. A partial rename (some files renamed, some not) corrupts the codebase more than a failed rename that changes nothing.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_batch_edit_partial_success_reindexes_completed() {
    // Setup: create 3 temp files, index all
    // batch_edit targeting all 3, but file 2 is read-only (will fail)
    // After the error:
    // - File 1: index reflects the edit (reindexed from disk)
    // - File 2: index unchanged (write failed)
    // - File 3: index unchanged (never reached)
    // - Response reports file 1 as successful, file 2 as failed
}

#[test]
fn test_batch_edit_no_speculative_index_mutation() {
    // Setup: create 3 temp files, index all
    // batch_edit targeting all 3, file 2 fails
    // Assert: file 3's index entry is EXACTLY the pre-edit content
    // (no speculative mutation occurred)
}

#[test]
fn test_batch_rename_rolls_back_on_failure() {
    // Setup: create 3 files with references to "OldName"
    // Make file 2 read-only so rename write will fail
    // Run batch_rename OldName → NewName
    // After the error:
    // - File 1: ROLLED BACK to original content ("OldName"), reindexed from disk
    // - File 2: unchanged (write failed)
    // - File 3: unchanged (never reached)
    // - All files still contain "OldName" — no partial rename
}

#[test]
fn test_batch_rename_rollback_failure_surfaced_loudly() {
    // Setup: create 3 files, rename OldName → NewName
    // File 1 writes successfully, file 2 fails
    // Make file 1 read-only AFTER it was written (so rollback write fails)
    // Assert: error contains rollback_incomplete with file 1's path
    // Assert: error message explicitly warns about incomplete rollback
}

#[test]
fn test_batch_rename_atomic_success_reindexes_all() {
    // Setup: create 3 files with references to "OldName"
    // Run batch_rename OldName → NewName (all writable)
    // After success:
    // - All 3 files contain "NewName" on disk AND in index
    // - search_text("OldName") returns nothing
    // - search_text("NewName") finds all 3
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_batch_edit_partial test_batch_rename_rolls_back test_batch_rename_atomic_success -- --test-threads=1`
Expected: FAIL

- [ ] **Step 3: Implement best-effort flow for batch_edit and batch_insert**

```rust
// Best-effort: write+reindex each file independently
let mut successful = Vec::new();
let mut failed = Vec::new();
for edit in edits {
    match write_and_reindex(&edit.path, &edit.new_content, index) {
        Ok(()) => successful.push(edit.path.clone()),
        Err(e) => failed.push((edit.path.clone(), e)),
    }
}
// Report both successful and failed — don't stop at first failure
```

- [ ] **Step 4: Implement atomic flow for batch_rename**

```rust
// Atomic: stage → validate → apply → rollback on failure
// 1. Stage: compute all new file contents in memory
let staged: Vec<(PathBuf, Vec<u8>, Vec<u8>)> = ...; // (path, old_content, new_content)

// 2. Validate: check all files are writable (permissions, not locked)
for (path, _, _) in &staged {
    check_writable(path)?; // Fail fast before any writes
}

// 3. Apply: write + reindex each file
let mut written = Vec::new();
for (path, old_content, new_content) in &staged {
    match write_and_reindex(path, new_content, index) {
        Ok(()) => written.push((path.clone(), old_content.clone())),
        Err(e) => {
            // 4. Rollback: restore all already-written files
            let mut rollback_failures = Vec::new();
            for (rollback_path, original) in &written {
                if let Err(rb_err) = std::fs::write(rollback_path, original) {
                    // Rollback write failed — record it, do NOT silently ignore
                    rollback_failures.push((rollback_path.clone(), rb_err));
                    continue;
                }
                match std::fs::read(rollback_path) {
                    Ok(on_disk) => {
                        let _ = index.reindex_file_full(rollback_path, &on_disk);
                    }
                    Err(rb_err) => {
                        rollback_failures.push((rollback_path.clone(), rb_err));
                    }
                }
            }
            return Err(AtomicRenameError {
                failed_file: path.clone(),
                error: e,
                rolled_back: written.len() - rollback_failures.len(),
                // CRITICAL: surface rollback failures explicitly so the user
                // knows the repo may be in a partially-renamed state
                rollback_incomplete: rollback_failures,
            });
        }
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test test_batch_edit_partial test_batch_rename_rolls_back test_batch_rename_atomic_success -- --test-threads=1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/protocol/edit.rs
git commit -m "fix: batch_rename atomic rollback on failure, batch_edit best-effort with correct index state"
```

---

## Chunk 4: Final Validation

### Task 13: Full test suite + formatting check

**Files:** None modified — validation only

- [ ] **Step 1: Run full test suite**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests PASS

- [ ] **Step 2: Run format check**

Run: `cargo fmt -- --check`
Expected: No formatting differences

- [ ] **Step 3: Run compilation check**

Run: `cargo check`
Expected: No errors or warnings

- [ ] **Step 4: Fix any issues found in steps 1-3**

If any test failures, formatting issues, or compilation errors exist, fix them before proceeding.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix: resolve test/format issues from Sprint 14 integration"
```

---

### Task 14: Acceptance test — admission tier coverage

**Files:**
- Create: `tests/admission_acceptance.rs` — acceptance test for the full admission flow

- [ ] **Step 1: Write acceptance test**

```rust
/// Exercises all three admission tiers across realistic file layouts:
/// nested dirs, multiple denylisted artifacts, size thresholds,
/// binary-without-denylisted-extension, and hard-skip files.
#[test]
fn test_admission_tier_acceptance() {
    let dir = tempdir().unwrap();

    // ── Tier 1: Normal source files (nested dirs) ──
    // Use .rs files — Rust parsing is guaranteed supported in this repo.
    create_file(&dir, "src/main.rs", "fn main() {}\n");
    create_file(&dir, "src/lib.rs", "pub fn helper() -> i32 { 42 }\n");
    create_file(&dir, "src/utils/mod.rs", "pub struct Config;\n");
    create_file(&dir, "README.md", "# Project\n");
    create_file(&dir, "config.toml", "[settings]\nkey = \"value\"\n");

    // ── Tier 2: Denylisted extensions (various categories) ──
    create_file(&dir, "models/v1.safetensors", "fake model");
    create_file(&dir, "models/v2.ckpt", "fake checkpoint");
    create_file(&dir, "assets/logo.png", "fake png");
    create_file(&dir, "assets/font.woff2", "fake font");
    create_file(&dir, "backups/data.sqlite3", "fake db");
    create_file(&dir, "release.zip", "fake archive");

    // ── Tier 2: Size threshold (>1MB, no denylisted extension) ──
    create_large_file(&dir, "data/big_config.json", 2 * 1024 * 1024);

    // ── Tier 2: Binary content without denylisted extension ──
    // A .dat file that is not on the denylist but contains binary content
    create_binary_file(&dir, "data/custom.dat", &{
        let mut v = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic bytes
        v.extend(vec![0x00; 1024]); // NUL-heavy binary content
        v
    });

    // ── Tier 3: Hard-skip (>100MB) ──
    // We can't create a real 150MB file in a unit test without being slow,
    // so we test via classify_admission directly with a fake size.
    // But for the integration path, create a file that triggers Tier 3
    // if the test harness supports sparse files, or skip with a note.

    // Index the directory
    let index = index_directory(dir.path());

    // ── Verify tier counts ──
    let (tier1, tier2, tier3) = index.tier_counts();

    // Tier 1: main.rs, lib.rs, mod.rs, README.md, config.toml = 5
    assert_eq!(tier1, 5, "Expected 5 Tier 1 files");

    // Tier 2: 6 denylisted + 1 size threshold + 1 binary = 8
    assert_eq!(tier2, 8, "Expected 8 Tier 2 files");

    // ── Verify symbol headline is Tier 1 only ──
    let symbol_count = index.symbol_count();
    // Rust files produce known symbols: main, helper, Config
    assert!(symbol_count >= 3, "Should have at least 3 symbols from .rs files");
    // Symbols must NOT include anything from .safetensors, .ckpt, .png, etc.

    // ── Verify Tier 2 files appear in skipped_files with correct reasons ──
    let skipped = index.skipped_files();
    let safetensors = skipped.iter().find(|f| f.path.contains("v1.safetensors"));
    assert!(safetensors.is_some(), ".safetensors should be in skipped_files");
    assert_eq!(
        safetensors.unwrap().reason(),
        Some(SkipReason::DenylistedExtension),
    );

    let big_json = skipped.iter().find(|f| f.path.contains("big_config.json"));
    assert!(big_json.is_some(), "big JSON should be in skipped_files");
    assert_eq!(
        big_json.unwrap().reason(),
        Some(SkipReason::SizeThreshold),
    );

    let binary_dat = skipped.iter().find(|f| f.path.contains("custom.dat"));
    assert!(binary_dat.is_some(), "binary .dat should be in skipped_files");
    assert_eq!(
        binary_dat.unwrap().reason(),
        Some(SkipReason::BinaryContent),
    );

    // ── Verify no half-indexed state ──
    // None of the Tier 2 files should have symbols in the index
    for sf in skipped {
        let symbols = index.symbols_for_file(&sf.path);
        assert!(
            symbols.is_empty(),
            "Skipped file {} should have no symbols in index",
            sf.path
        );
    }
}

/// Tests classify_admission directly for Tier 3 (hard-skip) since we
/// can't create 150MB files in tests without being wasteful.
#[test]
fn test_admission_tier3_classify_direct() {
    use std::path::Path;

    // 150MB text file → Tier 3
    let decision = classify_admission(Path::new("huge.log"), 150 * 1024 * 1024, None);
    assert_eq!(decision.tier, AdmissionTier::HardSkip);
    assert_eq!(decision.reason, Some(SkipReason::SizeCeiling));

    // 4.2GB .ckpt → Tier 3 (size ceiling beats denylist)
    let decision = classify_admission(Path::new("big.ckpt"), 4_200_000_000, None);
    assert_eq!(decision.tier, AdmissionTier::HardSkip);
    assert_eq!(decision.reason, Some(SkipReason::SizeCeiling));
}
```

- [ ] **Step 2: Run acceptance test**

Run: `cargo test admission_acceptance -- --test-threads=1`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add tests/admission_acceptance.rs
git commit -m "test: add admission tier acceptance test with nested dirs, binary, and multi-artifact coverage"
```

---

## Summary of Files Changed

| File | Track | Changes |
|------|-------|---------|
| `src/domain/index.rs` | B | `AdmissionTier` enum, `SkipReason` enum, `AdmissionDecision` struct, constants, `SkippedFile`, extension denylist |
| `src/discovery/mod.rs` | B | `is_binary_content()`, `classify_admission()`, gate wiring in walk |
| `src/live_index/store.rs` | B | `skipped_files` storage, `add_skipped_file()`, `tier_counts()` |
| `src/protocol/format.rs` | B | Health tier counts, repo_map Tier 2 tags, Tier 3 footer |
| `src/protocol/edit.rs` | A1, A2 | `find_qualified_usages()`, supplemental scan in `batch_rename`, `write_and_reindex()` |
| `tests/admission_acceptance.rs` | B | ComfyUI stress acceptance test |

## Dependency Graph

```
Task 1 (AdmissionTier enum)
  ├→ Task 2 (Extension denylist)
  ├→ Task 3 (Binary sniff)
  │   └→ Task 4 (Admission gate) ← depends on Tasks 1, 2, 3
  └→ Task 5 (SkippedFile + store)
       └→ Task 6 (Wire into discovery) ← depends on Tasks 4, 5
            ├→ Task 7 (Health output)
            └→ Task 8 (Repo map output)

Task 9 (Qualified path scanner) — independent
  └→ Task 10 (Integrate into batch_rename)

Task 11 (Reindex from disk) — independent
  └→ Task 12 (Partial failure correctness)

Task 13 (Full validation) ← depends on all above
  └→ Task 14 (Acceptance test)
```

## Parallelization

Three independent work streams can run concurrently:
- **Stream 1:** Tasks 1–8 (Track B — Admission Control)
- **Stream 2:** Tasks 9–10 (Track A1 — Qualified Paths)
- **Stream 3:** Tasks 11–12 (Track A2 — Disk Truthfulness)

Tasks 13–14 are sequential after all streams complete.

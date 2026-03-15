//! Acceptance tests for the full admission tier system end-to-end.
//!
//! Covers all three tiers across realistic file layouts:
//!   Tier 1 (Normal / indexed)   — source files that produce symbols
//!   Tier 2 (MetadataOnly)       — denylisted extensions, >1 MB, binary content
//!   Tier 3 (HardSkip)           — >100 MB (tested via classify_admission directly)

use std::fs;
use std::path::Path;
use tempfile::tempdir;
use tokenizor_agentic_mcp::live_index::LiveIndex;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn write_file(dir: &Path, name: &str, content: &[u8]) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
}

// ---------------------------------------------------------------------------
// Test 1: full pipeline — Tier 1 indexed, Tier 2 skipped, counts correct
// ---------------------------------------------------------------------------

/// Exercises all three admission tiers across realistic file layouts.
///
/// Tier 1: normal Rust/Markdown/TOML source files  → indexed, symbols extracted
/// Tier 2: denylisted extensions, >1 MB, binary    → MetadataOnly skip
/// Tier 3: not exercised at runtime (files would be >100 MB); see Test 2.
#[test]
fn test_admission_tier_acceptance() {
    let dir = tempdir().unwrap();

    // ── Tier 1: normal source files ──────────────────────────────────────
    write_file(dir.path(), "src/main.rs", b"fn main() {}\n");
    write_file(dir.path(), "src/lib.rs", b"pub fn helper() -> i32 { 42 }\n");
    write_file(dir.path(), "src/utils/mod.rs", b"pub struct Config;\n");
    write_file(dir.path(), "README.md", b"# Project\n");
    write_file(dir.path(), "config.toml", b"[settings]\nkey = \"value\"\n");

    // ── Tier 2: denylisted extensions ────────────────────────────────────
    write_file(dir.path(), "models/v1.safetensors", b"fake model");
    write_file(dir.path(), "models/v2.ckpt", b"fake checkpoint");
    write_file(dir.path(), "assets/logo.png", b"fake png");
    write_file(dir.path(), "assets/font.woff2", b"fake font");
    write_file(dir.path(), "backups/data.sqlite3", b"fake db");
    write_file(dir.path(), "release.zip", b"fake archive");

    // ── Tier 2: size threshold (>1 MB) ───────────────────────────────────
    // Write 1.5 MB of repeated ASCII — text content so no binary sniff
    let big_content = b"x".repeat(1_500_000);
    write_file(dir.path(), "data/big_config.json", &big_content);

    // ── Tier 2: binary content (not denylisted, contains NUL bytes) ───────
    let binary_content: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48];
    write_file(dir.path(), "data/custom.dat", &binary_content);

    // ── Load index ────────────────────────────────────────────────────────
    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read().unwrap();

    // ── Verify tier counts ────────────────────────────────────────────────
    let (tier1, tier2, tier3) = index.tier_counts();

    // Tier 1: src/main.rs, src/lib.rs, src/utils/mod.rs, README.md, config.toml = 5
    assert_eq!(
        tier1,
        5,
        "expected 5 Tier-1 (indexed) files, got {tier1}; skipped={:?}",
        index
            .skipped_files()
            .iter()
            .map(|sf| (&sf.path, sf.reason()))
            .collect::<Vec<_>>()
    );

    // Tier 2: 6 denylisted + 1 big + 1 binary = 8
    assert_eq!(
        tier2, 8,
        "expected 8 Tier-2 (MetadataOnly) files, got {tier2}"
    );

    // Tier 3: none created at runtime
    assert_eq!(tier3, 0, "expected 0 Tier-3 (HardSkip) files, got {tier3}");

    // ── Verify symbols come from Tier-1 files only ────────────────────────
    assert!(
        index.symbol_count() > 0,
        "Rust files should produce symbols (main, helper, Config)"
    );

    // None of the skipped files should appear as indexed files
    let skipped_paths: Vec<String> = index
        .skipped_files()
        .iter()
        .map(|sf| sf.path.replace('\\', "/"))
        .collect();

    // Normalise path separators for comparison (index uses forward slashes)
    for path in [
        "models/v1.safetensors",
        "models/v2.ckpt",
        "assets/logo.png",
        "assets/font.woff2",
        "backups/data.sqlite3",
        "release.zip",
        "data/big_config.json",
        "data/custom.dat",
    ] {
        assert!(
            skipped_paths.iter().any(|p: &String| p.ends_with(path)),
            "expected {path} in skipped_files but it was missing; skipped={skipped_paths:?}"
        );
        assert!(
            index.get_file(path).is_none(),
            "skipped file {path} must not appear as an indexed (Tier-1) file"
        );
    }

    // ── Verify skip reasons ───────────────────────────────────────────────
    use tokenizor_agentic_mcp::domain::index::SkipReason;

    // A denylisted extension
    let ckpt = index
        .skipped_files()
        .iter()
        .find(|sf| sf.path.replace('\\', "/").ends_with("models/v2.ckpt"))
        .expect("models/v2.ckpt should be in skipped_files");
    assert_eq!(
        ckpt.reason(),
        Some(SkipReason::DenylistedExtension),
        "models/v2.ckpt should be skipped with DenylistedExtension"
    );

    // The oversized file
    let big = index
        .skipped_files()
        .iter()
        .find(|sf| sf.path.replace('\\', "/").ends_with("data/big_config.json"))
        .expect("data/big_config.json should be in skipped_files");
    assert_eq!(
        big.reason(),
        Some(SkipReason::SizeThreshold),
        "data/big_config.json (1.5 MB) should be skipped with SizeThreshold"
    );

    // The binary file
    let bin = index
        .skipped_files()
        .iter()
        .find(|sf| sf.path.replace('\\', "/").ends_with("data/custom.dat"))
        .expect("data/custom.dat should be in skipped_files");
    assert_eq!(
        bin.reason(),
        Some(SkipReason::BinaryContent),
        "data/custom.dat (contains NUL) should be skipped with BinaryContent"
    );
}

// ---------------------------------------------------------------------------
// Test 2: classify_admission — Tier 3 (HardSkip / SizeCeiling) direct test
//
// We cannot create 150 MB files in tests, so we call classify_admission
// directly to verify the >100 MB ceiling.
// ---------------------------------------------------------------------------

/// Tests classify_admission directly for Tier 3 since we can't create 150 MB
/// files in tests.
#[test]
fn test_admission_tier3_classify_direct() {
    use tokenizor_agentic_mcp::discovery::classify_admission;
    use tokenizor_agentic_mcp::domain::index::{AdmissionTier, SkipReason};

    // Plain text file, but size exceeds 100 MB ceiling → HardSkip
    let decision = classify_admission(Path::new("huge.log"), 150 * 1024 * 1024, None);
    assert_eq!(
        decision.tier,
        AdmissionTier::HardSkip,
        "150 MB file should be HardSkip"
    );
    assert_eq!(
        decision.reason,
        Some(SkipReason::SizeCeiling),
        "150 MB file reason should be SizeCeiling"
    );

    // Denylisted extension AND over ceiling — ceiling wins (checked first)
    let decision = classify_admission(Path::new("big.ckpt"), 4_200_000_000, None);
    assert_eq!(
        decision.tier,
        AdmissionTier::HardSkip,
        "4.2 GB .ckpt should be HardSkip (size ceiling checked before denylist)"
    );
    assert_eq!(
        decision.reason,
        Some(SkipReason::SizeCeiling),
        "4.2 GB .ckpt reason should be SizeCeiling"
    );
}

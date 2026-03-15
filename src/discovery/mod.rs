use std::path::{Path, PathBuf};

use crate::domain::{FileClassification, LanguageId};
use crate::error::Result;

/// A file found during directory traversal that has a recognized language extension.
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    /// Relative path from the root, using forward slashes (e.g., "src/lib.rs").
    pub relative_path: String,
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Language inferred from the file extension.
    pub language: LanguageId,
    /// Deterministic semantic-lane classification captured at discovery time.
    pub classification: FileClassification,
}

/// A file found during a full-filesystem walk (all files, not just known-language ones).
///
/// Used by the admission gate to classify every file — including those with unknown or
/// denylisted extensions — before deciding whether to parse them.
#[derive(Debug, Clone)]
pub struct DiscoveredEntry {
    /// Relative path from the root, using forward slashes.
    pub relative_path: String,
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// File size in bytes from the walk metadata (no extra stat syscall).
    pub file_size: u64,
    /// Language inferred from the extension, if recognized.
    pub language: Option<LanguageId>,
    /// Semantic-lane classification (test/vendor/generated/config flags).
    pub classification: FileClassification,
}

/// Discover all source files under `root` that have a recognized language extension.
///
/// - Respects `.gitignore` files via the `ignore` crate.
/// - Normalizes path separators to `/` in `relative_path`.
/// - Returns files sorted case-insensitively by `relative_path`.
pub fn discover_files(root: &Path) -> Result<Vec<DiscoveredFile>> {
    use ignore::WalkBuilder;

    let mut files: Vec<DiscoveredFile> = WalkBuilder::new(root)
        .build()
        .filter_map(|entry_result| {
            let entry = entry_result.ok()?;
            let path = entry.path().to_path_buf();

            // Use the already-known file_type from the walker instead of
            // path.is_file() which would issue a redundant stat() syscall.
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                return None;
            }

            let ext = path.extension()?.to_str()?;
            let language = LanguageId::from_extension(ext)?;

            // Compute relative path from root
            let relative = path.strip_prefix(root).ok()?;
            // Normalize backslashes to forward slashes
            let relative_path = relative.to_string_lossy().replace('\\', "/");

            Some(DiscoveredFile {
                classification: FileClassification::for_code_path(&relative_path),
                relative_path,
                absolute_path: path,
                language,
            })
        })
        .collect();

    // Sort case-insensitively by relative_path for deterministic ordering
    files.sort_by(|a, b| {
        a.relative_path
            .to_lowercase()
            .cmp(&b.relative_path.to_lowercase())
    });

    Ok(files)
}

/// Discover ALL files under `root` regardless of extension, for admission-gate classification.
///
/// Unlike `discover_files`, this function:
/// - Yields every file (not just known-language ones), so denylisted/binary files are visible.
/// - Captures file size from walk metadata (avoids a separate stat() call).
/// - Sets `language = None` for files with unrecognized extensions.
/// - Returns files sorted case-insensitively by `relative_path`.
pub fn discover_all_files(root: &Path) -> Result<Vec<DiscoveredEntry>> {
    use ignore::WalkBuilder;

    let mut entries: Vec<DiscoveredEntry> = WalkBuilder::new(root)
        .build()
        .filter_map(|entry_result| {
            let entry = entry_result.ok()?;
            let path = entry.path().to_path_buf();

            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                return None;
            }

            // Get file size from the walk metadata (DirEntry has it on most platforms).
            // Fall back to a stat call only when metadata is unavailable.
            let file_size = entry
                .metadata()
                .ok()
                .map(|m| m.len())
                .unwrap_or_else(|| std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));

            // Compute relative path from root
            let relative = path.strip_prefix(root).ok()?;
            let relative_path = relative.to_string_lossy().replace('\\', "/");

            // Attempt language detection; None for unknown/denylisted extensions.
            let language = path
                .extension()
                .and_then(|e| e.to_str())
                .and_then(LanguageId::from_extension);

            let classification = FileClassification::for_code_path(&relative_path);

            Some(DiscoveredEntry {
                relative_path,
                absolute_path: path,
                file_size,
                language,
                classification,
            })
        })
        .collect();

    entries.sort_by(|a, b| {
        a.relative_path
            .to_lowercase()
            .cmp(&b.relative_path.to_lowercase())
    });

    Ok(entries)
}

/// Load all `.gitignore` patterns from a repository root and nested directories.
///
/// Uses `ignore::gitignore::GitignoreBuilder` to build a composite gitignore matcher.
/// Walks nested `.gitignore` files up to `max_depth` levels (default 6).
/// Returns `None` if no `.gitignore` files are found or if loading fails.
pub fn load_gitignore(root: &Path) -> Option<ignore::gitignore::Gitignore> {
    use ignore::gitignore::GitignoreBuilder;
    use std::collections::VecDeque;

    let root_gitignore = root.join(".gitignore");
    if !root_gitignore.exists() {
        return None;
    }

    let mut builder = GitignoreBuilder::new(root);

    // BFS to find nested .gitignore files (max depth 6)
    let max_depth: usize = 6;
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));

    while let Some((dir, depth)) = queue.pop_front() {
        let gitignore_path = dir.join(".gitignore");
        if gitignore_path.is_file() {
            if let Some(err) = builder.add(&gitignore_path) {
                tracing::debug!("failed to load {:?}: {}", gitignore_path, err);
            }
        }

        if depth < max_depth {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // Skip common directories that won't have relevant .gitignore files
                        let name = entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.starts_with('.') && name_str != ".github" {
                            continue;
                        }
                        queue.push_back((path, depth + 1));
                    }
                }
            }
        }
    }

    match builder.build() {
        Ok(gi) => {
            // Only return Some if there are actual patterns
            if gi.is_empty() { None } else { Some(gi) }
        }
        Err(e) => {
            tracing::debug!("failed to build gitignore matcher: {}", e);
            None
        }
    }
}

/// Walk upward from the current working directory, looking for a `.git` directory.
/// Returns `None` if no git root is found and the cwd is a forbidden directory.
pub fn find_project_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Try to find a git root first (always safe — scoped by repo boundary).
    let mut current = cwd.clone();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    // No git root found — use cwd if it's not a forbidden directory.
    if is_forbidden_root(&cwd) {
        tracing::warn!(
            path = %cwd.display(),
            "refusing to auto-index: directory is too broad (home dir, drive root, or system path)"
        );
        None
    } else {
        Some(cwd)
    }
}

/// Returns `true` if `path` is a directory that should never be auto-indexed
/// because it would be too large or contain unrelated files.
fn is_forbidden_root(path: &Path) -> bool {
    // Canonicalize for reliable comparison (resolves symlinks, normalizes separators).
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // 1. Drive roots: C:\, D:\, /, etc.
    if path.parent().is_none() {
        return true;
    }

    // 2. Windows drive roots that have a parent but are still just "C:\"
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        // Matches patterns like "C:\", "\\?\C:\"
        if path_str.len() <= 7 && path_str.ends_with('\\') {
            return true;
        }
    }

    // 3. User home directories.
    if let Some(home) = home_dir() {
        let home = home.canonicalize().unwrap_or(home);
        if path == home {
            return true;
        }
    }

    // 4. Known system/broad directory names.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_lowercase();
        let forbidden_names = [
            "windows",
            "system32",
            "program files",
            "program files (x86)",
            "programdata",
            "appdata",
            "node_modules",
            ".npm",
            ".cargo",
            "users",
            "home",
            "tmp",
            "temp",
            "var",
        ];
        if forbidden_names.contains(&lower.as_str()) {
            return true;
        }
    }

    // 5. Parent-of-home: e.g. C:\Users or /home
    if let Some(home) = home_dir() {
        let home = home.canonicalize().unwrap_or(home);
        if let Some(parent) = home.parent() {
            let parent = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            if path == parent {
                return true;
            }
        }
    }

    false
}

/// Cross-platform home directory lookup.
fn home_dir() -> Option<PathBuf> {
    // std::env::home_dir is deprecated but dirs::home_dir may not be available.
    // Use environment variables directly for reliability.
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// Deprecated: use `find_project_root()` instead.
#[deprecated(note = "use find_project_root() which returns Option and checks forbidden dirs")]
pub fn find_git_root() -> PathBuf {
    find_project_root()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Check if content appears to be binary.
/// Examines up to BINARY_SNIFF_BYTES of the content using three heuristics:
/// 1. NUL byte present -> binary
/// 2. UTF-8 decode failure -> binary
/// 3. >30% suspicious control bytes (excluding \t, \n, \r) -> binary
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
    // Control bytes: 0x01-0x08, 0x0E-0x1F, 0x7F
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

use crate::domain::index::{
    AdmissionDecision, AdmissionTier, HARD_SKIP_BYTES, METADATA_ONLY_BYTES, SkipReason,
};

/// Classify a file's admission tier. Returns AdmissionDecision with both tier and reason.
///
/// Precedence (first match wins):
/// 1. Hard-skip size ceiling (>100MB) → Tier 3
/// 2. Extension denylist → Tier 2
/// 3. Metadata-only size threshold (>1MB) → Tier 2
/// 4. Binary sniff (null bytes in first 8KB) → Tier 2
/// 5. All else → Tier 1
pub fn classify_admission(
    path: &std::path::Path,
    file_size: u64,
    content_sample: Option<&[u8]>,
) -> AdmissionDecision {
    use crate::domain::index::is_denylisted_extension;

    if file_size > HARD_SKIP_BYTES {
        return AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling);
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if is_denylisted_extension(ext) {
            return AdmissionDecision::skip(
                AdmissionTier::MetadataOnly,
                SkipReason::DenylistedExtension,
            );
        }
    }
    if file_size > METADATA_ONLY_BYTES {
        return AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::SizeThreshold);
    }
    if let Some(content) = content_sample {
        if is_binary_content(content) {
            return AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::BinaryContent);
        }
    }
    AdmissionDecision::normal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_discover_files_finds_rs_py_js() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "main.rs", "fn main() {}");
        create_file(tmp.path(), "script.py", "def foo(): pass");
        create_file(tmp.path(), "app.js", "function bar() {}");

        let files = discover_files(tmp.path()).unwrap();
        let extensions: Vec<&str> = files
            .iter()
            .map(|f| f.relative_path.rsplit('.').next().unwrap())
            .collect();

        assert!(extensions.contains(&"rs"), "should find .rs");
        assert!(extensions.contains(&"py"), "should find .py");
        assert!(extensions.contains(&"js"), "should find .js");
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_discover_files_includes_config_files() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "config.json", "{}");
        create_file(tmp.path(), "README.md", "# readme");
        create_file(tmp.path(), "Cargo.toml", "[package]");
        create_file(tmp.path(), "main.rs", "fn main() {}");

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 4, "should discover .rs + .json + .md + .toml");
        let paths: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&"config.json"), "should find .json");
        assert!(paths.contains(&"README.md"), "should find .md");
        assert!(paths.contains(&"Cargo.toml"), "should find .toml");
        assert!(paths.contains(&"main.rs"), "should find .rs");
    }

    #[test]
    fn test_discover_files_respects_gitignore() {
        let tmp = TempDir::new().unwrap();
        // Must create .git dir for gitignore to be respected
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join(".gitignore"), "ignored.rs\n").unwrap();

        create_file(tmp.path(), "main.rs", "fn main() {}");
        create_file(tmp.path(), "ignored.rs", "fn ignored() {}");

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1, "ignored.rs should be excluded");
        assert_eq!(files[0].relative_path, "main.rs");
    }

    #[test]
    fn test_discover_files_normalizes_backslashes() {
        let tmp = TempDir::new().unwrap();
        // Create a file in a subdirectory — the path separator will be OS-native
        create_file(tmp.path(), "src/lib.rs", "pub fn lib() {}");

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        // Must use forward slashes regardless of OS
        assert!(
            !files[0].relative_path.contains('\\'),
            "should have no backslashes: {:?}",
            files[0].relative_path
        );
        assert!(files[0].relative_path.contains('/') || files[0].relative_path == "src/lib.rs");
    }

    #[test]
    fn test_discover_files_deterministic_sorted_order() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "Zoo.rs", "fn zoo() {}");
        create_file(tmp.path(), "apple.rs", "fn apple() {}");
        create_file(tmp.path(), "Mango.rs", "fn mango() {}");

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 3);
        // Case-insensitive alphabetical order
        let names: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        // "apple" < "Mango" < "Zoo" case-insensitively
        let lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        let mut sorted = lower.clone();
        sorted.sort();
        assert_eq!(
            lower, sorted,
            "files should be in case-insensitive sorted order"
        );
    }

    #[test]
    fn test_discover_files_assigns_classification_tags_from_path() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "tests/unit_spec.rs", "fn spec_case() {}");
        create_file(tmp.path(), "vendor/pkg/lib.rs", "fn vendored() {}");
        create_file(
            tmp.path(),
            "src/generated/client.generated.rs",
            "fn generated() {}",
        );

        let files = discover_files(tmp.path()).unwrap();
        let by_path: std::collections::HashMap<&str, &DiscoveredFile> = files
            .iter()
            .map(|file| (file.relative_path.as_str(), file))
            .collect();

        assert!(
            by_path["tests/unit_spec.rs"].classification.is_test,
            "tests path should set is_test"
        );
        assert!(
            by_path["vendor/pkg/lib.rs"].classification.is_vendor,
            "vendor path should set is_vendor"
        );
        assert!(
            by_path["src/generated/client.generated.rs"]
                .classification
                .is_generated,
            "generated path should set is_generated"
        );
    }

    #[test]
    fn test_find_git_root_returns_git_containing_dir() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();

        // Verify the walk-up logic finds .git
        let mut found = false;
        let mut current = tmp.path().to_path_buf();
        loop {
            if current.join(".git").exists() {
                found = true;
                break;
            }
            match current.parent() {
                Some(p) => current = p.to_path_buf(),
                None => break,
            }
        }
        assert!(found, "should find .git directory");
    }

    #[test]
    fn test_is_forbidden_root_blocks_home_dir() {
        let home = home_dir();
        if let Some(h) = home {
            assert!(is_forbidden_root(&h), "home directory should be forbidden");
        }
    }

    #[test]
    fn test_is_forbidden_root_blocks_drive_root() {
        #[cfg(target_os = "windows")]
        assert!(is_forbidden_root(Path::new("C:\\")));
        #[cfg(not(target_os = "windows"))]
        assert!(is_forbidden_root(Path::new("/")));
    }

    #[test]
    fn test_is_forbidden_root_blocks_system_dirs() {
        assert!(is_forbidden_root(Path::new("/tmp")));
        assert!(is_forbidden_root(Path::new("/home")));
    }

    #[test]
    fn test_is_forbidden_root_allows_project_dirs() {
        let tmp = TempDir::new().unwrap();
        assert!(
            !is_forbidden_root(tmp.path()),
            "temp project dir should be allowed"
        );
    }

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
        let content: &[u8] = &[0x80, 0x81, 0x82, 0x83, 0x84];
        assert!(is_binary_content(content));
    }

    #[test]
    fn test_binary_sniff_detects_high_control_ratio() {
        let mut content = Vec::new();
        for _ in 0..80 {
            content.push(0x01); // SOH — control char
        }
        for _ in 0..20 {
            content.push(b'A'); // printable
        }
        // 80% control bytes > 30% threshold -> binary
        assert!(is_binary_content(&content));
    }

    #[test]
    fn test_binary_sniff_allows_low_control_ratio() {
        let content = b"line1\tvalue1\nline2\tvalue2\nline3\tvalue3\n";
        assert!(!is_binary_content(content));
    }

    #[test]
    fn test_binary_sniff_allows_common_whitespace_controls() {
        let content = b"col1\tcol2\tcol3\r\nval1\tval2\tval3\r\n";
        assert!(!is_binary_content(content));
    }

    // ── classify_admission tests ──

    use crate::domain::index::{AdmissionDecision, AdmissionTier, SkipReason};

    #[test]
    fn test_huge_text_file_is_hard_skip() {
        let decision =
            classify_admission(std::path::Path::new("huge.txt"), 150 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::HardSkip);
        assert_eq!(decision.reason, Some(SkipReason::SizeCeiling));
    }

    #[test]
    fn test_small_ckpt_is_metadata_only() {
        let decision = classify_admission(std::path::Path::new("model.ckpt"), 50 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::DenylistedExtension));
    }

    #[test]
    fn test_huge_ckpt_is_hard_skip() {
        let decision = classify_admission(std::path::Path::new("big.ckpt"), 4_200_000_000, None);
        assert_eq!(decision.tier, AdmissionTier::HardSkip);
        assert_eq!(decision.reason, Some(SkipReason::SizeCeiling));
    }

    #[test]
    fn test_large_json_is_metadata_only() {
        let decision = classify_admission(std::path::Path::new("big.json"), 2 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::SizeThreshold));
    }

    #[test]
    fn test_small_txt_is_normal() {
        let decision = classify_admission(std::path::Path::new("readme.txt"), 50 * 1024, None);
        assert_eq!(decision, AdmissionDecision::normal());
    }

    #[test]
    fn test_medium_rust_source_is_normal() {
        let decision = classify_admission(std::path::Path::new("big_module.rs"), 500 * 1024, None);
        assert_eq!(decision, AdmissionDecision::normal());
    }

    #[test]
    fn test_binary_content_is_metadata_only() {
        let content = b"ELF\x00\x00\x00binary";
        let decision =
            classify_admission(std::path::Path::new("unknown_file"), 1024, Some(content));
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::BinaryContent));
    }

    #[test]
    fn test_svg_not_denylisted() {
        let decision = classify_admission(std::path::Path::new("icon.svg"), 50 * 1024, None);
        assert_eq!(decision, AdmissionDecision::normal());
    }

    #[test]
    fn test_large_svg_is_metadata_only_by_size() {
        let decision = classify_admission(std::path::Path::new("huge.svg"), 2 * 1024 * 1024, None);
        assert_eq!(decision.tier, AdmissionTier::MetadataOnly);
        assert_eq!(decision.reason, Some(SkipReason::SizeThreshold));
    }

    // ── discover_all_files + admission gate integration tests ──

    #[test]
    fn test_discovery_skips_denylisted_extension() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "main.rs", "fn main() {}");
        // Write a fake .safetensors file (extension is on the denylist)
        fs::write(tmp.path().join("model.safetensors"), b"fake model bytes").unwrap();

        let entries = discover_all_files(tmp.path()).unwrap();

        // Classify each entry and collect skipped ones
        let mut rs_found = false;
        let mut safetensors_skipped = false;
        let mut safetensors_reason = None;

        for entry in &entries {
            let size = entry.file_size;
            let decision = classify_admission(&entry.absolute_path, size, None);
            if entry.relative_path == "main.rs" {
                assert_eq!(
                    decision.tier,
                    AdmissionTier::Normal,
                    ".rs file should be Normal"
                );
                rs_found = true;
            }
            if entry.relative_path == "model.safetensors" {
                assert_eq!(
                    decision.tier,
                    AdmissionTier::MetadataOnly,
                    ".safetensors should be MetadataOnly"
                );
                safetensors_skipped = true;
                safetensors_reason = decision.reason;
            }
        }

        assert!(rs_found, ".rs file must appear in discovered entries");
        assert!(
            safetensors_skipped,
            ".safetensors must appear in discovered entries and be skipped"
        );
        assert_eq!(
            safetensors_reason,
            Some(SkipReason::DenylistedExtension),
            ".safetensors skip reason must be DenylistedExtension"
        );
    }

    #[test]
    fn test_discovery_deferred_binary_sniff_reclassifies() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "lib.rs", "pub fn hello() {}");

        // Write a .dat file with NUL-heavy content: not on denylist, under 1MB,
        // but binary sniff (NUL bytes) should reclassify to MetadataOnly.
        let mut binary_content = vec![0u8; 512]; // NUL bytes — triggers binary sniff
        binary_content.extend_from_slice(b"some trailing text");
        fs::write(tmp.path().join("custom.dat"), &binary_content).unwrap();

        let entries = discover_all_files(tmp.path()).unwrap();

        let mut rs_normal = false;
        let mut dat_skipped = false;
        let mut dat_reason = None;

        for entry in &entries {
            let size = entry.file_size;

            // Phase 1: pre-content check
            let pre = classify_admission(&entry.absolute_path, size, None);

            if entry.relative_path == "lib.rs" {
                assert_eq!(pre.tier, AdmissionTier::Normal);
                rs_normal = true;
            }

            if entry.relative_path == "custom.dat" {
                // Pre-content: should be Normal (not denylisted, under 1MB)
                assert_eq!(
                    pre.tier,
                    AdmissionTier::Normal,
                    "custom.dat should be Normal before binary sniff"
                );

                // Phase 2: with content — binary sniff should reclassify
                let content = fs::read(&entry.absolute_path).unwrap();
                let post = classify_admission(&entry.absolute_path, size, Some(&content));
                assert_eq!(
                    post.tier,
                    AdmissionTier::MetadataOnly,
                    "custom.dat should be MetadataOnly after binary sniff"
                );
                dat_skipped = true;
                dat_reason = post.reason;
            }
        }

        assert!(rs_normal, "lib.rs must be Normal");
        assert!(
            dat_skipped,
            "custom.dat must be discovered and reclassified"
        );
        assert_eq!(
            dat_reason,
            Some(SkipReason::BinaryContent),
            "custom.dat skip reason must be BinaryContent"
        );
    }
}

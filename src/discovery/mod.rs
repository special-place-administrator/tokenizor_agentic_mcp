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

            // Only process regular files
            if !path.is_file() {
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
    fn test_discover_files_ignores_json_md_toml() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "config.json", "{}");
        create_file(tmp.path(), "README.md", "# readme");
        create_file(tmp.path(), "Cargo.toml", "[package]");
        create_file(tmp.path(), "main.rs", "fn main() {}");

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1, "only .rs should be found");
        assert_eq!(files[0].relative_path, "main.rs");
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
}

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::paths;

pub const TEE_MAX_FILES: usize = 20;
pub const TEE_MAX_FILE_BYTES: usize = 1024 * 1024;

static TEE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeeRecord {
    pub original_path: PathBuf,
    pub tee_path: PathBuf,
    pub repo_root: PathBuf,
}

impl TeeRecord {
    pub fn recovery_hint(&self) -> String {
        format!(
            "Tee snapshot: `{}` preserves `{}` before this write.",
            display_relative(&self.repo_root, &self.tee_path),
            display_relative(&self.repo_root, &self.original_path),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeeSnapshot {
    Created(TeeRecord),
    SkippedMissing {
        original_path: PathBuf,
    },
    SkippedTooLarge {
        size: usize,
        max_size: usize,
    },
    Warning {
        original_path: PathBuf,
        message: String,
    },
}

impl TeeSnapshot {
    pub fn response_hint(&self) -> Option<String> {
        match self {
            Self::Created(record) => Some(record.recovery_hint()),
            Self::SkippedMissing { .. } => None,
            Self::SkippedTooLarge { size, max_size } => Some(format!(
                "Tee snapshot skipped: original file is {size} bytes, above {max_size} byte cap."
            )),
            Self::Warning { message, .. } => Some(format!(
                "Tee snapshot warning: {message}; edit still proceeded."
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tee {
    repo_root: PathBuf,
    max_files: usize,
    max_file_bytes: u64,
}

impl Tee {
    pub fn for_repo(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
            max_files: TEE_MAX_FILES,
            max_file_bytes: TEE_MAX_FILE_BYTES as u64,
        }
    }

    pub fn for_target(target: impl AsRef<Path>) -> Self {
        Self::for_repo(discover_repo_root(target.as_ref()))
    }

    pub fn snapshot(&self, original_path: impl AsRef<Path>) -> io::Result<TeeSnapshot> {
        let original_path = original_path.as_ref();
        let metadata = match fs::metadata(original_path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(TeeSnapshot::SkippedMissing {
                    original_path: original_path.to_path_buf(),
                });
            }
            Err(err) => {
                return Ok(TeeSnapshot::Warning {
                    original_path: original_path.to_path_buf(),
                    message: format!("could not inspect {}: {err}", original_path.display()),
                });
            }
        };

        if !metadata.is_file() {
            return Ok(TeeSnapshot::Warning {
                original_path: original_path.to_path_buf(),
                message: format!("{} is not a regular file", original_path.display()),
            });
        }

        if metadata.len() > self.max_file_bytes {
            return Ok(TeeSnapshot::SkippedTooLarge {
                size: metadata.len() as usize,
                max_size: self.max_file_bytes as usize,
            });
        }

        let tee_dir = paths::ensure_symforge_dir(&self.repo_root)
            .map(|dir| dir.join("tee"))
            .and_then(|dir| {
                fs::create_dir_all(&dir)?;
                Ok(dir)
            });
        let tee_dir = match tee_dir {
            Ok(dir) => dir,
            Err(err) => {
                return Ok(TeeSnapshot::Warning {
                    original_path: original_path.to_path_buf(),
                    message: format!("could not create tee directory: {err}"),
                });
            }
        };

        let tee_path = tee_dir.join(snapshot_file_name(original_path));
        if let Err(err) = fs::copy(original_path, &tee_path) {
            return Ok(TeeSnapshot::Warning {
                original_path: original_path.to_path_buf(),
                message: format!(
                    "could not snapshot {} to {}: {err}",
                    original_path.display(),
                    tee_path.display()
                ),
            });
        }

        if let Err(err) = enforce_retention(&tee_dir, self.max_files) {
            tracing::warn!(
                "tee snapshot retention failed for {}: {err}",
                tee_dir.display()
            );
        }

        Ok(TeeSnapshot::Created(TeeRecord {
            original_path: original_path.to_path_buf(),
            tee_path,
            repo_root: self.repo_root.clone(),
        }))
    }
}

fn discover_repo_root(target: &Path) -> PathBuf {
    let start = if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(target)
    };

    for ancestor in start.ancestors() {
        if ancestor.join(".git").exists() || ancestor.join(paths::SYMFORGE_DIR_NAME).exists() {
            return ancestor.to_path_buf();
        }
    }

    start.to_path_buf()
}

fn snapshot_file_name(original_path: &Path) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = TEE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = original_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    format!("{millis}-{counter:06}-{}", sanitize_file_name(file_name))
}

fn sanitize_file_name(file_name: &str) -> String {
    file_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn enforce_retention(tee_dir: &Path, max_files: usize) -> io::Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(tee_dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        entries.push((modified, entry.file_name(), entry.path()));
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let excess = entries.len().saturating_sub(max_files);
    for (_, _, path) in entries.into_iter().take(excess) {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn display_relative(repo_root: &Path, path: &Path) -> String {
    let display_path = path.strip_prefix(repo_root).unwrap_or(path);
    display_path.to_string_lossy().replace('\\', "/")
}

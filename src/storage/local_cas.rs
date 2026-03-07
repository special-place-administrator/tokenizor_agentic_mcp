use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::BlobStoreConfig;
use crate::domain::{ComponentHealth, HealthIssueCategory};
use crate::error::{Result, TokenizorError};

use super::blob::{BlobStore, StoredBlob};
use super::sha256;

#[derive(Clone, Debug)]
pub struct LocalCasBlobStore {
    root_dir: PathBuf,
    blobs_dir: PathBuf,
    temp_dir: PathBuf,
    quarantine_dir: PathBuf,
    derived_dir: PathBuf,
}

impl LocalCasBlobStore {
    pub fn new(config: BlobStoreConfig) -> Self {
        let root_dir = config.root_dir;
        Self {
            blobs_dir: root_dir.join("blobs").join("sha256"),
            temp_dir: root_dir.join("temp"),
            quarantine_dir: root_dir.join("quarantine"),
            derived_dir: root_dir.join("derived"),
            root_dir,
        }
    }

    fn create_layout(&self) -> Result<()> {
        self.create_dir(&self.blobs_dir)?;
        self.create_dir(&self.temp_dir)?;
        self.create_dir(&self.quarantine_dir)?;
        self.create_dir(&self.derived_dir)?;
        Ok(())
    }

    fn create_dir(&self, path: &Path) -> Result<()> {
        fs::create_dir_all(path).map_err(|error| TokenizorError::io(path, error))
    }

    fn probe_writeability(&self, path: &Path) -> Result<()> {
        let probe_path = path.join(format!(
            ".tokenizor-write-probe-{}-{}",
            std::process::id(),
            crate::domain::unix_timestamp_ms()
        ));

        let result = (|| -> Result<()> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&probe_path)
                .map_err(|error| TokenizorError::io(&probe_path, error))?;
            file.write_all(b"probe")
                .map_err(|error| TokenizorError::io(&probe_path, error))?;
            file.sync_all()
                .map_err(|error| TokenizorError::io(&probe_path, error))?;
            Ok(())
        })();

        let remove_result = fs::remove_file(&probe_path);

        match (result, remove_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            (Ok(()), Err(error)) => Err(TokenizorError::io(&probe_path, error)),
        }
    }

    fn blob_path(&self, blob_id: &str) -> Result<PathBuf> {
        if blob_id.len() != 64
            || !blob_id
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(TokenizorError::InvalidArgument(format!(
                "blob id `{blob_id}` is not a valid sha256 digest"
            )));
        }

        Ok(self
            .blobs_dir
            .join(&blob_id[..2])
            .join(&blob_id[2..4])
            .join(blob_id))
    }

    fn temp_blob_path(&self, blob_id: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        self.temp_dir
            .join(format!("{blob_id}.{timestamp}.{}.tmp", std::process::id()))
    }
}

impl BlobStore for LocalCasBlobStore {
    fn backend_name(&self) -> &'static str {
        "local_cas"
    }

    fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    fn initialize(&self) -> Result<ComponentHealth> {
        self.create_layout()?;
        Ok(ComponentHealth::ok(
            "blob_store",
            HealthIssueCategory::Storage,
            format!("local CAS layout is ready at {}", self.root_dir.display()),
        ))
    }

    fn health_check(&self) -> Result<ComponentHealth> {
        let layout_paths = [
            ("root", self.root_dir.as_path()),
            ("blobs", self.blobs_dir.as_path()),
            ("temp", self.temp_dir.as_path()),
            ("quarantine", self.quarantine_dir.as_path()),
            ("derived", self.derived_dir.as_path()),
        ];

        let missing = layout_paths
            .iter()
            .filter_map(|(name, path)| (!path.exists()).then_some(*name))
            .collect::<Vec<_>>();

        if !missing.is_empty() {
            return Ok(ComponentHealth::error(
                "blob_store",
                HealthIssueCategory::Storage,
                format!(
                    "local CAS is missing required directories at {}: {}",
                    self.root_dir.display(),
                    missing.join(", ")
                ),
                "Run `tokenizor_agentic_mcp init` to create the CAS layout before starting the runtime.",
            ));
        }

        let unwritable = layout_paths
            .iter()
            .filter_map(|(name, path)| {
                self.probe_writeability(path)
                    .err()
                    .map(|error| format!("{name} ({error})"))
            })
            .collect::<Vec<_>>();

        if !unwritable.is_empty() {
            return Ok(ComponentHealth::error(
                "blob_store",
                HealthIssueCategory::Storage,
                format!(
                    "local CAS layout is not writable at {}: {}",
                    self.root_dir.display(),
                    unwritable.join(", ")
                ),
                "Ensure TOKENIZOR_BLOB_ROOT and the CAS layout directories are writable by the current user, or choose a different writable path and run `tokenizor_agentic_mcp init` again.",
            ));
        }

        Ok(ComponentHealth::ok(
            "blob_store",
            HealthIssueCategory::Storage,
            format!("local CAS layout is ready at {}", self.root_dir.display()),
        ))
    }

    fn store_bytes(&self, bytes: &[u8]) -> Result<StoredBlob> {
        self.create_layout()?;

        let blob_id = sha256::digest_hex(bytes);
        let final_path = self.blob_path(&blob_id)?;

        if final_path.exists() {
            return Ok(StoredBlob {
                blob_id,
                byte_len: bytes.len() as u64,
                was_created: false,
            });
        }

        let parent = final_path.parent().ok_or_else(|| {
            TokenizorError::Storage("blob path is missing a parent directory".into())
        })?;
        self.create_dir(parent)?;

        let temp_path = self.temp_blob_path(&blob_id);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|error| TokenizorError::io(&temp_path, error))?;

        let write_result = (|| -> Result<()> {
            file.write_all(bytes)
                .map_err(|error| TokenizorError::io(&temp_path, error))?;
            file.sync_all()
                .map_err(|error| TokenizorError::io(&temp_path, error))?;
            Ok(())
        })();

        if let Err(error) = write_result {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }

        match fs::rename(&temp_path, &final_path) {
            Ok(()) => Ok(StoredBlob {
                blob_id,
                byte_len: bytes.len() as u64,
                was_created: true,
            }),
            Err(_) if final_path.exists() => {
                let _ = fs::remove_file(&temp_path);
                Ok(StoredBlob {
                    blob_id,
                    byte_len: bytes.len() as u64,
                    was_created: false,
                })
            }
            Err(error) => {
                let _ = fs::remove_file(&temp_path);
                Err(TokenizorError::io(&final_path, error))
            }
        }
    }

    fn read_bytes(&self, blob_id: &str) -> Result<Vec<u8>> {
        let path = self.blob_path(blob_id)?;
        fs::read(&path).map_err(|error| TokenizorError::io(path, error))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::config::BlobStoreConfig;
    use crate::domain::{HealthIssueCategory, HealthSeverity, HealthStatus};

    use super::{BlobStore, LocalCasBlobStore};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "tokenizor-{name}-{}-{}",
                std::process::id(),
                crate::domain::unix_timestamp_ms()
            ));
            fs::create_dir_all(&path).expect("test directory should be created");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn preserves_exact_bytes_including_crlf_and_nul() {
        let test_dir = TestDir::new("local-cas-byte-exact");
        let store = LocalCasBlobStore::new(BlobStoreConfig {
            root_dir: test_dir.path.clone(),
        });
        let original = b"alpha\r\nbeta\0gamma\r\n";

        let stored = store
            .store_bytes(original)
            .expect("blob storage should succeed");
        let round_trip = store
            .read_bytes(&stored.blob_id)
            .expect("blob read should succeed");

        assert_eq!(round_trip, original);
    }

    #[test]
    fn reuses_existing_blob_for_identical_content() {
        let test_dir = TestDir::new("local-cas-idempotent");
        let store = LocalCasBlobStore::new(BlobStoreConfig {
            root_dir: test_dir.path.clone(),
        });

        let first = store
            .store_bytes(b"same bytes")
            .expect("first blob write should succeed");
        let second = store
            .store_bytes(b"same bytes")
            .expect("second blob write should succeed");

        assert!(first.was_created);
        assert!(!second.was_created);
        assert_eq!(first.blob_id, second.blob_id);
    }

    #[test]
    fn health_check_reports_missing_directories_without_creating_them() {
        let test_dir = TestDir::new("local-cas-health-check");
        let store = LocalCasBlobStore::new(BlobStoreConfig {
            root_dir: test_dir.path.clone(),
        });
        let blobs_dir = test_dir.path.join("blobs");

        assert!(!blobs_dir.exists());

        let report = store
            .health_check()
            .expect("health check should report missing directories");

        assert!(!blobs_dir.exists());
        assert_eq!(report.status, HealthStatus::Unavailable);
        assert_eq!(report.category, HealthIssueCategory::Storage);
        assert_eq!(report.severity, HealthSeverity::Error);
        assert!(
            report
                .remediation
                .as_deref()
                .expect("remediation should be present")
                .contains("tokenizor_agentic_mcp init")
        );
    }

    #[test]
    fn health_check_reports_non_writable_layout_entries() {
        let test_dir = TestDir::new("local-cas-write-probe");
        let store = LocalCasBlobStore::new(BlobStoreConfig {
            root_dir: test_dir.path.clone(),
        });

        fs::create_dir_all(test_dir.path.join("blobs")).expect("blobs parent should be created");
        fs::write(
            test_dir.path.join("blobs").join("sha256"),
            b"not a directory",
        )
        .expect("file should be created in place of blobs/sha256");
        fs::create_dir_all(test_dir.path.join("temp")).expect("temp dir should be created");
        fs::create_dir_all(test_dir.path.join("quarantine"))
            .expect("quarantine dir should be created");
        fs::create_dir_all(test_dir.path.join("derived")).expect("derived dir should be created");

        let report = store
            .health_check()
            .expect("health check should report non-writable layout entries");

        assert_eq!(report.status, HealthStatus::Unavailable);
        assert_eq!(report.category, HealthIssueCategory::Storage);
        assert_eq!(report.severity, HealthSeverity::Error);
        assert!(report.detail.contains("not writable"));
        assert!(report.detail.contains("blobs"));
        assert!(
            report
                .remediation
                .as_deref()
                .expect("remediation should be present")
                .contains("writable by the current user")
        );
    }
}

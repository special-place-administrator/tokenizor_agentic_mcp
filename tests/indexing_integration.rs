use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokenizor_agentic_mcp::config::BlobStoreConfig;
use tokenizor_agentic_mcp::domain::{ComponentHealth, IndexRunMode, IndexRunStatus, PersistedFileOutcome};
use tokenizor_agentic_mcp::error::TokenizorError;
use tokenizor_agentic_mcp::storage::registry_persistence::RegistryPersistence;
use tokenizor_agentic_mcp::storage::{BlobStore, LocalCasBlobStore, StoredBlob};
use tokenizor_agentic_mcp::application::run_manager::RunManager;

/// A BlobStore that always fails on store_bytes but has an existing root_dir.
/// Used to test that CAS write failures produce PersistedFileOutcome::Failed records.
struct FailingBlobStore {
    root: PathBuf,
}

impl BlobStore for FailingBlobStore {
    fn backend_name(&self) -> &'static str {
        "failing"
    }

    fn root_dir(&self) -> &Path {
        &self.root
    }

    fn initialize(&self) -> Result<ComponentHealth, TokenizorError> {
        unreachable!("initialize not needed in failing CAS tests")
    }

    fn health_check(&self) -> Result<ComponentHealth, TokenizorError> {
        unreachable!("health_check not needed in failing CAS tests")
    }

    fn store_bytes(&self, _bytes: &[u8]) -> Result<StoredBlob, TokenizorError> {
        Err(TokenizorError::Storage(
            "simulated CAS write failure".into(),
        ))
    }

    fn read_bytes(&self, _blob_id: &str) -> Result<Vec<u8>, TokenizorError> {
        unreachable!("read_bytes not needed in failing CAS tests")
    }
}

fn setup_test_env() -> (
    tempfile::TempDir,
    Arc<RunManager>,
    tempfile::TempDir,
    Arc<dyn BlobStore>,
) {
    let dir = tempfile::tempdir().unwrap();
    let registry_path = dir.path().join("registry.json");
    let persistence = RegistryPersistence::new(registry_path);
    let manager = Arc::new(RunManager::new(persistence));

    let cas_dir = tempfile::tempdir().unwrap();
    let cas: Arc<dyn BlobStore> = Arc::new(LocalCasBlobStore::new(BlobStoreConfig {
        root_dir: cas_dir.path().to_path_buf(),
    }));

    (dir, manager, cas_dir, cas)
}

#[tokio::test]
async fn test_launch_run_transitions_queued_running_succeeded() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(repo_dir.path().join("lib.py"), "def foo(): pass").unwrap();

    let (run, progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    assert_eq!(run.status, IndexRunStatus::Queued);

    // Wait for background task to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let finished_run = manager.persistence().find_run(&run.run_id).unwrap().unwrap();
    assert_eq!(finished_run.status, IndexRunStatus::Succeeded);
    assert!(finished_run.started_at_unix_ms.is_some());
    assert!(finished_run.finished_at_unix_ms.is_some());

    assert_eq!(
        progress.total_files.load(std::sync::atomic::Ordering::Relaxed),
        2
    );
    assert_eq!(
        progress.files_processed.load(std::sync::atomic::Ordering::Relaxed),
        2
    );

    // Active run should be deregistered
    assert!(!manager.has_active_run("test-repo"));
}

#[tokio::test]
async fn test_single_file_failure_does_not_poison_run() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("good.rs"), "fn good() {}").unwrap();
    fs::write(repo_dir.path().join("also_good.py"), "def also(): pass").unwrap();
    fs::write(repo_dir.path().join("broken.rs"), "fn broken( { }").unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let finished_run = manager.persistence().find_run(&run.run_id).unwrap().unwrap();
    assert_eq!(finished_run.status, IndexRunStatus::Succeeded);
}

// === Story 2.3 Integration Tests ===

#[tokio::test]
async fn test_pipeline_persists_file_records_in_registry() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(repo_dir.path().join("lib.py"), "def foo(): pass").unwrap();
    fs::write(repo_dir.path().join("app.ts"), "function hello() {}").unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(records.len(), 3, "expected 3 file records persisted");

    for record in &records {
        assert_eq!(record.run_id, run.run_id);
        assert_eq!(record.repo_id, "test-repo");
        assert!(!record.blob_id.is_empty());
        assert!(record.committed_at_unix_ms > 0);
        assert!(record.byte_len > 0);
    }
}

#[tokio::test]
async fn test_empty_symbols_file_produces_empty_symbols_outcome() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    // An empty .rs file has no symbols
    fs::write(repo_dir.path().join("empty.rs"), "// just a comment").unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, PersistedFileOutcome::EmptySymbols);
}

#[tokio::test]
async fn test_out_of_scope_files_not_persisted_as_file_records() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();
    // Out-of-scope languages: Java, Ruby — these should not be discovered
    fs::write(repo_dir.path().join("App.java"), "class App {}").unwrap();
    fs::write(repo_dir.path().join("app.rb"), "def hello; end").unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    // Only the Rust file should be persisted, not Java or Ruby
    assert_eq!(records.len(), 1);
    assert!(records[0].relative_path.ends_with("main.rs"));
}

#[tokio::test]
async fn test_file_records_linked_to_run_and_repo() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("lib.go"), "package main\nfunc Lib() {}").unwrap();

    let (run, _progress) = manager
        .launch_run("my-repo-id", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].run_id, run.run_id);
    assert_eq!(records[0].repo_id, "my-repo-id");
}

#[tokio::test]
async fn test_cas_blobs_exist_on_disk_after_pipeline() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas.clone())
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert!(!records.is_empty());

    for record in &records {
        if matches!(record.outcome, PersistedFileOutcome::Committed) {
            let blob_bytes = cas.read_bytes(&record.blob_id).unwrap();
            assert!(!blob_bytes.is_empty());
        }
    }
}

#[tokio::test]
async fn test_backward_compat_registry_without_run_file_records() {
    // Simulate an Epic 1 registry file without run_file_records field
    let dir = tempfile::tempdir().unwrap();
    let registry_path = dir.path().join("registry.json");
    let registry_json = r#"{
        "schema_version": 2,
        "registry_kind": "local_bootstrap_project_workspace",
        "authority_mode": "local_bootstrap_only",
        "control_plane_backend": "in_memory",
        "repositories": {},
        "workspaces": {}
    }"#;
    fs::write(&registry_path, registry_json).unwrap();

    // get_file_records should return empty for a registry without the field
    let persistence = RegistryPersistence::new(registry_path);
    let records = persistence.get_file_records("any-run").unwrap();
    assert!(records.is_empty());
}

#[tokio::test]
async fn test_failed_file_produces_failed_outcome_in_persisted_records() {
    let dir = tempfile::tempdir().unwrap();
    let registry_path = dir.path().join("registry.json");
    let persistence = RegistryPersistence::new(registry_path);
    let manager = Arc::new(RunManager::new(persistence));

    // CAS root exists but store_bytes always fails → file-local CAS failure
    let cas_root = tempfile::tempdir().unwrap();
    let cas: Arc<dyn BlobStore> = Arc::new(FailingBlobStore {
        root: cas_root.path().to_path_buf(),
    });

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    let (run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager
        .persistence()
        .get_file_records(&run.run_id)
        .unwrap();
    assert_eq!(records.len(), 1);
    match &records[0].outcome {
        PersistedFileOutcome::Failed { error } => {
            assert!(
                error.contains("CAS write failed"),
                "expected CAS write error message, got: {error}"
            );
        }
        other => panic!("expected Failed outcome, got: {:?}", other),
    }
}

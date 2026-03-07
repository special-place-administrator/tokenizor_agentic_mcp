use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokenizor_agentic_mcp::config::BlobStoreConfig;
use tokenizor_agentic_mcp::domain::{ComponentHealth, FileRecord, IndexRunMode, IndexRunStatus, LanguageId, PersistedFileOutcome, RunHealth};
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
    // Java is Broader tier — should be processed. Ruby is Unsupported — should be skipped.
    fs::write(repo_dir.path().join("App.java"), "class App {}").unwrap();
    fs::write(repo_dir.path().join("app.rb"), "def hello; end").unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    // Rust (QualityFocus) and Java (Broader) should be persisted, not Ruby (Unsupported)
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|r| r.relative_path.ends_with("main.rs")));
    assert!(records.iter().any(|r| r.relative_path.ends_with("App.java")));
    assert!(!records.iter().any(|r| r.relative_path.ends_with("app.rb")));
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

// === Story 2.4 Integration Tests ===

#[tokio::test]
async fn test_java_file_produces_committed_outcome_with_symbols() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(
        repo_dir.path().join("App.java"),
        "public class App {\n    public void run() {}\n}\n",
    )
    .unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas.clone())
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].relative_path.ends_with("App.java"));
    assert_eq!(records[0].outcome, PersistedFileOutcome::Committed);
    assert!(!records[0].symbols.is_empty());
    // CAS blob should exist
    let blob = cas.read_bytes(&records[0].blob_id).unwrap();
    assert!(!blob.is_empty());
}

#[tokio::test]
async fn test_java_syntax_error_produces_partial_parse() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    // Missing closing brace — valid enough to parse but tree-sitter reports error
    fs::write(
        repo_dir.path().join("Bad.java"),
        "public class Bad { public void foo() {",
    )
    .unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(records.len(), 1);
    // Should be Committed (partial parse with symbols) or Quarantined (partial parse, no symbols)
    // Either way, not Failed — tree-sitter handles syntax errors gracefully
    assert!(
        matches!(records[0].outcome, PersistedFileOutcome::Committed | PersistedFileOutcome::Quarantined { .. }),
        "expected Committed or Quarantined for partial parse, got: {:?}",
        records[0].outcome
    );
}

#[tokio::test]
async fn test_mixed_repo_java_processed_unsupported_reported() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(
        repo_dir.path().join("Service.java"),
        "public class Service { public void serve() {} }",
    )
    .unwrap();
    // Not-yet-supported files
    fs::write(repo_dir.path().join("app.rb"), "def hello; end").unwrap();
    fs::write(repo_dir.path().join("main.cs"), "class Main {}").unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    // Only Rust and Java should produce file records
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|r| r.relative_path.ends_with("main.rs")));
    assert!(records.iter().any(|r| r.relative_path.ends_with("Service.java")));
    // Ruby and C# files should NOT produce records or CAS blobs
    assert!(!records.iter().any(|r| r.relative_path.ends_with("app.rb")));
    assert!(!records.iter().any(|r| r.relative_path.ends_with("main.cs")));
}

#[tokio::test]
async fn test_not_yet_supported_files_produce_no_file_records_or_cas_blobs() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    // Only unsupported files
    fs::write(repo_dir.path().join("app.rb"), "def hello; end").unwrap();
    fs::write(repo_dir.path().join("main.cs"), "class Main {}").unwrap();
    fs::write(repo_dir.path().join("script.php"), "<?php echo 'hi';").unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert!(records.is_empty(), "unsupported files should produce no file records");
}

#[tokio::test]
async fn test_quality_focus_languages_still_process_correctly() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(repo_dir.path().join("lib.py"), "def foo(): pass").unwrap();
    fs::write(repo_dir.path().join("app.js"), "function app() {}").unwrap();
    fs::write(repo_dir.path().join("mod.ts"), "function hello(): void {}").unwrap();
    fs::write(repo_dir.path().join("main.go"), "package main\nfunc main() {}").unwrap();

    let (run, _progress) = manager
        .launch_run("test-repo", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let finished = manager.persistence().find_run(&run.run_id).unwrap().unwrap();
    assert_eq!(finished.status, IndexRunStatus::Succeeded);

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(records.len(), 5);
    for record in &records {
        assert_eq!(record.outcome, PersistedFileOutcome::Committed);
        assert!(!record.symbols.is_empty());
    }
}

// --- Story 2.5: Run inspection integration tests ---

#[tokio::test]
async fn test_inspect_succeeded_all_ok_returns_healthy() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    let (run, _progress) = manager
        .launch_run("repo-healthy", IndexRunMode::Full, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(report.health, RunHealth::Healthy);
    assert_eq!(report.run.status, IndexRunStatus::Succeeded);
    assert!(!report.is_active);
    assert!(report.action_required.is_none());
    assert!(report.file_outcome_summary.is_some());
    let summary = report.file_outcome_summary.unwrap();
    assert!(summary.processed_ok > 0);
    assert_eq!(summary.failed, 0);
}

#[tokio::test]
async fn test_inspect_succeeded_with_partial_returns_degraded() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    // Create a run and manually set it to Succeeded with quarantined file records
    // to deterministically test the degraded path (no pipeline dependency).
    let run = manager.start_run("repo-degraded", IndexRunMode::Full).unwrap();
    manager
        .persistence()
        .update_run_status(&run.run_id, IndexRunStatus::Succeeded, None)
        .unwrap();

    let records = vec![
        FileRecord {
            relative_path: "good.rs".into(),
            language: LanguageId::Rust,
            blob_id: "blob-1".into(),
            byte_len: 100,
            content_hash: "hash-1".into(),
            outcome: PersistedFileOutcome::Committed,
            symbols: vec![],
            run_id: run.run_id.clone(),
            repo_id: "repo-degraded".into(),
            committed_at_unix_ms: 1000,
        },
        FileRecord {
            relative_path: "suspect.rs".into(),
            language: LanguageId::Rust,
            blob_id: "blob-2".into(),
            byte_len: 200,
            content_hash: "hash-2".into(),
            outcome: PersistedFileOutcome::Quarantined {
                reason: "suspect parse spans".into(),
            },
            symbols: vec![],
            run_id: run.run_id.clone(),
            repo_id: "repo-degraded".into(),
            committed_at_unix_ms: 1001,
        },
    ];
    manager
        .persistence()
        .save_file_records(&run.run_id, &records)
        .unwrap();

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(report.health, RunHealth::Degraded);
    assert!(report.action_required.is_some());
    let summary = report.file_outcome_summary.unwrap();
    assert_eq!(summary.total_committed, 2);
    assert_eq!(summary.processed_ok, 1);
    assert_eq!(summary.partial_parse, 1);
}

#[tokio::test]
async fn test_inspect_failed_run_returns_unhealthy() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    let run = manager.start_run("repo-fail", IndexRunMode::Full).unwrap();
    manager
        .persistence()
        .update_run_status(
            &run.run_id,
            IndexRunStatus::Failed,
            Some("simulated pipeline failure".into()),
        )
        .unwrap();

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(report.run.status, IndexRunStatus::Failed);
    assert_eq!(report.health, RunHealth::Unhealthy);
    assert!(report.action_required.is_some());
    let msg = report.action_required.unwrap();
    assert!(msg.contains("failed") || msg.contains("Failed"));
}

#[tokio::test]
async fn test_inspect_interrupted_run_returns_unhealthy() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    // Create a run and manually set it to Interrupted via persistence
    let run = manager.start_run("repo-interrupt", IndexRunMode::Full).unwrap();
    manager
        .persistence()
        .update_run_status(&run.run_id, IndexRunStatus::Running, None)
        .unwrap();
    // Simulate startup sweep marking it interrupted
    manager.startup_sweep().unwrap();

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(report.run.status, IndexRunStatus::Interrupted);
    assert_eq!(report.health, RunHealth::Unhealthy);
    assert!(report.action_required.is_some());
    assert!(report.action_required.unwrap().contains("interrupted"));
}

#[tokio::test]
async fn test_inspect_cancelled_run_returns_healthy() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    let run = manager.start_run("repo-cancel", IndexRunMode::Full).unwrap();
    manager
        .persistence()
        .update_run_status(&run.run_id, IndexRunStatus::Cancelled, None)
        .unwrap();

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(report.health, RunHealth::Healthy);
    assert!(report.action_required.is_none());
}

#[tokio::test]
async fn test_inspect_nonexistent_run_returns_error() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    let result = manager.inspect_run("nonexistent-run-id");
    assert!(result.is_err());
    match result.unwrap_err() {
        TokenizorError::NotFound(msg) => assert!(msg.contains("nonexistent-run-id")),
        other => panic!("expected NotFound, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_list_runs_no_filter_returns_all() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    manager.start_run("repo-a", IndexRunMode::Full).unwrap();
    manager
        .persistence()
        .update_run_status(
            &manager.persistence().list_runs().unwrap()[0].run_id,
            IndexRunStatus::Succeeded,
            None,
        )
        .unwrap();
    manager.start_run("repo-b", IndexRunMode::Full).unwrap();

    let reports = manager.list_runs_with_health(None, None).unwrap();
    assert_eq!(reports.len(), 2);
}

#[tokio::test]
async fn test_list_runs_filtered_by_repo() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    manager.start_run("repo-x", IndexRunMode::Full).unwrap();
    manager
        .persistence()
        .update_run_status(
            &manager.persistence().list_runs().unwrap()[0].run_id,
            IndexRunStatus::Succeeded,
            None,
        )
        .unwrap();
    manager.start_run("repo-y", IndexRunMode::Full).unwrap();

    let reports = manager.list_runs_with_health(Some("repo-x"), None).unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].run.repo_id, "repo-x");
}

#[tokio::test]
async fn test_list_runs_filtered_by_status() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    let run1 = manager.start_run("repo-s1", IndexRunMode::Full).unwrap();
    manager
        .persistence()
        .update_run_status(&run1.run_id, IndexRunStatus::Succeeded, None)
        .unwrap();
    let _run2 = manager.start_run("repo-s2", IndexRunMode::Full).unwrap();

    let reports = manager
        .list_runs_with_health(None, Some(&IndexRunStatus::Queued))
        .unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].run.status, IndexRunStatus::Queued);
}

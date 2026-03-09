use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokenizor_agentic_mcp::ApplicationContext;
use tokenizor_agentic_mcp::application::run_manager::RunManager;
use tokenizor_agentic_mcp::config::{BlobStoreConfig, ControlPlaneBackend, ServerConfig};
use tokenizor_agentic_mcp::domain::{
    Checkpoint, ComponentHealth, DiscoveryManifest, FileRecord, IndexRunMode, IndexRunStatus,
    LanguageId, NextAction, PersistedFileOutcome, ProjectIdentityKind, RecoveryStateKind,
    Repository, RepositoryKind, RepositoryStatus, ResumeRejectReason, ResumeRunOutcome, RunHealth,
};
use tokenizor_agentic_mcp::error::TokenizorError;
use tokenizor_agentic_mcp::storage::registry_persistence::RegistryPersistence;
use tokenizor_agentic_mcp::storage::{BlobStore, LocalCasBlobStore, StoredBlob};

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

    let cas_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(RunManager::with_blob_root(
        persistence,
        cas_dir.path().to_path_buf(),
    ));
    let cas: Arc<dyn BlobStore> = Arc::new(LocalCasBlobStore::new(BlobStoreConfig {
        root_dir: cas_dir.path().to_path_buf(),
    }));

    (dir, manager, cas_dir, cas)
}

fn sample_committed_record(
    run_id: &str,
    repo_id: &str,
    relative_path: &str,
    committed_at: u64,
) -> FileRecord {
    FileRecord {
        relative_path: relative_path.to_string(),
        language: LanguageId::Rust,
        blob_id: format!("blob-{relative_path}"),
        byte_len: 16,
        content_hash: format!("hash-{relative_path}"),
        outcome: PersistedFileOutcome::Committed,
        symbols: vec![],
        run_id: run_id.to_string(),
        repo_id: repo_id.to_string(),
        committed_at_unix_ms: committed_at,
    }
}

fn sample_checkpoint(
    run_id: &str,
    cursor: &str,
    files_processed: u64,
    symbols_written: u64,
) -> Checkpoint {
    Checkpoint {
        run_id: run_id.to_string(),
        cursor: cursor.to_string(),
        files_processed,
        symbols_written,
        created_at_unix_ms: 2_000,
    }
}

fn sample_discovery_manifest(run_id: &str, relative_paths: &[&str]) -> DiscoveryManifest {
    DiscoveryManifest {
        run_id: run_id.to_string(),
        discovered_at_unix_ms: 1_500,
        relative_paths: relative_paths.iter().map(|path| path.to_string()).collect(),
    }
}

#[tokio::test]
async fn test_launch_run_transitions_queued_running_succeeded() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(repo_dir.path().join("lib.py"), "def foo(): pass").unwrap();

    let (run, progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    assert_eq!(run.status, IndexRunStatus::Queued);

    // Wait for background task to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let finished_run = manager
        .persistence()
        .find_run(&run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(finished_run.status, IndexRunStatus::Succeeded);
    assert!(finished_run.started_at_unix_ms.is_some());
    assert!(finished_run.finished_at_unix_ms.is_some());

    assert_eq!(
        progress
            .total_files
            .load(std::sync::atomic::Ordering::Relaxed),
        2
    );
    assert_eq!(
        progress
            .files_processed
            .load(std::sync::atomic::Ordering::Relaxed),
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
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let finished_run = manager
        .persistence()
        .find_run(&run.run_id)
        .unwrap()
        .unwrap();
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
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
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
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
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
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    // Rust (QualityFocus) and Java (Broader) should be persisted, not Ruby (Unsupported)
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|r| r.relative_path.ends_with("main.rs")));
    assert!(
        records
            .iter()
            .any(|r| r.relative_path.ends_with("App.java"))
    );
    assert!(!records.iter().any(|r| r.relative_path.ends_with("app.rb")));
}

#[tokio::test]
async fn test_file_records_linked_to_run_and_repo() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(
        repo_dir.path().join("lib.go"),
        "package main\nfunc Lib() {}",
    )
    .unwrap();

    let (run, _progress) = manager
        .launch_run(
            "my-repo-id",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
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
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
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

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
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
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
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
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(records.len(), 1);
    // Should be Committed (partial parse with symbols) or Quarantined (partial parse, no symbols)
    // Either way, not Failed — tree-sitter handles syntax errors gracefully
    assert!(
        matches!(
            records[0].outcome,
            PersistedFileOutcome::Committed | PersistedFileOutcome::Quarantined { .. }
        ),
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
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    // Only Rust and Java should produce file records
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|r| r.relative_path.ends_with("main.rs")));
    assert!(
        records
            .iter()
            .any(|r| r.relative_path.ends_with("Service.java"))
    );
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
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert!(
        records.is_empty(),
        "unsupported files should produce no file records"
    );
}

#[tokio::test]
async fn test_quality_focus_languages_still_process_correctly() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(repo_dir.path().join("lib.py"), "def foo(): pass").unwrap();
    fs::write(repo_dir.path().join("app.js"), "function app() {}").unwrap();
    fs::write(repo_dir.path().join("mod.ts"), "function hello(): void {}").unwrap();
    fs::write(
        repo_dir.path().join("main.go"),
        "package main\nfunc main() {}",
    )
    .unwrap();

    let (run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let finished = manager
        .persistence()
        .find_run(&run.run_id)
        .unwrap()
        .unwrap();
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
        .launch_run(
            "repo-healthy",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
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
    let run = manager
        .start_run("repo-degraded", IndexRunMode::Full)
        .unwrap();
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
    let run = manager
        .start_run("repo-interrupt", IndexRunMode::Full)
        .unwrap();
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
async fn test_list_runs_after_startup_sweep_keeps_interrupted_runs_unhealthy() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    let run = manager
        .start_run("repo-list-interrupt", IndexRunMode::Full)
        .unwrap();
    manager
        .persistence()
        .update_run_status(&run.run_id, IndexRunStatus::Running, None)
        .unwrap();
    manager.startup_sweep().unwrap();

    let reports = manager
        .list_runs_with_health(Some("repo-list-interrupt"), None)
        .unwrap();

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].run.status, IndexRunStatus::Interrupted);
    assert_eq!(reports[0].health, RunHealth::Unhealthy);
    assert!(
        reports[0]
            .action_required
            .as_deref()
            .is_some_and(|message| message.contains("interrupted"))
    );
}

#[tokio::test]
async fn test_startup_swept_interrupted_run_can_resume_from_durable_checkpoint() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), "fn b() {}\n").unwrap();
    fs::write(repo_dir.path().join("c.rs"), "fn c() {}\n").unwrap();
    seed_integration_repo(&manager, "repo-resume");

    let run = manager
        .start_run("repo-resume", IndexRunMode::Full)
        .unwrap();
    manager
        .persistence()
        .update_run_status(&run.run_id, IndexRunStatus::Running, None)
        .unwrap();

    let seeded_record = sample_committed_record(&run.run_id, "repo-resume", "a.rs", 1_111);
    manager
        .persistence()
        .save_file_records(&run.run_id, std::slice::from_ref(&seeded_record))
        .unwrap();
    manager
        .persistence()
        .save_checkpoint(&sample_checkpoint(&run.run_id, "a.rs", 1, 0))
        .unwrap();
    manager
        .persistence()
        .save_discovery_manifest(&sample_discovery_manifest(
            &run.run_id,
            &["a.rs", "b.rs", "c.rs"],
        ))
        .unwrap();

    manager.startup_sweep().unwrap();

    let interrupted = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(interrupted.run.status, IndexRunStatus::Interrupted);
    assert_eq!(interrupted.health, RunHealth::Unhealthy);
    assert!(
        interrupted
            .action_required
            .as_deref()
            .is_some_and(|message| message.contains("resume"))
    );

    let outcome = manager
        .resume_run(&run.run_id, repo_dir.path().to_path_buf(), cas)
        .unwrap();
    match outcome {
        ResumeRunOutcome::Resumed {
            checkpoint,
            durable_files_skipped,
            ..
        } => {
            assert_eq!(checkpoint.cursor, "a.rs");
            assert_eq!(durable_files_skipped, 1);
        }
        other => panic!("expected resumed outcome, got: {other:?}"),
    }

    let active_report = manager.inspect_run(&run.run_id).unwrap();
    assert!(active_report.is_active);
    let progress = active_report
        .progress
        .expect("resume should expose progress");
    assert_eq!(progress.files_processed, 1);
    assert_eq!(progress.total_files, 3);
    assert!(
        active_report
            .action_required
            .as_deref()
            .is_some_and(|message| message.contains("wait"))
    );

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let finished = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(finished.run.status, IndexRunStatus::Succeeded);
    assert_eq!(
        finished
            .run
            .recovery_state
            .as_ref()
            .map(|state| state.state.clone()),
        Some(RecoveryStateKind::Resumed)
    );

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(records.len(), 3);
    let first = records
        .iter()
        .find(|record| record.relative_path == "a.rs")
        .expect("seeded record should remain present");
    assert_eq!(first.committed_at_unix_ms, 1_111);
}

#[tokio::test]
async fn test_resume_rejection_surfaces_explicit_reindex_guidance() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").unwrap();
    seed_integration_repo(&manager, "repo-resume-reject");

    let run = manager
        .start_run("repo-resume-reject", IndexRunMode::Full)
        .unwrap();
    manager
        .persistence()
        .update_run_status(
            &run.run_id,
            IndexRunStatus::Interrupted,
            Some("startup sweep".into()),
        )
        .unwrap();

    let outcome = manager
        .resume_run(&run.run_id, repo_dir.path().to_path_buf(), cas)
        .unwrap();
    match outcome {
        ResumeRunOutcome::Rejected {
            reason,
            next_action,
            ..
        } => {
            assert_eq!(reason, ResumeRejectReason::MissingCheckpoint);
            assert_eq!(next_action, NextAction::Reindex);
        }
        other => panic!("expected rejected outcome, got: {other:?}"),
    }

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(report.run.status, IndexRunStatus::Interrupted);
    assert!(
        report
            .action_required
            .as_deref()
            .is_some_and(|message| message.contains("Resume rejected"))
    );
    assert!(
        report
            .action_required
            .as_deref()
            .is_some_and(|message| message.contains("reindex"))
    );
}

#[tokio::test]
async fn test_repeated_resume_attempts_do_not_duplicate_file_records() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), "fn b() {}\n").unwrap();
    seed_integration_repo(&manager, "repo-resume-repeat");

    let run = manager
        .start_run("repo-resume-repeat", IndexRunMode::Full)
        .unwrap();
    manager
        .persistence()
        .save_file_records(
            &run.run_id,
            &[sample_committed_record(
                &run.run_id,
                "repo-resume-repeat",
                "a.rs",
                9_999,
            )],
        )
        .unwrap();
    manager
        .persistence()
        .save_checkpoint(&sample_checkpoint(&run.run_id, "a.rs", 1, 0))
        .unwrap();
    manager
        .persistence()
        .save_discovery_manifest(&sample_discovery_manifest(&run.run_id, &["a.rs", "b.rs"]))
        .unwrap();
    manager
        .persistence()
        .update_run_status(
            &run.run_id,
            IndexRunStatus::Interrupted,
            Some("startup sweep".into()),
        )
        .unwrap();

    let first = manager
        .resume_run(&run.run_id, repo_dir.path().to_path_buf(), cas.clone())
        .unwrap();
    assert!(matches!(first, ResumeRunOutcome::Resumed { .. }));

    let second = manager
        .resume_run(&run.run_id, repo_dir.path().to_path_buf(), cas)
        .unwrap();
    match second {
        ResumeRunOutcome::Rejected {
            reason,
            next_action,
            ..
        } => {
            assert_eq!(reason, ResumeRejectReason::ActiveRunConflict);
            assert_eq!(next_action, NextAction::Wait);
        }
        other => panic!("expected active-run rejection, got: {other:?}"),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(records.len(), 2);
    let first_record = records
        .iter()
        .find(|record| record.relative_path == "a.rs")
        .expect("checkpoint record should still exist");
    assert_eq!(first_record.committed_at_unix_ms, 9_999);
}

#[tokio::test]
async fn test_resume_eligibility_latency_stays_under_one_second() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").unwrap();
    seed_integration_repo(&manager, "repo-resume-latency");

    let run = manager
        .start_run("repo-resume-latency", IndexRunMode::Full)
        .unwrap();
    manager
        .persistence()
        .update_run_status(
            &run.run_id,
            IndexRunStatus::Interrupted,
            Some("startup sweep".into()),
        )
        .unwrap();

    let started = std::time::Instant::now();
    let outcome = manager
        .resume_run(&run.run_id, repo_dir.path().to_path_buf(), cas)
        .unwrap();
    let elapsed = started.elapsed();

    assert!(matches!(
        outcome,
        ResumeRunOutcome::Rejected {
            reason: ResumeRejectReason::MissingCheckpoint,
            ..
        }
    ));
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "resume eligibility should remain fast, took {elapsed:?}"
    );
}

#[test]
fn test_application_context_from_config_transitions_running_runs_before_runtime_ready() {
    let temp = tempfile::tempdir().unwrap();
    let blob_root = temp.path().join(".tokenizor");
    std::fs::create_dir_all(blob_root.join("blobs").join("sha256")).unwrap();
    std::fs::create_dir_all(blob_root.join("temp")).unwrap();
    std::fs::create_dir_all(blob_root.join("quarantine")).unwrap();
    std::fs::create_dir_all(blob_root.join("derived")).unwrap();
    let registry_path = blob_root
        .join("control-plane")
        .join("project-workspace-registry.json");
    let persistence = RegistryPersistence::new(registry_path.clone());
    persistence
        .save_run(&tokenizor_agentic_mcp::domain::IndexRun {
            run_id: "stale-run".into(),
            repo_id: "repo-startup".into(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Running,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: None,
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        })
        .unwrap();

    let mut config = ServerConfig::default();
    config.control_plane.backend = ControlPlaneBackend::InMemory;
    config.blob_store.root_dir = blob_root;

    let application = ApplicationContext::from_config(config).unwrap();
    let report = application
        .ensure_runtime_ready()
        .expect("interrupted runs should warn, not block readiness");
    let stored = application
        .run_manager()
        .persistence()
        .find_run("stale-run")
        .unwrap()
        .unwrap();

    assert_eq!(stored.status, IndexRunStatus::Interrupted);
    assert!(report.is_ready());
    assert!(report.checks.iter().any(
        |check| check.category == tokenizor_agentic_mcp::domain::HealthIssueCategory::Recovery
    ));
}

#[test]
fn test_application_context_startup_reconciliation_blocks_readiness_on_blocking_finding() {
    let temp = tempfile::tempdir().unwrap();
    let blob_root = temp.path().join(".tokenizor");
    let registry_path = blob_root
        .join("control-plane")
        .join("project-workspace-registry.json");
    std::fs::create_dir_all(registry_path.parent().unwrap()).unwrap();

    let blocking_artifact = registry_path
        .parent()
        .unwrap()
        .join(".project-workspace-registry.json.123.tmp");
    std::fs::create_dir_all(&blocking_artifact).unwrap();

    let mut config = ServerConfig::default();
    config.control_plane.backend = ControlPlaneBackend::InMemory;
    config.blob_store.root_dir = blob_root;

    let application = ApplicationContext::from_config(config).unwrap();
    let error = application
        .ensure_runtime_ready()
        .expect_err("readiness should fail when startup recovery finds a blocker");

    assert!(error.to_string().contains("startup_recovery"));
    assert!(error.to_string().contains("repair") || error.to_string().contains("migrate"));
}

#[tokio::test]
async fn test_inspect_cancelled_run_returns_healthy() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    let run = manager
        .start_run("repo-cancel", IndexRunMode::Full)
        .unwrap();
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

// ── Story 2.6: Phase & Resource Integration Tests ──────────────────────────

use tokenizor_agentic_mcp::domain::RunPhase;

#[tokio::test]
async fn test_active_run_progress_includes_processing_phase() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    // Write several files to ensure the run spends time in Processing
    for i in 0..5 {
        fs::write(
            repo_dir.path().join(format!("mod_{i}.rs")),
            format!("pub fn f{i}() {{}}\npub fn g{i}() {{}}\n"),
        )
        .unwrap();
    }

    let (run, _progress) = manager
        .launch_run(
            "repo-phase",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Wait for completion
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert!(!report.is_active);
    assert!(report.progress.is_some());
    let progress = report.progress.unwrap();
    // Terminal run should show Complete phase
    assert_eq!(progress.phase, RunPhase::Complete);
    assert!(progress.total_files > 0);
    assert!(progress.files_processed > 0);
}

#[tokio::test]
async fn test_terminal_succeeded_run_has_complete_phase_in_progress() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    let (run, _progress) = manager
        .launch_run(
            "repo-terminal-ok",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert!(!report.is_active);
    assert_eq!(report.run.status, IndexRunStatus::Succeeded);
    assert!(report.progress.is_some());
    let progress = report.progress.unwrap();
    assert_eq!(progress.phase, RunPhase::Complete);
    assert!(report.file_outcome_summary.is_some());
}

#[tokio::test]
async fn test_failed_run_does_not_present_as_live() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    // Create a run with file records, then mark it as failed
    let run = manager.start_run("repo-fail", IndexRunMode::Full).unwrap();

    // Simulate partial progress: some files committed, some failed
    let committed_file = FileRecord {
        relative_path: "lib.rs".into(),
        language: LanguageId::Rust,
        blob_id: "blob-ok".into(),
        byte_len: 200,
        content_hash: "hash-ok".into(),
        outcome: PersistedFileOutcome::Committed,
        symbols: vec![],
        run_id: run.run_id.clone(),
        repo_id: "repo-fail".into(),
        committed_at_unix_ms: 1000,
    };
    let failed_file = FileRecord {
        relative_path: "broken.rs".into(),
        language: LanguageId::Rust,
        blob_id: "blob-fail".into(),
        byte_len: 50,
        content_hash: "hash-fail".into(),
        outcome: PersistedFileOutcome::Failed {
            error: "parse error".into(),
        },
        symbols: vec![],
        run_id: run.run_id.clone(),
        repo_id: "repo-fail".into(),
        committed_at_unix_ms: 1001,
    };
    manager
        .persistence()
        .save_file_records(&run.run_id, &[committed_file, failed_file])
        .unwrap();

    // Mark the run as failed after partial processing
    manager
        .persistence()
        .update_run_status(
            &run.run_id,
            IndexRunStatus::Failed,
            Some("systemic error".into()),
        )
        .unwrap();

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert!(!report.is_active);
    assert_eq!(report.run.status, IndexRunStatus::Failed);
    assert_eq!(report.health, RunHealth::Unhealthy);
    // Failed run with file records should have synthetic terminal progress
    assert!(report.progress.is_some());
    let progress = report.progress.unwrap();
    assert_eq!(progress.phase, RunPhase::Complete);
    assert_eq!(progress.total_files, 2);
    assert_eq!(progress.files_processed, 1);
    assert_eq!(progress.files_failed, 1);
}

#[tokio::test]
async fn test_read_nonexistent_run_status_returns_error() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();
    let result = manager.inspect_run("nonexistent-run-id-xyz");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_recent_run_ids_includes_active_and_terminal() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("lib.rs"), "pub fn x() {}").unwrap();

    let (run, _progress) = manager
        .launch_run(
            "repo-recent",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let ids = manager.list_recent_run_ids(10);
    assert!(!ids.is_empty());
    assert!(ids.contains(&run.run_id));
}

#[tokio::test]
async fn test_phase_transitions_during_pipeline_execution() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    let (run, progress) = manager
        .launch_run(
            "repo-phases",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Wait for completion
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // After pipeline completes, progress phase should be Complete
    assert_eq!(progress.phase(), RunPhase::Complete);

    let report = manager.inspect_run(&run.run_id).unwrap();
    assert!(!report.is_active);
    assert_eq!(report.run.status, IndexRunStatus::Succeeded);
}

// ============================================================
// Story 2.7 — Cancel an Active Indexing Run Safely
// ============================================================

#[tokio::test]
async fn test_cancel_active_run_transitions_to_cancelled() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    // Create a repo with many files so the pipeline takes long enough to cancel
    let repo_dir = tempfile::tempdir().unwrap();
    for i in 0..500 {
        fs::write(
            repo_dir.path().join(format!("file_{i}.rs")),
            format!("fn f{i}() {{ let x = {i}; let y = x + 1; let z = y * 2; }}"),
        )
        .unwrap();
    }

    let (run, _progress) = manager
        .launch_run(
            "repo-cancel-active",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Yield to let the spawned task start and transition to Running
    tokio::task::yield_now().await;
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Cancel the run
    let report = manager.cancel_run(&run.run_id).unwrap();
    assert_eq!(report.run.status, IndexRunStatus::Cancelled);
    assert!(!report.is_active);
    assert_eq!(report.health, RunHealth::Healthy);

    // Wait for spawned task to finish
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Verify the status is still Cancelled (spawned task didn't overwrite)
    let final_report = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(final_report.run.status, IndexRunStatus::Cancelled);
    assert!(!final_report.is_active);
    assert!(final_report.run.finished_at_unix_ms.is_some());

    // Progress phase should be Complete if files were processed; None if cancelled
    // before any processing. Either is acceptable — the key invariant is Cancelled status.
    if let Some(progress) = final_report.progress {
        assert_eq!(progress.phase, RunPhase::Complete);
    }
}

#[tokio::test]
async fn test_cancel_succeeded_run_returns_succeeded_unchanged() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    let (run, _progress) = manager
        .launch_run(
            "repo-cancel-succ",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Wait for pipeline to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let before = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(before.run.status, IndexRunStatus::Succeeded);

    // Cancel after success — AC #2: deterministic, no mutation
    let report = manager.cancel_run(&run.run_id).unwrap();
    assert_eq!(report.run.status, IndexRunStatus::Succeeded);
}

#[tokio::test]
async fn test_cancel_nonexistent_run_returns_not_found() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    let result = manager.cancel_run("does-not-exist");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), TokenizorError::NotFound(_)));
}

#[tokio::test]
async fn test_cancel_failed_run_returns_failed_unchanged() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    // Use a non-existent repo root to trigger discovery failure → Failed status
    let (run, _progress) = manager
        .launch_run(
            "repo-cancel-fail",
            IndexRunMode::Full,
            PathBuf::from("/nonexistent/repo/path"),
            cas,
        )
        .unwrap();

    // Wait for pipeline to fail
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let before = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(before.run.status, IndexRunStatus::Failed);

    // Cancel after failure — AC #2: no overwrite
    let report = manager.cancel_run(&run.run_id).unwrap();
    assert_eq!(report.run.status, IndexRunStatus::Failed);
}

#[tokio::test]
async fn test_cancel_immediate_processes_no_files() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(repo_dir.path().join("lib.py"), "def foo(): pass").unwrap();

    let (run, _progress) = manager
        .launch_run(
            "repo-precancel",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Cancel immediately before pipeline has a chance to start
    let _report = manager.cancel_run(&run.run_id).unwrap();

    // Wait for spawned task to observe the cancellation
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let final_report = manager.inspect_run(&run.run_id).unwrap();
    assert_eq!(final_report.run.status, IndexRunStatus::Cancelled);
    assert!(!final_report.is_active);

    // Verify no files were processed (files_processed == 0)
    let file_records = manager.persistence().get_file_records(&run.run_id).unwrap();
    assert_eq!(
        file_records.len(),
        0,
        "expected 0 file records for immediately cancelled run"
    );
}

#[tokio::test]
async fn test_double_cancel_same_run_returns_same_cancelled_report() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    let (run, _progress) = manager
        .launch_run(
            "repo-double-cancel",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // First cancel
    let report1 = manager.cancel_run(&run.run_id).unwrap();
    assert_eq!(report1.run.status, IndexRunStatus::Cancelled);

    // Wait for spawned task to finish
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Second cancel — AC #2: already terminal, returns same status
    let report2 = manager.cancel_run(&run.run_id).unwrap();
    assert_eq!(report2.run.status, IndexRunStatus::Cancelled);
}

// ============================================================
// Story 2.8 — Checkpoint Long-Running Indexing Work
// ============================================================

#[tokio::test]
async fn test_checkpoint_active_run_persists_with_correct_identity() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    // Create a repo with many files to ensure processing takes enough time
    let repo_dir = tempfile::tempdir().unwrap();
    for i in 0..500 {
        fs::write(
            repo_dir.path().join(format!("file_{i:04}.rs")),
            format!("pub fn f{i}() {{ let x = {i}; }}\npub fn g{i}() {{ let y = {i}; }}"),
        )
        .unwrap();
    }

    let (run, progress) = manager
        .launch_run(
            "repo-checkpoint",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Wait for pipeline to start and process some files
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let processed = progress
            .files_processed
            .load(std::sync::atomic::Ordering::Relaxed);
        if processed >= 10 {
            break;
        }
    }

    // Attempt manual checkpoint
    let result = manager.checkpoint_run(&run.run_id);
    // If pipeline hasn't committed enough contiguous files, cursor may be None.
    // Retry once after a brief wait.
    let checkpoint = match result {
        Ok(cp) => cp,
        Err(_) => {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            manager.checkpoint_run(&run.run_id).unwrap()
        }
    };

    assert_eq!(checkpoint.run_id, run.run_id);
    assert!(!checkpoint.cursor.is_empty());
    assert!(checkpoint.files_processed > 0);
    assert!(checkpoint.created_at_unix_ms > 0);

    // Verify persisted
    let latest = manager
        .persistence()
        .get_latest_checkpoint(&run.run_id)
        .unwrap();
    assert!(latest.is_some());
    let latest = latest.unwrap();
    assert_eq!(latest.run_id, run.run_id);
    assert_eq!(latest.cursor, checkpoint.cursor);

    // Wait for completion
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
}

#[tokio::test]
async fn test_checkpoint_terminal_succeeded_run_returns_error() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    let (run, _progress) = manager
        .launch_run(
            "repo-cp-term",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Wait for pipeline to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let finished = manager
        .persistence()
        .find_run(&run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(finished.status, IndexRunStatus::Succeeded);

    // Checkpoint after success — AC #2: explicit failure
    let result = manager.checkpoint_run(&run.run_id);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        TokenizorError::InvalidOperation(_)
    ));
}

#[tokio::test]
async fn test_checkpoint_nonexistent_run_returns_not_found() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    let result = manager.checkpoint_run("does-not-exist-cp");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), TokenizorError::NotFound(_)));
}

#[tokio::test]
async fn test_checkpoint_cancelled_run_returns_error() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    let (run, _progress) = manager
        .launch_run(
            "repo-cp-cancel",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Cancel immediately
    manager.cancel_run(&run.run_id).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let finished = manager
        .persistence()
        .find_run(&run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(finished.status, IndexRunStatus::Cancelled);

    // Checkpoint after cancel — AC #2: explicit failure
    let result = manager.checkpoint_run(&run.run_id);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        TokenizorError::InvalidOperation(_)
    ));
}

#[tokio::test]
async fn test_automatic_checkpoint_fires_during_processing() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    // Create 200+ files to trigger automatic checkpoint at interval=100
    let repo_dir = tempfile::tempdir().unwrap();
    for i in 0..250 {
        fs::write(
            repo_dir.path().join(format!("mod_{i:04}.rs")),
            format!("pub fn f{i}() {{ let x = {i}; }}"),
        )
        .unwrap();
    }

    let (run, _progress) = manager
        .launch_run(
            "repo-auto-cp",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Wait for pipeline to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    let finished = manager
        .persistence()
        .find_run(&run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(finished.status, IndexRunStatus::Succeeded);

    // Automatic checkpoints should have been created at files_processed=100, 200
    let latest = manager
        .persistence()
        .get_latest_checkpoint(&run.run_id)
        .unwrap();
    assert!(
        latest.is_some(),
        "expected at least one automatic checkpoint for 250 files with interval=100"
    );
    let latest = latest.unwrap();
    assert_eq!(latest.run_id, run.run_id);
    assert!(latest.files_processed > 0);
}

#[tokio::test]
async fn test_checkpoint_cursor_on_index_run_updated_after_checkpoint() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();

    let repo_dir = tempfile::tempdir().unwrap();
    for i in 0..500 {
        fs::write(
            repo_dir.path().join(format!("src_{i:04}.rs")),
            format!("pub fn f{i}() {{ let x = {i}; }}"),
        )
        .unwrap();
    }

    let (run, progress) = manager
        .launch_run(
            "repo-cursor-check",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Wait for some files to process
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let processed = progress
            .files_processed
            .load(std::sync::atomic::Ordering::Relaxed);
        if processed >= 10 {
            break;
        }
    }

    // Create checkpoint
    let result = manager.checkpoint_run(&run.run_id);
    let checkpoint = match result {
        Ok(cp) => cp,
        Err(_) => {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            manager.checkpoint_run(&run.run_id).unwrap()
        }
    };

    // Verify IndexRun.checkpoint_cursor matches
    let updated_run = manager
        .persistence()
        .find_run(&run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        updated_run.checkpoint_cursor,
        Some(checkpoint.cursor.clone()),
        "IndexRun.checkpoint_cursor should match the checkpoint's cursor"
    );

    // Wait for completion
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
}

// === Re-index integration tests (Story 2.9) ===

#[tokio::test]
async fn test_reindex_lifecycle_creates_new_run_with_prior_run_id() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    // Initial index run
    let (initial_run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let initial_report = manager.inspect_run(&initial_run.run_id).unwrap();
    assert_eq!(initial_report.run.status, IndexRunStatus::Succeeded);

    // Trigger re-index — now actually launches the pipeline
    let reindex_run = manager
        .reindex_repository(
            "test-repo",
            None,
            Some("files changed"),
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();
    assert_eq!(reindex_run.mode, IndexRunMode::Reindex);
    assert_eq!(reindex_run.prior_run_id, Some(initial_run.run_id.clone()));
    assert_eq!(reindex_run.status, IndexRunStatus::Queued);
    assert_eq!(reindex_run.description, Some("files changed".to_string()));

    // Both runs are inspectable
    let initial_report = manager.inspect_run(&initial_run.run_id).unwrap();
    assert_eq!(initial_report.run.status, IndexRunStatus::Succeeded);
    let reindex_report = manager.inspect_run(&reindex_run.run_id).unwrap();
    assert_eq!(reindex_report.run.mode, IndexRunMode::Reindex);
}

#[tokio::test]
async fn test_reindex_prior_state_preservation() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("lib.rs"), "pub fn hello() {}").unwrap();

    // Initial index run — complete it
    let (initial_run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Verify initial run has file records
    let initial_files = manager
        .persistence()
        .get_file_records(&initial_run.run_id)
        .unwrap();
    assert!(
        !initial_files.is_empty(),
        "initial run should have file records"
    );
    let initial_file_count = initial_files.len();

    // Trigger re-index
    let reindex_run = manager
        .reindex_repository("test-repo", None, None, repo_dir.path().to_path_buf(), cas)
        .unwrap();

    // Prior state still intact
    let prior_files = manager
        .persistence()
        .get_file_records(&initial_run.run_id)
        .unwrap();
    assert_eq!(
        prior_files.len(),
        initial_file_count,
        "prior run file records should be preserved"
    );

    let prior_report = manager.inspect_run(&initial_run.run_id).unwrap();
    assert_eq!(prior_report.run.status, IndexRunStatus::Succeeded);
    assert_eq!(prior_report.run.run_id, initial_run.run_id);

    // Reindex run exists alongside
    let reindex_report = manager.inspect_run(&reindex_run.run_id).unwrap();
    assert_eq!(reindex_report.run.prior_run_id, Some(initial_run.run_id));
}

#[tokio::test]
async fn test_reindex_idempotent_replay_returns_same_run() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    // Initial index
    let (_initial_run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // First reindex — launches pipeline, run is active
    let first_reindex = manager
        .reindex_repository(
            "test-repo",
            None,
            None,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();

    // H1 fix: idempotent replay while the reindex is still active — idempotency
    // check fires before active-run check, so stored result is returned
    let replay = manager
        .reindex_repository("test-repo", None, None, repo_dir.path().to_path_buf(), cas)
        .unwrap();
    assert_eq!(
        first_reindex.run_id, replay.run_id,
        "idempotent replay should return same run"
    );
}

#[tokio::test]
async fn test_reindex_conflicting_replay_returns_error() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    // Create an active (non-terminal) run that the idempotency record references
    use tokenizor_agentic_mcp::domain::{IdempotencyRecord, IdempotencyStatus};
    let active_run = manager
        .start_run("test-repo", IndexRunMode::Reindex)
        .unwrap();
    let record = IdempotencyRecord {
        operation: "reindex".to_string(),
        idempotency_key: "reindex::test-repo::".to_string(),
        request_hash: "fake-hash-that-wont-match".to_string(),
        status: IdempotencyStatus::Pending,
        result_ref: Some(active_run.run_id.clone()),
        created_at_unix_ms: 1000,
        expires_at_unix_ms: None,
    };
    manager
        .persistence()
        .save_idempotency_record(&record)
        .unwrap();

    let result =
        manager.reindex_repository("test-repo", None, None, repo_dir.path().to_path_buf(), cas);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("conflicting replay"),
        "error should mention conflicting replay: {err}"
    );
}

#[tokio::test]
async fn test_reindex_while_active_run_is_rejected() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    // Create a large file to keep the pipeline busy
    let mut content = String::new();
    for i in 0..200 {
        content.push_str(&format!("fn func_{i}() {{ let x = {i}; }}\n"));
    }
    fs::write(repo_dir.path().join("big.rs"), &content).unwrap();

    // Start a run that will take some time
    let (_run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();

    // Immediately attempt reindex — should fail (no matching idempotency record,
    // so falls through to active-run check)
    let result =
        manager.reindex_repository("test-repo", None, None, repo_dir.path().to_path_buf(), cas);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("active indexing run exists"),
        "error should mention active run: {err}"
    );
}

#[tokio::test]
async fn test_reindex_no_prior_completed_run_succeeds_with_none() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    // No prior runs — reindex should still work
    let reindex = manager
        .reindex_repository("fresh-repo", None, None, repo_dir.path().to_path_buf(), cas)
        .unwrap();
    assert_eq!(reindex.mode, IndexRunMode::Reindex);
    assert_eq!(
        reindex.prior_run_id, None,
        "no prior run should result in None"
    );
    assert_eq!(reindex.status, IndexRunStatus::Queued);
}

// === Invalidation integration tests (Story 2.10) ===

fn seed_integration_repo(manager: &RunManager, repo_id: &str) {
    let repo = Repository {
        repo_id: repo_id.to_string(),
        kind: RepositoryKind::Git,
        root_uri: format!("/tmp/{repo_id}"),
        project_identity: format!("identity-{repo_id}"),
        project_identity_kind: ProjectIdentityKind::GitCommonDir,
        default_branch: None,
        last_known_revision: None,
        status: RepositoryStatus::Ready,
        invalidated_at_unix_ms: None,
        invalidation_reason: None,
        quarantined_at_unix_ms: None,
        quarantine_reason: None,
    };
    manager.persistence().save_repository(&repo).unwrap();
}

#[tokio::test]
async fn test_invalidation_lifecycle_register_index_invalidate() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    seed_integration_repo(&manager, "test-repo");

    // Complete an index run
    let (run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let finished = manager
        .persistence()
        .find_run(&run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(finished.status, IndexRunStatus::Succeeded);

    // Invalidate the repo
    let result = manager
        .invalidate_repository("test-repo", None, Some("stale data"))
        .unwrap();
    assert_eq!(result.previous_status, RepositoryStatus::Ready);
    assert!(result.invalidated_at_unix_ms > 0);
    assert_eq!(result.reason.as_deref(), Some("stale data"));
    assert_eq!(result.action_required, "re-index or repair required");

    // Verify repo status in persistence
    let repo = manager
        .persistence()
        .get_repository("test-repo")
        .unwrap()
        .unwrap();
    assert_eq!(repo.status, RepositoryStatus::Invalidated);
    assert!(repo.invalidated_at_unix_ms.is_some());
    assert_eq!(repo.invalidation_reason.as_deref(), Some("stale data"));
}

#[tokio::test]
async fn test_invalidation_blocks_active_runs() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    // Large file to keep the pipeline busy
    let mut content = String::new();
    for i in 0..200 {
        content.push_str(&format!("fn func_{i}() {{ let x = {i}; }}\n"));
    }
    fs::write(repo_dir.path().join("big.rs"), &content).unwrap();

    seed_integration_repo(&manager, "test-repo");

    // Start a run that will take some time
    let (_run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    // Immediately attempt invalidation — should fail
    let result = manager.invalidate_repository("test-repo", None, Some("stale"));
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("active indexing run exists"),
        "error should mention active run: {err}"
    );
}

#[tokio::test]
async fn test_invalidation_idempotent_replay() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();
    seed_integration_repo(&manager, "test-repo");

    // First invalidation
    let first = manager
        .invalidate_repository("test-repo", None, Some("reason-1"))
        .unwrap();
    assert_eq!(first.previous_status, RepositoryStatus::Ready);

    // Replay with same params — should succeed with same result
    let replay = manager
        .invalidate_repository("test-repo", None, Some("reason-1"))
        .unwrap();
    assert_eq!(replay.previous_status, RepositoryStatus::Invalidated);
    assert_eq!(replay.reason.as_deref(), Some("reason-1"));
}

#[tokio::test]
async fn test_invalidation_conflicting_replay() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();
    seed_integration_repo(&manager, "test-repo");

    // First invalidation
    let first = manager
        .invalidate_repository("test-repo", None, Some("reason-1"))
        .unwrap();
    assert_eq!(first.previous_status, RepositoryStatus::Ready);

    // Replay with different reason — domain-level idempotency returns success
    // because repo is already invalidated (preserves original reason)
    let second = manager
        .invalidate_repository("test-repo", None, Some("reason-2"))
        .unwrap();
    assert_eq!(second.previous_status, RepositoryStatus::Invalidated);
    assert_eq!(
        second.reason.as_deref(),
        Some("reason-1"),
        "original reason preserved"
    );
}

#[tokio::test]
async fn test_invalidation_surfaces_in_inspect_run() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    seed_integration_repo(&manager, "test-repo");

    // Complete an index run
    let (run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Invalidate
    manager
        .invalidate_repository("test-repo", None, Some("compromised"))
        .unwrap();

    // Inspect the completed run — should surface invalidation
    let report = manager.inspect_run(&run.run_id).unwrap();
    let action = report.action_required.as_deref().unwrap_or("");
    assert!(
        action.contains("repository indexed state has been invalidated"),
        "action_required should surface invalidation: {action}"
    );
}

#[tokio::test]
async fn test_reindex_clears_invalidation() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    seed_integration_repo(&manager, "test-repo");

    // Complete initial index
    let (_run, _progress) = manager
        .launch_run(
            "test-repo",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Invalidate
    manager
        .invalidate_repository("test-repo", None, Some("stale"))
        .unwrap();
    let repo = manager
        .persistence()
        .get_repository("test-repo")
        .unwrap()
        .unwrap();
    assert_eq!(repo.status, RepositoryStatus::Invalidated);

    // Re-index — should be allowed and clear invalidation on success
    let reindex_run = manager
        .reindex_repository(
            "test-repo",
            None,
            Some("recovery"),
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let finished = manager
        .persistence()
        .find_run(&reindex_run.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(finished.status, IndexRunStatus::Succeeded);

    // Repo should transition back to Ready
    let repo = manager
        .persistence()
        .get_repository("test-repo")
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.status,
        RepositoryStatus::Ready,
        "re-index should clear invalidation"
    );
    assert!(repo.invalidated_at_unix_ms.is_none());
    assert!(repo.invalidation_reason.is_none());
}

#[tokio::test]
async fn test_invalidation_unknown_repo() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    let result = manager.invalidate_repository("nonexistent", None, None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("NotFound"),
        "should be NotFound: {err}"
    );
}

#[tokio::test]
async fn test_invalidation_already_invalidated_repo() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();
    seed_integration_repo(&manager, "test-repo");

    // First invalidation
    let first = manager
        .invalidate_repository("test-repo", None, Some("reason-1"))
        .unwrap();
    assert_eq!(first.previous_status, RepositoryStatus::Ready);

    // Second invalidation — should succeed idempotently
    let second = manager
        .invalidate_repository("test-repo", None, Some("reason-2"))
        .unwrap();
    assert_eq!(second.previous_status, RepositoryStatus::Invalidated);
    assert_eq!(
        second.reason.as_deref(),
        Some("reason-1"),
        "original reason preserved"
    );
}

// ============================================================
// Story 2.11: Cross-operation conflicting replay lifecycle tests
// ============================================================

use tokenizor_agentic_mcp::application::run_manager::IdempotentRunResult;

#[test]
fn test_index_run_completes_then_same_param_retry_creates_new_run() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    // Create run via idempotent path
    let result = manager
        .start_run_idempotent("repo-stale", "", IndexRunMode::Full)
        .unwrap();
    let first_id = match &result {
        IdempotentRunResult::NewRun { run } => run.run_id.clone(),
        _ => panic!("expected NewRun"),
    };

    // Complete the run
    manager
        .persistence()
        .update_run_status_with_finish(&first_id, IndexRunStatus::Succeeded, None, 2000, None)
        .unwrap();

    // Same-param retry → should create a new run (stale record bypassed)
    let retry = manager
        .start_run_idempotent("repo-stale", "", IndexRunMode::Full)
        .unwrap();
    match retry {
        IdempotentRunResult::NewRun { run } => {
            assert_ne!(run.run_id, first_id, "should be a new run, not the old one");
            assert_eq!(run.status, IndexRunStatus::Queued);
        }
        IdempotentRunResult::ExistingRun { .. } => panic!("expected NewRun for stale record"),
    }
}

#[test]
fn test_index_run_completes_then_different_param_retry_creates_new_run() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    // Create Full run via idempotent path
    let result = manager
        .start_run_idempotent("repo-stale2", "", IndexRunMode::Full)
        .unwrap();
    let first_id = match &result {
        IdempotentRunResult::NewRun { run } => run.run_id.clone(),
        _ => panic!("expected NewRun"),
    };

    // Complete the run
    manager
        .persistence()
        .update_run_status_with_finish(&first_id, IndexRunStatus::Succeeded, None, 2000, None)
        .unwrap();

    // Different-param retry (Incremental) → should create new run (stale record)
    let retry = manager
        .start_run_idempotent("repo-stale2", "", IndexRunMode::Incremental)
        .unwrap();
    match retry {
        IdempotentRunResult::NewRun { run } => {
            assert_ne!(run.run_id, first_id);
            assert_eq!(run.mode, IndexRunMode::Incremental);
        }
        IdempotentRunResult::ExistingRun { .. } => panic!("expected NewRun for stale record"),
    }
}

#[test]
fn test_index_active_then_different_param_retry_returns_conflicting_replay() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    // Create active run via idempotent path (Queued = non-terminal)
    let result = manager
        .start_run_idempotent("repo-conflict", "", IndexRunMode::Full)
        .unwrap();
    assert!(matches!(result, IdempotentRunResult::NewRun { .. }));

    // Different-param retry while active → ConflictingReplay
    let retry = manager.start_run_idempotent("repo-conflict", "", IndexRunMode::Incremental);
    assert!(retry.is_err());
    let err = retry.unwrap_err();
    assert!(
        matches!(err, TokenizorError::ConflictingReplay(_)),
        "expected ConflictingReplay, got: {err:?}"
    );
}

#[tokio::test]
async fn test_reindex_completes_then_same_param_retry_creates_new_run() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    // Create and complete an initial run
    let (initial, _) = manager
        .launch_run(
            "repo-ri",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    assert!(
        manager
            .inspect_run(&initial.run_id)
            .unwrap()
            .run
            .status
            .is_terminal()
    );

    // First reindex
    let first = manager
        .reindex_repository(
            "repo-ri",
            None,
            None,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();
    let first_id = first.run_id.clone();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    assert!(
        manager
            .inspect_run(&first_id)
            .unwrap()
            .run
            .status
            .is_terminal()
    );

    // Same-param reindex retry → new run (stale record bypassed)
    let second = manager
        .reindex_repository("repo-ri", None, None, repo_dir.path().to_path_buf(), cas)
        .unwrap();
    assert_ne!(second.run_id, first_id, "should be a new reindex run");
    assert_eq!(second.mode, IndexRunMode::Reindex);
}

#[tokio::test]
async fn test_reindex_completes_then_different_param_retry_creates_new_run() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    // Create and complete an initial run
    let (initial, _) = manager
        .launch_run(
            "repo-ri2",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    assert!(
        manager
            .inspect_run(&initial.run_id)
            .unwrap()
            .run
            .status
            .is_terminal()
    );

    // First reindex with no workspace
    let first = manager
        .reindex_repository(
            "repo-ri2",
            None,
            None,
            repo_dir.path().to_path_buf(),
            cas.clone(),
        )
        .unwrap();
    let first_id = first.run_id.clone();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    assert!(
        manager
            .inspect_run(&first_id)
            .unwrap()
            .run
            .status
            .is_terminal()
    );

    // Different-param reindex (with workspace_id) → new run (stale record bypassed)
    let second = manager
        .reindex_repository(
            "repo-ri2",
            Some("ws-new"),
            None,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();
    assert_ne!(second.run_id, first_id);
    assert_eq!(second.mode, IndexRunMode::Reindex);
}

#[test]
fn test_reindex_active_then_different_hash_returns_conflicting_replay() {
    use tokenizor_agentic_mcp::domain::{IdempotencyRecord, IdempotencyStatus};
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();

    // Create an active run and seed an idempotency record with different hash
    let active_run = manager
        .start_run("repo-ri3", IndexRunMode::Reindex)
        .unwrap();
    let record = IdempotencyRecord {
        operation: "reindex".to_string(),
        idempotency_key: "reindex::repo-ri3::".to_string(),
        request_hash: "different-hash-from-actual-request".to_string(),
        status: IdempotencyStatus::Pending,
        result_ref: Some(active_run.run_id.clone()),
        created_at_unix_ms: 1000,
        expires_at_unix_ms: None,
    };
    manager
        .persistence()
        .save_idempotency_record(&record)
        .unwrap();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    // Reindex with same key but hash mismatches stored record → ConflictingReplay
    let result =
        manager.reindex_repository("repo-ri3", None, None, repo_dir.path().to_path_buf(), _cas);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, TokenizorError::ConflictingReplay(_)),
        "expected ConflictingReplay, got: {err:?}"
    );
}

#[tokio::test]
async fn test_invalidate_reindex_invalidate_different_reason_succeeds() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    seed_integration_repo(&manager, "repo-inv");

    // Invalidate with reason A
    let first = manager
        .invalidate_repository("repo-inv", None, Some("reason-A"))
        .unwrap();
    assert_eq!(first.previous_status, RepositoryStatus::Ready);

    // Re-index to clear invalidation
    let reindex = manager
        .reindex_repository(
            "repo-inv",
            None,
            Some("restore"),
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    assert!(
        manager
            .inspect_run(&reindex.run_id)
            .unwrap()
            .run
            .status
            .is_terminal()
    );

    // Verify repo status is back to Ready (pipeline completion handler does this)
    let repo = manager
        .persistence()
        .get_repository("repo-inv")
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.status,
        RepositoryStatus::Ready,
        "re-index should restore Ready status"
    );

    // Re-invalidate with different reason → succeeds (stale record handled)
    let second = manager
        .invalidate_repository("repo-inv", None, Some("reason-B"))
        .unwrap();
    assert_eq!(second.previous_status, RepositoryStatus::Ready);
    assert_eq!(second.reason.as_deref(), Some("reason-B"));
}

#[tokio::test]
async fn test_invalidate_reindex_invalidate_same_reason_succeeds() {
    let (_dir, manager, _cas_dir, cas) = setup_test_env();
    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();

    seed_integration_repo(&manager, "repo-inv2");

    // Invalidate with reason A
    let first = manager
        .invalidate_repository("repo-inv2", None, Some("reason-A"))
        .unwrap();
    assert_eq!(first.previous_status, RepositoryStatus::Ready);

    // Re-index to clear invalidation
    let reindex = manager
        .reindex_repository(
            "repo-inv2",
            None,
            Some("restore"),
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    assert!(
        manager
            .inspect_run(&reindex.run_id)
            .unwrap()
            .run
            .status
            .is_terminal()
    );

    // Verify repo status is back to Ready
    let repo = manager
        .persistence()
        .get_repository("repo-inv2")
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.status,
        RepositoryStatus::Ready,
        "re-index should restore Ready status"
    );

    // Re-invalidate with same reason → succeeds (stale record handled)
    let second = manager
        .invalidate_repository("repo-inv2", None, Some("reason-A"))
        .unwrap();
    assert_eq!(second.previous_status, RepositoryStatus::Ready);
    assert_eq!(second.reason.as_deref(), Some("reason-A"));
}

#[test]
fn test_idempotency_key_space_isolation() {
    let (_dir, manager, _cas_dir, _cas) = setup_test_env();
    seed_integration_repo(&manager, "repo-iso");

    // Create index idempotency record
    let index_result = manager
        .start_run_idempotent("repo-iso", "", IndexRunMode::Full)
        .unwrap();
    let index_run_id = match &index_result {
        IdempotentRunResult::NewRun { run } => run.run_id.clone(),
        _ => panic!("expected NewRun"),
    };

    // Complete the run so it doesn't block invalidation
    manager
        .persistence()
        .update_run_status_with_finish(&index_run_id, IndexRunStatus::Succeeded, None, 2000, None)
        .unwrap();

    // Verify index:: key space
    let index_record = manager
        .persistence()
        .find_idempotency_record("index::repo-iso::")
        .unwrap();
    assert!(
        index_record.is_some(),
        "index idempotency record should exist"
    );

    // Verify reindex:: key space is separate
    let reindex_record = manager
        .persistence()
        .find_idempotency_record("reindex::repo-iso::")
        .unwrap();
    assert!(
        reindex_record.is_none(),
        "reindex key space should be empty"
    );

    // Verify invalidate:: key space is separate
    let invalidate_record = manager
        .persistence()
        .find_idempotency_record("invalidate::repo-iso::")
        .unwrap();
    assert!(
        invalidate_record.is_none(),
        "invalidate key space should be empty"
    );

    // Create invalidation record
    manager
        .invalidate_repository("repo-iso", None, Some("test"))
        .unwrap();

    // Verify invalidate:: key exists now but doesn't interfere with index::
    let invalidate_record = manager
        .persistence()
        .find_idempotency_record("invalidate::repo-iso::")
        .unwrap();
    assert!(
        invalidate_record.is_some(),
        "invalidate record should exist"
    );

    // Index key still intact
    let index_record = manager
        .persistence()
        .find_idempotency_record("index::repo-iso::")
        .unwrap();
    assert!(index_record.is_some(), "index record should still exist");
    assert_eq!(
        index_record.unwrap().result_ref.as_deref(),
        Some(index_run_id.as_str()),
        "index record should still reference original run"
    );
}

use symforge::edit_safety::tee::{TEE_MAX_FILE_BYTES, TEE_MAX_FILES, Tee, TeeSnapshot};

#[test]
fn tee_snapshot_creates_recovery_copy_under_symforge_dir() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    let file_path = temp.path().join("src/lib.rs");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    let original = b"pub fn original() {}\n";
    std::fs::write(&file_path, original).unwrap();

    let snapshot = Tee::for_repo(temp.path()).snapshot(&file_path).unwrap();
    let record = match snapshot {
        TeeSnapshot::Created(record) => record,
        other => panic!("expected created snapshot, got {other:?}"),
    };

    assert_eq!(record.original_path, file_path);
    assert!(
        record
            .tee_path
            .starts_with(temp.path().join(".symforge").join("tee"))
    );
    assert_eq!(std::fs::read(&record.tee_path).unwrap(), original);
    assert!(record.recovery_hint().contains(".symforge/tee/"));
    assert!(record.recovery_hint().contains("src/lib.rs"));
}

#[test]
fn tee_snapshot_retains_at_most_twenty_records() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    let file_path = temp.path().join("src/lib.rs");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    std::fs::write(&file_path, b"pub fn original() {}\n").unwrap();

    let tee = Tee::for_repo(temp.path());
    let mut created_paths = Vec::new();
    for i in 0..=TEE_MAX_FILES {
        std::fs::write(&file_path, format!("pub fn version_{i}() {{}}\n")).unwrap();
        let record = match tee.snapshot(&file_path).unwrap() {
            TeeSnapshot::Created(record) => record,
            other => panic!("expected created snapshot, got {other:?}"),
        };
        created_paths.push(record.tee_path);
    }

    let tee_dir = temp.path().join(".symforge").join("tee");
    let retained = std::fs::read_dir(&tee_dir).unwrap().count();
    assert_eq!(retained, TEE_MAX_FILES);
    assert!(
        !created_paths[0].exists(),
        "oldest snapshot should be evicted"
    );
    assert!(
        created_paths.last().unwrap().exists(),
        "newest snapshot should be retained"
    );
}

#[test]
fn tee_snapshot_skips_files_larger_than_size_cap() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    let file_path = temp.path().join("src/large.rs");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    std::fs::write(&file_path, vec![b'x'; TEE_MAX_FILE_BYTES + 1]).unwrap();

    let snapshot = Tee::for_repo(temp.path()).snapshot(&file_path).unwrap();

    assert!(matches!(
        snapshot,
        TeeSnapshot::SkippedTooLarge {
            size,
            max_size
        } if size == TEE_MAX_FILE_BYTES + 1 && max_size == TEE_MAX_FILE_BYTES
    ));
    assert!(!temp.path().join(".symforge").join("tee").exists());
}

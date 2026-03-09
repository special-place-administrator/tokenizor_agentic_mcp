use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = index_runs, public)]
pub struct IndexRunRow {
    #[primary_key]
    pub run_id: String,
    #[index(btree)]
    pub repo_id: String,
    #[index(btree)]
    pub status: String,
    pub requested_at_unix_ms: u64,
    pub finished_at_unix_ms: Option<u64>,
    pub payload_json: String,
}

#[spacetimedb::table(accessor = checkpoints, public)]
pub struct CheckpointRow {
    #[primary_key]
    pub checkpoint_id: String,
    #[index(btree)]
    pub run_id: String,
    pub created_at_unix_ms: u64,
    pub cursor: String,
    pub payload_json: String,
}

#[spacetimedb::table(accessor = file_records, public)]
pub struct FileRecordRow {
    #[primary_key]
    pub record_key: String,
    #[index(btree)]
    pub run_id: String,
    #[index(btree)]
    pub repo_id: String,
    pub relative_path: String,
    pub committed_at_unix_ms: u64,
    pub payload_json: String,
}

#[spacetimedb::table(accessor = idempotency_records, public)]
pub struct IdempotencyRecordRow {
    #[primary_key]
    pub idempotency_key: String,
    pub operation: String,
    pub created_at_unix_ms: u64,
    pub payload_json: String,
}

#[spacetimedb::table(accessor = discovery_manifests, public)]
pub struct DiscoveryManifestRow {
    #[primary_key]
    pub run_id: String,
    pub discovered_at_unix_ms: u64,
    pub payload_json: String,
}

#[spacetimedb::table(accessor = repositories, public)]
pub struct RepositoryRow {
    #[primary_key]
    pub repo_id: String,
    #[index(btree)]
    pub status: String,
    pub root_uri: String,
    pub payload_json: String,
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {}

#[spacetimedb::reducer]
pub fn put_index_run(
    ctx: &ReducerContext,
    run_id: String,
    repo_id: String,
    status: String,
    requested_at_unix_ms: u64,
    finished_at_unix_ms: Option<u64>,
    payload_json: String,
) -> Result<(), String> {
    let row = IndexRunRow {
        run_id: run_id.clone(),
        repo_id,
        status,
        requested_at_unix_ms,
        finished_at_unix_ms,
        payload_json,
    };

    if ctx.db.index_runs().run_id().find(&run_id).is_some() {
        ctx.db.index_runs().run_id().update(row);
    } else {
        ctx.db.index_runs().insert(row);
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn put_checkpoint(
    ctx: &ReducerContext,
    checkpoint_id: String,
    run_id: String,
    created_at_unix_ms: u64,
    cursor: String,
    payload_json: String,
) -> Result<(), String> {
    let row = CheckpointRow {
        checkpoint_id: checkpoint_id.clone(),
        run_id,
        created_at_unix_ms,
        cursor,
        payload_json,
    };

    if ctx.db.checkpoints().checkpoint_id().find(&checkpoint_id).is_some() {
        ctx.db.checkpoints().checkpoint_id().update(row);
    } else {
        ctx.db.checkpoints().insert(row);
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn put_file_record(
    ctx: &ReducerContext,
    record_key: String,
    run_id: String,
    repo_id: String,
    relative_path: String,
    committed_at_unix_ms: u64,
    payload_json: String,
) -> Result<(), String> {
    let row = FileRecordRow {
        record_key: record_key.clone(),
        run_id,
        repo_id,
        relative_path,
        committed_at_unix_ms,
        payload_json,
    };

    if ctx.db.file_records().record_key().find(&record_key).is_some() {
        ctx.db.file_records().record_key().update(row);
    } else {
        ctx.db.file_records().insert(row);
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn put_idempotency_record(
    ctx: &ReducerContext,
    idempotency_key: String,
    operation: String,
    created_at_unix_ms: u64,
    payload_json: String,
) -> Result<(), String> {
    let row = IdempotencyRecordRow {
        idempotency_key: idempotency_key.clone(),
        operation,
        created_at_unix_ms,
        payload_json,
    };

    if ctx
        .db
        .idempotency_records()
        .idempotency_key()
        .find(&idempotency_key)
        .is_some()
    {
        ctx.db.idempotency_records().idempotency_key().update(row);
    } else {
        ctx.db.idempotency_records().insert(row);
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn put_discovery_manifest(
    ctx: &ReducerContext,
    run_id: String,
    discovered_at_unix_ms: u64,
    payload_json: String,
) -> Result<(), String> {
    let row = DiscoveryManifestRow {
        run_id: run_id.clone(),
        discovered_at_unix_ms,
        payload_json,
    };

    if ctx.db.discovery_manifests().run_id().find(&run_id).is_some() {
        ctx.db.discovery_manifests().run_id().update(row);
    } else {
        ctx.db.discovery_manifests().insert(row);
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn put_repository(
    ctx: &ReducerContext,
    repo_id: String,
    status: String,
    root_uri: String,
    payload_json: String,
) -> Result<(), String> {
    let row = RepositoryRow {
        repo_id: repo_id.clone(),
        status,
        root_uri,
        payload_json,
    };

    if ctx.db.repositories().repo_id().find(&repo_id).is_some() {
        ctx.db.repositories().repo_id().update(row);
    } else {
        ctx.db.repositories().insert(row);
    }

    Ok(())
}

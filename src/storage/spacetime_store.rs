use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use spacetimedb_sdk::{DbContext, Table};

use crate::domain::{
    Checkpoint, DiscoveryManifest, FileRecord, IdempotencyRecord, IndexRun, IndexRunStatus,
    Repository,
};
use crate::error::{Result, TokenizorError};

#[path = "../../spacetime/tokenizor/generated/mod.rs"]
mod tokenizor_spacetime_client;

use tokenizor_spacetime_client::checkpoints_table::CheckpointsTableAccess;
use tokenizor_spacetime_client::discovery_manifests_table::DiscoveryManifestsTableAccess;
use tokenizor_spacetime_client::file_records_table::FileRecordsTableAccess;
use tokenizor_spacetime_client::idempotency_records_table::IdempotencyRecordsTableAccess;
use tokenizor_spacetime_client::index_runs_table::IndexRunsTableAccess;
use tokenizor_spacetime_client::put_checkpoint_reducer::put_checkpoint;
use tokenizor_spacetime_client::put_discovery_manifest_reducer::put_discovery_manifest;
use tokenizor_spacetime_client::put_file_record_reducer::put_file_record;
use tokenizor_spacetime_client::put_idempotency_record_reducer::put_idempotency_record;
use tokenizor_spacetime_client::put_index_run_reducer::put_index_run;
use tokenizor_spacetime_client::put_repository_reducer::put_repository;
use tokenizor_spacetime_client::repositories_table::RepositoriesTableAccess;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const KEY_DELIMITER: char = '\u{001f}';

pub(crate) trait SpacetimeStateStore: Send + Sync {
    fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>>;
    fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>>;
    fn list_runs(&self) -> Result<Vec<IndexRun>>;
    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>>;
    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>>;
    fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>>;
    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>>;
    fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>>;
    fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>>;
    fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>>;
    fn save_run(&self, run: &IndexRun) -> Result<()>;
    fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()>;
    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()>;
    fn save_repository(&self, repository: &Repository) -> Result<()>;
    fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()>;
    fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()>;
    fn has_any_mutable_state(&self) -> Result<bool>;
}

pub(crate) struct SdkSpacetimeStateStore {
    endpoint: String,
    database: String,
    mutation_client: CachedConnection<ConnectedClient>,
}

impl SdkSpacetimeStateStore {
    pub(crate) fn new(endpoint: impl Into<String>, database: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            database: database.into(),
            mutation_client: CachedConnection::new(),
        }
    }

    fn with_connection<T>(
        &self,
        operation: &str,
        action: impl FnOnce(&tokenizor_spacetime_client::DbConnection) -> Result<T>,
    ) -> Result<T> {
        let client = ConnectedClient::connect(&self.endpoint, &self.database, operation)?;
        action(client.connection())
    }

    fn query<T>(
        &self,
        operation: &str,
        query_sql: String,
        extract: impl FnOnce(&tokenizor_spacetime_client::DbConnection) -> Result<T>,
    ) -> Result<T> {
        self.with_connection(operation, |connection| {
            subscribe_to_query(connection, operation, query_sql)?;
            extract(connection)
        })
    }

    fn query_all_tables<T>(
        &self,
        operation: &str,
        extract: impl FnOnce(&tokenizor_spacetime_client::DbConnection) -> Result<T>,
    ) -> Result<T> {
        self.with_connection(operation, |connection| {
            subscribe_to_all_tables(connection, operation)?;
            extract(connection)
        })
    }

    fn mutate(
        &self,
        operation: &str,
        invoke: impl FnOnce(
            &tokenizor_spacetime_client::RemoteReducers,
            mpsc::Sender<Result<()>>,
        ) -> std::result::Result<(), spacetimedb_sdk::Error>,
    ) -> Result<()> {
        self.mutation_client.with_client(
            || ConnectedClient::connect(&self.endpoint, &self.database, operation),
            |client| {
                let (tx, rx) = mpsc::channel::<Result<()>>();
                invoke(client.connection().reducers(), tx)
                    .map_err(|error| map_sdk_error(operation, error))?;
                wait_for_channel(operation, "reducer callback", rx)
            },
        )
    }
}

struct CachedConnection<C> {
    client: Mutex<Option<C>>,
}

impl<C> CachedConnection<C> {
    fn new() -> Self {
        Self {
            client: Mutex::new(None),
        }
    }

    fn with_client<T>(
        &self,
        connect: impl FnOnce() -> Result<C>,
        action: impl FnOnce(&C) -> Result<T>,
    ) -> Result<T> {
        let mut client = self.client.lock().map_err(|_| {
            TokenizorError::ControlPlane(
                "cached SpacetimeDB mutation connection lock poisoned".into(),
            )
        })?;
        if client.is_none() {
            *client = Some(connect()?);
        }
        let result = action(
            client
                .as_ref()
                .expect("cached SpacetimeDB mutation connection should exist"),
        );
        if result.is_err() {
            client.take();
        }
        result
    }
}

impl SpacetimeStateStore for SdkSpacetimeStateStore {
    fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>> {
        self.query(
            "find_run",
            format!(
                "SELECT * FROM index_runs WHERE run_id = {}",
                sql_string_literal(run_id)
            ),
            |connection| {
                connection
                    .db
                    .index_runs()
                    .iter()
                    .next()
                    .map(|row| {
                        deserialize_payload("find_run", "index run", &row.run_id, &row.payload_json)
                    })
                    .transpose()
            },
        )
    }

    fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>> {
        let status = enum_label(status, "index run status")?;
        self.query(
            "find_runs_by_status",
            format!(
                "SELECT * FROM index_runs WHERE status = {}",
                sql_string_literal(&status)
            ),
            |connection| {
                connection
                    .db
                    .index_runs()
                    .iter()
                    .map(|row| {
                        deserialize_payload(
                            "find_runs_by_status",
                            "index run",
                            &row.run_id,
                            &row.payload_json,
                        )
                    })
                    .collect()
            },
        )
    }

    fn list_runs(&self) -> Result<Vec<IndexRun>> {
        self.query(
            "list_runs",
            "SELECT * FROM index_runs".to_string(),
            |connection| {
                connection
                    .db
                    .index_runs()
                    .iter()
                    .map(|row| {
                        deserialize_payload(
                            "list_runs",
                            "index run",
                            &row.run_id,
                            &row.payload_json,
                        )
                    })
                    .collect()
            },
        )
    }

    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
        let mut runs: Vec<IndexRun> = self.query(
            "get_runs_by_repo",
            format!(
                "SELECT * FROM index_runs WHERE repo_id = {}",
                sql_string_literal(repo_id)
            ),
            |connection| {
                connection
                    .db
                    .index_runs()
                    .iter()
                    .map(|row| {
                        deserialize_payload(
                            "get_runs_by_repo",
                            "index run",
                            &row.run_id,
                            &row.payload_json,
                        )
                    })
                    .collect()
            },
        )?;
        runs.sort_by(|left, right| {
            right
                .requested_at_unix_ms
                .cmp(&left.requested_at_unix_ms)
                .then_with(|| right.run_id.cmp(&left.run_id))
        });
        Ok(runs)
    }

    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
        let runs = self.get_runs_by_repo(repo_id)?;
        Ok(runs
            .into_iter()
            .filter(|run| run.status == IndexRunStatus::Succeeded)
            .max_by_key(|run| run.requested_at_unix_ms))
    }

    fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>> {
        self.query(
            "get_repository",
            format!(
                "SELECT * FROM repositories WHERE repo_id = {}",
                sql_string_literal(repo_id)
            ),
            |connection| {
                connection
                    .db
                    .repositories()
                    .iter()
                    .next()
                    .map(|row| {
                        deserialize_payload(
                            "get_repository",
                            "repository",
                            &row.repo_id,
                            &row.payload_json,
                        )
                    })
                    .transpose()
            },
        )
    }

    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
        let mut records: Vec<FileRecord> = self.query(
            "get_file_records",
            format!(
                "SELECT * FROM file_records WHERE run_id = {}",
                sql_string_literal(run_id)
            ),
            |connection| {
                connection
                    .db
                    .file_records()
                    .iter()
                    .map(|row| {
                        deserialize_payload(
                            "get_file_records",
                            "file record",
                            &row.record_key,
                            &row.payload_json,
                        )
                    })
                    .collect()
            },
        )?;
        sort_file_records(&mut records);
        Ok(records)
    }

    fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
        let checkpoints: Vec<Checkpoint> = self.query(
            "get_latest_checkpoint",
            format!(
                "SELECT * FROM checkpoints WHERE run_id = {}",
                sql_string_literal(run_id)
            ),
            |connection| {
                connection
                    .db
                    .checkpoints()
                    .iter()
                    .map(|row| {
                        deserialize_payload(
                            "get_latest_checkpoint",
                            "checkpoint",
                            &row.checkpoint_id,
                            &row.payload_json,
                        )
                    })
                    .collect()
            },
        )?;

        Ok(checkpoints
            .into_iter()
            .max_by_key(|checkpoint| checkpoint.created_at_unix_ms))
    }

    fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
        self.query(
            "find_idempotency_record",
            format!(
                "SELECT * FROM idempotency_records WHERE idempotency_key = {}",
                sql_string_literal(key)
            ),
            |connection| {
                connection
                    .db
                    .idempotency_records()
                    .iter()
                    .next()
                    .map(|row| {
                        deserialize_payload(
                            "find_idempotency_record",
                            "idempotency record",
                            &row.idempotency_key,
                            &row.payload_json,
                        )
                    })
                    .transpose()
            },
        )
    }

    fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>> {
        self.query(
            "get_discovery_manifest",
            format!(
                "SELECT * FROM discovery_manifests WHERE run_id = {}",
                sql_string_literal(run_id)
            ),
            |connection| {
                connection
                    .db
                    .discovery_manifests()
                    .iter()
                    .next()
                    .map(|row| {
                        deserialize_payload(
                            "get_discovery_manifest",
                            "discovery manifest",
                            &row.run_id,
                            &row.payload_json,
                        )
                    })
                    .transpose()
            },
        )
    }

    fn save_run(&self, run: &IndexRun) -> Result<()> {
        let payload_json = serde_json::to_string(run)?;
        let status = enum_label(&run.status, "index run status")?;
        self.mutate("save_run", |reducers, tx| {
            reducers.put_index_run_then(
                run.run_id.clone(),
                run.repo_id.clone(),
                status,
                run.requested_at_unix_ms,
                run.finished_at_unix_ms,
                payload_json,
                move |_, outcome| {
                    let _ = tx.send(reducer_callback_result("put_index_run", outcome));
                },
            )
        })
    }

    fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
        for record in records {
            if record.run_id != run_id {
                return Err(TokenizorError::InvalidArgument(format!(
                    "file record run_id `{}` does not match requested run_id `{run_id}`",
                    record.run_id
                )));
            }

            let payload_json = serde_json::to_string(record)?;
            let record_key = file_record_key(run_id, &record.relative_path);
            self.mutate("save_file_records", |reducers, tx| {
                reducers.put_file_record_then(
                    record_key,
                    record.run_id.clone(),
                    record.repo_id.clone(),
                    record.relative_path.clone(),
                    record.committed_at_unix_ms,
                    payload_json,
                    move |_, outcome| {
                        let _ = tx.send(reducer_callback_result("put_file_record", outcome));
                    },
                )
            })?;
        }

        Ok(())
    }

    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        let payload_json = serde_json::to_string(checkpoint)?;
        let checkpoint_id = checkpoint_key(checkpoint);
        self.mutate("save_checkpoint", |reducers, tx| {
            reducers.put_checkpoint_then(
                checkpoint_id,
                checkpoint.run_id.clone(),
                checkpoint.created_at_unix_ms,
                checkpoint.cursor.clone(),
                payload_json,
                move |_, outcome| {
                    let _ = tx.send(reducer_callback_result("put_checkpoint", outcome));
                },
            )
        })
    }

    fn save_repository(&self, repository: &Repository) -> Result<()> {
        let payload_json = serde_json::to_string(repository)?;
        let status = enum_label(&repository.status, "repository status")?;
        self.mutate("save_repository", |reducers, tx| {
            reducers.put_repository_then(
                repository.repo_id.clone(),
                status,
                repository.root_uri.clone(),
                payload_json,
                move |_, outcome| {
                    let _ = tx.send(reducer_callback_result("put_repository", outcome));
                },
            )
        })
    }

    fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()> {
        let payload_json = serde_json::to_string(record)?;
        self.mutate("save_idempotency_record", |reducers, tx| {
            reducers.put_idempotency_record_then(
                record.idempotency_key.clone(),
                record.operation.clone(),
                record.created_at_unix_ms,
                payload_json,
                move |_, outcome| {
                    let _ = tx.send(reducer_callback_result("put_idempotency_record", outcome));
                },
            )
        })
    }

    fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
        let payload_json = serde_json::to_string(manifest)?;
        self.mutate("save_discovery_manifest", |reducers, tx| {
            reducers.put_discovery_manifest_then(
                manifest.run_id.clone(),
                manifest.discovered_at_unix_ms,
                payload_json,
                move |_, outcome| {
                    let _ = tx.send(reducer_callback_result("put_discovery_manifest", outcome));
                },
            )
        })
    }

    fn has_any_mutable_state(&self) -> Result<bool> {
        self.query_all_tables("has_any_mutable_state", |connection| {
            Ok(connection.db.index_runs().count() > 0
                || connection.db.checkpoints().count() > 0
                || connection.db.file_records().count() > 0
                || connection.db.idempotency_records().count() > 0
                || connection.db.discovery_manifests().count() > 0)
        })
    }
}

struct ConnectedClient {
    connection: tokenizor_spacetime_client::DbConnection,
    background_loop: Option<JoinHandle<()>>,
}

impl ConnectedClient {
    fn connect(endpoint: &str, database: &str, operation: &str) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Result<()>>();
        let on_connect_tx = tx.clone();
        let on_error_tx = tx;

        let connection = tokenizor_spacetime_client::DbConnection::builder()
            .with_uri(endpoint)
            .with_database_name(database)
            .on_connect(move |_, _, _| {
                let _ = on_connect_tx.send(Ok(()));
            })
            .on_connect_error(move |_, error| {
                let _ = on_error_tx.send(Err(map_sdk_error("connect", error)));
            })
            .build()
            .map_err(|error| map_sdk_error(operation, error))?;

        let background_loop = connection.run_threaded();
        wait_for_channel(operation, "SpacetimeDB connection", rx)?;

        Ok(Self {
            connection,
            background_loop: Some(background_loop),
        })
    }

    fn connection(&self) -> &tokenizor_spacetime_client::DbConnection {
        &self.connection
    }
}

impl Drop for ConnectedClient {
    fn drop(&mut self) {
        let _ = self.connection.disconnect();
        if let Some(handle) = self.background_loop.take() {
            let _ = handle.join();
        }
    }
}

fn subscribe_to_query(
    connection: &tokenizor_spacetime_client::DbConnection,
    operation: &str,
    query_sql: String,
) -> Result<()> {
    let (tx, rx) = mpsc::channel::<Result<()>>();
    let applied_tx = tx.clone();
    let error_tx = tx;
    let operation_name = operation.to_string();
    connection
        .subscription_builder()
        .on_applied(move |_| {
            let _ = applied_tx.send(Ok(()));
        })
        .on_error(move |_, error| {
            let _ = error_tx.send(Err(map_sdk_error(&operation_name, error)));
        })
        .subscribe(query_sql);
    wait_for_channel(operation, "subscription", rx)
}

fn subscribe_to_all_tables(
    connection: &tokenizor_spacetime_client::DbConnection,
    operation: &str,
) -> Result<()> {
    let (tx, rx) = mpsc::channel::<Result<()>>();
    let applied_tx = tx.clone();
    let error_tx = tx;
    let operation_name = operation.to_string();
    connection
        .subscription_builder()
        .on_applied(move |_| {
            let _ = applied_tx.send(Ok(()));
        })
        .on_error(move |_, error| {
            let _ = error_tx.send(Err(map_sdk_error(&operation_name, error)));
        })
        .subscribe_to_all_tables();
    wait_for_channel(operation, "full-table subscription", rx)
}

fn wait_for_channel(operation: &str, wait_target: &str, rx: Receiver<Result<()>>) -> Result<()> {
    match rx.recv_timeout(if wait_target == "SpacetimeDB connection" {
        CONNECT_TIMEOUT
    } else {
        OPERATION_TIMEOUT
    }) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(TokenizorError::ControlPlane(format!(
            "timed out waiting for {wait_target} during `{operation}`"
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(TokenizorError::ControlPlane(format!(
            "{wait_target} closed unexpectedly during `{operation}`"
        ))),
    }
}

fn reducer_callback_result<E>(
    reducer_name: &str,
    outcome: std::result::Result<std::result::Result<(), String>, E>,
) -> Result<()>
where
    E: std::fmt::Display,
{
    match outcome {
        Ok(Ok(())) => Ok(()),
        Ok(Err(message)) => Err(TokenizorError::ControlPlane(format!(
            "SpacetimeDB reducer `{reducer_name}` rejected the request: {message}"
        ))),
        Err(error) => Err(TokenizorError::ControlPlane(format!(
            "SpacetimeDB reducer `{reducer_name}` failed internally: {error}"
        ))),
    }
}

fn map_sdk_error(operation: &str, error: impl std::fmt::Display) -> TokenizorError {
    TokenizorError::ControlPlane(format!(
        "SpacetimeDB SDK error during `{operation}`: {error}"
    ))
}

fn deserialize_payload<T: DeserializeOwned>(
    operation: &str,
    entity_name: &str,
    entity_id: &str,
    payload_json: &str,
) -> Result<T> {
    serde_json::from_str(payload_json).map_err(|error| {
        TokenizorError::Integrity(format!(
            "SpacetimeDB returned a corrupt {entity_name} payload for `{entity_id}` during `{operation}`: {error}"
        ))
    })
}

fn enum_label<T: Serialize>(value: &T, value_name: &str) -> Result<String> {
    let serialized = serde_json::to_value(value)?;
    serialized.as_str().map(str::to_string).ok_or_else(|| {
        TokenizorError::Serialization(format!("{value_name} did not serialize to a string label"))
    })
}

fn file_record_key(run_id: &str, relative_path: &str) -> String {
    format!("{run_id}{KEY_DELIMITER}{relative_path}")
}

fn checkpoint_key(checkpoint: &Checkpoint) -> String {
    format!(
        "{}{}{}{}{}",
        checkpoint.run_id,
        KEY_DELIMITER,
        checkpoint.created_at_unix_ms,
        KEY_DELIMITER,
        checkpoint.cursor
    )
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sort_file_records(records: &mut [FileRecord]) {
    records.sort_by(|left, right| {
        left.relative_path
            .to_lowercase()
            .cmp(&right.relative_path.to_lowercase())
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::CachedConnection;
    use crate::error::TokenizorError;

    #[test]
    fn cached_connection_reuses_existing_client_across_operations() {
        let cache = CachedConnection::new();
        let connect_calls = Arc::new(AtomicUsize::new(0));

        let first = cache
            .with_client(
                {
                    let connect_calls = Arc::clone(&connect_calls);
                    move || Ok(connect_calls.fetch_add(1, Ordering::SeqCst) + 1)
                },
                |client| Ok(*client),
            )
            .expect("first operation should connect");
        let second = cache
            .with_client(
                {
                    let connect_calls = Arc::clone(&connect_calls);
                    move || Ok(connect_calls.fetch_add(1, Ordering::SeqCst) + 1)
                },
                |client| Ok(*client),
            )
            .expect("second operation should reuse the cached client");

        assert_eq!(first, 1);
        assert_eq!(second, 1);
        assert_eq!(connect_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cached_connection_drops_failed_client_and_reconnects_on_next_operation() {
        let cache = CachedConnection::new();
        let connect_calls = Arc::new(AtomicUsize::new(0));

        let error = cache
            .with_client(
                {
                    let connect_calls = Arc::clone(&connect_calls);
                    move || Ok(connect_calls.fetch_add(1, Ordering::SeqCst) + 1)
                },
                |_client| -> crate::error::Result<usize> {
                    Err(TokenizorError::ControlPlane("forced failure".into()))
                },
            )
            .expect_err("failed operation should surface the error");
        assert!(matches!(error, TokenizorError::ControlPlane(_)));

        let next = cache
            .with_client(
                {
                    let connect_calls = Arc::clone(&connect_calls);
                    move || Ok(connect_calls.fetch_add(1, Ordering::SeqCst) + 1)
                },
                |client| Ok(*client),
            )
            .expect("next operation should reconnect");

        assert_eq!(next, 2);
        assert_eq!(connect_calls.load(Ordering::SeqCst), 2);
    }
}

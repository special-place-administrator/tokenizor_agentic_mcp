use tracing::warn;

use crate::domain::{
    BatchRetrievalRequest, BatchRetrievalResponseData, BatchRetrievalResultItem, FileOutcomeStatus,
    FileOutlineResponse, FileRecord, NextAction, OutlineSymbol, PersistedFileOutcome, Provenance,
    RepoOutlineCoverage, RepoOutlineEntry, RepoOutlineResponse, RepositoryStatus, RequestGateError,
    ResultEnvelope, RetrievalOutcome, SearchResultItem, SupportTier, SymbolCoverage, SymbolKind,
    SymbolResultItem, SymbolSearchResponse, TrustLevel, VerifiedCodeSliceResponse,
    VerifiedSourceResponse,
};
use crate::error::{Result, TokenizorError};
use crate::storage::{BlobStore, RegistryQuery, digest_hex};

use super::RunManager;

pub fn check_request_gate(
    repo_id: &str,
    persistence: &dyn RegistryQuery,
    run_manager: &RunManager,
) -> Result<()> {
    let repo = persistence.get_repository(repo_id)?;

    let repo = match repo {
        Some(r) => r,
        None => {
            return Err(gate_error(RequestGateError::NoActiveContext));
        }
    };

    match repo.status {
        RepositoryStatus::Invalidated => {
            return Err(gate_error(RequestGateError::RepositoryInvalidated {
                reason: repo.invalidation_reason.clone(),
            }));
        }
        RepositoryStatus::Failed => {
            return Err(gate_error(RequestGateError::RepositoryFailed));
        }
        RepositoryStatus::Degraded => {
            return Err(gate_error(RequestGateError::RepositoryDegraded));
        }
        RepositoryStatus::Quarantined => {
            return Err(gate_error(RequestGateError::RepositoryQuarantined {
                reason: repo.quarantine_reason.clone(),
            }));
        }
        RepositoryStatus::Pending | RepositoryStatus::Ready => {}
    }

    let repo_runs = persistence.get_runs_by_repo(repo_id)?;
    if let Some(active_run) = repo_runs.iter().find(|run| {
        matches!(
            run.status,
            crate::domain::IndexRunStatus::Queued | crate::domain::IndexRunStatus::Running
        )
    }) {
        return Err(gate_error(RequestGateError::ActiveMutation {
            run_id: active_run.run_id.clone(),
        }));
    }

    if let Some(active_run_id) = run_manager.get_active_run_id(repo_id) {
        return Err(gate_error(RequestGateError::ActiveMutation {
            run_id: active_run_id,
        }));
    }

    let latest_completed = repo_runs
        .iter()
        .find(|run| run.status == crate::domain::IndexRunStatus::Succeeded);
    if latest_completed.is_none() {
        if repo_runs.is_empty() {
            return Err(gate_error(RequestGateError::NeverIndexed));
        } else {
            let latest_status = repo_runs[0].status.clone();
            return Err(gate_error(RequestGateError::NoSuccessfulRuns {
                latest_status,
            }));
        }
    }

    Ok(())
}

pub fn search_text(
    repo_id: &str,
    query: &str,
    persistence: &dyn RegistryQuery,
    run_manager: &RunManager,
    blob_store: &dyn BlobStore,
) -> Result<ResultEnvelope<Vec<SearchResultItem>>> {
    if query.is_empty() {
        return Err(TokenizorError::InvalidArgument(
            "search query must not be empty".to_string(),
        ));
    }

    check_request_gate(repo_id, persistence, run_manager)?;

    search_text_ungated(repo_id, query, persistence, blob_store)
}

fn search_text_ungated(
    repo_id: &str,
    query: &str,
    persistence: &dyn RegistryQuery,
    blob_store: &dyn BlobStore,
) -> Result<ResultEnvelope<Vec<SearchResultItem>>> {
    // Defense-in-depth: gate should have caught this, but guard against direct calls
    let latest_run = match persistence.get_latest_completed_run(repo_id)? {
        Some(run) => run,
        None => {
            return Ok(ResultEnvelope {
                outcome: RetrievalOutcome::NotIndexed,
                trust: TrustLevel::Verified,
                provenance: None,
                data: None,
                next_action: None,
            });
        }
    };

    let run_provenance = Provenance {
        run_id: latest_run.run_id.clone(),
        committed_at_unix_ms: latest_run.requested_at_unix_ms,
        repo_id: repo_id.to_string(),
    };

    let file_records = persistence.get_file_records(&latest_run.run_id)?;

    let mut results: Vec<SearchResultItem> = Vec::new();

    for record in &file_records {
        match &record.outcome {
            PersistedFileOutcome::Quarantined { .. } => continue,
            _ => {}
        }

        let blob_bytes = match blob_store.read_bytes(&record.blob_id) {
            Ok(bytes) => bytes,
            Err(_) => {
                warn!(
                    blob_id = %record.blob_id,
                    path = %record.relative_path,
                    "blob read failed; skipping file"
                );
                continue;
            }
        };

        let computed_hash = digest_hex(&blob_bytes);
        if computed_hash != record.blob_id {
            warn!(
                blob_id = %record.blob_id,
                computed = %computed_hash,
                path = %record.relative_path,
                "blob integrity mismatch; skipping file"
            );
            continue;
        }

        let content = match std::str::from_utf8(&blob_bytes) {
            Ok(s) => s,
            Err(_) => {
                warn!(
                    path = %record.relative_path,
                    "non-UTF-8 blob content; skipping file for text search"
                );
                continue;
            }
        };

        let item_provenance = Provenance {
            run_id: record.run_id.clone(),
            committed_at_unix_ms: record.committed_at_unix_ms,
            repo_id: record.repo_id.clone(),
        };

        for (line_idx, line) in content.lines().enumerate() {
            let mut search_start = 0;
            while let Some(rel_offset) = line[search_start..].find(query) {
                let abs_offset = search_start + rel_offset;
                results.push(SearchResultItem {
                    relative_path: record.relative_path.clone(),
                    language: record.language.clone(),
                    line_number: (line_idx as u32) + 1,
                    line_content: line.to_string(),
                    match_offset: abs_offset as u32,
                    match_length: query.len() as u32,
                    provenance: item_provenance.clone(),
                });
                search_start = abs_offset + query.len();
            }
        }
    }

    if results.is_empty() {
        Ok(ResultEnvelope {
            outcome: RetrievalOutcome::Empty,
            trust: TrustLevel::Verified,
            provenance: Some(run_provenance),
            data: None,
            next_action: None,
        })
    } else {
        Ok(ResultEnvelope {
            outcome: RetrievalOutcome::Success,
            trust: TrustLevel::Verified,
            provenance: Some(run_provenance),
            data: Some(results),
            next_action: None,
        })
    }
}

pub fn search_symbols(
    repo_id: &str,
    query: &str,
    kind_filter: Option<SymbolKind>,
    persistence: &dyn RegistryQuery,
    run_manager: &RunManager,
) -> Result<ResultEnvelope<SymbolSearchResponse>> {
    if query.is_empty() {
        return Err(TokenizorError::InvalidArgument(
            "search query must not be empty".to_string(),
        ));
    }

    check_request_gate(repo_id, persistence, run_manager)?;

    search_symbols_ungated(repo_id, query, kind_filter, persistence)
}

fn search_symbols_ungated(
    repo_id: &str,
    query: &str,
    kind_filter: Option<SymbolKind>,
    persistence: &dyn RegistryQuery,
) -> Result<ResultEnvelope<SymbolSearchResponse>> {
    // Defense-in-depth: gate should have caught this, but guard against direct calls
    let latest_run = match persistence.get_latest_completed_run(repo_id)? {
        Some(run) => run,
        None => {
            return Ok(ResultEnvelope {
                outcome: RetrievalOutcome::NotIndexed,
                trust: TrustLevel::Verified,
                provenance: None,
                data: None,
                next_action: None,
            });
        }
    };

    let run_provenance = Provenance {
        run_id: latest_run.run_id.clone(),
        committed_at_unix_ms: latest_run.requested_at_unix_ms,
        repo_id: repo_id.to_string(),
    };

    let file_records = persistence.get_file_records(&latest_run.run_id)?;

    let mut matches: Vec<SymbolResultItem> = Vec::new();
    let mut files_searched: u32 = 0;
    let mut files_without_symbols: u32 = 0;
    let mut files_skipped_quarantined: u32 = 0;

    let query_lower = query.to_lowercase();

    for record in &file_records {
        match &record.outcome {
            PersistedFileOutcome::Quarantined { .. } => {
                files_skipped_quarantined += 1;
                continue;
            }
            PersistedFileOutcome::EmptySymbols | PersistedFileOutcome::Failed { .. } => {
                files_without_symbols += 1;
                continue;
            }
            PersistedFileOutcome::Committed => {}
        }

        if record.symbols.is_empty() {
            files_without_symbols += 1;
            continue;
        }

        files_searched += 1;

        let item_provenance = Provenance {
            run_id: record.run_id.clone(),
            committed_at_unix_ms: record.committed_at_unix_ms,
            repo_id: record.repo_id.clone(),
        };

        for symbol in &record.symbols {
            if !symbol.name.to_lowercase().contains(&query_lower) {
                continue;
            }

            if let Some(ref kind) = kind_filter {
                if symbol.kind != *kind {
                    continue;
                }
            }

            matches.push(SymbolResultItem {
                symbol_name: symbol.name.clone(),
                symbol_kind: symbol.kind,
                relative_path: record.relative_path.clone(),
                language: record.language.clone(),
                line_range: symbol.line_range,
                byte_range: symbol.byte_range,
                depth: symbol.depth,
                provenance: item_provenance.clone(),
            });
        }
    }

    let coverage = SymbolCoverage {
        files_searched,
        files_without_symbols,
        files_skipped_quarantined,
    };

    if matches.is_empty() {
        // Deliberate deviation from text search: Empty results still carry data: Some(...)
        // with coverage metadata so callers know coverage was partial (AC 2).
        Ok(ResultEnvelope {
            outcome: RetrievalOutcome::Empty,
            trust: TrustLevel::Verified,
            provenance: Some(run_provenance),
            data: Some(SymbolSearchResponse { matches, coverage }),
            next_action: None,
        })
    } else {
        Ok(ResultEnvelope {
            outcome: RetrievalOutcome::Success,
            trust: TrustLevel::Verified,
            provenance: Some(run_provenance),
            data: Some(SymbolSearchResponse { matches, coverage }),
            next_action: None,
        })
    }
}

pub fn get_file_outline(
    repo_id: &str,
    relative_path: &str,
    persistence: &dyn RegistryQuery,
    run_manager: &RunManager,
) -> Result<ResultEnvelope<FileOutlineResponse>> {
    check_request_gate(repo_id, persistence, run_manager)?;

    get_file_outline_ungated(repo_id, relative_path, persistence)
}

fn get_file_outline_ungated(
    repo_id: &str,
    relative_path: &str,
    persistence: &dyn RegistryQuery,
) -> Result<ResultEnvelope<FileOutlineResponse>> {
    let latest_run = match persistence.get_latest_completed_run(repo_id)? {
        Some(run) => run,
        None => {
            return Ok(ResultEnvelope {
                outcome: RetrievalOutcome::NotIndexed,
                trust: TrustLevel::Verified,
                provenance: None,
                data: None,
                next_action: None,
            });
        }
    };

    let file_records = persistence.get_file_records(&latest_run.run_id)?;
    let record = file_records
        .iter()
        .find(|record| record.relative_path == relative_path)
        .ok_or_else(|| {
            TokenizorError::InvalidArgument(format!("file not found in index: {relative_path}"))
        })?;

    let provenance = file_record_provenance(record);

    // Outlines are targeted retrieval requests, not discovery searches. Quarantined files are
    // surfaced explicitly so the caller can diagnose the trust issue for the requested path.
    if matches!(record.outcome, PersistedFileOutcome::Quarantined { .. }) {
        return Ok(ResultEnvelope {
            outcome: RetrievalOutcome::Quarantined,
            trust: TrustLevel::Quarantined,
            provenance: Some(provenance),
            data: None,
            next_action: Some(NextAction::Repair),
        });
    }

    let mut symbols: Vec<OutlineSymbol> = record
        .symbols
        .iter()
        .map(|symbol| OutlineSymbol {
            name: symbol.name.clone(),
            kind: symbol.kind,
            line_range: symbol.line_range,
            byte_range: symbol.byte_range,
            depth: symbol.depth,
            sort_order: symbol.sort_order,
        })
        .collect();
    symbols.sort_by_key(|symbol| symbol.sort_order);

    let response = FileOutlineResponse {
        relative_path: record.relative_path.clone(),
        language: record.language.clone(),
        byte_len: record.byte_len,
        symbols,
        has_symbol_support: has_symbol_support(record),
    };

    Ok(ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(provenance),
        data: Some(response),
        next_action: None,
    })
}

pub fn get_repo_outline(
    repo_id: &str,
    persistence: &dyn RegistryQuery,
    run_manager: &RunManager,
) -> Result<ResultEnvelope<RepoOutlineResponse>> {
    check_request_gate(repo_id, persistence, run_manager)?;

    get_repo_outline_ungated(repo_id, persistence)
}

fn get_repo_outline_ungated(
    repo_id: &str,
    persistence: &dyn RegistryQuery,
) -> Result<ResultEnvelope<RepoOutlineResponse>> {
    let latest_run = match persistence.get_latest_completed_run(repo_id)? {
        Some(run) => run,
        None => {
            return Ok(ResultEnvelope {
                outcome: RetrievalOutcome::NotIndexed,
                trust: TrustLevel::Verified,
                provenance: None,
                data: None,
                next_action: None,
            });
        }
    };

    let run_provenance =
        run_provenance(repo_id, &latest_run.run_id, latest_run.requested_at_unix_ms);
    let file_records = persistence.get_file_records(&latest_run.run_id)?;

    let mut entries = Vec::with_capacity(file_records.len());
    let mut files_with_symbols = 0u32;
    let mut files_without_symbols = 0u32;
    let mut files_quarantined = 0u32;
    let mut files_failed = 0u32;

    for record in &file_records {
        let status = FileOutcomeStatus::from(&record.outcome);
        match &record.outcome {
            PersistedFileOutcome::Committed if !record.symbols.is_empty() => {
                files_with_symbols += 1;
            }
            PersistedFileOutcome::Committed | PersistedFileOutcome::EmptySymbols => {
                files_without_symbols += 1;
            }
            PersistedFileOutcome::Failed { .. } => {
                files_failed += 1;
            }
            PersistedFileOutcome::Quarantined { .. } => {
                files_quarantined += 1;
            }
        }

        entries.push(RepoOutlineEntry {
            relative_path: record.relative_path.clone(),
            language: record.language.clone(),
            byte_len: record.byte_len,
            symbol_count: record.symbols.len() as u32,
            status,
        });
    }

    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let coverage = RepoOutlineCoverage {
        total_files: file_records.len() as u32,
        files_with_symbols,
        files_without_symbols,
        files_quarantined,
        files_failed,
    };
    let response = RepoOutlineResponse {
        files: entries,
        coverage,
    };

    Ok(ResultEnvelope {
        outcome: if response.files.is_empty() {
            RetrievalOutcome::Empty
        } else {
            RetrievalOutcome::Success
        },
        trust: TrustLevel::Verified,
        provenance: Some(run_provenance),
        data: Some(response),
        next_action: None,
    })
}

pub fn get_symbol(
    repo_id: &str,
    relative_path: &str,
    symbol_name: &str,
    kind_filter: Option<SymbolKind>,
    persistence: &dyn RegistryQuery,
    run_manager: &RunManager,
    blob_store: &dyn BlobStore,
) -> Result<ResultEnvelope<VerifiedSourceResponse>> {
    if symbol_name.is_empty() {
        return Err(TokenizorError::InvalidArgument(
            "symbol_name must not be empty".to_string(),
        ));
    }
    if relative_path.is_empty() {
        return Err(TokenizorError::InvalidArgument(
            "relative_path must not be empty".to_string(),
        ));
    }

    check_request_gate(repo_id, persistence, run_manager)?;

    get_symbol_ungated(
        repo_id,
        relative_path,
        symbol_name,
        kind_filter,
        persistence,
        blob_store,
    )
}

fn get_symbol_ungated(
    repo_id: &str,
    relative_path: &str,
    symbol_name: &str,
    kind_filter: Option<SymbolKind>,
    persistence: &dyn RegistryQuery,
    blob_store: &dyn BlobStore,
) -> Result<ResultEnvelope<VerifiedSourceResponse>> {
    let latest_run = match persistence.get_latest_completed_run(repo_id)? {
        Some(run) => run,
        None => {
            return Ok(ResultEnvelope {
                outcome: RetrievalOutcome::NotIndexed,
                trust: TrustLevel::Verified,
                provenance: None,
                data: None,
                next_action: None,
            });
        }
    };

    let file_records = persistence.get_file_records(&latest_run.run_id)?;
    let record = file_records
        .iter()
        .find(|r| r.relative_path == relative_path)
        .ok_or_else(|| {
            TokenizorError::InvalidArgument(format!("file not found in index: {relative_path}"))
        })?;

    let provenance = file_record_provenance(record);

    if matches!(record.outcome, PersistedFileOutcome::Quarantined { .. }) {
        return Ok(ResultEnvelope {
            outcome: RetrievalOutcome::Quarantined,
            trust: TrustLevel::Quarantined,
            provenance: Some(provenance),
            data: None,
            next_action: Some(NextAction::Repair),
        });
    }

    let mut candidates: Vec<&crate::domain::SymbolRecord> = record
        .symbols
        .iter()
        .filter(|s| s.name == symbol_name)
        .collect();

    if let Some(ref kind) = kind_filter {
        candidates.retain(|s| s.kind == *kind);
    }

    let symbol = candidates
        .iter()
        .min_by_key(|s| s.sort_order)
        .ok_or_else(|| {
            TokenizorError::InvalidArgument(format!(
                "symbol not found: `{symbol_name}` in file: {relative_path}"
            ))
        })?;

    let blob_bytes = match blob_store.read_bytes(&record.blob_id) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Ok(ResultEnvelope {
                outcome: RetrievalOutcome::Blocked {
                    reason: format!("blob read failed for blob_id `{}`: {e}", record.blob_id),
                },
                trust: TrustLevel::Suspect,
                provenance: Some(provenance),
                data: None,
                next_action: Some(NextAction::Repair),
            });
        }
    };

    let computed_hash = digest_hex(&blob_bytes);
    if computed_hash != record.blob_id {
        return Ok(ResultEnvelope {
            outcome: RetrievalOutcome::Blocked {
                reason: "blob integrity verification failed: content hash mismatch".to_string(),
            },
            trust: TrustLevel::Suspect,
            provenance: Some(provenance),
            data: None,
            next_action: Some(NextAction::Reindex),
        });
    }

    let (start, end) = symbol.byte_range;
    if end as usize > blob_bytes.len() || start > end {
        return Ok(ResultEnvelope {
            outcome: RetrievalOutcome::Blocked {
                reason: format!(
                    "symbol byte range ({start}..{end}) exceeds blob size ({}) or is malformed",
                    blob_bytes.len()
                ),
            },
            trust: TrustLevel::Suspect,
            provenance: Some(provenance),
            data: None,
            next_action: Some(NextAction::Reindex),
        });
    }

    let source_bytes = &blob_bytes[start as usize..end as usize];
    let source = match String::from_utf8(source_bytes.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            return Ok(ResultEnvelope {
                outcome: RetrievalOutcome::Blocked {
                    reason: "symbol source contains non-UTF-8 bytes".to_string(),
                },
                trust: TrustLevel::Suspect,
                provenance: Some(provenance),
                data: None,
                next_action: Some(NextAction::Repair),
            });
        }
    };

    let response = VerifiedSourceResponse {
        relative_path: record.relative_path.clone(),
        language: record.language.clone(),
        symbol_name: symbol.name.clone(),
        symbol_kind: symbol.kind,
        line_range: symbol.line_range,
        byte_range: symbol.byte_range,
        source,
    };

    Ok(ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(provenance),
        data: Some(response),
        next_action: None,
    })
}

fn has_symbol_support(record: &FileRecord) -> bool {
    match &record.outcome {
        PersistedFileOutcome::Committed => language_has_symbol_support(&record.language),
        PersistedFileOutcome::EmptySymbols => true,
        PersistedFileOutcome::Failed { .. } | PersistedFileOutcome::Quarantined { .. } => false,
    }
}

fn language_has_symbol_support(language: &crate::domain::LanguageId) -> bool {
    language.support_tier() != SupportTier::Unsupported
}

fn run_provenance(repo_id: &str, run_id: &str, committed_at_unix_ms: u64) -> Provenance {
    Provenance {
        run_id: run_id.to_string(),
        committed_at_unix_ms,
        repo_id: repo_id.to_string(),
    }
}

fn file_record_provenance(record: &FileRecord) -> Provenance {
    run_provenance(&record.repo_id, &record.run_id, record.committed_at_unix_ms)
}

fn gate_error(error: RequestGateError) -> TokenizorError {
    let action = error.next_action();
    TokenizorError::RequestGated {
        gate_error: format!("{error} [next_action: {action}]"),
    }
}

fn missing_batch_result(
    provenance: Option<Provenance>,
) -> ResultEnvelope<BatchRetrievalResponseData> {
    ResultEnvelope {
        outcome: RetrievalOutcome::Missing,
        trust: TrustLevel::Verified,
        provenance,
        data: None,
        next_action: None,
    }
}

fn blocked_batch_result(
    reason: impl Into<String>,
    provenance: Option<Provenance>,
    next_action: Option<NextAction>,
) -> ResultEnvelope<BatchRetrievalResponseData> {
    ResultEnvelope {
        outcome: RetrievalOutcome::Blocked {
            reason: reason.into(),
        },
        trust: TrustLevel::Suspect,
        provenance,
        data: None,
        next_action,
    }
}

fn quarantined_batch_result(
    provenance: Option<Provenance>,
) -> ResultEnvelope<BatchRetrievalResponseData> {
    ResultEnvelope {
        outcome: RetrievalOutcome::Quarantined,
        trust: TrustLevel::Quarantined,
        provenance,
        data: None,
        next_action: Some(NextAction::Repair),
    }
}

fn batch_symbol_result(
    relative_path: &str,
    symbol_name: &str,
    kind_filter: Option<SymbolKind>,
    file_records: &[FileRecord],
    blob_store: &dyn BlobStore,
) -> ResultEnvelope<BatchRetrievalResponseData> {
    let record = match file_records
        .iter()
        .find(|r| r.relative_path == relative_path)
    {
        Some(record) => record,
        None => return missing_batch_result(None),
    };

    let provenance = Some(file_record_provenance(record));
    if matches!(record.outcome, PersistedFileOutcome::Quarantined { .. }) {
        return quarantined_batch_result(provenance);
    }

    let mut candidates: Vec<&crate::domain::SymbolRecord> = record
        .symbols
        .iter()
        .filter(|s| s.name == symbol_name)
        .collect();

    if let Some(ref kind) = kind_filter {
        candidates.retain(|s| s.kind == *kind);
    }

    let symbol = match candidates.iter().min_by_key(|s| s.sort_order) {
        Some(symbol) => *symbol,
        None => return missing_batch_result(provenance),
    };

    let blob_bytes = match blob_store.read_bytes(&record.blob_id) {
        Ok(bytes) => bytes,
        Err(error) => {
            return blocked_batch_result(
                format!("blob read failed for blob_id `{}`: {error}", record.blob_id),
                provenance,
                Some(NextAction::Repair),
            );
        }
    };

    let computed_hash = digest_hex(&blob_bytes);
    if computed_hash != record.blob_id {
        return blocked_batch_result(
            "blob integrity verification failed: content hash mismatch",
            provenance,
            Some(NextAction::Reindex),
        );
    }

    let (start, end) = symbol.byte_range;
    if end as usize > blob_bytes.len() || start > end {
        return blocked_batch_result(
            format!(
                "symbol byte range ({start}..{end}) exceeds blob size ({}) or is malformed",
                blob_bytes.len()
            ),
            provenance,
            Some(NextAction::Reindex),
        );
    }

    let source_bytes = &blob_bytes[start as usize..end as usize];
    let source = match String::from_utf8(source_bytes.to_vec()) {
        Ok(source) => source,
        Err(_) => {
            return blocked_batch_result(
                "symbol source contains non-UTF-8 bytes",
                provenance,
                Some(NextAction::Repair),
            );
        }
    };

    ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance,
        data: Some(BatchRetrievalResponseData::Symbol(VerifiedSourceResponse {
            relative_path: record.relative_path.clone(),
            language: record.language.clone(),
            symbol_name: symbol.name.clone(),
            symbol_kind: symbol.kind,
            line_range: symbol.line_range,
            byte_range: symbol.byte_range,
            source,
        })),
        next_action: None,
    }
}

fn byte_range_to_line_range(blob_bytes: &[u8], start: u32, end: u32) -> (u32, u32) {
    let start_idx = start as usize;
    let end_idx = end as usize;
    let start_line = blob_bytes[..start_idx]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as u32;
    let line_breaks_in_slice = blob_bytes[start_idx..end_idx]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as u32;
    (start_line, start_line + line_breaks_in_slice)
}

fn batch_code_slice_result(
    relative_path: &str,
    byte_range: (u32, u32),
    file_records: &[FileRecord],
    blob_store: &dyn BlobStore,
) -> ResultEnvelope<BatchRetrievalResponseData> {
    let record = match file_records
        .iter()
        .find(|r| r.relative_path == relative_path)
    {
        Some(record) => record,
        None => return missing_batch_result(None),
    };

    let provenance = Some(file_record_provenance(record));
    if matches!(record.outcome, PersistedFileOutcome::Quarantined { .. }) {
        return quarantined_batch_result(provenance);
    }

    let blob_bytes = match blob_store.read_bytes(&record.blob_id) {
        Ok(bytes) => bytes,
        Err(error) => {
            return blocked_batch_result(
                format!("blob read failed for blob_id `{}`: {error}", record.blob_id),
                provenance,
                Some(NextAction::Repair),
            );
        }
    };

    let computed_hash = digest_hex(&blob_bytes);
    if computed_hash != record.blob_id {
        return blocked_batch_result(
            "blob integrity verification failed: content hash mismatch",
            provenance,
            Some(NextAction::Reindex),
        );
    }

    let (start, end) = byte_range;
    if end as usize > blob_bytes.len() || start > end {
        return blocked_batch_result(
            format!(
                "requested byte range ({start}..{end}) exceeds blob size ({}) or is malformed",
                blob_bytes.len()
            ),
            provenance,
            Some(NextAction::Repair),
        );
    }

    let source_bytes = &blob_bytes[start as usize..end as usize];
    let source = match String::from_utf8(source_bytes.to_vec()) {
        Ok(source) => source,
        Err(_) => {
            return blocked_batch_result(
                "code slice contains non-UTF-8 bytes",
                provenance,
                Some(NextAction::Repair),
            );
        }
    };

    ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance,
        data: Some(BatchRetrievalResponseData::CodeSlice(
            VerifiedCodeSliceResponse {
                relative_path: record.relative_path.clone(),
                language: record.language.clone(),
                line_range: byte_range_to_line_range(&blob_bytes, start, end),
                byte_range,
                source,
            },
        )),
        next_action: None,
    }
}

pub fn get_symbols(
    repo_id: &str,
    requests: &[BatchRetrievalRequest],
    persistence: &dyn RegistryQuery,
    run_manager: &super::RunManager,
    blob_store: &dyn BlobStore,
) -> Result<ResultEnvelope<crate::domain::GetSymbolsResponse>> {
    if requests.is_empty() {
        return Ok(ResultEnvelope {
            outcome: RetrievalOutcome::Empty,
            trust: TrustLevel::Verified,
            provenance: None,
            data: Some(crate::domain::GetSymbolsResponse { results: vec![] }),
            next_action: None,
        });
    }

    check_request_gate(repo_id, persistence, run_manager)?;

    get_symbols_ungated(repo_id, requests, persistence, blob_store)
}

fn get_symbols_ungated(
    repo_id: &str,
    requests: &[BatchRetrievalRequest],
    persistence: &dyn RegistryQuery,
    blob_store: &dyn BlobStore,
) -> Result<ResultEnvelope<crate::domain::GetSymbolsResponse>> {
    let latest_run = match persistence.get_latest_completed_run(repo_id)? {
        Some(run) => run,
        None => {
            return Ok(ResultEnvelope {
                outcome: RetrievalOutcome::NotIndexed,
                trust: TrustLevel::Verified,
                provenance: None,
                data: None,
                next_action: None,
            });
        }
    };

    let provenance = run_provenance(repo_id, &latest_run.run_id, latest_run.requested_at_unix_ms);
    let file_records = persistence.get_file_records(&latest_run.run_id)?;

    let results: Vec<BatchRetrievalResultItem> = requests
        .iter()
        .map(|request| {
            let result = match request {
                BatchRetrievalRequest::Symbol {
                    relative_path,
                    symbol_name,
                    kind_filter,
                } => batch_symbol_result(
                    relative_path,
                    symbol_name,
                    *kind_filter,
                    &file_records,
                    blob_store,
                ),
                BatchRetrievalRequest::CodeSlice {
                    relative_path,
                    byte_range,
                } => batch_code_slice_result(relative_path, *byte_range, &file_records, blob_store),
            };
            BatchRetrievalResultItem::from_request(request, result)
        })
        .collect();

    Ok(ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(provenance),
        data: Some(crate::domain::GetSymbolsResponse { results }),
        next_action: None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    use crate::application::run_manager::ActiveRun;
    use crate::domain::{
        BatchRetrievalRequest, BatchRetrievalResponseData, BatchRetrievalResultItem,
        ComponentHealth, FileOutcomeStatus, FileRecord, IndexRunMode, IndexRunStatus, LanguageId,
        NextAction, PersistedFileOutcome, RepositoryStatus, RetrievalOutcome, TrustLevel,
        VerifiedCodeSliceResponse, VerifiedSourceResponse,
    };
    use crate::error::{Result as TResult, TokenizorError};
    use crate::storage::{BlobStore, RegistryPersistence, StoredBlob, digest_hex};

    use crate::domain::{SymbolKind, SymbolRecord};

    use super::super::RunManager;
    use super::{
        check_request_gate, get_file_outline, get_file_outline_ungated, get_repo_outline,
        get_repo_outline_ungated, get_symbol, get_symbols, search_symbols, search_text,
        search_text_ungated,
    };

    fn setup_gate_env() -> (TempDir, Arc<RunManager>) {
        let dir = TempDir::new().unwrap();
        let registry_path = dir.path().join("registry.json");
        let persistence = RegistryPersistence::new(registry_path);
        let manager = Arc::new(RunManager::new(persistence));
        (dir, manager)
    }

    fn register_repo(manager: &RunManager, repo_id: &str, status: RepositoryStatus) {
        use crate::domain::{Repository, RepositoryKind};
        let (quarantined_at_unix_ms, quarantine_reason) = if status == RepositoryStatus::Quarantined
        {
            (
                Some(1_709_827_200_000),
                Some("retrieval trust suspended".to_string()),
            )
        } else {
            (None, None)
        };
        let repo = Repository {
            repo_id: repo_id.to_string(),
            kind: RepositoryKind::Local,
            root_uri: "/tmp/test".to_string(),
            project_identity: "test-project".to_string(),
            project_identity_kind: Default::default(),
            default_branch: None,
            last_known_revision: None,
            status,
            invalidated_at_unix_ms: None,
            invalidation_reason: None,
            quarantined_at_unix_ms,
            quarantine_reason,
        };
        manager.persistence().save_repository(&repo).unwrap();
    }

    fn register_repo_invalidated(manager: &RunManager, repo_id: &str, reason: Option<&str>) {
        use crate::domain::{Repository, RepositoryKind, unix_timestamp_ms};
        let repo = Repository {
            repo_id: repo_id.to_string(),
            kind: RepositoryKind::Local,
            root_uri: "/tmp/test".to_string(),
            project_identity: "test-project".to_string(),
            project_identity_kind: Default::default(),
            default_branch: None,
            last_known_revision: None,
            status: RepositoryStatus::Invalidated,
            invalidated_at_unix_ms: Some(unix_timestamp_ms()),
            invalidation_reason: reason.map(|s| s.to_string()),
            quarantined_at_unix_ms: None,
            quarantine_reason: None,
        };
        manager.persistence().save_repository(&repo).unwrap();
    }

    fn create_succeeded_run(manager: &RunManager, repo_id: &str) -> String {
        let run = manager.start_run(repo_id, IndexRunMode::Full).unwrap();
        let run_id = run.run_id.clone();
        manager
            .persistence()
            .transition_to_running(&run_id, 1000)
            .unwrap();
        manager
            .persistence()
            .update_run_status(&run_id, IndexRunStatus::Succeeded, None)
            .unwrap();
        run_id
    }

    fn create_failed_run(manager: &RunManager, repo_id: &str) -> String {
        let run = manager.start_run(repo_id, IndexRunMode::Full).unwrap();
        let run_id = run.run_id.clone();
        manager
            .persistence()
            .transition_to_running(&run_id, 1000)
            .unwrap();
        manager
            .persistence()
            .update_run_status(
                &run_id,
                IndexRunStatus::Failed,
                Some("test failure".to_string()),
            )
            .unwrap();
        run_id
    }

    struct FakeBlobStore {
        blobs: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl FakeBlobStore {
        fn new() -> Self {
            Self {
                blobs: Mutex::new(HashMap::new()),
            }
        }

        fn store(&self, content: &[u8]) -> String {
            let blob_id = digest_hex(content);
            self.blobs
                .lock()
                .unwrap()
                .insert(blob_id.clone(), content.to_vec());
            blob_id
        }

        fn store_corrupted(&self, blob_id: &str, content: &[u8]) {
            self.blobs
                .lock()
                .unwrap()
                .insert(blob_id.to_string(), content.to_vec());
        }
    }

    impl BlobStore for FakeBlobStore {
        fn backend_name(&self) -> &'static str {
            "fake"
        }
        fn root_dir(&self) -> &Path {
            Path::new("/fake")
        }
        fn initialize(&self) -> TResult<ComponentHealth> {
            unreachable!("not used in search tests")
        }
        fn health_check(&self) -> TResult<ComponentHealth> {
            unreachable!("not used in search tests")
        }
        fn store_bytes(&self, _bytes: &[u8]) -> TResult<StoredBlob> {
            unreachable!("not used in search tests")
        }
        fn read_bytes(&self, blob_id: &str) -> TResult<Vec<u8>> {
            self.blobs
                .lock()
                .unwrap()
                .get(blob_id)
                .cloned()
                .ok_or_else(|| TokenizorError::NotFound(format!("blob {blob_id}")))
        }
    }

    fn make_file_record(
        relative_path: &str,
        blob_id: &str,
        byte_len: u64,
        run_id: &str,
        repo_id: &str,
        outcome: PersistedFileOutcome,
    ) -> FileRecord {
        FileRecord {
            relative_path: relative_path.to_string(),
            language: LanguageId::Rust,
            blob_id: blob_id.to_string(),
            byte_len,
            content_hash: blob_id.to_string(),
            outcome,
            symbols: vec![],
            run_id: run_id.to_string(),
            repo_id: repo_id.to_string(),
            committed_at_unix_ms: 1000,
        }
    }

    fn setup_search_env() -> (TempDir, Arc<RunManager>, Arc<FakeBlobStore>) {
        let dir = TempDir::new().unwrap();
        let registry_path = dir.path().join("registry.json");
        let persistence = RegistryPersistence::new(registry_path);
        let manager = Arc::new(RunManager::new(persistence));
        let blob_store = Arc::new(FakeBlobStore::new());
        (dir, manager, blob_store)
    }

    fn setup_indexed_repo(
        manager: &RunManager,
        blob_store: &FakeBlobStore,
        repo_id: &str,
        files: &[(&str, &str)],
    ) -> String {
        register_repo(manager, repo_id, RepositoryStatus::Ready);
        let run_id = create_succeeded_run(manager, repo_id);

        let records: Vec<FileRecord> = files
            .iter()
            .map(|(path, content)| {
                let blob_id = blob_store.store(content.as_bytes());
                make_file_record(
                    path,
                    &blob_id,
                    content.len() as u64,
                    &run_id,
                    repo_id,
                    PersistedFileOutcome::Committed,
                )
            })
            .collect();

        manager
            .persistence()
            .save_file_records(&run_id, &records)
            .unwrap();
        run_id
    }

    #[test]
    fn test_gate_rejects_unknown_repo() {
        let (_dir, manager) = setup_gate_env();
        let result = check_request_gate("nonexistent", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("not found in registry"));
    }

    #[test]
    fn test_gate_rejects_invalidated_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo_invalidated(&manager, "repo-inv", Some("trust revoked"));
        create_succeeded_run(&manager, "repo-inv");

        let result = check_request_gate("repo-inv", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("invalidated"));
        assert!(err.to_string().contains("trust revoked"));
    }

    #[test]
    fn test_gate_rejects_failed_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "repo-fail", RepositoryStatus::Failed);
        create_succeeded_run(&manager, "repo-fail");

        let result = check_request_gate("repo-fail", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("failed state"));
    }

    #[test]
    fn test_gate_rejects_degraded_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "repo-deg", RepositoryStatus::Degraded);
        create_succeeded_run(&manager, "repo-deg");

        let result = check_request_gate("repo-deg", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("degraded state"));
    }

    #[test]
    fn test_gate_rejects_active_mutation() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "repo-mut", RepositoryStatus::Ready);
        create_succeeded_run(&manager, "repo-mut");

        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});
        manager.register_active_run(
            "repo-mut",
            ActiveRun {
                run_id: "active-run-123".to_string(),
                handle,
                cancellation_token: CancellationToken::new(),
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        let result = check_request_gate("repo-mut", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("active mutation"));
        assert!(err.to_string().contains("active-run-123"));
    }

    #[test]
    fn test_gate_rejects_persisted_queued_run_without_active_handle() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "repo-queued", RepositoryStatus::Ready);
        create_succeeded_run(&manager, "repo-queued");
        let queued_run = manager
            .start_run("repo-queued", IndexRunMode::Full)
            .expect("queued run should be created");

        let result = check_request_gate("repo-queued", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "request gated: active mutation in progress (run: {}) [next_action: wait]",
                queued_run.run_id
            )
        );
    }

    #[test]
    fn test_gate_rejects_persisted_running_run_without_active_handle() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "repo-running", RepositoryStatus::Ready);
        create_succeeded_run(&manager, "repo-running");
        let running_run = manager
            .start_run("repo-running", IndexRunMode::Full)
            .expect("running run should be created");
        manager
            .persistence()
            .transition_to_running(&running_run.run_id, 1_709_827_200_000)
            .expect("run should transition to running");

        let result = check_request_gate("repo-running", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "request gated: active mutation in progress (run: {}) [next_action: wait]",
                running_run.run_id
            )
        );
    }

    #[test]
    fn test_gate_rejects_never_indexed_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "repo-new", RepositoryStatus::Pending);

        let result = check_request_gate("repo-new", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("has not been indexed"));
    }

    #[test]
    fn test_gate_rejects_no_successful_runs() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "repo-nok", RepositoryStatus::Ready);
        create_failed_run(&manager, "repo-nok");

        let result = check_request_gate("repo-nok", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("no successful index"));
        assert!(err.to_string().contains("Failed"));
    }

    #[test]
    fn test_gate_passes_healthy_repo_with_completed_run() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "repo-ok", RepositoryStatus::Ready);
        create_succeeded_run(&manager, "repo-ok");

        let result = check_request_gate("repo-ok", manager.persistence(), &manager);
        assert!(result.is_ok());
    }

    // --- search_text tests ---

    #[test]
    fn test_search_text_returns_matching_results() {
        let (_dir, manager, blob_store) = setup_search_env();
        let content = "fn main() {\n    println!(\"hello world\");\n}\n";
        setup_indexed_repo(
            &manager,
            &blob_store,
            "repo-s1",
            &[("src/main.rs", content)],
        );

        let result = search_text(
            "repo-s1",
            "println",
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert!(result.provenance.is_some());
        let data = result.data.unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].relative_path, "src/main.rs");
        assert_eq!(data[0].line_number, 2);
        assert!(data[0].line_content.contains("println"));
        assert_eq!(data[0].match_length, 7); // "println"
        assert!(!data[0].provenance.run_id.is_empty());
        assert!(data[0].provenance.committed_at_unix_ms > 0);
    }

    #[test]
    fn test_search_text_returns_empty_for_no_matches() {
        let (_dir, manager, blob_store) = setup_search_env();
        setup_indexed_repo(
            &manager,
            &blob_store,
            "repo-s2",
            &[("src/lib.rs", "fn add(a: i32, b: i32) -> i32 { a + b }")],
        );

        let result = search_text(
            "repo-s2",
            "nonexistent_xyz",
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Empty);
        assert!(result.data.is_none());
        assert!(result.provenance.is_some());
    }

    #[test]
    fn test_search_text_rejects_invalidated_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo_invalidated(&manager, "repo-s3", Some("trust revoked"));
        create_succeeded_run(&manager, "repo-s3");

        let result = search_text(
            "repo-s3",
            "test",
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_search_text_rejects_degraded_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "repo-s4", RepositoryStatus::Degraded);
        let run_id = create_succeeded_run(&manager, "repo-s4");
        let content = "let x = 42;\n";
        let blob_id = blob_store.store(content.as_bytes());
        let record = make_file_record(
            "src/lib.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            "repo-s4",
            PersistedFileOutcome::Committed,
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        let result = search_text(
            "repo-s4",
            "42",
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("degraded state"));
    }

    #[test]
    fn test_search_text_rejects_failed_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "repo-s5", RepositoryStatus::Failed);
        create_succeeded_run(&manager, "repo-s5");

        let result = search_text(
            "repo-s5",
            "test",
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_search_text_rejects_active_mutation() {
        let (_dir, manager, blob_store) = setup_search_env();
        setup_indexed_repo(
            &manager,
            &blob_store,
            "repo-s6",
            &[("src/lib.rs", "test content")],
        );

        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});
        manager.register_active_run(
            "repo-s6",
            ActiveRun {
                run_id: "active-run".to_string(),
                handle,
                cancellation_token: CancellationToken::new(),
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        let result = search_text(
            "repo-s6",
            "test",
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_search_text_rejects_never_indexed_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "repo-s7", RepositoryStatus::Pending);

        let result = search_text(
            "repo-s7",
            "test",
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("has not been indexed"));
    }

    #[test]
    fn test_search_text_rejects_no_successful_runs() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "repo-s8", RepositoryStatus::Ready);
        create_failed_run(&manager, "repo-s8");

        let result = search_text(
            "repo-s8",
            "test",
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("no successful index"));
    }

    #[test]
    fn test_search_text_excludes_quarantined_files() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "repo-s9", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "repo-s9");

        let good_content = "fn good_function() {}";
        let quarantined_content = "fn good_function() {} // also in quarantined";
        let good_blob_id = blob_store.store(good_content.as_bytes());
        let q_blob_id = blob_store.store(quarantined_content.as_bytes());

        let records = vec![
            make_file_record(
                "src/good.rs",
                &good_blob_id,
                good_content.len() as u64,
                &run_id,
                "repo-s9",
                PersistedFileOutcome::Committed,
            ),
            make_file_record(
                "src/quarantined.rs",
                &q_blob_id,
                quarantined_content.len() as u64,
                &run_id,
                "repo-s9",
                PersistedFileOutcome::Quarantined {
                    reason: "suspicious".to_string(),
                },
            ),
        ];
        manager
            .persistence()
            .save_file_records(&run_id, &records)
            .unwrap();

        let result = search_text(
            "repo-s9",
            "good_function",
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].relative_path, "src/good.rs");
    }

    #[test]
    fn test_search_text_includes_provenance_metadata() {
        let (_dir, manager, blob_store) = setup_search_env();
        let run_id = setup_indexed_repo(
            &manager,
            &blob_store,
            "repo-s10",
            &[("src/main.rs", "fn main() {}")],
        );

        let result = search_text(
            "repo-s10",
            "main",
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let prov = result.provenance.unwrap();
        assert_eq!(prov.run_id, run_id);
        assert_eq!(prov.repo_id, "repo-s10");

        let data = result.data.unwrap();
        assert_eq!(data[0].provenance.run_id, run_id);
        assert_eq!(data[0].provenance.repo_id, "repo-s10");
        assert!(data[0].provenance.committed_at_unix_ms > 0);
    }

    #[test]
    fn test_search_text_skips_file_with_corrupted_blob() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "repo-s11", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "repo-s11");

        let good_content = "fn good() { let x = 1; }";
        let good_blob_id = blob_store.store(good_content.as_bytes());

        // Store corrupted blob: blob_id of "real content" but bytes of something else
        let real_content = "fn corrupted() { let x = 1; }";
        let real_blob_id = digest_hex(real_content.as_bytes());
        blob_store.store_corrupted(&real_blob_id, b"totally different content");

        let records = vec![
            make_file_record(
                "src/good.rs",
                &good_blob_id,
                good_content.len() as u64,
                &run_id,
                "repo-s11",
                PersistedFileOutcome::Committed,
            ),
            make_file_record(
                "src/corrupted.rs",
                &real_blob_id,
                real_content.len() as u64,
                &run_id,
                "repo-s11",
                PersistedFileOutcome::Committed,
            ),
        ];
        manager
            .persistence()
            .save_file_records(&run_id, &records)
            .unwrap();

        let result = search_text(
            "repo-s11",
            "let x",
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        // Only the good file should appear; corrupted file skipped
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].relative_path, "src/good.rs");
    }

    #[test]
    fn test_search_text_scopes_to_repo_context() {
        let (_dir, manager, blob_store) = setup_search_env();
        setup_indexed_repo(
            &manager,
            &blob_store,
            "repo-a",
            &[("src/lib.rs", "fn shared_token() {}")],
        );
        setup_indexed_repo(
            &manager,
            &blob_store,
            "repo-b",
            &[("src/lib.rs", "fn shared_token() {}")],
        );

        let result_a = search_text(
            "repo-a",
            "shared_token",
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();
        let result_b = search_text(
            "repo-b",
            "shared_token",
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        let data_a = result_a.data.unwrap();
        let data_b = result_b.data.unwrap();

        // Both repos have results, but provenance is repo-scoped
        assert_eq!(data_a[0].provenance.repo_id, "repo-a");
        assert_eq!(data_b[0].provenance.repo_id, "repo-b");
        assert_ne!(data_a[0].provenance.run_id, data_b[0].provenance.run_id);
    }

    #[test]
    fn test_search_text_latency_within_bounds() {
        let (_dir, manager, blob_store) = setup_search_env();

        // Create a repo with ~100 files of moderate size
        let mut files: Vec<(&str, String)> = Vec::new();
        let paths: Vec<String> = (0..100).map(|i| format!("src/file_{i}.rs")).collect();
        for path in &paths {
            let content = format!(
                "fn function_{p}() {{\n    let x = 42;\n    println!(\"hello\");\n}}\n",
                p = path.replace('/', "_")
            );
            files.push((path.as_str(), content));
        }

        let file_refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
        setup_indexed_repo(&manager, &blob_store, "repo-perf", &file_refs);

        let start = std::time::Instant::now();
        let result = search_text(
            "repo-perf",
            "println",
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.len(), 100);
        // Sanity check: should complete well under 500ms for 100 small files
        assert!(
            elapsed.as_millis() < 500,
            "search took {}ms, expected <500ms",
            elapsed.as_millis()
        );
    }

    // --- Review-fix tests ---

    #[test]
    fn test_search_text_ungated_returns_not_indexed() {
        // Exercises the defense-in-depth NotIndexed branch that the gate normally catches.
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "repo-ungated", RepositoryStatus::Pending);
        // No runs exist — gate would reject, but we bypass it via search_text_ungated.

        let result =
            search_text_ungated("repo-ungated", "test", manager.persistence(), &*blob_store)
                .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::NotIndexed);
        assert!(result.data.is_none());
        assert!(result.provenance.is_none());
    }

    #[test]
    fn test_search_text_rejects_empty_query() {
        let (_dir, manager, blob_store) = setup_search_env();
        setup_indexed_repo(
            &manager,
            &blob_store,
            "repo-empty-q",
            &[("src/lib.rs", "fn foo() {}")],
        );

        let result = search_text(
            "repo-empty-q",
            "",
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::InvalidArgument(_)
        ));
    }

    #[test]
    fn test_search_text_finds_multiple_matches_per_line() {
        let (_dir, manager, blob_store) = setup_search_env();
        let content = "aaa bbb aaa ccc aaa\n";
        setup_indexed_repo(
            &manager,
            &blob_store,
            "repo-multi",
            &[("src/lib.rs", content)],
        );

        let result = search_text(
            "repo-multi",
            "aaa",
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(
            data.len(),
            3,
            "expected 3 matches of 'aaa' on the same line"
        );
        assert_eq!(data[0].match_offset, 0);
        assert_eq!(data[1].match_offset, 8);
        assert_eq!(data[2].match_offset, 16);
        // All on the same line
        for item in &data {
            assert_eq!(item.line_number, 1);
            assert_eq!(item.match_length, 3);
        }
    }

    // --- Symbol search helpers ---

    fn make_symbol(name: &str, kind: SymbolKind, depth: u32) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth,
            sort_order: 0,
            byte_range: (0, 100),
            line_range: (0, 5),
        }
    }

    fn make_file_record_with_symbols(
        relative_path: &str,
        run_id: &str,
        repo_id: &str,
        outcome: PersistedFileOutcome,
        symbols: Vec<SymbolRecord>,
    ) -> FileRecord {
        FileRecord {
            relative_path: relative_path.to_string(),
            language: LanguageId::Rust,
            blob_id: "fake-blob".to_string(),
            byte_len: 100,
            content_hash: "fake-hash".to_string(),
            outcome,
            symbols,
            run_id: run_id.to_string(),
            repo_id: repo_id.to_string(),
            committed_at_unix_ms: 1000,
        }
    }

    fn make_file_record_with_language(
        relative_path: &str,
        language: LanguageId,
        run_id: &str,
        repo_id: &str,
        outcome: PersistedFileOutcome,
        symbols: Vec<SymbolRecord>,
    ) -> FileRecord {
        FileRecord {
            relative_path: relative_path.to_string(),
            language,
            blob_id: "fake-blob".to_string(),
            byte_len: 100,
            content_hash: "fake-hash".to_string(),
            outcome,
            symbols,
            run_id: run_id.to_string(),
            repo_id: repo_id.to_string(),
            committed_at_unix_ms: 1000,
        }
    }

    fn setup_symbol_env() -> (TempDir, Arc<RunManager>) {
        setup_gate_env()
    }

    fn setup_indexed_symbol_repo(
        manager: &RunManager,
        repo_id: &str,
        files: Vec<FileRecord>,
    ) -> String {
        register_repo(manager, repo_id, RepositoryStatus::Ready);
        let run_id = create_succeeded_run(manager, repo_id);

        // Update file records with the correct run_id
        let records: Vec<FileRecord> = files
            .into_iter()
            .map(|mut r| {
                r.run_id = run_id.clone();
                r.repo_id = repo_id.to_string();
                r
            })
            .collect();

        manager
            .persistence()
            .save_file_records(&run_id, &records)
            .unwrap();
        run_id
    }

    // --- Symbol search tests ---

    #[test]
    fn test_search_symbols_returns_matching_results() {
        let (_dir, manager) = setup_symbol_env();
        let symbols = vec![
            make_symbol("HashMap", SymbolKind::Struct, 0),
            make_symbol("main", SymbolKind::Function, 0),
        ];
        let file = make_file_record_with_symbols(
            "src/main.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            symbols,
        );
        setup_indexed_symbol_repo(&manager, "sym-repo-1", vec![file]);

        let result = search_symbols(
            "sym-repo-1",
            "HashMap",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.matches.len(), 1);
        assert_eq!(data.matches[0].symbol_name, "HashMap");
        assert_eq!(data.matches[0].symbol_kind, SymbolKind::Struct);
        assert_eq!(data.matches[0].relative_path, "src/main.rs");
        assert!(result.provenance.is_some());
    }

    #[test]
    fn test_search_symbols_returns_empty_for_no_matches() {
        let (_dir, manager) = setup_symbol_env();
        let symbols = vec![make_symbol("main", SymbolKind::Function, 0)];
        let file = make_file_record_with_symbols(
            "src/main.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            symbols,
        );
        setup_indexed_symbol_repo(&manager, "sym-repo-2", vec![file]);

        let result = search_symbols(
            "sym-repo-2",
            "nonexistent_xyz",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Empty);
        // Deliberate: data is Some with coverage even on Empty
        let data = result.data.unwrap();
        assert!(data.matches.is_empty());
        assert_eq!(data.coverage.files_searched, 1);
        assert!(result.provenance.is_some());
    }

    #[test]
    fn test_search_symbols_case_insensitive() {
        let (_dir, manager) = setup_symbol_env();
        let symbols = vec![make_symbol("HashMap", SymbolKind::Struct, 0)];
        let file = make_file_record_with_symbols(
            "src/lib.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            symbols,
        );
        setup_indexed_symbol_repo(&manager, "sym-repo-3", vec![file]);

        let result = search_symbols(
            "sym-repo-3",
            "hashmap",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.matches.len(), 1);
        assert_eq!(data.matches[0].symbol_name, "HashMap");
    }

    #[test]
    fn test_search_symbols_filters_by_kind() {
        let (_dir, manager) = setup_symbol_env();
        let symbols = vec![
            make_symbol("HashMap", SymbolKind::Struct, 0),
            make_symbol("hash_map", SymbolKind::Function, 0),
        ];
        let file = make_file_record_with_symbols(
            "src/lib.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            symbols,
        );
        setup_indexed_symbol_repo(&manager, "sym-repo-4", vec![file]);

        let result = search_symbols(
            "sym-repo-4",
            "hash",
            Some(SymbolKind::Struct),
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.matches.len(), 1);
        assert_eq!(data.matches[0].symbol_name, "HashMap");
        assert_eq!(data.matches[0].symbol_kind, SymbolKind::Struct);
    }

    #[test]
    fn test_search_symbols_rejects_invalidated_repo() {
        let (_dir, manager) = setup_symbol_env();
        register_repo_invalidated(&manager, "sym-repo-5", Some("trust revoked"));
        create_succeeded_run(&manager, "sym-repo-5");

        let result = search_symbols("sym-repo-5", "test", None, manager.persistence(), &manager);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_search_symbols_rejects_failed_repo() {
        let (_dir, manager) = setup_symbol_env();
        register_repo(&manager, "sym-repo-6", RepositoryStatus::Failed);
        create_succeeded_run(&manager, "sym-repo-6");

        let result = search_symbols("sym-repo-6", "test", None, manager.persistence(), &manager);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_search_symbols_rejects_active_mutation() {
        let (_dir, manager) = setup_symbol_env();
        register_repo(&manager, "sym-repo-7", RepositoryStatus::Ready);
        create_succeeded_run(&manager, "sym-repo-7");

        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});
        manager.register_active_run(
            "sym-repo-7",
            ActiveRun {
                run_id: "active-sym-run".to_string(),
                handle,
                cancellation_token: CancellationToken::new(),
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        let result = search_symbols("sym-repo-7", "test", None, manager.persistence(), &manager);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("active mutation"));
    }

    #[test]
    fn test_search_symbols_rejects_never_indexed_repo() {
        let (_dir, manager) = setup_symbol_env();
        register_repo(&manager, "sym-repo-8", RepositoryStatus::Pending);

        let result = search_symbols("sym-repo-8", "test", None, manager.persistence(), &manager);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("has not been indexed"));
    }

    #[test]
    fn test_search_symbols_rejects_no_successful_runs() {
        let (_dir, manager) = setup_symbol_env();
        register_repo(&manager, "sym-repo-9", RepositoryStatus::Ready);
        create_failed_run(&manager, "sym-repo-9");

        let result = search_symbols("sym-repo-9", "test", None, manager.persistence(), &manager);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("no successful index"));
    }

    #[test]
    fn test_search_symbols_excludes_quarantined_files() {
        let (_dir, manager) = setup_symbol_env();
        let good_symbols = vec![make_symbol("good_fn", SymbolKind::Function, 0)];
        let quarantined_symbols = vec![make_symbol("good_fn", SymbolKind::Function, 0)];
        let files = vec![
            make_file_record_with_symbols(
                "src/good.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                good_symbols,
            ),
            make_file_record_with_symbols(
                "src/quarantined.rs",
                "",
                "",
                PersistedFileOutcome::Quarantined {
                    reason: "suspicious".to_string(),
                },
                quarantined_symbols,
            ),
        ];
        setup_indexed_symbol_repo(&manager, "sym-repo-10", files);

        let result = search_symbols(
            "sym-repo-10",
            "good_fn",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.matches.len(), 1);
        assert_eq!(data.matches[0].relative_path, "src/good.rs");
        assert_eq!(data.coverage.files_skipped_quarantined, 1);
    }

    #[test]
    fn test_search_symbols_includes_provenance_metadata() {
        let (_dir, manager) = setup_symbol_env();
        let symbols = vec![make_symbol("main", SymbolKind::Function, 0)];
        let file = make_file_record_with_symbols(
            "src/main.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            symbols,
        );
        let run_id = setup_indexed_symbol_repo(&manager, "sym-repo-11", vec![file]);

        let result =
            search_symbols("sym-repo-11", "main", None, manager.persistence(), &manager).unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let prov = result.provenance.unwrap();
        assert_eq!(prov.run_id, run_id);
        assert_eq!(prov.repo_id, "sym-repo-11");

        let data = result.data.unwrap();
        assert_eq!(data.matches[0].provenance.run_id, run_id);
        assert_eq!(data.matches[0].provenance.repo_id, "sym-repo-11");
        assert!(data.matches[0].provenance.committed_at_unix_ms > 0);
    }

    #[test]
    fn test_search_symbols_rejects_degraded_repo() {
        let (_dir, manager) = setup_symbol_env();
        register_repo(&manager, "sym-repo-12", RepositoryStatus::Degraded);
        let run_id = create_succeeded_run(&manager, "sym-repo-12");
        let symbols = vec![make_symbol("test_fn", SymbolKind::Function, 0)];
        let mut file = make_file_record_with_symbols(
            "src/lib.rs",
            &run_id,
            "sym-repo-12",
            PersistedFileOutcome::Committed,
            symbols,
        );
        file.run_id = run_id.clone();
        file.repo_id = "sym-repo-12".to_string();
        manager
            .persistence()
            .save_file_records(&run_id, &[file])
            .unwrap();

        let result = search_symbols(
            "sym-repo-12",
            "test_fn",
            None,
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("degraded state"));
    }

    #[test]
    fn test_search_symbols_rejects_empty_query() {
        let (_dir, manager) = setup_symbol_env();
        let symbols = vec![make_symbol("main", SymbolKind::Function, 0)];
        let file = make_file_record_with_symbols(
            "src/main.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            symbols,
        );
        setup_indexed_symbol_repo(&manager, "sym-repo-13", vec![file]);

        let result = search_symbols("sym-repo-13", "", None, manager.persistence(), &manager);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::InvalidArgument(_)
        ));
    }

    #[test]
    fn test_search_symbols_scopes_to_repo_context() {
        let (_dir, manager) = setup_symbol_env();
        let sym_a = vec![make_symbol("shared_fn", SymbolKind::Function, 0)];
        let sym_b = vec![make_symbol("shared_fn", SymbolKind::Function, 0)];
        let file_a = make_file_record_with_symbols(
            "src/lib.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            sym_a,
        );
        let file_b = make_file_record_with_symbols(
            "src/lib.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            sym_b,
        );
        setup_indexed_symbol_repo(&manager, "sym-repo-a", vec![file_a]);
        setup_indexed_symbol_repo(&manager, "sym-repo-b", vec![file_b]);

        let result_a = search_symbols(
            "sym-repo-a",
            "shared_fn",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();
        let result_b = search_symbols(
            "sym-repo-b",
            "shared_fn",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();

        let data_a = result_a.data.unwrap();
        let data_b = result_b.data.unwrap();
        assert_eq!(data_a.matches[0].provenance.repo_id, "sym-repo-a");
        assert_eq!(data_b.matches[0].provenance.repo_id, "sym-repo-b");
        assert_ne!(
            data_a.matches[0].provenance.run_id,
            data_b.matches[0].provenance.run_id
        );
    }

    #[test]
    fn test_search_symbols_coverage_reports_files_without_symbols() {
        let (_dir, manager) = setup_symbol_env();
        let files = vec![
            make_file_record_with_symbols(
                "src/with_symbols.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![make_symbol("some_fn", SymbolKind::Function, 0)],
            ),
            make_file_record_with_symbols(
                "src/no_symbols.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![], // No symbols
            ),
            make_file_record_with_symbols(
                "src/also_no_symbols.txt",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![], // No symbols
            ),
        ];
        setup_indexed_symbol_repo(&manager, "sym-repo-cov", files);

        let result = search_symbols(
            "sym-repo-cov",
            "some_fn",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();

        let data = result.data.unwrap();
        assert_eq!(data.coverage.files_searched, 1);
        assert_eq!(data.coverage.files_without_symbols, 2);
        assert_eq!(data.coverage.files_skipped_quarantined, 0);
    }

    #[test]
    fn test_search_symbols_skips_failed_files_and_counts_coverage() {
        let (_dir, manager) = setup_symbol_env();
        let files = vec![
            make_file_record_with_symbols(
                "src/good.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![make_symbol("shared_fn", SymbolKind::Function, 0)],
            ),
            make_file_record_with_symbols(
                "src/failed.rs",
                "",
                "",
                PersistedFileOutcome::Failed {
                    error: "parse failed".to_string(),
                },
                vec![make_symbol("shared_fn", SymbolKind::Function, 0)],
            ),
        ];
        setup_indexed_symbol_repo(&manager, "sym-repo-failed", files);

        let result = search_symbols(
            "sym-repo-failed",
            "shared_fn",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.matches.len(), 1);
        assert_eq!(data.matches[0].relative_path, "src/good.rs");
        assert_eq!(data.coverage.files_searched, 1);
        assert_eq!(data.coverage.files_without_symbols, 1);
    }

    #[test]
    fn test_search_symbols_skips_files_with_empty_symbols() {
        let (_dir, manager) = setup_symbol_env();
        let files = vec![make_file_record_with_symbols(
            "src/empty_symbols.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            vec![], // EmptySymbols - language unsupported or no symbols extracted
        )];
        setup_indexed_symbol_repo(&manager, "sym-repo-empty", files);

        let result = search_symbols(
            "sym-repo-empty",
            "anything",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Empty);
        let data = result.data.unwrap();
        assert_eq!(data.coverage.files_searched, 0);
        assert_eq!(data.coverage.files_without_symbols, 1);
    }

    #[test]
    fn test_search_symbols_latency_within_bounds() {
        let (_dir, manager) = setup_symbol_env();

        // Create 100 files with 10 symbols each
        let mut files = Vec::new();
        for i in 0..100 {
            let symbols: Vec<SymbolRecord> = (0..10)
                .map(|j| SymbolRecord {
                    name: format!("symbol_{i}_{j}"),
                    kind: SymbolKind::Function,
                    depth: 0,
                    sort_order: j,
                    byte_range: (j * 100, (j + 1) * 100),
                    line_range: (j * 5, (j + 1) * 5),
                })
                .collect();
            files.push(make_file_record_with_symbols(
                &format!("src/file_{i}.rs"),
                "",
                "",
                PersistedFileOutcome::Committed,
                symbols,
            ));
        }
        setup_indexed_symbol_repo(&manager, "sym-repo-perf", files);

        let start = std::time::Instant::now();
        let result = search_symbols(
            "sym-repo-perf",
            "symbol_50",
            None,
            manager.persistence(),
            &manager,
        )
        .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.matches.len(), 10); // 10 symbols in file_50 match "symbol_50"
        // Symbol search should be well under 100ms for 1000 symbols (no CAS I/O)
        assert!(
            elapsed.as_millis() < 100,
            "symbol search took {}ms, expected <100ms",
            elapsed.as_millis()
        );
    }

    // --- File outline tests ---

    #[test]
    fn test_get_file_outline_returns_symbols() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_symbols(
            "src/main.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            vec![
                make_symbol("main", SymbolKind::Function, 0),
                make_symbol("Helper", SymbolKind::Struct, 0),
            ],
        );
        setup_indexed_symbol_repo(&manager, "outline-file-1", vec![file]);

        let result = get_file_outline(
            "outline-file-1",
            "src/main.rs",
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert_eq!(result.trust, TrustLevel::Verified);
        let data = result.data.unwrap();
        assert_eq!(data.relative_path, "src/main.rs");
        assert_eq!(data.language, LanguageId::Rust);
        assert_eq!(data.symbols.len(), 2);
        assert_eq!(data.symbols[0].name, "main");
        assert_eq!(data.symbols[0].kind, SymbolKind::Function);
        assert!(data.has_symbol_support);
    }

    #[test]
    fn test_get_file_outline_returns_empty_symbols_for_supported_language() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_language(
            "src/empty.rs",
            LanguageId::Rust,
            "",
            "",
            PersistedFileOutcome::EmptySymbols,
            vec![],
        );
        setup_indexed_symbol_repo(&manager, "outline-file-2", vec![file]);

        let result = get_file_outline(
            "outline-file-2",
            "src/empty.rs",
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert!(data.symbols.is_empty());
        assert!(data.has_symbol_support);
    }

    #[test]
    fn test_get_file_outline_returns_empty_symbols_for_unsupported_language() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_language(
            "src/native.c",
            LanguageId::C,
            "",
            "",
            PersistedFileOutcome::Committed,
            vec![],
        );
        setup_indexed_symbol_repo(&manager, "outline-file-3", vec![file]);

        let result = get_file_outline(
            "outline-file-3",
            "src/native.c",
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert!(data.symbols.is_empty());
        assert!(!data.has_symbol_support);
    }

    #[test]
    fn test_get_file_outline_error_for_missing_file() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_symbols(
            "src/lib.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            vec![make_symbol("present", SymbolKind::Function, 0)],
        );
        setup_indexed_symbol_repo(&manager, "outline-file-4", vec![file]);

        let result = get_file_outline(
            "outline-file-4",
            "src/missing.rs",
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::InvalidArgument(_)));
        assert!(
            err.to_string()
                .contains("file not found in index: src/missing.rs")
        );
    }

    #[test]
    fn test_get_file_outline_returns_quarantined_for_quarantined_file() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_symbols(
            "src/quarantined.rs",
            "",
            "",
            PersistedFileOutcome::Quarantined {
                reason: "suspect parse".to_string(),
            },
            vec![make_symbol("hidden", SymbolKind::Function, 0)],
        );
        setup_indexed_symbol_repo(&manager, "outline-file-5", vec![file]);

        let result = get_file_outline(
            "outline-file-5",
            "src/quarantined.rs",
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Quarantined);
        assert_eq!(result.trust, TrustLevel::Quarantined);
        assert!(result.data.is_none());
        assert!(result.provenance.is_some());
    }

    #[test]
    fn test_get_file_outline_rejects_invalidated_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo_invalidated(&manager, "outline-file-6", Some("trust revoked"));
        create_succeeded_run(&manager, "outline-file-6");

        let result = get_file_outline(
            "outline-file-6",
            "src/main.rs",
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_get_file_outline_rejects_failed_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-file-7", RepositoryStatus::Failed);
        create_succeeded_run(&manager, "outline-file-7");

        let result = get_file_outline(
            "outline-file-7",
            "src/main.rs",
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_get_file_outline_rejects_active_mutation() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_symbols(
            "src/main.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            vec![make_symbol("main", SymbolKind::Function, 0)],
        );
        setup_indexed_symbol_repo(&manager, "outline-file-8", vec![file]);

        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});
        manager.register_active_run(
            "outline-file-8",
            ActiveRun {
                run_id: "outline-active-run".to_string(),
                handle,
                cancellation_token: CancellationToken::new(),
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        let result = get_file_outline(
            "outline-file-8",
            "src/main.rs",
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("active mutation"));
    }

    #[test]
    fn test_get_file_outline_rejects_never_indexed_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-file-9", RepositoryStatus::Pending);

        let result = get_file_outline(
            "outline-file-9",
            "src/main.rs",
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("has not been indexed"));
    }

    #[test]
    fn test_get_file_outline_rejects_no_successful_runs() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-file-10", RepositoryStatus::Ready);
        create_failed_run(&manager, "outline-file-10");

        let result = get_file_outline(
            "outline-file-10",
            "src/main.rs",
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("no successful index"));
    }

    #[test]
    fn test_get_file_outline_includes_provenance_metadata() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_symbols(
            "src/main.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            vec![make_symbol("main", SymbolKind::Function, 0)],
        );
        let run_id = setup_indexed_symbol_repo(&manager, "outline-file-11", vec![file]);

        let result = get_file_outline(
            "outline-file-11",
            "src/main.rs",
            manager.persistence(),
            &manager,
        )
        .unwrap();

        let provenance = result.provenance.unwrap();
        assert_eq!(provenance.run_id, run_id);
        assert_eq!(provenance.repo_id, "outline-file-11");
        assert_eq!(provenance.committed_at_unix_ms, 1000);
    }

    #[test]
    fn test_get_file_outline_rejects_degraded_repo() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_symbols(
            "src/degraded.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            vec![make_symbol("main", SymbolKind::Function, 0)],
        );
        setup_indexed_symbol_repo(&manager, "outline-file-12", vec![file]);
        manager
            .persistence()
            .update_repository_status(
                "outline-file-12",
                RepositoryStatus::Degraded,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let result = get_file_outline(
            "outline-file-12",
            "src/degraded.rs",
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("degraded state"));
    }

    #[test]
    fn test_get_file_outline_preserves_sort_order() {
        let (_dir, manager) = setup_symbol_env();
        let symbols = vec![
            SymbolRecord {
                name: "third".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 2,
                byte_range: (30, 40),
                line_range: (3, 4),
            },
            SymbolRecord {
                name: "first".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 1),
            },
            SymbolRecord {
                name: "second".to_string(),
                kind: SymbolKind::Impl,
                depth: 1,
                sort_order: 1,
                byte_range: (10, 20),
                line_range: (1, 3),
            },
        ];
        let file = make_file_record_with_symbols(
            "src/order.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            symbols,
        );
        setup_indexed_symbol_repo(&manager, "outline-file-13", vec![file]);

        let result = get_file_outline(
            "outline-file-13",
            "src/order.rs",
            manager.persistence(),
            &manager,
        )
        .unwrap();

        let data = result.data.unwrap();
        let names: Vec<String> = data.symbols.into_iter().map(|symbol| symbol.name).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn test_get_file_outline_defense_in_depth_not_indexed() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-file-14", RepositoryStatus::Ready);

        let result =
            get_file_outline_ungated("outline-file-14", "src/main.rs", manager.persistence())
                .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::NotIndexed);
        assert!(result.provenance.is_none());
        assert!(result.data.is_none());
    }

    #[test]
    fn test_get_file_outline_latency_within_bounds() {
        let (_dir, manager) = setup_symbol_env();
        let mut files = Vec::new();
        for i in 0..200 {
            files.push(make_file_record_with_symbols(
                &format!("src/file_{i}.rs"),
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![
                    SymbolRecord {
                        name: format!("symbol_{i}_0"),
                        kind: SymbolKind::Function,
                        depth: 0,
                        sort_order: 0,
                        byte_range: (0, 10),
                        line_range: (0, 1),
                    },
                    SymbolRecord {
                        name: format!("symbol_{i}_1"),
                        kind: SymbolKind::Struct,
                        depth: 0,
                        sort_order: 1,
                        byte_range: (10, 20),
                        line_range: (1, 3),
                    },
                ],
            ));
        }
        setup_indexed_symbol_repo(&manager, "outline-file-perf", files);

        let start = std::time::Instant::now();
        let result = get_file_outline(
            "outline-file-perf",
            "src/file_150.rs",
            manager.persistence(),
            &manager,
        )
        .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert!(
            elapsed.as_millis() < 120,
            "file outline took {}ms, expected <120ms",
            elapsed.as_millis()
        );
    }

    // --- Repo outline tests ---

    #[test]
    fn test_get_repo_outline_returns_file_listing() {
        let (_dir, manager) = setup_symbol_env();
        let files = vec![
            make_file_record_with_symbols(
                "src/main.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![make_symbol("main", SymbolKind::Function, 0)],
            ),
            make_file_record_with_language(
                "src/empty.rs",
                LanguageId::Rust,
                "",
                "",
                PersistedFileOutcome::EmptySymbols,
                vec![],
            ),
        ];
        setup_indexed_symbol_repo(&manager, "outline-repo-1", files);

        let result = get_repo_outline("outline-repo-1", manager.persistence(), &manager).unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.files.len(), 2);
        assert_eq!(data.coverage.total_files, 2);
    }

    #[test]
    fn test_get_repo_outline_includes_quarantined_files() {
        let (_dir, manager) = setup_symbol_env();
        let files = vec![
            make_file_record_with_symbols(
                "src/good.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![make_symbol("good", SymbolKind::Function, 0)],
            ),
            make_file_record_with_symbols(
                "src/quarantined.rs",
                "",
                "",
                PersistedFileOutcome::Quarantined {
                    reason: "suspect parse".to_string(),
                },
                vec![],
            ),
        ];
        setup_indexed_symbol_repo(&manager, "outline-repo-2", files);

        let result = get_repo_outline("outline-repo-2", manager.persistence(), &manager).unwrap();

        let data = result.data.unwrap();
        let quarantined = data
            .files
            .iter()
            .find(|entry| entry.relative_path == "src/quarantined.rs")
            .unwrap();
        assert_eq!(quarantined.status, FileOutcomeStatus::Quarantined);
        assert_eq!(data.coverage.files_quarantined, 1);
    }

    #[test]
    fn test_get_repo_outline_includes_failed_files() {
        let (_dir, manager) = setup_symbol_env();
        let files = vec![
            make_file_record_with_symbols(
                "src/good.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![make_symbol("good", SymbolKind::Function, 0)],
            ),
            make_file_record_with_symbols(
                "src/failed.rs",
                "",
                "",
                PersistedFileOutcome::Failed {
                    error: "parse failed".to_string(),
                },
                vec![],
            ),
        ];
        setup_indexed_symbol_repo(&manager, "outline-repo-3", files);

        let result = get_repo_outline("outline-repo-3", manager.persistence(), &manager).unwrap();

        let data = result.data.unwrap();
        let failed = data
            .files
            .iter()
            .find(|entry| entry.relative_path == "src/failed.rs")
            .unwrap();
        assert_eq!(failed.status, FileOutcomeStatus::Failed);
        assert_eq!(data.coverage.files_failed, 1);
    }

    #[test]
    fn test_get_repo_outline_coverage_counts_correctly() {
        let (_dir, manager) = setup_symbol_env();
        let files = vec![
            make_file_record_with_symbols(
                "src/with_symbols.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![make_symbol("present", SymbolKind::Function, 0)],
            ),
            make_file_record_with_symbols(
                "src/committed_empty.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![],
            ),
            make_file_record_with_language(
                "src/empty_symbols.rs",
                LanguageId::Rust,
                "",
                "",
                PersistedFileOutcome::EmptySymbols,
                vec![],
            ),
            make_file_record_with_symbols(
                "src/quarantined.rs",
                "",
                "",
                PersistedFileOutcome::Quarantined {
                    reason: "bad spans".to_string(),
                },
                vec![],
            ),
            make_file_record_with_symbols(
                "src/failed.rs",
                "",
                "",
                PersistedFileOutcome::Failed {
                    error: "parse failed".to_string(),
                },
                vec![],
            ),
        ];
        setup_indexed_symbol_repo(&manager, "outline-repo-4", files);

        let result = get_repo_outline("outline-repo-4", manager.persistence(), &manager).unwrap();

        let coverage = result.data.unwrap().coverage;
        assert_eq!(coverage.total_files, 5);
        assert_eq!(coverage.files_with_symbols, 1);
        assert_eq!(coverage.files_without_symbols, 2);
        assert_eq!(coverage.files_quarantined, 1);
        assert_eq!(coverage.files_failed, 1);
    }

    #[test]
    fn test_get_repo_outline_sorts_by_path() {
        let (_dir, manager) = setup_symbol_env();
        let files = vec![
            make_file_record_with_symbols(
                "src/zeta.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![],
            ),
            make_file_record_with_symbols(
                "src/alpha.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![],
            ),
            make_file_record_with_symbols(
                "src/middle.rs",
                "",
                "",
                PersistedFileOutcome::Committed,
                vec![],
            ),
        ];
        setup_indexed_symbol_repo(&manager, "outline-repo-5", files);

        let result = get_repo_outline("outline-repo-5", manager.persistence(), &manager).unwrap();

        let paths: Vec<String> = result
            .data
            .unwrap()
            .files
            .into_iter()
            .map(|entry| entry.relative_path)
            .collect();
        assert_eq!(
            paths,
            vec![
                "src/alpha.rs".to_string(),
                "src/middle.rs".to_string(),
                "src/zeta.rs".to_string()
            ]
        );
    }

    #[test]
    fn test_get_repo_outline_rejects_invalidated_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo_invalidated(&manager, "outline-repo-6", Some("trust revoked"));
        create_succeeded_run(&manager, "outline-repo-6");

        let result = get_repo_outline("outline-repo-6", manager.persistence(), &manager);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_get_repo_outline_rejects_failed_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-repo-7", RepositoryStatus::Failed);
        create_succeeded_run(&manager, "outline-repo-7");

        let result = get_repo_outline("outline-repo-7", manager.persistence(), &manager);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_get_repo_outline_rejects_active_mutation() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-repo-8", RepositoryStatus::Ready);
        create_succeeded_run(&manager, "outline-repo-8");

        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});
        manager.register_active_run(
            "outline-repo-8",
            ActiveRun {
                run_id: "outline-repo-active".to_string(),
                handle,
                cancellation_token: CancellationToken::new(),
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        let result = get_repo_outline("outline-repo-8", manager.persistence(), &manager);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("active mutation"));
    }

    #[test]
    fn test_get_repo_outline_rejects_never_indexed_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-repo-9", RepositoryStatus::Pending);

        let result = get_repo_outline("outline-repo-9", manager.persistence(), &manager);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("has not been indexed"));
    }

    #[test]
    fn test_get_repo_outline_rejects_no_successful_runs() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-repo-10", RepositoryStatus::Ready);
        create_failed_run(&manager, "outline-repo-10");

        let result = get_repo_outline("outline-repo-10", manager.persistence(), &manager);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("no successful index"));
    }

    #[test]
    fn test_get_repo_outline_empty_index() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-repo-11", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "outline-repo-11");
        manager
            .persistence()
            .save_file_records(&run_id, &[])
            .unwrap();

        let result = get_repo_outline("outline-repo-11", manager.persistence(), &manager).unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Empty);
        let data = result.data.unwrap();
        assert!(data.files.is_empty());
        assert_eq!(data.coverage.total_files, 0);
    }

    #[test]
    fn test_get_repo_outline_includes_provenance_metadata() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_symbols(
            "src/main.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            vec![make_symbol("main", SymbolKind::Function, 0)],
        );
        let run_id = setup_indexed_symbol_repo(&manager, "outline-repo-12", vec![file]);

        let result = get_repo_outline("outline-repo-12", manager.persistence(), &manager).unwrap();

        let provenance = result.provenance.unwrap();
        assert_eq!(provenance.run_id, run_id);
        assert_eq!(provenance.repo_id, "outline-repo-12");
        assert!(provenance.committed_at_unix_ms > 0);
    }

    #[test]
    fn test_get_repo_outline_rejects_degraded_repo() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_symbols(
            "src/main.rs",
            "",
            "",
            PersistedFileOutcome::Committed,
            vec![make_symbol("main", SymbolKind::Function, 0)],
        );
        setup_indexed_symbol_repo(&manager, "outline-repo-13", vec![file]);
        manager
            .persistence()
            .update_repository_status(
                "outline-repo-13",
                RepositoryStatus::Degraded,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let result = get_repo_outline("outline-repo-13", manager.persistence(), &manager);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("degraded state"));
    }

    #[test]
    fn test_get_repo_outline_defense_in_depth_not_indexed() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-repo-14", RepositoryStatus::Ready);

        let result = get_repo_outline_ungated("outline-repo-14", manager.persistence()).unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::NotIndexed);
        assert!(result.provenance.is_none());
        assert!(result.data.is_none());
    }

    #[test]
    fn test_get_repo_outline_latency_within_bounds() {
        let (_dir, manager) = setup_symbol_env();
        let mut files = Vec::new();
        for i in 0..500 {
            files.push(make_file_record_with_symbols(
                &format!("src/file_{i}.rs"),
                "",
                "",
                if i % 10 == 0 {
                    PersistedFileOutcome::EmptySymbols
                } else {
                    PersistedFileOutcome::Committed
                },
                if i % 10 == 0 {
                    vec![]
                } else {
                    vec![make_symbol(&format!("symbol_{i}"), SymbolKind::Function, 0)]
                },
            ));
        }
        setup_indexed_symbol_repo(&manager, "outline-repo-perf", files);

        let start = std::time::Instant::now();
        let result =
            get_repo_outline("outline-repo-perf", manager.persistence(), &manager).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert!(
            elapsed.as_millis() < 150,
            "repo outline took {}ms, expected <150ms",
            elapsed.as_millis()
        );
    }

    // --- get_symbol unit tests (Story 3.5) ---

    fn make_verified_file_record(
        relative_path: &str,
        blob_id: &str,
        byte_len: u64,
        run_id: &str,
        repo_id: &str,
        outcome: PersistedFileOutcome,
        symbols: Vec<SymbolRecord>,
    ) -> FileRecord {
        FileRecord {
            relative_path: relative_path.to_string(),
            language: LanguageId::Rust,
            blob_id: blob_id.to_string(),
            byte_len,
            content_hash: blob_id.to_string(),
            outcome,
            symbols,
            run_id: run_id.to_string(),
            repo_id: repo_id.to_string(),
            committed_at_unix_ms: 1000,
        }
    }

    fn setup_verified_env(
        content: &str,
        symbols: Vec<SymbolRecord>,
    ) -> (TempDir, Arc<RunManager>, Arc<FakeBlobStore>, String) {
        let (_dir, manager, blob_store) = setup_search_env();
        let repo_id = "sym-repo";
        register_repo(&manager, repo_id, RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, repo_id);

        let blob_id = blob_store.store(content.as_bytes());
        let record = make_verified_file_record(
            "src/main.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            repo_id,
            PersistedFileOutcome::Committed,
            symbols,
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        (_dir, manager, blob_store, run_id)
    }

    fn verified_symbol(
        name: &str,
        kind: SymbolKind,
        byte_range: (u32, u32),
        line_range: (u32, u32),
        sort_order: u32,
    ) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order,
            byte_range,
            line_range,
        }
    }

    #[test]
    fn test_get_symbol_returns_verified_source() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let main_range = (0u32, 35u32); // "fn main() {\n    println!(\"hello\");\n}"
        let add_range = (36u32, 74u32); // "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}"
        let symbols = vec![
            verified_symbol("main", SymbolKind::Function, main_range, (0, 2), 0),
            verified_symbol("add", SymbolKind::Function, add_range, (4, 6), 1),
        ];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert_eq!(result.trust, TrustLevel::Verified);
        assert!(result.provenance.is_some());
        let data = result.data.unwrap();
        assert_eq!(data.symbol_name, "main");
        assert_eq!(data.symbol_kind, SymbolKind::Function);
        assert_eq!(data.relative_path, "src/main.rs");
        assert_eq!(data.line_range, (0, 2));
        assert_eq!(data.byte_range, main_range);
        assert_eq!(
            data.source,
            &content[main_range.0 as usize..main_range.1 as usize]
        );
    }

    #[test]
    fn test_get_symbol_preserves_raw_source_fidelity() {
        // Source with mixed line endings and trailing whitespace — must be preserved exactly
        let content = "fn foo() {\r\n    let x = 1;  \n}\n";
        let range = (0u32, content.len() as u32);
        let symbols = vec![verified_symbol(
            "foo",
            SymbolKind::Function,
            range,
            (0, 2),
            0,
        )];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "foo",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        let data = result.data.unwrap();
        // Byte-exact: no line-ending conversion, no whitespace normalization
        assert_eq!(data.source, content);
        assert!(data.source.contains("\r\n"), "CRLF must be preserved");
        assert!(
            data.source.contains("  \n"),
            "trailing whitespace must be preserved"
        );
    }

    #[test]
    fn test_get_symbol_error_for_missing_file() {
        let content = "fn main() {}\n";
        let symbols = vec![verified_symbol(
            "main",
            SymbolKind::Function,
            (0, 13),
            (0, 0),
            0,
        )];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "nonexistent.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::InvalidArgument(_)));
        assert!(
            err.to_string()
                .contains("file not found in index: nonexistent.rs")
        );
    }

    #[test]
    fn test_get_symbol_error_for_missing_symbol() {
        let content = "fn main() {}\n";
        let symbols = vec![verified_symbol(
            "main",
            SymbolKind::Function,
            (0, 13),
            (0, 0),
            0,
        )];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "nonexistent",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::InvalidArgument(_)));
        assert!(
            err.to_string()
                .contains("symbol not found: `nonexistent` in file: src/main.rs")
        );
    }

    #[test]
    fn test_get_symbol_returns_quarantined_for_quarantined_file() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "q-sym-repo", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "q-sym-repo");

        let content = "fn main() {}\n";
        let blob_id = blob_store.store(content.as_bytes());
        let record = make_verified_file_record(
            "src/main.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            "q-sym-repo",
            PersistedFileOutcome::Quarantined {
                reason: "suspect content".to_string(),
            },
            vec![verified_symbol(
                "main",
                SymbolKind::Function,
                (0, 13),
                (0, 0),
                0,
            )],
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        let result = get_symbol(
            "q-sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Quarantined);
        assert_eq!(result.trust, TrustLevel::Quarantined);
        assert!(result.data.is_none());
        assert!(result.provenance.is_some());
    }

    #[test]
    fn test_get_symbol_blocks_on_blob_integrity_mismatch() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "int-sym-repo", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "int-sym-repo");

        let content = "fn main() {}\n";
        let blob_id = blob_store.store(content.as_bytes());
        // Replace blob content with different bytes (same blob_id key, wrong content)
        blob_store.store_corrupted(&blob_id, b"corrupted content");

        let record = make_verified_file_record(
            "src/main.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            "int-sym-repo",
            PersistedFileOutcome::Committed,
            vec![verified_symbol(
                "main",
                SymbolKind::Function,
                (0, 13),
                (0, 0),
                0,
            )],
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        let result = get_symbol(
            "int-sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(
            result.outcome,
            RetrievalOutcome::Blocked {
                reason: "blob integrity verification failed: content hash mismatch".to_string()
            }
        );
        assert_eq!(result.trust, TrustLevel::Suspect);
        assert!(result.data.is_none());
    }

    #[test]
    fn test_get_symbol_blocks_on_byte_range_out_of_bounds() {
        let content = "fn a() {}\n";
        // Symbol byte range exceeds blob size
        let symbols = vec![verified_symbol(
            "a",
            SymbolKind::Function,
            (0, 999),
            (0, 0),
            0,
        )];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "a",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.trust, TrustLevel::Suspect);
        match &result.outcome {
            RetrievalOutcome::Blocked { reason } => {
                assert!(reason.contains("byte range"));
                assert!(reason.contains("exceeds blob size"));
            }
            other => panic!("expected Blocked, got {other:?}"),
        }
        assert!(result.data.is_none());
    }

    #[test]
    fn test_get_symbol_blocks_on_malformed_byte_range() {
        let content = "fn a() {}\n";
        // start > end
        let symbols = vec![verified_symbol(
            "a",
            SymbolKind::Function,
            (5, 2),
            (0, 0),
            0,
        )];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "a",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.trust, TrustLevel::Suspect);
        match &result.outcome {
            RetrievalOutcome::Blocked { reason } => {
                assert!(reason.contains("malformed"));
            }
            other => panic!("expected Blocked, got {other:?}"),
        }
        assert!(result.data.is_none());
    }

    #[test]
    fn test_get_symbol_blocks_on_non_utf8_source() {
        // Content with non-UTF-8 bytes in symbol range
        let content: &[u8] = &[0xFF, 0xFE, 0x00, 0x01, 0x02];
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "utf8-repo", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "utf8-repo");

        let blob_id = blob_store.store(content);
        let record = make_verified_file_record(
            "src/main.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            "utf8-repo",
            PersistedFileOutcome::Committed,
            vec![verified_symbol(
                "bad",
                SymbolKind::Function,
                (0, content.len() as u32),
                (0, 0),
                0,
            )],
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        let result = get_symbol(
            "utf8-repo",
            "src/main.rs",
            "bad",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.trust, TrustLevel::Suspect);
        match &result.outcome {
            RetrievalOutcome::Blocked { reason } => {
                assert!(reason.contains("non-UTF-8"));
            }
            other => panic!("expected Blocked, got {other:?}"),
        }
        assert!(result.data.is_none());
    }

    #[test]
    fn test_get_symbol_blocks_on_blob_read_failure() {
        let content = "fn main() {}\n";
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "blob-fail-repo", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "blob-fail-repo");

        // Store content to get blob_id but DON'T store in blob_store (simulate missing blob)
        let blob_id = digest_hex(content.as_bytes());
        let record = make_verified_file_record(
            "src/main.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            "blob-fail-repo",
            PersistedFileOutcome::Committed,
            vec![verified_symbol(
                "main",
                SymbolKind::Function,
                (0, 13),
                (0, 0),
                0,
            )],
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        let result = get_symbol(
            "blob-fail-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.trust, TrustLevel::Suspect);
        match &result.outcome {
            RetrievalOutcome::Blocked { reason } => {
                assert!(reason.contains("blob read failed"));
            }
            other => panic!("expected Blocked, got {other:?}"),
        }
        assert!(result.data.is_none());
    }

    #[test]
    fn test_get_symbol_rejects_invalidated_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo_invalidated(&manager, "inv-sym-repo", Some("revoked"));
        create_succeeded_run(&manager, "inv-sym-repo");

        let result = get_symbol(
            "inv-sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_get_symbol_rejects_failed_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "fail-sym-repo", RepositoryStatus::Failed);
        create_succeeded_run(&manager, "fail-sym-repo");

        let result = get_symbol(
            "fail-sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_get_symbol_rejects_active_mutation() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "mut-sym-repo", RepositoryStatus::Ready);
        create_succeeded_run(&manager, "mut-sym-repo");

        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});
        manager.register_active_run(
            "mut-sym-repo",
            ActiveRun {
                run_id: "active-run-sym".to_string(),
                handle,
                cancellation_token: CancellationToken::new(),
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        let result = get_symbol(
            "mut-sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("active mutation"));
    }

    #[test]
    fn test_get_symbol_rejects_never_indexed_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "never-sym-repo", RepositoryStatus::Ready);
        // No runs created

        let result = get_symbol(
            "never-sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("has not been indexed"));
    }

    #[test]
    fn test_get_symbol_rejects_no_successful_runs() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "nosuc-sym-repo", RepositoryStatus::Ready);
        create_failed_run(&manager, "nosuc-sym-repo");

        let result = get_symbol(
            "nosuc-sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(err.to_string().contains("no successful index"));
    }

    #[test]
    fn test_get_symbol_includes_provenance_metadata() {
        let content = "fn main() {}\n";
        let symbols = vec![verified_symbol(
            "main",
            SymbolKind::Function,
            (0, 13),
            (0, 0),
            0,
        )];
        let (_dir, manager, blob_store, run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        let prov = result.provenance.unwrap();
        assert_eq!(prov.run_id, run_id);
        assert!(prov.committed_at_unix_ms > 0);
        assert_eq!(prov.repo_id, "sym-repo");
    }

    #[test]
    fn test_get_symbol_with_kind_filter() {
        let content = "fn Foo() {}\nstruct Foo { x: i32 }\n";
        let symbols = vec![
            verified_symbol("Foo", SymbolKind::Function, (0, 11), (0, 0), 0),
            verified_symbol("Foo", SymbolKind::Struct, (12, 33), (1, 1), 1),
        ];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        // Without filter: returns first (Function)
        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "Foo",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();
        assert_eq!(result.data.unwrap().symbol_kind, SymbolKind::Function);

        // With filter: returns Struct
        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "Foo",
            Some(SymbolKind::Struct),
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();
        assert_eq!(result.data.unwrap().symbol_kind, SymbolKind::Struct);
    }

    #[test]
    fn test_get_symbol_returns_first_by_sort_order_when_ambiguous() {
        let content = "fn a() {}\nfn a() {}\n";
        let symbols = vec![
            verified_symbol("a", SymbolKind::Function, (10, 19), (1, 1), 1),
            verified_symbol("a", SymbolKind::Function, (0, 9), (0, 0), 0),
        ];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "a",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        let data = result.data.unwrap();
        // First by sort_order (0), not by vec position
        assert_eq!(data.byte_range, (0, 9));
        assert_eq!(data.source, "fn a() {}");
    }

    #[test]
    fn test_get_symbol_rejects_degraded_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "deg-sym-repo", RepositoryStatus::Degraded);
        create_succeeded_run(&manager, "deg-sym-repo");

        // Degraded repositories are request-fatal for trusted retrieval.
        let result = get_symbol(
            "deg-sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::RequestGated { .. }
        ));
    }

    #[test]
    fn test_get_symbol_defense_in_depth_not_indexed() {
        // Test the ungated defense-in-depth branch: no completed runs after gate passes
        let (_dir, manager, blob_store) = setup_search_env();
        // Create a repo with Ready status but no runs at all — bypass gate by calling ungated
        register_repo(&manager, "did-sym-repo", RepositoryStatus::Ready);

        let result = super::get_symbol_ungated(
            "did-sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::NotIndexed);
        assert!(result.data.is_none());
    }

    #[test]
    fn test_get_symbol_rejects_empty_symbol_name() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "empty-sym-repo", RepositoryStatus::Ready);
        create_succeeded_run(&manager, "empty-sym-repo");

        let result = get_symbol(
            "empty-sym-repo",
            "src/main.rs",
            "",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::InvalidArgument(_)));
        assert!(err.to_string().contains("symbol_name must not be empty"));
    }

    #[test]
    fn test_get_symbol_rejects_empty_relative_path() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "empty-path-repo", RepositoryStatus::Ready);
        create_succeeded_run(&manager, "empty-path-repo");

        let result = get_symbol(
            "empty-path-repo",
            "",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::InvalidArgument(_)));
        assert!(err.to_string().contains("relative_path must not be empty"));
    }

    #[test]
    fn test_get_symbol_latency_within_bounds() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        let symbols = vec![verified_symbol(
            "main",
            SymbolKind::Function,
            (0, 35),
            (0, 2),
            0,
        )];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let start = std::time::Instant::now();
        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert!(
            elapsed.as_millis() < 150,
            "get_symbol took {}ms, expected <150ms",
            elapsed.as_millis()
        );
    }

    // --- Story 3.6: Quarantine gate and NextAction tests ---

    // Task 5.2: Repo-level quarantine gate tests

    #[test]
    fn test_request_gate_blocks_quarantined_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "repo-quar", RepositoryStatus::Quarantined);
        create_succeeded_run(&manager, "repo-quar");

        let result = check_request_gate("repo-quar", manager.persistence(), &manager);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            "request gated: repository quarantined: retrieval trust suspended [next_action: repair]"
        );
    }

    #[test]
    fn test_search_text_rejects_quarantined_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "repo-q-text", RepositoryStatus::Quarantined);
        create_succeeded_run(&manager, "repo-q-text");

        let result = search_text(
            "repo-q-text",
            "test",
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(
            err.to_string().contains("quarantined"),
            "expected 'quarantined' in error message, got: {}",
            err
        );
    }

    #[test]
    fn test_search_symbols_rejects_quarantined_repo() {
        let (_dir, manager) = setup_symbol_env();
        register_repo(&manager, "sym-repo-quar", RepositoryStatus::Quarantined);
        create_succeeded_run(&manager, "sym-repo-quar");

        let result = search_symbols(
            "sym-repo-quar",
            "test",
            None,
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(
            err.to_string().contains("quarantined"),
            "expected 'quarantined' in error message, got: {}",
            err
        );
    }

    #[test]
    fn test_get_file_outline_rejects_quarantined_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-quar", RepositoryStatus::Quarantined);
        create_succeeded_run(&manager, "outline-quar");

        let result = get_file_outline(
            "outline-quar",
            "src/main.rs",
            manager.persistence(),
            &manager,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(
            err.to_string().contains("quarantined"),
            "expected 'quarantined' in error message, got: {}",
            err
        );
    }

    #[test]
    fn test_get_symbol_rejects_quarantined_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "sym-quar-repo", RepositoryStatus::Quarantined);
        create_succeeded_run(&manager, "sym-quar-repo");

        let result = get_symbol(
            "sym-quar-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(
            err.to_string().contains("quarantined"),
            "expected 'quarantined' in error message, got: {}",
            err
        );
    }

    #[test]
    fn test_get_repo_outline_rejects_quarantined_repo() {
        let (_dir, manager) = setup_gate_env();
        register_repo(&manager, "outline-repo-quar", RepositoryStatus::Quarantined);
        create_succeeded_run(&manager, "outline-repo-quar");

        let result = get_repo_outline("outline-repo-quar", manager.persistence(), &manager);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::RequestGated { .. }));
        assert!(
            err.to_string().contains("quarantined"),
            "expected 'quarantined' in error message, got: {}",
            err
        );
    }

    // Task 5.3: NextAction in blocked/quarantined outcomes

    #[test]
    fn test_blocked_blob_read_includes_next_action_repair() {
        let content = "fn main() {}\n";
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "na-blob-fail", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "na-blob-fail");

        // Store content to get blob_id but DON'T put it in blob_store (simulate missing blob)
        let blob_id = digest_hex(content.as_bytes());
        let record = make_verified_file_record(
            "src/main.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            "na-blob-fail",
            PersistedFileOutcome::Committed,
            vec![verified_symbol(
                "main",
                SymbolKind::Function,
                (0, 13),
                (0, 0),
                0,
            )],
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        let result = get_symbol(
            "na-blob-fail",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.next_action, Some(NextAction::Repair));
    }

    #[test]
    fn test_blocked_blob_integrity_includes_next_action_reindex() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "na-blob-int", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "na-blob-int");

        let content = "fn main() {}\n";
        let blob_id = blob_store.store(content.as_bytes());
        // Replace blob content with different bytes (same key, wrong content)
        blob_store.store_corrupted(&blob_id, b"corrupted content");

        let record = make_verified_file_record(
            "src/main.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            "na-blob-int",
            PersistedFileOutcome::Committed,
            vec![verified_symbol(
                "main",
                SymbolKind::Function,
                (0, 13),
                (0, 0),
                0,
            )],
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        let result = get_symbol(
            "na-blob-int",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.next_action, Some(NextAction::Reindex));
    }

    #[test]
    fn test_blocked_byte_range_includes_next_action_reindex() {
        let content = "fn a() {}\n";
        // Symbol byte range exceeds blob size
        let symbols = vec![verified_symbol(
            "a",
            SymbolKind::Function,
            (0, 999),
            (0, 0),
            0,
        )];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "a",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.next_action, Some(NextAction::Reindex));
    }

    #[test]
    fn test_blocked_non_utf8_includes_next_action_repair() {
        let content: &[u8] = &[0xFF, 0xFE, 0x00, 0x01, 0x02];
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "na-utf8-repo", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "na-utf8-repo");

        let blob_id = blob_store.store(content);
        let record = make_verified_file_record(
            "src/main.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            "na-utf8-repo",
            PersistedFileOutcome::Committed,
            vec![verified_symbol(
                "bad",
                SymbolKind::Function,
                (0, content.len() as u32),
                (0, 0),
                0,
            )],
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        let result = get_symbol(
            "na-utf8-repo",
            "src/main.rs",
            "bad",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.next_action, Some(NextAction::Repair));
    }

    #[test]
    fn test_quarantined_file_includes_next_action_repair() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "na-qf-sym", RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, "na-qf-sym");

        let content = "fn main() {}\n";
        let blob_id = blob_store.store(content.as_bytes());
        let record = make_verified_file_record(
            "src/main.rs",
            &blob_id,
            content.len() as u64,
            &run_id,
            "na-qf-sym",
            PersistedFileOutcome::Quarantined {
                reason: "suspect content".to_string(),
            },
            vec![verified_symbol(
                "main",
                SymbolKind::Function,
                (0, 13),
                (0, 0),
                0,
            )],
        );
        manager
            .persistence()
            .save_file_records(&run_id, &[record])
            .unwrap();

        let result = get_symbol(
            "na-qf-sym",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Quarantined);
        assert_eq!(result.next_action, Some(NextAction::Repair));
    }

    #[test]
    fn test_quarantined_file_outline_includes_next_action_repair() {
        let (_dir, manager) = setup_symbol_env();
        let file = make_file_record_with_symbols(
            "src/quarantined.rs",
            "",
            "",
            PersistedFileOutcome::Quarantined {
                reason: "suspect parse".to_string(),
            },
            vec![make_symbol("hidden", SymbolKind::Function, 0)],
        );
        setup_indexed_symbol_repo(&manager, "na-qf-outline", vec![file]);

        let result = get_file_outline(
            "na-qf-outline",
            "src/quarantined.rs",
            manager.persistence(),
            &manager,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Quarantined);
        assert_eq!(result.next_action, Some(NextAction::Repair));
    }

    #[test]
    fn test_success_result_has_no_next_action() {
        let content = "fn main() {}\n";
        let symbols = vec![verified_symbol(
            "main",
            SymbolKind::Function,
            (0, 13),
            (0, 0),
            0,
        )];
        let (_dir, manager, blob_store, _run_id) = setup_verified_env(content, symbols);

        let result = get_symbol(
            "sym-repo",
            "src/main.rs",
            "main",
            None,
            manager.persistence(),
            &manager,
            blob_store.as_ref(),
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert_eq!(result.next_action, None);
    }

    #[test]
    fn test_gate_error_message_includes_next_action() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo_invalidated(&manager, "na-gate-msg", Some("trust revoked"));
        create_succeeded_run(&manager, "na-gate-msg");

        let result = search_text(
            "na-gate-msg",
            "test",
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            "request gated: repository invalidated: trust revoked [next_action: reindex]"
        );
    }

    // =========================================================================
    // Story 3.7: Batch retrieval (get_symbols) unit tests
    // =========================================================================

    fn setup_multi_symbol_env() -> (TempDir, Arc<RunManager>, Arc<FakeBlobStore>, String) {
        let (_dir, manager, blob_store) = setup_search_env();
        let repo_id = "batch-repo";
        register_repo(&manager, repo_id, RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, repo_id);

        let content_a = "fn alpha() {}\nfn beta() {}\n";
        let blob_a = blob_store.store(content_a.as_bytes());
        let record_a = make_verified_file_record(
            "src/a.rs",
            &blob_a,
            content_a.len() as u64,
            &run_id,
            repo_id,
            PersistedFileOutcome::Committed,
            vec![
                verified_symbol("alpha", SymbolKind::Function, (0, 14), (0, 0), 0),
                verified_symbol("beta", SymbolKind::Function, (15, 27), (1, 1), 1),
            ],
        );

        let content_b = "struct Foo {}\nimpl Foo {}\n";
        let blob_b = blob_store.store(content_b.as_bytes());
        let record_b = make_verified_file_record(
            "src/b.rs",
            &blob_b,
            content_b.len() as u64,
            &run_id,
            repo_id,
            PersistedFileOutcome::Committed,
            vec![
                verified_symbol("Foo", SymbolKind::Struct, (0, 13), (0, 0), 0),
                verified_symbol("Foo", SymbolKind::Impl, (14, 25), (1, 1), 1),
            ],
        );

        manager
            .persistence()
            .save_file_records(&run_id, &[record_a, record_b])
            .unwrap();

        (_dir, manager, blob_store, run_id)
    }

    fn symbol_request(
        relative_path: &str,
        symbol_name: &str,
        kind_filter: Option<SymbolKind>,
    ) -> BatchRetrievalRequest {
        BatchRetrievalRequest::Symbol {
            relative_path: relative_path.to_string(),
            symbol_name: symbol_name.to_string(),
            kind_filter,
        }
    }

    fn code_slice_request(relative_path: &str, byte_range: (u32, u32)) -> BatchRetrievalRequest {
        BatchRetrievalRequest::CodeSlice {
            relative_path: relative_path.to_string(),
            byte_range,
        }
    }

    fn symbol_data(item: &BatchRetrievalResultItem) -> &VerifiedSourceResponse {
        let result = match item {
            BatchRetrievalResultItem::Symbol { result, .. } => result,
            other => panic!("expected symbol batch item, got: {other:?}"),
        };
        match result.data.as_ref().unwrap() {
            BatchRetrievalResponseData::Symbol(data) => data,
            other => panic!("expected symbol response data, got: {other:?}"),
        }
    }

    fn code_slice_data(item: &BatchRetrievalResultItem) -> &VerifiedCodeSliceResponse {
        let result = match item {
            BatchRetrievalResultItem::CodeSlice { result, .. } => result,
            other => panic!("expected code-slice batch item, got: {other:?}"),
        };
        match result.data.as_ref().unwrap() {
            BatchRetrievalResponseData::CodeSlice(data) => data,
            other => panic!("expected code-slice response data, got: {other:?}"),
        }
    }

    fn batch_result(
        item: &BatchRetrievalResultItem,
    ) -> &crate::domain::ResultEnvelope<BatchRetrievalResponseData> {
        match item {
            BatchRetrievalResultItem::Symbol { result, .. } => result,
            BatchRetrievalResultItem::CodeSlice { result, .. } => result,
        }
    }

    // --- Task 4.1: Batch gate tests (AC: 3) ---

    #[test]
    fn test_get_symbols_rejects_invalidated_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo_invalidated(&manager, "inv-batch", Some("corrupted"));
        create_succeeded_run(&manager, "inv-batch");

        let requests = vec![symbol_request("src/a.rs", "alpha", None)];

        let result = get_symbols(
            "inv-batch",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            TokenizorError::RequestGated { gate_error } => {
                assert!(gate_error.contains("invalidated"));
                assert!(gate_error.contains("[next_action: reindex]"));
            }
            other => panic!("expected RequestGated, got: {other}"),
        }
    }

    #[test]
    fn test_get_symbols_rejects_quarantined_repo() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "quar-batch", RepositoryStatus::Quarantined);
        create_succeeded_run(&manager, "quar-batch");

        let requests = vec![symbol_request("src/a.rs", "alpha", None)];

        let result = get_symbols(
            "quar-batch",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            TokenizorError::RequestGated { gate_error } => {
                assert!(gate_error.contains("quarantined"));
                assert!(gate_error.contains("[next_action: repair]"));
            }
            other => panic!("expected RequestGated, got: {other}"),
        }
    }

    #[test]
    fn test_get_symbols_rejects_active_mutation() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "mut-batch", RepositoryStatus::Ready);
        // Start a run but don't complete it — leaves active mutation
        let _run = manager.start_run("mut-batch", IndexRunMode::Full).unwrap();

        let requests = vec![symbol_request("src/a.rs", "alpha", None)];

        let result = get_symbols(
            "mut-batch",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            TokenizorError::RequestGated { gate_error } => {
                assert!(gate_error.contains("active mutation"));
                assert!(gate_error.contains("[next_action: wait]"));
            }
            other => panic!("expected RequestGated, got: {other}"),
        }
    }

    #[test]
    fn test_get_symbols_rejects_never_indexed() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "never-batch", RepositoryStatus::Ready);
        // No runs created

        let requests = vec![symbol_request("src/a.rs", "alpha", None)];

        let result = get_symbols(
            "never-batch",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            TokenizorError::RequestGated { gate_error } => {
                assert!(gate_error.contains("has not been indexed"));
                assert!(gate_error.contains("[next_action: reindex]"));
            }
            other => panic!("expected RequestGated, got: {other}"),
        }
    }

    // --- Task 4.2: Batch success tests (AC: 1) ---

    #[test]
    fn test_get_symbols_returns_multiple_verified_results() {
        let (_dir, manager, blob_store, _run_id) = setup_multi_symbol_env();

        let requests = vec![
            symbol_request("src/a.rs", "alpha", None),
            symbol_request("src/a.rs", "beta", None),
            symbol_request("src/b.rs", "Foo", Some(SymbolKind::Struct)),
        ];

        let result = get_symbols(
            "batch-repo",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert_eq!(result.trust, TrustLevel::Verified);
        assert!(result.provenance.is_some());

        let data = result.data.unwrap();
        assert_eq!(data.results.len(), 3);

        for item in &data.results {
            assert_eq!(batch_result(item).outcome, RetrievalOutcome::Success);
            assert_eq!(batch_result(item).trust, TrustLevel::Verified);
            assert!(batch_result(item).data.is_some());
        }

        assert_eq!(symbol_data(&data.results[0]).symbol_name, "alpha");
        assert_eq!(symbol_data(&data.results[1]).symbol_name, "beta");
        assert_eq!(symbol_data(&data.results[2]).symbol_name, "Foo");
        assert_eq!(
            symbol_data(&data.results[2]).symbol_kind,
            SymbolKind::Struct
        );
    }

    #[test]
    fn test_get_symbols_supports_code_slice_targets() {
        let (_dir, manager, blob_store, _run_id) = setup_multi_symbol_env();

        let requests = vec![
            symbol_request("src/a.rs", "alpha", None),
            code_slice_request("src/a.rs", (15, 27)),
            symbol_request("src/b.rs", "Foo", Some(SymbolKind::Struct)),
        ];

        let result = get_symbols(
            "batch-repo",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        let data = result.data.unwrap();
        assert_eq!(data.results.len(), 3);
        assert_eq!(
            batch_result(&data.results[0]).outcome,
            RetrievalOutcome::Success
        );
        assert_eq!(
            batch_result(&data.results[1]).outcome,
            RetrievalOutcome::Success
        );
        assert_eq!(
            batch_result(&data.results[2]).outcome,
            RetrievalOutcome::Success
        );

        assert_eq!(symbol_data(&data.results[0]).symbol_name, "alpha");
        assert_eq!(code_slice_data(&data.results[1]).byte_range, (15, 27));
        assert!(code_slice_data(&data.results[1]).source.contains("beta"));
        assert_eq!(symbol_data(&data.results[2]).symbol_name, "Foo");
    }

    #[test]
    fn test_get_symbols_preserves_request_order() {
        let (_dir, manager, blob_store, _run_id) = setup_multi_symbol_env();

        // Request in reverse order: Foo then beta then alpha
        let requests = vec![
            symbol_request("src/b.rs", "Foo", Some(SymbolKind::Struct)),
            symbol_request("src/a.rs", "beta", None),
            symbol_request("src/a.rs", "alpha", None),
        ];

        let result = get_symbols(
            "batch-repo",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        let data = result.data.unwrap();
        assert!(matches!(
            &data.results[0],
            BatchRetrievalResultItem::Symbol { symbol_name, .. } if symbol_name == "Foo"
        ));
        assert!(matches!(
            &data.results[1],
            BatchRetrievalResultItem::Symbol { symbol_name, .. } if symbol_name == "beta"
        ));
        assert!(matches!(
            &data.results[2],
            BatchRetrievalResultItem::Symbol { symbol_name, .. } if symbol_name == "alpha"
        ));
    }

    #[test]
    fn test_get_symbols_with_kind_filter() {
        let (_dir, manager, blob_store, _run_id) = setup_multi_symbol_env();

        let requests = vec![
            symbol_request("src/b.rs", "Foo", Some(SymbolKind::Struct)),
            symbol_request("src/b.rs", "Foo", Some(SymbolKind::Impl)),
        ];

        let result = get_symbols(
            "batch-repo",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        let data = result.data.unwrap();
        assert_eq!(data.results.len(), 2);
        assert_eq!(
            symbol_data(&data.results[0]).symbol_kind,
            SymbolKind::Struct
        );
        assert_eq!(symbol_data(&data.results[1]).symbol_kind, SymbolKind::Impl);
    }

    // --- Task 4.3: Mixed outcome tests (AC: 2, 4) ---

    #[test]
    fn test_get_symbols_mixed_outcomes_valid_and_quarantined() {
        let (_dir, manager, blob_store) = setup_search_env();
        let repo_id = "mix-quar";
        register_repo(&manager, repo_id, RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, repo_id);

        let content_ok = "fn good() {}\n";
        let blob_ok = blob_store.store(content_ok.as_bytes());
        let record_ok = make_verified_file_record(
            "src/ok.rs",
            &blob_ok,
            content_ok.len() as u64,
            &run_id,
            repo_id,
            PersistedFileOutcome::Committed,
            vec![verified_symbol(
                "good",
                SymbolKind::Function,
                (0, 13),
                (0, 0),
                0,
            )],
        );

        let record_quar = make_verified_file_record(
            "src/bad.rs",
            "deadbeef",
            100,
            &run_id,
            repo_id,
            PersistedFileOutcome::Quarantined {
                reason: "suspicious".to_string(),
            },
            vec![],
        );

        manager
            .persistence()
            .save_file_records(&run_id, &[record_ok, record_quar])
            .unwrap();

        let requests = vec![
            symbol_request("src/ok.rs", "good", None),
            symbol_request("src/bad.rs", "anything", None),
        ];

        let result = get_symbols(
            repo_id,
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.results.len(), 2);

        // First item: valid
        assert_eq!(
            batch_result(&data.results[0]).outcome,
            RetrievalOutcome::Success
        );
        assert_eq!(batch_result(&data.results[0]).trust, TrustLevel::Verified);

        // Second item: quarantined
        assert_eq!(
            batch_result(&data.results[1]).outcome,
            RetrievalOutcome::Quarantined
        );
        assert_eq!(
            batch_result(&data.results[1]).trust,
            TrustLevel::Quarantined
        );
        assert_eq!(
            batch_result(&data.results[1]).next_action,
            Some(NextAction::Repair)
        );
    }

    #[test]
    fn test_get_symbols_mixed_outcomes_valid_and_missing() {
        let (_dir, manager, blob_store, _run_id) = setup_multi_symbol_env();

        let requests = vec![
            symbol_request("src/a.rs", "alpha", None),
            symbol_request("src/nonexistent.rs", "missing", None),
        ];

        let result = get_symbols(
            "batch-repo",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data = result.data.unwrap();
        assert_eq!(data.results.len(), 2);

        // First item: success
        assert_eq!(
            batch_result(&data.results[0]).outcome,
            RetrievalOutcome::Success
        );

        // Second item: explicit missing outcome
        assert_eq!(
            batch_result(&data.results[1]).outcome,
            RetrievalOutcome::Missing
        );
        assert_eq!(batch_result(&data.results[1]).trust, TrustLevel::Verified);
        assert!(batch_result(&data.results[1]).next_action.is_none());
    }

    #[test]
    fn test_get_symbols_mixed_outcomes_valid_and_blob_mismatch() {
        let (_dir, manager, blob_store) = setup_search_env();
        let repo_id = "mix-blob";
        register_repo(&manager, repo_id, RepositoryStatus::Ready);
        let run_id = create_succeeded_run(&manager, repo_id);

        let good_content = "fn good() {}\n";
        let good_blob = blob_store.store(good_content.as_bytes());
        let record_good = make_verified_file_record(
            "src/good.rs",
            &good_blob,
            good_content.len() as u64,
            &run_id,
            repo_id,
            PersistedFileOutcome::Committed,
            vec![verified_symbol(
                "good",
                SymbolKind::Function,
                (0, 13),
                (0, 0),
                0,
            )],
        );

        let bad_content = "fn bad() {}\n";
        let bad_blob = blob_store.store(bad_content.as_bytes());
        // Store corrupted data under the correct blob_id
        blob_store.store_corrupted(&bad_blob, b"CORRUPTED DATA");
        let record_bad = make_verified_file_record(
            "src/bad.rs",
            &bad_blob,
            bad_content.len() as u64,
            &run_id,
            repo_id,
            PersistedFileOutcome::Committed,
            vec![verified_symbol(
                "bad",
                SymbolKind::Function,
                (0, 12),
                (0, 0),
                0,
            )],
        );

        manager
            .persistence()
            .save_file_records(&run_id, &[record_good, record_bad])
            .unwrap();

        let requests = vec![
            symbol_request("src/good.rs", "good", None),
            symbol_request("src/bad.rs", "bad", None),
        ];

        let result = get_symbols(
            repo_id,
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        let data = result.data.unwrap();
        assert_eq!(data.results.len(), 2);

        // First: success
        assert_eq!(
            batch_result(&data.results[0]).outcome,
            RetrievalOutcome::Success
        );

        // Second: blob mismatch → blocked with reindex
        match &batch_result(&data.results[1]).outcome {
            RetrievalOutcome::Blocked { reason } => {
                assert!(reason.contains("hash mismatch"));
            }
            other => panic!("expected Blocked, got: {other:?}"),
        }
        assert_eq!(batch_result(&data.results[1]).trust, TrustLevel::Suspect);
        assert_eq!(
            batch_result(&data.results[1]).next_action,
            Some(NextAction::Reindex)
        );
    }

    #[test]
    fn test_get_symbols_one_failure_does_not_affect_others() {
        let (_dir, manager, blob_store, _run_id) = setup_multi_symbol_env();

        // Request: valid, missing, valid — the missing one should NOT affect the valid ones
        let requests = vec![
            symbol_request("src/a.rs", "alpha", None),
            symbol_request("src/missing.rs", "gone", None),
            symbol_request("src/b.rs", "Foo", Some(SymbolKind::Struct)),
        ];

        let result = get_symbols(
            "batch-repo",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        let data = result.data.unwrap();
        assert_eq!(data.results.len(), 3);

        // First: success
        assert_eq!(
            batch_result(&data.results[0]).outcome,
            RetrievalOutcome::Success
        );
        assert_eq!(symbol_data(&data.results[0]).symbol_name, "alpha");

        // Second: explicit missing item
        assert_eq!(
            batch_result(&data.results[1]).outcome,
            RetrievalOutcome::Missing
        );
        assert_eq!(batch_result(&data.results[1]).trust, TrustLevel::Verified);

        // Third: success — NOT affected by the mid-batch failure
        assert_eq!(
            batch_result(&data.results[2]).outcome,
            RetrievalOutcome::Success
        );
        assert_eq!(symbol_data(&data.results[2]).symbol_name, "Foo");
    }

    // --- Task 4.4: Edge case tests (AC: 5) ---

    #[test]
    fn test_get_symbols_empty_batch_returns_empty() {
        let (_dir, manager, blob_store, _run_id) = setup_multi_symbol_env();

        let result = get_symbols(
            "batch-repo",
            &[],
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        assert_eq!(result.outcome, RetrievalOutcome::Empty);
        assert_eq!(result.trust, TrustLevel::Verified);
        assert!(result.next_action.is_none());
        let data = result.data.unwrap();
        assert!(data.results.is_empty());
    }

    #[test]
    fn test_get_symbols_single_item_batch() {
        let (_dir, manager, blob_store, _run_id) = setup_multi_symbol_env();

        let requests = vec![symbol_request("src/a.rs", "alpha", None)];

        let batch_response = get_symbols(
            "batch-repo",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        let single_result = get_symbol(
            "batch-repo",
            "src/a.rs",
            "alpha",
            None,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        let batch_data = batch_response.data.unwrap();
        assert_eq!(batch_data.results.len(), 1);
        let batch_item = &batch_data.results[0];

        // The per-item result should match get_symbol result
        assert_eq!(batch_result(batch_item).outcome, single_result.outcome);
        assert_eq!(batch_result(batch_item).trust, single_result.trust);
        assert_eq!(
            symbol_data(batch_item).source,
            single_result.data.as_ref().unwrap().source
        );
    }

    #[test]
    fn test_get_symbols_duplicate_requests() {
        let (_dir, manager, blob_store, _run_id) = setup_multi_symbol_env();

        let requests = vec![
            symbol_request("src/a.rs", "alpha", None),
            symbol_request("src/a.rs", "alpha", None),
        ];

        let result = get_symbols(
            "batch-repo",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        )
        .unwrap();

        let data = result.data.unwrap();
        assert_eq!(data.results.len(), 2);
        // Both should succeed independently
        assert_eq!(
            batch_result(&data.results[0]).outcome,
            RetrievalOutcome::Success
        );
        assert_eq!(
            batch_result(&data.results[1]).outcome,
            RetrievalOutcome::Success
        );
        assert_eq!(
            symbol_data(&data.results[0]).source,
            symbol_data(&data.results[1]).source
        );
    }

    #[test]
    fn test_get_symbols_not_indexed_returns_not_indexed() {
        let (_dir, manager, blob_store) = setup_search_env();
        register_repo(&manager, "no-runs-batch", RepositoryStatus::Ready);
        create_failed_run(&manager, "no-runs-batch");

        let requests = vec![symbol_request("src/a.rs", "alpha", None)];

        // This repo has a failed run but no successful runs → gate should reject
        let result = get_symbols(
            "no-runs-batch",
            &requests,
            manager.persistence(),
            &manager,
            &*blob_store,
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            TokenizorError::RequestGated { gate_error } => {
                assert!(gate_error.contains("no successful index"));
            }
            other => panic!("expected RequestGated, got: {other}"),
        }
    }
}

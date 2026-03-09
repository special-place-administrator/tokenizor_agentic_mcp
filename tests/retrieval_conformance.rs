use tokenizor_agentic_mcp::domain::{
    BatchRetrievalRequest, BatchRetrievalResponseData, BatchRetrievalResultItem, CodeSliceRequest,
    FileOutcomeStatus, FileOutlineResponse, GetSymbolsResponse, IndexRunStatus, LanguageId,
    NextAction, OutlineSymbol, Provenance, RepoOutlineCoverage, RepoOutlineEntry,
    RepoOutlineResponse, RepositoryStatus, RequestGateError, ResultEnvelope, RetrievalOutcome,
    SearchResultItem, SymbolCoverage, SymbolKind, SymbolRequest, SymbolResultItem,
    SymbolSearchResponse, TrustLevel, VerifiedCodeSliceResponse, VerifiedSourceResponse,
};

#[test]
fn test_retrieval_outcome_variants_are_exhaustive() {
    let outcomes = vec![
        RetrievalOutcome::Success,
        RetrievalOutcome::Empty,
        RetrievalOutcome::Missing,
        RetrievalOutcome::NotIndexed,
        RetrievalOutcome::Stale,
        RetrievalOutcome::Quarantined,
        RetrievalOutcome::Blocked {
            reason: "test".to_string(),
        },
    ];
    // Exhaustive match proves all variants are covered
    for outcome in &outcomes {
        match outcome {
            RetrievalOutcome::Success => {}
            RetrievalOutcome::Empty => {}
            RetrievalOutcome::Missing => {}
            RetrievalOutcome::NotIndexed => {}
            RetrievalOutcome::Stale => {}
            RetrievalOutcome::Quarantined => {}
            RetrievalOutcome::Blocked { reason: _ } => {}
        }
    }
    assert_eq!(outcomes.len(), 7);
}

#[test]
fn test_trust_level_variants_are_exhaustive() {
    let levels = vec![
        TrustLevel::Verified,
        TrustLevel::Unverified,
        TrustLevel::Suspect,
        TrustLevel::Quarantined,
    ];
    for level in &levels {
        match level {
            TrustLevel::Verified => {}
            TrustLevel::Unverified => {}
            TrustLevel::Suspect => {}
            TrustLevel::Quarantined => {}
        }
    }
    assert_eq!(levels.len(), 4);
}

#[test]
fn test_request_gate_error_covers_all_fatal_conditions() {
    let errors = vec![
        RequestGateError::NoActiveContext,
        RequestGateError::RepositoryInvalidated {
            reason: Some("test".to_string()),
        },
        RequestGateError::RepositoryFailed,
        RequestGateError::RepositoryDegraded,
        RequestGateError::RepositoryQuarantined {
            reason: Some("suspect data".to_string()),
        },
        RequestGateError::ActiveMutation {
            run_id: "run-1".to_string(),
        },
        RequestGateError::NeverIndexed,
        RequestGateError::NoSuccessfulRuns {
            latest_status: IndexRunStatus::Failed,
        },
    ];
    for error in &errors {
        match error {
            RequestGateError::NoActiveContext => {}
            RequestGateError::RepositoryInvalidated { .. } => {}
            RequestGateError::RepositoryFailed => {}
            RequestGateError::RepositoryDegraded => {}
            RequestGateError::RepositoryQuarantined { .. } => {
                assert_eq!(error.next_action(), NextAction::Repair);
            }
            RequestGateError::ActiveMutation { .. } => {}
            RequestGateError::NeverIndexed => {}
            RequestGateError::NoSuccessfulRuns { .. } => {}
        }
    }
    assert_eq!(errors.len(), 8);
}

#[test]
fn test_provenance_is_constructable_and_has_required_fields() {
    let prov = Provenance {
        run_id: "run-123".to_string(),
        committed_at_unix_ms: 1000,
        repo_id: "repo-1".to_string(),
    };
    assert_eq!(prov.run_id, "run-123");
    assert_eq!(prov.committed_at_unix_ms, 1000);
    assert_eq!(prov.repo_id, "repo-1");
}

#[test]
fn test_result_envelope_is_constructable_with_generic_data() {
    let envelope: ResultEnvelope<Vec<String>> = ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(Provenance {
            run_id: "run-1".to_string(),
            committed_at_unix_ms: 1000,
            repo_id: "repo-1".to_string(),
        }),
        data: Some(vec!["test".to_string()]),
        next_action: None,
    };
    assert_eq!(envelope.outcome, RetrievalOutcome::Success);
    assert!(envelope.data.is_some());
}

#[test]
fn test_search_result_item_includes_all_provenance_fields() {
    use tokenizor_agentic_mcp::domain::LanguageId;

    let item = SearchResultItem {
        relative_path: "src/main.rs".to_string(),
        language: LanguageId::Rust,
        line_number: 1,
        line_content: "fn main() {}".to_string(),
        match_offset: 3,
        match_length: 4,
        provenance: Provenance {
            run_id: "run-1".to_string(),
            committed_at_unix_ms: 1000,
            repo_id: "repo-1".to_string(),
        },
    };
    assert_eq!(item.provenance.run_id, "run-1");
    assert_eq!(item.provenance.committed_at_unix_ms, 1000);
    assert_eq!(item.provenance.repo_id, "repo-1");
}

#[test]
fn test_contract_types_are_serializable() {
    let outcome = RetrievalOutcome::Success;
    let json = serde_json::to_string(&outcome).unwrap();
    let deserialized: RetrievalOutcome = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, outcome);

    let trust = TrustLevel::Verified;
    let json = serde_json::to_string(&trust).unwrap();
    let deserialized: TrustLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, trust);

    let gate = RequestGateError::NeverIndexed;
    let json = serde_json::to_string(&gate).unwrap();
    let deserialized: RequestGateError = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, gate);

    let prov = Provenance {
        run_id: "r".to_string(),
        committed_at_unix_ms: 1,
        repo_id: "p".to_string(),
    };
    let json = serde_json::to_string(&prov).unwrap();
    let deserialized: Provenance = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, prov);

    let envelope: ResultEnvelope<Vec<String>> = ResultEnvelope {
        outcome: RetrievalOutcome::Empty,
        trust: TrustLevel::Verified,
        provenance: None,
        data: None,
        next_action: None,
    };
    let json = serde_json::to_string(&envelope).unwrap();
    let deserialized: ResultEnvelope<Vec<String>> = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, envelope);
}

#[test]
fn test_gate_error_disambiguates_never_indexed_vs_no_successful_runs() {
    let never = RequestGateError::NeverIndexed;
    let no_success = RequestGateError::NoSuccessfulRuns {
        latest_status: IndexRunStatus::Failed,
    };
    assert_ne!(never, no_success);

    let never_display = format!("{never}");
    let no_success_display = format!("{no_success}");
    assert!(never_display.contains("has not been indexed"));
    assert!(no_success_display.contains("no successful index"));
    assert!(no_success_display.contains("Failed"));
}

// --- Symbol search conformance tests (Story 3.2) ---

#[test]
fn test_symbol_result_item_is_constructable_and_serializable() {
    let item = SymbolResultItem {
        symbol_name: "HashMap".to_string(),
        symbol_kind: SymbolKind::Struct,
        relative_path: "src/lib.rs".to_string(),
        language: LanguageId::Rust,
        line_range: (10, 25),
        byte_range: (100, 500),
        depth: 0,
        provenance: Provenance {
            run_id: "run-1".to_string(),
            committed_at_unix_ms: 1000,
            repo_id: "repo-1".to_string(),
        },
    };

    let json = serde_json::to_string(&item).unwrap();
    let deserialized: SymbolResultItem = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, item);
}

#[test]
fn test_symbol_search_response_is_constructable_and_serializable() {
    let response = SymbolSearchResponse {
        matches: vec![SymbolResultItem {
            symbol_name: "main".to_string(),
            symbol_kind: SymbolKind::Function,
            relative_path: "src/main.rs".to_string(),
            language: LanguageId::Rust,
            line_range: (0, 3),
            byte_range: (0, 50),
            depth: 0,
            provenance: Provenance {
                run_id: "run-1".to_string(),
                committed_at_unix_ms: 1000,
                repo_id: "repo-1".to_string(),
            },
        }],
        coverage: SymbolCoverage {
            files_searched: 5,
            files_without_symbols: 2,
            files_skipped_quarantined: 1,
        },
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: SymbolSearchResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, response);
}

#[test]
fn test_symbol_coverage_is_constructable_and_serializable() {
    let coverage = SymbolCoverage {
        files_searched: 10,
        files_without_symbols: 3,
        files_skipped_quarantined: 1,
    };

    let json = serde_json::to_string(&coverage).unwrap();
    let deserialized: SymbolCoverage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, coverage);
}

#[test]
fn test_symbol_result_item_includes_all_required_provenance_fields() {
    let item = SymbolResultItem {
        symbol_name: "test_fn".to_string(),
        symbol_kind: SymbolKind::Function,
        relative_path: "src/test.rs".to_string(),
        language: LanguageId::Rust,
        line_range: (0, 5),
        byte_range: (0, 100),
        depth: 0,
        provenance: Provenance {
            run_id: "run-abc".to_string(),
            committed_at_unix_ms: 42000,
            repo_id: "repo-xyz".to_string(),
        },
    };

    assert_eq!(item.provenance.run_id, "run-abc");
    assert_eq!(item.provenance.committed_at_unix_ms, 42000);
    assert_eq!(item.provenance.repo_id, "repo-xyz");
}

// --- Outline conformance tests (Story 3.3) ---

#[test]
fn test_outline_symbol_is_constructable_and_serializable() {
    let symbol = OutlineSymbol {
        name: "main".to_string(),
        kind: SymbolKind::Function,
        line_range: (0, 3),
        byte_range: (0, 40),
        depth: 0,
        sort_order: 1,
    };

    let json = serde_json::to_string(&symbol).unwrap();
    let deserialized: OutlineSymbol = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, symbol);
}

#[test]
fn test_file_outline_response_is_constructable_and_serializable() {
    let response = FileOutlineResponse {
        relative_path: "src/main.rs".to_string(),
        language: LanguageId::Rust,
        byte_len: 128,
        symbols: vec![OutlineSymbol {
            name: "main".to_string(),
            kind: SymbolKind::Function,
            line_range: (0, 3),
            byte_range: (0, 40),
            depth: 0,
            sort_order: 0,
        }],
        has_symbol_support: true,
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: FileOutlineResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, response);
}

#[test]
fn test_file_outcome_status_is_constructable_and_serializable() {
    let statuses = vec![
        FileOutcomeStatus::Committed,
        FileOutcomeStatus::EmptySymbols,
        FileOutcomeStatus::Failed,
        FileOutcomeStatus::Quarantined,
    ];

    for status in statuses {
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: FileOutcomeStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }
}

#[test]
fn test_repo_outline_entry_is_constructable_and_serializable() {
    let entry = RepoOutlineEntry {
        relative_path: "src/lib.rs".to_string(),
        language: LanguageId::Rust,
        byte_len: 512,
        symbol_count: 3,
        status: FileOutcomeStatus::Committed,
    };

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: RepoOutlineEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, entry);
}

#[test]
fn test_repo_outline_coverage_is_constructable_and_serializable() {
    let coverage = RepoOutlineCoverage {
        total_files: 12,
        files_with_symbols: 6,
        files_without_symbols: 3,
        files_quarantined: 2,
        files_failed: 1,
    };

    let json = serde_json::to_string(&coverage).unwrap();
    let deserialized: RepoOutlineCoverage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, coverage);
}

#[test]
fn test_repo_outline_response_is_constructable_and_serializable() {
    let response = RepoOutlineResponse {
        files: vec![RepoOutlineEntry {
            relative_path: "src/main.rs".to_string(),
            language: LanguageId::Rust,
            byte_len: 256,
            symbol_count: 2,
            status: FileOutcomeStatus::Committed,
        }],
        coverage: RepoOutlineCoverage {
            total_files: 1,
            files_with_symbols: 1,
            files_without_symbols: 0,
            files_quarantined: 0,
            files_failed: 0,
        },
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: RepoOutlineResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, response);
}

// --- Story 3.4: Serialization fidelity conformance tests ---

fn assert_envelope_json_has_required_fields(json: &str) {
    let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
    assert!(
        parsed.get("outcome").is_some(),
        "missing 'outcome' field in JSON"
    );
    assert!(
        parsed.get("trust").is_some(),
        "missing 'trust' field in JSON"
    );
    assert!(
        parsed.get("provenance").is_some(),
        "missing 'provenance' field in JSON"
    );
    assert!(parsed.get("data").is_some(), "missing 'data' field in JSON");
}

#[test]
fn test_result_envelope_search_result_item_serializes_with_expected_fields() {
    let envelope: ResultEnvelope<Vec<SearchResultItem>> = ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(Provenance {
            run_id: "run-1".to_string(),
            committed_at_unix_ms: 1000,
            repo_id: "repo-1".to_string(),
        }),
        data: Some(vec![SearchResultItem {
            relative_path: "src/main.rs".to_string(),
            language: LanguageId::Rust,
            line_number: 1,
            line_content: "fn main() {}".to_string(),
            match_offset: 3,
            match_length: 4,
            provenance: Provenance {
                run_id: "run-1".to_string(),
                committed_at_unix_ms: 1000,
                repo_id: "repo-1".to_string(),
            },
        }]),
        next_action: None,
    };

    let json = serde_json::to_string_pretty(&envelope).unwrap();
    assert_envelope_json_has_required_fields(&json);

    let round_tripped: ResultEnvelope<Vec<SearchResultItem>> = serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped, envelope);
}

#[test]
fn test_result_envelope_symbol_search_response_serializes_with_expected_fields() {
    let envelope: ResultEnvelope<SymbolSearchResponse> = ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(Provenance {
            run_id: "run-1".to_string(),
            committed_at_unix_ms: 1000,
            repo_id: "repo-1".to_string(),
        }),
        data: Some(SymbolSearchResponse {
            matches: vec![SymbolResultItem {
                symbol_name: "main".to_string(),
                symbol_kind: SymbolKind::Function,
                relative_path: "src/main.rs".to_string(),
                language: LanguageId::Rust,
                line_range: (0, 3),
                byte_range: (0, 50),
                depth: 0,
                provenance: Provenance {
                    run_id: "run-1".to_string(),
                    committed_at_unix_ms: 1000,
                    repo_id: "repo-1".to_string(),
                },
            }],
            coverage: SymbolCoverage {
                files_searched: 1,
                files_without_symbols: 0,
                files_skipped_quarantined: 0,
            },
        }),
        next_action: None,
    };

    let json = serde_json::to_string_pretty(&envelope).unwrap();
    assert_envelope_json_has_required_fields(&json);

    let round_tripped: ResultEnvelope<SymbolSearchResponse> = serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped, envelope);
}

#[test]
fn test_result_envelope_file_outline_response_serializes_with_expected_fields() {
    let envelope: ResultEnvelope<FileOutlineResponse> = ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(Provenance {
            run_id: "run-1".to_string(),
            committed_at_unix_ms: 1000,
            repo_id: "repo-1".to_string(),
        }),
        data: Some(FileOutlineResponse {
            relative_path: "src/main.rs".to_string(),
            language: LanguageId::Rust,
            byte_len: 128,
            symbols: vec![OutlineSymbol {
                name: "main".to_string(),
                kind: SymbolKind::Function,
                line_range: (0, 3),
                byte_range: (0, 40),
                depth: 0,
                sort_order: 0,
            }],
            has_symbol_support: true,
        }),
        next_action: None,
    };

    let json = serde_json::to_string_pretty(&envelope).unwrap();
    assert_envelope_json_has_required_fields(&json);

    let round_tripped: ResultEnvelope<FileOutlineResponse> = serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped, envelope);
}

#[test]
fn test_result_envelope_repo_outline_response_serializes_with_expected_fields() {
    let envelope: ResultEnvelope<RepoOutlineResponse> = ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(Provenance {
            run_id: "run-1".to_string(),
            committed_at_unix_ms: 1000,
            repo_id: "repo-1".to_string(),
        }),
        data: Some(RepoOutlineResponse {
            files: vec![RepoOutlineEntry {
                relative_path: "src/main.rs".to_string(),
                language: LanguageId::Rust,
                byte_len: 256,
                symbol_count: 2,
                status: FileOutcomeStatus::Committed,
            }],
            coverage: RepoOutlineCoverage {
                total_files: 1,
                files_with_symbols: 1,
                files_without_symbols: 0,
                files_quarantined: 0,
                files_failed: 0,
            },
        }),
        next_action: None,
    };

    let json = serde_json::to_string_pretty(&envelope).unwrap();
    assert_envelope_json_has_required_fields(&json);

    let round_tripped: ResultEnvelope<RepoOutlineResponse> = serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped, envelope);
}

#[test]
fn test_retrieval_outcome_serde_round_trip_all_variants() {
    let variants = vec![
        RetrievalOutcome::Success,
        RetrievalOutcome::Empty,
        RetrievalOutcome::Missing,
        RetrievalOutcome::NotIndexed,
        RetrievalOutcome::Stale,
        RetrievalOutcome::Quarantined,
        RetrievalOutcome::Blocked {
            reason: "test reason".to_string(),
        },
    ];
    for variant in variants {
        let json = serde_json::to_string(&variant).unwrap();
        let deserialized: RetrievalOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, variant, "round-trip failed for {json}");
    }
}

#[test]
fn test_trust_level_serde_round_trip_all_variants() {
    let variants = vec![
        TrustLevel::Verified,
        TrustLevel::Unverified,
        TrustLevel::Suspect,
        TrustLevel::Quarantined,
    ];
    for variant in variants {
        let json = serde_json::to_string(&variant).unwrap();
        let deserialized: TrustLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, variant, "round-trip failed for {json}");
    }
}

// --- Story 3.5: VerifiedSourceResponse conformance tests ---

#[test]
fn test_verified_source_response_is_constructable_and_serializable() {
    let response = VerifiedSourceResponse {
        relative_path: "src/main.rs".to_string(),
        language: LanguageId::Rust,
        symbol_name: "main".to_string(),
        symbol_kind: SymbolKind::Function,
        line_range: (0, 3),
        byte_range: (0, 40),
        source: "fn main() {\n    println!(\"hello\");\n}".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: VerifiedSourceResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, response);
}

#[test]
fn test_result_envelope_verified_source_response_serializes_with_expected_fields() {
    let envelope: ResultEnvelope<VerifiedSourceResponse> = ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(Provenance {
            run_id: "run-1".to_string(),
            committed_at_unix_ms: 1000,
            repo_id: "repo-1".to_string(),
        }),
        data: Some(VerifiedSourceResponse {
            relative_path: "src/main.rs".to_string(),
            language: LanguageId::Rust,
            symbol_name: "main".to_string(),
            symbol_kind: SymbolKind::Function,
            line_range: (0, 3),
            byte_range: (0, 40),
            source: "fn main() {}".to_string(),
        }),
        next_action: None,
    };

    let json = serde_json::to_string_pretty(&envelope).unwrap();
    assert_envelope_json_has_required_fields(&json);

    let round_tripped: ResultEnvelope<VerifiedSourceResponse> =
        serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped, envelope);
}

#[test]
fn test_verified_source_response_json_includes_all_fields() {
    let response = VerifiedSourceResponse {
        relative_path: "src/lib.rs".to_string(),
        language: LanguageId::Rust,
        symbol_name: "add".to_string(),
        symbol_kind: SymbolKind::Function,
        line_range: (5, 8),
        byte_range: (100, 200),
        source: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(
        parsed.get("relative_path").is_some(),
        "missing 'relative_path'"
    );
    assert!(parsed.get("language").is_some(), "missing 'language'");
    assert!(parsed.get("symbol_name").is_some(), "missing 'symbol_name'");
    assert!(parsed.get("symbol_kind").is_some(), "missing 'symbol_kind'");
    assert!(parsed.get("line_range").is_some(), "missing 'line_range'");
    assert!(parsed.get("byte_range").is_some(), "missing 'byte_range'");
    assert!(parsed.get("source").is_some(), "missing 'source'");
}

// --- Story 3.6: NextAction and Quarantine conformance tests ---

#[test]
fn test_next_action_variants_are_exhaustive() {
    let variants = vec![
        NextAction::Resume,
        NextAction::Reindex,
        NextAction::Repair,
        NextAction::Wait,
        NextAction::ResolveContext,
    ];
    for variant in &variants {
        let json = serde_json::to_string(variant).unwrap();
        let deserialized: NextAction = serde_json::from_str(&json).unwrap();
        assert_eq!(&deserialized, variant, "round-trip failed for {json}");
    }
    assert_eq!(variants.len(), 5);
    // Exhaustive match proves all variants are covered
    for variant in &variants {
        match variant {
            NextAction::Resume => {}
            NextAction::Reindex => {}
            NextAction::Repair => {}
            NextAction::Wait => {}
            NextAction::ResolveContext => {}
        }
    }
}

#[test]
fn test_result_envelope_with_next_action_serializes() {
    // With next_action present
    let envelope: ResultEnvelope<String> = ResultEnvelope {
        outcome: RetrievalOutcome::Quarantined,
        trust: TrustLevel::Quarantined,
        provenance: None,
        data: None,
        next_action: Some(NextAction::Repair),
    };

    let json = serde_json::to_string(&envelope).unwrap();
    assert!(
        json.contains("\"next_action\":\"repair\""),
        "expected '\"next_action\":\"repair\"' in: {json}"
    );

    // Round-trip
    let deserialized: ResultEnvelope<String> = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, envelope);

    // With next_action absent (None) — key should be omitted via skip_serializing_if
    let envelope_none: ResultEnvelope<String> = ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: None,
        data: Some("data".to_string()),
        next_action: None,
    };

    let json_none = serde_json::to_string(&envelope_none).unwrap();
    assert!(
        !json_none.contains("next_action"),
        "expected 'next_action' key to be absent in: {json_none}"
    );
}

#[test]
fn test_request_gate_error_quarantined_variant() {
    let error = RequestGateError::RepositoryQuarantined {
        reason: Some("test".to_string()),
    };

    let json = serde_json::to_string(&error).unwrap();
    let deserialized: RequestGateError = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, error);

    // Verify the reason round-trips
    match &deserialized {
        RequestGateError::RepositoryQuarantined { reason } => {
            assert_eq!(reason.as_deref(), Some("test"));
        }
        other => panic!("expected RepositoryQuarantined, got: {other:?}"),
    }

    // Also test with None reason
    let error_no_reason = RequestGateError::RepositoryQuarantined { reason: None };
    let json_no_reason = serde_json::to_string(&error_no_reason).unwrap();
    let deserialized_no_reason: RequestGateError = serde_json::from_str(&json_no_reason).unwrap();
    assert_eq!(deserialized_no_reason, error_no_reason);
}

#[test]
fn test_repository_status_quarantined_variant() {
    let status = RepositoryStatus::Quarantined;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"quarantined\"");

    let deserialized: RepositoryStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, RepositoryStatus::Quarantined);
}

// --- Story 3.7: Batch retrieval conformance tests ---

#[test]
fn test_symbol_request_serializes() {
    let request = SymbolRequest {
        relative_path: "src/main.rs".to_string(),
        symbol_name: "main".to_string(),
        kind_filter: Some(SymbolKind::Function),
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: SymbolRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, request);

    // Without kind_filter
    let request_no_kind = SymbolRequest {
        relative_path: "src/lib.rs".to_string(),
        symbol_name: "add".to_string(),
        kind_filter: None,
    };
    let json_no_kind = serde_json::to_string(&request_no_kind).unwrap();
    assert!(
        !json_no_kind.contains("kind_filter"),
        "kind_filter should be omitted when None: {json_no_kind}"
    );
    let deserialized_no_kind: SymbolRequest = serde_json::from_str(&json_no_kind).unwrap();
    assert_eq!(deserialized_no_kind, request_no_kind);
}

#[test]
fn test_code_slice_request_serializes() {
    let request = CodeSliceRequest {
        relative_path: "src/main.rs".to_string(),
        byte_range: (5, 15),
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CodeSliceRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, request);
}

#[test]
fn test_batch_retrieval_request_serializes() {
    let symbol_request = BatchRetrievalRequest::Symbol {
        relative_path: "src/main.rs".to_string(),
        symbol_name: "main".to_string(),
        kind_filter: Some(SymbolKind::Function),
    };
    let slice_request = BatchRetrievalRequest::CodeSlice {
        relative_path: "src/lib.rs".to_string(),
        byte_range: (10, 20),
    };

    let symbol_json = serde_json::to_string(&symbol_request).unwrap();
    let symbol_round_trip: BatchRetrievalRequest = serde_json::from_str(&symbol_json).unwrap();
    assert_eq!(symbol_round_trip, symbol_request);
    assert!(symbol_json.contains("\"request_type\":\"symbol\""));

    let slice_json = serde_json::to_string(&slice_request).unwrap();
    let slice_round_trip: BatchRetrievalRequest = serde_json::from_str(&slice_json).unwrap();
    assert_eq!(slice_round_trip, slice_request);
    assert!(slice_json.contains("\"request_type\":\"code_slice\""));
}

#[test]
fn test_verified_code_slice_response_serializes() {
    let response = VerifiedCodeSliceResponse {
        relative_path: "src/lib.rs".to_string(),
        language: LanguageId::Rust,
        line_range: (1, 2),
        byte_range: (10, 20),
        source: "let value = 42;".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: VerifiedCodeSliceResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, response);
}

#[test]
fn test_batch_retrieval_result_item_serializes() {
    let item = BatchRetrievalResultItem::Symbol {
        relative_path: "src/main.rs".to_string(),
        symbol_name: "main".to_string(),
        kind_filter: None,
        result: ResultEnvelope {
            outcome: RetrievalOutcome::Success,
            trust: TrustLevel::Verified,
            provenance: Some(Provenance {
                run_id: "run-1".to_string(),
                committed_at_unix_ms: 1000,
                repo_id: "repo-1".to_string(),
            }),
            data: Some(BatchRetrievalResponseData::Symbol(VerifiedSourceResponse {
                relative_path: "src/main.rs".to_string(),
                language: LanguageId::Rust,
                symbol_name: "main".to_string(),
                symbol_kind: SymbolKind::Function,
                line_range: (0, 3),
                byte_range: (0, 40),
                source: "fn main() {}".to_string(),
            })),
            next_action: None,
        },
    };

    let json = serde_json::to_string(&item).unwrap();
    let deserialized: BatchRetrievalResultItem = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, item);
    assert!(json.contains("\"request_type\":\"symbol\""));
}

#[test]
fn test_get_symbols_response_serializes() {
    let response = GetSymbolsResponse {
        results: vec![
            BatchRetrievalResultItem::Symbol {
                relative_path: "src/a.rs".to_string(),
                symbol_name: "ok_fn".to_string(),
                kind_filter: Some(SymbolKind::Function),
                result: ResultEnvelope {
                    outcome: RetrievalOutcome::Success,
                    trust: TrustLevel::Verified,
                    provenance: Some(Provenance {
                        run_id: "run-1".to_string(),
                        committed_at_unix_ms: 1000,
                        repo_id: "repo-1".to_string(),
                    }),
                    data: Some(BatchRetrievalResponseData::Symbol(VerifiedSourceResponse {
                        relative_path: "src/a.rs".to_string(),
                        language: LanguageId::Rust,
                        symbol_name: "ok_fn".to_string(),
                        symbol_kind: SymbolKind::Function,
                        line_range: (0, 1),
                        byte_range: (0, 20),
                        source: "fn ok_fn() {}".to_string(),
                    })),
                    next_action: None,
                },
            },
            BatchRetrievalResultItem::CodeSlice {
                relative_path: "src/b.rs".to_string(),
                byte_range: (5, 15),
                result: ResultEnvelope {
                    outcome: RetrievalOutcome::Missing,
                    trust: TrustLevel::Verified,
                    provenance: Some(Provenance {
                        run_id: "run-1".to_string(),
                        committed_at_unix_ms: 1000,
                        repo_id: "repo-1".to_string(),
                    }),
                    data: None,
                    next_action: None,
                },
            },
        ],
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: GetSymbolsResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, response);

    // Verify JSON structure includes per-item next_action where present
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let results = parsed["results"].as_array().unwrap();
    // Success item should NOT have next_action
    assert!(
        results[0]["result"].get("next_action").is_none(),
        "success item should omit next_action"
    );
    assert_eq!(results[0]["request_type"], "symbol");
    assert_eq!(results[1]["request_type"], "code_slice");
    assert_eq!(results[1]["result"]["outcome"], "missing");
    assert!(results[1]["result"].get("next_action").is_none());
}

#[test]
fn test_batch_envelope_success_omits_next_action() {
    let envelope: ResultEnvelope<GetSymbolsResponse> = ResultEnvelope {
        outcome: RetrievalOutcome::Success,
        trust: TrustLevel::Verified,
        provenance: Some(Provenance {
            run_id: "run-1".to_string(),
            committed_at_unix_ms: 1000,
            repo_id: "repo-1".to_string(),
        }),
        data: Some(GetSymbolsResponse { results: vec![] }),
        next_action: None,
    };

    let json = serde_json::to_string(&envelope).unwrap();
    assert!(
        !json.contains("next_action"),
        "outer envelope with Success should omit next_action: {json}"
    );

    let deserialized: ResultEnvelope<GetSymbolsResponse> = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, envelope);
}

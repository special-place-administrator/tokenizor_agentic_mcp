use reqwest::Url;
use rmcp::ErrorData as McpError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    AnnotateAble, RawResource, RawResourceTemplate, ReadResourceResult, Resource, ResourceContents,
    ResourceTemplate,
};

use super::TokenizorServer;
use crate::protocol::tools::{
    GetFileContentInput, GetFileContextInput, GetSymbolContextInput, GetSymbolInput,
    WhatChangedInput,
};

pub(crate) const REPO_HEALTH_URI: &str = "tokenizor://repo/health";
pub(crate) const REPO_OUTLINE_URI: &str = "tokenizor://repo/outline";
pub(crate) const REPO_MAP_URI: &str = "tokenizor://repo/map";
pub(crate) const REPO_CHANGES_URI: &str = "tokenizor://repo/changes/uncommitted";

pub(crate) const FILE_CONTEXT_TEMPLATE: &str =
    "tokenizor://file/context?path={path}&max_tokens={max_tokens}";
pub(crate) const FILE_CONTENT_TEMPLATE: &str =
    "tokenizor://file/content?path={path}&start_line={start_line}&end_line={end_line}";
pub(crate) const SYMBOL_DETAIL_TEMPLATE: &str =
    "tokenizor://symbol/detail?path={path}&name={name}&kind={kind}";
pub(crate) const SYMBOL_CONTEXT_TEMPLATE: &str =
    "tokenizor://symbol/context?name={name}&file={file}";

enum ResourceRequest {
    RepoHealth,
    RepoOutline,
    RepoMap,
    RepoChangesUncommitted,
    FileContext {
        path: String,
        max_tokens: Option<u64>,
    },
    FileContent {
        path: String,
        start_line: Option<u32>,
        end_line: Option<u32>,
    },
    SymbolDetail {
        path: String,
        name: String,
        kind: Option<String>,
    },
    SymbolContext {
        name: String,
        file: Option<String>,
    },
}

impl TokenizorServer {
    pub(crate) fn resource_definitions(&self) -> Vec<Resource> {
        vec![
            make_resource(
                REPO_HEALTH_URI,
                "repo-health",
                "Repository health",
                "Live health report for the current project runtime.",
            ),
            make_resource(
                REPO_OUTLINE_URI,
                "repo-outline",
                "Repository outline",
                "Compact file-level outline for the current project.",
            ),
            make_resource(
                REPO_MAP_URI,
                "repo-map",
                "Repository map",
                "Compact directory and symbol map for the current project.",
            ),
            make_resource(
                REPO_CHANGES_URI,
                "repo-changes-uncommitted",
                "Uncommitted changes",
                "Changed files in the current worktree.",
            ),
        ]
    }

    pub(crate) fn resource_template_definitions(&self) -> Vec<ResourceTemplate> {
        vec![
            make_resource_template(
                FILE_CONTEXT_TEMPLATE,
                "file-context",
                "File context",
                "File outline plus key external references.",
            ),
            make_resource_template(
                FILE_CONTENT_TEMPLATE,
                "file-content",
                "File content",
                "Cached file content with optional line range.",
            ),
            make_resource_template(
                SYMBOL_DETAIL_TEMPLATE,
                "symbol-detail",
                "Symbol detail",
                "Definition body for a symbol in a file.",
            ),
            make_resource_template(
                SYMBOL_CONTEXT_TEMPLATE,
                "symbol-context",
                "Symbol context",
                "Grouped references for a symbol with enclosing annotations.",
            ),
        ]
    }

    pub(crate) async fn read_resource_uri(
        &self,
        uri: &str,
    ) -> Result<ReadResourceResult, McpError> {
        let request =
            parse_resource_uri(uri).map_err(|error| McpError::invalid_params(error, None))?;
        let text = self
            .render_resource_text(request)
            .await
            .map_err(|error| McpError::invalid_params(error, None))?;

        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, uri.to_string()).with_mime_type("text/markdown"),
        ]))
    }

    async fn render_resource_text(&self, request: ResourceRequest) -> Result<String, String> {
        let text = match request {
            ResourceRequest::RepoHealth => self.health().await,
            ResourceRequest::RepoOutline => self.get_repo_outline().await,
            ResourceRequest::RepoMap => self.get_repo_map().await,
            ResourceRequest::RepoChangesUncommitted => {
                self.what_changed(Parameters(WhatChangedInput {
                    since: None,
                    git_ref: None,
                    uncommitted: None,
                }))
                .await
            }
            ResourceRequest::FileContext { path, max_tokens } => {
                self.get_file_context(Parameters(GetFileContextInput { path, max_tokens }))
                    .await
            }
            ResourceRequest::FileContent {
                path,
                start_line,
                end_line,
            } => {
                self.get_file_content(Parameters(GetFileContentInput {
                    path,
                    start_line,
                    end_line,
                    around_line: None,
                    context_lines: None,
                }))
                .await
            }
            ResourceRequest::SymbolDetail { path, name, kind } => {
                self.get_symbol(Parameters(GetSymbolInput { path, name, kind }))
                    .await
            }
            ResourceRequest::SymbolContext { name, file } => {
                self.get_symbol_context(Parameters(GetSymbolContextInput {
                    name,
                    file,
                    path: None,
                    symbol_kind: None,
                    symbol_line: None,
                }))
                .await
            }
        };

        Ok(text)
    }
}

pub(crate) fn repo_health_resource() -> Resource {
    make_resource(
        REPO_HEALTH_URI,
        "repo-health",
        "Repository health",
        "Live health report for the current project runtime.",
    )
}

pub(crate) fn repo_outline_resource() -> Resource {
    make_resource(
        REPO_OUTLINE_URI,
        "repo-outline",
        "Repository outline",
        "Compact file-level outline for the current project.",
    )
}

pub(crate) fn repo_map_resource() -> Resource {
    make_resource(
        REPO_MAP_URI,
        "repo-map",
        "Repository map",
        "Compact directory and symbol map for the current project.",
    )
}

pub(crate) fn repo_changes_resource() -> Resource {
    make_resource(
        REPO_CHANGES_URI,
        "repo-changes-uncommitted",
        "Uncommitted changes",
        "Changed files in the current worktree.",
    )
}

pub(crate) fn file_context_resource(path: &str, max_tokens: Option<u64>) -> Resource {
    let uri = build_uri(
        "tokenizor://file/context",
        &[
            ("path", Some(path.to_string())),
            ("max_tokens", max_tokens.map(|v| v.to_string())),
        ],
    );
    make_resource(
        &uri,
        "file-context",
        "File context",
        "File outline plus key external references.",
    )
}

fn make_resource(uri: &str, name: &str, title: &str, description: &str) -> Resource {
    RawResource::new(uri.to_string(), name.to_string())
        .with_title(title.to_string())
        .with_description(description.to_string())
        .with_mime_type("text/markdown")
        .no_annotation()
}

fn make_resource_template(
    uri_template: &str,
    name: &str,
    title: &str,
    description: &str,
) -> ResourceTemplate {
    RawResourceTemplate::new(uri_template.to_string(), name.to_string())
        .with_title(title.to_string())
        .with_description(description.to_string())
        .with_mime_type("text/markdown")
        .no_annotation()
}

fn build_uri(base: &str, params: &[(&str, Option<String>)]) -> String {
    let mut url = Url::parse(base).expect("static tokenizor resource URI must parse");
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in params {
            if let Some(value) = value {
                query.append_pair(key, value);
            }
        }
    }
    url.to_string()
}

fn parse_resource_uri(uri: &str) -> Result<ResourceRequest, String> {
    let url = Url::parse(uri).map_err(|error| format!("invalid resource URI: {error}"))?;
    if url.scheme() != "tokenizor" {
        return Err(format!("unsupported resource scheme '{}'", url.scheme()));
    }

    let query: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();

    match (url.host_str(), url.path()) {
        (Some("repo"), "/health") => Ok(ResourceRequest::RepoHealth),
        (Some("repo"), "/outline") => Ok(ResourceRequest::RepoOutline),
        (Some("repo"), "/map") => Ok(ResourceRequest::RepoMap),
        (Some("repo"), "/changes/uncommitted") => Ok(ResourceRequest::RepoChangesUncommitted),
        (Some("file"), "/context") => Ok(ResourceRequest::FileContext {
            path: required_query(&query, "path")?,
            max_tokens: optional_query(&query, "max_tokens").transpose()?,
        }),
        (Some("file"), "/content") => Ok(ResourceRequest::FileContent {
            path: required_query(&query, "path")?,
            start_line: optional_query(&query, "start_line").transpose()?,
            end_line: optional_query(&query, "end_line").transpose()?,
        }),
        (Some("symbol"), "/detail") => Ok(ResourceRequest::SymbolDetail {
            path: required_query(&query, "path")?,
            name: required_query(&query, "name")?,
            kind: optional_text(&query, "kind"),
        }),
        (Some("symbol"), "/context") => Ok(ResourceRequest::SymbolContext {
            name: required_query(&query, "name")?,
            file: optional_text(&query, "file"),
        }),
        (host, path) => Err(format!(
            "unsupported Tokenizor resource target '{}{}'",
            host.unwrap_or("<none>"),
            path
        )),
    }
}

fn required_query(
    query: &std::collections::HashMap<String, String>,
    key: &str,
) -> Result<String, String> {
    query
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("resource URI missing required query parameter '{key}'"))
}

fn optional_text(query: &std::collections::HashMap<String, String>, key: &str) -> Option<String> {
    query
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn optional_query<T>(
    query: &std::collections::HashMap<String, String>,
    key: &str,
) -> Option<Result<T, String>>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    optional_text(query, key).map(|raw| {
        raw.parse::<T>()
            .map_err(|error| format!("invalid value for '{key}': {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use crate::domain::{LanguageId, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus};
    use crate::protocol::TokenizorServer;
    use crate::watcher::WatcherInfo;

    fn make_server() -> TokenizorServer {
        let symbol = SymbolRecord {
            name: "main".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (1, 3),
        };
        let file = IndexedFile {
            relative_path: "src/main.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/main.rs"),
            content: b"fn main() {}".to_vec(),
            symbols: vec![symbol],
            parse_status: ParseStatus::Parsed,
            byte_len: 12,
            content_hash: "test".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
        };
        let mut files = HashMap::new();
        files.insert("src/main.rs".to_string(), std::sync::Arc::new(file));
        let mut index = LiveIndex {
            files,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        TokenizorServer::new(
            crate::live_index::SharedIndexHandle::shared(index),
            "test_project".to_string(),
            Arc::new(Mutex::new(WatcherInfo::default())),
            None,
            None,
        )
    }

    #[test]
    fn test_resource_definitions_include_repo_surfaces() {
        let server = make_server();
        let resources = server.resource_definitions();
        let uris: Vec<&str> = resources
            .iter()
            .map(|resource| resource.uri.as_str())
            .collect();
        assert!(uris.contains(&REPO_HEALTH_URI));
        assert!(uris.contains(&REPO_MAP_URI));
    }

    #[test]
    fn test_resource_templates_include_file_and_symbol_templates() {
        let server = make_server();
        let templates = server.resource_template_definitions();
        let uris: Vec<&str> = templates
            .iter()
            .map(|template| template.uri_template.as_str())
            .collect();
        assert!(uris.contains(&FILE_CONTEXT_TEMPLATE));
        assert!(uris.contains(&SYMBOL_CONTEXT_TEMPLATE));
    }

    #[tokio::test]
    async fn test_read_static_repo_map_resource() {
        let server = make_server();
        let result = server
            .read_resource_uri(REPO_MAP_URI)
            .await
            .expect("read resource");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert!(text.contains("Index: 1 files, 1 symbols"));
    }

    #[tokio::test]
    async fn test_read_templated_file_context_resource() {
        let server = make_server();
        let uri = build_uri(
            "tokenizor://file/context",
            &[("path", Some("src/main.rs".to_string()))],
        );
        let result = server.read_resource_uri(&uri).await.expect("read resource");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert!(text.contains("src/main.rs"));
    }
}

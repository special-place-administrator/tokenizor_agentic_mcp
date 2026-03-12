use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{GetPromptResult, PromptMessage, PromptMessageRole};
use rmcp::{prompt, prompt_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::TokenizorServer;
use crate::protocol::resources::{
    file_context_resource, repo_changes_resource, repo_health_resource, repo_map_resource,
    repo_outline_resource,
};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CodeReviewPromptInput {
    pub path: Option<String>,
    pub focus: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ArchitectureMapPromptInput {
    pub area: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct FailureTriagePromptInput {
    pub symptom: String,
    pub path: Option<String>,
}

#[prompt_router(vis = "pub(crate)")]
impl TokenizorServer {
    #[prompt(
        name = "code-review",
        description = "Generate a code review plan using Tokenizor context surfaces."
    )]
    pub(crate) async fn code_review_prompt(
        &self,
        params: Parameters<CodeReviewPromptInput>,
    ) -> GetPromptResult {
        let mut messages = vec![
            PromptMessage::new_text(
                PromptMessageRole::User,
                build_code_review_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_health_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_map_resource()),
        ];

        if let Some(path) = params.0.path.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                PromptMessageRole::User,
                file_context_resource(path, Some(200)),
            ));
        }

        GetPromptResult::new(messages)
            .with_description("Review code using Tokenizor resources and targeted tools.")
    }

    #[prompt(
        name = "architecture-map",
        description = "Generate an architecture mapping plan using Tokenizor repo context."
    )]
    pub(crate) async fn architecture_map_prompt(
        &self,
        params: Parameters<ArchitectureMapPromptInput>,
    ) -> GetPromptResult {
        let mut messages = vec![
            PromptMessage::new_text(
                PromptMessageRole::User,
                build_architecture_map_instructions(&self.project_name, params.0.area.as_deref()),
            ),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_map_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_outline_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_health_resource()),
        ];

        if let Some(area) = params.0.area.as_deref() {
            messages.push(PromptMessage::new_text(
                PromptMessageRole::User,
                format!("Prioritize the area or subsystem named '{area}' if it exists."),
            ));
        }

        GetPromptResult::new(messages).with_description(
            "Map repository architecture using Tokenizor resources and cross-reference tools.",
        )
    }

    #[prompt(
        name = "failure-triage",
        description = "Generate a debugging and failure-triage plan using Tokenizor state."
    )]
    pub(crate) async fn failure_triage_prompt(
        &self,
        params: Parameters<FailureTriagePromptInput>,
    ) -> GetPromptResult {
        let mut messages = vec![
            PromptMessage::new_text(
                PromptMessageRole::User,
                build_failure_triage_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_health_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_changes_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_map_resource()),
        ];

        if let Some(path) = params.0.path.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                PromptMessageRole::User,
                file_context_resource(path, Some(200)),
            ));
        }

        GetPromptResult::new(messages).with_description(
            "Triage failures using Tokenizor runtime health, changed files, and local context.",
        )
    }
}

fn build_code_review_instructions(project_name: &str, input: &CodeReviewPromptInput) -> String {
    let mut text = format!(
        "Review code in project '{project_name}'. Focus first on correctness, regressions, edge cases, and missing tests."
    );
    if let Some(path) = input.path.as_deref() {
        text.push_str(&format!(" Start with the target path '{path}'."));
    } else {
        text.push_str(" Start from the repository-level context and narrow to the risky areas.");
    }
    if let Some(focus) = input.focus.as_deref() {
        text.push_str(&format!(" Pay special attention to: {focus}."));
    }
    text.push_str(" Use Tokenizor resources for orientation, then use targeted tools for proof.");
    text
}

fn build_architecture_map_instructions(project_name: &str, area: Option<&str>) -> String {
    let mut text = format!(
        "Map the architecture of project '{project_name}'. Identify the main subsystems, ownership boundaries, data flow, and the important symbols or files to inspect next."
    );
    if let Some(area) = area {
        text.push_str(&format!(" Emphasize the area '{area}'."));
    }
    text.push_str(" Prefer repository-level resources first, then descend into file or symbol detail where needed.");
    text
}

fn build_failure_triage_instructions(
    project_name: &str,
    input: &FailureTriagePromptInput,
) -> String {
    let mut text = format!(
        "Triage a problem in project '{project_name}'. Symptom: {}. Build a root-cause-first investigation plan using current health, changed files, and the smallest set of targeted lookups needed.",
        input.symptom
    );
    if let Some(path) = input.path.as_deref() {
        text.push_str(&format!(" Treat '{path}' as the initial hotspot."));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use crate::live_index::store::{CircuitBreakerState, LiveIndex};
    use crate::watcher::WatcherInfo;

    use crate::protocol::resources::REPO_HEALTH_URI;

    fn make_server() -> TokenizorServer {
        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(1),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
        };

        TokenizorServer::new(
            crate::live_index::SharedIndexHandle::shared(index),
            "prompt_project".to_string(),
            Arc::new(Mutex::new(WatcherInfo::default())),
            None,
            None,
        )
    }

    #[test]
    fn test_prompt_router_lists_expected_prompts() {
        let server = make_server();
        let prompts = server.prompt_router.list_all();
        let names: Vec<&str> = prompts.iter().map(|prompt| prompt.name.as_str()).collect();
        assert!(names.contains(&"code-review"));
        assert!(names.contains(&"architecture-map"));
        assert!(names.contains(&"failure-triage"));
    }

    #[tokio::test]
    async fn test_code_review_prompt_includes_resource_links() {
        let server = make_server();
        let result = server
            .code_review_prompt(Parameters(CodeReviewPromptInput {
                path: Some("src/lib.rs".to_string()),
                focus: Some("dependency risks".to_string()),
            }))
            .await;

        assert!(
            result.messages.iter().any(|message| matches!(
                &message.content,
                rmcp::model::PromptMessageContent::ResourceLink { link }
                    if link.uri == REPO_HEALTH_URI
            )),
            "code-review prompt should link repo health"
        );
        assert!(
            result.messages.iter().any(|message| matches!(
                &message.content,
                rmcp::model::PromptMessageContent::ResourceLink { link }
                    if link.uri.contains("tokenizor://file/context")
            )),
            "code-review prompt should link file context"
        );
    }
}

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{GitLabClient, encode_segment, env_flag};

type ToolResult = Result<CallToolResult, McpError>;

#[derive(Clone)]
pub struct GitLabMcp {
    client: GitLabClient,
    allow_write: bool,
}

impl GitLabMcp {
    pub fn new(client: GitLabClient) -> Self {
        Self {
            client,
            allow_write: env_flag("GITLAB_ALLOW_WRITE"),
        }
    }

    #[cfg(test)]
    fn with_write(client: GitLabClient, allow_write: bool) -> Self {
        Self {
            client,
            allow_write,
        }
    }

    async fn get(&self, path: String, query: Vec<(String, String)>) -> ToolResult {
        response(self.client.get(&path, &query).await)
    }

    async fn write(&self, path: String, body: Value) -> ToolResult {
        if !self.allow_write {
            return tool_error(
                "write tools are disabled; set GITLAB_ALLOW_WRITE=true to enable them",
            );
        }
        response(self.client.post(&path, body).await)
    }

    async fn optional_get(
        &self,
        enabled: bool,
        path: String,
        query: Vec<(String, String)>,
    ) -> anyhow::Result<Option<Value>> {
        if enabled {
            self.client.get(&path, &query).await.map(Some)
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ProjectId {
    #[schemars(
        description = "Numeric project ID or full namespace path, for example team/service"
    )]
    project: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchProjects {
    #[schemars(description = "Text contained in the project path or name")]
    search: Option<String>,
    #[schemars(description = "Only return projects the authenticated user belongs to")]
    membership: Option<bool>,
    #[schemars(description = "Page number, starting at 1")]
    page: Option<u32>,
    #[schemars(description = "Items per page, 1 through 100")]
    per_page: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ProjectList {
    project: String,
    #[schemars(description = "State filter such as opened, closed, merged, or all")]
    state: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ProjectItem {
    project: String,
    #[schemars(description = "Project-local IID, not the global database ID")]
    iid: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RepositoryTree {
    project: String,
    #[schemars(description = "Directory path; omit for repository root")]
    path: Option<String>,
    #[schemars(description = "Branch, tag, or commit SHA; omit for the default branch")]
    git_ref: Option<String>,
    #[schemars(description = "Whether to list the tree recursively")]
    recursive: Option<bool>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RepositoryFile {
    project: String,
    #[schemars(description = "Repository-relative file path")]
    file_path: String,
    #[schemars(description = "Branch, tag, or commit SHA")]
    git_ref: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PipelineList {
    project: String,
    #[schemars(description = "Filter by branch or tag")]
    git_ref: Option<String>,
    #[schemars(
        description = "Pipeline status such as running, pending, success, failed, or canceled"
    )]
    status: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PipelineJobs {
    project: String,
    pipeline_id: u64,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateIssue {
    project: String,
    title: String,
    description: Option<String>,
    #[schemars(description = "Comma-separated label names")]
    labels: Option<String>,
    #[schemars(description = "User IDs to assign")]
    assignee_ids: Option<Vec<u64>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateMergeRequest {
    project: String,
    source_branch: String,
    target_branch: String,
    title: String,
    description: Option<String>,
    #[schemars(description = "Create the merge request as a draft")]
    draft: Option<bool>,
    remove_source_branch: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AddNote {
    project: String,
    iid: u64,
    body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ReviewMergeRequestContext {
    project: String,
    #[schemars(description = "Project-local merge request IID, for example 128 for !128")]
    iid: u64,
    #[schemars(description = "Include file diffs; defaults to true")]
    include_diffs: Option<bool>,
    #[schemars(description = "Include commits; defaults to true")]
    include_commits: Option<bool>,
    #[schemars(description = "Include discussions; defaults to true")]
    include_discussions: Option<bool>,
    #[schemars(description = "Include related issues; defaults to true")]
    include_related_issues: Option<bool>,
    #[schemars(description = "Include merge request pipelines; defaults to true")]
    include_pipelines: Option<bool>,
    #[schemars(description = "Include approval status; defaults to true")]
    include_approvals: Option<bool>,
    #[schemars(
        description = "Maximum items requested per included collection, 1 through 100; defaults to 30"
    )]
    per_page: Option<u32>,
}

#[tool_router]
impl GitLabMcp {
    #[tool(description = "Get the authenticated GitLab user")]
    async fn get_current_user(&self) -> ToolResult {
        self.get("/user".into(), vec![]).await
    }

    #[tool(description = "Search or list GitLab projects visible to the authenticated user")]
    async fn list_projects(&self, Parameters(p): Parameters<SearchProjects>) -> ToolResult {
        let mut query = pagination(p.page, p.per_page)?;
        optional(&mut query, "search", p.search);
        if let Some(value) = p.membership {
            query.push(("membership".into(), value.to_string()));
        }
        query.push(("order_by".into(), "last_activity_at".into()));
        query.push(("sort".into(), "desc".into()));
        self.get("/projects".into(), query).await
    }

    #[tool(description = "Get one GitLab project by numeric ID or namespace path")]
    async fn get_project(&self, Parameters(p): Parameters<ProjectId>) -> ToolResult {
        self.get(format!("/projects/{}", encode_segment(&p.project)), vec![])
            .await
    }

    #[tool(description = "List merge requests in a project")]
    async fn list_merge_requests(&self, Parameters(p): Parameters<ProjectList>) -> ToolResult {
        let mut query = pagination(p.page, p.per_page)?;
        optional(&mut query, "state", p.state);
        query.push(("scope".into(), "all".into()));
        self.get(
            format!("/projects/{}/merge_requests", encode_segment(&p.project)),
            query,
        )
        .await
    }

    #[tool(description = "Get one project merge request")]
    async fn get_merge_request(&self, Parameters(p): Parameters<ProjectItem>) -> ToolResult {
        self.get(
            format!(
                "/projects/{}/merge_requests/{}",
                encode_segment(&p.project),
                p.iid
            ),
            vec![],
        )
        .await
    }

    #[tool(description = "List file diffs for one merge request")]
    async fn list_merge_request_diffs(&self, Parameters(p): Parameters<ProjectItem>) -> ToolResult {
        self.get(
            format!(
                "/projects/{}/merge_requests/{}/diffs",
                encode_segment(&p.project),
                p.iid
            ),
            vec![("per_page".into(), "100".into())],
        )
        .await
    }

    #[tool(
        description = "Aggregate the information needed to review one merge request: metadata, diffs, commits, discussions, related issues, pipelines, approvals, and a compact summary"
    )]
    async fn review_merge_request_context(
        &self,
        Parameters(p): Parameters<ReviewMergeRequestContext>,
    ) -> ToolResult {
        let per_page = p.per_page.unwrap_or(30);
        if !(1..=100).contains(&per_page) {
            return Err(invalid_params("per_page must be between 1 and 100"));
        }

        let project = encode_segment(&p.project);
        let base = format!("/projects/{project}/merge_requests/{}", p.iid);
        let merge_request = match self
            .client
            .get(
                &base,
                &[
                    ("include_diverged_commits_count".into(), "true".into()),
                    ("include_rebase_in_progress".into(), "true".into()),
                ],
            )
            .await
        {
            Ok(value) => value,
            Err(error) => return tool_error(error.to_string()),
        };

        let collection_query = vec![
            ("page".into(), "1".into()),
            ("per_page".into(), per_page.to_string()),
        ];
        let (diffs, commits, discussions, related_issues, pipelines, approvals) = tokio::join!(
            self.optional_get(
                p.include_diffs.unwrap_or(true),
                format!("{base}/diffs"),
                collection_query.clone(),
            ),
            self.optional_get(
                p.include_commits.unwrap_or(true),
                format!("{base}/commits"),
                collection_query.clone(),
            ),
            self.optional_get(
                p.include_discussions.unwrap_or(true),
                format!("{base}/discussions"),
                collection_query.clone(),
            ),
            self.optional_get(
                p.include_related_issues.unwrap_or(true),
                format!("{base}/related_issues"),
                vec![],
            ),
            self.optional_get(
                p.include_pipelines.unwrap_or(true),
                format!("{base}/pipelines"),
                collection_query,
            ),
            self.optional_get(
                p.include_approvals.unwrap_or(true),
                format!("{base}/approvals"),
                vec![],
            ),
        );

        let mut context = serde_json::Map::new();
        context.insert("merge_request".into(), merge_request);
        let mut warnings = Vec::new();
        insert_optional_section(&mut context, &mut warnings, "diffs", diffs);
        insert_optional_section(&mut context, &mut warnings, "commits", commits);
        insert_optional_section(&mut context, &mut warnings, "discussions", discussions);
        insert_optional_section(
            &mut context,
            &mut warnings,
            "related_issues",
            related_issues,
        );
        insert_optional_section(&mut context, &mut warnings, "pipelines", pipelines);
        insert_optional_section(&mut context, &mut warnings, "approvals", approvals);

        context.insert("summary".into(), review_summary(&context));
        context.insert("partial".into(), Value::Bool(!warnings.is_empty()));
        context.insert(
            "warnings".into(),
            Value::Array(warnings.into_iter().map(Value::String).collect()),
        );

        response_limited(Ok(Value::Object(context)), self.client.max_response_bytes())
    }

    #[tool(description = "List issues in a project")]
    async fn list_issues(&self, Parameters(p): Parameters<ProjectList>) -> ToolResult {
        let mut query = pagination(p.page, p.per_page)?;
        optional(&mut query, "state", p.state);
        query.push(("scope".into(), "all".into()));
        self.get(
            format!("/projects/{}/issues", encode_segment(&p.project)),
            query,
        )
        .await
    }

    #[tool(description = "Get one project issue")]
    async fn get_issue(&self, Parameters(p): Parameters<ProjectItem>) -> ToolResult {
        self.get(
            format!("/projects/{}/issues/{}", encode_segment(&p.project), p.iid),
            vec![],
        )
        .await
    }

    #[tool(description = "List files and directories in a repository tree")]
    async fn list_repository_tree(&self, Parameters(p): Parameters<RepositoryTree>) -> ToolResult {
        let mut query = pagination(p.page, p.per_page)?;
        optional(&mut query, "path", p.path);
        optional(&mut query, "ref", p.git_ref);
        if let Some(value) = p.recursive {
            query.push(("recursive".into(), value.to_string()));
        }
        self.get(
            format!("/projects/{}/repository/tree", encode_segment(&p.project)),
            query,
        )
        .await
    }

    #[tool(
        description = "Get a repository file. GitLab returns its content base64-encoded in the content field"
    )]
    async fn get_repository_file(&self, Parameters(p): Parameters<RepositoryFile>) -> ToolResult {
        self.get(
            format!(
                "/projects/{}/repository/files/{}",
                encode_segment(&p.project),
                encode_segment(&p.file_path)
            ),
            vec![("ref".into(), p.git_ref)],
        )
        .await
    }

    #[tool(description = "List pipelines in a project")]
    async fn list_pipelines(&self, Parameters(p): Parameters<PipelineList>) -> ToolResult {
        let mut query = pagination(p.page, p.per_page)?;
        optional(&mut query, "ref", p.git_ref);
        optional(&mut query, "status", p.status);
        self.get(
            format!("/projects/{}/pipelines", encode_segment(&p.project)),
            query,
        )
        .await
    }

    #[tool(description = "List jobs in a pipeline")]
    async fn list_pipeline_jobs(&self, Parameters(p): Parameters<PipelineJobs>) -> ToolResult {
        let query = pagination(p.page, p.per_page)?;
        self.get(
            format!(
                "/projects/{}/pipelines/{}/jobs",
                encode_segment(&p.project),
                p.pipeline_id
            ),
            query,
        )
        .await
    }

    #[tool(description = "Create an issue. Requires GITLAB_ALLOW_WRITE=true")]
    async fn create_issue(&self, Parameters(p): Parameters<CreateIssue>) -> ToolResult {
        self.write(
            format!("/projects/{}/issues", encode_segment(&p.project)),
            json!({"title": p.title, "description": p.description, "labels": p.labels, "assignee_ids": p.assignee_ids}),
        ).await
    }

    #[tool(description = "Create a merge request. Requires GITLAB_ALLOW_WRITE=true")]
    async fn create_merge_request(
        &self,
        Parameters(p): Parameters<CreateMergeRequest>,
    ) -> ToolResult {
        let title = if p.draft.unwrap_or(false) && !p.title.starts_with("Draft:") {
            format!("Draft: {}", p.title)
        } else {
            p.title
        };
        self.write(
            format!("/projects/{}/merge_requests", encode_segment(&p.project)),
            json!({"source_branch": p.source_branch, "target_branch": p.target_branch, "title": title, "description": p.description, "remove_source_branch": p.remove_source_branch}),
        ).await
    }

    #[tool(description = "Add a comment to an issue. Requires GITLAB_ALLOW_WRITE=true")]
    async fn add_issue_note(&self, Parameters(p): Parameters<AddNote>) -> ToolResult {
        self.write(
            format!(
                "/projects/{}/issues/{}/notes",
                encode_segment(&p.project),
                p.iid
            ),
            json!({"body": p.body}),
        )
        .await
    }

    #[tool(
        description = "Add a general comment to a merge request. Requires GITLAB_ALLOW_WRITE=true"
    )]
    async fn add_merge_request_note(&self, Parameters(p): Parameters<AddNote>) -> ToolResult {
        self.write(
            format!(
                "/projects/{}/merge_requests/{}/notes",
                encode_segment(&p.project),
                p.iid
            ),
            json!({"body": p.body}),
        )
        .await
    }
}

#[tool_handler(
    name = "gitlab-mcp",
    version = "0.1.0",
    instructions = "Access a self-managed GitLab instance. Prefer read tools first. Write tools require explicit server-side opt-in."
)]
impl ServerHandler for GitLabMcp {}

fn pagination(page: Option<u32>, per_page: Option<u32>) -> Result<Vec<(String, String)>, McpError> {
    let page = page.unwrap_or(1);
    let per_page = per_page.unwrap_or(30);
    if page == 0 {
        return Err(invalid_params("page must be at least 1"));
    }
    if !(1..=100).contains(&per_page) {
        return Err(invalid_params("per_page must be between 1 and 100"));
    }
    Ok(vec![
        ("page".into(), page.to_string()),
        ("per_page".into(), per_page.to_string()),
    ])
}

fn optional(query: &mut Vec<(String, String)>, name: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        query.push((name.into(), value));
    }
}

fn response(result: anyhow::Result<Value>) -> ToolResult {
    match result {
        Ok(value) => Ok(CallToolResult::success(vec![ContentBlock::text(
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
        )])),
        Err(error) => tool_error(error.to_string()),
    }
}

fn response_limited(result: anyhow::Result<Value>, max_bytes: usize) -> ToolResult {
    match result {
        Ok(value) => {
            let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
            if text.len() > max_bytes {
                return tool_error(format!(
                    "aggregated review context is {} bytes, exceeding the configured {max_bytes} byte limit; disable unneeded include_* sections, lower per_page, or increase GITLAB_MAX_RESPONSE_BYTES",
                    text.len()
                ));
            }
            Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
        }
        Err(error) => tool_error(error.to_string()),
    }
}

fn insert_optional_section(
    context: &mut serde_json::Map<String, Value>,
    warnings: &mut Vec<String>,
    name: &str,
    result: anyhow::Result<Option<Value>>,
) {
    match result {
        Ok(Some(value)) => {
            context.insert(name.into(), value);
        }
        Ok(None) => {}
        Err(error) => warnings.push(format!("{name}: {error}")),
    }
}

fn review_summary(context: &serde_json::Map<String, Value>) -> Value {
    let count = |name: &str| {
        context
            .get(name)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    let unresolved_discussions = context
        .get("discussions")
        .and_then(Value::as_array)
        .map(|discussions| {
            discussions
                .iter()
                .filter(|discussion| {
                    discussion
                        .get("notes")
                        .and_then(Value::as_array)
                        .is_some_and(|notes| {
                            notes.iter().any(|note| {
                                note.get("resolvable").and_then(Value::as_bool) == Some(true)
                                    && note.get("resolved").and_then(Value::as_bool) != Some(true)
                            })
                        })
                })
                .count()
        })
        .unwrap_or(0);

    json!({
        "files_changed_returned": count("diffs"),
        "commits_returned": count("commits"),
        "discussions_returned": count("discussions"),
        "unresolved_discussions": unresolved_discussions,
        "related_issues_returned": count("related_issues"),
        "pipelines_returned": count("pipelines"),
        "approved": context.get("approvals").and_then(|value| value.get("approved")).and_then(Value::as_bool),
    })
}

fn tool_error(message: impl Into<String>) -> ToolResult {
    Ok(CallToolResult::error(vec![ContentBlock::text(
        message.into(),
    )]))
}

fn invalid_params(message: impl Into<String>) -> McpError {
    McpError::invalid_params(message.into(), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn project_path_is_encoded() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/api/v4/projects/team%2Fservice");
            then.status(200).json_body(json!({"id": 42}));
        });
        let mcp = GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 1024), false);
        let result = mcp
            .get_project(Parameters(ProjectId {
                project: "team/service".into(),
            }))
            .await
            .unwrap();
        mock.assert();
        assert_ne!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn write_tools_are_disabled_by_default() {
        let server = MockServer::start();
        let mcp = GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 1024), false);
        let result = mcp
            .create_issue(Parameters(CreateIssue {
                project: "team/service".into(),
                title: "Example".into(),
                description: None,
                labels: None,
                assignee_ids: None,
            }))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn review_context_aggregates_optional_sections() {
        let server = MockServer::start();
        let base_path = "/api/v4/projects/team%2Fservice/merge_requests/128";
        let merge_request = server.mock(|when, then| {
            when.method(GET)
                .path(base_path)
                .query_param("include_diverged_commits_count", "true")
                .query_param("include_rebase_in_progress", "true");
            then.status(200)
                .json_body(json!({"iid": 128, "title": "Retry payments"}));
        });
        let diffs = server.mock(|when, then| {
            when.method(GET)
                .path(format!("{base_path}/diffs"))
                .query_param("page", "1")
                .query_param("per_page", "20");
            then.status(200)
                .json_body(json!([{"new_path": "src/main.rs", "diff": "@@"}]));
        });
        let commits = server.mock(|when, then| {
            when.method(GET).path(format!("{base_path}/commits"));
            then.status(200).json_body(json!([{"id": "abc"}]));
        });
        let discussions = server.mock(|when, then| {
            when.method(GET).path(format!("{base_path}/discussions"));
            then.status(200).json_body(json!([{
                "id": "discussion-1",
                "notes": [{"resolvable": true, "resolved": false}]
            }]));
        });
        let related_issues = server.mock(|when, then| {
            when.method(GET).path(format!("{base_path}/related_issues"));
            then.status(200).json_body(json!([{"iid": 25}]));
        });
        let pipelines = server.mock(|when, then| {
            when.method(GET).path(format!("{base_path}/pipelines"));
            then.status(200)
                .json_body(json!([{"id": 987, "status": "success"}]));
        });
        let approvals = server.mock(|when, then| {
            when.method(GET).path(format!("{base_path}/approvals"));
            then.status(200).json_body(json!({"approved": true}));
        });

        let mcp =
            GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 64 * 1024), false);
        let result = mcp
            .review_merge_request_context(Parameters(ReviewMergeRequestContext {
                project: "team/service".into(),
                iid: 128,
                include_diffs: None,
                include_commits: None,
                include_discussions: None,
                include_related_issues: None,
                include_pipelines: None,
                include_approvals: None,
                per_page: Some(20),
            }))
            .await
            .unwrap();

        merge_request.assert();
        diffs.assert();
        commits.assert();
        discussions.assert();
        related_issues.assert();
        pipelines.assert();
        approvals.assert();
        assert_ne!(result.is_error, Some(true));
        let result = serde_json::to_value(result).unwrap();
        let context: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(context["partial"], false);
        assert_eq!(context["summary"]["files_changed_returned"], 1);
        assert_eq!(context["summary"]["unresolved_discussions"], 1);
        assert_eq!(context["summary"]["approved"], true);
    }

    #[tokio::test]
    async fn review_context_keeps_core_data_when_optional_endpoint_fails() {
        let server = MockServer::start();
        let base_path = "/api/v4/projects/team%2Fservice/merge_requests/128";
        server.mock(|when, then| {
            when.method(GET).path(base_path);
            then.status(200).json_body(json!({"iid": 128}));
        });
        server.mock(|when, then| {
            when.method(GET).path(format!("{base_path}/approvals"));
            then.status(404)
                .json_body(json!({"message": "404 Not Found"}));
        });

        let mcp =
            GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 64 * 1024), false);
        let result = mcp
            .review_merge_request_context(Parameters(ReviewMergeRequestContext {
                project: "team/service".into(),
                iid: 128,
                include_diffs: Some(false),
                include_commits: Some(false),
                include_discussions: Some(false),
                include_related_issues: Some(false),
                include_pipelines: Some(false),
                include_approvals: Some(true),
                per_page: None,
            }))
            .await
            .unwrap();

        assert_ne!(result.is_error, Some(true));
        let result = serde_json::to_value(result).unwrap();
        let context: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(context["merge_request"]["iid"], 128);
        assert_eq!(context["partial"], true);
        assert!(
            context["warnings"][0]
                .as_str()
                .unwrap()
                .starts_with("approvals:")
        );
    }

    #[test]
    fn rejects_invalid_pagination() {
        assert!(pagination(Some(0), Some(30)).is_err());
        assert!(pagination(Some(1), Some(101)).is_err());
    }
}

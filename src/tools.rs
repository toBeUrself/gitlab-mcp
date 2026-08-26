use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

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

    async fn update(&self, path: String, body: Value) -> ToolResult {
        if !self.allow_write {
            return tool_error(
                "write tools are disabled; set GITLAB_ALLOW_WRITE=true to enable them",
            );
        }
        response(self.client.put(&path, body).await)
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
struct GroupProjects {
    #[schemars(description = "Numeric group ID or full group path, for example platform/backend")]
    group: String,
    #[schemars(description = "Text contained in the project path or name")]
    search: Option<String>,
    #[schemars(description = "Include projects in descendant subgroups; defaults to false")]
    include_subgroups: Option<bool>,
    #[schemars(description = "Include projects shared to the group; defaults to true")]
    with_shared: Option<bool>,
    #[schemars(description = "Filter by archived state")]
    archived: Option<bool>,
    page: Option<u32>,
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
struct BranchList {
    project: String,
    #[schemars(description = "Branch name substring; supports ^prefix and suffix$ forms")]
    search: Option<String>,
    #[schemars(description = "RE2 regular expression; cannot be combined with search")]
    regex: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct BranchRef {
    project: String,
    branch: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CompareRefs {
    project: String,
    #[schemars(description = "Source branch, tag, or commit SHA")]
    from: String,
    #[schemars(description = "Target branch, tag, or commit SHA")]
    to: String,
    #[schemars(
        description = "Use direct from..to comparison instead of merge-base from...to; defaults to false"
    )]
    straight: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CommitList {
    project: String,
    #[schemars(description = "Branch, tag, SHA, or revision range; omit for the default branch")]
    git_ref: Option<String>,
    #[schemars(description = "Only commits affecting this repository path")]
    path: Option<String>,
    #[schemars(description = "Only commits on or after this ISO 8601 timestamp")]
    since: Option<String>,
    #[schemars(description = "Only commits on or before this ISO 8601 timestamp")]
    until: Option<String>,
    #[schemars(description = "Follow only first parents at merge commits")]
    first_parent: Option<bool>,
    #[schemars(description = "Include added/deleted/total statistics for each commit")]
    with_stats: Option<bool>,
    #[schemars(description = "Commit ordering: default or topo")]
    order: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CommitRef {
    project: String,
    #[schemars(description = "Commit SHA, branch name, or tag name")]
    sha: String,
    #[schemars(description = "Include commit statistics; GitLab defaults to true")]
    stats: Option<bool>,
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
struct PipelineId {
    project: String,
    pipeline_id: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct JobTrace {
    project: String,
    job_id: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateBranch {
    project: String,
    #[schemars(description = "Name of the new branch, for example feature/retry-payment")]
    branch: String,
    #[schemars(description = "Existing branch, tag, or commit SHA used as the branch start point")]
    git_ref: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateTag {
    project: String,
    tag_name: String,
    #[schemars(description = "Commit SHA, branch name, or existing tag used as the tag target")]
    git_ref: String,
    #[schemars(description = "Annotated tag message; omit to create a lightweight tag")]
    message: Option<String>,
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
    #[schemars(
        description = "Squash all commits into one commit when merging; project settings can override this value"
    )]
    squash: Option<bool>,
    #[schemars(description = "Remove the source branch after the merge request is merged")]
    remove_source_branch: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct UpdateMergeRequest {
    project: String,
    #[schemars(description = "Project-local merge request IID, for example 128 for !128")]
    iid: u64,
    title: Option<String>,
    description: Option<String>,
    #[schemars(
        description = "Mark or unmark the merge request as Draft by updating its title prefix"
    )]
    draft: Option<bool>,
    #[schemars(
        description = "GitLab user IDs to set as reviewers; an empty array clears reviewers"
    )]
    reviewer_ids: Option<Vec<u64>>,
    #[schemars(
        description = "GitLab user IDs to set as assignees; an empty array clears assignees"
    )]
    assignee_ids: Option<Vec<u64>>,
    #[schemars(
        description = "Squash all commits into one commit when merging; project settings can override this value"
    )]
    squash: Option<bool>,
    #[schemars(description = "Remove the source branch after the merge request is merged")]
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

    #[tool(description = "List projects that belong to a GitLab group")]
    async fn list_group_projects(&self, Parameters(p): Parameters<GroupProjects>) -> ToolResult {
        let mut query = pagination(p.page, p.per_page)?;
        optional(&mut query, "search", p.search);
        optional_bool(&mut query, "include_subgroups", p.include_subgroups);
        optional_bool(&mut query, "with_shared", p.with_shared);
        optional_bool(&mut query, "archived", p.archived);
        query.push(("order_by".into(), "last_activity_at".into()));
        query.push(("sort".into(), "desc".into()));
        self.get(
            format!("/groups/{}/projects", encode_segment(&p.group)),
            query,
        )
        .await
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

    #[tool(
        description = "Update a merge request title, description, Draft state, reviewers, assignees, squash option, or source branch removal option. Requires GITLAB_ALLOW_WRITE=true"
    )]
    async fn update_merge_request(
        &self,
        Parameters(p): Parameters<UpdateMergeRequest>,
    ) -> ToolResult {
        if !self.allow_write {
            return tool_error(
                "write tools are disabled; set GITLAB_ALLOW_WRITE=true to enable them",
            );
        }
        if p.title.is_none()
            && p.description.is_none()
            && p.draft.is_none()
            && p.reviewer_ids.is_none()
            && p.assignee_ids.is_none()
            && p.squash.is_none()
            && p.remove_source_branch.is_none()
        {
            return Err(invalid_params(
                "at least one of title, description, draft, reviewer_ids, assignee_ids, squash, or remove_source_branch must be provided",
            ));
        }

        let path = format!(
            "/projects/{}/merge_requests/{}",
            encode_segment(&p.project),
            p.iid
        );
        let mut title = p.title;
        if let Some(draft) = p.draft {
            if title.is_none() {
                let current = match self.client.get(&path, &[]).await {
                    Ok(value) => value,
                    Err(error) => return tool_error(error.to_string()),
                };
                title = current
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if title.is_none() {
                    return tool_error("GitLab merge request response is missing title");
                }
            }
            title = title.map(|value| set_draft_title(&value, draft));
        }

        let mut body = Map::new();
        insert_json(&mut body, "title", title);
        insert_json(&mut body, "description", p.description);
        insert_json(&mut body, "reviewer_ids", p.reviewer_ids);
        insert_json(&mut body, "assignee_ids", p.assignee_ids);
        insert_json(&mut body, "squash", p.squash);
        insert_json(&mut body, "remove_source_branch", p.remove_source_branch);
        self.update(path, Value::Object(body)).await
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

    #[tool(
        description = "List repository branches, optionally filtering by search text or RE2 regex"
    )]
    async fn list_branches(&self, Parameters(p): Parameters<BranchList>) -> ToolResult {
        if p.search.is_some() && p.regex.is_some() {
            return Err(invalid_params("search and regex cannot be used together"));
        }
        let mut query = pagination(p.page, p.per_page)?;
        optional(&mut query, "search", p.search);
        optional(&mut query, "regex", p.regex);
        self.get(
            format!(
                "/projects/{}/repository/branches",
                encode_segment(&p.project)
            ),
            query,
        )
        .await
    }

    #[tool(description = "Get one repository branch and its latest commit and protection status")]
    async fn get_branch(&self, Parameters(p): Parameters<BranchRef>) -> ToolResult {
        self.get(
            format!(
                "/projects/{}/repository/branches/{}",
                encode_segment(&p.project),
                encode_segment(&p.branch)
            ),
            vec![],
        )
        .await
    }

    #[tool(
        description = "Compare two branches, tags, or commits and return commits and file diffs"
    )]
    async fn compare_refs(&self, Parameters(p): Parameters<CompareRefs>) -> ToolResult {
        let mut query = vec![("from".into(), p.from), ("to".into(), p.to)];
        optional_bool(&mut query, "straight", p.straight);
        self.get(
            format!(
                "/projects/{}/repository/compare",
                encode_segment(&p.project)
            ),
            query,
        )
        .await
    }

    #[tool(
        description = "List repository commits for a branch, tag, SHA, revision range, or default branch"
    )]
    async fn list_commits(&self, Parameters(p): Parameters<CommitList>) -> ToolResult {
        if let Some(order) = p.order.as_deref()
            && !matches!(order, "default" | "topo")
        {
            return Err(invalid_params("order must be default or topo"));
        }
        let mut query = pagination(p.page, p.per_page)?;
        optional(&mut query, "ref_name", p.git_ref);
        optional(&mut query, "path", p.path);
        optional(&mut query, "since", p.since);
        optional(&mut query, "until", p.until);
        optional(&mut query, "order", p.order);
        optional_bool(&mut query, "first_parent", p.first_parent);
        optional_bool(&mut query, "with_stats", p.with_stats);
        self.get(
            format!(
                "/projects/{}/repository/commits",
                encode_segment(&p.project)
            ),
            query,
        )
        .await
    }

    #[tool(description = "Get one commit by SHA, branch name, or tag name")]
    async fn get_commit(&self, Parameters(p): Parameters<CommitRef>) -> ToolResult {
        let mut query = Vec::new();
        optional_bool(&mut query, "stats", p.stats);
        self.get(
            format!(
                "/projects/{}/repository/commits/{}",
                encode_segment(&p.project),
                encode_segment(&p.sha)
            ),
            query,
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

    #[tool(description = "Get one GitLab pipeline by project and pipeline ID")]
    async fn get_pipeline(&self, Parameters(p): Parameters<PipelineId>) -> ToolResult {
        self.get(
            format!(
                "/projects/{}/pipelines/{}",
                encode_segment(&p.project),
                p.pipeline_id
            ),
            vec![],
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

    #[tool(
        description = "Get the complete text trace for one GitLab CI job; response size limits still apply"
    )]
    async fn get_job_trace(&self, Parameters(p): Parameters<JobTrace>) -> ToolResult {
        self.get(
            format!(
                "/projects/{}/jobs/{}/trace",
                encode_segment(&p.project),
                p.job_id
            ),
            vec![],
        )
        .await
    }

    #[tool(
        description = "Create a repository branch from an existing branch, tag, or commit SHA. Requires GITLAB_ALLOW_WRITE=true"
    )]
    async fn create_branch(&self, Parameters(p): Parameters<CreateBranch>) -> ToolResult {
        self.write(
            format!(
                "/projects/{}/repository/branches",
                encode_segment(&p.project)
            ),
            json!({"branch": p.branch, "ref": p.git_ref}),
        )
        .await
    }

    #[tool(
        description = "Create a lightweight or annotated repository tag. Requires GITLAB_ALLOW_WRITE=true"
    )]
    async fn create_tag(&self, Parameters(p): Parameters<CreateTag>) -> ToolResult {
        let mut body = Map::new();
        body.insert("tag_name".into(), Value::String(p.tag_name));
        body.insert("ref".into(), Value::String(p.git_ref));
        insert_json(&mut body, "message", p.message);
        self.write(
            format!("/projects/{}/repository/tags", encode_segment(&p.project)),
            Value::Object(body),
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
            json!({"source_branch": p.source_branch, "target_branch": p.target_branch, "title": title, "description": p.description, "squash": p.squash, "remove_source_branch": p.remove_source_branch}),
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
    version = "0.4.0",
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

fn optional_bool(query: &mut Vec<(String, String)>, name: &str, value: Option<bool>) {
    if let Some(value) = value {
        query.push((name.into(), value.to_string()));
    }
}

fn insert_json<T: serde::Serialize>(body: &mut Map<String, Value>, name: &str, value: Option<T>) {
    if let Some(value) = value {
        body.insert(name.into(), json!(value));
    }
}

fn set_draft_title(title: &str, draft: bool) -> String {
    let title = strip_draft_prefix(title.trim_start());
    if draft {
        format!("Draft: {title}")
    } else {
        title.to_owned()
    }
}

fn strip_draft_prefix(title: &str) -> &str {
    const PREFIXES: [&str; 8] = [
        "Draft:", "Draft -", "[Draft]", "(Draft)", "WIP:", "WIP -", "[WIP]", "(WIP)",
    ];
    PREFIXES
        .iter()
        .find_map(|prefix| {
            title
                .get(..prefix.len())
                .filter(|value| value.eq_ignore_ascii_case(prefix))
                .map(|_| title[prefix.len()..].trim_start())
        })
        .unwrap_or(title)
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
    async fn creates_branch_from_ref_when_writes_are_enabled() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v4/projects/team%2Fservice/repository/branches")
                .json_body(json!({
                    "branch": "feature/retry-payment",
                    "ref": "main"
                }));
            then.status(201).json_body(json!({
                "name": "feature/retry-payment",
                "commit": {"id": "abc123"}
            }));
        });
        let mcp = GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 4096), true);
        let result = mcp
            .create_branch(Parameters(CreateBranch {
                project: "team/service".into(),
                branch: "feature/retry-payment".into(),
                git_ref: "main".into(),
            }))
            .await
            .unwrap();
        mock.assert();
        assert_ne!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn repository_inspection_tools_map_paths_and_queries() {
        let server = MockServer::start();
        let branches = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/team%2Fservice/repository/branches")
                .query_param("page", "2")
                .query_param("per_page", "10")
                .query_param("search", "release");
            then.status(200).json_body(json!([{"name": "release"}]));
        });
        let branch = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/team%2Fservice/repository/branches/release%2Fqa");
            then.status(200).json_body(json!({"name": "release/qa"}));
        });
        let compare = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/team%2Fservice/repository/compare")
                .query_param("from", "release/qa")
                .query_param("to", "release/prod")
                .query_param("straight", "true");
            then.status(200)
                .json_body(json!({"commits": [], "diffs": []}));
        });
        let commits = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/team%2Fservice/repository/commits")
                .query_param("ref_name", "release/prod")
                .query_param("first_parent", "true")
                .query_param("page", "1")
                .query_param("per_page", "25");
            then.status(200).json_body(json!([{"id": "abc123"}]));
        });
        let commit = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/team%2Fservice/repository/commits/abc123")
                .query_param("stats", "false");
            then.status(200).json_body(json!({"id": "abc123"}));
        });
        let mcp =
            GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 64 * 1024), false);

        mcp.list_branches(Parameters(BranchList {
            project: "team/service".into(),
            search: Some("release".into()),
            regex: None,
            page: Some(2),
            per_page: Some(10),
        }))
        .await
        .unwrap();
        mcp.get_branch(Parameters(BranchRef {
            project: "team/service".into(),
            branch: "release/qa".into(),
        }))
        .await
        .unwrap();
        mcp.compare_refs(Parameters(CompareRefs {
            project: "team/service".into(),
            from: "release/qa".into(),
            to: "release/prod".into(),
            straight: Some(true),
        }))
        .await
        .unwrap();
        mcp.list_commits(Parameters(CommitList {
            project: "team/service".into(),
            git_ref: Some("release/prod".into()),
            path: None,
            since: None,
            until: None,
            first_parent: Some(true),
            with_stats: None,
            order: None,
            page: Some(1),
            per_page: Some(25),
        }))
        .await
        .unwrap();
        mcp.get_commit(Parameters(CommitRef {
            project: "team/service".into(),
            sha: "abc123".into(),
            stats: Some(false),
        }))
        .await
        .unwrap();

        branches.assert();
        branch.assert();
        compare.assert();
        commits.assert();
        commit.assert();
    }

    #[tokio::test]
    async fn group_and_cicd_tools_map_paths_and_trace_text() {
        let server = MockServer::start();
        let group = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/groups/platform%2Fbackend/projects")
                .query_param("include_subgroups", "true")
                .query_param("page", "1")
                .query_param("per_page", "30");
            then.status(200).json_body(json!([{"id": 42}]));
        });
        let pipeline = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/team%2Fservice/pipelines/987");
            then.status(200).json_body(json!({"id": 987}));
        });
        let trace = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/team%2Fservice/jobs/654/trace");
            then.status(200).body("compile\ntest failed\n");
        });
        let mcp =
            GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 64 * 1024), false);

        mcp.list_group_projects(Parameters(GroupProjects {
            group: "platform/backend".into(),
            search: None,
            include_subgroups: Some(true),
            with_shared: None,
            archived: None,
            page: None,
            per_page: None,
        }))
        .await
        .unwrap();
        mcp.get_pipeline(Parameters(PipelineId {
            project: "team/service".into(),
            pipeline_id: 987,
        }))
        .await
        .unwrap();
        let result = mcp
            .get_job_trace(Parameters(JobTrace {
                project: "team/service".into(),
                job_id: 654,
            }))
            .await
            .unwrap();

        group.assert();
        pipeline.assert();
        trace.assert();
        let result = serde_json::to_value(result).unwrap();
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("test failed")
        );
    }

    #[tokio::test]
    async fn update_merge_request_fetches_title_to_remove_draft_prefix() {
        let server = MockServer::start();
        let get = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v4/projects/team%2Fservice/merge_requests/128");
            then.status(200)
                .json_body(json!({"title": "WIP: Retry payment"}));
        });
        let update = server.mock(|when, then| {
            when.method(PUT)
                .path("/api/v4/projects/team%2Fservice/merge_requests/128")
                .json_body(json!({
                    "title": "Retry payment",
                    "description": "Ready for review",
                    "reviewer_ids": [10, 11],
                    "assignee_ids": [],
                    "squash": true,
                    "remove_source_branch": false
                }));
            then.status(200)
                .json_body(json!({"iid": 128, "draft": false}));
        });
        let mcp =
            GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 64 * 1024), true);
        let result = mcp
            .update_merge_request(Parameters(UpdateMergeRequest {
                project: "team/service".into(),
                iid: 128,
                title: None,
                description: Some("Ready for review".into()),
                draft: Some(false),
                reviewer_ids: Some(vec![10, 11]),
                assignee_ids: Some(vec![]),
                squash: Some(true),
                remove_source_branch: Some(false),
            }))
            .await
            .unwrap();
        get.assert();
        update.assert();
        assert_ne!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn creates_merge_request_with_squash_and_source_branch_removal() {
        let server = MockServer::start();
        let create = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v4/projects/team%2Fservice/merge_requests")
                .json_body(json!({
                    "source_branch": "feature/retry",
                    "target_branch": "main",
                    "title": "Retry payment",
                    "description": null,
                    "squash": true,
                    "remove_source_branch": true
                }));
            then.status(201).json_body(json!({"iid": 129}));
        });
        let mcp =
            GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 64 * 1024), true);

        let result = mcp
            .create_merge_request(Parameters(CreateMergeRequest {
                project: "team/service".into(),
                source_branch: "feature/retry".into(),
                target_branch: "main".into(),
                title: "Retry payment".into(),
                description: None,
                draft: None,
                squash: Some(true),
                remove_source_branch: Some(true),
            }))
            .await
            .unwrap();

        create.assert();
        assert_ne!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn creates_annotated_tag_when_writes_are_enabled() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/v4/projects/team%2Fservice/repository/tags")
                .json_body(json!({
                    "tag_name": "v1.2.0",
                    "ref": "abc123",
                    "message": "Production release"
                }));
            then.status(201).json_body(json!({"name": "v1.2.0"}));
        });
        let mcp = GitLabMcp::with_write(GitLabClient::for_test(&server.base_url(), 4096), true);
        let result = mcp
            .create_tag(Parameters(CreateTag {
                project: "team/service".into(),
                tag_name: "v1.2.0".into(),
                git_ref: "abc123".into(),
                message: Some("Production release".into()),
            }))
            .await
            .unwrap();
        mock.assert();
        assert_ne!(result.is_error, Some(true));
    }

    #[test]
    fn draft_title_helpers_support_gitlab_prefixes() {
        assert_eq!(set_draft_title("Feature", true), "Draft: Feature");
        assert_eq!(set_draft_title("[Draft] Feature", true), "Draft: Feature");
        assert_eq!(set_draft_title("wip: Feature", false), "Feature");
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

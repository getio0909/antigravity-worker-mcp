use crate::{broker::Broker, model::*};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct Server {
    broker: Arc<Broker>,
    tool_router: ToolRouter<Self>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobId {
    pub job_id: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListArgs {
    pub after: Option<String>,
    #[serde(default = "list_size")]
    pub limit: usize,
}
fn list_size() -> usize {
    20
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultArgs {
    pub job_id: String,
    /// Starting offset in Unicode characters.
    #[serde(default)]
    pub offset: usize,
    /// Page length in Unicode characters, from 1 to 16,000; defaults to 8,000.
    #[serde(default = "page_size")]
    pub limit: usize,
    #[serde(default)]
    pub section: Section,
}
fn page_size() -> usize {
    8000
}
#[derive(Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Section {
    #[default]
    Response,
    Patch,
}
fn reply(value: Outcome<Value>) -> CallToolResult {
    match value {
        Ok(v) => CallToolResult::structured(v),
        Err(e) => CallToolResult::structured_error(serde_json::json!({"error":e})),
    }
}
#[tool_router]
impl Server {
    pub fn new(broker: Arc<Broker>) -> Self {
        Self {
            broker,
            tool_router: Self::tool_router(),
        }
    }
    #[tool(
        description = "Delegate a background task to the autonomous Antigravity CLI. Default: no isolation, full current-user permissions, automatic execution, no task deadline. Jobs survive MCP disconnection and are shared by clients using the same state directory. Use an idempotency_key for safe submission retries, isolation=true for a working copy, or analysis for read-only inputs. Results require independent review.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn ag_submit(&self, Parameters(input): Parameters<Submission>) -> CallToolResult {
        reply(self.broker.submit(input).await)
    }
    #[tool(
        description = "Read job state, deadlines and bounded progress counters.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn ag_status(&self, Parameters(id): Parameters<JobId>) -> CallToolResult {
        reply(self.broker.status(&id.job_id))
    }
    #[tool(
        description = "Read an unverified report or patch in Unicode character pages. limit is 1-16000 (default 8000); offset counts characters. Follow next_offset for subsequent pages. Completion does not certify correctness.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn ag_result(&self, Parameters(args): Parameters<ResultArgs>) -> CallToolResult {
        reply(self.broker.result(
            &args.job_id,
            args.offset,
            args.limit,
            matches!(args.section, Section::Patch),
        ))
    }
    #[tool(
        description = "Request cancellation of a shared background job. Wait up to ten seconds for supervised cleanup; inspect process_stopped and poll ag_status if cancellation remains pending. Cancellation does not undo host effects.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ag_cancel(&self, Parameters(id): Parameters<JobId>) -> CallToolResult {
        reply(self.broker.cancel(&id.job_id).await)
    }
    #[tool(
        description = "Query CLI version, live model catalog, configured permissions and resource limits without inference. Remaining subscription quota is unavailable.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true)
    )]
    async fn ag_capabilities(&self, Parameters(_): Parameters<Empty>) -> CallToolResult {
        reply(self.broker.probe().await)
    }
    #[tool(
        description = "List shared background job IDs and bounded status metadata, including expired results. Use next_after for pagination. No task text is returned.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn ag_list(&self, Parameters(args): Parameters<ListArgs>) -> CallToolResult {
        reply(self.broker.list(args.after.as_deref(), args.limit))
    }
    #[tool(
        description = "Explicitly clear the shared quota pause after checking provider availability. No failed or interrupted task is replayed.",
        annotations(
            read_only_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ag_resume(&self, Parameters(_): Parameters<Empty>) -> CallToolResult {
        reply(self.broker.resume().await)
    }
    #[tool(
        description = "Discard one terminal job's result or patch to free result-store capacity. Its job metadata, idempotency key and enabled audit logs remain. Active jobs are rejected.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ag_forget(&self, Parameters(id): Parameters<JobId>) -> CallToolResult {
        reply(self.broker.forget(&id.job_id).await)
    }
}
#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            rmcp::model::Implementation::new("antigravity-worker-mcp", env!("CARGO_PKG_VERSION")),
        )
    }
}

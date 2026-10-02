use crate::{jobs::Worker, model::*};
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
    pub worker: Arc<Worker>,
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
pub struct ResultArgs {
    pub job_id: String,
    #[serde(default)]
    pub offset: usize,
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
    pub fn new(worker: Arc<Worker>) -> Self {
        Self {
            worker,
            tool_router: Self::tool_router(),
        }
    }
    #[tool(
        description = "Dispatch an asynchronous task to the official Antigravity CLI. Default isolation=false runs in the original allowed workspace with current-user permissions and automatic execution. Use isolation=true for a separate working copy, or execution_mode=analysis for read-only inputs. Results require independent review.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn ag_submit(&self, Parameters(input): Parameters<Submission>) -> CallToolResult {
        reply(self.worker.submit(input))
    }
    #[tool(
        description = "Read job state, deadlines and bounded progress counters.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn ag_status(&self, Parameters(id): Parameters<JobId>) -> CallToolResult {
        reply(self.worker.status(&id.job_id))
    }
    #[tool(
        description = "Read an unverified report or patch in Unicode character pages. Follow next_offset for subsequent pages. Completion does not certify correctness.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn ag_result(&self, Parameters(args): Parameters<ResultArgs>) -> CallToolResult {
        reply(self.worker.result(
            &args.job_id,
            args.offset,
            args.limit,
            matches!(args.section, Section::Patch),
        ))
    }
    #[tool(
        description = "Cancel a queued or running job and wait for runtime termination and cleanup.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn ag_cancel(&self, Parameters(id): Parameters<JobId>) -> CallToolResult {
        reply(self.worker.cancel(&id.job_id).await)
    }
    #[tool(
        description = "Query CLI version, live model catalog, configured permissions and resource limits without inference. Remaining subscription quota is unavailable.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true)
    )]
    async fn ag_capabilities(&self, Parameters(_): Parameters<Empty>) -> CallToolResult {
        reply(self.worker.capabilities().await)
    }
}
#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            rmcp::model::Implementation::new("antigravity-worker-mcp", "0.1.0"),
        )
    }
}

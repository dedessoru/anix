use anix_domain::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{future::Future, pin::Pin};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub read_only: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolInput {
    pub schema_version: u32,
    pub parameters: Value,
}
#[derive(Clone, Debug)]
pub struct ToolContext {
    pub project_id: ProjectId,
    pub run_id: Option<RunId>,
    pub context_snapshot_id: Option<ContextSnapshotId>,
    pub tool_invocation_id: Option<ToolInvocationId>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolOutput {
    pub schema_version: u32,
    pub data: Value,
    pub metadata: Value,
}
pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = Result<ToolOutput>> + Send + 'a>>;
pub trait Tool: Send + Sync {
    fn descriptor(&self) -> ToolDescriptor;
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_>;
}
/// Future coding integrations implement this port after application approval.
/// Core never requires a Codex executable or a particular model provider.
pub trait CodingExecutor: Send + Sync {
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_>;
}

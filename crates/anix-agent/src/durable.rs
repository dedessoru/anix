use crate::*;
use anix_core::{CommandContext, Core};
use chrono::Utc;
use serde_json::json;
use std::{sync::Arc, time::Instant};

/// Host-owned observer. Workers receive the ModelRuntime port, never this host's Core handle.
pub struct RecordedRuntime {
    core: Arc<Core>,
    inner: Arc<dyn ModelRuntime>,
    project: ProjectId,
    run: RunId,
    work: WorkNodeId,
    snapshot: ContextSnapshotId,
    provider: String,
    instruction_version: String,
}
impl RecordedRuntime {
    pub fn new(
        core: Arc<Core>,
        inner: Arc<dyn ModelRuntime>,
        project: ProjectId,
        run: RunId,
        snapshot: ContextSnapshotId,
        provider: String,
        instruction_version: String,
    ) -> Result<Self> {
        let state = core.runtime_state(project)?;
        let binding = state
            .execution_bindings
            .get(&run)
            .ok_or_else(|| Error::NotFound("binding".into()))?;
        let context = state
            .context_snapshots
            .get(&snapshot)
            .ok_or_else(|| Error::NotFound("snapshot".into()))?;
        if context.work_node_id != binding.work_node_id
            || context.plan_revision_id != binding.plan_revision_id
        {
            return Err(Error::Validation(
                "recorded runtime snapshot mismatch".into(),
            ));
        }
        Ok(Self {
            core,
            inner,
            project,
            run,
            work: binding.work_node_id,
            snapshot,
            provider,
            instruction_version,
        })
    }
    fn context(&self) -> CommandContext {
        CommandContext {
            actor: Actor::System,
            reason: "Durable model invocation audit".into(),
            run_id: Some(self.run),
            correlation_id: None,
            causation_id: None,
        }
    }
}
impl ModelRuntime for RecordedRuntime {
    fn available(&self) -> bool {
        self.inner.available()
    }
    fn capabilities(&self) -> RuntimeCapabilities {
        self.inner.capabilities()
    }
    fn generate<'a>(&'a self, request: RuntimeRequest) -> IntelligenceFuture<'a, RuntimeResponse> {
        Box::pin(async move {
            let start = Instant::now();
            let mut invocation = ModelInvocation {
                id: ModelInvocationId::new(),
                meta: Meta::new(self.project),
                run_id: self.run,
                work_node_id: self.work,
                context_snapshot_id: self.snapshot,
                provider: self.provider.clone(),
                model: "pending".into(),
                model_revision: "pending".into(),
                runtime: "pending".into(),
                instruction_version: self.instruction_version.clone(),
                output_schema: request.schema.clone(),
                started_at: Utc::now(),
                finished_at: None,
                latency_ms: None,
                status: InvocationStatus::Running,
                usage: TokenUsage::default(),
                metadata: json!({"schema_version":1,"request_input":request.input,"request_instruction":request.instruction}),
            };
            self.core
                .record_model_invocation(self.project, invocation.clone(), self.context())?;
            let result = self.inner.generate_structured(request).await;
            invocation.finished_at = Some(Utc::now());
            invocation.latency_ms = Some(start.elapsed().as_millis() as u64);
            match &result {
                Ok(r) => {
                    invocation.status = InvocationStatus::Succeeded;
                    invocation.model = r.model.clone();
                    invocation.model_revision = r.revision.clone();
                    invocation.runtime = r.runtime.clone();
                    invocation.usage = r.usage.clone();
                    invocation.metadata["output"] = r.output.clone();
                }
                Err(_) => {
                    invocation.status = InvocationStatus::Failed;
                    invocation.metadata["error_code"] = json!("runtime_failure");
                }
            }
            self.core
                .record_model_invocation(self.project, invocation, self.context())?;
            result
        })
    }
}

/// One router per durable run. Usage and anonymous routing decisions survive restarts.
pub struct DurableRouter {
    pub core: Arc<Core>,
    pub router: IntelligenceRouter,
    pub project: ProjectId,
    pub run: RunId,
}
impl DurableRouter {
    pub async fn execute(
        &self,
        requirements: &TaskRequirements,
        input: serde_json::Value,
        schema: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let result = self.router.execute(requirements, input, schema).await;
        let usage = serde_json::to_value(
            self.router
                .usage
                .lock()
                .map_err(|_| Error::Model("usage lock".into()))?
                .clone(),
        )
        .map_err(|e| Error::Model(e.to_string()))?;
        let telemetry = serde_json::to_value(
            self.router
                .telemetry
                .lock()
                .map_err(|_| Error::Model("telemetry lock".into()))?
                .clone(),
        )
        .map_err(|e| Error::Model(e.to_string()))?;
        self.core.record_routing_usage(
            self.project,
            self.run,
            usage,
            telemetry,
            CommandContext {
                actor: Actor::System,
                reason: "Persist anonymous local routing telemetry".into(),
                run_id: Some(self.run),
                correlation_id: None,
                causation_id: None,
            },
        )?;
        result
    }
}

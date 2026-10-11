use crate::*;
use anix_core::{CommandContext, Core};
use chrono::Utc;
use serde_json::json;

fn bounded_output_summary(data: &serde_json::Value, tool: &str) -> String {
    let encoded = serde_json::to_string(data).unwrap_or_default();
    const LIMIT: usize = 1400;
    let preview: String = encoded.chars().take(LIMIT).collect();
    let suffix = if encoded.chars().count() > LIMIT {
        "…[truncated]"
    } else {
        ""
    };
    format!("Output from {tool}; bounded JSON preview: {preview}{suffix}")
}

fn redact_runtime_identities(value: &mut serde_json::Value) {
    const INTERNAL_KEYS: &[&str] = &[
        "source_id",
        "paper_id",
        "available_source_ids",
        "search_invocation_id",
        "previous_search_id",
    ];
    match value {
        serde_json::Value::Object(object) => {
            for key in INTERNAL_KEYS {
                object.remove(*key);
            }
            for child in object.values_mut() {
                redact_runtime_identities(child);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                redact_runtime_identities(child);
            }
        }
        _ => {}
    }
}

pub struct RecordedTool {
    pub core: Arc<Core>,
    pub inner: Arc<dyn Tool>,
    pub offloader: ArtifactOffloader,
    pub version: String,
}
impl Tool for RecordedTool {
    fn descriptor(&self) -> ToolDescriptor {
        self.inner.descriptor()
    }
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let run = context
                .run_id
                .ok_or_else(|| Error::Validation("recorded tools require run".into()))?;
            let state = self.core.runtime_state(context.project_id)?;
            let b = state
                .execution_bindings
                .get(&run)
                .ok_or_else(|| Error::NotFound("binding".into()))?;
            let ctx = CommandContext {
                actor: Actor::Tool(self.descriptor().name),
                reason: "Durable tool invocation".into(),
                run_id: Some(run),
                correlation_id: None,
                causation_id: None,
            };
            let mut invocation = ToolInvocation {
                id: ToolInvocationId::new(),
                meta: Meta::new(context.project_id),
                run_id: run,
                work_node_id: b.work_node_id,
                tool: self.descriptor().name,
                tool_version: self.version.clone(),
                input_artifacts: vec![],
                output_artifacts: vec![],
                started_at: Utc::now(),
                finished_at: None,
                status: InvocationStatus::Running,
                error_code: None,
            };
            let input_bytes = serde_json::to_vec(&input).map_err(tool)?;
            let file = self.offloader.workspace.write_new(
                &format!("data/tool-input-{}.json", invocation.id),
                &input_bytes,
            )?;
            let artifact = self.core.register_execution_artifact(
                context.project_id,
                anix_core::NewArtifact {
                    name: "tool input".into(),
                    artifact_kind: ArtifactKind::Document,
                    path: file.path,
                    content_hash: file.content_hash,
                    byte_size: file.byte_size,
                    metadata: json!({"schema_version":1}),
                },
                ctx.clone(),
            )?;
            invocation.input_artifacts.push(artifact.id);
            self.core.record_tool_invocation(
                context.project_id,
                invocation.clone(),
                ctx.clone(),
            )?;
            let mut inner_context = context.clone();
            inner_context.tool_invocation_id = Some(invocation.id);
            let result = self.inner.execute(input, inner_context).await;
            let result = result.and_then(|mut output| {
                if output.schema_version != 1 {
                    return Err(Error::Tool("unsupported output schema".into()));
                }
                safe_metadata(&output.metadata)?;
                let output_schema = json!({
                    "schema_version": output.schema_version,
                    "tool_output_metadata": output.metadata.clone(),
                });
                let mut preview_data = output.data.clone();
                redact_runtime_identities(&mut preview_data);
                let stored = self.offloader.store(
                    &self.core,
                    context.project_id,
                    output.data.clone(),
                    output_schema.clone(),
                    bounded_output_summary(&preview_data, &invocation.tool),
                    ctx.clone(),
                )?;
                if let OffloadedOutput::Artifact { id, .. } = &stored {
                    invocation.output_artifacts.push(*id);
                    output.data = serde_json::to_value(stored).map_err(tool)?;
                } else {
                    let bytes = serde_json::to_vec(&output).map_err(tool)?;
                    let file = self
                        .offloader
                        .workspace
                        .write_new(&format!("data/tool-output-{}.json", invocation.id), &bytes)?;
                    let artifact = self.core.register_execution_artifact(
                        context.project_id,
                        anix_core::NewArtifact {
                            name: "tool output".into(),
                            artifact_kind: ArtifactKind::Output,
                            path: file.path,
                            content_hash: file.content_hash,
                            byte_size: file.byte_size,
                            metadata: output_schema,
                        },
                        ctx.clone(),
                    )?;
                    invocation.output_artifacts.push(artifact.id);
                }
                Ok(output)
            });
            let result = result.map(|mut output| {
                // Durable artifacts retain the complete result for replay and audit;
                // the Lead receives only runtime-issued handles and evidence fields.
                redact_runtime_identities(&mut output.data);
                redact_runtime_identities(&mut output.metadata);
                output
            });
            if result.is_ok() {
                invocation.status = InvocationStatus::Succeeded;
            } else {
                invocation.status = InvocationStatus::Failed;
                invocation.error_code = Some(
                    match result.as_ref().err() {
                        Some(Error::Validation(_)) => "validation_error",
                        Some(Error::InvalidTransition(_)) => "invalid_transition",
                        Some(Error::PermissionDenied) => "permission_denied",
                        _ => "tool_failure",
                    }
                    .into(),
                );
            }
            invocation.finished_at = Some(Utc::now());
            self.core
                .record_tool_invocation(context.project_id, invocation, ctx)?;
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::redact_runtime_identities;
    use serde_json::json;

    #[test]
    fn model_facing_tool_result_hides_runtime_identity_fields() {
        let mut result = json!({
            "sources":[{"handle":"S1","source_id":"source-uuid"}],
            "retrieval_batch":{"search_invocation_id":"invocation-uuid","returned":[{"source_id":"source-uuid"}]},
            "refinement":{"previous_search_id":"old-invocation"},
            "evidence":"retained",
            "papers":[{"paper_id":"paper-uuid","title":"Title"}]
        });
        redact_runtime_identities(&mut result);
        assert_eq!(result["sources"][0]["handle"], "S1");
        assert!(result["sources"][0].get("source_id").is_none());
        assert!(result["retrieval_batch"]
            .get("search_invocation_id")
            .is_none());
        assert!(result["refinement"].get("previous_search_id").is_none());
        assert_eq!(result["evidence"], "retained");
        assert!(result["papers"][0].get("paper_id").is_none());
    }
}

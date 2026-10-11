//! Long horizon research-loop campaign driver. It deliberately uses the same
//! `LeadAgent::run_step` entrypoint as an application instead of mutating Core
//! state in response to model JSON.
use crate::{audit_project, HarnessConfig};
use anix_agent::{
    ApprovedTools, FixedModelSelector, LeadAgent, LocalOnlyTestProvider, ModelProvider,
    ModelSelection, ProviderLocality, RetrievalProtocol,
};
use anix_core::{CommandContext, Core, NewArtifact, NewPaper, NewSource, Submission};
use anix_domain::*;
use anix_integrations::{
    ArtifactOffloader, OpenAiCompatibleModelProvider, RecordedTool, StoredFile, TestLeadConfig,
    Tool, ToolContext, ToolDescriptor, ToolFuture, ToolInput, ToolOutput, Workspace,
};
use anix_storage::SqliteStorage;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

const MAX_STEPS: u32 = 100;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BranchCapability {
    pub completed_evidence_branches: usize,
    pub reconciled: bool,
}

fn branch_capability(state: &ProjectState, root: WorkNodeId) -> BranchCapability {
    let branches: BTreeSet<_> = state
        .work_nodes
        .values()
        .filter(|work| {
            work.granularity == WorkGranularity::Atomic && work.status == WorkStatus::Succeeded
        })
        .filter(|work| {
            state.tool_invocations.values().any(|invocation| {
                invocation.work_node_id == work.id
                    && invocation.tool == "read_source"
                    && invocation.status == InvocationStatus::Succeeded
                    && invocation.output_artifacts.iter().any(|id| {
                        state.artifacts.get(id).is_some_and(|artifact| {
                            let metadata = artifact
                                .metadata
                                .get("tool_output_metadata")
                                .unwrap_or(&artifact.metadata["schema"]["tool_output_metadata"]);
                            metadata["retrieval_consumption"].is_object()
                                || metadata["source_content_read"].is_object()
                        })
                    })
            })
        })
        .map(|work| work.branch_id)
        .collect();
    let reconciled = state.events.iter().any(|event| {
        event.event_type == "BranchResultReconciled"
            && event.payload["target_work_node_id"] == json!(root)
    });
    BranchCapability {
        completed_evidence_branches: branches.len(),
        reconciled,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LongHorizonReport {
    #[serde(default)]
    pub branch_capability: BranchCapability,
    #[serde(default)]
    pub capability_failure: Option<String>,
    #[serde(default)]
    pub duplicate_work_nodes: usize,
    #[serde(default)]
    pub repeated_search_observations: usize,
    pub scenario: String,
    pub seed: u64,
    pub project_id: ProjectId,
    pub status: String,
    pub meaningful_steps: u32,
    pub lead_calls: u32,
    pub tool_calls: u32,
    #[serde(default)]
    pub retrieval_calls: usize,
    #[serde(default)]
    pub offloaded_source_reads: usize,
    #[serde(default)]
    pub tool_failures: usize,
    #[serde(default)]
    pub recovery_passed: bool,
    pub work_nodes: usize,
    pub branches: usize,
    pub gates: usize,
    pub checkpoints: usize,
    pub max_context_tokens: u32,
    pub context_tokens_p50: u32,
    pub context_tokens_p95: u32,
    pub lead_latency_p50_ms: u64,
    pub lead_latency_p95_ms: u64,
    pub context_compile_p50_ms: u64,
    pub context_compile_p95_ms: u64,
    pub tool_latency_p50_ms: u64,
    pub tool_latency_p95_ms: u64,
    pub core_overhead_estimate_ms: u64,
    pub micro_model_invocations: usize,
    pub repair_attempts: usize,
    pub repair_successes: usize,
    pub full_retries: usize,
    pub structured_failures: usize,
    #[serde(default)]
    pub invalid_reference_count: usize,
    #[serde(default)]
    pub search_stall_count: usize,
    #[serde(default)]
    pub refine_search_calls: usize,
    #[serde(default)]
    pub successful_refinements: usize,
    #[serde(default)]
    pub refinement_validation_failures: usize,
    #[serde(default)]
    pub read_source_calls: usize,
    #[serde(default)]
    pub inspect_calls: usize,
    #[serde(default)]
    pub extract_calls: usize,
    #[serde(default)]
    pub reject_calls: usize,
    #[serde(default)]
    pub pending_batches_created: usize,
    #[serde(default)]
    pub consumed_batches: usize,
    #[serde(default)]
    pub superseded_batches: usize,
    #[serde(default)]
    pub rejected_plain_search_attempts: usize,
    #[serde(default)]
    pub strong_model_calls: usize,
    #[serde(default)]
    pub budget_failures: usize,
    pub elapsed_ms: u64,
    pub audit: crate::AuditReport,
    pub error: Option<String>,
    pub model: String,
    pub provider_locality: String,
    #[serde(default)]
    pub diagnostic_trace_path: Option<String>,
    #[serde(default)]
    pub diagnostic_trace_artifact_id: Option<String>,
    pub report_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecoverySignature {
    succeeded_work: BTreeSet<String>,
    bindings: BTreeSet<String>,
    artifacts: BTreeSet<String>,
    branches: BTreeSet<String>,
}

struct SearchEntitiesTool {
    core: Arc<Core>,
    project: ProjectId,
    fail_once: Option<Arc<AtomicBool>>,
    retrieval: Arc<Mutex<RetrievalState>>,
}
#[derive(Default)]
struct RetrievalState {
    by_handle: std::collections::BTreeMap<String, SourceId>,
    pending: BTreeSet<SourceId>,
    active_search_id: Option<ToolInvocationId>,
    last_fingerprint: Option<String>,
    repeated_fingerprints: usize,
    searches_without_consumption: usize,
    has_unconsumed_retrieval: bool,
    invalid_references: usize,
    search_stalls: usize,
}
#[derive(Serialize)]
struct SearchObservation {
    observation_version: u32,
    research_objective: String,
    current_work_objective: String,
    query: String,
    candidates: Vec<SearchSourceRef>,
    previously_consumed_sources: Vec<String>,
    unresolved_evidence_needs: Vec<String>,
    available_actions: Vec<String>,
    pending_results: bool,
}
#[derive(Clone, Serialize)]
struct SearchSourceRef {
    handle: String,
    #[serde(skip_serializing)]
    source_id: SourceId,
    title: String,
    source_title: String,
    preview: String,
    unread: bool,
}
fn query_fingerprint(query: &str) -> String {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(" ")
}
impl Tool for SearchEntitiesTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "search_sources".into(),
            description: "Search the local research pack with Core FTS/BM25. Input JSON: {\"query\":\"terms to search\"}. The observation returns bounded candidate handles, paper titles, and short previews. Previews are for triage, not verified evidence. Read a plausible candidate with its exact handle before treating it as support or contradiction. Do not use entity IDs.".into(),
            read_only: true,
        }
    }
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            if context.project_id != self.project {
                return Err(Error::ProjectMismatch);
            }
            let query = input.parameters["query"]
                .as_str()
                .ok_or_else(|| Error::Validation("query is required".into()))?;
            if let Some(run_id) = context.run_id {
                let state = self.core.runtime_state(self.project)?;
                if let Some(binding) = state.execution_bindings.get(&run_id) {
                    let protocol = retrieval_protocol();
                    let batches = derive_retrieval_batches(
                        &state,
                        RetrievalScope {
                            anchor_run_id: Some(run_id),
                            work_node_id: binding.work_node_id,
                            branch_id: binding.branch_id,
                        },
                        &retrieval_tool_names(&protocol),
                    )?;
                    if batches.iter().any(|batch| {
                        matches!(
                            batch.status,
                            RetrievalBatchStatus::Pending | RetrievalBatchStatus::PartiallyConsumed
                        )
                    }) {
                        return Err(Error::InvalidTransition(
                            "PendingRetrievalMustBeResolved".into(),
                        ));
                    }
                }
            }
            self.perform_search(query, &context)
        })
    }
}
impl SearchEntitiesTool {
    fn perform_search(&self, query: &str, context: &ToolContext) -> Result<ToolOutput> {
        if self
            .fail_once
            .as_ref()
            .is_some_and(|fail| fail.swap(false, Ordering::SeqCst))
        {
            return Err(Error::Tool("injected one-shot search failure".into()));
        }
        let fingerprint = query_fingerprint(query);
        let mut retrieval = self
            .retrieval
            .lock()
            .map_err(|_| Error::Tool("retrieval tracker poisoned".into()))?;
        if retrieval.has_unconsumed_retrieval {
            if retrieval.last_fingerprint.as_deref() == Some(fingerprint.as_str()) {
                retrieval.repeated_fingerprints += 1;
            } else {
                retrieval.repeated_fingerprints = 1;
                retrieval.last_fingerprint = Some(fingerprint.clone());
            }
            if retrieval.searches_without_consumption >= 2 || retrieval.repeated_fingerprints >= 3 {
                let available: Vec<_> = retrieval
                    .by_handle
                    .iter()
                    .filter(|(_, source_id)| retrieval.pending.contains(source_id))
                    .map(|(handle, _)| handle.clone())
                    .collect();
                retrieval.search_stalls += 1;
                return Ok(ToolOutput {
                    schema_version: 1,
                    data: json!({"observation":"search_stall_detected","available_handles":available,"searches_without_consumption":retrieval.searches_without_consumption,"instruction":"Read, explicitly reject, or justify a refined retrieval before searching again."}),
                    metadata: json!({"stall_detected":true,"semantic_fingerprint":fingerprint}),
                });
            }
        } else {
            retrieval.last_fingerprint = Some(fingerprint.clone());
            retrieval.repeated_fingerprints = 1;
            retrieval.searches_without_consumption = 0;
        }
        let entities = self.core.search_entities(self.project, query, 10)?;
        let state = self.core.runtime_state(self.project)?;
        let mut source_ids = BTreeSet::new();
        for entity in &entities {
            match entity {
                EntityRef::Source(id) if state.sources.contains_key(id) => {
                    source_ids.insert(*id);
                }
                EntityRef::Paper(id) => {
                    source_ids.extend(
                        state
                            .paper_sources
                            .iter()
                            .filter(|link| link.paper_id == *id)
                            .map(|link| link.source_id),
                    );
                }
                _ => {}
            }
        }
        let api = self.core.agent_api("retrieval-source-policy")?;
        source_ids.retain(|id| api.validate_source_access(self.project, *id).is_ok());
        let source_state = serde_json::to_string(
            &source_ids
                .iter()
                .map(|id| {
                    (
                        id,
                        state.sources[id].meta.version,
                        state
                            .paper_sources
                            .iter()
                            .filter(|link| link.source_id == *id)
                            .filter_map(|link| state.papers.get(&link.paper_id))
                            .map(|paper| (paper.id, paper.meta.version))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .map_err(|error| Error::Tool(error.to_string()))?;
        let binding = context
            .run_id
            .and_then(|id| state.execution_bindings.get(&id));
        let work_version =
            binding.map(|binding| state.work_nodes[&binding.work_node_id].meta.version);
        if let Some(binding) = binding {
            let previous = state
                .tool_invocations
                .values()
                .filter(|inv| {
                    inv.work_node_id == binding.work_node_id
                        && inv.tool == "search_sources"
                        && inv.status == InvocationStatus::Succeeded
                })
                .find(|inv| {
                    inv.output_artifacts.iter().any(|id| {
                        state.artifacts.get(id).is_some_and(|artifact| {
                            let metadata = artifact
                                .metadata
                                .get("tool_output_metadata")
                                .unwrap_or(&artifact.metadata["schema"]["tool_output_metadata"]);
                            metadata["semantic_fingerprint"] == fingerprint
                                && metadata["source_state"] == source_state
                                && metadata["work_version"] == json!(work_version)
                        })
                    })
                });
            if let Some(previous) = previous {
                return Ok(ToolOutput {
                    schema_version: 1,
                    data: json!({"observation":{"kind":"repeated_search_without_new_evidence","existing_search":previous.id,"query":query,"instruction":"The same query and source versions were already explored for this work. Use the previous result, propose a revised strategy, or conclude the work inconclusive with an explanation. A changed query, source version or approved work revision permits a fresh search."}}),
                    metadata: json!({"repeated_without_progress":true,"existing_search":previous.id,"semantic_fingerprint":fingerprint,"source_state":source_state,"work_version":work_version}),
                });
            }
        }
        let search_invocation_id = if let Some(id) = context.tool_invocation_id {
            id
        } else {
            ToolInvocationId::new()
        };
        retrieval.by_handle.clear();
        retrieval.pending.clear();
        let mut sources = Vec::new();
        for id in &source_ids {
            let handle = format!("S{}", retrieval.by_handle.len() + 1);
            retrieval.by_handle.insert(handle.clone(), *id);
            retrieval.pending.insert(*id);
            let mut papers: Vec<_> = state
                .paper_sources
                .iter()
                .filter(|link| link.source_id == *id)
                .filter_map(|link| state.papers.get(&link.paper_id))
                .collect();
            papers.sort_by(|a, b| a.title.cmp(&b.title));
            let title = papers
                .first()
                .map(|paper| paper.title.clone())
                .unwrap_or_default();
            let preview = papers
                .iter()
                .take(2)
                .map(|paper| {
                    format!(
                        "{}: {}",
                        paper.title,
                        paper.abstract_text.chars().take(240).collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");
            let source_title = state
                .sources
                .get(id)
                .map(|source| source.title.clone())
                .unwrap_or_default();
            sources.push(SearchSourceRef {
                handle,
                source_id: *id,
                title: if title.is_empty() {
                    source_title.clone()
                } else {
                    title
                },
                source_title,
                preview,
                unread: true,
            });
        }
        retrieval.active_search_id = Some(search_invocation_id);
        retrieval.searches_without_consumption += 1;
        retrieval.has_unconsumed_retrieval = true;
        let binding = context
            .run_id
            .and_then(|run| state.execution_bindings.get(&run));
        let current_work_objective = binding
            .and_then(|binding| state.work_nodes.get(&binding.work_node_id))
            .map(|work| work.objective.clone())
            .unwrap_or_default();
        let research_objective = context
            .context_snapshot_id
            .and_then(|snapshot| state.context_snapshots.get(&snapshot))
            .and_then(|snapshot| {
                snapshot
                    .entries
                    .iter()
                    .find_map(|entry| match &entry.reference.entity {
                        EntityRef::Question(id) => state
                            .questions
                            .get(id)
                            .map(|question| question.title.clone()),
                        _ => None,
                    })
            })
            .unwrap_or_default();
        let previously_consumed_sources = if let Some(binding) = binding {
            derive_retrieval_batches(
                &state,
                RetrievalScope {
                    anchor_run_id: context.run_id,
                    work_node_id: binding.work_node_id,
                    branch_id: binding.branch_id,
                },
                &retrieval_tool_names(&retrieval_protocol()),
            )?
            .iter()
            .flat_map(|batch| {
                batch.consumed_handles.iter().filter_map(|handle| {
                    batch
                        .returned
                        .iter()
                        .find(|source| &source.handle == handle)
                        .and_then(|source| state.sources.get(&source.source_id))
                        .map(|source| source.title.clone())
                })
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
        } else {
            Vec::new()
        };
        let unresolved_evidence_needs = if current_work_objective.is_empty() {
            Vec::new()
        } else {
            vec![current_work_objective.clone()]
        };
        let mut available_actions = Vec::new();
        if !sources.is_empty() {
            available_actions.push("read_source".to_owned());
        }
        available_actions.extend([
            "refine_search".to_owned(),
            "reject_retrieved_sources".to_owned(),
        ]);
        let observation = SearchObservation {
            observation_version: 2,
            research_objective,
            current_work_objective,
            query: query.to_owned(),
            candidates: sources.clone(),
            previously_consumed_sources,
            unresolved_evidence_needs,
            available_actions,
            pending_results: !retrieval.pending.is_empty(),
        };
        Ok(ToolOutput {
            schema_version: 1,
            data: json!({"observation":serde_json::to_value(observation).unwrap_or_default()}),
            metadata: json!({
                "retrieval":"sqlite-fts-bm25",
                "count":entities.len(),
                "semantic_fingerprint":fingerprint,
                "source_state":source_state,"work_version":work_version,
                "retrieval_batch": {
                    "search_invocation_id": search_invocation_id,
                    "query": query,
                    "returned": sources.iter().map(|source| json!({"handle":source.handle,"source_id":source.source_id})).collect::<Vec<_>>(),
                }
            }),
        })
    }
}

struct ReadSourceTool {
    core: Arc<Core>,
    project: ProjectId,
    retrieval: Arc<Mutex<RetrievalState>>,
}

struct RefineSearchTool {
    core: Arc<Core>,
    project: ProjectId,
    search: Arc<SearchEntitiesTool>,
}
impl Tool for RefineSearchTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "refine_search".into(),
            description: "Replace the current pending retrieval only when its candidates do not address the objective and a materially different query can help. Input JSON: {\"reason\":\"optional concise audit note\",\"query\":\"new search terms\"}. Runtime binds the current pending search and rejects identical normalized queries. Do not refine merely because candidates are unread.".into(),
            read_only: true,
        }
    }
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            if context.project_id != self.project {
                return Err(Error::ProjectMismatch);
            }
            let previous: ToolInvocationId = input.parameters["previous_search_id"]
                .as_str()
                .ok_or_else(|| Error::InvalidTransition("NoPendingRetrieval".into()))?
                .parse()
                .map_err(|_| Error::InvalidTransition("InvalidRuntimeSearchBinding".into()))?;
            let reason = input.parameters["reason"]
                .as_str()
                .map(str::trim)
                .filter(|reason| !reason.is_empty());
            let query = input.parameters["query"]
                .as_str()
                .filter(|query| !query.trim().is_empty())
                .ok_or_else(|| Error::Validation("refinement query is required".into()))?;
            let run_id = context
                .run_id
                .ok_or_else(|| Error::Validation("refinement requires a bound run".into()))?;
            let state = self.core.runtime_state(self.project)?;
            let binding = state
                .execution_bindings
                .get(&run_id)
                .ok_or_else(|| Error::NotFound("execution binding".into()))?;
            let protocol = retrieval_protocol();
            let batches = derive_retrieval_batches(
                &state,
                RetrievalScope {
                    anchor_run_id: Some(run_id),
                    work_node_id: binding.work_node_id,
                    branch_id: binding.branch_id,
                },
                &retrieval_tool_names(&protocol),
            )?;
            let pending = batches
                .iter()
                .rev()
                .find(|batch| {
                    matches!(
                        batch.status,
                        RetrievalBatchStatus::Pending | RetrievalBatchStatus::PartiallyConsumed
                    )
                })
                .ok_or_else(|| Error::InvalidTransition("no pending retrieval to refine".into()))?;
            if pending.search_invocation_id != previous {
                return Err(Error::InvalidTransition(
                    "refinement does not match the current pending search".into(),
                ));
            }
            validate_refinement_query(query, &pending.query)?;
            let mut output = self.search.perform_search(query, &context)?;
            if let Some(metadata) = output.metadata.as_object_mut() {
                metadata.insert(
                    "refinement".into(),
                    json!({"previous_search_id":previous,"reason":reason}),
                );
                metadata.insert(
                    "runtime_binding".into(),
                    json!({"search_invocation_id":previous,"resolved_by":"lead_runtime"}),
                );
            }
            Ok(output)
        })
    }
}

struct RejectRetrievedSourcesTool {
    core: Arc<Core>,
    project: ProjectId,
    retrieval: Arc<Mutex<RetrievalState>>,
}
impl Tool for RejectRetrievedSourcesTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "reject_retrieved_sources".into(),
            description: "Close the current retrieval batch only when every candidate is irrelevant or unusable for a specific reason; unread is not a reason. Input JSON: {\"reason\":\"specific reason the results cannot be used\"}. Runtime binds the current pending search. This rejects the batch; it does not assert that the research question is answered.".into(),
            read_only: true,
        }
    }
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            if context.project_id != self.project {
                return Err(Error::ProjectMismatch);
            }
            let requested: ToolInvocationId = input.parameters["search_invocation_id"]
                .as_str()
                .ok_or_else(|| Error::InvalidTransition("NoPendingRetrieval".into()))?
                .parse()
                .map_err(|_| Error::InvalidTransition("InvalidRuntimeSearchBinding".into()))?;
            let reason = input.parameters["reason"]
                .as_str()
                .filter(|reason| !reason.trim().is_empty())
                .ok_or_else(|| Error::Validation("rejection reason is required".into()))?;
            let run_id = context
                .run_id
                .ok_or_else(|| Error::Validation("rejection requires a bound run".into()))?;
            let state = self.core.runtime_state(self.project)?;
            let binding = state
                .execution_bindings
                .get(&run_id)
                .ok_or_else(|| Error::NotFound("execution binding".into()))?;
            let protocol = retrieval_protocol();
            let batches = derive_retrieval_batches(
                &state,
                RetrievalScope {
                    anchor_run_id: Some(run_id),
                    work_node_id: binding.work_node_id,
                    branch_id: binding.branch_id,
                },
                &retrieval_tool_names(&protocol),
            )?;
            let pending = batches
                .iter()
                .rev()
                .find(|batch| {
                    matches!(
                        batch.status,
                        RetrievalBatchStatus::Pending | RetrievalBatchStatus::PartiallyConsumed
                    )
                })
                .ok_or_else(|| Error::InvalidTransition("no pending retrieval to reject".into()))?;
            if pending.search_invocation_id != requested {
                return Err(Error::InvalidTransition(
                    "rejection does not match the current pending search".into(),
                ));
            }
            let mut retrieval = self
                .retrieval
                .lock()
                .map_err(|_| Error::Tool("retrieval tracker poisoned".into()))?;
            retrieval.pending.clear();
            retrieval.has_unconsumed_retrieval = false;
            retrieval.searches_without_consumption = 0;
            retrieval.repeated_fingerprints = 0;
            Ok(ToolOutput {
                schema_version: 1,
                data: json!({"observation":"retrieval_rejected","reason":reason}),
                metadata: json!({"retrieval_rejection":{"search_invocation_id":requested,"reason":reason},"runtime_binding":{"search_invocation_id":requested,"resolved_by":"lead_runtime"}}),
            })
        })
    }
}

impl Tool for ReadSourceTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "read_source".into(),
            description: "Inspect actual source content using the exact bounded handle from the current retrieval batch (such as S1) or runtime_source_catalog (such as E1). Supplied sources can be read directly without a new search. Search previews are not verified evidence. Read before treating a candidate as supporting or contradicting evidence. Runtime resolves the handle; raw IDs are not accepted.".into(),
            read_only: true,
        }
    }
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            if context.project_id != self.project {
                return Err(Error::ProjectMismatch);
            }
            let raw = input.parameters["handle"]
                .as_str()
                .ok_or_else(|| Error::Validation("handle is required".into()))?;
            let persisted = self.core.runtime_state(self.project)?;
            if let Some(run_id) = context.run_id {
                if let Some(binding) = persisted.execution_bindings.get(&run_id) {
                    let protocol = retrieval_protocol();
                    let batches = derive_retrieval_batches(
                        &persisted,
                        RetrievalScope {
                            anchor_run_id: Some(run_id),
                            work_node_id: binding.work_node_id,
                            branch_id: binding.branch_id,
                        },
                        &retrieval_tool_names(&protocol),
                    )?;
                    if let Some(batch) = batches.iter().rev().find(|batch| {
                        matches!(
                            batch.status,
                            RetrievalBatchStatus::Pending | RetrievalBatchStatus::PartiallyConsumed
                        )
                    }) {
                        let mut retrieval = self
                            .retrieval
                            .lock()
                            .map_err(|_| Error::Tool("retrieval tracker poisoned".into()))?;
                        retrieval.by_handle.clear();
                        retrieval.pending.clear();
                        for source in &batch.returned {
                            retrieval
                                .by_handle
                                .insert(source.handle.clone(), source.source_id);
                            if !batch.consumed_handles.contains(&source.handle) {
                                retrieval.pending.insert(source.source_id);
                            }
                        }
                        retrieval.active_search_id = Some(batch.search_invocation_id);
                        retrieval.has_unconsumed_retrieval = !retrieval.pending.is_empty();
                    } else if raw.starts_with('E') {
                        let api = self.core.agent_api("work-source-policy")?;
                        let source_id = api
                            .work_source_handles(self.project, binding.work_node_id)?
                            .into_iter()
                            .find(|(reference, _)| reference.handle == raw)
                            .map(|(reference, _)| reference.source_id)
                            .ok_or_else(|| {
                                Error::Validation("invalid supplied source handle".into())
                            })?;
                        if input.parameters["runtime_source_ref"] != json!(source_id) {
                            return Err(Error::InvalidTransition(
                                "SuppliedSourceBindingStale".into(),
                            ));
                        }
                        api.validate_source_access(self.project, source_id)?;
                        let source = persisted
                            .sources
                            .get(&source_id)
                            .ok_or_else(|| Error::NotFound("supplied source".into()))?;
                        let papers: Vec<_> = persisted.papers.values().filter(|paper| persisted.paper_sources.iter()
                            .any(|link| link.paper_id == paper.id && link.source_id == source_id))
                            .map(|paper| json!({"paper_id":paper.id,"title":paper.title,"abstract_text":paper.abstract_text})).collect();
                        if papers.iter().all(|paper| {
                            paper["abstract_text"]
                                .as_str()
                                .is_none_or(|text| text.trim().is_empty())
                        }) {
                            return Err(Error::Tool("SourceContentUnavailable: supply readable source content or an approved source adapter".into()));
                        }
                        return Ok(ToolOutput {
                            schema_version: 1,
                            data: json!({"title":source.title,"locator":source.locator,"papers":papers}),
                            metadata: json!({"read_only":true,"source_content_read":{"handle":raw,"source_id":source_id,"origin":"work_contract","work_node_id":binding.work_node_id}}),
                        });
                    }
                }
            }
            let mut retrieval = self
                .retrieval
                .lock()
                .map_err(|_| Error::Tool("retrieval tracker poisoned".into()))?;
            let id: SourceId = retrieval
                .by_handle
                .get(raw)
                .copied()
                .ok_or_else(|| Error::Validation("invalid source reference".into()))?;
            self.core
                .agent_api("retrieval-source-policy")?
                .validate_source_access(self.project, id)?;
            if !retrieval.pending.contains(&id) {
                retrieval.invalid_references += 1;
                let allowed: Vec<_> = retrieval
                    .by_handle
                    .iter()
                    .filter(|(_, source_id)| retrieval.pending.contains(source_id))
                    .map(|(handle, _)| handle.clone())
                    .collect();
                return Ok(ToolOutput {
                    schema_version: 1,
                    data: json!({"observation":"InvalidSourceHandle","requested":raw,"available":allowed,"instruction":"Use an exact handle from the latest retrieval batch."}),
                    metadata: json!({"reference_valid":false,"invalid_source_handle":{"requested":raw,"available":allowed}}),
                });
            }
            let state = self.core.runtime_state(self.project)?;
            let source = state
                .sources
                .get(&id)
                .ok_or_else(|| Error::NotFound("source in this research pack".into()))?;
            let search_invocation_id = retrieval.active_search_id.ok_or_else(|| {
                Error::InvalidTransition("source read is not bound to a retrieval batch".into())
            })?;
            let handle = retrieval
                .by_handle
                .iter()
                .find_map(|(handle, source_id)| (*source_id == id).then(|| handle.clone()))
                .ok_or_else(|| {
                    Error::Validation("source handle is not in retrieval batch".into())
                })?;
            retrieval.pending.remove(&id);
            retrieval.searches_without_consumption = 0;
            retrieval.repeated_fingerprints = 0;
            retrieval.has_unconsumed_retrieval = !retrieval.pending.is_empty();
            let papers: Vec<_> = state
                .papers
                .values()
                .filter(|p| {
                    state
                        .paper_sources
                        .iter()
                        .any(|link| link.paper_id == p.id && link.source_id == id)
                })
                .map(|p| json!({"paper_id":p.id,"title":p.title,"abstract_text":p.abstract_text}))
                .collect();
            Ok(ToolOutput {
                schema_version: 1,
                data: json!({"title":source.title,"locator":source.locator,"papers":papers}),
                metadata: json!({
                    "read_only":true,
                    "retrieval_consumption": {
                        "search_invocation_id": search_invocation_id,
                        "handle": handle,
                    }
                }),
            })
        })
    }
}

fn selector() -> Arc<dyn anix_agent::ModelSelector> {
    Arc::new(FixedModelSelector {
        selection: ModelSelection {
            provider: "local-test-lead".into(),
            model: "configured-local-test-lead".into(),
        },
    })
}

fn retrieval_protocol() -> RetrievalProtocol {
    RetrievalProtocol {
        search_tool: "search_sources".into(),
        refine_tool: "refine_search".into(),
        read_tools: vec!["read_source".into()],
        reject_tool: "reject_retrieved_sources".into(),
    }
}

fn validate_refinement_query(query: &str, previous_query: &str) -> Result<()> {
    if query_fingerprint(query) == query_fingerprint(previous_query) {
        return Err(Error::InvalidTransition(
            "NoProgressSearchRefinement".into(),
        ));
    }
    Ok(())
}

fn retrieval_tool_names(protocol: &RetrievalProtocol) -> RetrievalToolNames {
    RetrievalToolNames {
        search: protocol.search_tool.clone(),
        refine: protocol.refine_tool.clone(),
        read: protocol.read_tools.clone(),
        reject: protocol.reject_tool.clone(),
    }
}

fn update_retrieval_telemetry(state: &ProjectState, report: &mut LongHorizonReport) {
    let mut consumed = BTreeSet::new();
    let mut superseded = BTreeSet::new();
    report.refine_search_calls = 0;
    report.successful_refinements = 0;
    report.refinement_validation_failures = 0;
    report.read_source_calls = 0;
    report.inspect_calls = 0;
    report.extract_calls = 0;
    report.reject_calls = 0;
    report.pending_batches_created = 0;
    for invocation in state.tool_invocations.values() {
        match invocation.tool.as_str() {
            "refine_search" => {
                report.refine_search_calls += 1;
                if invocation.status == InvocationStatus::Failed
                    && matches!(
                        invocation.error_code.as_deref(),
                        Some("validation_error" | "invalid_transition")
                    )
                {
                    report.refinement_validation_failures += 1;
                }
            }
            "read_source" => report.read_source_calls += 1,
            "inspect_source" => report.inspect_calls += 1,
            "extract_evidence" => report.extract_calls += 1,
            "reject_retrieved_sources" => report.reject_calls += 1,
            _ => {}
        }
        for artifact_id in &invocation.output_artifacts {
            let Some(artifact) = state.artifacts.get(artifact_id) else {
                continue;
            };
            let metadata = artifact
                .metadata
                .get("tool_output_metadata")
                .or_else(|| artifact.metadata["schema"].get("tool_output_metadata"));
            let Some(metadata) = metadata else {
                continue;
            };
            if metadata.get("retrieval_batch").is_some() {
                report.pending_batches_created += 1;
                if invocation.tool == "refine_search" {
                    report.successful_refinements += 1;
                }
            }
            if let Some(id) = metadata["retrieval_consumption"]["search_invocation_id"].as_str() {
                consumed.insert(id.to_owned());
            }
            if metadata.get("retrieval_batch").is_some() {
                if let Some(id) = metadata["refinement"]["previous_search_id"].as_str() {
                    superseded.insert(id.to_owned());
                }
            }
        }
    }
    report.consumed_batches = consumed.len();
    report.superseded_batches = superseded.len();
}

fn percentile(values: &[u64], fraction: f64) -> u64 {
    if values.is_empty() {
        0
    } else {
        values[((values.len() - 1) as f64 * fraction).ceil() as usize]
    }
}

fn report_percentile(
    reports: &[LongHorizonReport],
    select: fn(&LongHorizonReport) -> u64,
    fraction: f64,
) -> u64 {
    let mut values: Vec<u64> = reports.iter().map(select).collect();
    values.sort_unstable();
    percentile(&values, fraction)
}

fn capability_assertion_failure(
    scenario: &str,
    branches: usize,
    gates: usize,
    retrieval_calls: usize,
    offloaded_source_reads: usize,
    tool_failures: usize,
    branch: &BranchCapability,
) -> Option<&'static str> {
    match scenario {
        "branching" if branches < 2 => {
            Some("branching capability assertion failed: no child Branch was persisted")
        }
        "branching" if branch.completed_evidence_branches<2||!branch.reconciled => Some("branching capability assertion failed: independent completed branch evidence and result reconciliation are required"),
        "long-context" if retrieval_calls == 0 || offloaded_source_reads == 0 => Some(
            "long-context capability assertion failed: retrieval and source offload are required",
        ),
        "tool-failure" if tool_failures == 0 => {
            Some("tool-failure scenario did not record its injected tool error")
        }
        "experiment-proposal" if gates == 0 => {
            Some("experiment-proposal scenario did not exercise a Human Gate")
        }
        _ => None,
    }
}

/// Run one persistent synthetic research loop with a local-only Lead provider.
/// Completion requires an explicit Lead outcome, source consumption and work contracts.
pub fn run_one(
    config: &HarnessConfig,
    scenario: &str,
    seed: u64,
    provider: Arc<dyn ModelProvider>,
) -> Result<LongHorizonReport> {
    run_one_with_policy(config, scenario, seed, provider, false)
}

/// Explicit cloud opt-in applies only to freshly created synthetic catalog projects.
/// Existing local entry points still reject external providers before any request.
pub fn run_one_cloud_synthetic(
    config: &HarnessConfig,
    scenario: &str,
    seed: u64,
    provider: Arc<dyn ModelProvider>,
) -> Result<LongHorizonReport> {
    if config.endpoint != "https://api.groq.com/openai/v1"
        || config.model_name != "openai/gpt-oss-120b"
        || !config.capabilities.contains("groq")
    {
        return Err(Error::PermissionDenied);
    }
    run_one_with_policy(config, scenario, seed, provider, true)
}

fn run_one_with_policy(
    config: &HarnessConfig,
    scenario: &str,
    seed: u64,
    provider: Arc<dyn ModelProvider>,
    allow_external: bool,
) -> Result<LongHorizonReport> {
    let locality = provider.locality();
    if locality != ProviderLocality::Local && !allow_external {
        return Err(Error::PermissionDenied);
    }
    let started = Instant::now();
    let ws = Arc::new(Workspace::open(&config.workspace)?);
    let db_path = ws.database_path()?;
    let core = Arc::new(Core::new(Arc::new(SqliteStorage::open(&db_path)?)));
    let local: Arc<dyn ModelProvider> = if allow_external {
        provider
    } else {
        Arc::new(LocalOnlyTestProvider::new(provider))
    };
    let mut report = LongHorizonReport {
        branch_capability: BranchCapability::default(),
        capability_failure: None,
        duplicate_work_nodes: 0,
        repeated_search_observations: 0,
        scenario: scenario.into(),
        seed,
        project_id: ProjectId::new(),
        status: "failed_safely".into(),
        meaningful_steps: 0,
        lead_calls: 0,
        tool_calls: 0,
        retrieval_calls: 0,
        offloaded_source_reads: 0,
        tool_failures: 0,
        recovery_passed: false,
        work_nodes: 0,
        branches: 0,
        gates: 0,
        checkpoints: 0,
        max_context_tokens: 0,
        context_tokens_p50: 0,
        context_tokens_p95: 0,
        lead_latency_p50_ms: 0,
        lead_latency_p95_ms: 0,
        context_compile_p50_ms: 0,
        context_compile_p95_ms: 0,
        tool_latency_p50_ms: 0,
        tool_latency_p95_ms: 0,
        core_overhead_estimate_ms: 0,
        micro_model_invocations: 0,
        repair_attempts: 0,
        repair_successes: 0,
        full_retries: 0,
        structured_failures: 0,
        invalid_reference_count: 0,
        search_stall_count: 0,
        refine_search_calls: 0,
        successful_refinements: 0,
        refinement_validation_failures: 0,
        read_source_calls: 0,
        inspect_calls: 0,
        extract_calls: 0,
        reject_calls: 0,
        pending_batches_created: 0,
        consumed_batches: 0,
        superseded_batches: 0,
        rejected_plain_search_attempts: 0,
        strong_model_calls: 0,
        budget_failures: 0,
        elapsed_ms: 0,
        audit: crate::AuditReport::default(),
        error: None,
        model: config.model_name.clone(),
        provider_locality: if locality == ProviderLocality::Local {
            "local"
        } else {
            "external"
        }
        .into(),
        diagnostic_trace_path: None,
        diagnostic_trace_artifact_id: None,
        report_path: String::new(),
    };
    let mut recovery_signature: Option<RecoverySignature> = None;
    let retrieval_tracker = Arc::new(Mutex::new(RetrievalState::default()));
    let result = (|| -> Result<()> {
        let kind = crate::ScenarioKind::ALL
            .into_iter()
            .find(|kind| kind.name() == scenario)
            .ok_or_else(|| {
                Error::Validation(format!("unknown long-horizon scenario {scenario}"))
            })?;
        let (question_text, documents, _ground_truth) = crate::scenario_text(kind, seed)?;
        let project = core.projects().create(
            format!("long-{scenario}-{seed}"),
            "Synthetic long-horizon pack".into(),
            CommandContext::human("eval", "create local campaign fixture"),
        )?;
        report.project_id = project.id;
        let question = core.questions().add(
            project.id,
            question_text.clone(),
            "Use only supplied evidence and ground conclusions in retrieved local sources.".into(),
            CommandContext::human("eval", "create research question"),
        )?;
        for (i, document) in documents.iter().enumerate() {
            let source = core.sources().add(
                project.id,
                NewSource {
                    title: format!("Local source {}", i + 1),
                    source_kind: SourceKind::Manual,
                    locator: format!("pack://{scenario}/{i}"),
                    checksum: Some(format!("seed-{seed}-{i}")),
                    metadata: json!({"schema_version":1,"scenario":scenario}),
                },
                CommandContext::human("eval", "load synthetic source"),
            )?;
            core.papers().add(
                project.id,
                NewPaper {
                    title: format!("Local paper {}", i + 1),
                    authors: vec!["ANIX synthetic pack".into()],
                    publication_year: Some(2026),
                    doi: None,
                    abstract_text: document.clone(),
                    source_ids: vec![source.id],
                    metadata: json!({"schema_version":1}),
                },
                CommandContext::human("eval", "load synthetic evidence"),
            )?;
        }
        let branch = core.create_branch(
            project.id,
            "main".into(),
            None,
            CommandContext::human("eval", "create main branch"),
        )?;
        let root = core.create_work(project.id, anix_core::NewWork {
            branch_id: branch.id, kind: "long-horizon-research".into(), granularity: WorkGranularity::Composite,
            title: format!("Investigate {scenario}"), objective: format!("Investigate the research question: {question_text}. Use the local research pack to inspect source content, persist grounded evidence, and conclude when evidence is sufficient or explicitly inconclusive. Use branches only for independent research paths."),
            acceptance_criteria: OutputContract { required_entities: vec![EntityRef::Question(question.id)], required_artifacts: vec![], minimum_children: 1 },
            capability_requirements: vec![], context_policy_id: "long-horizon-bounded-v1".into(),
            budget: WorkBudget { context_tokens: 4096, max_model_calls: 40, max_tool_calls: 60 },
        }, CommandContext::human("eval", "create composite research plan"))?;
        let root = core.transition_work(
            project.id,
            root.id,
            root.meta.version,
            WorkStatus::Ready,
            CommandContext::human("eval", "ready composite research plan"),
        )?;
        let api = core.agent_api("local-test-lead")?;
        let task = api.create_task(project.id, format!("Long-horizon research for {scenario}"))?;
        let parent_run = api.start_run(project.id, task.id, None)?;
        api.bind_run(project.id, parent_run.id, root.id)?;
        let mut research_scope = root.id;
        let mut tools = ApprovedTools::default();
        let retrieval = retrieval_tracker.clone();
        let search = Arc::new(SearchEntitiesTool {
            core: core.clone(),
            project: project.id,
            fail_once: (scenario == "tool-failure").then(|| Arc::new(AtomicBool::new(true))),
            retrieval: retrieval.clone(),
        });
        for inner in [
            search.clone() as Arc<dyn Tool>,
            Arc::new(ReadSourceTool {
                core: core.clone(),
                project: project.id,
                retrieval: retrieval.clone(),
            }) as Arc<dyn Tool>,
            Arc::new(RefineSearchTool {
                core: core.clone(),
                project: project.id,
                search,
            }) as Arc<dyn Tool>,
            Arc::new(RejectRetrievedSourcesTool {
                core: core.clone(),
                project: project.id,
                retrieval,
            }) as Arc<dyn Tool>,
        ] {
            tools.register(Arc::new(RecordedTool {
                core: core.clone(),
                inner,
                offloader: ArtifactOffloader {
                    workspace: ws.clone(),
                    threshold_bytes: 8_192,
                },
                version: "1".into(),
            }))?;
        }
        let mut lead = LeadAgent::new(
            core.agent_api("local-test-lead")?,
            local.clone(),
            selector(),
        )
        .with_tools(tools)
        .with_retrieval_protocol(retrieval_protocol());
        let mut active_child: Option<WorkNodeId>;
        let mut idle = 0u32;
        let mut dispatched_scopes = std::collections::BTreeSet::from([root.id]);
        let mut branch_creation_pending = scenario == "branching";
        if scenario == "experiment-proposal" {
            core.pause_research(
                project.id,
                None,
                CommandContext::human("eval", "exercise pause before Gate regression"),
            )?;
            core.resume_research(
                project.id,
                None,
                CommandContext::human("eval", "exercise resume before Gate regression"),
            )?;
            let lead_context = CommandContext {
                actor: Actor::LeadAgent("local-test-lead".into()),
                reason: "deterministic experiment proposal for mixed Gate coverage".into(),
                run_id: Some(parent_run.id),
                correlation_id: None,
                causation_id: None,
            };
            let submission = core.submit_proposal(
                project.id,
                ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
                    question_id: Some(question.id),
                    title: "Synthetic gated hypothesis".into(),
                    statement: "The synthetic intervention changes the measured outcome".into(),
                })),
                lead_context,
                "deterministic Gate capability assertion".into(),
            )?;
            let Submission::Gate(gate_id) = submission else {
                return Err(Error::Validation(
                    "Gate capability fixture bypassed the human gate".into(),
                ));
            };
            crate::test_human_resolve_gate_as(&core, project.id, gate_id, "eval")?;
        }
        for _ in 0..MAX_STEPS {
            let recovery_calls =
                core.runtime_state(project.id)?
                    .model_invocations
                    .values()
                    .map(|v| {
                        usize::from(v.metadata["structured_repair_attempted"] == true)
                            + usize::from(v.metadata["full_retry_attempted"] == true)
                            + usize::from(v.metadata["failure"].as_str().is_some_and(|e| {
                                e.contains("structured output recovery exhausted")
                            })) * 2
                    })
                    .sum::<usize>();
            if recovery_calls >= 5 {
                report.status = "budget_exceeded".into();
                report.error = Some("structured-output repair budget exhausted".into());
                break;
            }
            if report.lead_calls >= 40
                || report.tool_calls >= 60
                || report.meaningful_steps >= MAX_STEPS
            {
                report.status = "budget_exceeded".into();
                report.error = Some("long-horizon execution budget exhausted".into());
                break;
            }
            if started.elapsed().as_secs() >= config.budget.max_wall_time_seconds {
                report.status = "budget_exceeded".into();
                report.error = Some("long-horizon wall-time budget exhausted".into());
                break;
            }
            active_child = core
                .executable_child_work(project.id, research_scope)?
                .first()
                .map(|work| work.id);
            let current = active_child.unwrap_or(research_scope);
            let goal = if branch_creation_pending {
                "Create one child research Branch from the current checkpoint before continuing the research. Use a concise branch name that reflects a distinct analysis path. Do not finish the research in this step.".into()
            } else if scenario == "branching" {
                format!("Assess the research question: {question_text}. Review the current decision state, completed results and actual evidence before choosing the next action. Use existing work; justify independent replication or propose a revised objective when needed. Make an explicit termination decision when its execution conditions and your evidence assessment allow it. Two branches already exist; do not split again.")
            } else if scenario == "long-context" && report.offloaded_source_reads == 0 {
                let context_goal = if report.retrieval_calls == 0 {
                    "Use search_sources to find the detailed synthetic intervention report."
                } else {
                    "Use read_source on the detailed report returned by retrieval. Choose only a bounded handle offered by the current retrieval batch; do not copy or invent entity IDs."
                };
                format!("Research {scenario}. {context_goal} Complete only when the evidence task and its output contract are satisfied.")
            } else {
                format!("Research {scenario} using the local evidence pack. Use search_sources with a focused query for each atomic task, inspect retrieved candidates and their content, persist grounded evidence, and complete only after the required tool succeeds. Finish research only after the question is answered or explicitly inconclusive.")
            };
            if branch_creation_pending {
                lead = lead.with_process_action_constraint(vec![
                    anix_agent::ProcessActionKind::CreateBranch,
                ]);
            } else if scenario == "branching" && active_child.is_none() {
                lead = lead.with_process_action_constraint(vec![
                    anix_agent::ProcessActionKind::CreateWorkNode,
                    anix_agent::ProcessActionKind::FinishResearch,
                    anix_agent::ProcessActionKind::RepeatWorkNode,
                    anix_agent::ProcessActionKind::ProposeObjectiveRevision,
                    anix_agent::ProcessActionKind::ProposeWorkPlan,
                    anix_agent::ProcessActionKind::SwitchResearchBranch,
                ]);
            }
            let outcome = futures::executor::block_on(lead.run_step(
                project.id,
                goal,
                Some(EntityRef::Question(question.id)),
                Some(parent_run.id),
                current,
            ));
            if branch_creation_pending || scenario == "branching" {
                lead = lead.without_process_action_constraint();
            }
            report.lead_calls += 1;
            match outcome {
                Ok(step) => {
                    if let Some(output) = &step.tool_output {
                        if output.metadata["repeated_without_progress"] == true {
                            report.repeated_search_observations += 1;
                        }
                        if output.data["kind"] == "existing_work" {
                            report.duplicate_work_nodes += 1;
                        }
                        if matches!(step.decision, anix_agent::AgentDecision::UseTool { .. }) {
                            report.tool_calls += 1;
                        }
                        if output.metadata["stall_detected"] == true {
                            report.status = "stalled".into();
                            report.error = Some(
                                "SearchStallDetected: retrieval results remained unconsumed after bounded refinement".into(),
                            );
                            break;
                        }
                        if matches!(&step.decision, anix_agent::AgentDecision::UseTool { name, .. } if name == "read_source")
                            && output.metadata["reference_valid"] != false
                            && (output.metadata["retrieval_consumption"].is_object()
                                || output.metadata["source_content_read"].is_object())
                        {
                            report.meaningful_steps += 1;
                        }
                        let observation = serde_json::to_string(&output.data).unwrap_or_default();
                        let observation_limit =
                            if output.data["observation"]["observation_version"] == 2 {
                                6_000
                            } else {
                                400
                            };
                        let observation_kind = if output.metadata["retrieval_consumption"]
                            .is_object()
                            || output.metadata["source_content_read"].is_object()
                        {
                            "Source content observation"
                        } else {
                            "Tool observation"
                        };
                        api.write_process_memory(
                            project.id,
                            step.run_id,
                            current,
                            format!(
                                "{observation_kind}: {}",
                                observation
                                    .chars()
                                    .take(observation_limit)
                                    .collect::<String>()
                            ),
                        )?;
                    }
                    if let Some(snapshot) = core
                        .runtime_state(project.id)?
                        .context_snapshots
                        .values()
                        .max_by_key(|s| s.meta.created_at)
                    {
                        report.max_context_tokens =
                            report.max_context_tokens.max(snapshot.estimated_tokens);
                    }
                    if let Some(Submission::Gate(gate_id)) = step.submission.as_ref() {
                        report.gates += 1;
                        let _ =
                            crate::test_human_resolve_gate_as(&core, project.id, *gate_id, "eval")?;
                        api.finish(project.id, step.run_id, RunStatus::Completed)?;
                    }
                    match step.decision {
                        anix_agent::AgentDecision::Process {
                            action:
                                anix_agent::ProcessAction::CreateWorkNode { .. }
                                | anix_agent::ProcessAction::RepeatWorkNode { .. },
                            ..
                        } => {
                            let created = step
                                .tool_output
                                .as_ref()
                                .is_some_and(|output| output.data["created"] == true);
                            if created {
                                report.meaningful_steps += 1;
                            }
                            if created {
                                idle = 0;
                            } else {
                                idle += 1;
                            }
                        }
                        anix_agent::AgentDecision::Process {
                            action: anix_agent::ProcessAction::CompleteWorkNode { work_node_id, .. },
                            ..
                        } => {
                            report.meaningful_steps += 1;
                            if Some(work_node_id) == active_child {
                                idle = 0;
                            } else {
                                idle += 1;
                            }
                            let note = format!("Completed bounded source task {work_node_id}. Review persisted evidence and decide whether further work is needed.");
                            let current_run = core
                                .runtime_state(project.id)?
                                .runs
                                .values()
                                .filter(|r| r.status == RunStatus::Completed)
                                .max_by_key(|r| r.meta.created_at)
                                .map(|r| r.id);
                            if let Some(run) = current_run {
                                let _ =
                                    api.write_process_memory(project.id, run, research_scope, note);
                            }
                        }
                        anix_agent::AgentDecision::Process {
                            action: anix_agent::ProcessAction::ConcludeWorkInconclusive { .. },
                            ..
                        } => {
                            idle = 0;
                        }
                        anix_agent::AgentDecision::Process {
                            action: anix_agent::ProcessAction::CreateBranch { .. },
                            ..
                        } => {
                            report.meaningful_steps += 1;
                            branch_creation_pending = false;
                        }
                        anix_agent::AgentDecision::Process {
                            action: anix_agent::ProcessAction::SwitchResearchBranch { .. },
                            ..
                        } => {
                            let target = step
                                .tool_output
                                .as_ref()
                                .and_then(|output| output.data.get("selected_work_node_id"))
                                .cloned()
                                .ok_or_else(|| {
                                    Error::Validation("missing selected branch scope".into())
                                })?;
                            research_scope = serde_json::from_value(target)
                                .map_err(|error| Error::Validation(error.to_string()))?;
                            // First dispatch to a new executable research scope is
                            // execution progress, not new evidence. Revisiting scopes
                            // cannot continually reset the no-progress guard.
                            if dispatched_scopes.insert(research_scope) {
                                idle = 0;
                            } else {
                                idle += 1;
                            }
                        }
                        anix_agent::AgentDecision::Process {
                            action:
                                anix_agent::ProcessAction::FinishResearch {
                                    outcome, summary, ..
                                },
                            ..
                        } => {
                            if research_scope != root.id
                                && outcome != anix_agent::ResearchOutcome::Blocked
                            {
                                let scopes =
                                    api.research_branch_scopes(project.id, research_scope)?;
                                research_scope = scopes
                                    .iter()
                                    .find(|scope| scope.active)
                                    .and_then(|scope| scope.parent_work_node_id)
                                    .unwrap_or(root.id);
                                report.meaningful_steps += 1;
                                idle = 0;
                                continue;
                            }
                            report.status = match outcome {
                                anix_agent::ResearchOutcome::Completed => "completed",
                                anix_agent::ResearchOutcome::Inconclusive => "inconclusive",
                                anix_agent::ResearchOutcome::Blocked => "blocked",
                            }
                            .into();
                            if outcome == anix_agent::ResearchOutcome::Blocked {
                                report.error = Some(format!("Lead reported blocked: {summary}"));
                            }
                            break;
                        }
                        anix_agent::AgentDecision::Process {
                            action:
                                anix_agent::ProcessAction::ProposeWorkPlan { .. }
                                | anix_agent::ProcessAction::ProposeObjectiveRevision { .. },
                            ..
                        } => {
                            let approved = if let Some(Submission::Gate(id)) = &step.submission {
                                core.runtime_state(project.id)?
                                    .gates
                                    .get(id)
                                    .is_some_and(|gate| gate.status == GateStatus::Approved)
                            } else {
                                false
                            };
                            if approved {
                                report.meaningful_steps += 1;
                                idle = 0;
                            } else {
                                idle += 1;
                            }
                        }
                        anix_agent::AgentDecision::UseTool { .. } => {
                            if step.tool_output.as_ref().is_some_and(|output| {
                                output.metadata["repeated_without_progress"] == true
                                    || output.metadata["no_progress"] == true
                            }) {
                                idle += 1;
                            } else {
                                idle = 0;
                            }
                        }
                        _ => {
                            idle += 1;
                        }
                    }
                }
                Err(e) => {
                    if e.to_string().contains("PendingRetrievalMustBeResolved") {
                        report.rejected_plain_search_attempts += 1;
                    }
                    report.error = Some(e.to_string());
                    idle += 1;
                }
            }
            let observed_state = core.runtime_state(project.id)?;
            report.retrieval_calls = observed_state
                .tool_invocations
                .values()
                .filter(|invocation| invocation.tool == "search_sources")
                .count();
            update_retrieval_telemetry(&observed_state, &mut report);
            report.offloaded_source_reads = observed_state
                .tool_invocations
                .values()
                .filter(|invocation| {
                    invocation.tool == "read_source"
                        && invocation.output_artifacts.iter().any(|id| {
                            observed_state.artifacts.get(id).is_some_and(|artifact| {
                                artifact.name == "offloaded tool output"
                                    && artifact.metadata["summary"].as_str().is_some_and(
                                        |summary| summary.starts_with("Output from read_source;"),
                                    )
                            })
                        })
                })
                .count();
            if idle >= 4 {
                report.status = "blocked".into();
                break;
            }
        }
        // Drain pending test gates and close all open executions before the
        // terminal audit. This makes a model failure recoverable and auditable.
        let before_cleanup = core.runtime_state(project.id)?;
        for gate in before_cleanup
            .gates
            .values()
            .filter(|g| g.status == GateStatus::Pending)
        {
            let _ = crate::test_human_resolve_gate_as(&core, project.id, gate.id, "eval")?;
        }
        let mut active_runs: Vec<_> = core
            .runtime_state(project.id)?
            .runs
            .values()
            .filter(|r| r.status == RunStatus::Running)
            .cloned()
            .collect();
        active_runs.sort_by_key(|r| std::cmp::Reverse(r.meta.created_at));
        for run in active_runs {
            let terminal = if report.status == "completed" {
                RunStatus::Completed
            } else {
                RunStatus::Failed
            };
            let _ = api.finish(project.id, run.id, terminal);
        }
        let state_before_terminal = core.runtime_state(project.id)?;
        if report.status != "completed" {
            if let Some(root_now) = state_before_terminal.work_nodes.get(&root.id) {
                if root_now.status == WorkStatus::Running {
                    let _ = core.transition_work(
                        project.id,
                        root.id,
                        root_now.meta.version,
                        WorkStatus::Failed,
                        CommandContext::human(
                            "eval",
                            "fail safely after bounded long-horizon recovery",
                        ),
                    );
                }
            }
        }
        let state = core.runtime_state(project.id)?;
        report.work_nodes = state.work_nodes.len();
        report.branches = state.branches.len();
        report.gates = state.gates.len();
        report.checkpoints = state.checkpoints.len();
        report.retrieval_calls = state
            .tool_invocations
            .values()
            .filter(|invocation| invocation.tool == "search_sources")
            .count();
        update_retrieval_telemetry(&state, &mut report);
        report.offloaded_source_reads =
            state
                .tool_invocations
                .values()
                .filter(|invocation| {
                    invocation.tool == "read_source"
                        && invocation.output_artifacts.iter().any(|id| {
                            state.artifacts.get(id).is_some_and(|artifact| {
                                artifact.name == "offloaded tool output"
                                    && artifact.metadata["summary"].as_str().is_some_and(
                                        |summary| summary.starts_with("Output from read_source;"),
                                    )
                            })
                        })
                })
                .count();
        recovery_signature = Some(RecoverySignature {
            succeeded_work: state
                .work_nodes
                .values()
                .filter(|work| work.status == WorkStatus::Succeeded)
                .map(|work| work.id.to_string())
                .collect(),
            bindings: state
                .execution_bindings
                .keys()
                .map(ToString::to_string)
                .collect(),
            artifacts: state.artifacts.keys().map(ToString::to_string).collect(),
            branches: state.branches.keys().map(ToString::to_string).collect(),
        });
        report.audit = audit_project(&state);
        report.tool_failures = state
            .tool_invocations
            .values()
            .filter(|invocation| {
                invocation.status == InvocationStatus::Failed
                    && invocation.error_code.as_deref() == Some("tool_failure")
            })
            .count();
        report.branch_capability = branch_capability(&state, root.id);
        if let Some(message) = capability_assertion_failure(
            scenario,
            report.branches,
            report.gates,
            report.retrieval_calls,
            report.offloaded_source_reads,
            report.tool_failures,
            &report.branch_capability,
        ) {
            report.capability_failure = Some(message.into());
            if report.status == "completed" {
                report.status = "capability_failed".into();
                report.error = Some(message.into());
            }
        }
        if report.status != "completed" && report.status != "inconclusive" && report.error.is_none()
        {
            report.error = Some("step budget exhausted or repeated no-progress decisions".into());
        }
        Ok(())
    })();
    if let Err(e) = result {
        report.error.get_or_insert_with(|| e.to_string());
    }
    if let Ok(retrieval) = retrieval_tracker.lock() {
        report.invalid_reference_count = retrieval.invalid_references;
        report.search_stall_count = retrieval.search_stalls;
    }
    drop(core);
    let reopened_core = SqliteStorage::open(&db_path)
        .ok()
        .map(|storage| Core::new(Arc::new(storage)));
    if let Some(reopened_core) = reopened_core.as_ref() {
        if let Ok(state) = reopened_core.runtime_state(report.project_id) {
            let recovered_signature = RecoverySignature {
                succeeded_work: state
                    .work_nodes
                    .values()
                    .filter(|work| work.status == WorkStatus::Succeeded)
                    .map(|work| work.id.to_string())
                    .collect::<BTreeSet<_>>(),
                bindings: state
                    .execution_bindings
                    .keys()
                    .map(ToString::to_string)
                    .collect::<BTreeSet<_>>(),
                artifacts: state.artifacts.keys().map(ToString::to_string).collect(),
                branches: state.branches.keys().map(ToString::to_string).collect(),
            };
            report.recovery_passed = recovery_signature
                .as_ref()
                .is_some_and(|before| *before == recovered_signature)
                && audit_project(&state).passed;
            report.work_nodes = state.work_nodes.len();
            report.branches = state.branches.len();
            report.gates = state.gates.len();
            report.checkpoints = state.checkpoints.len();
            report.audit = audit_project(&state);
            let mut lead: Vec<u64> = state
                .model_invocations
                .values()
                .filter_map(|v| v.latency_ms)
                .collect();
            lead.sort_unstable();
            report.lead_latency_p50_ms = percentile(&lead, 0.50);
            report.lead_latency_p95_ms = percentile(&lead, 0.95);
            let mut context_compile: Vec<u64> = state
                .model_invocations
                .values()
                .filter_map(|v| v.metadata["context_compile_ms"].as_u64())
                .collect();
            context_compile.sort_unstable();
            report.context_compile_p50_ms = percentile(&context_compile, 0.50);
            report.context_compile_p95_ms = percentile(&context_compile, 0.95);
            let mut tools: Vec<u64> = state
                .tool_invocations
                .values()
                .filter_map(|v| {
                    v.finished_at
                        .map(|end| (end - v.started_at).num_milliseconds().max(0) as u64)
                })
                .collect();
            tools.sort_unstable();
            report.tool_latency_p50_ms = percentile(&tools, 0.50);
            report.tool_latency_p95_ms = percentile(&tools, 0.95);
            let mut contexts: Vec<u64> = state
                .context_snapshots
                .values()
                .map(|v| v.estimated_tokens as u64)
                .collect();
            contexts.sort_unstable();
            report.context_tokens_p50 = percentile(&contexts, 0.50) as u32;
            report.context_tokens_p95 = percentile(&contexts, 0.95) as u32;
            report.micro_model_invocations = state
                .model_invocations
                .values()
                .filter(|v| v.provider != "local-test-lead")
                .count();
            report.repair_attempts =
                state
                    .model_invocations
                    .values()
                    .map(|v| {
                        usize::from(v.metadata["structured_repair_attempted"] == true)
                            + usize::from(v.metadata["failure"].as_str().is_some_and(|e| {
                                e.contains("structured output recovery exhausted")
                            })) * 2
                    })
                    .sum();
            report.repair_successes = state
                .model_invocations
                .values()
                .filter(|v| v.metadata["structured_repair_succeeded"] == true)
                .count();
            report.full_retries = state
                .model_invocations
                .values()
                .filter(|v| v.metadata["full_retry_attempted"] == true)
                .count();
            report.structured_failures = state
                .model_invocations
                .values()
                .filter(|v| {
                    v.status == InvocationStatus::Failed
                        && v.metadata["failure"]
                            .as_str()
                            .is_some_and(|e| e.contains("structured output"))
                })
                .count();
            report.strong_model_calls = state
                .model_invocations
                .values()
                .filter(|v| v.provider == "local-test-lead")
                .count();
            report.budget_failures = usize::from(
                report
                    .error
                    .as_deref()
                    .is_some_and(|error| error.to_lowercase().contains("budget")),
            );
            if report.status == "completed" && !report.recovery_passed {
                report.status = "recovery_failed".into();
                report.error = Some(
                "restart recovery assertion failed: persisted state changed after reopening Core"
                    .into(),
            );
            }
        }
    }
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    if let Some(reopened_core) = reopened_core.as_ref() {
        if let Ok(state) = reopened_core.runtime_state(report.project_id) {
            let lead_total: u64 = state
                .model_invocations
                .values()
                .filter_map(|v| v.latency_ms)
                .sum();
            let tool_total: u64 = state
                .tool_invocations
                .values()
                .filter_map(|v| {
                    v.finished_at
                        .map(|end| (end - v.started_at).num_milliseconds().max(0) as u64)
                })
                .sum();
            report.core_overhead_estimate_ms = report
                .elapsed_ms
                .saturating_sub(lead_total.saturating_add(tool_total));
        }
    }
    let out = PathBuf::from(&config.workspace)
        .join("eval-results")
        .join("long-horizon")
        .join("runs")
        .join(Utc::now().format("%Y%m%dT%H%M%S%3f").to_string())
        .join(format!("{scenario}-{seed:06}"));
    std::fs::create_dir_all(&out).map_err(|e| Error::Storage(e.to_string()))?;
    if let Some(reopened_core) = reopened_core.as_ref() {
        let state = reopened_core.runtime_state(report.project_id)?;
        let mut lines = Vec::new();
        let mut traced_invocations: Vec<_> = state
            .model_invocations
            .values()
            .filter(|invocation| invocation.metadata["diagnostic_trace"].is_object())
            .collect();
        traced_invocations.sort_by_key(|invocation| invocation.started_at);
        for invocation in traced_invocations {
            let binding = state.execution_bindings.get(&invocation.run_id);
            let branch = binding.and_then(|binding| state.branches.get(&binding.branch_id));
            let related_tools: Vec<_> = state
                .tool_invocations
                .values()
                .filter(|tool| tool.run_id == invocation.run_id)
                .collect();
            let context_snapshot = state.context_snapshots.get(&invocation.context_snapshot_id);
            let work_node = state.work_nodes.get(&invocation.work_node_id);
            let trace = json!({
                "model_invocation": invocation,
                "trace": invocation.metadata["diagnostic_trace"],
                "context_snapshot": context_snapshot,
                "work_node": work_node,
                "branch": branch,
                "tool_invocations": related_tools,
                "run_status": state.runs.get(&invocation.run_id).map(|run| run.status),
                "checkpoints": state.checkpoints.values().filter(|checkpoint| checkpoint.run_id == Some(invocation.run_id)).collect::<Vec<_>>(),
            });
            lines
                .push(serde_json::to_string(&trace).map_err(|e| Error::Validation(e.to_string()))?);
        }
        if !lines.is_empty() {
            let trace_path = out.join("lead-decision-trace.jsonl");
            let trace_relative = trace_path
                .strip_prefix(&config.workspace)
                .map_err(|e| Error::Storage(e.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let stored = ws.write_new(&trace_relative, (lines.join("\n") + "\n").as_bytes())?;
            let artifact = reopened_core.register_artifact(
                report.project_id,
                NewArtifact {
                    name: "Lead decision diagnostic trace".into(),
                    artifact_kind: ArtifactKind::Document,
                    path: stored.path,
                    content_hash: stored.content_hash,
                    byte_size: stored.byte_size,
                    metadata: json!({
                        "schema_version":1,
                        "diagnostic_trace":true,
                        "trace_version":1,
                        "model_invocation_ids":state.model_invocations.values()
                            .filter(|invocation| invocation.metadata["diagnostic_trace"].is_object())
                            .map(|invocation| invocation.id).collect::<Vec<_>>(),
                    }),
                },
                CommandContext::human("eval", "Persist bounded Lead decision diagnostic trace"),
            )?;
            let stored_file = StoredFile {
                path: artifact.path.clone(),
                content_hash: artifact.content_hash.clone(),
                byte_size: artifact.byte_size,
            };
            let trace_file_verified = ws.verify(&stored_file)?;
            let trace_reopened = Core::new(Arc::new(SqliteStorage::open(&db_path)?));
            let recovered_trace_state = trace_reopened.runtime_state(report.project_id)?;
            report.recovery_passed &=
                trace_file_verified && recovered_trace_state.artifacts.contains_key(&artifact.id);
            report.audit = audit_project(&recovered_trace_state);
            report.diagnostic_trace_path = Some(trace_path.to_string_lossy().into_owned());
            report.diagnostic_trace_artifact_id = Some(artifact.id.to_string());
        }
    }
    report.report_path = out.join("report.json").to_string_lossy().into_owned();
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| Error::Validation(e.to_string()))?,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    Ok(report)
}

pub fn run_campaign(
    config: &HarnessConfig,
    count: usize,
    seed: u64,
    stage: &str,
) -> Result<Vec<LongHorizonReport>> {
    if count != 10 && count != 20 {
        return Err(Error::Validation(
            "long-horizon campaign count must be 10 or 20".into(),
        ));
    }
    if !matches!(
        (count, stage),
        (10, "initial" | "regression") | (20, "variation")
    ) {
        return Err(Error::Validation(
            "campaign stages are initial/regression for 10 runs and variation for 20 runs".into(),
        ));
    }
    let primary_path =
        PathBuf::from(&config.workspace).join("eval-results/long-horizon/primary-10.json");
    if stage == "variation" {
        let previous: Vec<LongHorizonReport> = serde_json::from_slice(&std::fs::read(&primary_path).map_err(|_| Error::Validation("20-run variation is gated: first run and same-10 regression summary are required".into()))?)
            .map_err(|e| Error::Validation(format!("invalid primary campaign summary: {e}")))?;
        let completed = previous.iter().filter(|r| r.status == "completed").count();
        let intact = previous.len() == 10 && previous.iter().all(|r| r.audit.passed);
        if !intact || completed < 8 {
            return Err(Error::Validation("20-run variation is gated until the 10-run regression has intact audits and at least 80% completion".into()));
        }
    }
    let p = TestLeadConfig {
        endpoint: config.endpoint.clone(),
        model_name: config.model_name.clone(),
        context_size: config.context_size,
        temperature: config.temperature,
        timeout_seconds: config.timeout_seconds,
        capabilities: config.capabilities.clone(),
        diagnostic_traces: false,
        api_key_env: None,
        reasoning_effort: None,
    };
    let provider: Arc<dyn ModelProvider> = Arc::new(OpenAiCompatibleModelProvider::new(p)?);
    let mut reports = Vec::with_capacity(count);
    for i in 0..count {
        let scenario = crate::ScenarioKind::ALL[i % crate::ScenarioKind::ALL.len()].name();
        reports.push(run_one(
            config,
            scenario,
            seed.wrapping_add(i as u64),
            provider.clone(),
        )?);
    }
    let out_dir = PathBuf::from(&config.workspace).join("eval-results/long-horizon");
    std::fs::create_dir_all(&out_dir).map_err(|e| Error::Storage(e.to_string()))?;
    if stage == "regression" {
        std::fs::write(
            &primary_path,
            serde_json::to_vec_pretty(&reports).map_err(|e| Error::Validation(e.to_string()))?,
        )
        .map_err(|e| Error::Storage(e.to_string()))?;
    }
    let campaign_path = out_dir.join(format!("{stage}-{seed}-{count}.json"));
    std::fs::write(
        &campaign_path,
        serde_json::to_vec_pretty(&reports).map_err(|e| Error::Validation(e.to_string()))?,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    let completed = reports.iter().filter(|r| r.status == "completed").count();
    let intact = reports.iter().filter(|r| r.audit.passed).count();
    let recovered = reports.iter().filter(|r| r.recovery_passed).count();
    let branch_cases: Vec<_> = reports
        .iter()
        .filter(|r| r.scenario == "branching")
        .collect();
    let branch_passed = branch_cases
        .iter()
        .filter(|r| {
            r.status == "completed"
                && r.branches >= 2
                && r.branch_capability.completed_evidence_branches >= 2
                && r.branch_capability.reconciled
        })
        .count();
    let context_cases: Vec<_> = reports
        .iter()
        .filter(|r| r.scenario == "long-context")
        .collect();
    let context_passed = context_cases
        .iter()
        .filter(|r| r.retrieval_calls > 0 && r.offloaded_source_reads > 0)
        .count();
    let capability_failures = reports
        .iter()
        .filter(|r| matches!(r.status.as_str(), "capability_failed" | "recovery_failed"))
        .count();
    let tool_failures = reports.iter().map(|r| r.tool_failures).sum::<usize>();
    let text = format!("# Long-horizon campaign\n\n- Requested researches: {count}\n- Completed: {completed}\n- Passing integrity audits: {intact}\n- Recovery preserved: {recovered}/{count}\n- Branch capability: {branch_passed}/{}\n- Long-context retrieval/offload capability: {context_passed}/{}\n- Injected tool failures recorded: {tool_failures}\n- Capability/recovery failures: {capability_failures}\n- Seed: {seed}\n- Meaningful steps p50/p95: {}/{}\n- Lead calls p50/p95: {}/{}\n- Tool calls p50/p95: {}/{}\n- Context token p50/p95 across researches: {}/{}\n- Research elapsed ms p50/p95: {}/{}\n- Micro-model invocations: {}\n- Gate records: {}\n- Checkpoints: {}\n- Campaign JSON: {}\n", branch_cases.len(), context_cases.len(), report_percentile(&reports, |r| r.meaningful_steps as u64, 0.50), report_percentile(&reports, |r| r.meaningful_steps as u64, 0.95), report_percentile(&reports, |r| r.lead_calls as u64, 0.50), report_percentile(&reports, |r| r.lead_calls as u64, 0.95), report_percentile(&reports, |r| r.tool_calls as u64, 0.50), report_percentile(&reports, |r| r.tool_calls as u64, 0.95), report_percentile(&reports, |r| r.context_tokens_p95 as u64, 0.50), report_percentile(&reports, |r| r.context_tokens_p95 as u64, 0.95), report_percentile(&reports, |r| r.elapsed_ms, 0.50), report_percentile(&reports, |r| r.elapsed_ms, 0.95), reports.iter().map(|r| r.micro_model_invocations).sum::<usize>(), reports.iter().map(|r| r.gates).sum::<usize>(), reports.iter().map(|r| r.checkpoints).sum::<usize>(), campaign_path.display());
    std::fs::write(out_dir.join(format!("{stage}-{seed}-{count}.md")), text)
        .map_err(|e| Error::Storage(e.to_string()))?;
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anix_agent::{
        AgentDecision, ModelFuture, ModelRequest, ModelResponse, ProcessAction, ResearchOutcome,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn two_empty_branch_records_do_not_establish_branch_research_capability() {
        assert!(
            capability_assertion_failure("branching", 2, 0, 1, 0, 0, &BranchCapability::default())
                .is_some(),
            "Branch rows without completed branch evidence and reconciliation must fail capability"
        );
    }

    #[test]
    fn mixed_capability_assertions_require_child_branch_and_offloaded_retrieval() {
        assert!(capability_assertion_failure(
            "branching",
            1,
            0,
            0,
            0,
            0,
            &BranchCapability::default()
        )
        .is_some());
        assert!(capability_assertion_failure(
            "branching",
            2,
            0,
            0,
            0,
            0,
            &BranchCapability {
                completed_evidence_branches: 2,
                reconciled: true
            }
        )
        .is_none());
        assert!(capability_assertion_failure(
            "long-context",
            1,
            0,
            1,
            0,
            0,
            &BranchCapability::default()
        )
        .is_some());
        assert!(capability_assertion_failure(
            "long-context",
            1,
            0,
            0,
            1,
            0,
            &BranchCapability::default()
        )
        .is_some());
        assert!(capability_assertion_failure(
            "long-context",
            1,
            0,
            1,
            1,
            0,
            &BranchCapability::default()
        )
        .is_none());
        assert!(capability_assertion_failure(
            "tool-failure",
            1,
            0,
            1,
            1,
            0,
            &BranchCapability::default()
        )
        .is_some());
        assert!(capability_assertion_failure(
            "tool-failure",
            1,
            0,
            1,
            1,
            1,
            &BranchCapability::default()
        )
        .is_none());
        assert!(capability_assertion_failure(
            "experiment-proposal",
            1,
            0,
            0,
            0,
            0,
            &BranchCapability::default()
        )
        .is_some());
        assert!(capability_assertion_failure(
            "experiment-proposal",
            1,
            1,
            0,
            0,
            0,
            &BranchCapability::default()
        )
        .is_none());
    }

    #[test]
    fn model_facing_retrieval_tools_expose_semantics_without_identity_fields() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(temp.path()).unwrap();
        let core = Arc::new(Core::new(Arc::new(
            SqliteStorage::open(workspace.database_path().unwrap()).unwrap(),
        )));
        let project = core
            .projects()
            .create(
                "retrieval descriptor fixture".into(),
                String::new(),
                CommandContext::human("test", "create retrieval descriptor fixture"),
            )
            .unwrap();
        let retrieval = Arc::new(Mutex::new(RetrievalState::default()));
        let search = Arc::new(SearchEntitiesTool {
            core: core.clone(),
            project: project.id,
            fail_once: None,
            retrieval: retrieval.clone(),
        });
        let refine = RefineSearchTool {
            core: core.clone(),
            project: project.id,
            search,
        }
        .descriptor();
        let reject = RejectRetrievedSourcesTool {
            core: core.clone(),
            project: project.id,
            retrieval: retrieval.clone(),
        }
        .descriptor();
        let read = ReadSourceTool {
            core,
            project: project.id,
            retrieval,
        }
        .descriptor();
        assert!(refine.description.contains("reason") && refine.description.contains("query"));
        assert!(!refine.description.contains("previous_search_id"));
        assert!(!refine.description.contains("search_invocation_id"));
        assert!(reject.description.contains("reason"));
        assert!(!reject.description.contains("search_invocation_id"));
        assert!(read.description.contains("handle") && read.description.contains("S1"));
        assert!(read.description.contains("not verified evidence"));
        assert!(refine.description.contains("materially different query"));
        assert!(reject.description.contains("unread is not a reason"));
    }

    #[test]
    fn retrieval_observation_contains_objective_mapped_preview_and_unread_handle() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(temp.path()).unwrap();
        let core = Arc::new(Core::new(Arc::new(
            SqliteStorage::open(workspace.database_path().unwrap()).unwrap(),
        )));
        let project = core
            .projects()
            .create(
                "retrieval observation fixture".into(),
                String::new(),
                CommandContext::human("test", "create retrieval observation fixture"),
            )
            .unwrap();
        let question = core
            .questions()
            .add(
                project.id,
                "Which pathway explains the measured outcome?".into(),
                "Use local evidence".into(),
                CommandContext::human("test", "add retrieval fixture question"),
            )
            .unwrap();
        let branch = core
            .create_branch(
                project.id,
                "main".into(),
                None,
                CommandContext::human("test", "create retrieval fixture branch"),
            )
            .unwrap();
        let work = core
            .create_work(
                project.id,
                anix_core::NewWork {
                    branch_id: branch.id,
                    kind: "evidence".into(),
                    granularity: WorkGranularity::Atomic,
                    title: "Inspect pathway evidence".into(),
                    objective: "Compare source content with the active question.".into(),
                    acceptance_criteria: OutputContract {
                        required_entities: vec![EntityRef::Question(question.id)],
                        required_artifacts: vec![],
                        minimum_children: 0,
                    },
                    capability_requirements: vec!["tool:search_sources".into()],
                    context_policy_id: "bounded".into(),
                    budget: WorkBudget {
                        context_tokens: 4096,
                        max_model_calls: 10,
                        max_tool_calls: 10,
                    },
                },
                CommandContext::human("test", "create retrieval fixture work"),
            )
            .unwrap();
        let work = core
            .transition_work(
                project.id,
                work.id,
                work.meta.version,
                WorkStatus::Ready,
                CommandContext::human("test", "ready retrieval fixture work"),
            )
            .unwrap();
        let api = core.agent_api("observation-test-lead").unwrap();
        let task = api
            .create_task(project.id, "Read local evidence".into())
            .unwrap();
        let run = api.start_run(project.id, task.id, None).unwrap();
        api.bind_run(project.id, run.id, work.id).unwrap();
        let (snapshot, _) = api
            .compile_context(
                project.id,
                run.id,
                work.id,
                vec![EntityRef::Question(question.id)],
            )
            .unwrap();
        let source = core
            .sources()
            .add(
                project.id,
                anix_core::NewSource {
                    title: "Repository attachment".into(),
                    source_kind: SourceKind::Manual,
                    locator: "pack://observation/source".into(),
                    checksum: None,
                    metadata: json!({"schema_version":1}),
                },
                CommandContext::human("test", "add source fixture"),
            )
            .unwrap();
        core.papers()
            .add(
                project.id,
                anix_core::NewPaper {
                    title: "Pathway alpha study".into(),
                    authors: vec![],
                    publication_year: Some(2025),
                    doi: None,
                    abstract_text: "Measured outcome changes through pathway alpha.".into(),
                    source_ids: vec![source.id],
                    metadata: json!({"schema_version":1}),
                },
                CommandContext::human("test", "add paper fixture"),
            )
            .unwrap();
        let search = SearchEntitiesTool {
            core: core.clone(),
            project: project.id,
            fail_once: None,
            retrieval: Arc::new(Mutex::new(RetrievalState::default())),
        };
        let context = ToolContext {
            project_id: project.id,
            run_id: Some(run.id),
            context_snapshot_id: Some(snapshot.id),
            tool_invocation_id: None,
        };
        let output = search.perform_search("pathway alpha", &context).unwrap();
        let observation = &output.data["observation"];
        assert_eq!(
            observation["research_objective"],
            "Which pathway explains the measured outcome?"
        );
        assert_eq!(
            observation["current_work_objective"],
            "Compare source content with the active question."
        );
        assert_eq!(observation["candidates"][0]["handle"], "S1");
        assert_eq!(observation["candidates"][0]["title"], "Pathway alpha study");
        assert!(observation["candidates"][0]["preview"]
            .as_str()
            .unwrap()
            .contains("pathway alpha"));
        assert_eq!(observation["candidates"][0]["unread"], true);
        assert_eq!(observation["available_actions"][0], "read_source");
        assert_eq!(
            observation["unresolved_evidence_needs"][0],
            "Compare source content with the active question."
        );
        assert!(!observation.to_string().contains("source_id"));
    }

    #[test]
    fn repeated_refinement_does_not_reset_progress_guard() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(temp.path()).unwrap();
        let core = Arc::new(Core::new(Arc::new(
            SqliteStorage::open(workspace.database_path().unwrap()).unwrap(),
        )));
        let project = core
            .projects()
            .create(
                "stall guard".into(),
                String::new(),
                CommandContext::human("test", "create stall guard fixture"),
            )
            .unwrap();
        let source_id = SourceId::new();
        let retrieval = Arc::new(Mutex::new(RetrievalState {
            by_handle: [("S1".into(), source_id)].into_iter().collect(),
            pending: [source_id].into_iter().collect(),
            active_search_id: None,
            last_fingerprint: Some("previous query".into()),
            repeated_fingerprints: 1,
            searches_without_consumption: 2,
            has_unconsumed_retrieval: true,
            invalid_references: 0,
            search_stalls: 0,
        }));
        let search = SearchEntitiesTool {
            core,
            project: project.id,
            fail_once: None,
            retrieval: retrieval.clone(),
        };
        let context = ToolContext {
            project_id: project.id,
            run_id: None,
            context_snapshot_id: None,
            tool_invocation_id: None,
        };
        let output = search
            .perform_search("materially different query", &context)
            .unwrap();
        assert_eq!(output.metadata["stall_detected"], true);
        let state = retrieval.lock().unwrap();
        assert_eq!(state.searches_without_consumption, 2);
        assert_eq!(state.search_stalls, 1);
    }

    #[test]
    fn identical_refinement_query_is_rejected_deterministically() {
        assert!(matches!(
            validate_refinement_query("  Evidence   search ", "evidence search"),
            Err(Error::InvalidTransition(message)) if message == "NoProgressSearchRefinement"
        ));
        assert!(validate_refinement_query("evidence search recent", "evidence search").is_ok());
    }

    #[test]
    fn tool_failure_scenario_injects_one_failure_then_allows_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(temp.path()).unwrap();
        let core = Arc::new(Core::new(Arc::new(
            SqliteStorage::open(workspace.database_path().unwrap()).unwrap(),
        )));
        let project = core
            .projects()
            .create(
                "tool failure fixture".into(),
                String::new(),
                CommandContext::human("test", "create tool-failure fixture"),
            )
            .unwrap();
        let tool = SearchEntitiesTool {
            core,
            project: project.id,
            fail_once: Some(Arc::new(AtomicBool::new(true))),
            retrieval: Arc::new(Mutex::new(RetrievalState::default())),
        };
        let input = || ToolInput {
            schema_version: 1,
            parameters: json!({"query":"temporary retrieval failure"}),
        };
        let context = ToolContext {
            project_id: project.id,
            run_id: None,
            context_snapshot_id: None,
            tool_invocation_id: None,
        };
        assert!(matches!(
            futures::executor::block_on(tool.execute(input(), context.clone())),
            Err(Error::Tool(message)) if message == "injected one-shot search failure"
        ));
        assert!(futures::executor::block_on(tool.execute(input(), context.clone())).is_ok());
        let refined = || ToolInput {
            schema_version: 1,
            parameters: json!({"query":"temporary retrieval failure refined"}),
        };
        let second = futures::executor::block_on(tool.execute(refined(), context.clone())).unwrap();
        assert_ne!(second.metadata["stall_detected"], true);
        let third = futures::executor::block_on(tool.execute(refined(), context.clone())).unwrap();
        assert_eq!(third.metadata["stall_detected"], true);
        let forged = SourceId::new().to_string();
        let read = ReadSourceTool {
            core: tool.core.clone(),
            project: project.id,
            retrieval: tool.retrieval.clone(),
        };
        let rejected = futures::executor::block_on(read.execute(
            ToolInput {
                schema_version: 1,
                parameters: json!({"handle":forged}),
            },
            context,
        ))
        .unwrap_err();
        assert!(
            matches!(rejected, Error::Validation(message) if message == "invalid source reference")
        );
    }

    struct ScriptedLongLead {
        call: AtomicUsize,
    }
    struct FailingLead;
    impl ModelProvider for FailingLead {
        fn generate(&self, _: ModelRequest) -> ModelFuture<'_> {
            Box::pin(async { Err(Error::Model("injected deterministic timeout".into())) })
        }
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
    }
    struct GatedLead;
    #[test]
    fn supplied_source_is_read_without_search_through_the_lead_boundary() {
        use anix_agent::{AgentDecision, FakeModelProvider, ModelResponse};
        let temp = tempfile::tempdir().unwrap();
        let workspace = Arc::new(Workspace::open(temp.path()).unwrap());
        let core = Arc::new(Core::new(Arc::new(
            SqliteStorage::open(workspace.database_path().unwrap()).unwrap(),
        )));
        let human = || CommandContext::human("test", "explicit source fixture");
        let project = core
            .create_project(
                "Supplied source".into(),
                "No search required".into(),
                human(),
            )
            .unwrap();
        let source = core
            .add_source(
                project.id,
                NewSource {
                    title: "Provided study".into(),
                    source_kind: SourceKind::Manual,
                    locator: "pack://provided".into(),
                    checksum: None,
                    metadata: json!({}),
                },
                human(),
            )
            .unwrap();
        core.add_paper(
            project.id,
            NewPaper {
                title: "Provided study".into(),
                authors: vec![],
                publication_year: None,
                doi: None,
                abstract_text: "The provided source reports an uncertain measured effect.".into(),
                source_ids: vec![source.id],
                metadata: json!({}),
            },
            human(),
        )
        .unwrap();
        let branch = core
            .create_branch(project.id, "main".into(), None, human())
            .unwrap();
        let node = core
            .create_work(
                project.id,
                anix_core::NewWork {
                    branch_id: branch.id,
                    kind: "evidence".into(),
                    granularity: WorkGranularity::Atomic,
                    title: "Read supplied source".into(),
                    objective: "Inspect the source supplied by the human".into(),
                    acceptance_criteria: OutputContract {
                        required_entities: vec![EntityRef::Source(source.id)],
                        required_artifacts: vec![],
                        minimum_children: 0,
                    },
                    capability_requirements: vec!["tool:read_source".into()],
                    context_policy_id: "bounded-v1".into(),
                    budget: WorkBudget::default(),
                },
                human(),
            )
            .unwrap();
        core.transition_work(
            project.id,
            node.id,
            node.meta.version,
            WorkStatus::Ready,
            human(),
        )
        .unwrap();
        let provider = Arc::new(FakeModelProvider::new(vec![
            ModelResponse {
                schema_version: 1,
                decision: AgentDecision::UseTool {
                    name: "read_source".into(),
                    input: ToolInput {
                        schema_version: 1,
                        parameters: json!({"handle":"E1","runtime_source_ref":"forged"}),
                    },
                },
                metadata: json!({}),
            },
            ModelResponse {
                schema_version: 1,
                decision: AgentDecision::Process {
                    action: ProcessAction::CompleteWorkNode {
                        work_node_id: node.id,
                        summary: Some("The local source was inspected; its reported observation requires scientific review.".into()),
                    },
                    rationale: "Source inspected".into(),
                    expected_result: None,
                },
                metadata: json!({}),
            },
        ]));
        let mut tools = ApprovedTools::default();
        tools
            .register(Arc::new(RecordedTool {
                core: core.clone(),
                inner: Arc::new(ReadSourceTool {
                    core: core.clone(),
                    project: project.id,
                    retrieval: Arc::new(Mutex::new(RetrievalState::default())),
                }),
                offloader: ArtifactOffloader {
                    workspace,
                    threshold_bytes: 8192,
                },
                version: "1".into(),
            }))
            .unwrap();
        let lead = LeadAgent::new(
            core.agent_api("supplied-source-lead").unwrap(),
            provider.clone(),
            selector(),
        )
        .with_tools(tools)
        .with_retrieval_protocol(retrieval_protocol());
        let outcome = futures::executor::block_on(lead.run_step(
            project.id,
            "Inspect supplied content".into(),
            Some(EntityRef::Source(source.id)),
            None,
            node.id,
        ))
        .unwrap();
        assert_eq!(
            outcome.tool_output.unwrap().data["papers"][0]["abstract_text"],
            "The provided source reports an uncertain measured effect."
        );
        let requests = provider.requests().unwrap();
        assert!(!requests[0].task.retrieval_pending);
        assert_eq!(requests[0].task.retrieval_source_handles, ["E1"]);
        assert_eq!(
            requests[0]
                .tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["read_source"]
        );
        assert_eq!(
            requests[0].tool_results[0].output.data["supplied_sources"][0]["title"],
            "Provided study"
        );
        futures::executor::block_on(lead.run_step(
            project.id,
            "Complete supplied content inspection".into(),
            Some(EntityRef::Source(source.id)),
            None,
            node.id,
        ))
        .unwrap();
        let state = core.runtime_state(project.id).unwrap();
        assert_eq!(state.work_nodes[&node.id].status, WorkStatus::Succeeded);
        let result = state
            .process_memories
            .values()
            .find(|memory| {
                memory.work_node_id == Some(node.id)
                    && memory.content.starts_with("Work result observation: ")
            })
            .expect("completed task returns a substantive result");
        let payload: serde_json::Value = serde_json::from_str(
            result
                .content
                .strip_prefix("Work result observation: ")
                .unwrap(),
        )
        .unwrap();
        assert!(payload["summary"]
            .as_str()
            .unwrap()
            .contains("reported observation"));
        assert!(!payload["source_artifacts"].as_array().unwrap().is_empty());
        assert!(core
            .agent_api("foreign-result-writer")
            .unwrap()
            .record_work_result(
                project.id,
                outcome.run_id,
                node.id,
                "Forged replacement".into()
            )
            .is_err());
        assert_eq!(state.tool_invocations.len(), 1);
        assert!(state
            .tool_invocations
            .values()
            .all(|invocation| invocation.tool == "read_source"));
        core.add_constraint(
            project.id,
            "Exclude supplied source".into(),
            ConstraintKind::Hard,
            Some(source.id),
            None,
            human(),
        )
        .unwrap();
        assert!(core
            .agent_api("source-check")
            .unwrap()
            .work_source_handles(project.id, node.id)
            .unwrap()
            .is_empty());
    }
    struct ReadTaskLead {
        tasks: AtomicUsize,
        terminal: Option<ResearchOutcome>,
        requests: Mutex<Vec<ModelRequest>>,
    }
    struct RejectedSearchLead {
        step: AtomicUsize,
    }
    impl ModelProvider for RejectedSearchLead {
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
        fn generate(&self, _request: anix_agent::ModelRequest) -> anix_agent::ModelFuture<'_> {
            Box::pin(async move {
                use anix_agent::{AgentDecision, ProcessAction, ResearchOutcome};
                let step = self.step.fetch_add(1, Ordering::SeqCst);
                let decision=match step {
                    0=>AgentDecision::Process {action:ProcessAction::CreateWorkNode {title:"Assess mechanism".into(),objective:"Check whether local sources discriminate mechanisms".into(),required_tool:"read_source".into()},rationale:"bounded evidence question".into(),expected_result:None},
                    1|3=>AgentDecision::UseTool {name:"search_sources".into(),input:ToolInput {schema_version:1,parameters:json!({"query":"Study"})}},
                    2=>AgentDecision::UseTool {name:"reject_retrieved_sources".into(),input:ToolInput {schema_version:1,parameters:json!({"reason":"The supplied candidates cannot discriminate the proposed mechanisms"})}},
                    4=>AgentDecision::Process {action:ProcessAction::ConcludeWorkInconclusive {summary:"Recorded triage did not produce discriminating evidence; the repeated query adds no evidence".into()},rationale:"preserve unsuccessful result".into(),expected_result:None},
                    _=>AgentDecision::Process {action:ProcessAction::FinishResearch {reviewed_branches:vec![],outcome:ResearchOutcome::Blocked,summary:"Need additional discriminating material or Human-approved revision".into()},rationale:"no supported conclusion".into(),expected_result:None},
                };
                Ok(anix_agent::ModelResponse {
                    schema_version: 1,
                    decision,
                    metadata: json!({}),
                })
            })
        }
    }
    #[test]
    fn repeated_rejected_search_returns_existing_result_and_no_false_progress() {
        let temp = tempfile::tempdir().unwrap();
        let report = run_one(
            &HarnessConfig::local_default(temp.path()),
            "basic-evidence",
            19,
            Arc::new(RejectedSearchLead {
                step: AtomicUsize::new(0),
            }),
        )
        .unwrap();
        assert_eq!(report.status, "blocked", "{report:?}");
        assert_eq!(report.repeated_search_observations, 1);
        assert_eq!(report.read_source_calls, 0);
        assert!(report.audit.passed && report.recovery_passed);
        let workspace = Workspace::open(temp.path()).unwrap();
        let core = Core::new(Arc::new(
            SqliteStorage::open(workspace.database_path().unwrap()).unwrap(),
        ));
        assert!(core
            .runtime_state(report.project_id)
            .unwrap()
            .work_nodes
            .values()
            .any(|work| work.outcome == Some(ResearchOutcome::Inconclusive)
                && work.status == WorkStatus::Failed));
    }
    impl ModelProvider for ReadTaskLead {
        fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
            Box::pin(async move {
                self.requests.lock().unwrap().push(request.clone());
                let work = request.task.current_work.as_ref().unwrap();
                let action = if work.granularity == WorkGranularity::Composite {
                    let previous_tasks = self.tasks.fetch_add(1, Ordering::SeqCst);
                    if let Some(outcome) = self.terminal.filter(|_| previous_tasks > 0) {
                        ProcessAction::FinishResearch {
                            reviewed_branches: vec![],
                            outcome,
                            summary:
                                "Source content was inspected; uncertainty remains documented."
                                    .into(),
                        }
                    } else if self.terminal.is_none() {
                        ProcessAction::RepeatWorkNode {
                            title: "Independent verification".into(),
                            objective:
                                "Inspect local scenario source content for the research question"
                                    .into(),
                            required_tool: "read_source".into(),
                            reason: format!(
                                "Explicit independent verification attempt {previous_tasks}"
                            ),
                        }
                    } else {
                        ProcessAction::CreateWorkNode {
                            title: "Inspect evidence".into(),
                            objective:
                                "Inspect local scenario source content for the research question"
                                    .into(),
                            required_tool: "read_source".into(),
                        }
                    }
                } else if !request.task.retrieval_pending && request.task.required_tool_succeeded {
                    ProcessAction::CompleteWorkNode {
                        work_node_id: work.id,
                        summary: Some("The local scenario source content has been inspected; this is an unreviewed task result.".into()),
                    }
                } else {
                    let (name, parameters) =
                        if let Some(handle) = request.task.retrieval_source_handles.first() {
                            ("read_source", json!({"handle":handle}))
                        } else {
                            ("search_sources", json!({"query":"Study"}))
                        };
                    return Ok(ModelResponse {
                        schema_version: 1,
                        decision: AgentDecision::UseTool {
                            name: name.into(),
                            input: ToolInput {
                                schema_version: 1,
                                parameters,
                            },
                        },
                        metadata: json!({}),
                    });
                };
                Ok(ModelResponse {
                    schema_version: 1,
                    decision: AgentDecision::Process {
                        action,
                        rationale: "Use persisted evidence and contracts".into(),
                        expected_result: None,
                    },
                    metadata: json!({}),
                })
            })
        }
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
    }
    #[test]
    fn atomic_read_task_can_search_then_read_and_complete_through_real_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let provider = Arc::new(ReadTaskLead {
            tasks: AtomicUsize::new(0),
            terminal: Some(ResearchOutcome::Completed),
            requests: Mutex::new(vec![]),
        });
        let report = run_one(
            &HarnessConfig::local_default(temp.path()),
            "basic-evidence",
            30,
            provider.clone(),
        )
        .unwrap();
        assert_eq!(report.status, "completed", "{report:?}");
        assert!(report.consumed_batches > 0 && report.audit.passed && report.recovery_passed);
        let requests = provider.requests.lock().unwrap();
        let pre_read = &requests[1];
        assert!(pre_read.task.retrieval_source_handles.is_empty());
        assert_eq!(
            pre_read
                .tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["search_sources"]
        );
        let pending = &requests[2];
        assert!(!pending.task.retrieval_source_handles.is_empty());
        assert!(pending.tools.iter().any(|tool| tool.name == "read_source"));
        assert!(!pending
            .tools
            .iter()
            .any(|tool| tool.name == "search_sources"));
    }
    #[test]
    fn experiment_fixture_uses_project_owner_for_human_controls_and_gate() {
        let temp = tempfile::tempdir().unwrap();
        let provider = Arc::new(ReadTaskLead {
            tasks: AtomicUsize::new(0),
            terminal: Some(ResearchOutcome::Completed),
            requests: Mutex::new(vec![]),
        });
        let report = run_one(
            &HarnessConfig::local_default(temp.path()),
            "experiment-proposal",
            32,
            provider,
        )
        .unwrap();
        assert_eq!(report.status, "completed", "{report:?}");
        assert_eq!(report.gates, 1);
        assert!(report.consumed_batches > 0 && report.lead_calls > 0);
        assert!(report.audit.passed && report.recovery_passed);
    }
    #[test]
    fn blocked_and_inconclusive_lead_outcomes_are_not_counted_as_completed() {
        for (outcome, expected) in [
            (ResearchOutcome::Blocked, "blocked"),
            (ResearchOutcome::Inconclusive, "inconclusive"),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let provider = Arc::new(ReadTaskLead {
                tasks: AtomicUsize::new(0),
                terminal: Some(outcome),
                requests: Mutex::new(vec![]),
            });
            let report = run_one(
                &HarnessConfig::local_default(temp.path()),
                "basic-evidence",
                31,
                provider,
            )
            .unwrap();
            assert_eq!(report.status, expected, "{report:?}");
            assert!(report.audit.passed && report.recovery_passed);
        }
    }
    #[test]
    fn tool_and_step_counts_cannot_finish_research_without_lead_decision() {
        let temp = tempfile::tempdir().unwrap();
        let provider = Arc::new(ReadTaskLead {
            tasks: AtomicUsize::new(0),
            terminal: None,
            requests: Mutex::new(vec![]),
        });
        let report = run_one(
            &HarnessConfig::local_default(temp.path()),
            "basic-evidence",
            32,
            provider,
        )
        .unwrap();
        assert_ne!(report.status, "completed", "{report:?}");
        assert!(
            report.meaningful_steps >= 20 && report.tool_calls >= 5,
            "{report:?}"
        );
        assert!(report.audit.passed && report.recovery_passed);
    }

    struct DuplicateChildLead(ReadTaskLead);
    struct IndependentBranchLead(ReadTaskLead);
    struct ReferenceEvidenceLead(IndependentBranchLead);
    impl ModelProvider for ReferenceEvidenceLead {
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
        fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
            Box::pin(async move {
                let mut response = self.0.generate(request).await?;
                if let AgentDecision::Process {
                    action: ProcessAction::FinishResearch { summary, .. },
                    ..
                } = &mut response.decision
                {
                    *summary = "Study F describes association through pathway P; Study G describes association through pathway Q. Both are reported possible pathways. These synthetic observational sources do not establish unique causality or rank the mechanisms.".into();
                }
                Ok(response)
            })
        }
    }
    #[test]
    fn seed664_reference_reconciles_two_evidence_paths_without_inventing_unique_causality() {
        let temp = tempfile::tempdir().unwrap();
        let provider = Arc::new(ReferenceEvidenceLead(IndependentBranchLead(ReadTaskLead {
            tasks: AtomicUsize::new(0),
            terminal: Some(ResearchOutcome::Completed),
            requests: Mutex::new(vec![]),
        })));
        let report = run_one(
            &HarnessConfig::local_default(temp.path()),
            "branching",
            664,
            provider,
        )
        .unwrap();
        assert_eq!(report.status, "completed", "{report:?}");
        assert_eq!(report.branch_capability.completed_evidence_branches, 2);
        assert!(
            report.branch_capability.reconciled && report.audit.passed && report.recovery_passed
        );
        let ws = Workspace::open(temp.path()).unwrap();
        let reopened = Core::new(Arc::new(
            SqliteStorage::open(ws.database_path().unwrap()).unwrap(),
        ));
        let state = reopened.runtime_state(report.project_id).unwrap();
        assert!(state
            .events
            .iter()
            .any(|event| event.event_type == "ResearchTerminated"
                && event.payload["outcome"] == "completed"
                && event.payload["summary"]
                    .as_str()
                    .unwrap_or("")
                    .contains("do not establish unique causality")));
    }
    #[test]
    fn cloud_synthetic_opt_in_does_not_relax_default_local_boundary() {
        struct ExternalLead;
        impl ModelProvider for ExternalLead {
            fn generate(&self, _: ModelRequest) -> ModelFuture<'_> {
                Box::pin(async { panic!("denied provider must never be called") })
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let config = HarnessConfig::local_default(temp.path());
        assert!(run_one(&config, "branching", 664, Arc::new(ExternalLead)).is_err());
        assert!(
            run_one_cloud_synthetic(&config, "branching", 664, Arc::new(ExternalLead)).is_err()
        );
    }
    struct RecoveringBranchLead {
        inner: IndependentBranchLead,
        rejected_finishes: AtomicUsize,
    }
    struct SwitchingOnlyLead(IndependentBranchLead);
    impl ModelProvider for SwitchingOnlyLead {
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
        fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
            Box::pin(async move {
                if request.research_branches.len() < 2 {
                    return self.0.generate(request).await;
                }
                let target = request
                    .research_branches
                    .iter()
                    .find(|scope| !scope.active)
                    .unwrap();
                Ok(ModelResponse {
                    schema_version: 1,
                    decision: AgentDecision::Process {
                        action: ProcessAction::SwitchResearchBranch {
                            branch: target.handle.clone(),
                        },
                        rationale: "Switch scopes without producing evidence".into(),
                        expected_result: None,
                    },
                    metadata: json!({}),
                })
            })
        }
    }
    #[test]
    fn repeated_branch_switches_do_not_disable_the_stall_guard() {
        let temp = tempfile::tempdir().unwrap();
        let report = run_one(
            &HarnessConfig::local_default(temp.path()),
            "branching",
            47,
            Arc::new(SwitchingOnlyLead(IndependentBranchLead(ReadTaskLead {
                tasks: AtomicUsize::new(0),
                terminal: None,
                requests: Mutex::new(vec![]),
            }))),
        )
        .unwrap();
        assert_eq!(report.status, "blocked", "{report:?}");
        assert!(
            report.lead_calls <= 7 && report.meaningful_steps == 1,
            "{report:?}"
        );
        assert_eq!(report.branch_capability.completed_evidence_branches, 0);
        assert!(report.audit.passed && report.recovery_passed);
    }
    impl ModelProvider for RecoveringBranchLead {
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
        fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
            Box::pin(async move {
                let mut response = self.inner.generate(request).await?;
                if let AgentDecision::Process { action, .. } = &mut response.decision {
                    if let ProcessAction::SwitchResearchBranch { branch } = action {
                        if self.rejected_finishes.fetch_add(1, Ordering::SeqCst) < 3 {
                            *action = ProcessAction::FinishResearch {
                                outcome: ResearchOutcome::Completed,
                                summary: "Premature fixture finish before branch work".into(),
                                reviewed_branches: vec![branch.clone()],
                            };
                        }
                    }
                }
                Ok(response)
            })
        }
    }
    #[test]
    fn first_branch_dispatch_after_rejections_can_recover_without_false_research_progress() {
        let temp = tempfile::tempdir().unwrap();
        let report = run_one(
            &HarnessConfig::local_default(temp.path()),
            "branching",
            46,
            Arc::new(RecoveringBranchLead {
                inner: IndependentBranchLead(ReadTaskLead {
                    tasks: AtomicUsize::new(0),
                    terminal: Some(ResearchOutcome::Completed),
                    requests: Mutex::new(vec![]),
                }),
                rejected_finishes: AtomicUsize::new(0),
            }),
        )
        .unwrap();
        assert_eq!(report.status, "completed", "{report:?}");
        assert_eq!(report.branch_capability.completed_evidence_branches, 2);
        assert!(
            report.branch_capability.reconciled && report.audit.passed && report.recovery_passed
        );
    }
    impl ModelProvider for IndependentBranchLead {
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
        fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
            Box::pin(async move {
                let work = request.task.current_work.as_ref().unwrap();
                if work.granularity == WorkGranularity::Atomic {
                    return self.0.generate(request).await;
                }
                let state: serde_json::Value = request
                    .compiled_context
                    .as_ref()
                    .unwrap()
                    .process_notes
                    .iter()
                    .find_map(|note| note.strip_prefix("Decision state: "))
                    .map(|note| serde_json::from_str(note).unwrap())
                    .unwrap();
                let children: Vec<_> = request
                    .research_branches
                    .iter()
                    .filter(|scope| scope.parent_work_node_id == Some(work.id))
                    .collect();
                let action = if request
                    .allowed_process_actions
                    .as_ref()
                    .is_some_and(|actions| {
                        actions == &[anix_agent::ProcessActionKind::CreateBranch]
                    }) {
                    ProcessAction::CreateBranch {
                        name: "Independent verification path".into(),
                    }
                } else if state["child_count"] == 0 {
                    ProcessAction::CreateWorkNode {
                        title: "Inspect path evidence".into(),
                        objective: "Inspect local source content on this independent path".into(),
                        required_tool: "read_source".into(),
                    }
                } else if let Some(scope) = children.iter().find(|scope| scope.outcome.is_none()) {
                    ProcessAction::SwitchResearchBranch {
                        branch: scope.handle.clone(),
                    }
                } else {
                    ProcessAction::FinishResearch { outcome: ResearchOutcome::Completed, summary: "Supplied observations inspected independently; parent compared returned results. This fixture validates execution, not scientific truth.".into(), reviewed_branches: children.iter().map(|scope| scope.handle.clone()).collect() }
                };
                Ok(ModelResponse {
                    schema_version: 1,
                    decision: AgentDecision::Process {
                        action,
                        rationale: "Controlled branch execution and explicit reconciliation".into(),
                        expected_result: None,
                    },
                    metadata: json!({}),
                })
            })
        }
    }
    #[test]
    fn independent_branches_execute_return_results_and_reconcile_through_real_driver() {
        let temp = tempfile::tempdir().unwrap();
        let report = run_one(
            &HarnessConfig::local_default(temp.path()),
            "branching",
            45,
            Arc::new(IndependentBranchLead(ReadTaskLead {
                tasks: AtomicUsize::new(0),
                terminal: Some(ResearchOutcome::Completed),
                requests: Mutex::new(vec![]),
            })),
        )
        .unwrap();
        assert_eq!(report.status, "completed", "{report:?}");
        assert_eq!(report.branch_capability.completed_evidence_branches, 2);
        assert!(
            report.branch_capability.reconciled && report.audit.passed && report.recovery_passed
        );
    }
    impl ModelProvider for DuplicateChildLead {
        fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
            Box::pin(async move {
                let mut response = self.0.generate(request).await?;
                if let AgentDecision::Process { action, .. } = &mut response.decision {
                    if let ProcessAction::RepeatWorkNode {
                        title,
                        objective,
                        required_tool,
                        ..
                    } = action
                    {
                        *action = ProcessAction::CreateWorkNode {
                            title: title.clone(),
                            objective: objective.clone(),
                            required_tool: required_tool.clone(),
                        };
                    }
                }
                Ok(response)
            })
        }
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
    }
    #[test]
    fn reusing_completed_work_does_not_reset_the_stall_guard() {
        let temp = tempfile::tempdir().unwrap();
        let provider = Arc::new(DuplicateChildLead(ReadTaskLead {
            tasks: AtomicUsize::new(0),
            terminal: None,
            requests: Mutex::new(vec![]),
        }));
        let report = run_one(
            &HarnessConfig::local_default(temp.path()),
            "basic-evidence",
            42,
            provider,
        )
        .unwrap();
        assert_eq!(report.status, "blocked", "{report:?}");
        assert_eq!(report.duplicate_work_nodes, 4);
        assert!(report.lead_calls <= 12, "{report:?}");
        assert!(report.audit.passed && report.recovery_passed);
    }
    impl ModelProvider for GatedLead {
        fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
            Box::pin(async move {
                let question_id = match request.task.focus {
                    Some(EntityRef::Question(id)) => Some(id),
                    _ => None,
                };
                Ok(ModelResponse {
                    schema_version: 1,
                    decision: AgentDecision::Propose {
                        action: ProposedAction::CreateHypothesis(NewHypothesis {
                            question_id,
                            title: "Bounded synthetic hypothesis".into(),
                            statement: "The synthetic source pack needs further inspection.".into(),
                        }),
                        reason: "Create a safe hypothesis proposal for test-human review".into(),
                    },
                    metadata: json!({"input_tokens":32,"output_tokens":20}),
                })
            })
        }
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
    }
    impl ModelProvider for ScriptedLongLead {
        fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
            Box::pin(async move {
                let n = self.call.fetch_add(1, Ordering::SeqCst);
                let decision = if n >= 15 {
                    AgentDecision::Process { action: ProcessAction::FinishResearch {reviewed_branches:vec![], outcome: ResearchOutcome::Completed, summary: "Five source tasks and twenty meaningful persisted steps are complete.".into() }, rationale: "The campaign minimum and evidence review are complete".into(), expected_result: None }
                } else {
                    match n % 3 {
                        0 => AgentDecision::Process {
                            action: ProcessAction::CreateWorkNode {
                                title: format!("Inspect source {}", n / 3 + 1),
                                objective: "Search the local evidence corpus".into(),
                                required_tool: "search_sources".into(),
                            },
                            rationale: "Retrieve a bounded source set".into(),
                            expected_result: Some("Search output artifact".into()),
                        },
                        1 => AgentDecision::UseTool {
                            name: "search_sources".into(),
                            input: ToolInput {
                                schema_version: 1,
                                parameters: json!({"query":"scenario"}),
                            },
                        },
                        _ => AgentDecision::Process {
                            action: ProcessAction::CompleteWorkNode {
                                work_node_id: request
                                    .task
                                    .current_work
                                    .as_ref()
                                    .expect("current atomic work")
                                    .id,
                                summary: None,
                            },
                            rationale: "The required search tool succeeded".into(),
                            expected_result: None,
                        },
                    }
                };
                Ok(ModelResponse {
                    schema_version: 1,
                    decision,
                    metadata: json!({"input_tokens":64,"output_tokens":24}),
                })
            })
        }
        fn locality(&self) -> ProviderLocality {
            ProviderLocality::Local
        }
    }

    #[test]
    fn pending_retrieval_rejects_repeated_plain_search() {
        let temp = tempfile::tempdir().unwrap();
        let config = HarnessConfig::local_default(temp.path());
        let provider: Arc<dyn ModelProvider> = Arc::new(ScriptedLongLead {
            call: AtomicUsize::new(0),
        });
        let report = run_one(&config, "basic-evidence", 22, provider).unwrap();
        assert_eq!(report.status, "blocked", "{report:?}");
        assert_eq!(report.search_stall_count, 0);
        assert_eq!(report.retrieval_calls, 1);
        assert!(report
            .error
            .as_deref()
            .is_some_and(|e| e.contains("PendingRetrievalMustBeResolved")));
        assert!(report.lead_calls <= 10);
        assert!(report.tool_calls <= 60);
        assert!(report.checkpoints >= report.lead_calls as usize);
        assert!(report.context_tokens_p95 <= 4096);
        assert!(report.repair_attempts <= 5);
        assert!(report.recovery_passed);
        assert!(report.audit.passed, "{:?}", report.audit.violations);

        let workspace = Workspace::open(temp.path()).unwrap();
        let reopened = Core::new(Arc::new(
            SqliteStorage::open(workspace.database_path().unwrap()).unwrap(),
        ));
        let state = reopened.runtime_state(report.project_id).unwrap();
        let successful_searches: Vec<_> = state
            .tool_invocations
            .values()
            .filter(|invocation| {
                invocation.tool == "search_sources"
                    && invocation.status == InvocationStatus::Succeeded
            })
            .collect();
        assert_eq!(successful_searches.len(), 1, "{successful_searches:?}");
        let invocation = successful_searches[0];
        let binding = state.execution_bindings.get(&invocation.run_id).unwrap();
        let tools = retrieval_tool_names(&retrieval_protocol());
        let scope = RetrievalScope {
            anchor_run_id: Some(invocation.run_id),
            work_node_id: binding.work_node_id,
            branch_id: binding.branch_id,
        };
        let batches = derive_retrieval_batches(&state, scope, &tools).unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].status, RetrievalBatchStatus::Pending);
        let other_branch = derive_retrieval_batches(
            &state,
            RetrievalScope {
                branch_id: BranchId::new(),
                ..scope
            },
            &tools,
        )
        .unwrap();
        assert!(other_branch.is_empty());
    }

    #[test]
    fn repeated_model_failure_closes_runs_and_leaves_auditable_state() {
        let temp = tempfile::tempdir().unwrap();
        let config = HarnessConfig::local_default(temp.path());
        let report = run_one(&config, "sparse-evidence", 23, Arc::new(FailingLead)).unwrap();
        assert_eq!(report.status, "blocked");
        assert!(report.audit.passed, "{:?}", report.audit.violations);
        assert_eq!(report.lead_calls, 4);
        let core = Core::new(Arc::new(
            SqliteStorage::open(temp.path().join(".anix/state.sqlite")).unwrap(),
        ));
        let state = core.runtime_state(report.project_id).unwrap();
        assert!(state
            .runs
            .values()
            .all(|run| run.status != RunStatus::Running));
        assert!(state
            .work_nodes
            .values()
            .all(|work| work.status != WorkStatus::Running));
    }

    #[test]
    fn composite_step_rejects_gate_proposals_and_passes_audit() {
        let temp = tempfile::tempdir().unwrap();
        let config = HarnessConfig::local_default(temp.path());
        let report = run_one(&config, "sparse-evidence", 24, Arc::new(GatedLead)).unwrap();
        assert_eq!(report.status, "blocked");
        assert!(report.audit.passed, "{:?}", report.audit.violations);
        assert_eq!(report.gates, 0);
        let core = Core::new(Arc::new(
            SqliteStorage::open(temp.path().join(".anix/state.sqlite")).unwrap(),
        ));
        let state = core.runtime_state(report.project_id).unwrap();
        assert!(state
            .gates
            .values()
            .all(|gate| gate.status != GateStatus::Pending));
        assert!(state
            .runs
            .values()
            .all(|run| run.status != RunStatus::Running));
    }
}

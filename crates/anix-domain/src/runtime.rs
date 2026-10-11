//! Runtime records deliberately contain no provider SDK or orchestration framework types.
use crate::*;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeSet;

macro_rules! enumeration {
    ($name:ident: $($variant:ident),+) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
        #[serde(rename_all="snake_case")]
        pub enum $name { $($variant),+ }
    };
}
enumeration!(WorkGranularity: Composite, Atomic);
enumeration!(WorkStatus: Proposed, Ready, Running, Blocked, WaitingForHuman, Succeeded, Failed, Cancelled, Superseded);
enumeration!(WorkEdgeKind: Contains, DependsOn, Blocks, BranchesFrom, Refines, Supersedes);
enumeration!(CheckpointBoundary: PlanCreated, WorkCompleted, BeforeGate, AfterGate, BeforeSideEffect, AfterSideEffect, Suspended);
enumeration!(MemoryNamespace: ScientificKnowledge, ProcessMemory, OperationalMemory, UserPreferences);
enumeration!(InvocationStatus: Running, Succeeded, Failed, Cancelled);
enumeration!(AutonomyMode: Manual, Supervised, Autonomous, Batch);
enumeration!(ResearchStatus: Draft, Running, WaitingForHuman, Paused, Blocked, Completed, Inconclusive, Cancelled, Failed);
enumeration!(Priority: Low, Normal, High, Critical);
enumeration!(ConstraintKind: Hard, Soft);
enumeration!(ActionImpact: Routine, Meaningful, Consequential, NonDelegable);
enumeration!(HumanDirectiveKind: PauseResearch, ResumeResearch, CancelResearch, PrioritizeWork, DeprioritizeWork, CreateBranch, PauseBranch, ResumeBranch, CancelBranch, PauseWork, ResumeWork, CancelWork, RequestPlanRevision, RequestDeeperInvestigation, RequestContradictionSearch, AdjustBudget, AddConstraint, SetAutonomy, PrioritizeBranch, OverrideRecommendation);

/// Project-level authority and human directives. Persisted as an aggregate by storage.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AutonomyPolicy {
    pub mode: AutonomyMode,
    pub version: u64,
    pub allow_branch_creation: bool,
    pub allow_plan_revision: bool,
    pub allow_experiment_activation: bool,
    pub review_termination: bool,
}
impl Default for AutonomyPolicy {
    fn default() -> Self {
        Self {
            mode: AutonomyMode::Supervised,
            version: 1,
            allow_branch_creation: false,
            allow_plan_revision: false,
            allow_experiment_activation: false,
            review_termination: true,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ResearchBudget {
    pub max_steps: Option<u64>,
    pub max_lead_calls: Option<u64>,
    pub max_strong_model_calls: Option<u64>,
    pub max_tool_calls: Option<u64>,
    pub max_elapsed_seconds: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ResearchConstraint {
    pub id: String,
    pub text: String,
    pub kind: ConstraintKind,
    pub excluded_source: Option<SourceId>,
    pub created_by: Actor,
    pub created_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HumanDirective {
    pub id: EventId,
    pub project_id: ProjectId,
    pub actor_id: String,
    pub created_at: DateTime<Utc>,
    pub kind: HumanDirectiveKind,
    pub target: Option<String>,
    pub parameters: Value,
    pub reason: Option<String>,
    pub expected_version: Option<u64>,
    pub version: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HumanControlState {
    #[serde(default)]
    pub owner_id: Option<String>,
    pub autonomy: AutonomyPolicy,
    pub status: ResearchStatus,
    pub version: u64,
    pub budget: ResearchBudget,
    pub work_priorities: std::collections::BTreeMap<WorkNodeId, Priority>,
    pub branch_priorities: std::collections::BTreeMap<BranchId, Priority>,
    pub paused_branches: std::collections::BTreeSet<BranchId>,
    pub cancelled_branches: std::collections::BTreeSet<BranchId>,
    pub directives: Vec<HumanDirective>,
    pub constraints: Vec<ResearchConstraint>,
}
impl Default for HumanControlState {
    fn default() -> Self {
        Self {
            autonomy: AutonomyPolicy::default(),
            owner_id: None,
            status: ResearchStatus::Draft,
            version: 1,
            budget: ResearchBudget::default(),
            work_priorities: Default::default(),
            branch_priorities: Default::default(),
            paused_branches: Default::default(),
            cancelled_branches: Default::default(),
            directives: Vec::new(),
            constraints: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkBudget {
    pub context_tokens: u32,
    pub max_model_calls: u32,
    pub max_tool_calls: u32,
}
impl Default for WorkBudget {
    fn default() -> Self {
        Self {
            context_tokens: 4096,
            max_model_calls: 4,
            max_tool_calls: 8,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OutputContract {
    pub required_entities: Vec<EntityRef>,
    pub required_artifacts: Vec<ArtifactId>,
    pub minimum_children: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkNode {
    pub id: WorkNodeId,
    #[serde(flatten)]
    pub meta: Meta,
    pub branch_id: BranchId,
    pub kind: String,
    pub granularity: WorkGranularity,
    pub title: String,
    pub objective: String,
    pub acceptance_criteria: OutputContract,
    pub status: WorkStatus,
    pub owner: Actor,
    pub capability_requirements: Vec<String>,
    pub context_policy_id: String,
    pub budget: WorkBudget,
    pub created_by: Actor,
    #[serde(default)]
    pub outcome: Option<ResearchOutcome>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkEdge {
    pub id: WorkEdgeId,
    #[serde(flatten)]
    pub meta: Meta,
    pub branch_id: BranchId,
    pub from: WorkNodeId,
    pub to: WorkNodeId,
    pub kind: WorkEdgeKind,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanRevision {
    pub id: PlanRevisionId,
    #[serde(flatten)]
    pub meta: Meta,
    pub branch_id: BranchId,
    pub parent_revision_id: Option<PlanRevisionId>,
    pub nodes: Vec<WorkNode>,
    pub edges: Vec<WorkEdge>,
    pub reason: String,
    pub created_by: Actor,
}

/// Proposed executable work, not a second persisted representation of the plan.
/// Dependencies refer to earlier steps; Runtime allocates all durable IDs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkPlanStep {
    pub title: String,
    pub objective: String,
    pub required_tool: String,
    #[serde(default)]
    pub depends_on: Vec<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Branch {
    pub id: BranchId,
    #[serde(flatten)]
    pub meta: Meta,
    pub name: String,
    pub parent_branch_id: Option<BranchId>,
    pub base_checkpoint_id: Option<CheckpointId>,
    pub head_revision_id: Option<PlanRevisionId>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExecutionCheckpoint {
    pub id: CheckpointId,
    #[serde(flatten)]
    pub meta: Meta,
    pub run_id: Option<RunId>,
    pub work_node_id: Option<WorkNodeId>,
    pub branch_id: BranchId,
    pub plan_revision_id: PlanRevisionId,
    pub context_snapshot_id: Option<ContextSnapshotId>,
    pub boundary: CheckpointBoundary,
    pub execution_state: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VersionedEntity {
    pub entity: EntityRef,
    pub version: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextEntry {
    pub reference: VersionedEntity,
    pub text: String,
    pub pinned: bool,
    pub estimated_tokens: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextSnapshot {
    pub id: ContextSnapshotId,
    #[serde(flatten)]
    pub meta: Meta,
    pub work_node_id: WorkNodeId,
    pub plan_revision_id: PlanRevisionId,
    pub entries: Vec<ContextEntry>,
    pub process_memory_ids: Vec<ProcessMemoryId>,
    pub retrieval_query: String,
    pub retrieval_candidate_count: u32,
    pub retrieval_selected: Vec<ContextRetrievalSelection>,
    pub instruction_version: String,
    pub skill_versions: Vec<String>,
    pub budget_tokens: u32,
    pub estimated_tokens: u32,
    pub estimation_method: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextRetrievalSelection {
    pub entity: EntityRef,
    pub lexical_score: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProcessMemory {
    pub id: ProcessMemoryId,
    #[serde(flatten)]
    pub meta: Meta,
    pub namespace: MemoryNamespace,
    pub work_node_id: Option<WorkNodeId>,
    pub content: String,
    pub created_by: Actor,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExecutionBinding {
    pub id: RunId,
    #[serde(flatten)]
    pub meta: Meta,
    pub work_node_id: WorkNodeId,
    pub branch_id: BranchId,
    pub plan_revision_id: PlanRevisionId,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub estimated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelInvocation {
    pub id: ModelInvocationId,
    #[serde(flatten)]
    pub meta: Meta,
    pub run_id: RunId,
    pub work_node_id: WorkNodeId,
    pub context_snapshot_id: ContextSnapshotId,
    pub provider: String,
    pub model: String,
    pub model_revision: String,
    pub runtime: String,
    pub instruction_version: String,
    pub output_schema: Value,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub latency_ms: Option<u64>,
    pub status: InvocationStatus,
    pub usage: TokenUsage,
    pub metadata: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolInvocation {
    pub id: ToolInvocationId,
    #[serde(flatten)]
    pub meta: Meta,
    pub run_id: RunId,
    pub work_node_id: WorkNodeId,
    pub tool: String,
    pub tool_version: String,
    pub input_artifacts: Vec<ArtifactId>,
    pub output_artifacts: Vec<ArtifactId>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: InvocationStatus,
    pub error_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalBatchStatus {
    Pending,
    PartiallyConsumed,
    Consumed,
    Rejected,
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetrievedSourceHandle {
    pub handle: String,
    pub source_id: SourceId,
}

/// A retrieval batch reconstructed from durable tool invocation artifacts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetrievalBatchState {
    pub search_invocation_id: ToolInvocationId,
    pub run_id: RunId,
    pub work_node_id: WorkNodeId,
    pub branch_id: BranchId,
    pub query: String,
    pub returned: Vec<RetrievedSourceHandle>,
    pub consumed_handles: BTreeSet<String>,
    pub status: RetrievalBatchStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrievalScope {
    pub anchor_run_id: Option<RunId>,
    pub work_node_id: WorkNodeId,
    pub branch_id: BranchId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalToolNames {
    pub search: String,
    pub refine: String,
    pub read: Vec<String>,
    pub reject: String,
}

/// Derive branch and root-run scoped retrieval state from persisted invocations.
/// Tool output metadata is stored with each invocation's output artifact, so the
/// state survives reopening the project database.
pub fn derive_retrieval_batches(
    state: &ProjectState,
    scope: RetrievalScope,
    tools: &RetrievalToolNames,
) -> Result<Vec<RetrievalBatchState>> {
    fn root_run(state: &ProjectState, mut id: RunId) -> Option<RunId> {
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(id) {
                return None;
            }
            let run = state.runs.get(&id)?;
            match run.parent_run_id {
                Some(parent) => id = parent,
                None => return Some(id),
            }
        }
    }
    let scope_root = scope.anchor_run_id.and_then(|id| root_run(state, id));
    let mut invocations: Vec<_> = state
        .tool_invocations
        .values()
        .filter(|invocation| {
            invocation.status == InvocationStatus::Succeeded
                && state
                    .execution_bindings
                    .get(&invocation.run_id)
                    .is_some_and(|binding| binding.branch_id == scope.branch_id)
                && (invocation.work_node_id == scope.work_node_id
                    || scope_root
                        .is_some_and(|root| root_run(state, invocation.run_id) == Some(root)))
        })
        .collect();
    invocations.sort_by_key(|invocation| invocation.started_at);

    let mut batches: Vec<RetrievalBatchState> = Vec::new();
    for invocation in invocations {
        let output_metadata = invocation
            .output_artifacts
            .iter()
            .filter_map(|id| state.artifacts.get(id))
            .find_map(|artifact| {
                artifact
                    .metadata
                    .get("tool_output_metadata")
                    .or_else(|| artifact.metadata["schema"].get("tool_output_metadata"))
            });
        if invocation.tool == tools.search || invocation.tool == tools.refine {
            let metadata = output_metadata
                .and_then(|value| value.get("retrieval_batch"))
                .cloned();
            let Some(metadata) = metadata else {
                // A stall response is a successful tool observation but did not
                // create a new batch or supersede the existing pending batch.
                continue;
            };
            if let Some(batch) = batches.last_mut() {
                if matches!(
                    batch.status,
                    RetrievalBatchStatus::Pending | RetrievalBatchStatus::PartiallyConsumed
                ) {
                    batch.status = RetrievalBatchStatus::Superseded;
                }
            }
            let search_invocation_id = metadata["search_invocation_id"]
                .as_str()
                .and_then(|id| id.parse().ok())
                .unwrap_or(invocation.id);
            let returned: Vec<RetrievedSourceHandle> =
                serde_json::from_value(metadata["returned"].clone()).unwrap_or_default();
            batches.push(RetrievalBatchState {
                search_invocation_id,
                run_id: invocation.run_id,
                work_node_id: invocation.work_node_id,
                branch_id: scope.branch_id,
                query: metadata["query"].as_str().unwrap_or_default().to_owned(),
                returned,
                consumed_handles: BTreeSet::new(),
                status: RetrievalBatchStatus::Pending,
            });
            continue;
        }
        if let Some(metadata) = output_metadata {
            if tools.read.iter().any(|tool| tool == &invocation.tool) {
                let event = &metadata["retrieval_consumption"];
                if let Some(batch) = batches.iter_mut().find(|batch| {
                    event["search_invocation_id"].as_str()
                        == Some(batch.search_invocation_id.to_string().as_str())
                }) {
                    if let Some(handle) = event["handle"].as_str() {
                        batch.consumed_handles.insert(handle.to_owned());
                    }
                    batch.status = if !batch.returned.is_empty()
                        && batch
                            .returned
                            .iter()
                            .all(|source| batch.consumed_handles.contains(&source.handle))
                    {
                        RetrievalBatchStatus::Consumed
                    } else {
                        RetrievalBatchStatus::PartiallyConsumed
                    };
                }
            } else if invocation.tool == tools.reject {
                let event = &metadata["retrieval_rejection"];
                if let Some(batch) = batches.iter_mut().find(|batch| {
                    event["search_invocation_id"].as_str()
                        == Some(batch.search_invocation_id.to_string().as_str())
                }) {
                    batch.status = RetrievalBatchStatus::Rejected;
                }
            }
        }
    }
    Ok(batches)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CandidateClaim {
    pub id: CandidateClaimId,
    #[serde(flatten)]
    pub meta: Meta,
    pub paper_id: PaperId,
    pub source_id: SourceId,
    pub text: String,
    pub source_quote: String,
    pub location: ClaimLocation,
    pub promoted_claim_id: Option<ClaimId>,
}

#[cfg(test)]
mod retrieval_state_tests {
    use super::*;

    fn fixture() -> (ProjectState, ProjectId, RunId, WorkNodeId, BranchId) {
        let project = ProjectId::new();
        let run = Run {
            id: RunId::new(),
            meta: Meta::new(project),
            task_id: TaskId::new(),
            parent_run_id: None,
            status: RunStatus::Running,
            started_at: Some(Utc::now()),
            finished_at: None,
            error: None,
            provider_metadata: Value::Null,
        };
        let node = WorkNodeId::new();
        let branch = BranchId::new();
        let mut state = ProjectState::default();
        state.runs.insert(run.id, run.clone());
        state.execution_bindings.insert(
            run.id,
            ExecutionBinding {
                id: run.id,
                meta: Meta::new(project),
                work_node_id: node,
                branch_id: branch,
                plan_revision_id: PlanRevisionId::new(),
            },
        );
        (state, project, run.id, node, branch)
    }

    fn record_output(
        state: &mut ProjectState,
        project: ProjectId,
        run: RunId,
        node: WorkNodeId,
        tool: &str,
        metadata: Value,
        order: i64,
    ) -> ToolInvocationId {
        let id = ToolInvocationId::new();
        let artifact_id = ArtifactId::new();
        let time = Utc::now() + chrono::Duration::milliseconds(order);
        state.artifacts.insert(
            artifact_id,
            Artifact {
                id: artifact_id,
                meta: Meta::new(project),
                name: format!("{tool} output"),
                artifact_kind: ArtifactKind::Output,
                path: format!("{tool}-{order}.json"),
                content_hash: format!("hash-{order}"),
                byte_size: 1,
                metadata: serde_json::json!({"tool_output_metadata":metadata}),
            },
        );
        state.tool_invocations.insert(
            id,
            ToolInvocation {
                id,
                meta: Meta::new(project),
                run_id: run,
                work_node_id: node,
                tool: tool.into(),
                tool_version: "test".into(),
                input_artifacts: vec![],
                output_artifacts: vec![artifact_id],
                started_at: time,
                finished_at: Some(time),
                status: InvocationStatus::Succeeded,
                error_code: None,
            },
        );
        id
    }

    fn names() -> RetrievalToolNames {
        RetrievalToolNames {
            search: "search_sources".into(),
            refine: "refine_search".into(),
            read: vec!["read_source".into()],
            reject: "reject_retrieved_sources".into(),
        }
    }

    #[test]
    fn persisted_retrieval_events_derive_pending_consumed_rejected_and_superseded() {
        let (mut state, project, run, node, branch) = fixture();
        let first_search = ToolInvocationId::new();
        let s1 = SourceId::new();
        let s2 = SourceId::new();
        record_output(
            &mut state,
            project,
            run,
            node,
            "search_sources",
            serde_json::json!({"retrieval_batch":{"search_invocation_id":first_search,"query":"alpha","returned":[{"handle":"S1","source_id":s1},{"handle":"S2","source_id":s2}]}}),
            1,
        );
        let scope = RetrievalScope {
            anchor_run_id: Some(run),
            work_node_id: node,
            branch_id: branch,
        };
        let tools = names();
        let pending = derive_retrieval_batches(&state, scope, &tools).unwrap();
        assert_eq!(pending[0].status, RetrievalBatchStatus::Pending);
        assert_eq!(pending[0].query, "alpha");
        assert_eq!(pending[0].returned.len(), 2);

        record_output(
            &mut state,
            project,
            run,
            node,
            "read_source",
            serde_json::json!({"retrieval_consumption":{"search_invocation_id":first_search,"handle":"S1"}}),
            2,
        );
        assert_eq!(
            derive_retrieval_batches(&state, scope, &tools).unwrap()[0].status,
            RetrievalBatchStatus::PartiallyConsumed
        );
        record_output(
            &mut state,
            project,
            run,
            node,
            "read_source",
            serde_json::json!({"retrieval_consumption":{"search_invocation_id":first_search,"handle":"S2"}}),
            3,
        );
        assert_eq!(
            derive_retrieval_batches(&state, scope, &tools).unwrap()[0].status,
            RetrievalBatchStatus::Consumed
        );

        let second_search = ToolInvocationId::new();
        record_output(
            &mut state,
            project,
            run,
            node,
            "search_sources",
            serde_json::json!({"retrieval_batch":{"search_invocation_id":second_search,"query":"beta","returned":[{"handle":"S1","source_id":s1}]}}),
            4,
        );
        record_output(
            &mut state,
            project,
            run,
            node,
            "reject_retrieved_sources",
            serde_json::json!({"retrieval_rejection":{"search_invocation_id":second_search,"reason":"irrelevant"}}),
            5,
        );
        let rejected = derive_retrieval_batches(&state, scope, &tools).unwrap();
        assert_eq!(rejected[1].status, RetrievalBatchStatus::Rejected);

        let third_search = ToolInvocationId::new();
        record_output(
            &mut state,
            project,
            run,
            node,
            "search_sources",
            serde_json::json!({"retrieval_batch":{"search_invocation_id":third_search,"query":"gamma","returned":[{"handle":"S1","source_id":s1}]}}),
            6,
        );
        let refined_search = ToolInvocationId::new();
        record_output(
            &mut state,
            project,
            run,
            node,
            "refine_search",
            serde_json::json!({"retrieval_batch":{"search_invocation_id":refined_search,"query":"delta","returned":[{"handle":"S1","source_id":s2}]},"refinement":{"previous_search_id":third_search,"reason":"narrow the topic"}}),
            7,
        );
        let refined = derive_retrieval_batches(&state, scope, &tools).unwrap();
        assert_eq!(refined[2].status, RetrievalBatchStatus::Superseded);
        assert_eq!(refined[3].status, RetrievalBatchStatus::Pending);

        record_output(
            &mut state,
            project,
            run,
            node,
            "refine_search",
            serde_json::json!({"stall_detected":true}),
            8,
        );
        let after_stall = derive_retrieval_batches(&state, scope, &tools).unwrap();
        assert_eq!(after_stall.len(), 4);
        assert_eq!(after_stall[3].status, RetrievalBatchStatus::Pending);

        let round_trip: ProjectState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            derive_retrieval_batches(&round_trip, scope, &tools).unwrap(),
            after_stall
        );
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchOutcome {
    Completed,
    Inconclusive,
    Blocked,
}

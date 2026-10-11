use anix_core::{
    AgentApi, ProviderContext, ResearchContext, Submission, Tool, ToolContext, ToolDescriptor,
    ToolInput, ToolOutput,
};
use anix_domain::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

pub type ModelFuture<'a> = Pin<Box<dyn Future<Output = Result<ModelResponse>> + Send + 'a>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderLocality {
    Local,
    External,
}
pub trait ModelProvider: Send + Sync {
    fn generate(&self, request: ModelRequest) -> ModelFuture<'_>;
    fn locality(&self) -> ProviderLocality {
        ProviderLocality::External
    }
}

/// Test-only boundary that makes an accidental external model route fail before
/// it can make a request. Unknown providers are external by default.
pub struct LocalOnlyTestProvider {
    inner: Arc<dyn ModelProvider>,
}
impl LocalOnlyTestProvider {
    pub fn new(inner: Arc<dyn ModelProvider>) -> Self {
        Self { inner }
    }
}
impl ModelProvider for LocalOnlyTestProvider {
    fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
        Box::pin(async move {
            if self.inner.locality() != ProviderLocality::Local {
                return Err(Error::PermissionDenied);
            }
            self.inner.generate(request).await
        })
    }
    fn locality(&self) -> ProviderLocality {
        ProviderLocality::Local
    }
}

/// The application explicitly registers approved, read-only tools. Models can
/// select a name and input, but cannot install capabilities or authorize writes.
#[derive(Default)]
pub struct ApprovedTools {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}
impl ApprovedTools {
    pub fn register(&mut self, tool: Arc<dyn Tool>) -> Result<()> {
        let descriptor = tool.descriptor();
        text(&descriptor.name, "tool name")?;
        if !descriptor.read_only {
            return Err(Error::GateRequired);
        }
        if self.tools.contains_key(&descriptor.name) {
            return Err(Error::Conflict("duplicate tool".into()));
        }
        self.tools.insert(descriptor.name, tool);
        Ok(())
    }
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools.values().map(|t| t.descriptor()).collect()
    }
    pub async fn execute(
        &self,
        name: &str,
        input: ToolInput,
        context: ToolContext,
    ) -> Result<ToolOutput> {
        safe_metadata(&input.parameters)?;
        if input.schema_version != 1 {
            return Err(Error::Validation("tool input version".into()));
        }
        let tool = self.tools.get(name).ok_or(Error::PermissionDenied)?;
        let output = tool.execute(input, context).await?;
        if output.schema_version != 1 {
            return Err(Error::Tool("tool output version".into()));
        }
        safe_metadata(&output.metadata)?;
        safe_metadata(&output.data)?;
        if serde_json::to_vec(&output)
            .map_err(|e| Error::Tool(e.to_string()))?
            .len()
            > 1_048_576
        {
            return Err(Error::Tool("tool output exceeds 1 MiB".into()));
        }
        Ok(output)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelTask {
    pub goal: String,
    pub focus: Option<EntityRef>,
    pub current_work: Option<WorkNode>,
    pub required_tool_succeeded: bool,
    #[serde(default)]
    pub retrieval_pending: bool,
    /// Bounded runtime-issued handles from a pending batch or the current work's supplied sources.
    #[serde(default)]
    pub retrieval_source_handles: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RetrievalProtocol {
    pub search_tool: String,
    pub refine_tool: String,
    pub read_tools: Vec<String>,
    pub reject_tool: String,
}
impl RetrievalProtocol {
    fn pending_tools(&self) -> BTreeSet<String> {
        self.read_tools
            .iter()
            .cloned()
            .chain([self.refine_tool.clone(), self.reject_tool.clone()])
            .collect()
    }

    /// The same executable set is advertised to the model and checked at execution.
    /// Auxiliary retrieval is limited to resolving a required read's prerequisites.
    fn executable_tools(
        &self,
        descriptors: &[ToolDescriptor],
        required: Option<&str>,
        pending: bool,
        handles: &[String],
    ) -> Vec<ToolDescriptor> {
        let pending_names = self.pending_tools();
        descriptors
            .iter()
            .filter(|tool| {
                if self.read_tools.contains(&tool.name) {
                    return !handles.is_empty()
                        && (pending || Some(tool.name.as_str()) == required);
                }
                if pending {
                    return pending_names.contains(&tool.name);
                }
                if tool.name == self.refine_tool || tool.name == self.reject_tool {
                    return false;
                }
                Some(tool.name.as_str()) == required
                    || (tool.name == self.search_tool
                        && handles.is_empty()
                        && required
                            .is_some_and(|name| self.read_tools.iter().any(|read| read == name)))
            })
            .cloned()
            .collect()
    }
}

fn resolve_pending_search_id(batches: &[RetrievalBatchState]) -> Result<ToolInvocationId> {
    let mut pending = batches.iter().filter(|batch| {
        matches!(
            batch.status,
            RetrievalBatchStatus::Pending | RetrievalBatchStatus::PartiallyConsumed
        )
    });
    let Some(batch) = pending.next() else {
        return Err(Error::InvalidTransition("NoPendingRetrieval".into()));
    };
    if pending.next().is_some() {
        return Err(Error::InvalidTransition("AmbiguousPendingRetrieval".into()));
    }
    Ok(batch.search_invocation_id)
}

fn bind_pending_search_id(
    input: &ToolInput,
    batches: &[RetrievalBatchState],
    field: &str,
) -> Result<ToolInput> {
    let search_id = resolve_pending_search_id(batches)?;
    let mut bound = input.clone();
    let parameters = bound
        .parameters
        .as_object_mut()
        .ok_or_else(|| Error::Validation("tool parameters must be an object".into()))?;
    parameters.insert(field.into(), serde_json::json!(search_id));
    Ok(bound)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelSelection {
    pub provider: String,
    pub model: String,
}
pub trait ModelSelector: Send + Sync {
    fn select(&self, task: &ModelTask) -> Result<ModelSelection>;
}
pub struct FixedModelSelector {
    pub selection: ModelSelection,
}
impl ModelSelector for FixedModelSelector {
    fn select(&self, _: &ModelTask) -> Result<ModelSelection> {
        text(&self.selection.provider, "provider name")?;
        text(&self.selection.model, "model name")?;
        Ok(self.selection.clone())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelRequest {
    pub task: ModelTask,
    pub context: ResearchContext,
    pub compiled_context: Option<ProviderContext>,
    pub selection: ModelSelection,
    pub output_schema_version: u32,
    pub max_output_tokens: u32,
    pub tools: Vec<ToolDescriptor>,
    pub available_child_tools: Vec<String>,
    pub allowed_process_actions: Option<Vec<ProcessActionKind>>,
    pub tool_results: Vec<NamedToolResult>,
    #[serde(default)]
    pub research_branches: Vec<anix_core::ResearchBranchScope>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessActionKind {
    SwitchResearchBranch,
    CreateWorkNode,
    CreateBranch,
    FinishResearch,
    RepeatWorkNode,
    #[serde(alias = "propose_plan_revision")]
    ProposeObjectiveRevision,
    ProposeWorkPlan,
}
impl ProcessActionKind {
    fn allows(self, action: &ProcessAction) -> bool {
        matches!(
            (self, action),
            (
                Self::SwitchResearchBranch,
                ProcessAction::SwitchResearchBranch { .. }
            ) | (Self::ProposeWorkPlan, ProcessAction::ProposeWorkPlan { .. })
                | (Self::CreateWorkNode, ProcessAction::CreateWorkNode { .. })
                | (Self::RepeatWorkNode, ProcessAction::RepeatWorkNode { .. })
                | (
                    Self::ProposeObjectiveRevision,
                    ProcessAction::ProposeObjectiveRevision { .. }
                )
                | (Self::CreateBranch, ProcessAction::CreateBranch { .. })
                | (Self::FinishResearch, ProcessAction::FinishResearch { .. })
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedToolResult {
    pub name: String,
    pub output: ToolOutput,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentDecision {
    Propose {
        action: ProposedAction,
        reason: String,
    },
    NoChange {
        summary: String,
    },
    UseTool {
        name: String,
        input: ToolInput,
    },
    Process {
        action: ProcessAction,
        #[serde(default)]
        rationale: String,
        #[serde(default)]
        expected_result: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ProcessAction {
    SwitchResearchBranch {
        branch: String,
    },
    CreateWorkNode {
        title: String,
        objective: String,
        required_tool: String,
    },
    CompleteWorkNode {
        work_node_id: WorkNodeId,
        #[serde(default)]
        summary: Option<String>,
    },
    RepeatWorkNode {
        title: String,
        objective: String,
        required_tool: String,
        reason: String,
    },
    #[serde(alias = "propose_plan_revision")]
    ProposeObjectiveRevision {
        objective: String,
        reason: String,
    },
    ProposeWorkPlan {
        steps: Vec<WorkPlanStep>,
        reason: String,
    },
    ConcludeWorkInconclusive {
        summary: String,
    },
    CreateBranch {
        name: String,
    },
    FinishResearch {
        outcome: ResearchOutcome,
        summary: String,
        #[serde(default)]
        reviewed_branches: Vec<String>,
    },
}

pub use anix_domain::ResearchOutcome;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentStepOutcome {
    pub task_id: TaskId,
    pub run_id: RunId,
    pub decision: AgentDecision,
    pub tool_output: Option<ToolOutput>,
    pub submission: Option<Submission>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelResponse {
    pub schema_version: u32,
    pub decision: AgentDecision,
    pub metadata: Value,
}

/// A deterministic, offline provider. It records requests for assertions and
/// consumes supplied structured responses in order. No production network path
/// depends on the fake; real providers implement ModelProvider separately.
pub struct FakeModelProvider {
    responses: Mutex<VecDeque<ModelResponse>>,
    requests: Mutex<Vec<ModelRequest>>,
}
impl FakeModelProvider {
    pub fn new(responses: Vec<ModelResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(vec![]),
        }
    }
    pub fn requests(&self) -> Result<Vec<ModelRequest>> {
        Ok(self
            .requests
            .lock()
            .map_err(|_| Error::Model("request lock".into()))?
            .clone())
    }
}
impl ModelProvider for FakeModelProvider {
    fn generate(&self, request: ModelRequest) -> ModelFuture<'_> {
        Box::pin(async move {
            self.requests
                .lock()
                .map_err(|_| Error::Model("request lock".into()))?
                .push(request);
            self.responses
                .lock()
                .map_err(|_| Error::Model("response lock".into()))?
                .pop_front()
                .ok_or_else(|| Error::Model("fake provider exhausted".into()))
        })
    }
    fn locality(&self) -> ProviderLocality {
        ProviderLocality::Local
    }
}

pub struct ContextBuilder;
fn bound(s: &mut String) {
    if s.len() > 4000 {
        let mut end = 4000;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
}
fn bound_action(action: &mut ProposedAction) {
    match action {
        ProposedAction::CreateHypothesis(v) => {
            bound(&mut v.title);
            bound(&mut v.statement)
        }
        ProposedAction::ReviseHypothesis { statement, .. } => bound(statement),
        ProposedAction::CreateExperiment(v) => {
            bound(&mut v.title);
            bound(&mut v.objective);
            bound(&mut v.methodology);
            bound(&mut v.protocol)
        }
        ProposedAction::CreateFinding(v) => {
            bound(&mut v.title);
            bound(&mut v.summary)
        }
        _ => {}
    }
}
impl ContextBuilder {
    pub fn build(
        api: &AgentApi<'_>,
        project: ProjectId,
        focus: Option<EntityRef>,
    ) -> Result<ResearchContext> {
        let mut ctx = api.context_data(project, focus)?;
        bound(&mut ctx.project.name);
        bound(&mut ctx.project.description);
        for q in &mut ctx.questions {
            bound(&mut q.title);
            bound(&mut q.description)
        }
        for h in &mut ctx.hypotheses {
            bound(&mut h.title);
            bound(&mut h.statement)
        }
        for c in &mut ctx.claims {
            bound(&mut c.text);
            c.normalized_text = None;
            c.location.text_span = None;
            for value in [
                &mut c.location.section,
                &mut c.location.table,
                &mut c.location.figure,
            ]
            .into_iter()
            .flatten()
            {
                bound(value);
            }
        }
        for f in &mut ctx.findings {
            bound(&mut f.title);
            bound(&mut f.summary)
        }
        for e in &mut ctx.experiments {
            bound(&mut e.title);
            bound(&mut e.objective);
            bound(&mut e.methodology);
            bound(&mut e.protocol)
        }
        // Context communicates decisions, not arbitrarily large action payloads.
        for e in &mut ctx.recent_events {
            e.payload = serde_json::json!({"event_id":e.id,"event_type":e.event_type});
        }
        for g in &mut ctx.pending_gates {
            bound(&mut g.reason);
            bound(&mut g.risk);
            bound_action(&mut g.proposed_action.action);
        }
        if serde_json::to_vec(&ctx)
            .map_err(|e| Error::Validation(e.to_string()))?
            .len()
            > 1_048_576
        {
            return Err(Error::Validation("research context exceeds 1 MiB".into()));
        }
        Ok(ctx)
    }

    pub fn build_for_snapshot(
        api: &AgentApi<'_>,
        project: ProjectId,
        focus: Option<EntityRef>,
        snapshot: &ProviderContext,
    ) -> Result<ResearchContext> {
        let mut context = Self::build(api, project, focus)?;
        let included: BTreeSet<EntityRef> = snapshot
            .entries
            .iter()
            .map(|entry| entry.reference.entity)
            .chain(focus)
            .collect();
        context
            .questions
            .retain(|v| included.contains(&EntityRef::Question(v.id)));
        context
            .hypotheses
            .retain(|v| included.contains(&EntityRef::Hypothesis(v.id)));
        context
            .claims
            .retain(|v| included.contains(&EntityRef::Claim(v.id)));
        context
            .findings
            .retain(|v| included.contains(&EntityRef::Finding(v.id)));
        context
            .experiments
            .retain(|v| included.contains(&EntityRef::Experiment(v.id)));
        context
            .neighborhood
            .retain(|v| included.contains(&v.from_entity) || included.contains(&v.to_entity));
        context
            .pending_gates
            .retain(|v| v.status == GateStatus::Pending);
        context.recent_events.clear();
        Ok(context)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentOutcome {
    pub task_id: TaskId,
    pub run_id: RunId,
    pub submission: Option<Submission>,
    pub summary: Option<String>,
}
/// AgentService owns only a restricted application capability and replaceable
/// providers. It has no storage dependency and no human approval capability.
#[allow(clippy::too_many_arguments)]
fn trace_permission_denial(
    error: Error,
    actor: &str,
    project: ProjectId,
    branch: BranchId,
    action: &str,
    resource_type: &str,
    resource_id: Option<String>,
    required_capability: &str,
    granted_capabilities: Vec<String>,
    reason: &str,
) -> Error {
    match error {
        Error::PermissionDenied => Error::AuthorizationDenied(Box::new(AuthorizationTrace {
            actor: Actor::LeadAgent(actor.into()),
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id,
            project_id: project.to_string(),
            branch_id: Some(branch.to_string()),
            parent_branch_id: Some(branch.to_string()),
            required_capability: required_capability.into(),
            granted_capabilities,
            result: "deny".into(),
            reason: reason.into(),
        })),
        detailed @ Error::AuthorizationDenied(_) => detailed,
        other => other,
    }
}

pub struct LeadAgent<'a> {
    api: AgentApi<'a>,
    provider: Arc<dyn ModelProvider>,
    selector: Arc<dyn ModelSelector>,
    tools: ApprovedTools,
    process_action_constraint: Option<Vec<ProcessActionKind>>,
    retrieval_protocol: Option<RetrievalProtocol>,
}
struct FailedRunGuard<'a> {
    api: &'a AgentApi<'a>,
    project: ProjectId,
    run: RunId,
    armed: bool,
}
impl FailedRunGuard<'_> {
    fn disarm(&mut self) {
        self.armed = false;
    }
}
impl Drop for FailedRunGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.api.finish(self.project, self.run, RunStatus::Failed);
        }
    }
}
pub type AgentService<'a> = LeadAgent<'a>;
impl<'a> LeadAgent<'a> {
    pub fn new(
        api: AgentApi<'a>,
        provider: Arc<dyn ModelProvider>,
        selector: Arc<dyn ModelSelector>,
    ) -> Self {
        Self {
            api,
            provider,
            selector,
            tools: ApprovedTools::default(),
            process_action_constraint: None,
            retrieval_protocol: None,
        }
    }
    pub fn with_tools(mut self, tools: ApprovedTools) -> Self {
        self.tools = tools;
        self
    }
    pub fn with_process_action_constraint(mut self, actions: Vec<ProcessActionKind>) -> Self {
        self.process_action_constraint = Some(actions);
        self
    }
    pub fn without_process_action_constraint(mut self) -> Self {
        self.process_action_constraint = None;
        self
    }
    pub fn with_retrieval_protocol(mut self, protocol: RetrievalProtocol) -> Self {
        self.retrieval_protocol = Some(protocol);
        self
    }
    pub async fn run(
        &self,
        project: ProjectId,
        goal: String,
        focus: Option<EntityRef>,
        parent_run_id: Option<RunId>,
    ) -> Result<AgentOutcome> {
        text(&goal, "agent goal")?;
        if goal.len() > 16000 {
            return Err(Error::Validation("agent goal exceeds 16000 bytes".into()));
        }
        safe_metadata(&Value::String(goal.clone()))?;
        let task = self.api.create_task(project, goal.clone())?;
        let run = self.api.start_run(project, task.id, parent_run_id)?;
        let result = self.decide(project, run.id, goal, focus).await;
        match result {
            Ok((submission, summary)) => {
                if !matches!(submission, Some(Submission::Gate(_))) {
                    self.api.finish(project, run.id, RunStatus::Completed)?;
                }
                Ok(AgentOutcome {
                    task_id: task.id,
                    run_id: run.id,
                    submission,
                    summary,
                })
            }
            Err(error) => {
                self.api.finish(project, run.id, RunStatus::Failed)?;
                Err(error)
            }
        }
    }

    /// Executes exactly one bounded Lead decision against a persisted work node and context snapshot.
    pub async fn run_step(
        &self,
        project: ProjectId,
        goal: String,
        focus: Option<EntityRef>,
        parent_run_id: Option<RunId>,
        work_node_id: WorkNodeId,
    ) -> Result<AgentStepOutcome> {
        text(&goal, "agent goal")?;
        if goal.len() > 16_000 {
            return Err(Error::Validation("agent goal exceeds 16000 bytes".into()));
        }
        safe_metadata(&Value::String(goal.clone()))?;
        let work = self.api.start_work(project, work_node_id)?;
        let required_tool_succeeded = self
            .api
            .required_tool_output_exists(project, work_node_id)?;
        let retrieval_batches = match &self.retrieval_protocol {
            Some(protocol) => self.api.retrieval_batches(
                project,
                parent_run_id,
                work_node_id,
                work.branch_id,
                &RetrievalToolNames {
                    search: protocol.search_tool.clone(),
                    refine: protocol.refine_tool.clone(),
                    read: protocol.read_tools.clone(),
                    reject: protocol.reject_tool.clone(),
                },
            )?,
            None => Vec::new(),
        };
        if retrieval_batches
            .iter()
            .filter(|batch| {
                matches!(
                    batch.status,
                    RetrievalBatchStatus::Pending | RetrievalBatchStatus::PartiallyConsumed
                )
            })
            .count()
            > 1
        {
            return Err(Error::InvalidTransition("AmbiguousPendingRetrieval".into()));
        }
        let retrieval_pending = retrieval_batches.iter().any(|batch| {
            matches!(
                batch.status,
                RetrievalBatchStatus::Pending | RetrievalBatchStatus::PartiallyConsumed
            )
        });
        let supplied_sources = if self.retrieval_protocol.is_some()
            && !retrieval_pending
            && work
                .acceptance_criteria
                .required_entities
                .iter()
                .any(|reference| matches!(reference, EntityRef::Source(_) | EntityRef::Paper(_)))
        {
            self.api.work_source_handles(project, work_node_id)?
        } else {
            Vec::new()
        };
        let task = ModelTask {
            goal: goal.clone(),
            focus,
            current_work: Some(work.clone()),
            required_tool_succeeded: required_tool_succeeded && !retrieval_pending,
            retrieval_pending,
            retrieval_source_handles: retrieval_batches
                .iter()
                .rev()
                .find(|batch| {
                    matches!(
                        batch.status,
                        RetrievalBatchStatus::Pending | RetrievalBatchStatus::PartiallyConsumed
                    )
                })
                .map(|batch| {
                    batch
                        .returned
                        .iter()
                        .filter(|source| !batch.consumed_handles.contains(&source.handle))
                        .map(|source| source.handle.clone())
                        .collect()
                })
                .unwrap_or_else(|| {
                    supplied_sources
                        .iter()
                        .map(|(reference, _)| reference.handle.clone())
                        .collect()
                }),
        };
        let selection = self.selector.select(&task)?;
        let research_branches = self.api.research_branch_scopes(project, work_node_id)?;
        let selection_metadata =
            serde_json::to_value(&selection).map_err(|e| Error::Model(e.to_string()))?;
        safe_metadata(&selection_metadata)?;
        let task_record = self.api.create_task(project, goal)?;
        let run = self.api.start_run(project, task_record.id, parent_run_id)?;
        let mut failure_guard = FailedRunGuard {
            api: &self.api,
            project,
            run: run.id,
            armed: true,
        };
        self.api.bind_run(project, run.id, work_node_id)?;
        let pinned = focus.into_iter().collect();
        let context_timer = std::time::Instant::now();
        let (snapshot, compiled_context) =
            self.api
                .compile_context(project, run.id, work_node_id, pinned)?;
        let context =
            ContextBuilder::build_for_snapshot(&self.api, project, focus, &compiled_context)?;
        let context_compile_ms = context_timer.elapsed().as_millis() as u64;
        let all_descriptors = self.tools.descriptors();
        let pending_tool_names = self
            .retrieval_protocol
            .as_ref()
            .map(RetrievalProtocol::pending_tools)
            .unwrap_or_default();
        let required = work
            .capability_requirements
            .iter()
            .find_map(|v| v.strip_prefix("tool:"));
        let allowed_tools = if task.required_tool_succeeded && !retrieval_pending {
            Vec::new()
        } else if let Some(protocol) = &self.retrieval_protocol {
            if work.granularity == WorkGranularity::Atomic || retrieval_pending {
                protocol.executable_tools(
                    &all_descriptors,
                    required,
                    retrieval_pending,
                    &task.retrieval_source_handles,
                )
            } else {
                Vec::new()
            }
        } else if retrieval_pending {
            all_descriptors
                .iter()
                .filter(|descriptor| pending_tool_names.contains(&descriptor.name))
                .cloned()
                .collect()
        } else if work.granularity == WorkGranularity::Atomic {
            let required = work
                .capability_requirements
                .iter()
                .find_map(|v| v.strip_prefix("tool:"));
            all_descriptors
                .iter()
                .filter(|d| Some(d.name.as_str()) == required)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        if work.granularity == WorkGranularity::Atomic
            && !task.required_tool_succeeded
            && allowed_tools.is_empty()
        {
            let reason = format!(
                "ActionPrerequisitesUnavailable: required tool {}; no authorized source handle or registered prerequisite provider. Supply material, register an approved provider, or revise the work plan.",
                required.unwrap_or("missing")
            );
            self.api.finish(project, run.id, RunStatus::Failed)?;
            failure_guard.disarm();
            self.api.block_work(project, work_node_id, reason.clone())?;
            return Err(Error::InvalidTransition(reason));
        }
        let available_child_tools: Vec<String> = if retrieval_pending {
            all_descriptors
                .iter()
                .filter(|descriptor| pending_tool_names.contains(&descriptor.name))
                .map(|descriptor| descriptor.name.clone())
                .collect()
        } else {
            all_descriptors
                .iter()
                .filter(|tool| {
                    self.retrieval_protocol.as_ref().is_none_or(|protocol| {
                        tool.name != protocol.refine_tool
                            && tool.name != protocol.reject_tool
                            && (!protocol.read_tools.contains(&tool.name)
                                || !task.retrieval_source_handles.is_empty()
                                || all_descriptors
                                    .iter()
                                    .any(|candidate| candidate.name == protocol.search_tool))
                    })
                })
                .map(|tool| tool.name.clone())
                .collect()
        };
        let started = chrono::Utc::now();
        let timer = std::time::Instant::now();
        // Reserve the durable call budget before crossing the provider boundary.
        // The Running invocation also makes failed/time-out calls count across restart.
        let invocation_id = self.api.start_model_invocation(
            project,
            run.id,
            work_node_id,
            snapshot.id,
            selection.provider.clone(),
            selection.model.clone(),
            started,
        )?;
        let response = self
            .provider
            .generate(ModelRequest {
                task: task.clone(),
                context,
                compiled_context: Some(compiled_context),
                selection: selection.clone(),
                output_schema_version: 1,
                max_output_tokens: 224,
                tools: allowed_tools.clone(),
                available_child_tools: available_child_tools.clone(),
                allowed_process_actions: self.process_action_constraint.clone(),
                research_branches:research_branches.clone(),
                tool_results: if supplied_sources.is_empty() { Vec::new() } else { vec![NamedToolResult {
                    name: "runtime_source_catalog".into(), output: ToolOutput { schema_version: 1,
                        data: serde_json::json!({"supplied_sources": supplied_sources.iter().map(|(reference, title)| serde_json::json!({"handle":reference.handle,"title":title})).collect::<Vec<_>>()}),
                        metadata: serde_json::json!({"runtime_observation":true}) }
                }] },
            })
            .await;
        let latency_ms = timer.elapsed().as_millis() as u64;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                let _ = self.api.finish_model_invocation(
                    project,
                    invocation_id,
                    latency_ms,
                    TokenUsage::default(),
                    serde_json::json!({"failure":error.to_string()}),
                    false,
                );
                let _ = self.api.finish(project, run.id, RunStatus::Failed);
                failure_guard.disarm();
                return Err(error);
            }
        };
        if response.schema_version != 1 {
            let error = Error::Model("unsupported structured response version".into());
            let _ = self.api.finish_model_invocation(
                project,
                invocation_id,
                latency_ms,
                TokenUsage::default(),
                serde_json::json!({"failure":error.to_string()}),
                false,
            );
            let _ = self.api.finish(project, run.id, RunStatus::Failed);
            failure_guard.disarm();
            return Err(error);
        }
        safe_metadata(&response.metadata)?;
        let progress_before = self.api.lead_progress_snapshot(project, work_node_id)?;
        let mut response = response;
        if let Some(trace) = response
            .metadata
            .get_mut("diagnostic_trace")
            .and_then(Value::as_object_mut)
        {
            trace.insert(
                "identity".into(),
                serde_json::json!({
                    "project_id":project,
                    "run_id":run.id,
                    "work_node_id":work_node_id,
                    "branch_id":work.branch_id,
                    "model_invocation_id":invocation_id,
                    "context_snapshot_id":snapshot.id,
                }),
            );
            trace.insert("research_objective".into(), serde_json::json!(task.goal));
            trace.insert(
                "work_node_objective".into(),
                serde_json::json!(work.objective),
            );
            trace.insert(
                "retrieval_state".into(),
                serde_json::to_value(&retrieval_batches).unwrap_or(Value::Null),
            );
            trace.insert(
                "allowed_process_actions".into(),
                serde_json::to_value(&self.process_action_constraint).unwrap_or(Value::Null),
            );
            trace.insert(
                "available_source_handles".into(),
                serde_json::to_value(&task.retrieval_source_handles).unwrap_or(Value::Null),
            );
            trace.insert("progress_before".into(), progress_before.clone());
        }
        self.api.record_provider(project, run.id, serde_json::json!({"schema_version":1,"selection":selection_metadata,"response":response.metadata}))?;
        let usage = TokenUsage {
            input: response.metadata["input_tokens"].as_u64().unwrap_or(0),
            output: response.metadata["output_tokens"].as_u64().unwrap_or(0),
            estimated: response.metadata["input_tokens"].as_u64().is_none(),
        };
        let mut invocation_metadata = response.metadata.clone();
        if let Some(metadata) = invocation_metadata.as_object_mut() {
            metadata.insert(
                "context_compile_ms".into(),
                serde_json::json!(context_compile_ms),
            );
        }
        self.api.finish_model_invocation(
            project,
            invocation_id,
            latency_ms,
            usage,
            invocation_metadata,
            true,
        )?;
        let diagnostic_trace_enabled = response.metadata["diagnostic_trace"].is_object();
        let trace_action_name = response.metadata["final_action"].clone();
        let decision = response.decision;
        let process_constraint_ok = match (&self.process_action_constraint, &decision) {
            (None, _) => true,
            (Some(allowed), AgentDecision::Process { action, .. }) => {
                allowed.iter().any(|kind| kind.allows(action))
            }
            (Some(_), _) => false,
        };
        let step_contract_ok = match (&work.granularity, required_tool_succeeded, &decision) {
            (WorkGranularity::Composite, _, AgentDecision::Process { action, .. }) => {
                matches!(
                    action,
                    ProcessAction::CreateWorkNode { .. }
                        | ProcessAction::RepeatWorkNode { .. }
                        | ProcessAction::ProposeObjectiveRevision { .. }
                        | ProcessAction::ProposeWorkPlan { .. }
                        | ProcessAction::CreateBranch { .. }
                        | ProcessAction::FinishResearch { .. }
                        | ProcessAction::SwitchResearchBranch { .. }
                ) && !(retrieval_pending && matches!(action, ProcessAction::FinishResearch { .. }))
            }
            (WorkGranularity::Atomic, false, AgentDecision::UseTool { .. }) => true,
            (
                WorkGranularity::Atomic,
                _,
                AgentDecision::Process {
                    action:
                        ProcessAction::ProposeObjectiveRevision { .. }
                        | ProcessAction::ConcludeWorkInconclusive { .. },
                    ..
                },
            ) => !retrieval_pending,
            (WorkGranularity::Atomic, true, AgentDecision::UseTool { .. }) if retrieval_pending => {
                true
            }
            (WorkGranularity::Atomic, true, AgentDecision::Process { action, .. }) => matches!(
                action,
                ProcessAction::CompleteWorkNode { work_node_id: target, .. }
                    if *target == work_node_id && !retrieval_pending
            ),
            _ => false,
        };
        if !step_contract_ok || !process_constraint_ok {
            if retrieval_pending
                && matches!(
                    &decision,
                    AgentDecision::Process {
                        action: ProcessAction::CompleteWorkNode { .. }
                            | ProcessAction::FinishResearch { .. },
                        ..
                    }
                )
            {
                return Err(Error::InvalidTransition(
                    "PendingRetrievalMustBeResolved".into(),
                ));
            }
            return Err(Error::InvalidTransition(format!(
                "Lead decision is not allowed for {:?} work node",
                work.granularity
            )));
        }
        let mut tool_output = None;
        let mut submission = None;
        let mut finish_run = true;
        match &decision {
            AgentDecision::Propose { action, reason } => {
                if reason.len() > 2000 {
                    return Err(Error::Validation(
                        "decision rationale exceeds 500-token bound".into(),
                    ));
                }
                submission = Some(
                    self.api
                        .submit(
                            project,
                            run.id,
                            ActionEnvelope::new(action.clone()),
                            reason.clone(),
                        )
                        .map_err(|error| {
                            trace_permission_denial(
                                error,
                                self.api.identity(),
                                project,
                                work.branch_id,
                                "ProposeAction",
                                "ResearchAction",
                                None,
                                "ProposeResearchAction",
                                vec![],
                                "Core proposal authorization rejected the Lead proposal",
                            )
                        })?,
                );
                finish_run = !matches!(&submission, Some(Submission::Gate(_)));
            }
            AgentDecision::NoChange { summary } => {
                if summary.len() > 2000 {
                    return Err(Error::Validation(
                        "decision summary exceeds 500-token bound".into(),
                    ));
                }
                safe_metadata(&Value::String(summary.clone()))?;
            }
            AgentDecision::UseTool { name, input } => {
                let mut tool_input = match self.retrieval_protocol.as_ref() {
                    Some(protocol) if name == &protocol.refine_tool => {
                        bind_pending_search_id(input, &retrieval_batches, "previous_search_id")?
                    }
                    Some(protocol) if name == &protocol.reject_tool => {
                        bind_pending_search_id(input, &retrieval_batches, "search_invocation_id")?
                    }
                    _ => input.clone(),
                };
                if retrieval_pending
                    && self
                        .retrieval_protocol
                        .as_ref()
                        .is_some_and(|protocol| name == &protocol.search_tool)
                {
                    return Err(Error::InvalidTransition(
                        "PendingRetrievalMustBeResolved".into(),
                    ));
                }
                if !allowed_tools.iter().any(|d| d.name == *name) {
                    return Err(trace_permission_denial(
                        Error::PermissionDenied,
                        self.api.identity(),
                        project,
                        work.branch_id,
                        "ExecuteTool",
                        "Tool",
                        Some(name.clone()),
                        &format!("tool:{name}"),
                        allowed_tools.iter().map(|tool| tool.name.clone()).collect(),
                        "requested tool is not in this step's approved tool set",
                    ));
                }
                if self
                    .retrieval_protocol
                    .as_ref()
                    .is_some_and(|protocol| protocol.read_tools.contains(name))
                {
                    let handle = tool_input.parameters["handle"].as_str();
                    if !handle.is_some_and(|handle| {
                        task.retrieval_source_handles
                            .iter()
                            .any(|valid| valid == handle)
                    }) {
                        return Err(Error::Validation("InvalidSourceHandle: use an authorized handle advertised for this step".into()));
                    }
                    if let Some((reference, _)) = supplied_sources
                        .iter()
                        .find(|(reference, _)| Some(reference.handle.as_str()) == handle)
                    {
                        tool_input.parameters["runtime_source_ref"] =
                            serde_json::json!(reference.source_id);
                    }
                }
                let output = match self
                    .tools
                    .execute(
                        name,
                        tool_input,
                        ToolContext {
                            project_id: project,
                            run_id: Some(run.id),
                            context_snapshot_id: Some(snapshot.id),
                            tool_invocation_id: None,
                        },
                    )
                    .await
                {
                    Ok(output) => output,
                    Err(error) => {
                        if diagnostic_trace_enabled {
                            self.api.update_decision_trace_outcome(
                                project,
                                invocation_id,
                                serde_json::json!({
                                    "validation":{"step_contract":step_contract_ok,"process_action_constraint":process_constraint_ok},
                                    "executed_action":trace_action_name,
                                    "execution_status":"rejected_or_failed",
                                    "error":error.to_string(),
                                    "progress_after":self.api.lead_progress_snapshot(project, work_node_id)?,
                                }),
                            )?;
                        }
                        return Err(trace_permission_denial(
                            error,
                            self.api.identity(),
                            project,
                            work.branch_id,
                            "ExecuteTool",
                            "Tool",
                            Some(name.clone()),
                            &format!("tool:{name}"),
                            allowed_tools.iter().map(|tool| tool.name.clone()).collect(),
                            "approved tool execution rejected the invocation",
                        ));
                    }
                };
                tool_output = Some(output);
            }
            AgentDecision::Process {
                action,
                rationale,
                expected_result,
            } => {
                if rationale.len() > 2000
                    || expected_result.as_ref().is_some_and(|v| v.len() > 2000)
                {
                    return Err(Error::Validation(
                        "decision text exceeds 500-token bound".into(),
                    ));
                }
                match action {
                    ProcessAction::CreateWorkNode {
                        title,
                        objective,
                        required_tool,
                    }
                    | ProcessAction::RepeatWorkNode {
                        title,
                        objective,
                        required_tool,
                        ..
                    } => {
                        if !available_child_tools.contains(required_tool) {
                            return Err(trace_permission_denial(
                                Error::PermissionDenied,
                                self.api.identity(),
                                project,
                                work.branch_id,
                                "CreateWorkNode",
                                "WorkNode",
                                None,
                                &format!("tool:{required_tool}"),
                                self.tools.tools.keys().cloned().collect(),
                                "child capability is not registered for this Lead step",
                            ));
                        }
                        let child = self
                            .api
                            .create_child_work_with_repeat(
                                project,
                                run.id,
                                work_node_id,
                                title.clone(),
                                objective.clone(),
                                required_tool.clone(),
                                match action {
                                    ProcessAction::RepeatWorkNode { reason, .. } => {
                                        Some(reason.clone())
                                    }
                                    _ => None,
                                },
                            )
                            .map_err(|error| {
                                trace_permission_denial(
                                    error,
                                    self.api.identity(),
                                    project,
                                    work.branch_id,
                                    "CreateWorkNode",
                                    "WorkNode",
                                    Some(title.clone()),
                                    &format!("tool:{required_tool}"),
                                    self.tools.tools.keys().cloned().collect(),
                                    "Core denied child WorkNode creation",
                                )
                            })?;
                        let created = child.meta.created_at >= started;
                        let observation = serde_json::json!({"kind":if created {"work_created"} else {"existing_work"},"created":created,"work_node_id":child.id,"status":child.status,"objective":child.objective,"instruction":"Use the existing result; justify replication, propose a changed strategy, or assess explicit termination."});
                        self.api.write_process_memory(
                            project,
                            run.id,
                            work_node_id,
                            format!("Planning observation: {observation}"),
                        )?;
                        tool_output = Some(ToolOutput {
                            schema_version: 1,
                            data: observation,
                            metadata: serde_json::json!({"runtime_observation":true}),
                        });
                    }
                    ProcessAction::SwitchResearchBranch { branch } => {
                        let scope = research_branches
                            .iter()
                            .find(|scope| scope.handle == *branch)
                            .ok_or(Error::PermissionDenied)?;
                        let selected = self.api.select_research_branch(
                            project,
                            run.id,
                            work_node_id,
                            scope.work_node_id,
                            scope.version,
                        )?;
                        tool_output = Some(ToolOutput {
                            schema_version: 1,
                            data: serde_json::json!({"kind":"branch_selected","handle":scope.handle,"name":scope.name,"selected_work_node_id":selected.id}),
                            metadata: serde_json::json!({"runtime_observation":true}),
                        });
                    }
                    ProcessAction::ProposeWorkPlan { steps, reason } => {
                        text(reason, "work strategy reason")?;
                        if steps
                            .iter()
                            .any(|step| !available_child_tools.contains(&step.required_tool))
                        {
                            return Err(Error::PermissionDenied);
                        }
                        let expected_revision = snapshot.plan_revision_id;
                        submission = Some(self.api.submit(
                            project,
                            run.id,
                            ActionEnvelope::new(ProposedAction::ReviseWorkPlan {
                                work_node_id,
                                expected_version: work.meta.version,
                                expected_revision,
                                steps: steps.clone(),
                            }),
                            reason.clone(),
                        )?);
                        finish_run = !matches!(&submission, Some(Submission::Gate(_)));
                        if finish_run {
                            let observation = serde_json::json!({"kind":"existing_plan","work_node_id":work_node_id,"instruction":"The executable work and dependencies already exist. Continue ready work, use completed results or make an explicit research decision."});
                            self.api.write_process_memory(
                                project,
                                run.id,
                                work_node_id,
                                format!("Planning observation: {observation}"),
                            )?;
                            tool_output = Some(ToolOutput {
                                schema_version: 1,
                                data: observation,
                                metadata: serde_json::json!({"runtime_observation":true,"no_progress":true}),
                            });
                        }
                    }
                    ProcessAction::ProposeObjectiveRevision { objective, reason } => {
                        text(reason, "plan revision reason")?;
                        submission = Some(self.api.submit(
                            project,
                            run.id,
                            ActionEnvelope::new(ProposedAction::ReviseWorkObjective {
                                work_node_id,
                                expected_version: work.meta.version,
                                objective: objective.clone(),
                            }),
                            reason.clone(),
                        )?);
                        finish_run = !matches!(&submission, Some(Submission::Gate(_)));
                        if finish_run {
                            let observation = serde_json::json!({"kind":"existing_plan","work_node_id":work_node_id,"objective":work.objective,"instruction":"The objective is unchanged; no Gate or PlanRevision was created. Execute a concrete evidence task, use existing results, or propose a materially changed objective."});
                            self.api.write_process_memory(
                                project,
                                run.id,
                                work_node_id,
                                format!("Planning observation: {observation}"),
                            )?;
                            tool_output = Some(ToolOutput {
                                schema_version: 1,
                                data: observation,
                                metadata: serde_json::json!({"runtime_observation":true,"no_progress":true}),
                            });
                        }
                    }
                    ProcessAction::ConcludeWorkInconclusive { summary } => {
                        self.api.conclude_work_inconclusive(
                            project,
                            run.id,
                            work_node_id,
                            summary.clone(),
                        )?;
                        failure_guard.disarm();
                        finish_run = false;
                    }
                    ProcessAction::CompleteWorkNode {
                        work_node_id: target,
                        summary,
                    } => {
                        if *target != work_node_id {
                            return Err(trace_permission_denial(
                                Error::PermissionDenied,
                                self.api.identity(),
                                project,
                                work.branch_id,
                                "CompleteWorkNode",
                                "WorkNode",
                                Some(target.to_string()),
                                "complete-bound-work-node",
                                vec![work_node_id.to_string()],
                                "Lead targeted a WorkNode other than the active execution binding",
                            ));
                        }
                        // Validate acceptance before making the run terminal,
                        // then transition after closing this execution binding;
                        // Core forbids changing work state while a run is active.
                        self.api
                            .validate_work_completion(project, run.id, work_node_id)?;
                        if let Some(summary) = summary {
                            self.api.record_work_result(
                                project,
                                run.id,
                                work_node_id,
                                summary.clone(),
                            )?;
                        }
                        self.api.finish(project, run.id, RunStatus::Completed)?;
                        self.api.complete_work(project, run.id, work_node_id)?;
                        failure_guard.disarm();
                        finish_run = false;
                    }
                    ProcessAction::CreateBranch { name } => {
                        self.api.create_branch(project, run.id, name.clone())?;
                    }
                    ProcessAction::FinishResearch {
                        outcome,
                        summary,
                        reviewed_branches,
                    } => {
                        if work.granularity != WorkGranularity::Composite {
                            return Err(Error::InvalidTransition(
                                "only a composite research goal can finish".into(),
                            ));
                        }
                        let reviewed = reviewed_branches
                            .iter()
                            .map(|handle| {
                                research_branches
                                    .iter()
                                    .find(|scope| scope.handle == *handle)
                                    .map(|scope| scope.work_node_id)
                                    .ok_or(Error::PermissionDenied)
                            })
                            .collect::<Result<Vec<_>>>()?;
                        self.api.finish_research_with_branch_reviews(
                            project,
                            run.id,
                            work_node_id,
                            *outcome,
                            summary.clone(),
                            reviewed,
                        )?;
                        failure_guard.disarm();
                        finish_run = false;
                    }
                }
            }
        }
        if !matches!(
            &decision,
            AgentDecision::Process {
                action: ProcessAction::CompleteWorkNode { .. }
                    | ProcessAction::FinishResearch { .. }
                    | ProcessAction::ConcludeWorkInconclusive { .. },
                ..
            }
        ) {
            self.api.checkpoint_step(
                project,
                run.id,
                work_node_id,
                snapshot.id,
                serde_json::json!({"decision":decision,"work_node_id":work_node_id}),
            )?;
        }
        if finish_run {
            self.api.finish(project, run.id, RunStatus::Completed)?;
            failure_guard.disarm();
        } else if matches!(&submission, Some(Submission::Gate(_))) {
            failure_guard.disarm();
        }
        if diagnostic_trace_enabled {
            self.api.update_decision_trace_outcome(
                project,
                invocation_id,
                serde_json::json!({
                    "validation": {"step_contract":step_contract_ok,"process_action_constraint":process_constraint_ok},
                    "executed_action":trace_action_name,
                    "execution_status":"completed",
                    "tool_output":tool_output.clone(),
                    "progress_before":progress_before.clone(),
                    "progress_after":self.api.lead_progress_snapshot(project, work_node_id)?,
                }),
            )?;
        }
        Ok(AgentStepOutcome {
            task_id: task_record.id,
            run_id: run.id,
            decision,
            tool_output,
            submission,
        })
    }
    async fn decide(
        &self,
        project: ProjectId,
        run: RunId,
        goal: String,
        focus: Option<EntityRef>,
    ) -> Result<(Option<Submission>, Option<String>)> {
        let task = ModelTask {
            goal,
            focus,
            current_work: None,
            required_tool_succeeded: false,
            retrieval_pending: false,
            retrieval_source_handles: Vec::new(),
        };
        let selection = self.selector.select(&task)?;
        let selection_metadata =
            serde_json::to_value(&selection).map_err(|e| Error::Model(e.to_string()))?;
        safe_metadata(&selection_metadata)?;
        let context = ContextBuilder::build(&self.api, project, focus)?;
        let mut tool_results = vec![];
        for _ in 0..4 {
            let response = self
                .provider
                .generate(ModelRequest {
                    task: task.clone(),
                    context: context.clone(),
                    compiled_context: None,
                    selection: selection.clone(),
                    output_schema_version: 1,
                    max_output_tokens: 384,
                    tools: self.tools.descriptors(),
                    available_child_tools: self
                        .tools
                        .descriptors()
                        .into_iter()
                        .map(|tool| tool.name)
                        .collect(),
                    allowed_process_actions: None,
                    tool_results: tool_results.clone(),
                    research_branches: vec![],
                })
                .await?;
            if response.schema_version != 1 {
                return Err(Error::Model(
                    "unsupported structured response version".into(),
                ));
            }
            safe_metadata(&response.metadata)?;
            self.api.record_provider(project,run,serde_json::json!({"schema_version":1,"selection":selection_metadata,"response":response.metadata}))?;
            match response.decision {
                AgentDecision::Propose { action, reason } => {
                    text(&reason, "proposal reason")?;
                    return Ok((
                        Some(
                            self.api
                                .submit(project, run, ActionEnvelope::new(action), reason)?,
                        ),
                        None,
                    ));
                }
                AgentDecision::NoChange { summary } => {
                    text(&summary, "analysis summary")?;
                    safe_metadata(&Value::String(summary.clone()))?;
                    return Ok((None, Some(summary)));
                }
                AgentDecision::UseTool { name, input } => {
                    let output = self
                        .tools
                        .execute(
                            &name,
                            input,
                            ToolContext {
                                project_id: project,
                                run_id: Some(run),
                                context_snapshot_id: None,
                                tool_invocation_id: None,
                            },
                        )
                        .await?;
                    tool_results.push(NamedToolResult { name, output });
                }
                AgentDecision::Process { .. } => {
                    return Err(Error::InvalidTransition(
                        "process actions require the bound long-horizon runtime".into(),
                    ));
                }
            }
        }
        Err(Error::Model("agent exceeded four model steps".into()))
    }
}
mod durable;
mod intelligence;
pub use durable::*;
pub use intelligence::*;

#[cfg(test)]
mod retrieval_binding_tests {
    use super::*;
    use serde_json::json;

    fn protocol() -> RetrievalProtocol {
        RetrievalProtocol {
            search_tool: "search_sources".into(),
            refine_tool: "refine_search".into(),
            read_tools: vec!["read_source".into()],
            reject_tool: "reject_retrieved_sources".into(),
        }
    }
    fn descriptors() -> Vec<ToolDescriptor> {
        [
            "search_sources",
            "read_source",
            "refine_search",
            "reject_retrieved_sources",
            "unrelated",
        ]
        .into_iter()
        .map(|name| ToolDescriptor {
            name: name.into(),
            description: name.into(),
            read_only: true,
        })
        .collect()
    }
    #[test]
    fn required_read_without_source_allows_prerequisite_resolution() {
        let tools = protocol().executable_tools(&descriptors(), Some("read_source"), false, &[]);
        assert_eq!(
            tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["search_sources"]
        );
    }
    #[test]
    fn existing_source_can_be_read_without_new_search() {
        let tools =
            protocol().executable_tools(&descriptors(), Some("read_source"), false, &["S1".into()]);
        assert_eq!(
            tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["read_source"]
        );
    }
    #[test]
    fn action_prerequisite_matrix_preserves_scope_and_argument_domains() {
        for pending in [false, true] {
            for handles in [vec![], vec!["S1".into()]] {
                for required in [
                    None,
                    Some("read_source"),
                    Some("search_sources"),
                    Some("unrelated"),
                ] {
                    let tools =
                        protocol().executable_tools(&descriptors(), required, pending, &handles);
                    for tool in tools {
                        match tool.name.as_str() {
                            "read_source" => assert!(!handles.is_empty()),
                            "search_sources" => assert!(
                                !pending
                                    && (required == Some("search_sources")
                                        || required == Some("read_source"))
                            ),
                            "refine_search" | "reject_retrieved_sources" => assert!(pending),
                            "unrelated" => assert!(!pending && required == Some("unrelated")),
                            _ => panic!("unregistered capability"),
                        }
                    }
                }
            }
        }
        let only_read = vec![descriptors()[1].clone()];
        assert!(protocol()
            .executable_tools(&only_read, Some("read_source"), false, &[])
            .is_empty());
    }
    #[test]
    fn partially_consumed_batch_can_be_refined_or_rejected() {
        let id = ToolInvocationId::new();
        let mut batch = pending_batch(id);
        batch.status = RetrievalBatchStatus::PartiallyConsumed;
        assert_eq!(resolve_pending_search_id(&[batch]).unwrap(), id);
    }

    fn pending_batch(search_id: ToolInvocationId) -> RetrievalBatchState {
        RetrievalBatchState {
            search_invocation_id: search_id,
            run_id: RunId::new(),
            work_node_id: WorkNodeId::new(),
            branch_id: BranchId::new(),
            query: "prior query".into(),
            returned: vec![],
            consumed_handles: BTreeSet::new(),
            status: RetrievalBatchStatus::Pending,
        }
    }

    #[test]
    fn model_refine_and_reject_inputs_bind_to_the_single_pending_search() {
        let search_id = ToolInvocationId::new();
        let batch = pending_batch(search_id);
        let refine = ToolInput {
            schema_version: 1,
            parameters: json!({"reason":"results lack dates","query":"dated results"}),
        };
        let bound_refine =
            bind_pending_search_id(&refine, std::slice::from_ref(&batch), "previous_search_id")
                .unwrap();
        assert_eq!(
            bound_refine.parameters["previous_search_id"],
            json!(search_id)
        );
        assert_eq!(bound_refine.parameters["reason"], "results lack dates");
        assert_eq!(bound_refine.parameters["query"], "dated results");

        let reject = ToolInput {
            schema_version: 1,
            parameters: json!({"reason":"sources are outside the topic","search_invocation_id":"forged"}),
        };
        let bound_reject =
            bind_pending_search_id(&reject, &[batch], "search_invocation_id").unwrap();
        assert_eq!(
            bound_reject.parameters["search_invocation_id"],
            json!(search_id)
        );
        assert_eq!(
            bound_reject.parameters["reason"],
            "sources are outside the topic"
        );
    }

    #[test]
    fn missing_or_ambiguous_pending_search_is_rejected_without_guessing() {
        assert!(matches!(
            resolve_pending_search_id(&[]),
            Err(Error::InvalidTransition(message)) if message == "NoPendingRetrieval"
        ));
        let reason_only = ToolInput {
            schema_version: 1,
            parameters: serde_json::json!({"reason":"no usable sources"}),
        };
        for field in ["previous_search_id", "search_invocation_id"] {
            assert!(matches!(
                bind_pending_search_id(&reason_only, &[], field),
                Err(Error::InvalidTransition(message)) if message == "NoPendingRetrieval"
            ));
        }
        assert!(matches!(
            resolve_pending_search_id(&[
                pending_batch(ToolInvocationId::new()),
                pending_batch(ToolInvocationId::new()),
            ]),
            Err(Error::InvalidTransition(message)) if message == "AmbiguousPendingRetrieval"
        ));
    }
}

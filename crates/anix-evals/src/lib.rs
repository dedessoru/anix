pub mod long_horizon;
pub mod true_capabilities;
pub use long_horizon::{run_campaign as run_long_horizon_campaign, LongHorizonReport};
pub use true_capabilities::{run_capability_case, write_capability_campaign, CapabilityRunReport};

use anix_agent::{
    FixedModelSelector, LeadAgent, LocalOnlyTestProvider, ModelProvider, ModelSelection,
    ProviderLocality,
};
use anix_core::{CommandContext, Core, NewClaim, NewPaper, NewSource, NewWork, Submission};
use anix_domain::*;
use anix_integrations::{OpenAiCompatibleModelProvider, TestLeadConfig, Workspace};
use anix_storage::SqliteStorage;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioKind {
    BasicEvidence,
    ContradictoryEvidence,
    DuplicateSources,
    Branching,
    LongContext,
    SparseEvidence,
    ExperimentProposal,
    FailedExperiment,
    Revision,
    InterruptedResearch,
    ToolFailure,
}
impl ScenarioKind {
    pub const ALL: [Self; 11] = [
        Self::BasicEvidence,
        Self::ContradictoryEvidence,
        Self::DuplicateSources,
        Self::Branching,
        Self::LongContext,
        Self::SparseEvidence,
        Self::ExperimentProposal,
        Self::FailedExperiment,
        Self::Revision,
        Self::InterruptedResearch,
        Self::ToolFailure,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::BasicEvidence => "basic-evidence",
            Self::ContradictoryEvidence => "contradictory-evidence",
            Self::DuplicateSources => "duplicate-sources",
            Self::Branching => "branching",
            Self::LongContext => "long-context",
            Self::SparseEvidence => "sparse-evidence",
            Self::ExperimentProposal => "experiment-proposal",
            Self::FailedExperiment => "failed-experiment",
            Self::Revision => "revision",
            Self::InterruptedResearch => "interrupted-research",
            Self::ToolFailure => "tool-failure",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchBudget {
    pub max_steps: u32,
    pub max_model_invocations: u32,
    pub max_tool_invocations: u32,
    pub max_context_tokens: u32,
    pub max_wall_time_seconds: u64,
}
impl Default for ResearchBudget {
    fn default() -> Self {
        Self {
            max_steps: 12,
            max_model_invocations: 4,
            max_tool_invocations: 8,
            max_context_tokens: 8192,
            max_wall_time_seconds: 900,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaultKind {
    ModelTimeout,
    MalformedStructuredOutput,
    ModelUnavailable,
    ToolTimeout,
    ToolFailure,
    MalformedToolResult,
    ProcessKillSimulation,
    DatabaseOperationFailure,
    ContextOverflow,
    DuplicateAction,
    CancelledRun,
    StaleGate,
    ArtifactReadFailure,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FaultPlan {
    pub seed: u64,
    pub after_model_call: Option<u32>,
    pub after_checkpoint: Option<String>,
    pub fault: FaultKind,
}
impl FaultPlan {
    pub fn triggered(&self, model_calls: u32, checkpoint: Option<&str>) -> bool {
        self.after_model_call.is_some_and(|n| n == model_calls)
            || self
                .after_checkpoint
                .as_deref()
                .is_some_and(|n| Some(n) == checkpoint)
    }
}

#[derive(Clone, Debug)]
pub struct BudgetUsage {
    pub steps: u32,
    pub model_invocations: u32,
    pub tool_invocations: u32,
    pub context_tokens: u64,
    pub started: Instant,
}
impl BudgetUsage {
    pub fn new() -> Self {
        Self {
            steps: 0,
            model_invocations: 0,
            tool_invocations: 0,
            context_tokens: 0,
            started: Instant::now(),
        }
    }
    pub fn check(&self, b: &ResearchBudget) -> Result<()> {
        if self.steps > b.max_steps
            || self.model_invocations > b.max_model_invocations
            || self.tool_invocations > b.max_tool_invocations
            || self.context_tokens > b.max_context_tokens as u64
            || self.started.elapsed().as_secs() > b.max_wall_time_seconds
        {
            Err(Error::Validation("research budget exceeded".into()))
        } else {
            Ok(())
        }
    }
}
impl Default for BudgetUsage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct ActionLoopGuard {
    limit: usize,
    seen: BTreeMap<String, usize>,
}
impl ActionLoopGuard {
    pub fn new(limit: usize) -> Result<Self> {
        if limit < 2 {
            return Err(Error::Validation(
                "loop threshold must be at least two".into(),
            ));
        }
        Ok(Self {
            limit,
            seen: BTreeMap::new(),
        })
    }
    pub fn observe(&mut self, action: &Value) -> Result<()> {
        let key = serde_json::to_string(action).map_err(|e| Error::Validation(e.to_string()))?;
        let n = self.seen.entry(key).or_default();
        *n += 1;
        if *n >= self.limit {
            Err(Error::InvalidTransition(
                "RunBlocked: LoopDetected repeated action fingerprint".into(),
            ))
        } else {
            Ok(())
        }
    }
}

pub struct InjectingModelProvider {
    pub inner: Arc<dyn ModelProvider>,
    pub plan: FaultPlan,
    calls: std::sync::atomic::AtomicU32,
}
impl InjectingModelProvider {
    pub fn new(inner: Arc<dyn ModelProvider>, plan: FaultPlan) -> Self {
        Self {
            inner,
            plan,
            calls: std::sync::atomic::AtomicU32::new(0),
        }
    }
}
impl ModelProvider for InjectingModelProvider {
    fn generate(&self, request: anix_agent::ModelRequest) -> anix_agent::ModelFuture<'_> {
        Box::pin(async move {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if self.plan.triggered(n, None) {
                return Err(match self.plan.fault {
                    FaultKind::ModelTimeout => Error::Model("injected model timeout".into()),
                    FaultKind::ModelUnavailable => {
                        Error::Model("injected unavailable model".into())
                    }
                    FaultKind::MalformedStructuredOutput => {
                        Error::Model("injected malformed structured output".into())
                    }
                    FaultKind::CancelledRun => {
                        Error::InvalidTransition("injected cancelled run".into())
                    }
                    _ => Error::Model(format!(
                        "fault {:?} is not a model-provider fault",
                        self.plan.fault
                    )),
                });
            }
            self.inner.generate(request).await
        })
    }
    fn locality(&self) -> ProviderLocality {
        self.inner.locality()
    }
}

pub struct LoopGuardProvider {
    inner: Arc<dyn ModelProvider>,
    guard: std::sync::Mutex<ActionLoopGuard>,
}
impl LoopGuardProvider {
    pub fn new(inner: Arc<dyn ModelProvider>, repeats: usize) -> Result<Self> {
        Ok(Self {
            inner,
            guard: std::sync::Mutex::new(ActionLoopGuard::new(repeats)?),
        })
    }
}
impl ModelProvider for LoopGuardProvider {
    fn generate(&self, request: anix_agent::ModelRequest) -> anix_agent::ModelFuture<'_> {
        Box::pin(async move {
            let result = self.inner.generate(request).await?;
            let fingerprint =
                serde_json::to_value(&result.decision).map_err(|e| Error::Model(e.to_string()))?;
            self.guard
                .lock()
                .map_err(|_| Error::Model("loop guard lock".into()))?
                .observe(&fingerprint)?;
            Ok(result)
        })
    }
    fn locality(&self) -> ProviderLocality {
        self.inner.locality()
    }
}

pub struct MeteredProvider {
    inner: Arc<dyn ModelProvider>,
    calls: Arc<std::sync::atomic::AtomicU64>,
}
impl ModelProvider for MeteredProvider {
    fn generate(&self, request: anix_agent::ModelRequest) -> anix_agent::ModelFuture<'_> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.generate(request)
    }
    fn locality(&self) -> ProviderLocality {
        self.inner.locality()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AuditReport {
    pub passed: bool,
    pub checked: u64,
    pub violations: Vec<AuditViolation>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditViolation {
    pub invariant: String,
    pub entity: Option<String>,
    pub detail: String,
}
impl AuditReport {
    fn violation(
        &mut self,
        invariant: &str,
        entity: impl Into<Option<String>>,
        detail: impl Into<String>,
    ) {
        self.violations.push(AuditViolation {
            invariant: invariant.into(),
            entity: entity.into(),
            detail: detail.into(),
        });
    }
}

pub fn audit_project(state: &ProjectState) -> AuditReport {
    let mut r = AuditReport {
        passed: true,
        checked: 0,
        violations: vec![],
    };
    let branch_ids: BTreeSet<_> = state.branches.keys().copied().collect();
    for e in state.work_edges.values() {
        r.checked += 1;
        if !state.work_nodes.contains_key(&e.from)
            || !state.work_nodes.contains_key(&e.to)
            || !branch_ids.contains(&e.branch_id)
        {
            r.violation(
                "process_edge_references",
                Some(e.id.to_string()),
                "edge endpoint or branch is missing",
            );
        }
    }
    let mut graph: BTreeMap<WorkNodeId, Vec<WorkNodeId>> = BTreeMap::new();
    for e in state.work_edges.values() {
        if e.kind == WorkEdgeKind::DependsOn || e.kind == WorkEdgeKind::Contains {
            graph.entry(e.from).or_default().push(e.to);
        }
    }
    fn visit(
        id: WorkNodeId,
        g: &BTreeMap<WorkNodeId, Vec<WorkNodeId>>,
        gray: &mut BTreeSet<WorkNodeId>,
        black: &mut BTreeSet<WorkNodeId>,
    ) -> bool {
        if black.contains(&id) {
            return false;
        }
        if !gray.insert(id) {
            return true;
        }
        let cycle = g
            .get(&id)
            .is_some_and(|ns| ns.iter().any(|n| visit(*n, g, gray, black)));
        gray.remove(&id);
        black.insert(id);
        cycle
    }
    let mut gray = BTreeSet::new();
    let mut black = BTreeSet::new();
    for id in state.work_nodes.keys() {
        if visit(*id, &graph, &mut gray, &mut black) {
            r.violation(
                "process_dag_acyclic",
                Some(id.to_string()),
                "cycle in process dependency/containment edges",
            );
            break;
        }
    }
    for b in state.execution_bindings.values() {
        r.checked += 1;
        if !state.runs.contains_key(&b.id)
            || !state.work_nodes.contains_key(&b.work_node_id)
            || !state.branches.contains_key(&b.branch_id)
            || !state.plan_revisions.contains_key(&b.plan_revision_id)
        {
            r.violation(
                "execution_binding_resolves",
                Some(b.id.to_string()),
                "binding references missing execution state",
            );
        }
    }
    for i in state.model_invocations.values() {
        r.checked += 1;
        if !state.runs.contains_key(&i.run_id)
            || !state.work_nodes.contains_key(&i.work_node_id)
            || !state.context_snapshots.contains_key(&i.context_snapshot_id)
        {
            r.violation(
                "model_invocation_resolves",
                Some(i.id.to_string()),
                "model invocation references missing run, work, or snapshot",
            );
        }
    }
    for i in state.tool_invocations.values() {
        r.checked += 1;
        if !state.runs.contains_key(&i.run_id) || !state.work_nodes.contains_key(&i.work_node_id) {
            r.violation(
                "tool_invocation_resolves",
                Some(i.id.to_string()),
                "tool invocation references missing run or work",
            );
        }
    }
    for s in state.context_snapshots.values() {
        r.checked += 1;
        if !state.work_nodes.contains_key(&s.work_node_id)
            || !state.plan_revisions.contains_key(&s.plan_revision_id)
            || s.estimated_tokens > s.budget_tokens
        {
            r.violation(
                "context_snapshot_resolves_and_within_budget",
                Some(s.id.to_string()),
                "missing work/revision or token budget exceeded",
            );
        }
    }
    for c in state.claims.values() {
        r.checked += 1;
        if !state.papers.contains_key(&c.paper_id)
            || c.source_id
                .is_none_or(|id| !state.sources.contains_key(&id))
        {
            r.violation(
                "claim_source_trace",
                Some(c.id.to_string()),
                "claim paper or source is missing",
            );
        }
        if !state
            .provenance
            .iter()
            .any(|p| p.entity == Some(EntityRef::Claim(c.id)))
        {
            r.violation(
                "claim_has_provenance",
                Some(c.id.to_string()),
                "claim has no provenance record",
            );
        }
    }
    for f in state.findings.values() {
        r.checked += 1;
        if !state.experiment_runs.contains_key(&f.experiment_run_id) {
            r.violation(
                "finding_origin_resolves",
                Some(f.id.to_string()),
                "finding has no experiment run",
            );
        }
    }
    for g in state.gates.values() {
        r.checked += 1;
        if g.status != GateStatus::Pending && g.resolved_at.is_none() {
            r.violation(
                "gate_resolution_audited",
                Some(g.id.to_string()),
                "resolved gate has no resolution timestamp",
            );
        }
    }
    for cp in state.checkpoints.values() {
        r.checked += 1;
        if !state.branches.contains_key(&cp.branch_id)
            || !state.plan_revisions.contains_key(&cp.plan_revision_id)
            || cp.run_id.is_some_and(|id| !state.runs.contains_key(&id))
            || cp
                .context_snapshot_id
                .is_some_and(|id| !state.context_snapshots.contains_key(&id))
        {
            r.violation(
                "checkpoint_resolves",
                Some(cp.id.to_string()),
                "checkpoint references missing records",
            );
        }
    }
    r.passed = r.violations.is_empty();
    r
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub scenario: String,
    pub seed: u64,
    pub project_id: ProjectId,
    pub status: String,
    pub elapsed_ms: u64,
    pub model_invocations: u64,
    pub lead_model_invocations: u64,
    pub micro_model_invocations: u64,
    pub tool_invocations: u64,
    pub claim_count: usize,
    pub relation_count: usize,
    pub duplicate_actions_prevented: usize,
    pub lead_input_tokens: u64,
    pub lead_output_tokens: u64,
    pub lead_preflight_input_tokens: u64,
    pub micro_input_tokens: u64,
    pub micro_output_tokens: u64,
    pub max_context_tokens: u32,
    pub average_context_tokens: u32,
    pub retries: u32,
    pub malformed_model_outputs: u32,
    pub schema_failures: u32,
    pub loops_detected: u32,
    pub restarts: u32,
    pub resumes: u32,
    pub completed_work_preserved_after_resume: bool,
    pub hypothesis_count: usize,
    pub experiment_count: usize,
    pub experiment_run_count: usize,
    pub failed_experiment_count: usize,
    pub finding_count: usize,
    pub gate_count: usize,
    pub branch_count: usize,
    pub checkpoint_count: usize,
    pub audit: AuditReport,
    pub error: Option<String>,
    pub model: String,
    pub profile: String,
}

fn test_human_resolve_gate(
    core: &Core,
    project: ProjectId,
    id: GateId,
) -> Result<Option<ActionResult>> {
    test_human_resolve_gate_as(core, project, id, "anix-evals")
}

fn test_human_resolve_gate_as(
    core: &Core,
    project: ProjectId,
    id: GateId,
    human: &str,
) -> Result<Option<ActionResult>> {
    let gate = core.gates().get(project, id)?;
    let safe = matches!(
        gate.proposed_action.action,
        ProposedAction::ReviseWorkObjective { .. }
            | ProposedAction::ReviseWorkPlan { .. }
            | ProposedAction::CreateExperiment(_)
            | ProposedAction::ActivateExperiment { .. }
            | ProposedAction::ReviseHypothesis { .. }
            | ProposedAction::CreateHypothesis(_)
    );
    if safe {
        Ok(Some(core.gates().approve(
            project,
            id,
            CommandContext::human(
                human,
                "test-human policy approves safe synthetic work revision, experiment or hypothesis proposal",
            ),
        )?))
    } else {
        let _ = core.gates().reject(
            project,
            id,
            CommandContext::human(
                human,
                "test-human policy rejects unsafe or unsupported proposal",
            ),
        )?;
        Ok(None)
    }
}

fn propose_via_gate(
    core: &Core,
    project: ProjectId,
    action: ProposedAction,
    reason: &str,
) -> Result<ActionResult> {
    match core.gates().propose(
        project,
        ActionEnvelope::new(action),
        ctx(reason),
        "Synthetic research test-human gate".into(),
    )? {
        Submission::Applied(result) => Ok(result),
        Submission::Gate(id) => {
            test_human_resolve_gate(core, project, id)?.ok_or(Error::GateRequired)
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchReport {
    pub date: String,
    pub requested_runs: usize,
    pub completed_runs: usize,
    pub invariant_passes: usize,
    pub resume_successes: usize,
    pub loop_rate: f64,
    pub duplicate_work_rate: f64,
    pub malformed_output_rate: f64,
    pub mean_model_calls: f64,
    pub mean_tool_calls: f64,
    pub mean_lead_input_tokens: f64,
    pub mean_lead_output_tokens: f64,
    pub mean_lead_preflight_input_tokens: f64,
    pub mean_micro_input_tokens: f64,
    pub mean_micro_output_tokens: f64,
    pub mean_context_tokens: f64,
    pub p50_runtime_ms: u64,
    pub p95_runtime_ms: u64,
    pub runs: Vec<RunReport>,
}

fn ctx(reason: &str) -> CommandContext {
    CommandContext::human("anix-evals", reason)
}
#[derive(Deserialize)]
struct PackCatalog {
    schema_version: u32,
    scenarios: Vec<PackEntry>,
}
#[derive(Deserialize)]
struct PackEntry {
    id: String,
    question: String,
    documents: Vec<String>,
    ground_truth: Vec<String>,
}
fn scenario_text(kind: ScenarioKind, variation: u64) -> Result<(String, Vec<String>, Vec<String>)> {
    let catalog: PackCatalog =
        serde_json::from_str(include_str!("../../../tests/research-packs/catalog.json"))
            .map_err(|e| Error::Validation(e.to_string()))?;
    if catalog.schema_version != 1 {
        return Err(Error::Validation(
            "unsupported research pack catalog".into(),
        ));
    }
    let mut entry = catalog
        .scenarios
        .into_iter()
        .find(|p| p.id == kind.name())
        .ok_or_else(|| Error::NotFound(format!("research pack {}", kind.name())))?;
    if kind == ScenarioKind::LongContext {
        // Keep the corpus large enough to exercise context selection, while
        // leaving room for the actual agent request and response in an 8K profile.
        for i in 0..(12 + variation as usize % 5) {
            entry.documents.push(format!("Irrelevant record {i}: administrative note with no outcome data; variation {variation}."));
        }
        let detailed_record = (0..180)
            .map(|i| format!(
                "Long-context report segment {i}: intervention X changed synthetic endpoint Y by seven percent in the pilot, with a bounded sample, calibration note, and explicit uncertainty. "
            ))
            .collect::<String>();
        entry.documents.push(detailed_record);
    }
    if variation % 2 == 1 {
        entry.documents.reverse();
    }
    if variation > 0 {
        entry.question = format!(
            "{} (wording variation {variation}; use only supplied evidence)",
            entry.question
        );
    }
    Ok((entry.question, entry.documents, entry.ground_truth))
}

pub struct HarnessConfig {
    pub workspace: PathBuf,
    pub endpoint: String,
    pub model_name: String,
    pub context_size: u32,
    pub temperature: f32,
    pub timeout_seconds: u64,
    pub capabilities: BTreeSet<String>,
    pub budget: ResearchBudget,
}
impl HarnessConfig {
    pub fn local_default(workspace: impl Into<PathBuf>) -> Self {
        let p = TestLeadConfig::local_default();
        Self {
            workspace: workspace.into(),
            endpoint: p.endpoint,
            model_name: p.model_name,
            context_size: p.context_size,
            temperature: p.temperature,
            timeout_seconds: p.timeout_seconds,
            capabilities: p.capabilities,
            budget: ResearchBudget::default(),
        }
    }
    pub fn read(workspace: impl Into<PathBuf>) -> Result<Self> {
        let workspace = workspace.into();
        let path = std::env::var_os("ANIX_TEST_LEAD_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace.join("config/local-test-lead.toml"));
        let profile = if path.is_file() {
            TestLeadConfig::read(&path)?
        } else {
            TestLeadConfig::local_default()
        };
        Ok(Self {
            workspace,
            endpoint: profile.endpoint,
            model_name: profile.model_name,
            context_size: profile.context_size,
            temperature: profile.temperature,
            timeout_seconds: profile.timeout_seconds,
            capabilities: profile.capabilities,
            budget: ResearchBudget {
                max_context_tokens: profile.context_size,
                ..ResearchBudget::default()
            },
        })
    }
}

pub fn run_batch(config: &HarnessConfig, count: usize, seed: u64) -> Result<BatchReport> {
    if count != 10 && count != 50 {
        return Err(Error::Validation(
            "research batch count must be exactly 10 or 50".into(),
        ));
    }
    let provider_cfg = TestLeadConfig {
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
    let provider = Arc::new(OpenAiCompatibleModelProvider::new(provider_cfg)?);
    run_batch_with_provider(config, count, seed, provider)
}
pub fn run_scenario(config: &HarnessConfig, name: &str, seed: u64) -> Result<RunReport> {
    let kind = ScenarioKind::ALL
        .into_iter()
        .find(|k| k.name() == name)
        .ok_or_else(|| Error::Validation(format!("unknown research scenario: {name}")))?;
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
    let provider: Arc<dyn ModelProvider> = Arc::new(LoopGuardProvider::new(
        Arc::new(LocalOnlyTestProvider::new(Arc::new(
            OpenAiCompatibleModelProvider::new(p)?,
        ))),
        3,
    )?);
    let ws = Workspace::open(&config.workspace)?;
    let db_path = ws.database_path()?;
    let core = Core::new(Arc::new(SqliteStorage::open(&db_path)?));
    let mut report = run_one(&core, kind, seed, seed, provider, config);
    drop(core);
    verify_restart(&db_path, &mut report)?;
    let dir = config
        .workspace
        .join("eval-results")
        .join(Utc::now().format("%Y-%m-%d").to_string())
        .join(format!("run-{seed:06}"));
    std::fs::create_dir_all(&dir).map_err(|e| Error::Storage(e.to_string()))?;
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| Error::Validation(e.to_string()))?,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    Ok(report)
}
pub fn run_batch_with_provider(
    config: &HarnessConfig,
    count: usize,
    seed: u64,
    provider: Arc<dyn ModelProvider>,
) -> Result<BatchReport> {
    if count != 10 && count != 50 {
        return Err(Error::Validation(
            "research batch count must be exactly 10 or 50".into(),
        ));
    }
    if provider.locality() != ProviderLocality::Local {
        return Err(Error::PermissionDenied);
    }
    let local: Arc<dyn ModelProvider> = Arc::new(LocalOnlyTestProvider::new(provider));
    let ws = Workspace::open(&config.workspace)?;
    let db_path = ws.database_path()?;
    let now = Utc::now();
    let date = now.format("%Y-%m-%d").to_string();
    let result_dir =
        config
            .workspace
            .join("eval-results")
            .join(format!("{}-{}", date, now.format("%H%M%S")));
    std::fs::create_dir_all(&result_dir).map_err(|e| Error::Storage(e.to_string()))?;
    let mut runs = Vec::with_capacity(count);
    for i in 0..count {
        let kind = ScenarioKind::ALL[i % ScenarioKind::ALL.len()];
        let variation = seed.wrapping_add((i / 10) as u64);
        // Loop fingerprints are scoped to one research run. Reusing this guard
        // across the batch falsely blocks the same legitimate action in a new project.
        let guarded: Arc<dyn ModelProvider> = Arc::new(LoopGuardProvider::new(local.clone(), 3)?);
        let core = Core::new(Arc::new(SqliteStorage::open(&db_path)?));
        let mut report = run_one(
            &core,
            kind,
            seed.wrapping_add(i as u64),
            variation,
            guarded,
            config,
        );
        drop(core);
        verify_restart(&db_path, &mut report)?;
        let name = format!("run-{:03}", i + 1);
        let path = result_dir.join(&name);
        std::fs::create_dir_all(&path).map_err(|e| Error::Storage(e.to_string()))?;
        let bytes =
            serde_json::to_vec_pretty(&report).map_err(|e| Error::Validation(e.to_string()))?;
        std::fs::write(path.join("report.json"), bytes)
            .map_err(|e| Error::Storage(e.to_string()))?;
        runs.push(report);
    }
    let completed_runs = runs.iter().filter(|r| r.status == "completed").count();
    let invariant_passes = runs.iter().filter(|r| r.audit.passed).count();
    let mut durations: Vec<_> = runs.iter().map(|r| r.elapsed_ms).collect();
    durations.sort_unstable();
    let average = |f: fn(&RunReport) -> u64| runs.iter().map(f).sum::<u64>() as f64 / count as f64;
    let batch = BatchReport {
        date,
        requested_runs: count,
        completed_runs,
        invariant_passes,
        resume_successes: runs
            .iter()
            .filter(|r| r.completed_work_preserved_after_resume)
            .count(),
        loop_rate: runs.iter().filter(|r| r.loops_detected > 0).count() as f64 / count as f64,
        duplicate_work_rate: runs
            .iter()
            .map(|r| r.duplicate_actions_prevented)
            .sum::<usize>() as f64
            / runs.iter().map(|r| r.claim_count).sum::<usize>().max(1) as f64,
        malformed_output_rate: runs
            .iter()
            .filter(|r| r.malformed_model_outputs > 0)
            .count() as f64
            / count as f64,
        mean_model_calls: average(|r| r.model_invocations),
        mean_tool_calls: average(|r| r.tool_invocations),
        mean_lead_input_tokens: average(|r| r.lead_input_tokens),
        mean_lead_output_tokens: average(|r| r.lead_output_tokens),
        mean_lead_preflight_input_tokens: average(|r| r.lead_preflight_input_tokens),
        mean_micro_input_tokens: average(|r| r.micro_input_tokens),
        mean_micro_output_tokens: average(|r| r.micro_output_tokens),
        mean_context_tokens: runs
            .iter()
            .map(|r| r.average_context_tokens as u64)
            .sum::<u64>() as f64
            / count as f64,
        p50_runtime_ms: percentile(&durations, 0.50),
        p95_runtime_ms: percentile(&durations, 0.95),
        runs,
    };
    std::fs::write(
        result_dir.join("summary.json"),
        serde_json::to_vec_pretty(&batch).map_err(|e| Error::Validation(e.to_string()))?,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    let md=format!("# Research batch\n\n- Requested: {}\n- Completed: {}\n- Invariant passes: {}\n- Resume successes: {}\n- Model: {}\n- Mean model calls: {:.2}\n- Mean lead input/output tokens: {:.1} / {:.1}\n- Mean lead preflight input tokens: {:.1}\n- Mean micro input/output tokens: {:.1} / {:.1}\n- Mean context tokens (Core estimate): {:.1}\n- Runtime p50/p95: {} / {} ms\n",batch.requested_runs,batch.completed_runs,batch.invariant_passes,batch.resume_successes,config.model_name,batch.mean_model_calls,batch.mean_lead_input_tokens,batch.mean_lead_output_tokens,batch.mean_lead_preflight_input_tokens,batch.mean_micro_input_tokens,batch.mean_micro_output_tokens,batch.mean_context_tokens,batch.p50_runtime_ms,batch.p95_runtime_ms);
    std::fs::write(result_dir.join("summary.md"), md).map_err(|e| Error::Storage(e.to_string()))?;
    Ok(batch)
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    if sorted.is_empty() {
        0
    } else {
        sorted[((sorted.len() - 1) as f64 * fraction).ceil() as usize]
    }
}
fn verify_restart(db_path: &Path, report: &mut RunReport) -> Result<()> {
    report.restarts = 1;
    let core = Core::new(Arc::new(SqliteStorage::open(db_path)?));
    let before = core.runtime_state(report.project_id)?;
    let completed_before = before
        .work_nodes
        .values()
        .filter(|w| w.status == WorkStatus::Succeeded)
        .count();
    let claims_before = before.claims.len();
    let checkpoint = before
        .checkpoints
        .values()
        .next()
        .ok_or_else(|| Error::Validation("restart fixture has no checkpoint".into()))?
        .id;
    let descriptor = core.resume_checkpoint(report.project_id, checkpoint)?;
    report.resumes = 1;
    let after = core.runtime_state(report.project_id)?;
    let completed_after = after
        .work_nodes
        .values()
        .filter(|w| w.status == WorkStatus::Succeeded)
        .count();
    report.completed_work_preserved_after_resume = descriptor.checkpoint.id == checkpoint
        && completed_before == completed_after
        && claims_before == after.claims.len();
    if !report.completed_work_preserved_after_resume {
        report.audit.violation(
            "resume_preserves_committed_work",
            Some(checkpoint.to_string()),
            "completed work or committed claims changed after restart",
        );
        report.audit.passed = false;
    }
    report.audit.checked += 1;
    Ok(())
}

fn run_one(
    core: &Core,
    kind: ScenarioKind,
    seed: u64,
    variation: u64,
    provider: Arc<dyn ModelProvider>,
    config: &HarnessConfig,
) -> RunReport {
    let start = Instant::now();
    let name = format!("eval-{}-{seed}", kind.name());
    let mut out = RunReport {
        scenario: kind.name().into(),
        seed,
        project_id: ProjectId::new(),
        status: "failed".into(),
        elapsed_ms: 0,
        model_invocations: 0,
        lead_model_invocations: 0,
        micro_model_invocations: 0,
        tool_invocations: 0,
        claim_count: 0,
        relation_count: 0,
        duplicate_actions_prevented: 0,
        lead_input_tokens: 0,
        lead_output_tokens: 0,
        lead_preflight_input_tokens: 0,
        micro_input_tokens: 0,
        micro_output_tokens: 0,
        max_context_tokens: 0,
        average_context_tokens: 0,
        retries: 0,
        malformed_model_outputs: 0,
        schema_failures: 0,
        loops_detected: 0,
        restarts: 0,
        resumes: 0,
        completed_work_preserved_after_resume: false,
        hypothesis_count: 0,
        experiment_count: 0,
        experiment_run_count: 0,
        failed_experiment_count: 0,
        finding_count: 0,
        gate_count: 0,
        branch_count: 0,
        checkpoint_count: 0,
        audit: AuditReport::default(),
        error: None,
        model: config.model_name.clone(),
        profile: "local-test-lead".into(),
    };
    let (question, docs, expected) = match scenario_text(kind, variation) {
        Ok(v) => v,
        Err(e) => {
            out.error = Some(e.to_string());
            return out;
        }
    };
    let result = (|| -> Result<()> {
        let project = core.projects().create(
            name,
            "Offline synthetic research pack".into(),
            ctx("create research harness project"),
        )?;
        out.project_id = project.id;
        let q = core.questions().add(
            project.id,
            question,
            format!("Synthetic research scenario: {}", kind.name()),
            ctx("create test question"),
        )?;
        let mut created_claims = Vec::new();
        let mut claim_texts = BTreeMap::new();
        let mut created_sources = Vec::new();
        let mut seen_claim_text = BTreeSet::new();
        let mut duplicate_actions_prevented = 0usize;
        for (n, doc) in docs.iter().enumerate() {
            let source = core.sources().add(
                project.id,
                NewSource {
                    title: format!("Synthetic source {}", n + 1),
                    source_kind: SourceKind::Manual,
                    locator: format!("pack://{}/{n}", kind.name()),
                    checksum: Some(format!("seed-{seed}-{n}")),
                    metadata: json!({"schema_version":1,"ground_truth":expected,"model_route":"local-test-lead"}),
                },
                ctx("add offline source"),
            )?;
            let paper = core.papers().add(
                project.id,
                NewPaper {
                    title: format!("Synthetic study {}", n + 1),
                    authors: vec!["ANIX Evaluation".into()],
                    publication_year: Some(2026),
                    doi: None,
                    abstract_text: doc.clone(),
                    source_ids: vec![source.id],
                    metadata: json!({"schema_version":1}),
                },
                ctx("add offline paper"),
            )?;
            if !seen_claim_text.insert(
                doc.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase(),
            ) {
                duplicate_actions_prevented += 1;
                created_sources.push(source.id);
                continue;
            }
            let claim = core.claims().add(
                project.id,
                NewClaim {
                    paper_id: paper.id,
                    source_id: Some(source.id),
                    text: doc.clone(),
                    location: ClaimLocation {
                        section: Some("results".into()),
                        ..Default::default()
                    },
                },
                ctx("extract grounded synthetic claim"),
            )?;
            created_claims.push(claim.id);
            claim_texts.insert(claim.id, doc.clone());
            created_sources.push(source.id);
        }
        let hypothesis = core.hypotheses().create(
            project.id,
            NewHypothesis {
                question_id: Some(q.id),
                title: format!("Initial hypothesis for {}", kind.name()),
                statement: format!("Synthetic evidence about X and Y in {}", kind.name()),
            },
            ctx("record initial research hypothesis"),
        )?;
        out.hypothesis_count = 1;
        let focus = match hypothesis {
            ActionResult::Entity(EntityRef::Hypothesis(id)) => EntityRef::Hypothesis(id),
            _ => {
                return Err(Error::Validation(
                    "hypothesis action result did not reference a hypothesis".into(),
                ))
            }
        };
        if matches!(
            kind,
            ScenarioKind::ExperimentProposal | ScenarioKind::FailedExperiment
        ) {
            let experiment = propose_via_gate(
                core,
                project.id,
                ProposedAction::CreateExperiment(NewExperiment {
                    hypothesis_id: match focus {
                        EntityRef::Hypothesis(id) => id,
                        _ => return Err(Error::Validation("experiment needs a hypothesis".into())),
                    },
                    expected_version: 1,
                    title: format!("Synthetic test experiment: {}", kind.name()),
                    objective: "Measure outcome Y under intervention X".into(),
                    methodology: "Compare an intervention arm with a synthetic control".into(),
                    protocol: "Use deterministic synthetic measurements; no external effects"
                        .into(),
                }),
                "test-human reviews safe synthetic experiment proposal",
            )?;
            let experiment_id = match experiment {
                ActionResult::Entity(EntityRef::Experiment(id)) => id,
                _ => {
                    return Err(Error::Validation(
                        "experiment proposal gate returned an unexpected entity".into(),
                    ))
                }
            };
            if kind == ScenarioKind::FailedExperiment {
                let activated = propose_via_gate(
                    core,
                    project.id,
                    ProposedAction::ActivateExperiment {
                        experiment_id,
                        expected_version: 1,
                    },
                    "test-human reviews safe synthetic activation",
                )?;
                if activated != ActionResult::Entity(EntityRef::Experiment(experiment_id)) {
                    return Err(Error::Validation(
                        "experiment activation did not resolve".into(),
                    ));
                }
                let attempt = core.experiments().start_run(
                    project.id,
                    experiment_id,
                    2,
                    vec![],
                    ctx("start synthetic failed experiment"),
                )?;
                let _=core.experiments().finish_run(project.id,anix_core::FinishExperimentRun{id:attempt.id,expected_version:1,status:RunStatus::Failed,outputs:vec![],metadata:json!({"schema_version":1,"failure":"synthetic instrument failure"})},ctx("persist synthetic experiment failure"))?;
            }
        }
        for claim_id in &created_claims {
            let text = claim_texts
                .get(claim_id)
                .map(String::as_str)
                .unwrap_or_default()
                .to_lowercase();
            let relation_kind = match kind {
                ScenarioKind::ContradictoryEvidence if text.contains("worsened") => {
                    RelationKind::Qualifies
                }
                ScenarioKind::ContradictoryEvidence if text.contains("no measurable") => {
                    RelationKind::Contradicts
                }
                ScenarioKind::Revision if text.contains("reduced") => RelationKind::Contradicts,
                _ => RelationKind::Supports,
            };
            let _ = core.graph().create(
                project.id,
                NewRelation {
                    from_entity: EntityRef::Claim(*claim_id),
                    to_entity: focus,
                    kind: relation_kind,
                    confidence: Some(0.8),
                },
                ctx("link claim to hypothesis with explicit evidence relation"),
            )?;
        }
        let branch = core.create_branch(
            project.id,
            "main".into(),
            None,
            ctx("create research branch"),
        )?;
        let work = core.create_work(
            project.id,
            NewWork {
                branch_id: branch.id,
                kind: "evidence-review".into(),
                granularity: WorkGranularity::Atomic,
                title: format!("Review {} evidence", kind.name()),
                objective: format!("Evaluate {} evidence with traceable claims", kind.name()),
                acceptance_criteria: OutputContract {
                    required_entities: vec![EntityRef::Question(q.id)],
                    required_artifacts: vec![],
                    minimum_children: 0,
                },
                capability_requirements: vec!["structured_output".into()],
                context_policy_id: "research-harness-v1".into(),
                budget: WorkBudget {
                    context_tokens: config.budget.max_context_tokens,
                    max_model_calls: config.budget.max_model_invocations,
                    max_tool_calls: config.budget.max_tool_invocations,
                },
            },
            ctx("create atomic evidence work"),
        )?;
        let task = core.create_task(
            project.id,
            format!("Research {}", kind.name()),
            "Persisted harness work execution".into(),
            ctx("create work task"),
        )?;
        let run = core.start_run(project.id, task.id, None, ctx("start work execution"))?;
        let binding = core.bind_run(project.id, run.id, work.id, ctx("bind task to work"))?;
        let snapshot = core.compile_context(
            project.id,
            work.id,
            binding.plan_revision_id,
            vec![EntityRef::Question(q.id), focus],
            ctx("compile bounded context"),
        )?;
        let checkpoint = ExecutionCheckpoint {
            id: CheckpointId::new(),
            meta: Meta::new(project.id),
            run_id: Some(run.id),
            work_node_id: Some(work.id),
            branch_id: binding.branch_id,
            plan_revision_id: binding.plan_revision_id,
            context_snapshot_id: Some(snapshot.id),
            boundary: CheckpointBoundary::Suspended,
            execution_state: json!({"step":0,"seed":seed}),
        };
        let _checkpoint = core.checkpoint(
            project.id,
            checkpoint,
            ctx("persist harness restart checkpoint"),
        )?;
        if kind == ScenarioKind::Branching {
            let _ = core.create_branch(
                project.id,
                "mechanism-path-p".into(),
                Some(_checkpoint.id),
                ctx("fork mechanism P from shared checkpoint"),
            )?;
            let _ = core.create_branch(
                project.id,
                "mechanism-path-q".into(),
                Some(_checkpoint.id),
                ctx("fork mechanism Q from shared checkpoint"),
            )?;
        }
        let _ = core.finish_run(
            project.id,
            run.id,
            RunStatus::Completed,
            ctx("complete work execution"),
        )?;
        let current = core.runtime_state(project.id)?.work_nodes[&work.id]
            .meta
            .version;
        let _ = core.transition_work(
            project.id,
            work.id,
            current,
            WorkStatus::Succeeded,
            ctx("complete deterministic evidence setup"),
        )?;
        out.checkpoint_count = 1;
        out.branch_count = 1;
        let selector = Arc::new(FixedModelSelector {
            selection: ModelSelection {
                provider: "local-test-lead".into(),
                model: config.model_name.clone(),
            },
        });
        let model_calls = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let measured_provider: Arc<dyn ModelProvider> = Arc::new(MeteredProvider {
            inner: provider,
            calls: model_calls.clone(),
        });
        let agent = LeadAgent::new(
            core.agent_api("anix-evals-test-lead")?,
            measured_provider,
            selector,
        );
        let goal=format!("Analyze the evidence in this synthetic offline research pack for scenario {kind:?}. Preserve conflicts and uncertainty. Do not claim evidence that is absent. Consider the research question and existing claims. If a safe, valid hypothesis revision is justified, propose it; otherwise return no_change.");
        let outcome = if start.elapsed().as_secs() > config.budget.max_wall_time_seconds {
            Err(Error::Validation(
                "research wall-clock budget exceeded".into(),
            ))
        } else {
            futures::executor::block_on(agent.run(project.id, goal, Some(focus), None))
        };
        match outcome {
            Ok(o) => {
                out.status = "completed".into();
                if let Some(Submission::Gate(id)) = o.submission {
                    out.gate_count = 1;
                    let g = core.gates().get(project.id, id)?;
                    if g.status == GateStatus::Pending {
                        let _ = test_human_resolve_gate(core, project.id, id)?;
                    }
                }
            }
            Err(e) => {
                let error = e.to_string();
                if let Some(tokens) = preflight_tokens_from_error(&error) {
                    out.lead_preflight_input_tokens = tokens;
                }
                if is_malformed_model_response(&error) {
                    out.malformed_model_outputs += 1;
                }
                if is_model_schema_failure(&error) {
                    out.schema_failures += 1;
                }
                if error.contains("LoopDetected") {
                    out.loops_detected += 1;
                }
                out.error = Some(error);
            }
        }
        let state = core.runtime_state(project.id)?;
        let research = core.get_research_state(project.id)?;
        out.claim_count = research.claims.len();
        out.relation_count = state.relations.len();
        out.duplicate_actions_prevented = duplicate_actions_prevented;
        out.hypothesis_count = research.hypotheses.len();
        out.experiment_count = research.experiments.len();
        out.experiment_run_count = state.experiment_runs.len();
        out.failed_experiment_count = state
            .experiment_runs
            .values()
            .filter(|r| r.status == RunStatus::Failed)
            .count();
        out.finding_count = research.findings.len();
        if kind == ScenarioKind::FailedExperiment
            && (out.failed_experiment_count == 0 || out.finding_count != 0)
        {
            return Err(Error::Validation(
                "failed experiment was lost or an unsupported finding was created".into(),
            ));
        }
        out.gate_count = state.gates.len();
        out.branch_count = state.branches.len();
        out.checkpoint_count = state.checkpoints.len();
        out.lead_model_invocations = model_calls.load(std::sync::atomic::Ordering::Relaxed);
        out.micro_model_invocations = state.model_invocations.len() as u64;
        out.model_invocations = out.lead_model_invocations + out.micro_model_invocations;
        let normalized_provider_metadata = state
            .model_invocations
            .values()
            .filter(|invocation| invocation.metadata["metadata_defaulted"] == true)
            .count() as u32;
        out.malformed_model_outputs += normalized_provider_metadata;
        out.schema_failures += normalized_provider_metadata;
        out.lead_input_tokens = state
            .runs
            .values()
            .map(|r| {
                r.provider_metadata["response"]["input_tokens"]
                    .as_u64()
                    .unwrap_or(0)
            })
            .sum();
        out.lead_preflight_input_tokens = out.lead_preflight_input_tokens.max(
            state
                .runs
                .values()
                .map(|run| {
                    run.provider_metadata["response"]["preflight_input_tokens"]
                        .as_u64()
                        .unwrap_or_default()
                })
                .sum(),
        );
        out.lead_output_tokens = state
            .runs
            .values()
            .map(|r| {
                r.provider_metadata["response"]["output_tokens"]
                    .as_u64()
                    .unwrap_or(0)
            })
            .sum();
        out.micro_input_tokens = state
            .model_invocations
            .values()
            .map(|v| v.usage.input)
            .sum();
        out.micro_output_tokens = state
            .model_invocations
            .values()
            .map(|v| v.usage.output)
            .sum();
        let contexts: Vec<_> = state
            .context_snapshots
            .values()
            .map(|s| s.estimated_tokens)
            .collect();
        out.max_context_tokens = contexts.iter().copied().max().unwrap_or(0);
        out.average_context_tokens = if contexts.is_empty() {
            0
        } else {
            contexts.iter().map(|v| *v as u64).sum::<u64>() as u32 / contexts.len() as u32
        };
        out.tool_invocations = state.tool_invocations.len() as u64;
        // Every invocation from this run is linked to a persisted run; the LeadAgent creates the task/run through AgentApi.
        for c in &created_claims {
            if !state
                .provenance
                .iter()
                .any(|p| p.entity == Some(EntityRef::Claim(*c)))
            {
                return Err(Error::Validation(format!(
                    "missing provenance for claim {}",
                    c
                )));
            }
        }
        if created_sources.len() != docs.len() {
            return Err(Error::Validation(
                "offline source cardinality mismatch".into(),
            ));
        }
        out.audit = audit_project(&state);
        Ok(())
    })();
    if let Err(e) = result {
        if out.error.is_none() {
            out.error = Some(e.to_string());
        }
    }
    if out.status != "completed" && out.error.is_none() {
        out.error = Some("research run incomplete".into());
    }
    out.elapsed_ms = start.elapsed().as_millis() as u64;
    out
}

fn is_malformed_model_response(error: &str) -> bool {
    error.contains("invalid Test Lead response")
        || error.contains("Test Lead returned no message content")
        || error.contains("Test Lead response exceeded")
        || error.contains("malformed structured output")
}

fn is_model_schema_failure(error: &str) -> bool {
    error.contains("invalid Test Lead response")
        || error.contains("schema version mismatch")
        || error.contains("metadata must be a JSON object")
        || error.contains("malformed structured output")
}

fn preflight_tokens_from_error(error: &str) -> Option<u64> {
    let marker = "preflight_input_tokens=";
    let start = error.find(marker)? + marker.len();
    let digits: String = error[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

pub fn inspect_run(workspace: &Path, project_id: ProjectId) -> Result<Value> {
    let ws = Workspace::open(workspace)?;
    let core = Core::new(Arc::new(SqliteStorage::open(ws.database_path()?)?));
    let state = core.runtime_state(project_id)?;
    let research = core.get_research_state(project_id)?;
    Ok(
        json!({"project":research.project,"research":research,"process_graph":{"branches":state.branches,"work_nodes":state.work_nodes,"work_edges":state.work_edges,"plan_revisions":state.plan_revisions,"checkpoints":state.checkpoints,"snapshots":state.context_snapshots},"execution_graph":{"bindings":state.execution_bindings,"model_invocations":state.model_invocations,"tool_invocations":state.tool_invocations,"runs":state.runs},"provenance":state.provenance,"events":state.events,"audit":audit_project(&state)}),
    )
}

pub fn export_debug_bundle(
    workspace: &Path,
    project_id: ProjectId,
    destination: &Path,
) -> Result<()> {
    let value = inspect_run(workspace, project_id)?;
    std::fs::create_dir_all(destination).map_err(|e| Error::Storage(e.to_string()))?;
    let scrubbed = scrub_secrets(value);
    std::fs::write(
        destination.join("diagnostic.json"),
        serde_json::to_vec_pretty(&scrubbed).map_err(|e| Error::Validation(e.to_string()))?,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    Ok(())
}
fn scrub_secrets(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .filter(|(k, _)| {
                    let l: String = k
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .collect();
                    !(l.contains("apikey")
                        || l.contains("authorization")
                        || l.contains("secret")
                        || l.contains("password")
                        || l == "token"
                        || l.contains("accesstoken")
                        || l.contains("refreshtoken")
                        || l.contains("bearertoken"))
                })
                .map(|(k, v)| (k, scrub_secrets(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(scrub_secrets).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anix_agent::{AgentDecision, FakeModelProvider, ModelResponse};
    fn fake(n: usize) -> Arc<dyn ModelProvider> {
        Arc::new(FakeModelProvider::new(
            (0..n)
                .map(|_| ModelResponse {
                    schema_version: 1,
                    decision: AgentDecision::NoChange {
                        summary: "Fixture response".into(),
                    },
                    metadata: json!({}),
                })
                .collect(),
        ))
    }
    #[test]
    fn fault_plan_is_repeatable() {
        let p = FaultPlan {
            seed: 17,
            after_model_call: Some(3),
            after_checkpoint: None,
            fault: FaultKind::ModelTimeout,
        };
        assert_eq!(p.triggered(3, None), p.triggered(3, None));
        assert!(!p.triggered(2, None));
    }
    #[test]
    fn scenario_catalog_has_eleven_distinct_entries() {
        let names: BTreeSet<_> = ScenarioKind::ALL.iter().map(|k| k.name()).collect();
        assert_eq!(names.len(), 11);
        let packs: PackCatalog =
            serde_json::from_str(include_str!("../../../tests/research-packs/catalog.json"))
                .unwrap();
        assert_eq!(
            packs
                .scenarios
                .iter()
                .map(|p| p.id.as_str())
                .collect::<BTreeSet<_>>(),
            names
        );
    }
    #[test]
    fn scenario_variations_are_seeded_and_preserve_ground_truth() {
        let a = scenario_text(ScenarioKind::ContradictoryEvidence, 3).unwrap();
        let b = scenario_text(ScenarioKind::ContradictoryEvidence, 3).unwrap();
        let c = scenario_text(ScenarioKind::ContradictoryEvidence, 4).unwrap();
        assert_eq!(a, b);
        assert_ne!(a.0, c.0);
        assert_eq!(a.2, c.2);
    }
    #[test]
    fn long_context_variants_leave_a_bounded_local_request_window() {
        let small = scenario_text(ScenarioKind::LongContext, 700).unwrap();
        let large = scenario_text(ScenarioKind::LongContext, 704).unwrap();
        assert_eq!(small.1.len(), 14);
        assert_eq!(large.1.len(), 18);
        assert!(small.1.len() < large.1.len());
        assert!(small
            .1
            .last()
            .is_some_and(|document| document.len() > 8_192));
    }
    #[test]
    fn test_lead_parse_errors_count_as_malformed_schema_failures() {
        let error = "model: invalid Test Lead response: invalid type: string \\\"revise_hypothesis\\\", expected adjacently tagged enum ProposedAction";
        assert!(is_malformed_model_response(error));
        assert!(is_model_schema_failure(error));
        assert!(!is_malformed_model_response(
            "validation: Test Lead context budget exceeded"
        ));
        assert!(!is_model_schema_failure("model: request transport timeout"));
    }
    #[test]
    fn local_only_refuses_external_model_provider() {
        let cfg = HarnessConfig::local_default(".");
        assert!(matches!(
            run_batch_with_provider(&cfg, 10, 1, Arc::new(ExternalProvider)),
            Err(Error::PermissionDenied)
        ));
    }
    struct ExternalProvider;
    impl ModelProvider for ExternalProvider {
        fn generate(&self, _: anix_agent::ModelRequest) -> anix_agent::ModelFuture<'_> {
            Box::pin(async { Err(Error::Model("should not call".into())) })
        }
    }
    #[test]
    fn report_serializes_and_secret_bundle_scrubber_strips_tokens() {
        let v = scrub_secrets(json!({"api_key":"secret","context_tokens":55,"ok":true}));
        assert!(v.get("api_key").is_none());
        assert_eq!(v["context_tokens"], 55);
    }
    #[test]
    fn auditor_detects_missing_provenance() {
        let mut s = ProjectState::default();
        let project = ProjectId::new();
        let source = Source {
            id: SourceId::new(),
            meta: Meta::new(project),
            title: "src".into(),
            source_kind: SourceKind::Manual,
            locator: "pack://src".into(),
            checksum: None,
            metadata: json!({"schema_version":1}),
        };
        let paper = Paper {
            id: PaperId::new(),
            meta: Meta::new(project),
            title: "p".into(),
            authors: vec![],
            publication_year: None,
            doi: None,
            abstract_text: String::new(),
            metadata: json!({"schema_version":1}),
        };
        let claim = Claim {
            id: ClaimId::new(),
            meta: Meta::new(project),
            paper_id: paper.id,
            source_id: Some(source.id),
            normalized_text: None,
            text: "x".into(),
            location: ClaimLocation::default(),
            created_by: Actor::System,
        };
        s.sources.insert(source.id, source);
        s.papers.insert(paper.id, paper);
        s.claims.insert(claim.id, claim);
        assert!(audit_project(&s)
            .violations
            .iter()
            .any(|v| v.invariant == "claim_has_provenance"));
    }
    #[test]
    fn loop_guard_and_budget_stop_repeated_or_unbounded_actions() {
        let mut g = ActionLoopGuard::new(3).unwrap();
        let action = json!({"type":"same","target":"x"});
        assert!(g.observe(&action).is_ok());
        assert!(g.observe(&action).is_ok());
        assert!(g.observe(&action).is_err());
        let b = ResearchBudget {
            max_steps: 1,
            max_model_invocations: 1,
            max_tool_invocations: 1,
            max_context_tokens: 4,
            max_wall_time_seconds: 99,
        };
        let mut usage = BudgetUsage::new();
        usage.context_tokens = 5;
        assert!(usage.check(&b).is_err());
    }
    #[test]
    fn malformed_model_fault_fails_safely_and_preserves_research_state() {
        let temp = tempfile::tempdir().unwrap();
        let cfg = HarnessConfig::local_default(temp.path());
        let plan = FaultPlan {
            seed: 99,
            after_model_call: Some(1),
            after_checkpoint: None,
            fault: FaultKind::MalformedStructuredOutput,
        };
        let injected: Arc<dyn ModelProvider> =
            Arc::new(InjectingModelProvider::new(fake(10), plan));
        let report = run_batch_with_provider(&cfg, 10, 99, injected).unwrap();
        assert_eq!(report.runs[0].status, "failed");
        assert!(report.runs[0]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("malformed structured"));
        assert!(report.runs[0].claim_count > 0 && report.runs[0].audit.passed);
        assert_eq!(report.completed_runs, 9);
    }
    #[test]
    fn harness_persists_ten_real_core_projects_and_audits_them() {
        let temp = tempfile::tempdir().unwrap();
        let cfg = HarnessConfig::local_default(temp.path());
        let report = run_batch_with_provider(&cfg, 10, 42, fake(10)).unwrap();
        assert_eq!(report.requested_runs, 10);
        assert_eq!(
            report.completed_runs,
            10,
            "{:?}",
            report
                .runs
                .iter()
                .map(|r| (&r.scenario, &r.error))
                .collect::<Vec<_>>()
        );
        assert_eq!(report.invariant_passes, 10);
        assert_eq!(report.resume_successes, 10);
        assert!(
            report.runs.iter().all(|r| r.claim_count > 0
                && r.hypothesis_count > 0
                && r.branch_count >= 1
                && r.checkpoint_count >= 1
                && r.completed_work_preserved_after_resume),
            "{:?}",
            report
                .runs
                .iter()
                .map(|r| (
                    &r.scenario,
                    r.claim_count,
                    r.hypothesis_count,
                    r.branch_count,
                    r.checkpoint_count,
                    &r.error
                ))
                .collect::<Vec<_>>()
        );
        let duplicates = &report.runs[2];
        assert_eq!(duplicates.claim_count, 1);
        assert_eq!(duplicates.duplicate_actions_prevented, 2);
        assert!(report.runs[1].relation_count >= 3);
        assert!(report.runs[3].branch_count >= 3);
        let bundle = temp.path().join("bundle");
        export_debug_bundle(temp.path(), report.runs[0].project_id, &bundle).unwrap();
        let diagnostic: Value =
            serde_json::from_slice(&std::fs::read(bundle.join("diagnostic.json")).unwrap())
                .unwrap();
        assert!(
            diagnostic.get("events").is_some()
                && diagnostic.get("process_graph").is_some()
                && diagnostic.get("provenance").is_some()
                && diagnostic.get("audit").is_some()
        );
    }
}

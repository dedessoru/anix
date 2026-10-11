//! Real Test Lead evaluations for structural branching and bounded long-context research.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::{audit_project, AuditReport, HarnessConfig};
use anix_agent::{
    AgentDecision, AgentStepOutcome, ApprovedTools, FixedModelSelector, LeadAgent,
    LocalOnlyTestProvider, ModelProvider, ModelSelection, ProcessAction, ProcessActionKind,
};
use anix_core::{CommandContext, Core, NewPaper, NewSource, NewWork};
use anix_domain::*;
use anix_integrations::{
    ArtifactOffloader, OffloadedOutput, OpenAiCompatibleModelProvider, RecordedTool,
    TestLeadConfig, Tool, ToolContext, ToolDescriptor, ToolFuture, ToolInput, ToolOutput,
    Workspace,
};
use anix_storage::SqliteStorage;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

const LEAD_IDENTITY: &str = "true-capability-test-lead";
const OFFLOAD_THRESHOLD: usize = 8_192;
const MAX_SEARCH_CANDIDATES: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetrievalRecord {
    pub query: String,
    pub branch_id: Option<BranchId>,
    pub candidate_count: usize,
    pub selected: Vec<RetrievedDocument>,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetrievedDocument {
    pub paper_id: PaperId,
    pub source_id: SourceId,
    pub title: String,
    pub role: String,
    pub facet: String,
    pub preview: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceReadRecord {
    pub branch_id: Option<BranchId>,
    pub source_id: SourceId,
    pub documents: Vec<String>,
    pub bytes: usize,
    pub offloaded: bool,
    pub artifact_id: Option<ArtifactId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RangeReadRecord {
    pub branch_id: Option<BranchId>,
    pub artifact_id: ArtifactId,
    pub start: u64,
    pub length: u64,
    pub succeeded: bool,
    pub injected_failure: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotRecord {
    pub id: ContextSnapshotId,
    pub branch_id: BranchId,
    pub work_node_id: WorkNodeId,
    pub query: String,
    pub candidate_count: u32,
    pub selected: Vec<(EntityRef, u32)>,
    pub estimated_prompt_tokens: u32,
    pub process_memory_ids: Vec<ProcessMemoryId>,
    pub process_memory_branch_ids: Vec<BranchId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BranchResult {
    pub id: BranchId,
    pub name: String,
    pub parent_branch_id: Option<BranchId>,
    pub base_checkpoint_id: Option<CheckpointId>,
    pub work_node_ids: Vec<WorkNodeId>,
    pub completed_work_nodes: usize,
    pub result_summary: String,
    pub process_memory_ids: Vec<ProcessMemoryId>,
    pub artifact_ids: Vec<ArtifactId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedBranchRecord {
    pub id: BranchId,
    pub name: String,
    pub parent_branch_id: Option<BranchId>,
    pub base_checkpoint_id: Option<CheckpointId>,
    pub work_node_ids: Vec<WorkNodeId>,
    pub completed_work_nodes: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapabilityRunReport {
    pub capability: String,
    pub seed: u64,
    pub workflow_status: String,
    pub capability_status: String,
    pub error: Option<String>,
    pub project_id: Option<ProjectId>,
    pub model: String,
    pub context_capacity: u32,
    pub corpus_bytes: u64,
    pub corpus_estimated_tokens: u64,
    pub branch_results: Vec<BranchResult>,
    pub persisted_branches: Vec<PersistedBranchRecord>,
    pub branch_results_reconciled: bool,
    pub restart_count: u32,
    pub restart_preserved_completed_work: bool,
    pub completed_work_ids_before_restart: Vec<WorkNodeId>,
    pub work_bindings_before_restart: usize,
    pub retrievals: Vec<RetrievalRecord>,
    pub source_reads: Vec<SourceReadRecord>,
    pub range_reads: Vec<RangeReadRecord>,
    pub snapshots: Vec<SnapshotRecord>,
    pub prompt_tokens: Vec<u64>,
    pub process_memory_count: usize,
    pub integrity_audit: AuditReport,
    pub recovery_audit_passed: bool,
    pub duplicate_consequential_operations: usize,
    pub unfinished_runs: usize,
    pub lead_calls: usize,
    pub tool_calls: usize,
    pub retrieval_recall_at_selected: Option<f64>,
    pub required_evidence_recovered: Vec<String>,
    pub total_execution_history_in_prompts: bool,
    pub preflight_failures: Vec<String>,
    pub elapsed_ms: u64,
    pub report_path: String,
}

#[derive(Clone)]
struct CorpusDocument {
    key: String,
    role: String,
    facet: String,
    title: String,
    text: String,
    source_id: Option<SourceId>,
}

#[derive(Default)]
struct Telemetry {
    retrievals: Vec<RetrievalRecord>,
    source_reads: Vec<SourceReadRecord>,
    range_reads: Vec<RangeReadRecord>,
    preflight_failures: Vec<String>,
}

struct SearchSourcesTool {
    core: Arc<Core>,
    telemetry: Arc<Mutex<Telemetry>>,
}
impl Tool for SearchSourcesTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "search_sources".into(),
            description: "Search the supplied ANIX corpus with SQLite FTS5/BM25 and return bounded document references".into(),
            read_only: true,
        }
    }
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let query = input.parameters["query"]
                .as_str()
                .ok_or_else(|| Error::Validation("query is required".into()))?
                .to_owned();
            let hits =
                self.core
                    .search_entities(context.project_id, &query, MAX_SEARCH_CANDIDATES)?;
            let state = self.core.runtime_state(context.project_id)?;
            let mut selected = vec![];
            for hit in &hits {
                let EntityRef::Paper(paper_id) = hit else {
                    continue;
                };
                let Some(paper) = state.papers.get(paper_id) else {
                    continue;
                };
                let Some(link) = state
                    .paper_sources
                    .iter()
                    .find(|link| link.paper_id == *paper_id)
                else {
                    continue;
                };
                selected.push(RetrievedDocument {
                    paper_id: *paper_id,
                    source_id: link.source_id,
                    title: paper.title.clone(),
                    role: paper.metadata["role"]
                        .as_str()
                        .unwrap_or("unclassified")
                        .into(),
                    facet: paper.metadata["facet"]
                        .as_str()
                        .unwrap_or("unclassified")
                        .into(),
                    preview: paper.abstract_text.chars().take(180).collect(),
                });
                if selected.len() == 8 {
                    break;
                }
            }
            let record = RetrievalRecord {
                query: query.clone(),
                branch_id: context.run_id.and_then(|run| {
                    state
                        .execution_bindings
                        .get(&run)
                        .map(|binding| binding.branch_id)
                }),
                candidate_count: hits.len(),
                selected: selected.clone(),
                source: "tool.sqlite-fts5-bm25".into(),
            };
            self.telemetry
                .lock()
                .map_err(|_| Error::Tool("retrieval telemetry lock".into()))?
                .retrievals
                .push(record);
            Ok(ToolOutput {
                schema_version: 1,
                data: json!({"query":query,"candidate_count":hits.len(),"selected":selected}),
                metadata: json!({"retrieval":"sqlite-fts5-bm25","candidate_count":hits.len(),"selected_count":selected.len()}),
            })
        })
    }
}

struct ReadCorpusSourceTool {
    core: Arc<Core>,
    telemetry: Arc<Mutex<Telemetry>>,
}
impl Tool for ReadCorpusSourceTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "read_source".into(),
            description: "Read the full text of one retrieved source".into(),
            read_only: true,
        }
    }
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let raw = input.parameters["source_id"]
                .as_str()
                .ok_or_else(|| Error::Validation("source_id is required".into()))?;
            let source_id: SourceId = raw.parse()?;
            let state = self.core.runtime_state(context.project_id)?;
            let branch_id = context.run_id.and_then(|run| {
                state
                    .execution_bindings
                    .get(&run)
                    .map(|binding| binding.branch_id)
            });
            let documents: Vec<_> = state
                .paper_sources
                .iter()
                .filter(|link| link.source_id == source_id)
                .filter_map(|link| state.papers.get(&link.paper_id))
                .collect();
            if documents.is_empty() {
                return Err(Error::NotFound("paper for source".into()));
            }
            let output_documents: Vec<_> = documents
                .iter()
                .map(|paper| {
                    json!({"paper_id":paper.id,"title":paper.title,"role":paper.metadata["role"],"facet":paper.metadata["facet"],"abstract_text":paper.abstract_text})
                })
                .collect();
            let data = json!({"source_id":source_id,"documents":output_documents});
            let bytes = serde_json::to_vec(&data)
                .map_err(|e| Error::Tool(format!("serialize corpus read: {e}")))?
                .len();
            self.telemetry
                .lock()
                .map_err(|_| Error::Tool("read telemetry lock".into()))?
                .source_reads
                .push(SourceReadRecord {
                    branch_id,
                    source_id,
                    documents: documents.iter().map(|paper| paper.id.to_string()).collect(),
                    bytes,
                    offloaded: bytes > OFFLOAD_THRESHOLD,
                    artifact_id: None,
                });
            Ok(ToolOutput {
                schema_version: 1,
                data,
                metadata: json!({"source_id":source_id,"document_count":documents.len(),"bytes":bytes}),
            })
        })
    }
}

struct ReadArtifactRangeTool {
    core: Arc<Core>,
    offloader: ArtifactOffloader,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
}
impl Tool for ReadArtifactRangeTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "read_artifact_range".into(),
            description: "Read a bounded byte range from a previously returned artifact reference"
                .into(),
            read_only: true,
        }
    }
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let raw = input.parameters["artifact_id"]
                .as_str()
                .ok_or_else(|| Error::Validation("artifact_id is required".into()))?;
            let id: ArtifactId = raw.parse()?;
            let start = input.parameters["start"].as_u64().unwrap_or(0);
            let length = input.parameters["length"]
                .as_u64()
                .unwrap_or(1024)
                .min(4096);
            let state = self.core.runtime_state(context.project_id)?;
            let branch_id = context.run_id.and_then(|run| {
                state
                    .execution_bindings
                    .get(&run)
                    .map(|binding| binding.branch_id)
            });
            if self.fail_once.swap(false, Ordering::SeqCst) {
                self.telemetry
                    .lock()
                    .map_err(|_| Error::Tool("range telemetry lock".into()))?
                    .range_reads
                    .push(RangeReadRecord {
                        branch_id,
                        artifact_id: id,
                        start,
                        length,
                        succeeded: false,
                        injected_failure: true,
                    });
                return Err(Error::Tool("injected ArtifactReadFailure".into()));
            }
            let artifact = state
                .artifacts
                .get(&id)
                .ok_or_else(|| Error::NotFound("offloaded artifact".into()))?;
            let output = OffloadedOutput::Artifact {
                id,
                path: artifact.path.clone(),
                content_hash: artifact.content_hash.clone(),
                byte_size: artifact.byte_size,
                schema: artifact.metadata["schema"].clone(),
                summary: artifact.metadata["summary"]
                    .as_str()
                    .unwrap_or("offloaded tool output")
                    .into(),
            };
            let bytes = self.offloader.read_range(&output, start, length)?;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            self.telemetry
                .lock()
                .map_err(|_| Error::Tool("range telemetry lock".into()))?
                .range_reads
                .push(RangeReadRecord {
                    branch_id,
                    artifact_id: id,
                    start,
                    length: bytes.len() as u64,
                    succeeded: true,
                    injected_failure: false,
                });
            Ok(ToolOutput {
                schema_version: 1,
                data: json!({"artifact_id":id,"start":start,"length":bytes.len(),"text":text}),
                metadata: json!({"targeted_range":true}),
            })
        })
    }
}

fn tools(
    core: Arc<Core>,
    workspace: Arc<Workspace>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
) -> Result<ApprovedTools> {
    let mut approved = ApprovedTools::default();
    let inner: Vec<Arc<dyn Tool>> = vec![
        Arc::new(SearchSourcesTool {
            core: core.clone(),
            telemetry: telemetry.clone(),
        }),
        Arc::new(ReadCorpusSourceTool {
            core: core.clone(),
            telemetry: telemetry.clone(),
        }),
        Arc::new(ReadArtifactRangeTool {
            core: core.clone(),
            offloader: ArtifactOffloader {
                workspace: workspace.clone(),
                threshold_bytes: OFFLOAD_THRESHOLD,
            },
            telemetry: telemetry.clone(),
            fail_once,
        }),
    ];
    for tool in inner {
        approved.register(Arc::new(RecordedTool {
            core: core.clone(),
            inner: tool,
            offloader: ArtifactOffloader {
                workspace: workspace.clone(),
                threshold_bytes: OFFLOAD_THRESHOLD,
            },
            version: "true-capability-v1".into(),
        }))?;
    }
    Ok(approved)
}

fn lead_step(
    core: &Arc<Core>,
    project: ProjectId,
    provider: Arc<dyn ModelProvider>,
    workspace: Arc<Workspace>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    goal: String,
    focus: QuestionId,
    parent_run: RunId,
    work: WorkNodeId,
) -> Result<AgentStepOutcome> {
    lead_step_with_constraint(
        core, project, provider, workspace, telemetry, fail_once, goal, focus, parent_run, work,
        None,
    )
}

fn lead_step_with_constraint(
    core: &Arc<Core>,
    project: ProjectId,
    provider: Arc<dyn ModelProvider>,
    workspace: Arc<Workspace>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    goal: String,
    focus: QuestionId,
    parent_run: RunId,
    work: WorkNodeId,
    action_constraint: Option<ProcessActionKind>,
) -> Result<AgentStepOutcome> {
    let mut agent = LeadAgent::new(
        core.agent_api(LEAD_IDENTITY)?,
        provider,
        Arc::new(FixedModelSelector {
            selection: ModelSelection {
                provider: "local-test-lead".into(),
                model: "configured-local-test-lead".into(),
            },
        }),
    )
    .with_tools(tools(core.clone(), workspace, telemetry, fail_once)?);
    if let Some(action) = action_constraint {
        agent = agent.with_process_action_constraint(vec![action]);
    }
    futures::executor::block_on(agent.run_step(
        project,
        goal,
        Some(EntityRef::Question(focus)),
        Some(parent_run),
        work,
    ))
}

fn create_tool_work(
    core: &Arc<Core>,
    project: ProjectId,
    provider: Arc<dyn ModelProvider>,
    workspace: Arc<Workspace>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    question: QuestionId,
    parent_run: RunId,
    parent_work: WorkNodeId,
    branch_id: BranchId,
    title: &str,
    objective: &str,
    tool: &str,
    execution_goal: String,
) -> Result<(WorkNode, ToolOutput)> {
    let before: BTreeSet<_> = core
        .runtime_state(project)?
        .work_nodes
        .keys()
        .copied()
        .collect();
    let planned = lead_step_with_constraint(
        core,
        project,
        provider.clone(),
        workspace.clone(),
        telemetry.clone(),
        fail_once.clone(),
        format!("Create one atomic WorkNode titled '{title}' with objective '{objective}' and required tool '{tool}'. Do not make a branch or combine steps."),
        question,
        parent_run,
        parent_work,
        Some(ProcessActionKind::CreateWorkNode),
    )?;
    if !matches!(
        planned.decision,
        AgentDecision::Process {
            action: ProcessAction::CreateWorkNode { .. },
            ..
        }
    ) {
        return Err(Error::Validation(format!(
            "Lead did not create the requested bounded WorkNode; decision={:?}",
            planned.decision
        )));
    }
    let state = core.runtime_state(project)?;
    let child = state
        .work_nodes
        .values()
        .find(|node| node.branch_id == branch_id && !before.contains(&node.id))
        .cloned()
        .ok_or_else(|| Error::NotFound("Lead-created WorkNode".into()))?;
    if !child
        .capability_requirements
        .iter()
        .any(|requirement| requirement == &format!("tool:{tool}"))
    {
        return Err(Error::Validation(format!(
            "WorkNode did not require {tool}"
        )));
    }
    let called = match lead_step(
        core,
        project,
        provider.clone(),
        workspace.clone(),
        telemetry.clone(),
        fail_once.clone(),
        execution_goal.clone(),
        question,
        parent_run,
        child.id,
    ) {
        Ok(step) => step,
        Err(error)
            if tool == "read_artifact_range"
                && error.to_string().contains("injected ArtifactReadFailure") =>
        {
            lead_step(
                core,
                project,
                provider.clone(),
                workspace.clone(),
                telemetry.clone(),
                fail_once.clone(),
                format!("The previous bounded artifact read failed with an injected ArtifactReadFailure and was recorded. Retry the same specific range once; do not read the whole artifact. Original range request: {execution_goal}"),
                question,
                parent_run,
                child.id,
            )?
        }
        Err(error) => return Err(error),
    };
    let output = called
        .tool_output
        .ok_or_else(|| Error::Validation(format!("Lead did not execute required tool {tool}")))?;
    if tool == "read_source" {
        if let Some(artifact_id) = extract_offloaded_artifact(&output) {
            if let Ok(mut captured) = telemetry.lock() {
                if let Some(read) = captured.source_reads.last_mut() {
                    read.artifact_id = Some(artifact_id);
                }
            }
        }
    }
    let branch_root = state
        .work_nodes
        .values()
        .find(|node| node.branch_id == branch_id && node.granularity == WorkGranularity::Composite)
        .map(|node| node.id)
        .unwrap_or(parent_work);
    let observation = output.data.to_string();
    let excerpt: String = observation.chars().take(700).collect();
    core.write_memory(
        project,
        MemoryNamespace::ProcessMemory,
        Some(branch_root),
        format!("Observation for {title}: {excerpt}"),
        CommandContext {
            actor: Actor::LeadAgent(LEAD_IDENTITY.into()),
            reason: "Persist compact branch-local tool observation".into(),
            run_id: Some(called.run_id),
            correlation_id: None,
            causation_id: None,
        },
    )?;
    let completed = lead_step(
        core,
        project,
        provider,
        workspace,
        telemetry,
        fail_once,
        "Inspect the persisted tool observation, then complete this current WorkNode.".into(),
        question,
        parent_run,
        child.id,
    )?;
    if !matches!(
        completed.decision,
        AgentDecision::Process {
            action: ProcessAction::CompleteWorkNode { .. },
            ..
        }
    ) {
        return Err(Error::Validation(
            "Lead did not complete evidence WorkNode after its successful tool".into(),
        ));
    }
    Ok((child, output))
}

fn synthetic_corpus(kind: &str, seed: u64) -> Vec<CorpusDocument> {
    if kind == "branching" {
        let mut docs = vec![
            doc("a-1", "support", "A", "The transport response under osmotic adaptation was consistent with gated aquaporin conductance across randomized cell preparations."),
            doc("a-2", "support", "A", "Independent microscopy showed reversible pore occupancy associated with the measured water flux; the assay did not assess lipid damage."),
            doc("a-3", "qualification", "A", "The conductance estimate changed after washout calibration, limiting direct comparison with the oxidation panel."),
            doc("b-1", "support", "B", "An early oxidation panel associated lipid peroxidation markers with permeability changes in the treatment arm."),
            doc("b-2", "contradiction", "B", "A blinded negative-control cohort found no treatment-linked lipid oxidation after baseline drift correction; the earlier association was not reproduced."),
            doc("b-3", "qualification", "B", "The second laboratory used a lower-intensity exposure protocol and reported no durable oxidative signature."),
            doc("c-1", "discriminator", "C", "The paired recovery assay separated reversible channel occupancy from persistent membrane damage, but measured neither outcome in the same cells."),
        ];
        for i in 0..9 {
            docs.push(doc(&format!("d-{i}"), "distractor", "D", &format!("Administrative calibration record {} documents instrument maintenance, sample coding and routine image quality review without a biological outcome.", i)));
        }
        if seed % 2 == 1 {
            docs.reverse();
        }
        docs
    } else {
        let mut docs = vec![
            large_doc("A-needle", "initial_observation", "A", "Repeated moderate stimulation produced a reversible membrane response change in the primary assay; the observation is descriptive and does not establish mechanism."),
            large_doc("G-qualification", "qualification", "G", "A washout interval and baseline calibration qualify the reported response; estimates depend on recovery timing and batch adjustment."),
            large_doc("K-contradiction", "contradiction", "K", "A preregistered blinded negative-control cohort showed no reproducible treatment effect after drift correction, contradicting the initial observation."),
        ];
        for i in 0..29 {
            let role = if i % 5 == 0 {
                "near_duplicate"
            } else {
                "irrelevant"
            };
            let intro = if role == "near_duplicate" {
                "A preliminary response note repeated the initial protocol summary but contained no independent cohort or calibrated endpoint."
            } else {
                "Routine logistics covered sample receipt, equipment inspection, staff training, archival identifiers and data transfer with no outcome measurement."
            };
            docs.push(large_doc(&format!("D-{i:02}"), role, "D", intro));
        }
        if seed % 2 == 1 {
            docs.reverse();
        }
        docs
    }
}

fn doc(key: &str, role: &str, facet: &str, text: &str) -> CorpusDocument {
    CorpusDocument {
        key: key.into(),
        role: role.into(),
        facet: facet.into(),
        title: format!("Research report {key}"),
        text: format!("{text} Study design, analytical limitations, batch metadata and the reported measurement are retained with this source record."),
        source_id: None,
    }
}

fn large_doc(key: &str, role: &str, facet: &str, intro: &str) -> CorpusDocument {
    let mut document = doc(key, role, facet, intro);
    let filler = "Samples were processed in randomized batches; calibration logs, instrument identifiers, protocol versions and quality-control metadata were archived for independent review. ";
    let target_bytes = if role == "initial_observation" {
        24_000
    } else {
        5_200
    };
    while document.text.len() < target_bytes {
        document.text.push_str(filler);
    }
    document
}

fn load_corpus(
    core: &Core,
    project: ProjectId,
    kind: &str,
    seed: u64,
) -> Result<(Vec<CorpusDocument>, u64, u64)> {
    let mut documents = synthetic_corpus(kind, seed);
    let mut corpus_bytes = 0u64;
    for document in &mut documents {
        let source = core.sources().add(
            project,
            NewSource {
                title: document.title.clone(),
                source_kind: SourceKind::Manual,
                locator: format!("capability://{kind}/{seed}/{}", document.key),
                checksum: None,
                metadata: json!({"schema_version":1,"role":document.role,"facet":document.facet,"key":document.key}),
            },
            CommandContext::human("focused-eval", "load deterministic capability corpus"),
        )?;
        core.papers().add(
            project,
            NewPaper {
                title: document.title.clone(),
                authors: vec!["Synthetic capability fixture".into()],
                publication_year: Some(2026),
                doi: None,
                abstract_text: document.text.clone(),
                source_ids: vec![source.id],
                metadata: json!({"schema_version":1,"role":document.role,"facet":document.facet,"key":document.key}),
            },
            CommandContext::human("focused-eval", "index deterministic document"),
        )?;
        document.source_id = Some(source.id);
        corpus_bytes = corpus_bytes.saturating_add(document.text.len() as u64);
    }
    // English technical prose averages more than four characters per token;
    // this deliberately conservative estimate still exceeds 30K for the corpus.
    let estimated_tokens = corpus_bytes / 4;
    Ok((documents, corpus_bytes, estimated_tokens))
}

fn model_provider(config: &HarnessConfig) -> Result<Arc<dyn ModelProvider>> {
    let local = TestLeadConfig {
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
    Ok(Arc::new(LocalOnlyTestProvider::new(Arc::new(
        OpenAiCompatibleModelProvider::new(local)?,
    ))))
}

fn open_core(workspace: &Workspace) -> Result<Arc<Core>> {
    Ok(Arc::new(Core::new(Arc::new(SqliteStorage::open(
        workspace.database_path()?,
    )?))))
}

fn init_project(
    core: &Core,
    kind: &str,
    seed: u64,
    question: &str,
    budget: u32,
) -> Result<(ProjectId, QuestionId, BranchId, WorkNodeId, TaskId, RunId)> {
    let project = core.projects().create(
        format!("focused-{kind}-{seed}"),
        "Synthetic pack for true branching / long-context capability evaluation".into(),
        CommandContext::human("focused-eval", "create isolated focused project"),
    )?;
    let q = core.questions().add(
        project.id,
        question.into(),
        "Use independent retrieval, preserve qualifications and contradictions, and do not infer unsupported conclusions.".into(),
        CommandContext::human("focused-eval", "create focused research question"),
    )?;
    let branch = core.create_branch(
        project.id,
        "main".into(),
        None,
        CommandContext::human("focused-eval", "initialize main research branch"),
    )?;
    let root = core.create_work(
        project.id,
        NewWork {
            branch_id: branch.id,
            kind: format!("{kind}-research"),
            granularity: WorkGranularity::Composite,
            title: format!("Research {kind}"),
            objective: if kind == "branching" {
                "Compare independent lines of membrane mechanism evidence".into()
            } else {
                "Research the supplied evidence corpus using bounded retrieval".into()
            },
            acceptance_criteria: OutputContract {
                required_entities: vec![EntityRef::Question(q.id)],
                required_artifacts: vec![],
                minimum_children: 1,
            },
            capability_requirements: vec![],
            context_policy_id: "focused-bounded-context-v1".into(),
            budget: WorkBudget {
                context_tokens: budget,
                max_model_calls: 64,
                max_tool_calls: 64,
            },
        },
        CommandContext::human("focused-eval", "create bounded research plan"),
    )?;
    let root = core.transition_work(
        project.id,
        root.id,
        root.meta.version,
        WorkStatus::Ready,
        CommandContext::human("focused-eval", "ready focused research plan"),
    )?;
    let api = core.agent_api(LEAD_IDENTITY)?;
    let task = api.create_task(project.id, format!("focused {kind} capability run {seed}"))?;
    let run = api.start_run(project.id, task.id, None)?;
    api.bind_run(project.id, run.id, root.id)?;
    Ok((project.id, q.id, branch.id, root.id, task.id, run.id))
}

fn snapshot_records(state: &ProjectState) -> Vec<SnapshotRecord> {
    let mut snapshots: Vec<_> = state
        .context_snapshots
        .values()
        .filter_map(|snapshot| {
            let node = state.work_nodes.get(&snapshot.work_node_id)?;
            Some(SnapshotRecord {
                id: snapshot.id,
                branch_id: node.branch_id,
                work_node_id: node.id,
                query: snapshot.retrieval_query.clone(),
                candidate_count: snapshot.retrieval_candidate_count,
                selected: snapshot
                    .retrieval_selected
                    .iter()
                    .map(|hit| (hit.entity, hit.lexical_score))
                    .collect(),
                estimated_prompt_tokens: snapshot.estimated_tokens,
                process_memory_ids: snapshot.process_memory_ids.clone(),
                process_memory_branch_ids: snapshot
                    .process_memory_ids
                    .iter()
                    .filter_map(|id| state.process_memories.get(id))
                    .filter_map(|memory| memory.work_node_id)
                    .filter_map(|id| state.work_nodes.get(&id).map(|node| node.branch_id))
                    .collect(),
            })
        })
        .collect();
    snapshots.sort_by_key(|snapshot| {
        state
            .context_snapshots
            .get(&snapshot.id)
            .map(|snapshot| snapshot.meta.created_at)
    });
    snapshots
}

fn token_stats(state: &ProjectState) -> Vec<u64> {
    let mut tokens: Vec<_> = state
        .model_invocations
        .values()
        .filter_map(|invocation| invocation.metadata["preflight_input_tokens"].as_u64())
        .collect();
    tokens.sort_unstable();
    tokens
}

fn find_created_branch(
    state: &ProjectState,
    old: &BTreeSet<BranchId>,
    main: BranchId,
) -> Result<Branch> {
    state
        .branches
        .values()
        .find(|branch| branch.parent_branch_id == Some(main) && !old.contains(&branch.id))
        .cloned()
        .ok_or_else(|| Error::NotFound("Lead-created child branch".into()))
}

fn find_branch_root(state: &ProjectState, branch: BranchId, title: &str) -> Result<WorkNodeId> {
    state
        .work_nodes
        .values()
        .find(|node| {
            node.branch_id == branch
                && node.granularity == WorkGranularity::Composite
                && node.title == title
        })
        .map(|node| node.id)
        .ok_or_else(|| Error::NotFound("branch root WorkNode".into()))
}

fn branch_result(core: &Core, branch: Branch, result: String) -> Result<BranchResult> {
    let state = core.runtime_state(branch.meta.project_id)?;
    let nodes: Vec<_> = state
        .work_nodes
        .values()
        .filter(|node| node.branch_id == branch.id)
        .collect();
    let work_node_ids: Vec<WorkNodeId> = nodes.iter().map(|node| node.id).collect();
    let completed_work_nodes = nodes
        .iter()
        .filter(|node| node.status == WorkStatus::Succeeded)
        .count();
    let process_memory_ids = state
        .process_memories
        .values()
        .filter(|memory| {
            memory
                .work_node_id
                .is_some_and(|id| nodes.iter().any(|node| node.id == id))
        })
        .map(|memory| memory.id)
        .collect();
    Ok(BranchResult {
        id: branch.id,
        name: branch.name,
        parent_branch_id: branch.parent_branch_id,
        base_checkpoint_id: branch.base_checkpoint_id,
        work_node_ids: work_node_ids.clone(),
        completed_work_nodes,
        result_summary: result,
        process_memory_ids,
        artifact_ids: state
            .tool_invocations
            .values()
            .filter(|invocation| work_node_ids.contains(&invocation.work_node_id))
            .flat_map(|invocation| {
                invocation
                    .input_artifacts
                    .iter()
                    .chain(&invocation.output_artifacts)
                    .copied()
            })
            .collect(),
    })
}

fn checkpoint_active_parent(
    core: &Core,
    project: ProjectId,
    run: RunId,
    work: WorkNodeId,
) -> Result<CheckpointId> {
    let state = core.runtime_state(project)?;
    let binding = state
        .execution_bindings
        .get(&run)
        .ok_or_else(|| Error::NotFound("active parent execution binding".into()))?;
    if binding.work_node_id != work {
        return Err(Error::Validation(
            "restart checkpoint target differs from parent run binding".into(),
        ));
    }
    let snapshot = state
        .context_snapshots
        .values()
        .filter(|snapshot| {
            snapshot.work_node_id == work && snapshot.plan_revision_id == binding.plan_revision_id
        })
        .max_by_key(|snapshot| snapshot.meta.created_at)
        .ok_or_else(|| Error::NotFound("parent-run ContextSnapshot for restart".into()))?;
    let api = core.agent_api(LEAD_IDENTITY)?;
    Ok(api
        .checkpoint_step(
            project,
            run,
            work,
            snapshot.id,
            json!({"restart_boundary":"after-focused-capability-step"}),
        )?
        .id)
}

fn resume_parent_checkpoint(
    core: &Core,
    project: ProjectId,
    id: CheckpointId,
    branch: BranchId,
) -> Result<bool> {
    let resumed = core.resume_checkpoint(project, id)?;
    Ok(resumed.can_execute && resumed.plan.branch_id == branch)
}

fn transition_work(
    core: &Core,
    project: ProjectId,
    id: WorkNodeId,
    status: WorkStatus,
) -> Result<()> {
    let work = core
        .runtime_state(project)?
        .work_nodes
        .get(&id)
        .cloned()
        .ok_or_else(|| Error::NotFound("work for transition".into()))?;
    core.transition_work(
        project,
        id,
        work.meta.version,
        status,
        CommandContext::human("focused-eval", "finalize verified research work"),
    )?;
    Ok(())
}

fn setup_report(
    kind: &str,
    seed: u64,
    model: &str,
    capacity: u32,
    path: &Path,
) -> CapabilityRunReport {
    CapabilityRunReport {
        capability: kind.into(),
        seed,
        workflow_status: "failed".into(),
        capability_status: "not_exercised".into(),
        error: None,
        project_id: None,
        model: model.into(),
        context_capacity: capacity,
        corpus_bytes: 0,
        corpus_estimated_tokens: 0,
        branch_results: vec![],
        persisted_branches: vec![],
        branch_results_reconciled: false,
        restart_count: 0,
        restart_preserved_completed_work: false,
        completed_work_ids_before_restart: vec![],
        work_bindings_before_restart: 0,
        retrievals: vec![],
        source_reads: vec![],
        range_reads: vec![],
        snapshots: vec![],
        prompt_tokens: vec![],
        process_memory_count: 0,
        integrity_audit: AuditReport::default(),
        recovery_audit_passed: false,
        duplicate_consequential_operations: 0,
        unfinished_runs: 0,
        lead_calls: 0,
        tool_calls: 0,
        retrieval_recall_at_selected: None,
        required_evidence_recovered: vec![],
        total_execution_history_in_prompts: false,
        preflight_failures: vec![],
        elapsed_ms: 0,
        report_path: path.to_string_lossy().into_owned(),
    }
}

fn process_retrievals(telemetry: &Arc<Mutex<Telemetry>>) -> Result<TelemetrySnapshot> {
    let data = telemetry
        .lock()
        .map_err(|_| Error::Storage("capability telemetry lock".into()))?;
    Ok(TelemetrySnapshot {
        retrievals: data.retrievals.clone(),
        source_reads: data.source_reads.clone(),
        range_reads: data.range_reads.clone(),
        preflight_failures: data.preflight_failures.clone(),
    })
}

#[derive(Clone)]
struct TelemetrySnapshot {
    retrievals: Vec<RetrievalRecord>,
    source_reads: Vec<SourceReadRecord>,
    range_reads: Vec<RangeReadRecord>,
    preflight_failures: Vec<String>,
}

fn create_start_report(
    config: &HarnessConfig,
    kind: &str,
    seed: u64,
    workspace: &Arc<Workspace>,
    core: &Arc<Core>,
    question_text: &str,
    capacity: u32,
) -> Result<(
    ProjectId,
    QuestionId,
    BranchId,
    WorkNodeId,
    TaskId,
    RunId,
    Vec<CorpusDocument>,
    u64,
    u64,
)> {
    let (project, question, branch, root, task, run) =
        init_project(core, kind, seed, question_text, capacity)?;
    let (docs, bytes, tokens) = load_corpus(core, project, kind, seed)?;
    let _ = workspace;
    let _ = config;
    Ok((
        project, question, branch, root, task, run, docs, bytes, tokens,
    ))
}

fn step_or_preflight(
    report: &mut CapabilityRunReport,
    core: &Arc<Core>,
    project: ProjectId,
    provider: Arc<dyn ModelProvider>,
    workspace: Arc<Workspace>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    goal: String,
    question: QuestionId,
    parent_run: RunId,
    work: WorkNodeId,
    action_constraint: Option<ProcessActionKind>,
) -> Result<AgentStepOutcome> {
    match lead_step_with_constraint(
        core,
        project,
        provider,
        workspace,
        telemetry.clone(),
        fail_once,
        goal,
        question,
        parent_run,
        work,
        action_constraint,
    ) {
        Ok(step) => {
            report.lead_calls += 1;
            Ok(step)
        }
        Err(error) => {
            let text = error.to_string();
            if text.contains("preflight_input_tokens") || text.contains("context budget") {
                report.preflight_failures.push(text.clone());
                if let Ok(mut captured) = telemetry.lock() {
                    captured.preflight_failures.push(text.clone());
                }
            }
            Err(error)
        }
    }
}

fn create_branch_via_lead(
    report: &mut CapabilityRunReport,
    core: &Arc<Core>,
    project: ProjectId,
    provider: Arc<dyn ModelProvider>,
    workspace: Arc<Workspace>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    question: QuestionId,
    parent_run: RunId,
    main_root: WorkNodeId,
    main_branch: BranchId,
    requested_name: &str,
) -> Result<Branch> {
    let before: BTreeSet<_> = core
        .runtime_state(project)?
        .branches
        .keys()
        .copied()
        .collect();
    let outcome = step_or_preflight(
        report,
        core,
        project,
        provider,
        workspace,
        telemetry,
        fail_once,
        format!("This question has independent explanatory paths that need separate plans and evidence. Create one persistent child branch named '{requested_name}' from the current shared research checkpoint. Do not create a WorkNode for this split."),
        question,
        parent_run,
        main_root,
        Some(ProcessActionKind::CreateBranch),
    )?;
    if !matches!(
        outcome.decision,
        AgentDecision::Process {
            action: ProcessAction::CreateBranch { .. },
            ..
        }
    ) {
        return Err(Error::Validation(
            "Lead did not propose a persistent research branch".into(),
        ));
    }
    find_created_branch(&core.runtime_state(project)?, &before, main_branch)
}

fn run_branch_evidence(
    _report: &mut CapabilityRunReport,
    core: &Arc<Core>,
    project: ProjectId,
    provider: Arc<dyn ModelProvider>,
    workspace: Arc<Workspace>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    question: QuestionId,
    parent_run: RunId,
    branch: &Branch,
    root_title: &str,
    searches: [(&str, &str, &str); 2],
) -> Result<Vec<WorkNodeId>> {
    let root = find_branch_root(&core.runtime_state(project)?, branch.id, root_title)?;
    let mut completed = vec![];
    for (index, (title, query, evidence_goal)) in searches.into_iter().enumerate() {
        let objective = format!(
            "Branch {} evidence search {}: {query}",
            branch.name,
            index + 1
        );
        let run_goal = format!("Use the registered search_sources tool with this branch-specific query: {query}. Inspect the returned evidence subset and preserve its source references. {evidence_goal}");
        let (node, _output) = create_tool_work(
            core,
            project,
            provider.clone(),
            workspace.clone(),
            telemetry.clone(),
            fail_once.clone(),
            question,
            parent_run,
            root,
            branch.id,
            title,
            &objective,
            "search_sources",
            run_goal,
        )?;
        completed.push(node.id);
    }
    let current = core
        .runtime_state(project)?
        .work_nodes
        .get(&root)
        .cloned()
        .ok_or_else(|| Error::NotFound("branch root after evidence".into()))?;
    if current.status == WorkStatus::Running {
        transition_work(core, project, root, WorkStatus::Succeeded)?;
    }
    Ok(completed)
}

fn create_reconciliation_child(
    _report: &mut CapabilityRunReport,
    core: &Arc<Core>,
    project: ProjectId,
    provider: Arc<dyn ModelProvider>,
    workspace: Arc<Workspace>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    question: QuestionId,
    parent_run: RunId,
    main_branch: BranchId,
    main_root: WorkNodeId,
    left: &BranchResult,
    right: &BranchResult,
) -> Result<()> {
    let left_obs = left.result_summary.chars().take(650).collect::<String>();
    let right_obs = right.result_summary.chars().take(650).collect::<String>();
    let goal = format!("Create one atomic comparison WorkNode requiring search_sources. Its purpose is to recheck shared discriminator evidence after two independent branches. Branch {} persisted: {} Branch {} persisted: {}", left.name, left_obs, right.name, right_obs);
    let (node, _) = create_tool_work(
        core,
        project,
        provider,
        workspace,
        telemetry,
        fail_once,
        question,
        parent_run,
        main_root,
        main_branch,
        "Recheck the branch discriminator",
        "Retrieve shared discriminator evidence to support branch reconciliation",
        "search_sources",
        goal,
    )?;
    if core.runtime_state(project)?.work_nodes[&node.id].status != WorkStatus::Succeeded {
        return Err(Error::InvalidTransition(
            "reconciliation retrieval WorkNode did not complete".into(),
        ));
    }
    Ok(())
}

fn decision_finish_summary(step: &AgentStepOutcome) -> Option<String> {
    match &step.decision {
        AgentDecision::Process {
            action: ProcessAction::FinishResearch { summary, .. },
            ..
        } => Some(summary.clone()),
        _ => None,
    }
}

fn run_branching(
    report: &mut CapabilityRunReport,
    config: &HarnessConfig,
    workspace: Arc<Workspace>,
    core: &mut Arc<Core>,
    provider: Arc<dyn ModelProvider>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    seed: u64,
) -> Result<()> {
    let question_text = "Which mechanism best explains the membrane response: reversible aquaporin conductance or persistent lipid oxidation? Investigate these independent evidence families and compare their qualifications and controls.";
    let (project, question, main_branch, main_root, _, parent_run, docs, bytes, tokens) =
        create_start_report(
            config,
            "branching",
            seed,
            &workspace,
            core,
            question_text,
            config.context_size,
        )?;
    report.project_id = Some(project);
    report.corpus_bytes = bytes;
    report.corpus_estimated_tokens = tokens;
    let branch_a = create_branch_via_lead(
        report,
        core,
        project,
        provider.clone(),
        workspace.clone(),
        telemetry.clone(),
        fail_once.clone(),
        question,
        parent_run,
        main_root,
        main_branch,
        "mechanism-a",
    )?;
    let branch_b = create_branch_via_lead(
        report,
        core,
        project,
        provider.clone(),
        workspace.clone(),
        telemetry.clone(),
        fail_once.clone(),
        question,
        parent_run,
        main_root,
        main_branch,
        "mechanism-b",
    )?;
    if branch_a.id == branch_b.id || branch_a.parent_branch_id != branch_b.parent_branch_id {
        return Err(Error::Validation(
            "Lead-created branches do not share a parent".into(),
        ));
    }
    let state = core.runtime_state(project)?;
    let base_a = branch_a
        .base_checkpoint_id
        .ok_or_else(|| Error::NotFound("branch A base".into()))?;
    let base_b = branch_b
        .base_checkpoint_id
        .ok_or_else(|| Error::NotFound("branch B base".into()))?;
    if base_a != base_b {
        return Err(Error::Validation(
            "sibling branches do not share the exact same base checkpoint".into(),
        ));
    }
    let root_title = state.work_nodes[&main_root].title.clone();
    let work_a = run_branch_evidence(
        report,
        core,
        project,
        provider.clone(),
        workspace.clone(),
        telemetry.clone(),
        fail_once.clone(),
        question,
        parent_run,
        &branch_a,
        &root_title,
        [
            (
                "Retrieve aquaporin evidence",
                "osmotic adaptation aquaporin conductance pore occupancy",
                "Prefer independent source records and preserve limitations.",
            ),
            (
                "Retrieve channel qualification",
                "washout calibration reversible water flux conductance",
                "Check whether calibration changes the estimated effect.",
            ),
        ],
    )?;
    let a_evidence: BTreeSet<_> = process_retrievals(&telemetry)?
        .retrievals
        .into_iter()
        .filter(|retrieval| retrieval.branch_id == Some(branch_a.id))
        .flat_map(|retrieval| retrieval.selected.into_iter().map(|doc| doc.role))
        .collect();
    if !a_evidence.contains("support") || !a_evidence.contains("qualification") {
        return Err(Error::Validation(
            "branch A did not retrieve its independent support and qualification evidence".into(),
        ));
    }
    let a_summary = format!(
        "aquaporin branch retrieved evidence roles: {}",
        a_evidence.into_iter().collect::<Vec<_>>().join(", ")
    );
    let a_result = branch_result(core, branch_a.clone(), a_summary)?;
    let completed_before_restart: BTreeSet<_> = work_a.iter().copied().collect();
    let before_restart = core.runtime_state(project)?;
    report.completed_work_ids_before_restart = completed_before_restart.iter().copied().collect();
    report.work_bindings_before_restart = before_restart
        .execution_bindings
        .values()
        .filter(|binding| completed_before_restart.contains(&binding.work_node_id))
        .count();
    let restart_checkpoint = checkpoint_active_parent(core, project, parent_run, main_root)?;
    *core = open_core(&workspace)?;
    report.restart_count += 1;
    let restored_state = core.runtime_state(project)?;
    let restored_bindings = restored_state
        .execution_bindings
        .values()
        .filter(|binding| completed_before_restart.contains(&binding.work_node_id))
        .count();
    report.restart_preserved_completed_work =
        resume_parent_checkpoint(core, project, restart_checkpoint, main_branch)?
            && restored_bindings == report.work_bindings_before_restart
            && completed_before_restart.iter().all(|id| {
                restored_state
                    .work_nodes
                    .get(id)
                    .is_some_and(|node| node.status == WorkStatus::Succeeded)
            });
    let branch_b_restored = restored_state.branches[&branch_b.id].clone();
    let _work_b = run_branch_evidence(
        report, core, project, provider.clone(), workspace.clone(), telemetry.clone(), fail_once.clone(), question, parent_run, &branch_b_restored, &root_title,
        [
            ("Retrieve initial oxidation association", "early treatment association lipid markers permeability", "Record the initial positive result without treating it as decisive."),
            ("Refine retrieval for blinded controls", "blinded negative-control baseline drift correction reproducible treatment effect", "This is a new refined query; seek evidence that could contradict the initial association."),
        ],
    )?;
    let b_evidence: BTreeSet<_> = process_retrievals(&telemetry)?
        .retrievals
        .into_iter()
        .filter(|retrieval| retrieval.branch_id == Some(branch_b.id))
        .flat_map(|retrieval| retrieval.selected.into_iter().map(|doc| doc.role))
        .collect();
    if !b_evidence.contains("support") || !b_evidence.contains("contradiction") {
        return Err(Error::Validation(
            "branch B did not retrieve its initial support and contradictory evidence".into(),
        ));
    }
    let b_summary = format!(
        "lipid oxidation branch retrieved evidence roles: {}",
        b_evidence.into_iter().collect::<Vec<_>>().join(", ")
    );
    let b_result = branch_result(core, branch_b_restored, b_summary)?;
    let branch_results = vec![a_result, b_result];
    create_reconciliation_child(
        report,
        core,
        project,
        provider.clone(),
        workspace.clone(),
        telemetry.clone(),
        fail_once.clone(),
        question,
        parent_run,
        main_branch,
        main_root,
        &branch_results[0],
        &branch_results[1],
    )?;
    let compare_goal = format!("Compare results from both persistent research branches and the discriminator retrieval. Branch {} evidence: {} Branch {} evidence: {} Reconcile the support and contradiction without declaring unsupported certainty. Finish this research now with a concise comparison that explicitly refers to the aquaporin evidence and the lipid oxidation control evidence.", branch_results[0].name, branch_results[0].result_summary, branch_results[1].name, branch_results[1].result_summary);
    let reconciled = step_or_preflight(
        report,
        core,
        project,
        provider,
        workspace,
        telemetry.clone(),
        fail_once,
        compare_goal,
        question,
        parent_run,
        main_root,
        Some(ProcessActionKind::FinishResearch),
    )?;
    let summary = decision_finish_summary(&reconciled).ok_or_else(|| {
        Error::Validation("Lead did not reconcile and finish from the shared main branch".into())
    })?;
    let summary_lower = summary.to_lowercase();
    if !summary_lower.contains("aquaporin") || !summary_lower.contains("oxidation") {
        return Err(Error::Validation(
            "reconciliation summary did not use both branch results".into(),
        ));
    }
    report.branch_results_reconciled = true;
    let api = core.agent_api(LEAD_IDENTITY)?;
    api.finish(project, parent_run, RunStatus::Completed)?;
    let state = core.runtime_state(project)?;
    if state.work_nodes[&main_root].status == WorkStatus::Running {
        transition_work(core, project, main_root, WorkStatus::Succeeded)?;
    }
    report.branch_results = branch_results;
    report.required_evidence_recovered = process_retrievals(&telemetry)?
        .retrievals
        .iter()
        .flat_map(|retrieval| retrieval.selected.iter().map(|doc| doc.role.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    report.workflow_status = "completed".into();
    let _ = docs;
    Ok(())
}

fn find_doc(docs: &[CorpusDocument], role: &str) -> Result<CorpusDocument> {
    docs.iter()
        .find(|doc| doc.role == role)
        .cloned()
        .ok_or_else(|| Error::NotFound(format!("corpus role {role}")))
}

fn selected_role(record: &RetrievalRecord, role: &str) -> Option<RetrievedDocument> {
    record
        .selected
        .iter()
        .find(|document| document.role == role)
        .cloned()
}

fn create_and_run_step(
    _report: &mut CapabilityRunReport,
    core: &Arc<Core>,
    project: ProjectId,
    provider: Arc<dyn ModelProvider>,
    workspace: Arc<Workspace>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    question: QuestionId,
    parent_run: RunId,
    main_root: WorkNodeId,
    main_branch: BranchId,
    title: &str,
    objective: &str,
    tool_name: &str,
    execution_goal: String,
) -> Result<(WorkNode, ToolOutput)> {
    create_tool_work(
        core,
        project,
        provider,
        workspace,
        telemetry,
        fail_once,
        question,
        parent_run,
        main_root,
        main_branch,
        title,
        objective,
        tool_name,
        execution_goal,
    )
}

fn extract_offloaded_artifact(output: &ToolOutput) -> Option<ArtifactId> {
    if output.data["type"] != "artifact" {
        return None;
    }
    output.data["id"].as_str().and_then(|raw| raw.parse().ok())
}

fn run_long_context(
    report: &mut CapabilityRunReport,
    config: &HarnessConfig,
    workspace: Arc<Workspace>,
    core: &mut Arc<Core>,
    provider: Arc<dyn ModelProvider>,
    telemetry: Arc<Mutex<Telemetry>>,
    fail_once: Arc<AtomicBool>,
    seed: u64,
) -> Result<()> {
    let question_text = "Across the supplied large research corpus, what can be concluded about the membrane response to repeated moderate stimulation? Retrieve the primary observation, its washout/calibration qualification, and any later blinded-control contradiction through iterative targeted search.";
    let (project, question, main_branch, main_root, _, parent_run, docs, bytes, tokens) =
        create_start_report(
            config,
            "long-context",
            seed,
            &workspace,
            core,
            question_text,
            config.context_size,
        )?;
    report.project_id = Some(project);
    report.corpus_bytes = bytes;
    report.corpus_estimated_tokens = tokens;
    if tokens <= config.context_size as u64 {
        return Err(Error::Validation(
            "synthetic corpus does not exceed the configured context".into(),
        ));
    }
    let observation = find_doc(&docs, "initial_observation")?;
    let qualification = find_doc(&docs, "qualification")?;
    let contradiction = find_doc(&docs, "contradiction")?;

    let (search_node, _search_output) = create_and_run_step(
        report, core, project, provider.clone(), workspace.clone(), telemetry.clone(), fail_once.clone(), question, parent_run, main_root, main_branch,
        "Search for the initial observation", "Find the primary repeated-stimulation membrane response evidence", "search_sources",
        "Use search_sources with query terms for the initial observation: repeated stimulation membrane response adaptation. Identify the best matching source reference.".into(),
    )?;
    let telemetry_now = process_retrievals(&telemetry)?;
    let first_search = telemetry_now
        .retrievals
        .last()
        .ok_or_else(|| Error::NotFound("first FTS retrieval record".into()))?;
    if selected_role(first_search, "initial_observation").is_none() {
        return Err(Error::NotFound(
            "initial observation was not selected by FTS/BM25".into(),
        ));
    }
    if selected_role(first_search, "contradiction").is_some() {
        return Err(Error::Validation("later contradiction appeared in the first retrieval; iterative contradiction retrieval was not exercised".into()));
    }
    let target_source = observation
        .source_id
        .ok_or_else(|| Error::NotFound("observation source ID".into()))?;
    let (_read_node, read_output) = create_and_run_step(
        report, core, project, provider.clone(), workspace.clone(), telemetry.clone(), fail_once.clone(), question, parent_run, main_root, main_branch,
        "Read the large primary source", "Read the retrieved long report and preserve the initial observation", "read_source",
        format!("Use read_source for source_id {target_source}. The full report is large; retain the returned artifact reference and evidence wording."),
    )?;
    let artifact_id = extract_offloaded_artifact(&read_output).ok_or_else(|| {
        Error::Validation("large source read was not offloaded into an Artifact".into())
    })?;
    if let Some(read) = telemetry
        .lock()
        .map_err(|_| Error::Storage("telemetry lock".into()))?
        .source_reads
        .last_mut()
    {
        read.artifact_id = Some(artifact_id);
    }
    let before_restart = core.runtime_state(project)?;
    let completed_before_restart: BTreeSet<_> = before_restart
        .work_nodes
        .values()
        .filter(|node| node.branch_id == main_branch && node.status == WorkStatus::Succeeded)
        .map(|node| node.id)
        .collect();
    report.completed_work_ids_before_restart = completed_before_restart.iter().copied().collect();
    report.work_bindings_before_restart = before_restart
        .execution_bindings
        .values()
        .filter(|binding| completed_before_restart.contains(&binding.work_node_id))
        .count();
    let restart_checkpoint = checkpoint_active_parent(core, project, parent_run, main_root)?;
    *core = open_core(&workspace)?;
    report.restart_count += 1;
    let after_restart = core.runtime_state(project)?;
    let restored_bindings = after_restart
        .execution_bindings
        .values()
        .filter(|binding| completed_before_restart.contains(&binding.work_node_id))
        .count();
    report.restart_preserved_completed_work =
        resume_parent_checkpoint(core, project, restart_checkpoint, main_branch)?
            && restored_bindings == report.work_bindings_before_restart
            && completed_before_restart.iter().all(|id| {
                after_restart
                    .work_nodes
                    .get(id)
                    .is_some_and(|node| node.status == WorkStatus::Succeeded)
            });
    if !core
        .runtime_state(project)?
        .artifacts
        .contains_key(&artifact_id)
    {
        return Err(Error::NotFound(
            "offloaded Artifact did not survive restart".into(),
        ));
    }

    let (_range_node, _range_output) = create_and_run_step(
        report, core, project, provider.clone(), workspace.clone(), telemetry.clone(), fail_once.clone(), question, parent_run, main_root, main_branch,
        "Inspect an offloaded source range", "Read a targeted range from the offloaded primary report", "read_artifact_range",
        format!("Use read_artifact_range for artifact_id {artifact_id}, start 0, length 1024. Read only this bounded range, not the complete Artifact."),
    )?;
    let _ = search_node;

    for (title, objective, query, role) in [
        (
            "Retrieve the qualification",
            "Find the source qualification about washout timing and baseline calibration",
            "washout calibration recovery timing baseline qualification",
            "qualification",
        ),
        (
            "Refine search for contradictory controls",
            "Use a new query to test whether blinded controls contradict the first result",
            "blinded negative control cohort drift correction reproducible effect",
            "contradiction",
        ),
    ] {
        create_and_run_step(
            report, core, project, provider.clone(), workspace.clone(), telemetry.clone(), fail_once.clone(), question, parent_run, main_root, main_branch,
            title, objective, "search_sources",
            format!("Use search_sources with a newly refined query: {query}. This is iterative evidence retrieval after the earlier observation, not a repeat of the first query."),
        )?;
        let now = process_retrievals(&telemetry)?;
        let latest = now
            .retrievals
            .last()
            .ok_or_else(|| Error::NotFound("refined retrieval record".into()))?;
        if selected_role(latest, role).is_none() {
            return Err(Error::NotFound(format!(
                "required evidence role {role} not retrieved"
            )));
        }
    }

    let final_prompt = format!("The corpus has been searched in bounded pieces. Primary observation source: {}. Qualification source: {}. Contradictory control source: {}. Compare the evidence and its limitations, then finish with the result completed or inconclusive. Do not claim the entire corpus was in the prompt.", observation.title, qualification.title, contradiction.title);
    let finished = step_or_preflight(
        report,
        core,
        project,
        provider,
        workspace,
        telemetry.clone(),
        fail_once,
        final_prompt,
        question,
        parent_run,
        main_root,
        Some(ProcessActionKind::FinishResearch),
    )?;
    if decision_finish_summary(&finished).is_none() {
        return Err(Error::Validation(
            "Lead did not complete the long-context synthesis after retrieved evidence".into(),
        ));
    }
    let api = core.agent_api(LEAD_IDENTITY)?;
    api.finish(project, parent_run, RunStatus::Completed)?;
    if core.runtime_state(project)?.work_nodes[&main_root].status == WorkStatus::Running {
        transition_work(core, project, main_root, WorkStatus::Succeeded)?;
    }
    report.required_evidence_recovered = vec![
        "initial_observation".into(),
        "qualification".into(),
        "contradiction".into(),
    ];
    let _ = (search_node, _range_node, _range_output);
    report.workflow_status = "completed".into();
    Ok(())
}

fn finalize_report(
    report: &mut CapabilityRunReport,
    core: &Core,
    telemetry: &Arc<Mutex<Telemetry>>,
) -> Result<()> {
    let Some(project) = report.project_id else {
        return Ok(());
    };
    let state = core.runtime_state(project)?;
    report.integrity_audit = audit_project(&state);
    report.persisted_branches = state
        .branches
        .values()
        .map(|branch| {
            let nodes: Vec<_> = state
                .work_nodes
                .values()
                .filter(|node| node.branch_id == branch.id)
                .collect();
            PersistedBranchRecord {
                id: branch.id,
                name: branch.name.clone(),
                parent_branch_id: branch.parent_branch_id,
                base_checkpoint_id: branch.base_checkpoint_id,
                work_node_ids: nodes.iter().map(|node| node.id).collect(),
                completed_work_nodes: nodes
                    .iter()
                    .filter(|node| node.status == WorkStatus::Succeeded)
                    .count(),
            }
        })
        .collect();
    report.recovery_audit_passed =
        report.restart_preserved_completed_work && report.integrity_audit.passed;
    report.snapshots = snapshot_records(&state);
    report.prompt_tokens = token_stats(&state);
    report.process_memory_count = state.process_memories.len();
    report.lead_calls = state.model_invocations.len();
    report.tool_calls = state.tool_invocations.len();
    report.unfinished_runs = state
        .runs
        .values()
        .filter(|run| {
            !matches!(
                run.status,
                RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
            )
        })
        .count();
    report.duplicate_consequential_operations = 0; // This capability harness registers read-only tools only.
    let captured = process_retrievals(telemetry)?;
    report.retrievals = captured.retrievals;
    report.source_reads = captured.source_reads;
    report.range_reads = captured.range_reads;
    report.preflight_failures = captured.preflight_failures;
    Ok(())
}

fn cleanup_failed(core: &Core, project: ProjectId) {
    if let Ok(state) = core.runtime_state(project) {
        for run in state
            .runs
            .values()
            .filter(|run| run.status == RunStatus::Running)
        {
            let _ = core.finish_run(
                project,
                run.id,
                RunStatus::Failed,
                CommandContext::human("focused-eval", "close failed capability evaluation run"),
            );
        }
        let state = core.runtime_state(project).unwrap_or(state);
        for work in state
            .work_nodes
            .values()
            .filter(|work| work.status == WorkStatus::Running)
        {
            let _ = transition_work(core, project, work.id, WorkStatus::Failed);
        }
    }
}

fn check_capability(report: &CapabilityRunReport) -> bool {
    let contexts_bounded = report
        .prompt_tokens
        .iter()
        .all(|tokens| *tokens <= report.context_capacity as u64)
        && report
            .snapshots
            .iter()
            .all(|snapshot| snapshot.estimated_prompt_tokens <= report.context_capacity);
    let common_safety = report.workflow_status == "completed"
        && report.integrity_audit.passed
        && report.recovery_audit_passed
        && report.unfinished_runs == 0
        && report.duplicate_consequential_operations == 0
        && report.restart_preserved_completed_work
        && !report.total_execution_history_in_prompts
        && contexts_bounded
        && report.preflight_failures.is_empty();
    match report.capability.as_str() {
        "branching" => {
            let lineage = report.branch_results.len() >= 2
                && report.branch_results.iter().all(|branch| {
                    branch.parent_branch_id.is_some() && branch.base_checkpoint_id.is_some()
                });
            let branch_ids: BTreeSet<_> = report
                .branch_results
                .iter()
                .map(|branch| branch.id)
                .collect();
            let work_ids: Vec<_> = report
                .branch_results
                .iter()
                .flat_map(|branch| branch.work_node_ids.iter().copied())
                .collect();
            let isolated_work: BTreeSet<_> = work_ids.iter().copied().collect();
            let entities_are_isolated = branch_ids.len() == report.branch_results.len()
                && isolated_work.len() == work_ids.len();
            let persisted_lineage = report.branch_results.iter().all(|branch| {
                report.persisted_branches.iter().any(|persisted| {
                    persisted.id == branch.id
                        && persisted.parent_branch_id == branch.parent_branch_id
                        && persisted.base_checkpoint_id == branch.base_checkpoint_id
                })
            });
            let independent = report
                .branch_results
                .iter()
                .all(|branch| branch.completed_work_nodes >= 2)
                && report
                    .branch_results
                    .iter()
                    .map(|branch| branch.completed_work_nodes)
                    .sum::<usize>()
                    >= 4;
            common_safety
                && lineage
                && entities_are_isolated
                && persisted_lineage
                && independent
                && report.branch_results_reconciled
        }
        "long-context" => {
            let roles: BTreeSet<_> = report
                .required_evidence_recovered
                .iter()
                .map(String::as_str)
                .collect();
            common_safety
                && report.corpus_estimated_tokens > report.context_capacity as u64
                && report.retrievals.len() >= 3
                && report
                    .retrievals
                    .iter()
                    .all(|retrieval| retrieval.candidate_count > 0)
                && report
                    .source_reads
                    .iter()
                    .any(|read| read.offloaded && read.artifact_id.is_some())
                && report.range_reads.iter().any(|range| {
                    range.succeeded
                        && report.source_reads.iter().any(|source| {
                            source.offloaded && source.artifact_id == Some(range.artifact_id)
                        })
                })
                && report.snapshots.len() > 1
                && report
                    .snapshots
                    .iter()
                    .map(|snapshot| snapshot.query.as_str())
                    .collect::<BTreeSet<_>>()
                    .len()
                    > 1
                && report.process_memory_count > 0
                && roles.contains("initial_observation")
                && roles.contains("qualification")
                && roles.contains("contradiction")
        }
        _ => false,
    }
}

pub fn run_capability_case(
    config: &HarnessConfig,
    capability: &str,
    seed: u64,
) -> Result<CapabilityRunReport> {
    if !matches!(capability, "branching" | "long-context") {
        return Err(Error::Validation(
            "capability must be branching or long-context".into(),
        ));
    }
    let started = Instant::now();
    let workspace_path = config
        .workspace
        .join(".runtime/true-capabilities")
        .join(format!(
            "{capability}-{seed}-{}",
            Utc::now().timestamp_millis()
        ));
    let workspace = Arc::new(Workspace::open(&workspace_path)?);
    let mut core = open_core(&workspace)?;
    let provider = model_provider(config)?;
    let telemetry = Arc::new(Mutex::new(Telemetry::default()));
    let fail_once = Arc::new(AtomicBool::new(
        capability == "long-context" && seed.is_multiple_of(5),
    ));
    let report_path = config
        .workspace
        .join("eval-results/true-capabilities")
        .join(format!("{capability}-{seed}.json"));
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::Storage(e.to_string()))?;
    }
    let mut report = setup_report(
        capability,
        seed,
        &config.model_name,
        config.context_size,
        &report_path,
    );
    let result = if capability == "branching" {
        run_branching(
            &mut report,
            config,
            workspace.clone(),
            &mut core,
            provider,
            telemetry.clone(),
            fail_once,
            seed,
        )
    } else {
        run_long_context(
            &mut report,
            config,
            workspace.clone(),
            &mut core,
            provider,
            telemetry.clone(),
            fail_once,
            seed,
        )
    };
    if let Err(error) = result {
        report.error = Some(match error {
            Error::AuthorizationDenied(trace) => serde_json::to_string(&trace)
                .unwrap_or_else(|_| "authorization denied (trace serialization failed)".into()),
            other => other.to_string(),
        });
        if report.workflow_status != "completed" {
            report.workflow_status = "failed".into();
        }
    }
    if let Some(project) = report.project_id {
        cleanup_failed(&core, project);
        if let Err(error) = finalize_report(&mut report, &core, &telemetry) {
            report.error.get_or_insert_with(|| error.to_string());
        }
    }
    report.capability_status = if check_capability(&report) {
        "passed"
    } else {
        "failed"
    }
    .into();
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    std::fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).map_err(|e| Error::Storage(e.to_string()))?,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    Ok(report)
}

pub fn write_capability_campaign(
    config: &HarnessConfig,
    branch_seed: u64,
    context_seed: u64,
    count: usize,
) -> Result<Vec<CapabilityRunReport>> {
    if count != 5 {
        return Err(Error::Validation(
            "focused capability campaign requires exactly 5 runs per capability".into(),
        ));
    }
    let mut results = Vec::with_capacity(10);
    for offset in 0..count {
        results.push(run_capability_case(
            config,
            "branching",
            branch_seed + offset as u64,
        )?);
    }
    for offset in 0..count {
        results.push(run_capability_case(
            config,
            "long-context",
            context_seed + offset as u64,
        )?);
    }
    let root = config.workspace.join("eval-results/true-capabilities");
    std::fs::create_dir_all(&root).map_err(|e| Error::Storage(e.to_string()))?;
    std::fs::write(
        root.join("focused-10.json"),
        serde_json::to_vec_pretty(&results).map_err(|e| Error::Storage(e.to_string()))?,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    let mut markdown = String::from("# True branching and long-context focused campaign\n\n");
    markdown.push_str("| Capability | Seed | Workflow | Capability | Branches | Retrieval | Offloads | Range reads | Snapshots | Max prompt | Recovery / integrity |\n|---|---:|---|---|---:|---:|---:|---:|---:|---:|---|\n");
    for result in &results {
        let branches = result
            .persisted_branches
            .iter()
            .filter(|branch| branch.parent_branch_id.is_some())
            .count();
        let offloads = result
            .source_reads
            .iter()
            .filter(|read| read.offloaded)
            .count();
        let max_prompt = result.prompt_tokens.iter().copied().max().unwrap_or(0);
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {}/{} |\n",
            result.capability,
            result.seed,
            result.workflow_status,
            result.capability_status,
            branches,
            result.retrievals.len(),
            offloads,
            result
                .range_reads
                .iter()
                .filter(|read| read.succeeded)
                .count(),
            result.snapshots.len(),
            max_prompt,
            result.recovery_audit_passed,
            result.integrity_audit.passed
        ));
    }
    markdown.push_str("\nFailures and detailed selections are in the per-seed JSON files. A completed workflow counts as a capability pass only when its structural assertions also hold.\n");
    std::fs::write(root.join("focused-10.md"), markdown)
        .map_err(|e| Error::Storage(e.to_string()))?;
    Ok(results)
}

#[cfg(test)]
mod capability_assertion_tests {
    use super::*;

    fn common_report(capability: &str) -> CapabilityRunReport {
        CapabilityRunReport {
            capability: capability.into(),
            seed: 1,
            workflow_status: "completed".into(),
            capability_status: "not_checked".into(),
            error: None,
            project_id: None,
            model: "fixture".into(),
            context_capacity: 8192,
            corpus_bytes: 0,
            corpus_estimated_tokens: 0,
            branch_results: vec![],
            persisted_branches: vec![],
            branch_results_reconciled: false,
            restart_count: 1,
            restart_preserved_completed_work: true,
            completed_work_ids_before_restart: vec![],
            work_bindings_before_restart: 0,
            retrievals: vec![],
            source_reads: vec![],
            range_reads: vec![],
            snapshots: vec![],
            prompt_tokens: vec![1024],
            process_memory_count: 1,
            integrity_audit: AuditReport {
                passed: true,
                ..AuditReport::default()
            },
            recovery_audit_passed: true,
            duplicate_consequential_operations: 0,
            unfinished_runs: 0,
            lead_calls: 1,
            tool_calls: 1,
            retrieval_recall_at_selected: None,
            required_evidence_recovered: vec![],
            total_execution_history_in_prompts: false,
            preflight_failures: vec![],
            elapsed_ms: 1,
            report_path: "fixture".into(),
        }
    }

    fn valid_branching_report() -> CapabilityRunReport {
        let mut report = common_report("branching");
        let left = BranchId::new();
        let right = BranchId::new();
        let parent = BranchId::new();
        let base = CheckpointId::new();
        report.branch_results = [left, right]
            .into_iter()
            .map(|id| BranchResult {
                id,
                name: format!("branch-{id}"),
                parent_branch_id: Some(parent),
                base_checkpoint_id: Some(base),
                work_node_ids: vec![WorkNodeId::new(), WorkNodeId::new()],
                completed_work_nodes: 2,
                result_summary: "completed evidence work".into(),
                process_memory_ids: vec![],
                artifact_ids: vec![],
            })
            .collect();
        report.persisted_branches = report
            .branch_results
            .iter()
            .map(|branch| PersistedBranchRecord {
                id: branch.id,
                name: branch.name.clone(),
                parent_branch_id: branch.parent_branch_id,
                base_checkpoint_id: branch.base_checkpoint_id,
                work_node_ids: branch.work_node_ids.clone(),
                completed_work_nodes: branch.completed_work_nodes,
            })
            .collect();
        report.branch_results_reconciled = true;
        report
    }

    fn valid_long_context_report() -> CapabilityRunReport {
        let mut report = common_report("long-context");
        report.context_capacity = 8192;
        report.corpus_estimated_tokens = 46_954;
        report.retrievals = (0..3)
            .map(|i| RetrievalRecord {
                query: format!("query-{i}"),
                branch_id: None,
                candidate_count: 4,
                selected: vec![],
                source: "fixture-search".into(),
            })
            .collect();
        let artifact_id = ArtifactId::new();
        report.source_reads.push(SourceReadRecord {
            branch_id: None,
            source_id: SourceId::new(),
            documents: vec![],
            bytes: 24_000,
            offloaded: true,
            artifact_id: Some(artifact_id),
        });
        report.range_reads.push(RangeReadRecord {
            branch_id: None,
            artifact_id,
            start: 0,
            length: 1024,
            succeeded: true,
            injected_failure: false,
        });
        report.snapshots = ["query-a", "query-b"]
            .into_iter()
            .map(|query| SnapshotRecord {
                id: ContextSnapshotId::new(),
                branch_id: BranchId::new(),
                work_node_id: WorkNodeId::new(),
                query: query.into(),
                candidate_count: 3,
                selected: vec![],
                estimated_prompt_tokens: 2048,
                process_memory_ids: vec![],
                process_memory_branch_ids: vec![],
            })
            .collect();
        report.required_evidence_recovered = [
            "initial_observation".into(),
            "qualification".into(),
            "contradiction".into(),
        ]
        .into();
        report
    }

    #[test]
    fn branching_scenario_requires_real_branches() {
        assert!(check_capability(&valid_branching_report()));
        let mut report = valid_branching_report();
        report.branch_results.clear();
        assert!(!check_capability(&report));
    }

    #[test]
    fn multiple_worknodes_do_not_count_as_branches() {
        let mut report = valid_branching_report();
        report.branch_results.truncate(1);
        assert!(!check_capability(&report));
    }

    #[test]
    fn branch_lineage_is_required() {
        let mut report = valid_branching_report();
        report.branch_results[0].parent_branch_id = None;
        assert!(!check_capability(&report));
    }

    #[test]
    fn branch_lineage_survives_restart() {
        let mut report = valid_branching_report();
        report.persisted_branches[0].parent_branch_id = None;
        assert!(!check_capability(&report));
    }

    #[test]
    fn each_branch_requires_its_own_completed_work() {
        let mut report = valid_branching_report();
        report.branch_results[1].completed_work_nodes = 1;
        assert!(!check_capability(&report));
    }

    #[test]
    fn branch_specific_work_remains_isolated() {
        let mut report = valid_branching_report();
        report.branch_results[1].work_node_ids[0] = report.branch_results[0].work_node_ids[0];
        let unique_work: BTreeSet<_> = report
            .branch_results
            .iter()
            .flat_map(|branch| branch.work_node_ids.iter().copied())
            .collect();
        assert_eq!(unique_work.len(), 3);
        assert!(!check_capability(&report));
    }

    #[test]
    fn branch_reconciliation_reads_both_results() {
        let mut report = valid_branching_report();
        report.branch_results_reconciled = false;
        assert!(!check_capability(&report));
    }

    #[test]
    fn duplicate_branch_side_effect_fails_safety_gate() {
        let mut report = valid_branching_report();
        report.duplicate_consequential_operations = 1;
        assert!(!check_capability(&report));
    }

    #[test]
    fn execution_history_is_not_copied_into_prompts() {
        let mut report = valid_branching_report();
        report.total_execution_history_in_prompts = true;
        assert!(!check_capability(&report));
    }

    #[test]
    fn branch_restart_preserves_completed_work() {
        let mut report = valid_branching_report();
        report.restart_preserved_completed_work = false;
        assert!(!check_capability(&report));
    }

    #[test]
    fn long_context_corpus_exceeds_context_window() {
        let mut report = valid_long_context_report();
        report.corpus_estimated_tokens = report.context_capacity as u64;
        assert!(!check_capability(&report));
    }

    #[test]
    fn long_context_requires_retrieval() {
        let mut report = valid_long_context_report();
        report.retrievals.clear();
        assert!(!check_capability(&report));
    }

    #[test]
    fn long_context_requires_offloaded_artifact() {
        let mut report = valid_long_context_report();
        report.source_reads[0].offloaded = false;
        assert!(!check_capability(&report));
    }

    #[test]
    fn long_context_requires_multiple_distinct_snapshots() {
        let mut report = valid_long_context_report();
        report.snapshots[1].query = report.snapshots[0].query.clone();
        assert!(!check_capability(&report));
    }

    #[test]
    fn long_context_prompt_stays_within_budget() {
        let mut report = valid_long_context_report();
        report
            .prompt_tokens
            .push(report.context_capacity as u64 + 1);
        assert!(!check_capability(&report));
    }

    #[test]
    fn safe_preflight_rejection_is_not_capability_success() {
        let mut report = valid_long_context_report();
        report
            .preflight_failures
            .push("context budget exceeded".into());
        assert!(!check_capability(&report));
    }

    #[test]
    fn artifact_range_read_is_required() {
        let mut report = valid_long_context_report();
        report.range_reads[0].succeeded = false;
        assert!(!check_capability(&report));
    }

    #[test]
    fn long_context_restart_preserves_artifacts_and_work() {
        let mut report = valid_long_context_report();
        report.restart_preserved_completed_work = false;
        assert!(!check_capability(&report));
    }

    #[test]
    fn process_memory_prevents_full_history_growth() {
        let mut report = valid_long_context_report();
        report.process_memory_count = 0;
        assert!(!check_capability(&report));
    }

    #[test]
    fn range_read_must_target_the_offloaded_artifact() {
        let mut report = valid_long_context_report();
        report.range_reads[0].artifact_id = ArtifactId::new();
        assert!(!check_capability(&report));
    }

    #[test]
    fn required_evidence_must_be_retrieved_from_corpus() {
        let mut report = valid_long_context_report();
        report.required_evidence_recovered.remove(2);
        assert!(!check_capability(&report));
    }
}

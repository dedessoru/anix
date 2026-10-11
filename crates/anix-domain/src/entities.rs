use crate::*;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

macro_rules! status {
    ($name:ident: $($value:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($value),+ }
    };
}
status!(ProjectStatus: Active, Archived);
status!(QuestionStatus: Draft, Active, Answered, Archived);
status!(HypothesisStatus: Draft, Proposed, Active, Supported, Rejected, Inconclusive, Archived);
status!(ExperimentStatus: Draft, Proposed, Approved, Ready, Running, Completed, Failed, Cancelled, Archived);
status!(RunStatus: Pending, Running, WaitingForHuman, Completed, Failed, Cancelled);
status!(TaskStatus: Pending, Running, Completed, Failed, Cancelled);
status!(GateStatus: Pending, Approved, Rejected, Expired, Cancelled);
status!(SourceKind: Pdf, Url, Doi, WebPage, LocalFile, Dataset, Book, Manual);
status!(ArtifactKind: Dataset, Csv, Figure, Notebook, Code, Report, Output, Document);
status!(RelationKind: Contains, DerivedFrom, Supports, Contradicts, Qualifies, Addresses, RelatedTo, Tests, Produced, Uses, References, BasedOn, Supersedes);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "identity", rename_all = "snake_case")]
pub enum Actor {
    Human(String),
    LeadAgent(String),
    System,
    Tool(String),
}
impl Actor {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::System => Ok(()),
            Self::Human(v) | Self::LeadAgent(v) | Self::Tool(v) => {
                text(v, "actor identity")?;
                safe_metadata(&Value::String(v.clone()))
            }
        }
    }
    pub fn require_human(&self) -> Result<()> {
        self.validate()?;
        if matches!(self, Self::Human(_)) {
            Ok(())
        } else {
            Err(Error::PermissionDenied)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Meta {
    pub project_id: ProjectId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
    pub archived: bool,
}
impl Meta {
    pub fn new(project_id: ProjectId) -> Self {
        let now = Utc::now();
        Self {
            project_id,
            created_at: now,
            updated_at: now,
            version: 1,
            archived: false,
        }
    }
    pub fn update(&mut self, expected: u64) -> Result<()> {
        if self.version != expected {
            return Err(Error::StaleVersion {
                expected,
                actual: self.version,
            });
        }
        self.version = self
            .version
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or_else(|| Error::Validation("version overflow".into()))?;
        self.updated_at = Utc::now();
        Ok(())
    }
}
macro_rules! entity {
    ($name:ident, $id:ident, {$($field:ident: $ty:ty),* $(,)?}) => {
        #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
        pub struct $name { pub id: $id, #[serde(flatten)] pub meta: Meta, $(pub $field: $ty),* }
    };
}
entity!(Project, ProjectId, {name:String, description:String, status:ProjectStatus});
entity!(ResearchQuestion, QuestionId, {title:String, description:String, status:QuestionStatus});
entity!(Source, SourceId, {title:String, source_kind:SourceKind, locator:String, checksum:Option<String>, metadata:Value});
entity!(Paper, PaperId, {title:String, authors:Vec<String>, publication_year:Option<u16>, doi:Option<String>, abstract_text:String, metadata:Value});
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PaperSource {
    pub project_id: ProjectId,
    pub paper_id: PaperId,
    pub source_id: SourceId,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ClaimLocation {
    pub page: Option<u32>,
    pub section: Option<String>,
    pub paragraph: Option<u32>,
    pub text_span: Option<String>,
    pub offset: Option<u64>,
    pub table: Option<String>,
    pub figure: Option<String>,
}
entity!(Claim, ClaimId, {paper_id:PaperId, source_id:Option<SourceId>, text:String, normalized_text:Option<String>, location:ClaimLocation, created_by:Actor});
entity!(Hypothesis, HypothesisId, {question_id:Option<QuestionId>, title:String, statement:String, status:HypothesisStatus, confidence:Option<f64>});
entity!(Experiment, ExperimentId, {hypothesis_id:HypothesisId, title:String, objective:String, methodology:String, protocol:String, status:ExperimentStatus});
entity!(ExperimentRun, ExperimentRunId, {experiment_id:ExperimentId, run_id:RunId, status:RunStatus, started_at:DateTime<Utc>, finished_at:Option<DateTime<Utc>>, input_artifacts:Vec<ArtifactId>, output_artifacts:Vec<ArtifactId>, metadata:Value});
entity!(Finding, FindingId, {experiment_run_id:ExperimentRunId, title:String, summary:String, confidence:Option<f64>, created_by:Actor});
entity!(Artifact, ArtifactId, {name:String, artifact_kind:ArtifactKind, path:String, content_hash:String, byte_size:u64, metadata:Value});

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum EntityRef {
    Question(QuestionId),
    Source(SourceId),
    Paper(PaperId),
    Claim(ClaimId),
    Hypothesis(HypothesisId),
    Experiment(ExperimentId),
    ExperimentRun(ExperimentRunId),
    Finding(FindingId),
    Artifact(ArtifactId),
}
impl EntityRef {
    pub fn kind(self) -> &'static str {
        match self {
            Self::Question(_) => "question",
            Self::Source(_) => "source",
            Self::Paper(_) => "paper",
            Self::Claim(_) => "claim",
            Self::Hypothesis(_) => "hypothesis",
            Self::Experiment(_) => "experiment",
            Self::ExperimentRun(_) => "experiment_run",
            Self::Finding(_) => "finding",
            Self::Artifact(_) => "artifact",
        }
    }
    pub fn id(self) -> String {
        match self {
            Self::Question(v) => v.to_string(),
            Self::Source(v) => v.to_string(),
            Self::Paper(v) => v.to_string(),
            Self::Claim(v) => v.to_string(),
            Self::Hypothesis(v) => v.to_string(),
            Self::Experiment(v) => v.to_string(),
            Self::ExperimentRun(v) => v.to_string(),
            Self::Finding(v) => v.to_string(),
            Self::Artifact(v) => v.to_string(),
        }
    }
}
entity!(Relation, RelationId, {from_entity:EntityRef,to_entity:EntityRef,kind:RelationKind,confidence:Option<f64>,created_by:Actor});
entity!(Task, TaskId, {title:String,description:String,status:TaskStatus,created_by:Actor});
entity!(Run, RunId, {task_id:TaskId,parent_run_id:Option<RunId>,status:RunStatus,started_at:Option<DateTime<Utc>>,finished_at:Option<DateTime<Utc>>,error:Option<String>,provider_metadata:Value});

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NewHypothesis {
    pub question_id: Option<QuestionId>,
    pub title: String,
    pub statement: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NewExperiment {
    pub hypothesis_id: HypothesisId,
    pub expected_version: u64,
    pub title: String,
    pub objective: String,
    pub methodology: String,
    pub protocol: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NewFinding {
    pub experiment_run_id: ExperimentRunId,
    pub title: String,
    pub summary: String,
    pub confidence: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NewRelation {
    pub from_entity: EntityRef,
    pub to_entity: EntityRef,
    pub kind: RelationKind,
    pub confidence: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ProposedAction {
    ReviseWorkPlan {
        work_node_id: WorkNodeId,
        expected_version: u64,
        expected_revision: PlanRevisionId,
        steps: Vec<WorkPlanStep>,
    },
    ReviseWorkObjective {
        work_node_id: WorkNodeId,
        expected_version: u64,
        objective: String,
    },
    CreateHypothesis(NewHypothesis),
    ReviseHypothesis {
        hypothesis_id: HypothesisId,
        expected_version: u64,
        statement: String,
    },
    ChangeHypothesisStatus {
        hypothesis_id: HypothesisId,
        expected_version: u64,
        status: HypothesisStatus,
    },
    CreateExperiment(NewExperiment),
    ActivateExperiment {
        experiment_id: ExperimentId,
        expected_version: u64,
    },
    CreateFinding(NewFinding),
    CreateRelation(NewRelation),
    ArchiveEntity {
        entity: EntityRef,
        expected_version: u64,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActionEnvelope {
    pub id: ActionId,
    pub schema_version: u32,
    pub action: ProposedAction,
}
impl ActionEnvelope {
    pub fn new(action: ProposedAction) -> Self {
        Self {
            id: ActionId::new(),
            schema_version: 1,
            action,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExpectedVersion {
    pub entity: EntityRef,
    pub version: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum ActionResult {
    WorkNode(WorkNodeId),
    Entity(EntityRef),
    Relation(RelationId),
}
entity!(Gate, GateId, {status:GateStatus,proposed_action:ActionEnvelope,affected_entities:Vec<EntityRef>,expected_versions:Vec<ExpectedVersion>,reason:String,risk:String,created_by:Actor,run_id:Option<RunId>,resolved_at:Option<DateTime<Utc>>,resolved_by:Option<Actor>,result:Option<ActionResult>});
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActionReceipt {
    pub id: ActionId,
    pub project_id: ProjectId,
    pub action: ActionEnvelope,
    pub result: ActionResult,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DomainEvent {
    pub id: EventId,
    pub project_id: ProjectId,
    pub event_type: String,
    pub schema_version: u32,
    pub actor: Actor,
    pub run_id: Option<RunId>,
    pub correlation_id: Option<EventId>,
    pub causation_id: Option<EventId>,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProvenanceRecord {
    pub id: ProvenanceId,
    pub project_id: ProjectId,
    pub actor: Actor,
    pub action: String,
    pub entity: Option<EntityRef>,
    pub run_id: Option<RunId>,
    pub gate_id: Option<GateId>,
    pub reason: String,
    pub source_entities: Vec<EntityRef>,
    pub event_id: EventId,
    pub created_at: DateTime<Utc>,
}

/// Typed local aggregate. Every collection persists in its own relational table.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ProjectState {
    #[serde(default)]
    pub work_nodes: BTreeMap<WorkNodeId, WorkNode>,
    #[serde(default)]
    pub work_edges: BTreeMap<WorkEdgeId, WorkEdge>,
    #[serde(default)]
    pub plan_revisions: BTreeMap<PlanRevisionId, PlanRevision>,
    #[serde(default)]
    pub branches: BTreeMap<BranchId, Branch>,
    #[serde(default)]
    pub checkpoints: BTreeMap<CheckpointId, ExecutionCheckpoint>,
    #[serde(default)]
    pub context_snapshots: BTreeMap<ContextSnapshotId, ContextSnapshot>,
    #[serde(default)]
    pub process_memories: BTreeMap<ProcessMemoryId, ProcessMemory>,
    #[serde(default)]
    pub execution_bindings: BTreeMap<RunId, ExecutionBinding>,
    #[serde(default)]
    pub model_invocations: BTreeMap<ModelInvocationId, ModelInvocation>,
    #[serde(default)]
    pub tool_invocations: BTreeMap<ToolInvocationId, ToolInvocation>,
    #[serde(default)]
    pub candidate_claims: BTreeMap<CandidateClaimId, CandidateClaim>,
    #[serde(default)]
    pub human_control: Option<HumanControlState>,
    pub project: Option<Project>,
    pub questions: BTreeMap<QuestionId, ResearchQuestion>,
    pub sources: BTreeMap<SourceId, Source>,
    pub papers: BTreeMap<PaperId, Paper>,
    pub paper_sources: Vec<PaperSource>,
    pub claims: BTreeMap<ClaimId, Claim>,
    pub hypotheses: BTreeMap<HypothesisId, Hypothesis>,
    pub experiments: BTreeMap<ExperimentId, Experiment>,
    pub experiment_runs: BTreeMap<ExperimentRunId, ExperimentRun>,
    pub findings: BTreeMap<FindingId, Finding>,
    pub artifacts: BTreeMap<ArtifactId, Artifact>,
    pub relations: BTreeMap<RelationId, Relation>,
    pub gates: BTreeMap<GateId, Gate>,
    pub tasks: BTreeMap<TaskId, Task>,
    pub runs: BTreeMap<RunId, Run>,
    pub events: Vec<DomainEvent>,
    pub provenance: Vec<ProvenanceRecord>,
    pub receipts: BTreeMap<ActionId, ActionReceipt>,
}
impl ProjectState {
    pub fn project(&self) -> Result<&Project> {
        self.project
            .as_ref()
            .ok_or_else(|| Error::NotFound("project".into()))
    }
    pub fn meta(&self, entity: EntityRef) -> Result<&Meta> {
        macro_rules! lookup {
            ($map:ident,$id:expr) => {
                self.$map.get(&$id).map(|e| &e.meta)
            };
        }
        let meta = match entity {
            EntityRef::Question(v) => lookup!(questions, v),
            EntityRef::Source(v) => lookup!(sources, v),
            EntityRef::Paper(v) => lookup!(papers, v),
            EntityRef::Claim(v) => lookup!(claims, v),
            EntityRef::Hypothesis(v) => lookup!(hypotheses, v),
            EntityRef::Experiment(v) => lookup!(experiments, v),
            EntityRef::ExperimentRun(v) => lookup!(experiment_runs, v),
            EntityRef::Finding(v) => lookup!(findings, v),
            EntityRef::Artifact(v) => lookup!(artifacts, v),
        };
        meta.ok_or_else(|| Error::NotFound(format!("{entity:?}")))
    }
    pub fn meta_mut(&mut self, entity: EntityRef) -> Result<&mut Meta> {
        macro_rules! lookup {
            ($map:ident,$id:expr) => {
                self.$map.get_mut(&$id).map(|e| &mut e.meta)
            };
        }
        let meta = match entity {
            EntityRef::Question(v) => lookup!(questions, v),
            EntityRef::Source(v) => lookup!(sources, v),
            EntityRef::Paper(v) => lookup!(papers, v),
            EntityRef::Claim(v) => lookup!(claims, v),
            EntityRef::Hypothesis(v) => lookup!(hypotheses, v),
            EntityRef::Experiment(v) => lookup!(experiments, v),
            EntityRef::ExperimentRun(v) => lookup!(experiment_runs, v),
            EntityRef::Finding(v) => lookup!(findings, v),
            EntityRef::Artifact(v) => lookup!(artifacts, v),
        };
        meta.ok_or_else(|| Error::NotFound(format!("{entity:?}")))
    }
    pub fn check(&self, entity: EntityRef) -> Result<()> {
        let meta = self.meta(entity)?;
        if meta.project_id != self.project()?.id {
            return Err(Error::ProjectMismatch);
        }
        if meta.archived {
            return Err(Error::Validation("archived entity".into()));
        }
        Ok(())
    }
}

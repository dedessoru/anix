use anix_core::{CommandContext, Core, NewClaim, NewPaper, NewSource};
use anix_domain::*;
use anix_integrations::Workspace;
use anix_storage::SqliteStorage;
use clap::{Parser, Subcommand};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(name = "anix", version, about = "ANIX development client")]
struct Cli {
    #[arg(long, global = true, default_value = ".")]
    workspace: PathBuf,
    #[arg(long, global = true)]
    project: Option<ProjectId>,
    #[arg(long, global = true, default_value = "cli-user")]
    actor: String,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Process {
        #[command(subcommand)]
        command: ProcessCommand,
    },
    Research {
        #[command(subcommand)]
        command: HumanControlCommand,
    },
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    Eval {
        #[command(subcommand)]
        command: EvalCommand,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Question {
        #[command(subcommand)]
        command: QuestionCommand,
    },
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    Paper {
        #[command(subcommand)]
        command: PaperCommand,
    },
    Claim {
        #[command(subcommand)]
        command: ClaimCommand,
    },
    Hypothesis {
        #[command(subcommand)]
        command: HypothesisCommand,
    },
    Experiment {
        #[command(subcommand)]
        command: ExperimentCommand,
    },
    Finding {
        #[command(subcommand)]
        command: FindingCommand,
    },
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommand,
    },
    Gate {
        #[command(subcommand)]
        command: GateCommand,
    },
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    Run {
        #[command(subcommand)]
        command: RunCommand,
    },
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    Timeline,
    ResearchState,
    Projections,
}

#[derive(Subcommand)]
enum HumanControlCommand {
    Status,
    Pause,
    Resume,
    Cancel,
    Autonomy {
        #[arg(value_parser=parse_enum::<AutonomyMode>)]
        mode: AutonomyMode,
    },
    BudgetShow,
    BudgetSet {
        #[arg(long)]
        max_steps: Option<u64>,
        #[arg(long)]
        max_lead_calls: Option<u64>,
        #[arg(long)]
        max_strong_model_calls: Option<u64>,
        #[arg(long)]
        max_tool_calls: Option<u64>,
        #[arg(long)]
        max_elapsed_seconds: Option<u64>,
    },
    BranchPause {
        id: BranchId,
    },
    BranchResume {
        id: BranchId,
    },
    BranchCancel {
        id: BranchId,
    },
    BranchPrioritize {
        id: BranchId,
        #[arg(value_parser=parse_enum::<Priority>)]
        priority: Priority,
    },
    WorkPrioritize {
        id: WorkNodeId,
        #[arg(value_parser=parse_enum::<Priority>)]
        priority: Priority,
    },
    Explain {
        target: String,
    },
    RequestDeeper {
        #[arg(long)]
        focus: Option<String>,
    },
    RequestContradictions {
        #[arg(long)]
        focus: Option<String>,
    },
    PlanRevision {
        branch: BranchId,
    },
    AddConstraint {
        #[arg(value_parser=parse_enum::<ConstraintKind>)]
        kind: ConstraintKind,
        #[arg(long)]
        text: String,
        #[arg(long)]
        excluded_source: Option<SourceId>,
    },
}

#[derive(Subcommand)]
enum ModelCommand {
    Bootstrap {
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Named manifest profile: micro (default), economy, test-lead, test-lead-4b, minicpm5-q6, minicpm5-q8, or qwen3-micro.
        #[arg(long, conflicts_with = "manifest")]
        profile: Option<String>,
    },
}
#[derive(Subcommand)]
enum EvalCommand {
    Research {
        #[command(subcommand)]
        command: ResearchEvalCommand,
    },
}
#[derive(Subcommand)]
enum ResearchEvalCommand {
    Batch {
        #[arg(long, default_value = "local")]
        profile: String,
        #[arg(long, default_value_t = 10)]
        count: usize,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
    LongHorizon {
        /// Run exactly 10 for the first campaign or 20 for a gated variation campaign.
        #[arg(long, default_value_t = 10)]
        count: usize,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// `initial`, `regression`, or `variation` (variation requires passing regression data).
        #[arg(long, default_value = "initial")]
        stage: String,
    },
    Run {
        scenario: String,
        #[arg(long, default_value = "local")]
        profile: String,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
    Inspect {
        project: ProjectId,
    },
    Bundle {
        project: ProjectId,
        destination: PathBuf,
    },
}
#[derive(Subcommand)]
enum ProcessCommand {
    Branch {
        #[arg(long)]
        name: String,
        #[arg(long)]
        base: Option<CheckpointId>,
    },
    Add {
        #[arg(long)]
        input: PathBuf,
    },
    Edge {
        #[arg(long)]
        from: WorkNodeId,
        #[arg(long)]
        to: WorkNodeId,
        #[arg(long,value_parser=parse_enum::<WorkEdgeKind>)]
        kind: WorkEdgeKind,
    },
    Status {
        #[arg(long)]
        id: WorkNodeId,
        #[arg(long)]
        expected_version: u64,
        #[arg(long,value_parser=parse_enum::<WorkStatus>)]
        status: WorkStatus,
    },
    Bind {
        #[arg(long)]
        run: RunId,
        #[arg(long)]
        work: WorkNodeId,
    },
    Context {
        #[arg(long)]
        work: WorkNodeId,
        #[arg(long)]
        revision: PlanRevisionId,
        #[arg(long,value_parser=parse_entity)]
        pinned: Vec<EntityRef>,
    },
    Checkpoint {
        #[arg(long)]
        input: PathBuf,
    },
    ResumeCheckpoint {
        #[arg(long)]
        id: CheckpointId,
    },
    Resume {
        #[arg(long)]
        branch: BranchId,
    },
    Show,
}
#[derive(Subcommand)]
enum ProjectCommand {
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    List,
}
#[derive(Subcommand)]
enum QuestionCommand {
    Add {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
    },
}
#[derive(Subcommand)]
enum SourceCommand {
    Add {
        #[arg(long)]
        title: String,
        #[arg(long,value_parser=parse_enum::<SourceKind>,default_value="manual")]
        kind: SourceKind,
        #[arg(long)]
        locator: String,
    },
}
#[derive(Subcommand)]
enum PaperCommand {
    Add {
        #[arg(long)]
        title: String,
        #[arg(long)]
        author: Vec<String>,
        #[arg(long)]
        year: Option<u16>,
        #[arg(long)]
        doi: Option<String>,
        #[arg(long, default_value = "")]
        abstract_text: String,
        #[arg(long)]
        source: Vec<SourceId>,
    },
}
#[derive(Subcommand)]
enum ClaimCommand {
    Add {
        #[arg(long)]
        paper: PaperId,
        #[arg(long)]
        source: Option<SourceId>,
        #[arg(long)]
        text: String,
        #[arg(long)]
        page: Option<u32>,
        #[arg(long)]
        section: Option<String>,
    },
}
#[derive(Subcommand)]
enum HypothesisCommand {
    Create {
        #[arg(long)]
        question: Option<QuestionId>,
        #[arg(long)]
        title: String,
        #[arg(long)]
        statement: String,
    },
    Show {
        id: HypothesisId,
    },
    List,
    Revise {
        id: HypothesisId,
        #[arg(long)]
        expected_version: u64,
        #[arg(long)]
        statement: String,
    },
    Status {
        id: HypothesisId,
        #[arg(long)]
        expected_version: u64,
        #[arg(long,value_parser=parse_enum::<HypothesisStatus>)]
        status: HypothesisStatus,
    },
}
#[derive(Subcommand)]
enum ExperimentCommand {
    Create {
        #[arg(long)]
        hypothesis: HypothesisId,
        #[arg(long)]
        expected_version: u64,
        #[arg(long)]
        title: String,
        #[arg(long)]
        objective: String,
        #[arg(long)]
        methodology: String,
        #[arg(long)]
        protocol: String,
    },
    Show {
        id: ExperimentId,
    },
    Activate {
        id: ExperimentId,
        #[arg(long)]
        expected_version: u64,
    },
    Start {
        id: ExperimentId,
        #[arg(long)]
        expected_version: u64,
        #[arg(long)]
        input: Vec<ArtifactId>,
    },
    Complete {
        id: ExperimentRunId,
        #[arg(long)]
        expected_version: u64,
        #[arg(long)]
        output: Vec<ArtifactId>,
    },
}
#[derive(Subcommand)]
enum FindingCommand {
    Add {
        #[arg(long)]
        run: ExperimentRunId,
        #[arg(long)]
        title: String,
        #[arg(long)]
        summary: String,
        #[arg(long)]
        confidence: Option<f64>,
    },
}
#[derive(Subcommand)]
enum GraphCommand {
    Show,
    Add {
        #[arg(long)]
        from_kind: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to_kind: String,
        #[arg(long)]
        to: String,
        #[arg(long,value_parser=parse_enum::<RelationKind>)]
        kind: RelationKind,
    },
}
#[derive(Subcommand)]
enum EvidenceCommand {
    Trace { hypothesis: HypothesisId },
}
#[derive(Subcommand)]
enum GateCommand {
    List {
        #[arg(long)]
        all: bool,
    },
    Show {
        id: GateId,
    },
    Approve {
        id: GateId,
    },
    Reject {
        id: GateId,
    },
    Cancel {
        id: GateId,
    },
    Expire {
        id: GateId,
    },
}
#[derive(Subcommand)]
enum TaskCommand {
    List,
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
    },
}
#[derive(Subcommand)]
enum RunCommand {
    List,
    Show {
        id: RunId,
    },
    Start {
        #[arg(long)]
        task: TaskId,
        #[arg(long)]
        parent: Option<RunId>,
    },
    Complete {
        id: RunId,
    },
    Fail {
        id: RunId,
    },
    Cancel {
        id: RunId,
    },
}
#[derive(Subcommand)]
enum AgentCommand {
    /// Offline deterministic provider for development and demos.
    DemoExperiment {
        #[arg(long)]
        hypothesis: HypothesisId,
        #[arg(long)]
        expected_version: u64,
        #[arg(long)]
        title: String,
        #[arg(long)]
        objective: String,
        #[arg(long)]
        methodology: String,
        #[arg(long)]
        protocol: String,
    },
}

fn parse_enum<T: DeserializeOwned>(s: &str) -> std::result::Result<T, String> {
    serde_json::from_value(json!(s)).map_err(|e| e.to_string())
}
fn entity(kind: String, id: String) -> Result<EntityRef> {
    serde_json::from_value(json!({"type":kind,"id":id}))
        .map_err(|e| Error::Validation(e.to_string()))
}
fn parse_entity(s: &str) -> std::result::Result<EntityRef, String> {
    let (kind, id) = s
        .split_once(':')
        .ok_or_else(|| "expected kind:UUID".to_string())?;
    entity(kind.into(), id.into()).map_err(|e| e.to_string())
}
fn output(value: impl Serialize) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|e| Error::Validation(e.to_string()))?
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Command::Model {
        command:
            ModelCommand::Bootstrap {
                manifest: manifest_path,
                profile,
            },
    } = &cli.command
    {
        let path = manifest_path.clone().unwrap_or_else(|| {
            let name = match profile.as_deref().unwrap_or("micro") {
                "micro" => return cli.workspace.join("models/manifest.toml"),
                "economy" => "economy-0.6b.toml",
                "test-lead" => "qwen3-8b.toml",
                "test-lead-4b" => "test-lead-4b.toml",
                "minicpm5-q6" => "minicpm5-2b-q6.toml",
                "minicpm5-q8" => "minicpm5-2b-q8.toml",
                "qwen3-micro" => "qwen3-1.7b-micro.toml",
                other => panic!("unknown model profile: {other}"),
            };
            cli.workspace.join("models/profiles").join(name)
        });
        let manifest = anix_integrations::ModelManifest::read(&path)?;
        let path = anix_integrations::bootstrap(&cli.workspace, &manifest)?;
        return output(json!({"path":path,"manifest":manifest,"verified":true}));
    }
    let workspace = Workspace::open(&cli.workspace)?;
    let core = Core::new(Arc::new(SqliteStorage::open(workspace.database_path()?)?));
    let context = || CommandContext::human(cli.actor.clone(), "Explicit CLI research decision");
    let project = || {
        cli.project
            .ok_or_else(|| Error::Validation("--project is required for this command".into()))
    };
    match cli.command {
        Command::Research { command } => {
            let p = project()?;
            let controls = core.human_control();
            match command {
                HumanControlCommand::Status => output(core.research().activity(p)?),
                HumanControlCommand::Pause => output(controls.pause(p, None, context())?),
                HumanControlCommand::Resume => output(controls.resume(p, None, context())?),
                HumanControlCommand::Cancel => output(controls.cancel(p, None, context())?),
                HumanControlCommand::Autonomy { mode } => {
                    output(controls.set_autonomy(p, mode, None, context())?)
                }
                HumanControlCommand::BudgetShow => output(controls.state(p)?.budget),
                HumanControlCommand::BudgetSet {
                    max_steps,
                    max_lead_calls,
                    max_strong_model_calls,
                    max_tool_calls,
                    max_elapsed_seconds,
                } => output(controls.budget(
                    p,
                    ResearchBudget {
                        max_steps,
                        max_lead_calls,
                        max_strong_model_calls,
                        max_tool_calls,
                        max_elapsed_seconds,
                    },
                    None,
                    context(),
                )?),
                HumanControlCommand::BranchPause { id } => {
                    output(controls.branch_pause(p, id, true, None, context())?)
                }
                HumanControlCommand::BranchResume { id } => {
                    output(controls.branch_pause(p, id, false, None, context())?)
                }
                HumanControlCommand::BranchCancel { id } => {
                    output(core.cancel_branch(p, id, None, context())?)
                }
                HumanControlCommand::BranchPrioritize { id, priority } => {
                    output(controls.prioritize_branch(p, id, priority, None, context())?)
                }
                HumanControlCommand::WorkPrioritize { id, priority } => {
                    output(core.set_work_priority(p, id, priority, None, context())?)
                }
                HumanControlCommand::Explain { target } => {
                    output(core.research().decision_trace(p, &target)?)
                }
                HumanControlCommand::RequestDeeper { focus } => {
                    output(controls.request_investigation(p, false, focus, None, context())?)
                }
                HumanControlCommand::RequestContradictions { focus } => {
                    output(controls.request_investigation(p, true, focus, None, context())?)
                }
                HumanControlCommand::PlanRevision { branch } => {
                    output(controls.request_plan_revision(p, branch, None, context())?)
                }
                HumanControlCommand::AddConstraint {
                    kind,
                    text,
                    excluded_source,
                } => output(controls.add_constraint(
                    p,
                    text,
                    kind,
                    excluded_source,
                    None,
                    context(),
                )?),
            }
        }
        Command::Process { command } => {
            let p = project()?;
            match command {
                ProcessCommand::Branch { name, base } => {
                    output(core.create_branch(p, name, base, context())?)
                }
                ProcessCommand::Add { input } => {
                    let work = serde_json::from_slice(
                        &std::fs::read(input).map_err(|e| Error::Storage(e.to_string()))?,
                    )
                    .map_err(|e| Error::Validation(e.to_string()))?;
                    output(core.create_work(p, work, context())?)
                }
                ProcessCommand::Edge { from, to, kind } => {
                    output(core.add_work_edge(p, from, to, kind, context())?)
                }
                ProcessCommand::Status {
                    id,
                    expected_version,
                    status,
                } => output(core.transition_work(p, id, expected_version, status, context())?),
                ProcessCommand::Bind { run, work } => {
                    output(core.bind_run(p, run, work, context())?)
                }
                ProcessCommand::Context {
                    work,
                    revision,
                    pinned,
                } => output(core.compile_context(p, work, revision, pinned, context())?),
                ProcessCommand::Checkpoint { input } => {
                    let checkpoint = serde_json::from_slice(
                        &std::fs::read(input).map_err(|e| Error::Storage(e.to_string()))?,
                    )
                    .map_err(|e| Error::Validation(e.to_string()))?;
                    output(core.checkpoint(p, checkpoint, context())?)
                }
                ProcessCommand::ResumeCheckpoint { id } => output(core.resume_checkpoint(p, id)?),
                ProcessCommand::Resume { branch } => output(core.resumable_work(p, branch)?),
                ProcessCommand::Show => {
                    let state = core.runtime_state(p)?;
                    output(
                        json!({"branches":state.branches,"nodes":state.work_nodes,"edges":state.work_edges,"revisions":state.plan_revisions,"checkpoints":state.checkpoints,"bindings":state.execution_bindings,"snapshots":state.context_snapshots,"model_invocations":state.model_invocations,"tool_invocations":state.tool_invocations}),
                    )
                }
            }
        }
        Command::Model { .. } => unreachable!("handled before workspace initialization"),
        Command::Eval {
            command: EvalCommand::Research { command },
        } => {
            let config = anix_evals::HarnessConfig::read(&cli.workspace)?;
            match command {
                ResearchEvalCommand::Batch {
                    profile,
                    count,
                    seed,
                } => {
                    if profile != "local" && profile != "local-test-lead" {
                        return Err(Error::Validation(
                            "research runs require the local-test-lead profile".into(),
                        ));
                    }
                    output(anix_evals::run_batch(&config, count, seed)?)
                }
                ResearchEvalCommand::LongHorizon { count, seed, stage } => output(
                    anix_evals::run_long_horizon_campaign(&config, count, seed, &stage)?,
                ),
                ResearchEvalCommand::Run {
                    scenario,
                    profile,
                    seed,
                } => {
                    if profile != "local" && profile != "local-test-lead" {
                        return Err(Error::Validation(
                            "research runs require the local-test-lead profile".into(),
                        ));
                    }
                    output(anix_evals::run_scenario(&config, &scenario, seed)?)
                }
                ResearchEvalCommand::Inspect { project } => {
                    output(anix_evals::inspect_run(&cli.workspace, project)?)
                }
                ResearchEvalCommand::Bundle {
                    project,
                    destination,
                } => {
                    anix_evals::export_debug_bundle(&cli.workspace, project, &destination)?;
                    output(json!({"exported":true,"destination":destination}))
                }
            }
        }
        Command::Project { command } => match command {
            ProjectCommand::Create { name, description } => {
                output(core.projects().create(name, description, context())?)
            }
            ProjectCommand::List => output(core.projects().list()?),
        },
        Command::Question {
            command: QuestionCommand::Add { title, description },
        } => output(
            core.questions()
                .add(project()?, title, description, context())?,
        ),
        Command::Source {
            command:
                SourceCommand::Add {
                    title,
                    kind,
                    locator,
                },
        } => output(core.sources().add(
            project()?,
            NewSource {
                title,
                source_kind: kind,
                locator,
                checksum: None,
                metadata: json!({"schema_version":1}),
            },
            context(),
        )?),
        Command::Paper {
            command:
                PaperCommand::Add {
                    title,
                    author,
                    year,
                    doi,
                    abstract_text,
                    source,
                },
        } => output(core.papers().add(
            project()?,
            NewPaper {
                title,
                authors: author,
                publication_year: year,
                doi,
                abstract_text,
                source_ids: source,
                metadata: json!({"schema_version":1}),
            },
            context(),
        )?),
        Command::Claim {
            command:
                ClaimCommand::Add {
                    paper,
                    source,
                    text,
                    page,
                    section,
                },
        } => output(core.claims().add(
            project()?,
            NewClaim {
                paper_id: paper,
                source_id: source,
                text,
                location: ClaimLocation {
                    page,
                    section,
                    ..Default::default()
                },
            },
            context(),
        )?),
        Command::Hypothesis { command } => {
            let p = project()?;
            match command {
                HypothesisCommand::Create {
                    question,
                    title,
                    statement,
                } => output(core.hypotheses().create(
                    p,
                    NewHypothesis {
                        question_id: question,
                        title,
                        statement,
                    },
                    context(),
                )?),
                HypothesisCommand::Show { id } => output(core.hypotheses().get(p, id)?),
                HypothesisCommand::List => output(core.hypotheses().list(p)?),
                HypothesisCommand::Revise {
                    id,
                    expected_version,
                    statement,
                } => output(core.execute(
                    p,
                    ActionEnvelope::new(ProposedAction::ReviseHypothesis {
                        hypothesis_id: id,
                        expected_version,
                        statement,
                    }),
                    context(),
                )?),
                HypothesisCommand::Status {
                    id,
                    expected_version,
                    status,
                } => output(core.execute(
                    p,
                    ActionEnvelope::new(ProposedAction::ChangeHypothesisStatus {
                        hypothesis_id: id,
                        expected_version,
                        status,
                    }),
                    context(),
                )?),
            }
        }
        Command::Experiment { command } => {
            let p = project()?;
            match command {
                ExperimentCommand::Create {
                    hypothesis,
                    expected_version,
                    title,
                    objective,
                    methodology,
                    protocol,
                } => output(core.experiments().create(
                    p,
                    NewExperiment {
                        hypothesis_id: hypothesis,
                        expected_version,
                        title,
                        objective,
                        methodology,
                        protocol,
                    },
                    context(),
                )?),
                ExperimentCommand::Show { id } => output(core.experiments().get(p, id)?),
                ExperimentCommand::Activate {
                    id,
                    expected_version,
                } => output(core.execute(
                    p,
                    ActionEnvelope::new(ProposedAction::ActivateExperiment {
                        experiment_id: id,
                        expected_version,
                    }),
                    context(),
                )?),
                ExperimentCommand::Start {
                    id,
                    expected_version,
                    input,
                } => {
                    output(core.start_experiment_run(p, id, expected_version, input, context())?)
                }
                ExperimentCommand::Complete {
                    id,
                    expected_version,
                    output: artifacts,
                } => output(core.finish_experiment_run(
                    p,
                    anix_core::FinishExperimentRun {
                        id,
                        expected_version,
                        status: RunStatus::Completed,
                        outputs: artifacts,
                        metadata: json!({"schema_version":1}),
                    },
                    context(),
                )?),
            }
        }
        Command::Finding {
            command:
                FindingCommand::Add {
                    run,
                    title,
                    summary,
                    confidence,
                },
        } => output(core.findings().add(
            project()?,
            NewFinding {
                experiment_run_id: run,
                title,
                summary,
                confidence,
            },
            context(),
        )?),
        Command::Graph { command } => {
            let p = project()?;
            match command {
                GraphCommand::Show => output(core.graph().list(p, None)?),
                GraphCommand::Add {
                    from_kind,
                    from,
                    to_kind,
                    to,
                    kind,
                } => output(core.graph().create(
                    p,
                    NewRelation {
                        from_entity: entity(from_kind, from)?,
                        to_entity: entity(to_kind, to)?,
                        kind,
                        confidence: None,
                    },
                    context(),
                )?),
            }
        }
        Command::Evidence {
            command: EvidenceCommand::Trace { hypothesis },
        } => output(core.evidence().trace(project()?, hypothesis)?),
        Command::Gate { command } => {
            let p = project()?;
            match command {
                GateCommand::List { all } => output(core.gates().list(p, !all)?),
                GateCommand::Show { id } => output(core.gates().get(p, id)?),
                GateCommand::Approve { id } => output(core.gates().approve(p, id, context())?),
                GateCommand::Reject { id } => output(core.gates().reject(p, id, context())?),
                GateCommand::Cancel { id } => {
                    output(core.resolve_gate(p, id, GateStatus::Cancelled, context())?)
                }
                GateCommand::Expire { id } => {
                    output(core.resolve_gate(p, id, GateStatus::Expired, context())?)
                }
            }
        }
        Command::Task { command } => {
            let p = project()?;
            match command {
                TaskCommand::List => output(core.tasks().list(p)?),
                TaskCommand::Create { title, description } => {
                    output(core.tasks().create(p, title, description, context())?)
                }
            }
        }
        Command::Run { command } => {
            let p = project()?;
            match command {
                RunCommand::List => output(core.runs().list(p)?),
                RunCommand::Show { id } => output(core.runs().get(p, id)?),
                RunCommand::Start { task, parent } => {
                    output(core.start_run(p, task, parent, context())?)
                }
                RunCommand::Complete { id } => {
                    output(core.finish_run(p, id, RunStatus::Completed, context())?)
                }
                RunCommand::Fail { id } => {
                    output(core.finish_run(p, id, RunStatus::Failed, context())?)
                }
                RunCommand::Cancel { id } => {
                    output(core.finish_run(p, id, RunStatus::Cancelled, context())?)
                }
            }
        }
        Command::Agent {
            command:
                AgentCommand::DemoExperiment {
                    hypothesis,
                    expected_version,
                    title,
                    objective,
                    methodology,
                    protocol,
                },
        } => {
            use anix_agent::*;
            let provider = Arc::new(FakeModelProvider::new(vec![ModelResponse {
                schema_version: 1,
                decision: AgentDecision::Propose {
                    action: ProposedAction::CreateExperiment(NewExperiment {
                        hypothesis_id: hypothesis,
                        expected_version,
                        title,
                        objective,
                        methodology,
                        protocol,
                    }),
                    reason: "Offline development experiment proposal".into(),
                },
                metadata: json!({"schema_version":1}),
            }]));
            let selector = Arc::new(FixedModelSelector {
                selection: ModelSelection {
                    provider: "fake".into(),
                    model: "offline".into(),
                },
            });
            let agent = LeadAgent::new(core.agent_api("cli-demo-agent")?, provider, selector);
            output(futures::executor::block_on(agent.run(
                project()?,
                "Propose an experiment".into(),
                Some(EntityRef::Hypothesis(hypothesis)),
                None,
            ))?)
        }
        Command::Timeline => output(core.events().timeline(project()?)?),
        Command::ResearchState => output(core.research().state(project()?)?),
        Command::Projections => {
            let state = core.research().state(project()?)?;
            workspace.rebuild_projections(&state.hypotheses, &state.findings)?;
            output(json!({"rebuilt":true}))
        }
    }
}

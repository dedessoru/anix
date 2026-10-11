use super::*;

// Lightweight service views keep the root API ergonomic without duplicating
// transaction ownership or business logic across clients.
macro_rules! service {
    ($name:ident,$accessor:ident) => {
        pub struct $name<'a> {
            core: &'a Core,
        }
        impl Core {
            pub fn $accessor(&self) -> $name<'_> {
                $name { core: self }
            }
        }
    };
}
service!(ProjectService, projects);
service!(QuestionService, questions);
service!(SourceService, sources);
service!(PaperService, papers);
service!(ClaimService, claims);
service!(HypothesisService, hypotheses);
service!(ExperimentService, experiments);
service!(FindingService, findings);
service!(ArtifactService, artifacts);
service!(GraphService, graph);
service!(EvidenceService, evidence);
service!(GateService, gates);
service!(TaskService, tasks);
service!(RunService, runs);
service!(EventService, events);
service!(ProvenanceService, provenance);
service!(ResearchQueryService, research);
service!(HumanControlService, human_control);
impl HumanControlService<'_> {
    pub fn state(&self, p: ProjectId) -> Result<HumanControlState> {
        self.core.get_human_control(p)
    }
    pub fn set_autonomy(
        &self,
        p: ProjectId,
        mode: AutonomyMode,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<AutonomyPolicy> {
        self.core.set_autonomy(p, mode, expected, ctx)
    }
    pub fn pause(
        &self,
        p: ProjectId,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.core.pause_research(p, expected, ctx)
    }
    pub fn resume(
        &self,
        p: ProjectId,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.core.resume_research(p, expected, ctx)
    }
    pub fn cancel(
        &self,
        p: ProjectId,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.core.cancel_research(p, expected, ctx)
    }
    pub fn budget(
        &self,
        p: ProjectId,
        budget: ResearchBudget,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.core.set_budget(p, budget, expected, ctx)
    }
    pub fn prioritize_branch(
        &self,
        p: ProjectId,
        id: BranchId,
        priority: Priority,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.core
            .set_branch_priority(p, id, priority, expected, ctx)
    }
    pub fn branch_pause(
        &self,
        p: ProjectId,
        id: BranchId,
        paused: bool,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.core.set_branch_paused(p, id, paused, expected, ctx)
    }
    pub fn request_investigation(
        &self,
        p: ProjectId,
        contradiction: bool,
        focus: Option<String>,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.core
            .request_investigation(p, contradiction, focus, expected, ctx)
    }
    pub fn request_plan_revision(
        &self,
        p: ProjectId,
        b: BranchId,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<PlanRevision> {
        self.core.request_plan_revision(p, b, expected, ctx)
    }
    pub fn add_constraint(
        &self,
        p: ProjectId,
        text: String,
        kind: ConstraintKind,
        excluded_source: Option<SourceId>,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.core
            .add_constraint(p, text, kind, excluded_source, expected, ctx)
    }
}
impl ProjectService<'_> {
    pub fn archive(
        &self,
        p: ProjectId,
        expected_version: u64,
        ctx: CommandContext,
    ) -> Result<Project> {
        self.core.archive_project(p, expected_version, ctx)
    }
    pub fn create(
        &self,
        name: String,
        description: String,
        ctx: CommandContext,
    ) -> Result<Project> {
        self.core.create_project(name, description, ctx)
    }
    pub fn list(&self) -> Result<Vec<Project>> {
        self.core.list_projects()
    }
    pub fn get(&self, id: ProjectId) -> Result<Project> {
        self.core.get_project(id)
    }
}
impl QuestionService<'_> {
    pub fn add(
        &self,
        p: ProjectId,
        title: String,
        description: String,
        ctx: CommandContext,
    ) -> Result<ResearchQuestion> {
        self.core.add_question(p, title, description, ctx)
    }
}
impl SourceService<'_> {
    pub fn add(&self, p: ProjectId, v: NewSource, ctx: CommandContext) -> Result<Source> {
        self.core.add_source(p, v, ctx)
    }
}
impl PaperService<'_> {
    pub fn add(&self, p: ProjectId, v: NewPaper, ctx: CommandContext) -> Result<Paper> {
        self.core.add_paper(p, v, ctx)
    }
}
impl ClaimService<'_> {
    pub fn add(&self, p: ProjectId, v: NewClaim, ctx: CommandContext) -> Result<Claim> {
        self.core.add_claim(p, v, ctx)
    }
}
impl HypothesisService<'_> {
    pub fn revise(
        &self,
        p: ProjectId,
        id: HypothesisId,
        expected_version: u64,
        statement: String,
        ctx: CommandContext,
    ) -> Result<ActionResult> {
        self.core.execute(
            p,
            ActionEnvelope::new(ProposedAction::ReviseHypothesis {
                hypothesis_id: id,
                expected_version,
                statement,
            }),
            ctx,
        )
    }
    pub fn change_status(
        &self,
        p: ProjectId,
        id: HypothesisId,
        expected_version: u64,
        status: HypothesisStatus,
        ctx: CommandContext,
    ) -> Result<ActionResult> {
        self.core.execute(
            p,
            ActionEnvelope::new(ProposedAction::ChangeHypothesisStatus {
                hypothesis_id: id,
                expected_version,
                status,
            }),
            ctx,
        )
    }
    pub fn create(
        &self,
        p: ProjectId,
        v: NewHypothesis,
        ctx: CommandContext,
    ) -> Result<ActionResult> {
        self.core.execute(
            p,
            ActionEnvelope::new(ProposedAction::CreateHypothesis(v)),
            ctx,
        )
    }
    pub fn get(&self, p: ProjectId, id: HypothesisId) -> Result<Hypothesis> {
        self.core.get_hypothesis(p, id)
    }
    pub fn list(&self, p: ProjectId) -> Result<Vec<Hypothesis>> {
        self.core.list_hypotheses(p)
    }
}
impl ExperimentService<'_> {
    pub fn activate(
        &self,
        p: ProjectId,
        id: ExperimentId,
        expected_version: u64,
        ctx: CommandContext,
    ) -> Result<ActionResult> {
        self.core.execute(
            p,
            ActionEnvelope::new(ProposedAction::ActivateExperiment {
                experiment_id: id,
                expected_version,
            }),
            ctx,
        )
    }
    pub fn start_run(
        &self,
        p: ProjectId,
        id: ExperimentId,
        expected_version: u64,
        inputs: Vec<ArtifactId>,
        ctx: CommandContext,
    ) -> Result<ExperimentRun> {
        self.core
            .start_experiment_run(p, id, expected_version, inputs, ctx)
    }
    pub fn finish_run(
        &self,
        p: ProjectId,
        input: FinishExperimentRun,
        ctx: CommandContext,
    ) -> Result<ExperimentRun> {
        self.core.finish_experiment_run(p, input, ctx)
    }
    pub fn create(
        &self,
        p: ProjectId,
        v: NewExperiment,
        ctx: CommandContext,
    ) -> Result<ActionResult> {
        self.core.execute(
            p,
            ActionEnvelope::new(ProposedAction::CreateExperiment(v)),
            ctx,
        )
    }
    pub fn get(&self, p: ProjectId, id: ExperimentId) -> Result<Experiment> {
        self.core.get_experiment(p, id)
    }
}
impl FindingService<'_> {
    pub fn add(&self, p: ProjectId, v: NewFinding, ctx: CommandContext) -> Result<ActionResult> {
        self.core.execute(
            p,
            ActionEnvelope::new(ProposedAction::CreateFinding(v)),
            ctx,
        )
    }
}
impl ArtifactService<'_> {
    pub fn register(&self, p: ProjectId, v: NewArtifact, ctx: CommandContext) -> Result<Artifact> {
        self.core.register_artifact(p, v, ctx)
    }
    pub fn list(&self, p: ProjectId) -> Result<Vec<Artifact>> {
        self.core.list_artifacts(p)
    }
}
impl GraphService<'_> {
    pub fn create(
        &self,
        p: ProjectId,
        v: NewRelation,
        ctx: CommandContext,
    ) -> Result<ActionResult> {
        self.core.execute(
            p,
            ActionEnvelope::new(ProposedAction::CreateRelation(v)),
            ctx,
        )
    }
    pub fn list(&self, p: ProjectId, e: Option<EntityRef>) -> Result<Vec<Relation>> {
        self.core.list_relations(p, e)
    }
}
impl EvidenceService<'_> {
    pub fn trace(&self, p: ProjectId, h: HypothesisId) -> Result<EvidenceTrace> {
        self.core.trace_evidence(p, h)
    }
}
impl GateService<'_> {
    pub fn propose(
        &self,
        p: ProjectId,
        action: ActionEnvelope,
        ctx: CommandContext,
        risk: String,
    ) -> Result<Submission> {
        self.core.submit_proposal(p, action, ctx, risk)
    }
    pub fn list(&self, p: ProjectId, pending: bool) -> Result<Vec<Gate>> {
        self.core.list_gates(p, pending)
    }
    pub fn get(&self, p: ProjectId, id: GateId) -> Result<Gate> {
        self.core.get_gate(p, id)
    }
    pub fn approve(&self, p: ProjectId, id: GateId, ctx: CommandContext) -> Result<ActionResult> {
        self.core.approve_gate(p, id, ctx)
    }
    pub fn reject(&self, p: ProjectId, id: GateId, ctx: CommandContext) -> Result<Gate> {
        self.core.resolve_gate(p, id, GateStatus::Rejected, ctx)
    }
}
impl TaskService<'_> {
    pub fn list(&self, p: ProjectId) -> Result<Vec<Task>> {
        self.core.list_tasks(p)
    }
    pub fn create(
        &self,
        p: ProjectId,
        title: String,
        description: String,
        ctx: CommandContext,
    ) -> Result<Task> {
        self.core.create_task(p, title, description, ctx)
    }
}
impl RunService<'_> {
    pub fn start(
        &self,
        p: ProjectId,
        task: TaskId,
        parent: Option<RunId>,
        ctx: CommandContext,
    ) -> Result<Run> {
        self.core.start_run(p, task, parent, ctx)
    }
    pub fn finish(
        &self,
        p: ProjectId,
        id: RunId,
        status: RunStatus,
        ctx: CommandContext,
    ) -> Result<Run> {
        self.core.finish_run(p, id, status, ctx)
    }
    pub fn list(&self, p: ProjectId) -> Result<Vec<Run>> {
        self.core.list_runs(p)
    }
    pub fn get(&self, p: ProjectId, id: RunId) -> Result<Run> {
        self.core.get_run(p, id)
    }
}
impl EventService<'_> {
    pub fn timeline(&self, p: ProjectId) -> Result<Vec<DomainEvent>> {
        self.core.get_project_timeline(p)
    }
}
impl ProvenanceService<'_> {
    pub fn list(&self, p: ProjectId, e: Option<EntityRef>) -> Result<Vec<ProvenanceRecord>> {
        self.core.get_provenance(p, e)
    }
}
impl ResearchQueryService<'_> {
    pub fn state(&self, p: ProjectId) -> Result<ResearchState> {
        self.core.get_research_state(p)
    }
    pub fn context(&self, p: ProjectId, e: Option<EntityRef>) -> Result<ResearchContext> {
        self.core.build_context(p, e)
    }
    pub fn activity(&self, p: ProjectId) -> Result<CurrentResearchActivity> {
        self.core.current_activity(p)
    }
    pub fn decision_trace(&self, p: ProjectId, target: &str) -> Result<DecisionTrace> {
        self.core.decision_trace(p, target)
    }
}

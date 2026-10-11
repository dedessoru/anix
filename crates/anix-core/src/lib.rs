mod actions;
mod human_control;
mod queries;
mod runtime;
mod services;
mod tools;
pub use anix_domain as domain;
use anix_domain::*;
use chrono::Utc;
use human_control::{
    authorize_project_human, mark_waiting_for_gate, policy_requires_gate, refresh_gate_status,
    require_agent_progress,
};
pub use queries::*;
pub use runtime::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub use services::*;
use std::sync::{
    mpsc::{sync_channel, Receiver, SyncSender},
    Arc, Mutex,
};
pub use tools::*;

/// A transaction calls its callback once, atomically persists the resulting
/// state, and returns only events committed by that transaction.
/// Adapters must also give snapshot-consistent reads and serialize writers.
pub trait Storage: Send + Sync {
    fn read(&self, project: ProjectId) -> Result<ProjectState>;
    fn list_projects(&self) -> Result<Vec<Project>>;
    fn search(&self, project: ProjectId, query: &str, limit: u32) -> Result<Vec<EntityRef>> {
        let state = self.read(project)?;
        let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        Ok(state
            .claims
            .values()
            .filter(|c| terms.iter().any(|t| c.text.to_lowercase().contains(t)))
            .take(limit as usize)
            .map(|c| EntityRef::Claim(c.id))
            .collect())
    }
    fn transaction(
        &self,
        project: ProjectId,
        operation: &mut dyn FnMut(&mut ProjectState) -> Result<()>,
    ) -> Result<Vec<DomainEvent>>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommandContext {
    pub actor: Actor,
    pub reason: String,
    pub run_id: Option<RunId>,
    pub correlation_id: Option<EventId>,
    pub causation_id: Option<EventId>,
}
impl CommandContext {
    pub fn human(identity: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            actor: Actor::Human(identity.into()),
            reason: reason.into(),
            run_id: None,
            correlation_id: None,
            causation_id: None,
        }
    }
    fn validate(&self, state: &ProjectState) -> Result<()> {
        self.actor.validate()?;
        if matches!(self.actor, Actor::Human(_)) {
            // Direct scientific mutations must not bypass the creator check
            // already enforced by Gate resolution and Human controls.
            authorize_project_human(state, &self.actor)?;
        }
        text(&self.reason, "reason")?;
        safe_metadata(&json!({"reason":self.reason}))?;
        if let Some(id) = self.run_id {
            if !state.runs.contains_key(&id) {
                return Err(Error::NotFound("run".into()));
            }
        }
        for id in [self.correlation_id, self.causation_id]
            .into_iter()
            .flatten()
        {
            if !state.events.iter().any(|e| e.id == id) {
                return Err(Error::NotFound("correlation/causation event".into()));
            }
        }
        Ok(())
    }
}

pub struct Core {
    storage: Arc<dyn Storage>,
    subscribers: Mutex<Vec<SyncSender<DomainEvent>>>,
    mutation: Mutex<()>,
}
impl Core {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            subscribers: Mutex::new(Vec::new()),
            mutation: Mutex::new(()),
        }
    }
    /// Slow subscribers are disconnected when their 256-event buffer fills.
    /// Recover missed events through the persisted project timeline.
    pub fn subscribe(&self) -> Result<Receiver<DomainEvent>> {
        let (tx, rx) = sync_channel(256);
        self.subscribers
            .lock()
            .map_err(|_| Error::Storage("event bus lock".into()))?
            .push(tx);
        Ok(rx)
    }
    fn mutate<T>(
        &self,
        project: ProjectId,
        operation: impl FnOnce(&mut ProjectState) -> Result<T>,
    ) -> Result<T> {
        let _guard = self
            .mutation
            .lock()
            .map_err(|_| Error::Storage("mutation lock".into()))?;
        let mut operation = Some(operation);
        let mut result = None;
        let events = self.storage.transaction(project, &mut |state| {
            let op = operation
                .take()
                .ok_or_else(|| Error::Storage("storage invoked callback twice".into()))?;
            result = Some(op(state)?);
            Ok(())
        })?;
        // The adapter has committed before returning. Poisoning the subscription
        // mutex must not turn an already committed mutation into an error.
        let mut subscribers = self.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        for event in events {
            subscribers.retain(|tx| tx.try_send(event.clone()).is_ok());
        }
        result.ok_or_else(|| Error::Storage("storage skipped transaction callback".into()))
    }
    pub(crate) fn state(&self, project: ProjectId) -> Result<ProjectState> {
        let state = self.storage.read(project)?;
        state.project()?;
        Ok(state)
    }
    pub fn create_project(
        &self,
        name: String,
        description: String,
        context: CommandContext,
    ) -> Result<Project> {
        context.actor.require_human()?;
        text(&name, "project name")?;
        text(&context.reason, "reason")?;
        let id = ProjectId::new();
        self.mutate(id, |state| {
            if state.project.is_some() {
                return Err(Error::Conflict("project exists".into()));
            }
            let project = Project {
                id,
                meta: Meta::new(id),
                name,
                description,
                status: ProjectStatus::Active,
            };
            state.project = Some(project.clone());
            context.validate(state)?;
            audit(
                state,
                &context,
                "ProjectCreated",
                None,
                None,
                vec![],
                json!({"project_id":id}),
            )?;
            Ok(project)
        })
    }
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        self.storage.list_projects()
    }
    pub fn get_project(&self, id: ProjectId) -> Result<Project> {
        self.state(id)?
            .project
            .ok_or_else(|| Error::NotFound("project".into()))
    }
    pub fn add_question(
        &self,
        project: ProjectId,
        title: String,
        description: String,
        context: CommandContext,
    ) -> Result<ResearchQuestion> {
        context.actor.require_human()?;
        text(&title, "question title")?;
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            let entity = ResearchQuestion {
                id: QuestionId::new(),
                meta: Meta::new(project),
                title,
                description,
                status: QuestionStatus::Active,
            };
            s.questions.insert(entity.id, entity.clone());
            audit(
                s,
                &context,
                "QuestionCreated",
                Some(EntityRef::Question(entity.id)),
                None,
                vec![],
                json!({"question_id":entity.id}),
            )?;
            Ok(entity)
        })
    }
    pub fn add_source(
        &self,
        project: ProjectId,
        input: NewSource,
        context: CommandContext,
    ) -> Result<Source> {
        context.actor.require_human()?;
        text(&input.title, "source title")?;
        text(&input.locator, "source locator")?;
        safe_metadata(&input.metadata)?;
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            let entity = Source {
                id: SourceId::new(),
                meta: Meta::new(project),
                title: input.title,
                source_kind: input.source_kind,
                locator: input.locator,
                checksum: input.checksum,
                metadata: input.metadata,
            };
            s.sources.insert(entity.id, entity.clone());
            audit(
                s,
                &context,
                "SourceAdded",
                Some(EntityRef::Source(entity.id)),
                None,
                vec![],
                json!({"source_id":entity.id}),
            )?;
            Ok(entity)
        })
    }
    pub fn add_paper(
        &self,
        project: ProjectId,
        input: NewPaper,
        context: CommandContext,
    ) -> Result<Paper> {
        context.actor.require_human()?;
        text(&input.title, "paper title")?;
        safe_metadata(&input.metadata)?;
        if input
            .publication_year
            .is_some_and(|y| !(1..=9999).contains(&y))
        {
            return Err(Error::Validation("publication year".into()));
        }
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            for id in &input.source_ids {
                actions::check_entity_policy(s, EntityRef::Source(*id))?;
            }
            let entity = Paper {
                id: PaperId::new(),
                meta: Meta::new(project),
                title: input.title,
                authors: input.authors,
                publication_year: input.publication_year,
                doi: input.doi,
                abstract_text: input.abstract_text,
                metadata: input.metadata,
            };
            for source_id in input.source_ids {
                let link = PaperSource {
                    project_id: project,
                    paper_id: entity.id,
                    source_id,
                };
                if !s.paper_sources.contains(&link) {
                    s.paper_sources.push(link);
                }
            }
            s.papers.insert(entity.id, entity.clone());
            audit(
                s,
                &context,
                "PaperAdded",
                Some(EntityRef::Paper(entity.id)),
                None,
                s.paper_sources
                    .iter()
                    .filter(|p| p.paper_id == entity.id)
                    .map(|p| EntityRef::Source(p.source_id))
                    .collect(),
                json!({"paper_id":entity.id}),
            )?;
            Ok(entity)
        })
    }
    pub fn add_claim(
        &self,
        project: ProjectId,
        input: NewClaim,
        context: CommandContext,
    ) -> Result<Claim> {
        context.actor.require_human()?;
        text(&input.text, "claim text")?;
        if input.location.page == Some(0) {
            return Err(Error::Validation("page is one-based".into()));
        }
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            s.check(EntityRef::Paper(input.paper_id))?;
            if let Some(source_id) = input.source_id {
                actions::check_entity_policy(s, EntityRef::Source(source_id))?;
                if !s
                    .paper_sources
                    .iter()
                    .any(|p| p.paper_id == input.paper_id && p.source_id == source_id)
                {
                    return Err(Error::Validation("source is not attached to paper".into()));
                }
            }
            let entity = Claim {
                id: ClaimId::new(),
                meta: Meta::new(project),
                paper_id: input.paper_id,
                source_id: input.source_id,
                text: input.text,
                normalized_text: None,
                location: input.location,
                created_by: context.actor.clone(),
            };
            s.claims.insert(entity.id, entity.clone());
            let mut sources = vec![EntityRef::Paper(entity.paper_id)];
            if let Some(id) = entity.source_id {
                sources.push(EntityRef::Source(id));
            }
            audit(
                s,
                &context,
                "ClaimCreated",
                Some(EntityRef::Claim(entity.id)),
                None,
                sources,
                json!({"claim_id":entity.id}),
            )?;
            Ok(entity)
        })
    }
    pub fn register_artifact(
        &self,
        project: ProjectId,
        input: NewArtifact,
        context: CommandContext,
    ) -> Result<Artifact> {
        context.actor.require_human()?;
        self.persist_artifact(project, input, context, false)
    }
    /// Runtime output storage does not authorize scientific state changes.
    pub fn register_execution_artifact(
        &self,
        project: ProjectId,
        input: NewArtifact,
        context: CommandContext,
    ) -> Result<Artifact> {
        if !matches!(context.actor, Actor::Tool(_) | Actor::System)
            || !matches!(
                input.artifact_kind,
                ArtifactKind::Output | ArtifactKind::Document
            )
        {
            return Err(Error::PermissionDenied);
        }
        self.persist_artifact(project, input, context, true)
    }
    fn persist_artifact(
        &self,
        project: ProjectId,
        input: NewArtifact,
        context: CommandContext,
        execution: bool,
    ) -> Result<Artifact> {
        text(&input.name, "artifact name")?;
        safe_metadata(&input.metadata)?;
        if input.path.is_empty()
            || input.path.contains('\\')
            || input.path.contains(':')
            || input.path.starts_with('/')
            || input
                .path
                .split('/')
                .any(|v| v == ".." || v == "." || v.is_empty())
        {
            return Err(Error::Validation(
                "artifact path must be relative and normalized".into(),
            ));
        }
        if input.content_hash.len() != 64
            || !input.content_hash.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(Error::Validation("SHA256 content hash".into()));
        }
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            if execution {
                let run = context.run_id.ok_or(Error::PermissionDenied)?;
                if !s.execution_bindings.contains_key(&run)
                    || s.runs
                        .get(&run)
                        .is_none_or(|r| r.status != RunStatus::Running)
                {
                    return Err(Error::PermissionDenied);
                }
            }
            if s.artifacts
                .values()
                .any(|a| a.path == input.path && !a.meta.archived)
            {
                return Err(Error::Conflict("artifact path already registered".into()));
            }
            let entity = Artifact {
                id: ArtifactId::new(),
                meta: Meta::new(project),
                name: input.name,
                artifact_kind: input.artifact_kind,
                path: input.path,
                content_hash: input.content_hash,
                byte_size: input.byte_size,
                metadata: input.metadata,
            };
            s.artifacts.insert(entity.id, entity.clone());
            audit(
                s,
                &context,
                "ArtifactCreated",
                Some(EntityRef::Artifact(entity.id)),
                None,
                vec![],
                json!({"artifact_id":entity.id}),
            )?;
            Ok(entity)
        })
    }
    /// Direct commands require a human actor. Agents only receive AgentApi.
    pub fn execute(
        &self,
        project: ProjectId,
        envelope: ActionEnvelope,
        context: CommandContext,
    ) -> Result<ActionResult> {
        context.actor.require_human()?;
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            execute_action(s, envelope, &context, None)
        })
    }
    pub fn submit_proposal(
        &self,
        project: ProjectId,
        envelope: ActionEnvelope,
        context: CommandContext,
        risk: String,
    ) -> Result<Submission> {
        text(&risk, "risk summary")?;
        safe_metadata(&json!({"risk":risk}))?;
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            require_agent_progress(s, &context.actor)?;
            if let ProposedAction::ReviseWorkObjective { work_node_id, .. } | ProposedAction::ReviseWorkPlan { work_node_id, .. } = &envelope.action {
                if let Actor::LeadAgent(identity) = &context.actor {
                    let run = context
                        .run_id
                        .and_then(|id| s.runs.get(&id))
                        .ok_or(Error::PermissionDenied)?;
                    if run.status != RunStatus::Running
                        || s.tasks[&run.task_id].created_by != Actor::LeadAgent(identity.clone())
                        || s.execution_bindings
                            .get(&run.id)
                            .is_none_or(|binding| binding.work_node_id != *work_node_id)
                    {
                        return Err(Error::PermissionDenied);
                    }
                }
            }
            if let Some(receipt) = s.receipts.get(&envelope.id) {
                if receipt.action != envelope {
                    return Err(Error::Conflict("action ID reused".into()));
                }
                return Ok(Submission::Applied(receipt.result.clone()));
            }
            if let Some(gate) = s
                .gates
                .values()
                .find(|g| g.proposed_action.id == envelope.id)
            {
                if gate.proposed_action != envelope {
                    return Err(Error::Conflict("action ID reused".into()));
                }
                return Ok(Submission::Gate(gate.id));
            }
            validate_action(s, &envelope)?;
            if let ProposedAction::ReviseWorkPlan {work_node_id,steps,..}=&envelope.action {
                if actions::work_plan_is_unchanged(s,*work_node_id,steps) {
                    let result=ActionResult::WorkNode(*work_node_id);
                    audit(s,&context,"PlanRevisionNoChange",None,None,vec![],json!({"work_node_id":work_node_id,"action_id":envelope.id,"existing_work_graph":true}))?;
                    s.receipts.insert(envelope.id,ActionReceipt {id:envelope.id,project_id:project,action:envelope,result:result.clone()});
                    return Ok(Submission::Applied(result));
                }
            }
            if let ProposedAction::ReviseWorkObjective { work_node_id, objective, .. } = &envelope.action {
                let normalized=|value:&str| value.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" ");
                if normalized(&s.work_nodes[work_node_id].objective)==normalized(objective) {
                    let result=ActionResult::WorkNode(*work_node_id);
                    audit(s,&context,"PlanRevisionNoChange",None,None,vec![],json!({"work_node_id":work_node_id,"action_id":envelope.id,"existing_plan":true}))?;
                    s.receipts.insert(envelope.id,ActionReceipt {id:envelope.id,project_id:project,action:envelope,result:result.clone()});
                    return Ok(Submission::Applied(result));
                }
            }
            if !policy_requires_gate(s, &envelope.action) {
                return Ok(Submission::Applied(execute_action(
                    s, envelope, &context, None,
                )?));
            }
            let affected = action_dependencies(&envelope.action);
            let expected = affected
                .iter()
                .map(|e| {
                    Ok(ExpectedVersion {
                        entity: *e,
                        version: s.meta(*e)?.version,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let gate = Gate {
                id: GateId::new(),
                meta: Meta::new(project),
                status: GateStatus::Pending,
                proposed_action: envelope,
                affected_entities: affected.clone(),
                expected_versions: expected,
                reason: context.reason.clone(),
                risk,
                created_by: context.actor.clone(),
                run_id: context.run_id,
                resolved_at: None,
                resolved_by: None,
                result: None,
            };
            s.gates.insert(gate.id, gate.clone());
            mark_waiting_for_gate(s);
            audit(
                s,
                &context,
                "GateCreated",
                None,
                Some(gate.id),
                affected,
                json!({"gate_id":gate.id,"action_id":gate.proposed_action.id}),
            )?;
            if let Some(run_id) = context.run_id {
                let run = s
                    .runs
                    .get_mut(&run_id)
                    .ok_or_else(|| Error::NotFound("run".into()))?;
                if run.status == RunStatus::Running {
                    run.meta.update(run.meta.version)?;
                    run.status = RunStatus::WaitingForHuman;
                    audit(
                        s,
                        &context,
                        "RunWaitingForHuman",
                        None,
                        Some(gate.id),
                        vec![],
                        json!({"run_id":run_id}),
                    )?;
                }
            }
            Ok(Submission::Gate(gate.id))
        })
    }
    pub fn approve_gate(
        &self,
        project: ProjectId,
        id: GateId,
        context: CommandContext,
    ) -> Result<ActionResult> {
        context.actor.require_human()?;
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            authorize_project_human(s, &context.actor)?;
            let gate = s
                .gates
                .get(&id)
                .cloned()
                .ok_or_else(|| Error::NotFound("gate".into()))?;
            if gate.status == GateStatus::Approved {
                return gate
                    .result
                    .ok_or_else(|| Error::Storage("approved gate lacks result".into()));
            }
            gate.status.resolve(GateStatus::Approved)?;
            for expected in &gate.expected_versions {
                if s.meta(expected.entity)?.version != expected.version {
                    return Err(Error::StaleProposal(expected.entity));
                }
            }
            let mut approved_context = context.clone();
            approved_context.run_id = gate.run_id;
            let result =
                execute_action(s, gate.proposed_action.clone(), &approved_context, Some(id))?;
            let current = s
                .gates
                .get_mut(&id)
                .ok_or_else(|| Error::NotFound("gate".into()))?;
            current.meta.update(current.meta.version)?;
            current.status = GateStatus::Approved;
            current.resolved_at = Some(Utc::now());
            current.resolved_by = Some(context.actor.clone());
            current.result = Some(result.clone());
            audit(
                s,
                &approved_context,
                "GateApproved",
                result_entity(&result),
                Some(id),
                gate.affected_entities,
                json!({"gate_id":id,"result":result}),
            )?;
            resolve_waiting_run(s, &approved_context, id)?;
            refresh_gate_status(s);
            Ok(result)
        })
    }
    pub fn resolve_gate(
        &self,
        project: ProjectId,
        id: GateId,
        status: GateStatus,
        context: CommandContext,
    ) -> Result<Gate> {
        context.actor.require_human()?;
        if !matches!(
            status,
            GateStatus::Rejected | GateStatus::Expired | GateStatus::Cancelled
        ) {
            return Err(Error::Validation("use approve_gate for approval".into()));
        }
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            authorize_project_human(s, &context.actor)?;
            let g = s
                .gates
                .get_mut(&id)
                .ok_or_else(|| Error::NotFound("gate".into()))?;
            if g.status == status {
                return Ok(g.clone());
            }
            g.status.resolve(status)?;
            g.meta.update(g.meta.version)?;
            g.status = status;
            g.resolved_at = Some(Utc::now());
            g.resolved_by = Some(context.actor.clone());
            let gate = g.clone();
            let mut ctx = context.clone();
            ctx.run_id = gate.run_id;
            audit(
                s,
                &ctx,
                match status {
                    GateStatus::Rejected => "GateRejected",
                    GateStatus::Expired => "GateExpired",
                    _ => "GateCancelled",
                },
                None,
                Some(id),
                gate.affected_entities.clone(),
                json!({"gate_id":id,"proposed_action":gate.proposed_action,"human_reason":context.reason,"rejected":status==GateStatus::Rejected}),
            )?;
            resolve_waiting_run(s, &ctx, id)?;
            refresh_gate_status(s);
            Ok(gate)
        })
    }
    pub fn create_task(
        &self,
        project: ProjectId,
        title: String,
        description: String,
        context: CommandContext,
    ) -> Result<Task> {
        text(&title, "task title")?;
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            require_agent_progress(s, &context.actor)?;
            let task = Task {
                id: TaskId::new(),
                meta: Meta::new(project),
                title,
                description,
                status: TaskStatus::Pending,
                created_by: context.actor.clone(),
            };
            s.tasks.insert(task.id, task.clone());
            audit(
                s,
                &context,
                "TaskCreated",
                None,
                None,
                vec![],
                json!({"task_id":task.id}),
            )?;
            Ok(task)
        })
    }
    pub fn start_run(
        &self,
        project: ProjectId,
        task_id: TaskId,
        parent_run_id: Option<RunId>,
        context: CommandContext,
    ) -> Result<Run> {
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            require_agent_progress(s, &context.actor)?;
            if let Some(id) = parent_run_id {
                let parent = s
                    .runs
                    .get(&id)
                    .ok_or_else(|| Error::NotFound("parent run".into()))?;
                if parent.status.terminal() {
                    return Err(Error::InvalidTransition("parent run is terminal".into()));
                }
            }
            let task = s
                .tasks
                .get_mut(&task_id)
                .ok_or_else(|| Error::NotFound("task".into()))?;
            if matches!(task.status, TaskStatus::Completed | TaskStatus::Cancelled) {
                return Err(Error::InvalidTransition("task is terminal".into()));
            }
            if s.runs
                .values()
                .any(|r| r.task_id == task_id && !r.status.terminal())
            {
                return Err(Error::Conflict("task already has active run".into()));
            }
            task.meta.update(task.meta.version)?;
            task.status = TaskStatus::Running;
            let run = Run {
                id: RunId::new(),
                meta: Meta::new(project),
                task_id,
                parent_run_id,
                status: RunStatus::Running,
                started_at: Some(Utc::now()),
                finished_at: None,
                error: None,
                provider_metadata: json!({"schema_version":1}),
            };
            s.runs.insert(run.id, run.clone());
            let mut ctx = context.clone();
            ctx.run_id = Some(run.id);
            audit(
                s,
                &ctx,
                "RunStarted",
                None,
                None,
                vec![],
                json!({"run_id":run.id,"task_id":task_id}),
            )?;
            Ok(run)
        })
    }
    pub fn finish_run(
        &self,
        project: ProjectId,
        id: RunId,
        status: RunStatus,
        context: CommandContext,
    ) -> Result<Run> {
        if !status.terminal() {
            return Err(Error::Validation("terminal status required".into()));
        }
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            if s.gates
                .values()
                .any(|g| g.run_id == Some(id) && g.status == GateStatus::Pending)
            {
                return Err(Error::GateRequired);
            }
            if s.experiment_runs
                .values()
                .any(|e| e.run_id == id && !e.status.terminal())
            {
                return Err(Error::InvalidTransition(
                    "finish the experiment run through ExperimentService".into(),
                ));
            }
            if s.runs
                .values()
                .any(|r| r.parent_run_id == Some(id) && !r.status.terminal())
            {
                return Err(Error::Conflict(
                    "active child runs must finish first".into(),
                ));
            }
            let run = s
                .runs
                .get_mut(&id)
                .ok_or_else(|| Error::NotFound("run".into()))?;
            if run.status == status {
                return Ok(run.clone());
            }
            run.status.transition(status)?;
            run.meta.update(run.meta.version)?;
            run.status = status;
            run.finished_at = Some(Utc::now());
            let output = run.clone();
            let task = s
                .tasks
                .get_mut(&output.task_id)
                .ok_or_else(|| Error::NotFound("task".into()))?;
            task.meta.update(task.meta.version)?;
            task.status = match status {
                RunStatus::Completed => TaskStatus::Completed,
                RunStatus::Cancelled => TaskStatus::Cancelled,
                _ => TaskStatus::Failed,
            };
            let mut ctx = context.clone();
            ctx.run_id = Some(id);
            audit(
                s,
                &ctx,
                match status {
                    RunStatus::Completed => "RunCompleted",
                    RunStatus::Failed => "RunFailed",
                    _ => "RunCancelled",
                },
                None,
                None,
                vec![],
                json!({"run_id":id}),
            )?;
            audit(
                s,
                &ctx,
                "TaskStatusChanged",
                None,
                None,
                vec![],
                json!({"task_id":output.task_id,"status":status}),
            )?;
            Ok(output)
        })
    }
    pub fn start_experiment_run(
        &self,
        project: ProjectId,
        experiment_id: ExperimentId,
        expected_version: u64,
        input_artifacts: Vec<ArtifactId>,
        context: CommandContext,
    ) -> Result<ExperimentRun> {
        context.actor.require_human()?;
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            if context
                .run_id
                .is_some_and(|id| s.runs.get(&id).is_some_and(|r| r.status.terminal()))
            {
                return Err(Error::InvalidTransition("parent run is terminal".into()));
            }
            for id in &input_artifacts {
                s.check(EntityRef::Artifact(*id))?;
            }
            s.check(EntityRef::Experiment(experiment_id))?;
            let experiment = s
                .experiments
                .get_mut(&experiment_id)
                .ok_or_else(|| Error::NotFound("experiment".into()))?;
            if experiment.status != ExperimentStatus::Ready {
                return Err(Error::GateRequired);
            }
            experiment.status.transition(ExperimentStatus::Running)?;
            experiment.meta.update(expected_version)?;
            experiment.status = ExperimentStatus::Running;
            let task = Task {
                id: TaskId::new(),
                meta: Meta::new(project),
                title: format!("Execute experiment {experiment_id}"),
                description: String::new(),
                status: TaskStatus::Running,
                created_by: context.actor.clone(),
            };
            s.tasks.insert(task.id, task.clone());
            let run = Run {
                id: RunId::new(),
                meta: Meta::new(project),
                task_id: task.id,
                parent_run_id: context.run_id,
                status: RunStatus::Running,
                started_at: Some(Utc::now()),
                finished_at: None,
                error: None,
                provider_metadata: json!({"schema_version":1}),
            };
            s.runs.insert(run.id, run.clone());
            let attempt = ExperimentRun {
                id: ExperimentRunId::new(),
                meta: Meta::new(project),
                experiment_id,
                run_id: run.id,
                status: RunStatus::Running,
                started_at: Utc::now(),
                finished_at: None,
                input_artifacts,
                output_artifacts: vec![],
                metadata: json!({"schema_version":1}),
            };
            s.experiment_runs.insert(attempt.id, attempt.clone());
            let mut ctx = context.clone();
            ctx.run_id = Some(run.id);
            audit(
                s,
                &ctx,
                "TaskCreated",
                None,
                None,
                vec![],
                json!({"task_id":task.id}),
            )?;
            audit(
                s,
                &ctx,
                "RunStarted",
                None,
                None,
                vec![],
                json!({"run_id":run.id}),
            )?;
            audit(
                s,
                &ctx,
                "ExperimentStarted",
                Some(EntityRef::Experiment(experiment_id)),
                None,
                vec![],
                json!({"experiment_run_id":attempt.id}),
            )?;
            audit(
                s,
                &ctx,
                "ExperimentRunStarted",
                Some(EntityRef::ExperimentRun(attempt.id)),
                None,
                vec![EntityRef::Experiment(experiment_id)],
                json!({"experiment_run_id":attempt.id}),
            )?;
            Ok(attempt)
        })
    }
    pub fn finish_experiment_run(
        &self,
        project: ProjectId,
        input: FinishExperimentRun,
        context: CommandContext,
    ) -> Result<ExperimentRun> {
        let FinishExperimentRun {
            id,
            expected_version,
            status,
            outputs,
            metadata,
        } = input;
        context.actor.require_human()?;
        safe_metadata(&metadata)?;
        if !status.terminal() {
            return Err(Error::Validation("terminal status required".into()));
        }
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            for output in &outputs {
                s.check(EntityRef::Artifact(*output))?;
            }
            let attempt = s
                .experiment_runs
                .get_mut(&id)
                .ok_or_else(|| Error::NotFound("experiment run".into()))?;
            if attempt.status == status {
                if attempt.output_artifacts == outputs && attempt.metadata == metadata {
                    return Ok(attempt.clone());
                }
                return Err(Error::Conflict(
                    "completed experiment output differs".into(),
                ));
            }
            attempt.status.transition(status)?;
            attempt.meta.update(expected_version)?;
            attempt.status = status;
            attempt.finished_at = Some(Utc::now());
            attempt.output_artifacts = outputs;
            attempt.metadata = metadata;
            let result = attempt.clone();
            let experiment = s
                .experiments
                .get_mut(&result.experiment_id)
                .ok_or_else(|| Error::NotFound("experiment".into()))?;
            let next = match status {
                RunStatus::Completed => ExperimentStatus::Completed,
                RunStatus::Failed => ExperimentStatus::Failed,
                _ => ExperimentStatus::Cancelled,
            };
            experiment.status.transition(next)?;
            experiment.meta.update(experiment.meta.version)?;
            experiment.status = next;
            let run = s
                .runs
                .get_mut(&result.run_id)
                .ok_or_else(|| Error::NotFound("run".into()))?;
            run.status.transition(status)?;
            run.meta.update(run.meta.version)?;
            run.status = status;
            run.finished_at = Some(Utc::now());
            let task_id = run.task_id;
            let task = s
                .tasks
                .get_mut(&task_id)
                .ok_or_else(|| Error::NotFound("task".into()))?;
            task.meta.update(task.meta.version)?;
            task.status = match status {
                RunStatus::Completed => TaskStatus::Completed,
                RunStatus::Failed => TaskStatus::Failed,
                _ => TaskStatus::Cancelled,
            };
            let mut ctx = context.clone();
            ctx.run_id = Some(result.run_id);
            audit(
                s,
                &ctx,
                "ExperimentRunFinished",
                Some(EntityRef::ExperimentRun(id)),
                None,
                vec![],
                json!({"status":status}),
            )?;
            audit(
                s,
                &ctx,
                "ExperimentStatusChanged",
                Some(EntityRef::Experiment(result.experiment_id)),
                None,
                vec![EntityRef::ExperimentRun(id)],
                json!({"status":next}),
            )?;
            audit(
                s,
                &ctx,
                "RunFinished",
                None,
                None,
                vec![],
                json!({"run_id":result.run_id,"status":status}),
            )?;
            audit(
                s,
                &ctx,
                "TaskStatusChanged",
                None,
                None,
                vec![],
                json!({"task_id":task_id,"status":status}),
            )?;
            Ok(result)
        })
    }
    pub fn record_run_metadata(
        &self,
        project: ProjectId,
        id: RunId,
        metadata: Value,
        context: CommandContext,
    ) -> Result<Run> {
        safe_metadata(&metadata)?;
        if metadata.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(Error::Validation(
                "provider metadata schema version must be 1".into(),
            ));
        }
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            let current = s
                .runs
                .get(&id)
                .ok_or_else(|| Error::NotFound("run".into()))?;
            if current.status != RunStatus::Running {
                return Err(Error::InvalidTransition(
                    "provider metadata requires a running run".into(),
                ));
            }
            if !matches!(context.actor, Actor::Human(_))
                && s.tasks.get(&current.task_id).map(|t| &t.created_by) != Some(&context.actor)
            {
                return Err(Error::PermissionDenied);
            }
            let run = s
                .runs
                .get_mut(&id)
                .ok_or_else(|| Error::NotFound("run".into()))?;
            run.meta.update(run.meta.version)?;
            run.provider_metadata = metadata;
            let result = run.clone();
            let mut ctx = context.clone();
            ctx.run_id = Some(id);
            audit(
                s,
                &ctx,
                "RunMetadataRecorded",
                None,
                None,
                vec![],
                json!({"run_id":id}),
            )?;
            Ok(result)
        })
    }
    pub fn archive_project(
        &self,
        project: ProjectId,
        expected_version: u64,
        context: CommandContext,
    ) -> Result<Project> {
        context.actor.require_human()?;
        self.mutate(project, |s| {
            active(s)?;
            context.validate(s)?;
            if s.runs.values().any(|r| !r.status.terminal())
                || s.gates.values().any(|g| g.status == GateStatus::Pending)
            {
                return Err(Error::Conflict(
                    "resolve pending gates and active runs before archiving".into(),
                ));
            }
            let p = s
                .project
                .as_mut()
                .ok_or_else(|| Error::NotFound("project".into()))?;
            p.meta.update(expected_version)?;
            p.meta.archived = true;
            p.status = ProjectStatus::Archived;
            let result = p.clone();
            audit(
                s,
                &context,
                "ProjectArchived",
                None,
                None,
                vec![],
                json!({"project_id":project}),
            )?;
            Ok(result)
        })
    }
    pub fn agent_api(&self, identity: impl Into<String>) -> Result<AgentApi<'_>> {
        let identity = identity.into();
        text(&identity, "agent identity")?;
        Ok(AgentApi {
            core: self,
            identity,
        })
    }
}

fn active(s: &ProjectState) -> Result<()> {
    if s.project()?.status != ProjectStatus::Active {
        return Err(Error::Validation("project is archived".into()));
    }
    Ok(())
}
fn audit(
    s: &mut ProjectState,
    ctx: &CommandContext,
    action: &str,
    entity: Option<EntityRef>,
    gate_id: Option<GateId>,
    source_entities: Vec<EntityRef>,
    payload: Value,
) -> Result<()> {
    ctx.actor.validate()?;
    safe_metadata(&payload)?;
    safe_metadata(&json!({"reason":ctx.reason}))?;
    let event = DomainEvent {
        id: EventId::new(),
        project_id: s.project()?.id,
        event_type: action.into(),
        schema_version: 1,
        actor: ctx.actor.clone(),
        run_id: ctx.run_id,
        correlation_id: ctx
            .correlation_id
            .or_else(|| {
                s.events
                    .iter()
                    .find(|e| e.run_id == ctx.run_id)
                    .map(|e| e.id)
            })
            .or_else(|| s.events.first().map(|e| e.id)),
        causation_id: ctx
            .causation_id
            .or_else(|| {
                s.events
                    .iter()
                    .rev()
                    .find(|e| e.run_id == ctx.run_id)
                    .map(|e| e.id)
            })
            .or_else(|| s.events.last().map(|e| e.id)),
        occurred_at: Utc::now(),
        payload,
    };
    s.provenance.push(ProvenanceRecord {
        id: ProvenanceId::new(),
        project_id: event.project_id,
        actor: ctx.actor.clone(),
        action: action.into(),
        entity,
        run_id: ctx.run_id,
        gate_id,
        reason: ctx.reason.clone(),
        source_entities,
        event_id: event.id,
        created_at: event.occurred_at,
    });
    s.events.push(event);
    let boundary = match action {
        "ToolInvocationStarted" => Some(CheckpointBoundary::BeforeSideEffect),
        "ToolInvocationCompleted" | "ToolInvocationFailed" => {
            Some(CheckpointBoundary::AfterSideEffect)
        }
        "GateCreated" => Some(CheckpointBoundary::BeforeGate),
        "GateApproved" | "GateRejected" | "GateCancelled" | "GateExpired" => {
            Some(CheckpointBoundary::AfterGate)
        }
        _ => None,
    };
    if let (Some(boundary), Some(binding)) = (
        boundary,
        ctx.run_id
            .and_then(|id| s.execution_bindings.get(&id))
            .cloned(),
    ) {
        let checkpoint = ExecutionCheckpoint {
            id: CheckpointId::new(),
            meta: Meta::new(s.project()?.id),
            run_id: Some(binding.id),
            work_node_id: Some(binding.work_node_id),
            branch_id: binding.branch_id,
            plan_revision_id: binding.plan_revision_id,
            context_snapshot_id: s
                .model_invocations
                .values()
                .filter(|i| i.run_id == binding.id)
                .max_by_key(|i| i.started_at)
                .map(|i| i.context_snapshot_id)
                .or_else(|| {
                    s.context_snapshots
                        .values()
                        .filter(|c| {
                            c.work_node_id == binding.work_node_id
                                && c.plan_revision_id == binding.plan_revision_id
                        })
                        .max_by_key(|c| c.meta.created_at)
                        .map(|c| c.id)
                }),
            boundary,
            execution_state: json!({"schema_version":1,"gate_id":gate_id}),
        };
        s.checkpoints.insert(checkpoint.id, checkpoint.clone());
        audit(
            s,
            ctx,
            "checkpoint.created",
            None,
            gate_id,
            vec![],
            json!({"checkpoint":checkpoint.id,"boundary":boundary}),
        )?;
    }
    Ok(())
}
fn resolve_waiting_run(s: &mut ProjectState, ctx: &CommandContext, gate: GateId) -> Result<()> {
    if let Some(id) = ctx.run_id {
        if !s
            .gates
            .values()
            .any(|g| g.run_id == Some(id) && g.status == GateStatus::Pending)
            && !s
                .runs
                .values()
                .any(|r| r.parent_run_id == Some(id) && !r.status.terminal())
        {
            let resume_work = s.execution_bindings.get(&id).is_some_and(|binding| {
                s.work_nodes.get(&binding.work_node_id).is_some_and(|work| {
                    matches!(work.status, WorkStatus::Ready | WorkStatus::Running)
                })
            });
            let run = s
                .runs
                .get_mut(&id)
                .ok_or_else(|| Error::NotFound("run".into()))?;
            if run.status == RunStatus::WaitingForHuman {
                run.meta.update(run.meta.version)?;
                run.status = if resume_work {
                    RunStatus::Running
                } else {
                    RunStatus::Completed
                };
                run.finished_at = if resume_work { None } else { Some(Utc::now()) };
                let task_id = run.task_id;
                let task = s
                    .tasks
                    .get_mut(&task_id)
                    .ok_or_else(|| Error::NotFound("task".into()))?;
                task.meta.update(task.meta.version)?;
                task.status = if resume_work {
                    TaskStatus::Running
                } else {
                    TaskStatus::Completed
                };
                audit(
                    s,
                    ctx,
                    if resume_work {
                        "RunResumed"
                    } else {
                        "RunCompleted"
                    },
                    None,
                    Some(gate),
                    vec![],
                    json!({"run_id":id}),
                )?;
                audit(
                    s,
                    ctx,
                    if resume_work {
                        "TaskResumed"
                    } else {
                        "TaskCompleted"
                    },
                    None,
                    Some(gate),
                    vec![],
                    json!({"task_id":task_id}),
                )?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewSource {
    pub title: String,
    pub source_kind: SourceKind,
    pub locator: String,
    pub checksum: Option<String>,
    pub metadata: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewPaper {
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<u16>,
    pub doi: Option<String>,
    pub abstract_text: String,
    pub source_ids: Vec<SourceId>,
    pub metadata: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewClaim {
    pub paper_id: PaperId,
    pub source_id: Option<SourceId>,
    pub text: String,
    pub location: ClaimLocation,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewArtifact {
    pub name: String,
    pub artifact_kind: ArtifactKind,
    pub path: String,
    pub content_hash: String,
    pub byte_size: u64,
    pub metadata: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FinishExperimentRun {
    pub id: ExperimentRunId,
    pub expected_version: u64,
    pub status: RunStatus,
    pub outputs: Vec<ArtifactId>,
    pub metadata: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Submission {
    Gate(GateId),
    Applied(ActionResult),
}

/// Restricted capability handed to anix-agent. It exposes no Core, storage, or
/// human-decision handle. The actor identity is fixed by its constructor.
///
/// ```compile_fail
/// use anix_core::{AgentApi, CommandContext};
/// use anix_core::domain::{ProjectId, GateId};
/// fn bypass(api: AgentApi<'_>, p: ProjectId, g: GateId) {
///     api.approve_gate(p, g, CommandContext::human("forged", "approve"));
/// }
/// ```
///
/// ```compile_fail
/// fn database(api: anix_core::AgentApi<'_>) { let _ = api.core.storage; }
/// ```
///
/// ```compile_fail
/// use anix_core::AgentApi;
/// use anix_core::domain::{ProjectId, ResearchBudget};
/// fn extend_budget(api: AgentApi<'_>, project: ProjectId) {
///     api.set_budget(
///         project,
///         ResearchBudget { max_strong_model_calls: Some(100), ..Default::default() },
///         None,
///     );
/// }
/// ```
pub struct AgentApi<'a> {
    core: &'a Core,
    identity: String,
}
fn usable_required_output(state: &ProjectState, invocation: &ToolInvocation) -> bool {
    invocation.status == InvocationStatus::Succeeded
        && invocation.output_artifacts.iter().any(|id| {
            state.artifacts.get(id).is_some_and(|artifact| {
                let metadata = artifact
                    .metadata
                    .get("tool_output_metadata")
                    .unwrap_or(&artifact.metadata["schema"]["tool_output_metadata"]);
                metadata["reference_valid"] != false
                    && metadata["stall_detected"] != true
                    && metadata["repeated_without_progress"] != true
            })
        })
}
impl AgentApi<'_> {
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn retrieval_batches(
        &self,
        project: ProjectId,
        anchor_run: Option<RunId>,
        work_node: WorkNodeId,
        branch: BranchId,
        tools: &RetrievalToolNames,
    ) -> Result<Vec<RetrievalBatchState>> {
        let state = self.core.runtime_state(project)?;
        let mut batches = derive_retrieval_batches(
            &state,
            RetrievalScope {
                anchor_run_id: anchor_run,
                work_node_id: work_node,
                branch_id: branch,
            },
            tools,
        )?;
        for batch in &mut batches {
            batch.returned.retain(|source| {
                actions::check_entity_policy(&state, EntityRef::Source(source.source_id)).is_ok()
            });
        }
        Ok(batches)
    }
    /// Checks the same source exclusion policy used for canonical mutations.
    pub fn validate_source_access(&self, project: ProjectId, source: SourceId) -> Result<()> {
        actions::check_entity_policy(
            &self.core.runtime_state(project)?,
            EntityRef::Source(source),
        )
    }
    /// Sources explicitly supplied by the current work contract can be read
    /// without searching. These handles are scoped to that work, not a batch.
    pub fn work_source_handles(
        &self,
        project: ProjectId,
        work: WorkNodeId,
    ) -> Result<Vec<(RetrievedSourceHandle, String)>> {
        let state = self.core.runtime_state(project)?;
        let node = state
            .work_nodes
            .get(&work)
            .ok_or_else(|| Error::NotFound("work".into()))?;
        let mut ids = std::collections::BTreeSet::new();
        for reference in &node.acceptance_criteria.required_entities {
            if actions::check_entity_policy(&state, *reference).is_err() {
                continue;
            }
            match reference {
                EntityRef::Source(id) => {
                    ids.insert(*id);
                }
                EntityRef::Paper(id) => {
                    ids.extend(
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
        ids.retain(|id| actions::check_entity_policy(&state, EntityRef::Source(*id)).is_ok());
        Ok(ids
            .into_iter()
            .enumerate()
            .map(|(index, id)| {
                (
                    RetrievedSourceHandle {
                        handle: format!("E{}", index + 1),
                        source_id: id,
                    },
                    state.sources[&id].title.chars().take(160).collect(),
                )
            })
            .collect())
    }
    pub fn block_work(
        &self,
        project: ProjectId,
        work: WorkNodeId,
        reason: String,
    ) -> Result<WorkNode> {
        let state = self.core.runtime_state(project)?;
        let node = state
            .work_nodes
            .get(&work)
            .ok_or_else(|| Error::NotFound("work".into()))?;
        self.core.transition_work(
            project,
            work,
            node.meta.version,
            WorkStatus::Blocked,
            self.context(reason, None),
        )
    }
    pub fn record_provider(&self, project: ProjectId, run: RunId, metadata: Value) -> Result<Run> {
        self.core.record_run_metadata(
            project,
            run,
            metadata,
            self.context("Record provider usage metadata".into(), Some(run)),
        )
    }
    fn context(&self, reason: String, run_id: Option<RunId>) -> CommandContext {
        CommandContext {
            actor: Actor::LeadAgent(self.identity.clone()),
            reason,
            run_id,
            correlation_id: None,
            causation_id: None,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn start_model_invocation(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        snapshot: ContextSnapshotId,
        provider: String,
        model: String,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<ModelInvocationId> {
        let record = ModelInvocation {
            id: ModelInvocationId::new(),
            meta: Meta::new(project),
            run_id: run,
            work_node_id: work,
            context_snapshot_id: snapshot,
            provider,
            model,
            model_revision: "configured-local-profile".into(),
            runtime: "model-provider".into(),
            instruction_version: "anix-lead-step-v2".into(),
            started_at,
            finished_at: None,
            latency_ms: None,
            status: InvocationStatus::Running,
            usage: TokenUsage::default(),
            metadata: serde_json::json!({"phase":"provider_call_start","route":"strong"}),
            output_schema: serde_json::json!({"schema_version":1}),
        };
        let record = self.core.record_model_invocation(
            project,
            record,
            self.context("Start bounded lead decision invocation".into(), Some(run)),
        )?;
        Ok(record.id)
    }
    pub fn finish_model_invocation(
        &self,
        project: ProjectId,
        id: ModelInvocationId,
        latency_ms: u64,
        usage: TokenUsage,
        metadata: Value,
        succeeded: bool,
    ) -> Result<ModelInvocation> {
        let mut record = self
            .core
            .runtime_state(project)?
            .model_invocations
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::NotFound("model invocation".into()))?;
        record.finished_at = Some(chrono::Utc::now());
        record.latency_ms = Some(latency_ms);
        record.usage = usage;
        record.metadata = metadata;
        let metadata = record
            .metadata
            .as_object_mut()
            .ok_or_else(|| Error::Validation("provider metadata must be an object".into()))?;
        metadata.insert("route".into(), json!("strong"));
        if let Some(schema) = record.metadata["diagnostic_trace"]["action_schema"].as_object() {
            record.output_schema = serde_json::Value::Object(schema.clone());
        }
        record.status = if succeeded {
            InvocationStatus::Succeeded
        } else {
            InvocationStatus::Failed
        };
        self.core.record_model_invocation(
            project,
            record.clone(),
            self.context(
                "Finish bounded lead decision invocation".into(),
                Some(record.run_id),
            ),
        )
    }
    pub fn lead_progress_snapshot(&self, project: ProjectId, work: WorkNodeId) -> Result<Value> {
        let state = self.core.runtime_state(project)?;
        let active = state
            .work_nodes
            .get(&work)
            .ok_or(Error::NotFound("work".into()))?;
        Ok(serde_json::json!({
            "work_status":active.status,
            "succeeded_work_nodes":state.work_nodes.values().filter(|node| node.status == WorkStatus::Succeeded).count(),
            "branches":state.branches.len(),
            "successful_source_reads":state.tool_invocations.values().filter(|call| call.tool == "read_source" && usable_required_output(&state, call)).count(),
            "findings":state.findings.len(),
            "claims":state.claims.len(),
        }))
    }
    pub fn update_decision_trace_outcome(
        &self,
        project: ProjectId,
        id: ModelInvocationId,
        outcome: Value,
    ) -> Result<ModelInvocation> {
        let record = self
            .core
            .runtime_state(project)?
            .model_invocations
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::NotFound("model invocation".into()))?;
        if record.metadata["diagnostic_trace"].is_object() {
            self.core.update_model_invocation_trace(
                project,
                id,
                outcome,
                self.context(
                    "Record diagnostic Lead decision outcome".into(),
                    Some(record.run_id),
                ),
            )
        } else {
            Ok(record)
        }
    }
    pub fn context_data(
        &self,
        project: ProjectId,
        focus: Option<EntityRef>,
    ) -> Result<ResearchContext> {
        self.core.build_context(project, focus)
    }
    pub fn work_node(&self, project: ProjectId, work: WorkNodeId) -> Result<WorkNode> {
        self.core
            .runtime_state(project)?
            .work_nodes
            .get(&work)
            .cloned()
            .ok_or_else(|| Error::NotFound("work".into()))
    }
    pub fn research_branch_scopes(
        &self,
        project: ProjectId,
        work: WorkNodeId,
    ) -> Result<Vec<ResearchBranchScope>> {
        runtime::branch_scopes(&self.core.runtime_state(project)?, work)
    }
    pub fn select_research_branch(
        &self,
        project: ProjectId,
        run: RunId,
        current: WorkNodeId,
        target: WorkNodeId,
        expected_version: u64,
    ) -> Result<WorkNode> {
        let ctx = self.context(
            "Lead selects an existing independent research scope".into(),
            Some(run),
        );
        self.core.mutate(project,|s| {
            ctx.validate(s)?;require_agent_progress(s,&ctx.actor)?;
            let execution=s.runs.get(&run).ok_or(Error::PermissionDenied)?;
            if execution.status!=RunStatus::Running||s.tasks[&execution.task_id].created_by!=ctx.actor||s.execution_bindings.get(&run).is_none_or(|binding|binding.work_node_id!=current) {return Err(Error::PermissionDenied);}
            let scope=runtime::branch_scopes(s,current)?.into_iter().find(|scope|scope.work_node_id==target).ok_or(Error::PermissionDenied)?;
            if scope.active||scope.paused||!matches!(scope.status,WorkStatus::Proposed|WorkStatus::Ready|WorkStatus::Running) {return Err(Error::InvalidTransition("research branch scope is not executable".into()));}
            if scope.version!=expected_version {return Err(Error::StaleVersion{expected:expected_version,actual:scope.version});}
            let node=s.work_nodes[&target].clone();
            audit(s,&ctx,"ResearchBranchSelected",None,None,vec![],json!({"source_work_node_id":current,"target_work_node_id":target,"branch_id":node.branch_id}))?;
            Ok(node)
        })
    }
    pub fn create_task(&self, project: ProjectId, goal: String) -> Result<Task> {
        self.core.create_task(
            project,
            goal.clone(),
            goal.clone(),
            self.context(goal, None),
        )
    }
    pub fn start_run(
        &self,
        project: ProjectId,
        task: TaskId,
        parent: Option<RunId>,
    ) -> Result<Run> {
        let s = self.core.state(project)?;
        if s.tasks.get(&task).map(|t| &t.created_by)
            != Some(&Actor::LeadAgent(self.identity.clone()))
        {
            return Err(Error::PermissionDenied);
        }
        self.core.start_run(
            project,
            task,
            parent,
            self.context("Lead agent execution".into(), None),
        )
    }
    pub fn bind_run(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
    ) -> Result<ExecutionBinding> {
        let s = self.core.state(project)?;
        let r = s
            .runs
            .get(&run)
            .ok_or_else(|| Error::NotFound("run".into()))?;
        let t = s
            .tasks
            .get(&r.task_id)
            .ok_or_else(|| Error::NotFound("task".into()))?;
        if t.created_by != Actor::LeadAgent(self.identity.clone()) {
            return Err(Error::PermissionDenied);
        }
        self.core.bind_run(
            project,
            run,
            work,
            self.context("Bind lead decision to WorkNode".into(), Some(run)),
        )
    }
    pub fn start_work(&self, project: ProjectId, work: WorkNodeId) -> Result<WorkNode> {
        let mut node = self.work_node(project, work)?;
        if node.status == WorkStatus::Proposed {
            node = self.core.transition_work(
                project,
                work,
                node.meta.version,
                WorkStatus::Ready,
                self.context("Resolve executable WorkNode prerequisites".into(), None),
            )?;
        }
        if node.status == WorkStatus::Ready {
            self.core.transition_work(
                project,
                work,
                node.meta.version,
                WorkStatus::Running,
                self.context("Lead starts bounded WorkNode execution".into(), None),
            )
        } else if node.status == WorkStatus::Running {
            Ok(node)
        } else {
            Err(Error::InvalidTransition(format!(
                "WorkNode is not executable from {:?}",
                node.status
            )))
        }
    }
    pub fn compile_context(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        pinned: Vec<EntityRef>,
    ) -> Result<(ContextSnapshot, ProviderContext)> {
        let state = self.core.runtime_state(project)?;
        let binding = state
            .execution_bindings
            .get(&run)
            .ok_or_else(|| Error::NotFound("execution binding".into()))?;
        if binding.work_node_id != work {
            return Err(Error::PermissionDenied);
        }
        let snapshot = self.core.compile_context(
            project,
            work,
            binding.plan_revision_id,
            pinned,
            self.context("Compile bounded lead context".into(), Some(run)),
        )?;
        let provider_context = self.core.provider_context(project, snapshot.id)?;
        Ok((snapshot, provider_context))
    }
    pub fn create_child_work(
        &self,
        project: ProjectId,
        run: RunId,
        parent: WorkNodeId,
        title: String,
        objective: String,
        required_tool: String,
    ) -> Result<WorkNode> {
        self.create_child_work_with_repeat(
            project,
            run,
            parent,
            title,
            objective,
            required_tool,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn create_child_work_with_repeat(
        &self,
        project: ProjectId,
        run: RunId,
        parent: WorkNodeId,
        title: String,
        objective: String,
        required_tool: String,
        repeat_reason: Option<String>,
    ) -> Result<WorkNode> {
        let state = self.core.runtime_state(project)?;
        self.context("Validate bounded planning scope".into(), Some(run))
            .validate(&state)?;
        if state.runs.get(&run).is_none_or(|run| {
            run.status != RunStatus::Running
                || state
                    .tasks
                    .get(&run.task_id)
                    .is_none_or(|task| task.created_by != Actor::LeadAgent(self.identity.clone()))
        }) {
            return Err(Error::PermissionDenied);
        }
        if let Some(reason) = &repeat_reason {
            text(reason, "replication reason")?;
            if reason.len() > 1000 {
                return Err(Error::Validation("replication reason too long".into()));
            }
        }
        let binding = state
            .execution_bindings
            .get(&run)
            .ok_or_else(|| Error::NotFound("execution binding".into()))?;
        if binding.work_node_id != parent
            || state
                .work_nodes
                .get(&parent)
                .is_none_or(|n| n.granularity != WorkGranularity::Composite)
        {
            return Err(Error::PermissionDenied);
        }
        text(&required_tool, "required tool")?;
        let parent_node = &state.work_nodes[&parent];
        let normalized = |value: &str| {
            value
                .split_whitespace()
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
                .join(" ")
        };
        if let Some(existing) = state
            .work_edges
            .values()
            .filter(|edge| edge.from == parent && edge.kind == WorkEdgeKind::Contains)
            .filter_map(|edge| state.work_nodes.get(&edge.to))
            .find(|node| {
                repeat_reason.is_none()
                    && node.branch_id == parent_node.branch_id
                    && normalized(&node.objective) == normalized(&objective)
                    && node.capability_requirements == vec![format!("tool:{required_tool}")]
                    && !matches!(node.status, WorkStatus::Cancelled | WorkStatus::Superseded)
            })
        {
            return Ok(existing.clone());
        }
        let child = self.core.create_work(
            project,
            NewWork {
                branch_id: parent_node.branch_id,
                kind: "long-horizon-step".into(),
                granularity: WorkGranularity::Atomic,
                title,
                objective,
                acceptance_criteria: OutputContract {
                    required_entities: parent_node.acceptance_criteria.required_entities.clone(),
                    required_artifacts: parent_node.acceptance_criteria.required_artifacts.clone(),
                    minimum_children: 0,
                },
                capability_requirements: vec![format!("tool:{required_tool}")],
                context_policy_id: "long-horizon-bounded-v1".into(),
                budget: WorkBudget {
                    context_tokens: parent_node.budget.context_tokens,
                    max_model_calls: parent_node.budget.max_model_calls,
                    max_tool_calls: parent_node.budget.max_tool_calls,
                },
            },
            self.context(
                repeat_reason
                    .map(|reason| format!("Lead plans justified replication: {reason}"))
                    .unwrap_or_else(|| "Lead plans bounded atomic WorkNode".into()),
                Some(run),
            ),
        )?;
        self.core.add_work_edge(
            project,
            parent,
            child.id,
            WorkEdgeKind::Contains,
            self.context("Attach planned child WorkNode".into(), Some(run)),
        )?;
        self.core.transition_work(
            project,
            child.id,
            child.meta.version,
            WorkStatus::Ready,
            self.context(
                "Make approved bounded child WorkNode ready".into(),
                Some(run),
            ),
        )
    }
    pub fn complete_work(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
    ) -> Result<WorkNode> {
        self.validate_work_completion(project, run, work)?;
        let state = self.core.runtime_state(project)?;
        let node = state
            .work_nodes
            .get(&work)
            .ok_or_else(|| Error::NotFound("work".into()))?;
        self.core.transition_work(
            project,
            work,
            node.meta.version,
            WorkStatus::Succeeded,
            self.context(
                "Lead completes WorkNode after tool-output validation".into(),
                Some(run),
            ),
        )
    }
    pub fn finish_research(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        outcome: ResearchOutcome,
        summary: String,
    ) -> Result<WorkNode> {
        self.finish_research_with_branch_reviews(project, run, work, outcome, summary, vec![])
    }
    pub fn finish_research_with_branch_reviews(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        outcome: ResearchOutcome,
        summary: String,
        reviewed_branches: Vec<WorkNodeId>,
    ) -> Result<WorkNode> {
        text(&summary, "research termination summary")?;
        if summary.len() > 4000 {
            return Err(Error::Validation("termination summary too long".into()));
        }
        let ctx = self.context("Explicit Lead research termination".into(), Some(run));
        self.core.mutate(project, |s| {
            ctx.validate(s)?;
            require_agent_progress(s, &ctx.actor)?;
            let current_run = s.runs.get(&run).ok_or(Error::PermissionDenied)?;
            if current_run.status != RunStatus::Running || s.tasks[&current_run.task_id].created_by != ctx.actor
                || s.execution_bindings.get(&run).is_none_or(|binding| binding.work_node_id != work) { return Err(Error::PermissionDenied); }
            let node = s.work_nodes.get(&work).ok_or(Error::PermissionDenied)?;
            if node.granularity != WorkGranularity::Composite || node.status != WorkStatus::Running { return Err(Error::InvalidTransition("only running composite research can terminate".into())); }
            let readiness = runtime::decision_state(s, work)?;
            let required_branches:std::collections::BTreeSet<_>=runtime::branch_scopes(s,work)?.into_iter().filter(|scope|scope.parent_work_node_id==Some(work)).map(|scope|scope.work_node_id).collect();
            let reviewed:std::collections::BTreeSet<_>=reviewed_branches.iter().copied().collect();
            if reviewed.len()!=reviewed_branches.len()||reviewed.iter().any(|id|!required_branches.contains(id)) {return Err(Error::PermissionDenied);}
            if outcome!=ResearchOutcome::Blocked&&reviewed!=required_branches {return Err(Error::InvalidTransition("independent branch results require explicit Lead review".into()));}
            for id in &reviewed {
                if !s.events.iter().any(|event|event.event_type=="BranchResultAvailable"&&event.payload["source_work_node_id"]==json!(id)&&event.payload["target_work_node_id"]==json!(work)) {return Err(Error::InvalidTransition("branch result has not been returned to this research scope".into()));}
            }
            let evidence_present = readiness["evidence_stages"]["read_outputs"].as_u64().unwrap_or(0)>0
                || node.acceptance_criteria.required_entities.iter().any(|entity| matches!(entity,EntityRef::Claim(_) | EntityRef::Finding(_)));
            if outcome == ResearchOutcome::Completed && !evidence_present { return Err(Error::InvalidTransition("research completion requires consumed source content or grounded Claim/Finding references".into())); }
            if outcome != ResearchOutcome::Blocked && readiness["completion"]["ready"] != true {
                return Err(Error::InvalidTransition(format!("research completion conditions unmet: {}",readiness["completion"]["blockers"])));
            }
            let mut ids = std::collections::BTreeSet::from([run]);
            let mut ancestor = current_run.parent_run_id;
            while let Some(id) = ancestor {
                if s.execution_bindings.get(&id).is_none_or(|binding| binding.work_node_id != work) { break; }
                ids.insert(id); ancestor = s.runs.get(&id).and_then(|run| run.parent_run_id);
            }
            if s.runs.values().any(|r| !r.status.terminal() && !ids.contains(&r.id)
                && (r.parent_run_id.is_some_and(|parent| ids.contains(&parent)) || s.execution_bindings.get(&r.id).is_some_and(|binding| binding.work_node_id==work))) {
                return Err(Error::Conflict("research has active child or parallel runs".into()));
            }
            for id in ids {
                let r=s.runs.get_mut(&id).ok_or(Error::PermissionDenied)?;
                r.meta.update(r.meta.version)?; r.status=if outcome==ResearchOutcome::Blocked {RunStatus::Failed} else {RunStatus::Completed};r.finished_at=Some(Utc::now());
                let task=s.tasks.get_mut(&r.task_id).ok_or(Error::PermissionDenied)?;task.meta.update(task.meta.version)?;task.status=if outcome==ResearchOutcome::Blocked {TaskStatus::Failed} else {TaskStatus::Completed};
            }
            let node=s.work_nodes.get_mut(&work).ok_or(Error::PermissionDenied)?;node.meta.update(node.meta.version)?;
            node.status=match outcome { ResearchOutcome::Completed=>WorkStatus::Succeeded,ResearchOutcome::Inconclusive=>WorkStatus::Failed,ResearchOutcome::Blocked=>WorkStatus::Blocked };
            node.outcome = Some(outcome);
            let result=node.clone();
            runtime::revision(s,result.branch_id,&ctx)?;
            for id in reviewed {
                audit(s,&ctx,"BranchResultReconciled",None,None,vec![],json!({"source_work_node_id":id,"source_branch_id":s.work_nodes[&id].branch_id,"target_work_node_id":work,"review_summary":summary}))?;
            }
            if matches!(outcome,ResearchOutcome::Completed|ResearchOutcome::Inconclusive) {
                let parent=s.work_edges.values().find(|edge|edge.from==work&&edge.kind==WorkEdgeKind::BranchesFrom).map(|edge|edge.to);
                if let Some(parent)=parent {
                    let mut result_work=std::collections::BTreeSet::from([work]);
                    loop {let count=result_work.len();for edge in s.work_edges.values().filter(|edge|edge.kind==WorkEdgeKind::Contains&&edge.branch_id==result.branch_id) {if result_work.contains(&edge.from) {result_work.insert(edge.to);}}if count==result_work.len(){break;}}
                    let artifacts:Vec<_>=s.tool_invocations.values().filter(|invocation|result_work.contains(&invocation.work_node_id)&&invocation.status==InvocationStatus::Succeeded).flat_map(|invocation|invocation.output_artifacts.iter().copied()).collect();
                    audit(s,&ctx,"BranchResultAvailable",None,None,vec![],json!({"source_work_node_id":work,"source_branch_id":result.branch_id,"target_work_node_id":parent,"outcome":outcome,"summary":summary,"source_artifacts":artifacts}))?;
                }
            }
            audit(s,&ctx,"ResearchTerminated",None,None,vec![],json!({"work_node_id":work,"outcome":outcome,"summary":summary,"readiness":readiness}))?;
            Ok(result)
        })
    }
    pub fn conclude_work_inconclusive(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        summary: String,
    ) -> Result<WorkNode> {
        text(&summary, "inconclusive explanation")?;
        let ctx = self.context("Explicit inconclusive work result".into(), Some(run));
        self.core.mutate(project, |s| {
            ctx.validate(s)?;
            require_agent_progress(s, &ctx.actor)?;
            let r = s.runs.get(&run).ok_or(Error::PermissionDenied)?;
            if r.status != RunStatus::Running
                || s.tasks[&r.task_id].created_by != ctx.actor
                || s.execution_bindings
                    .get(&run)
                    .is_none_or(|binding| binding.work_node_id != work)
            {
                return Err(Error::PermissionDenied);
            }
            if s.work_nodes.get(&work).is_none_or(|node| {
                node.granularity != WorkGranularity::Atomic || node.status != WorkStatus::Running
            }) {
                return Err(Error::InvalidTransition(
                    "only active atomic work can conclude inconclusive".into(),
                ));
            }
            if !s.tool_invocations.values().any(|inv| {
                inv.work_node_id == work
                    && matches!(
                        inv.status,
                        InvocationStatus::Succeeded | InvocationStatus::Failed
                    )
            }) {
                return Err(Error::InvalidTransition(
                    "inconclusive work needs an actual recorded attempt".into(),
                ));
            }
            if s.gates
                .values()
                .any(|gate| gate.status == GateStatus::Pending)
            {
                return Err(Error::GateRequired);
            }
            if runtime::has_pending_retrieval(s, &std::collections::BTreeSet::from([work])) {
                return Err(Error::InvalidTransition(
                    "PendingRetrievalMustBeResolved".into(),
                ));
            }
            if s.runs.values().any(|other| {
                other.id != run
                    && !other.status.terminal()
                    && s.execution_bindings
                        .get(&other.id)
                        .is_some_and(|binding| binding.work_node_id == work)
            }) {
                return Err(Error::Conflict("work has another active run".into()));
            }
            let r = s.runs.get_mut(&run).ok_or(Error::PermissionDenied)?;
            r.meta.update(r.meta.version)?;
            r.status = RunStatus::Completed;
            r.finished_at = Some(Utc::now());
            let task = s.tasks.get_mut(&r.task_id).ok_or(Error::PermissionDenied)?;
            task.meta.update(task.meta.version)?;
            task.status = TaskStatus::Completed;
            let node = s.work_nodes.get_mut(&work).ok_or(Error::PermissionDenied)?;
            node.meta.update(node.meta.version)?;
            node.status = WorkStatus::Failed;
            node.outcome = Some(ResearchOutcome::Inconclusive);
            let result = node.clone();
            runtime::revision(s, result.branch_id, &ctx)?;
            audit(
                s,
                &ctx,
                "WorkConcludedInconclusive",
                None,
                None,
                vec![],
                json!({"work_node_id":work,"summary":summary,"outcome":"inconclusive"}),
            )?;
            Ok(result)
        })
    }
    pub fn validate_work_completion(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
    ) -> Result<()> {
        let state = self.core.runtime_state(project)?;
        if runtime::has_pending_retrieval(&state, &std::collections::BTreeSet::from([work])) {
            return Err(Error::InvalidTransition(
                "PendingRetrievalMustBeResolved".into(),
            ));
        }
        let binding = state
            .execution_bindings
            .get(&run)
            .ok_or_else(|| Error::NotFound("execution binding".into()))?;
        if binding.work_node_id != work {
            return Err(Error::PermissionDenied);
        }
        let node = state
            .work_nodes
            .get(&work)
            .ok_or_else(|| Error::NotFound("work".into()))?;
        let required_tool = node
            .capability_requirements
            .iter()
            .find_map(|v| v.strip_prefix("tool:"))
            .ok_or_else(|| {
                Error::Validation("WorkNode has no required tool acceptance criterion".into())
            })?;
        let has_output = state.tool_invocations.values().any(|i| {
            i.work_node_id == work && i.tool == required_tool && usable_required_output(&state, i)
        });
        if !has_output {
            return Err(Error::InvalidTransition(
                "acceptance criteria unmet: required tool output is missing".into(),
            ));
        }
        Ok(())
    }
    pub fn required_tool_output_exists(
        &self,
        project: ProjectId,
        work: WorkNodeId,
    ) -> Result<bool> {
        let state = self.core.runtime_state(project)?;
        let node = state
            .work_nodes
            .get(&work)
            .ok_or_else(|| Error::NotFound("work".into()))?;
        let Some(required_tool) = node
            .capability_requirements
            .iter()
            .find_map(|v| v.strip_prefix("tool:"))
        else {
            return Ok(false);
        };
        Ok(state.tool_invocations.values().any(|invocation| {
            invocation.work_node_id == work
                && invocation.tool == required_tool
                && usable_required_output(&state, invocation)
        }))
    }
    pub fn create_branch(&self, project: ProjectId, run: RunId, name: String) -> Result<Branch> {
        let state = self.core.runtime_state(project)?;
        let binding = state
            .execution_bindings
            .get(&run)
            .ok_or_else(|| Error::NotFound("execution binding".into()))?;
        let base = state
            .checkpoints
            .values()
            .filter(|c| {
                c.branch_id == binding.branch_id && c.plan_revision_id == binding.plan_revision_id
            })
            // Sibling branches created by consecutive Lead decisions on the
            // same immutable plan revision inherit the same split point.
            .min_by_key(|c| c.meta.created_at)
            .or_else(|| {
                state
                    .checkpoints
                    .values()
                    .filter(|c| c.branch_id == binding.branch_id)
                    .max_by_key(|c| c.meta.created_at)
            })
            .map(|c| c.id);
        self.core.create_branch(
            project,
            name,
            base,
            self.context(
                "Lead creates research branch from checkpoint".into(),
                Some(run),
            ),
        )
    }
    /// Preserve a Lead-produced task result without promoting it to scientific knowledge.
    pub fn record_work_result(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        summary: String,
    ) -> Result<ProcessMemory> {
        if summary.trim().is_empty() || summary.chars().count() > 800 {
            return Err(Error::Validation(
                "work result must contain 1..800 characters".into(),
            ));
        }
        self.validate_work_completion(project, run, work)?;
        let state = self.core.runtime_state(project)?;
        let execution = state.runs.get(&run).ok_or(Error::PermissionDenied)?;
        if execution.status != RunStatus::Running
            || state
                .tasks
                .get(&execution.task_id)
                .is_none_or(|task| task.created_by != Actor::LeadAgent(self.identity.clone()))
        {
            return Err(Error::PermissionDenied);
        }
        let source_artifacts: std::collections::BTreeSet<_> = state
            .tool_invocations
            .values()
            .filter(|inv| inv.work_node_id == work && inv.status == InvocationStatus::Succeeded)
            .flat_map(|inv| inv.output_artifacts.iter().copied())
            .collect();
        self.write_process_memory(project, run, work, format!("Work result observation: {}",
            json!({"summary":summary,"source_artifacts":source_artifacts,"scientific_status":"unreviewed Lead task result"})))
    }
    pub fn write_process_memory(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        content: String,
    ) -> Result<ProcessMemory> {
        self.core.write_memory(
            project,
            MemoryNamespace::ProcessMemory,
            Some(work),
            content,
            self.context("Persist concise research process memory".into(), Some(run)),
        )
    }
    pub fn checkpoint_step(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        snapshot: ContextSnapshotId,
        state: Value,
    ) -> Result<ExecutionCheckpoint> {
        let s = self.core.runtime_state(project)?;
        let binding = s
            .execution_bindings
            .get(&run)
            .ok_or_else(|| Error::NotFound("execution binding".into()))?;
        if binding.work_node_id != work {
            return Err(Error::PermissionDenied);
        }
        self.core.checkpoint(
            project,
            ExecutionCheckpoint {
                id: CheckpointId::new(),
                meta: Meta::new(project),
                run_id: Some(run),
                work_node_id: Some(work),
                branch_id: binding.branch_id,
                plan_revision_id: binding.plan_revision_id,
                context_snapshot_id: Some(snapshot),
                boundary: CheckpointBoundary::Suspended,
                execution_state: state,
            },
            self.context(
                "Checkpoint long-horizon decision boundary".into(),
                Some(run),
            ),
        )
    }
    // The fields form one invocation record; grouping them in an opaque tuple
    // would make callers less explicit about provenance and terminal status.
    #[allow(clippy::too_many_arguments)]
    pub fn record_model_invocation(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        snapshot: ContextSnapshotId,
        model: String,
        started_at: chrono::DateTime<chrono::Utc>,
        latency_ms: u64,
        usage: TokenUsage,
        metadata: Value,
        succeeded: bool,
    ) -> Result<ModelInvocation> {
        let mut record = ModelInvocation {
            id: ModelInvocationId::new(),
            meta: Meta::new(project),
            run_id: run,
            work_node_id: work,
            context_snapshot_id: snapshot,
            provider: "local-test-lead".into(),
            model,
            model_revision: "configured-local-profile".into(),
            runtime: "openai-compatible-loopback".into(),
            instruction_version: "anix-lead-step-v2".into(),
            started_at,
            finished_at: None,
            latency_ms: None,
            status: InvocationStatus::Running,
            usage,
            metadata,
            output_schema: serde_json::json!({"schema_version":1}),
        };
        record = self.core.record_model_invocation(
            project,
            record,
            self.context("Start bounded lead decision invocation".into(), Some(run)),
        )?;
        record.finished_at = Some(chrono::Utc::now());
        record.latency_ms = Some(latency_ms);
        record.status = if succeeded {
            InvocationStatus::Succeeded
        } else {
            InvocationStatus::Failed
        };
        self.core.record_model_invocation(
            project,
            record,
            self.context("Finish bounded lead decision invocation".into(), Some(run)),
        )
    }
    pub fn submit(
        &self,
        project: ProjectId,
        run: RunId,
        action: ActionEnvelope,
        reason: String,
    ) -> Result<Submission> {
        let s = self.core.state(project)?;
        let r = s
            .runs
            .get(&run)
            .ok_or_else(|| Error::NotFound("run".into()))?;
        let t = s
            .tasks
            .get(&r.task_id)
            .ok_or_else(|| Error::NotFound("task".into()))?;
        if t.created_by != Actor::LeadAgent(self.identity.clone()) || r.status != RunStatus::Running
        {
            return Err(Error::PermissionDenied);
        }
        self.core.submit_proposal(
            project,
            action,
            self.context(reason, Some(run)),
            "Research state change proposed by lead agent".into(),
        )
    }
    pub fn finish(&self, project: ProjectId, run: RunId, status: RunStatus) -> Result<Run> {
        let s = self.core.state(project)?;
        let r = s
            .runs
            .get(&run)
            .ok_or_else(|| Error::NotFound("run".into()))?;
        if s.tasks.get(&r.task_id).map(|t| &t.created_by)
            != Some(&Actor::LeadAgent(self.identity.clone()))
        {
            return Err(Error::PermissionDenied);
        }
        self.core.finish_run(
            project,
            run,
            status,
            self.context("Lead agent execution finished".into(), Some(run)),
        )
    }
}

use actions::{action_dependencies, execute_action, result_entity, validate_action};

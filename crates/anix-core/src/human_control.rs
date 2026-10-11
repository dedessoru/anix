use crate::*;
use chrono::Utc;

pub(crate) fn control(state: &ProjectState) -> HumanControlState {
    let mut control = state.human_control.clone().unwrap_or_default();
    if control.owner_id.is_none() {
        control.owner_id = state.events.iter().find_map(|e| {
            if e.event_type == "ProjectCreated" {
                if let Actor::Human(id) = &e.actor {
                    Some(id.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });
    }
    control
}
pub(crate) fn authorize_project_human(state: &ProjectState, actor: &Actor) -> Result<()> {
    actor.require_human()?;
    let identity = match actor {
        Actor::Human(id) => id,
        _ => unreachable!(),
    };
    if control(state)
        .owner_id
        .as_ref()
        .is_some_and(|owner| owner != identity)
    {
        return Err(Error::PermissionDenied);
    }
    Ok(())
}
fn directive_kind(value: &str) -> Result<HumanDirectiveKind> {
    match value {
        "PauseResearch" => Ok(HumanDirectiveKind::PauseResearch),
        "ResumeResearch" => Ok(HumanDirectiveKind::ResumeResearch),
        "CancelResearch" => Ok(HumanDirectiveKind::CancelResearch),
        "PrioritizeWork" => Ok(HumanDirectiveKind::PrioritizeWork),
        "DeprioritizeWork" => Ok(HumanDirectiveKind::DeprioritizeWork),
        "CreateBranch" => Ok(HumanDirectiveKind::CreateBranch),
        "PauseBranch" => Ok(HumanDirectiveKind::PauseBranch),
        "ResumeBranch" => Ok(HumanDirectiveKind::ResumeBranch),
        "CancelBranch" => Ok(HumanDirectiveKind::CancelBranch),
        "PauseWork" => Ok(HumanDirectiveKind::PauseWork),
        "ResumeWork" => Ok(HumanDirectiveKind::ResumeWork),
        "CancelWork" => Ok(HumanDirectiveKind::CancelWork),
        "OverrideRecommendation" => Ok(HumanDirectiveKind::OverrideRecommendation),
        "RequestPlanRevision" => Ok(HumanDirectiveKind::RequestPlanRevision),
        "RequestDeeperInvestigation" => Ok(HumanDirectiveKind::RequestDeeperInvestigation),
        "RequestContradictionSearch" => Ok(HumanDirectiveKind::RequestContradictionSearch),
        "AdjustBudget" => Ok(HumanDirectiveKind::AdjustBudget),
        "AddConstraint" => Ok(HumanDirectiveKind::AddConstraint),
        "SetAutonomy" => Ok(HumanDirectiveKind::SetAutonomy),
        "PrioritizeBranch" => Ok(HumanDirectiveKind::PrioritizeBranch),
        _ => Err(Error::Validation("unsupported human directive type".into())),
    }
}
pub(crate) fn policy_requires_gate(state: &ProjectState, action: &ProposedAction) -> bool {
    // Plan strategy changes are proposals, never a Human capability delegated to a model.
    if matches!(
        action,
        ProposedAction::ReviseWorkObjective { .. } | ProposedAction::ReviseWorkPlan { .. }
    ) {
        return true;
    }
    let c = control(state);
    let impact = match action {
        ProposedAction::ArchiveEntity { .. } => ActionImpact::NonDelegable,
        ProposedAction::ActivateExperiment { .. } => ActionImpact::Consequential,
        ProposedAction::CreateRelation(NewRelation {
            kind: RelationKind::References | RelationKind::RelatedTo,
            ..
        }) => ActionImpact::Routine,
        _ => ActionImpact::Meaningful,
    };
    if impact == ActionImpact::NonDelegable {
        return true;
    }
    match c.autonomy.mode {
        AutonomyMode::Manual => impact != ActionImpact::Routine,
        AutonomyMode::Supervised => {
            impact == ActionImpact::Consequential || GatePolicy::requires_approval(action)
        }
        AutonomyMode::Autonomous | AutonomyMode::Batch => match action {
            ProposedAction::ActivateExperiment { .. } => !c.autonomy.allow_experiment_activation,
            _ => false,
        },
    }
}
pub(crate) fn require_agent_progress(state: &ProjectState, actor: &Actor) -> Result<()> {
    if !matches!(actor, Actor::LeadAgent(_)) {
        return Ok(());
    }
    let control = control(state);
    match control.status {
        ResearchStatus::Paused => Err(Error::Conflict(
            "research is paused at a safe boundary".into(),
        )),
        ResearchStatus::Cancelled => Err(Error::Conflict("research is cancelled".into())),
        _ => Ok(()),
    }
}
pub(crate) fn require_runtime_progress(state: &ProjectState, run: RunId) -> Result<()> {
    let c = control(state);
    if matches!(c.status, ResearchStatus::Paused | ResearchStatus::Cancelled) {
        return Err(Error::Conflict("research is paused or cancelled".into()));
    }
    if let Some(binding) = state.execution_bindings.get(&run) {
        if c.paused_branches.contains(&binding.branch_id)
            || c.cancelled_branches.contains(&binding.branch_id)
        {
            return Err(Error::Conflict(
                "execution branch is paused or cancelled".into(),
            ));
        }
    }
    Ok(())
}
pub(crate) fn mark_waiting_for_gate(state: &mut ProjectState) {
    let mut current = control(state);
    if !matches!(
        current.status,
        ResearchStatus::Paused | ResearchStatus::Cancelled
    ) {
        current.status = ResearchStatus::WaitingForHuman;
    }
    state.human_control = Some(current);
}
pub(crate) fn refresh_gate_status(state: &mut ProjectState) {
    if !state
        .gates
        .values()
        .any(|g| g.status == GateStatus::Pending)
    {
        let mut current = control(state);
        if current.status == ResearchStatus::WaitingForHuman {
            current.status = ResearchStatus::Running;
        }
        state.human_control = Some(current);
    }
}

impl Core {
    pub fn get_human_control(&self, project: ProjectId) -> Result<HumanControlState> {
        Ok(control(&self.state(project)?))
    }

    #[allow(clippy::too_many_arguments)]
    fn directive_apply(
        &self,
        project: ProjectId,
        kind: &str,
        target: Option<String>,
        parameters: Value,
        expected: Option<u64>,
        ctx: CommandContext,
        apply: impl FnOnce(&mut ProjectState, &mut HumanControlState, &HumanDirective) -> Result<()>,
    ) -> Result<HumanDirective> {
        authorize_project_human(&self.state(project)?, &ctx.actor)?;
        text(&ctx.reason, "reason")?;
        safe_metadata(&parameters)?;
        self.mutate(project, |state| {
            active(state)?;
            ctx.validate(state)?;
            let mut current = control(state);
            if kind == "ResumeResearch" && current.status == ResearchStatus::Running {
                if let Some(existing)=current.directives.iter().rev().find(|d|d.kind==HumanDirectiveKind::ResumeResearch) { return Ok(existing.clone()); }
            }
            if let Some(expected) = expected {
                if current.version != expected { return Err(Error::StaleVersion { expected, actual: current.version }); }
            }
            let next = current.version.checked_add(1).ok_or_else(|| Error::Validation("control version overflow".into()))?;
            let actor_id = match &ctx.actor { Actor::Human(id) => id.clone(), _ => unreachable!() };
            let mut parameters = parameters;
            if kind == "AdjustBudget" {
                let new_budget = parameters.get("budget").cloned().unwrap_or(Value::Null);
                parameters = json!({"old":current.budget,"new":new_budget});
            }
            let directive = HumanDirective { id: EventId::new(), project_id: project, actor_id, created_at: Utc::now(), kind: directive_kind(kind)?, target, parameters: parameters.clone(), reason: Some(ctx.reason.clone()), expected_version: expected, version: next };
            apply(state, &mut current, &directive)?;
            current.version = next;
            current.directives.push(directive.clone());
            state.human_control = Some(current);
            audit(state, &ctx, "human.directive", None, None, vec![], json!({"directive_id":directive.id,"kind":kind,"target":directive.target,"parameters":parameters,"version":next}))?;
            Ok(directive)
        })
    }

    pub fn set_autonomy(
        &self,
        project: ProjectId,
        mode: AutonomyMode,
        expected_version: Option<u64>,
        ctx: CommandContext,
    ) -> Result<AutonomyPolicy> {
        authorize_project_human(&self.state(project)?, &ctx.actor)?;
        let target = mode;
        self.mutate(project, |state| {
            active(state)?;
            ctx.validate(state)?;
            let mut current = control(state);
            if let Some(expected) = expected_version {
                if current.version != expected {
                    return Err(Error::StaleVersion {
                        expected,
                        actual: current.version,
                    });
                }
            }
            let old = current.autonomy.mode;
            current.version = current
                .version
                .checked_add(1)
                .ok_or_else(|| Error::Validation("control version overflow".into()))?;
            current.autonomy.mode = target;
            current.autonomy.version = current
                .autonomy
                .version
                .checked_add(1)
                .ok_or_else(|| Error::Validation("policy version overflow".into()))?;
            let directive = HumanDirective {
                id: EventId::new(),
                project_id: project,
                actor_id: match &ctx.actor {
                    Actor::Human(id) => id.clone(),
                    _ => unreachable!(),
                },
                created_at: Utc::now(),
                kind: HumanDirectiveKind::SetAutonomy,
                target: None,
                parameters: json!({"from":old,"to":target}),
                reason: Some(ctx.reason.clone()),
                expected_version,
                version: current.version,
            };
            current.directives.push(directive);
            let policy = current.autonomy.clone();
            state.human_control = Some(current);
            audit(
                state,
                &ctx,
                "human.autonomy_changed",
                None,
                None,
                vec![],
                json!({"from":old,"to":target,"policy_version":policy.version}),
            )?;
            Ok(policy)
        })
    }

    pub fn pause_research(
        &self,
        project: ProjectId,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.directive_apply(
            project,
            "PauseResearch",
            None,
            Value::Null,
            expected,
            ctx,
            |state, c, directive| {
                c.status = ResearchStatus::Paused;
                for binding in state.execution_bindings.values() {
                    if state.runs.get(&binding.id).is_some_and(|r| r.status == RunStatus::Running) {
                        let checkpoint=ExecutionCheckpoint{id:CheckpointId::new(),meta:Meta::new(project),run_id:Some(binding.id),work_node_id:Some(binding.work_node_id),branch_id:binding.branch_id,plan_revision_id:binding.plan_revision_id,context_snapshot_id:state.context_snapshots.values().filter(|x|x.work_node_id==binding.work_node_id).max_by_key(|x|x.meta.created_at).map(|x|x.id),boundary:CheckpointBoundary::Suspended,execution_state:json!({"schema_version":1,"human_directive_id":directive.id})};
                        state.checkpoints.insert(checkpoint.id,checkpoint);
                    }
                }
                Ok(())
            },
        )
    }
    pub fn resume_research(
        &self,
        project: ProjectId,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.directive_apply(
            project,
            "ResumeResearch",
            None,
            Value::Null,
            expected,
            ctx,
            |_, c, _| {
                if c.status == ResearchStatus::Cancelled {
                    return Err(Error::Conflict(
                        "cancelled research cannot be resumed".into(),
                    ));
                }
                c.status = ResearchStatus::Running;
                Ok(())
            },
        )
    }
    pub fn cancel_research(
        &self,
        project: ProjectId,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        self.directive_apply(
            project,
            "CancelResearch",
            None,
            Value::Null,
            expected,
            ctx,
            |_, c, _| {
                c.status = ResearchStatus::Cancelled;
                Ok(())
            },
        )
    }
    pub fn set_work_priority(
        &self,
        project: ProjectId,
        id: WorkNodeId,
        priority: Priority,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        let state = self.state(project)?;
        if !state.work_nodes.contains_key(&id) {
            return Err(Error::NotFound("work node".into()));
        }
        let kind = if priority == Priority::Low {
            "DeprioritizeWork"
        } else {
            "PrioritizeWork"
        };
        self.directive_apply(
            project,
            kind,
            Some(id.to_string()),
            json!({"priority":priority}),
            expected,
            ctx,
            |_, c, _| {
                c.work_priorities.insert(id, priority);
                Ok(())
            },
        )
    }
    pub fn set_branch_priority(
        &self,
        project: ProjectId,
        id: BranchId,
        priority: Priority,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        let state = self.state(project)?;
        if !state.branches.contains_key(&id) {
            return Err(Error::NotFound("branch".into()));
        }
        self.directive_apply(
            project,
            "PrioritizeBranch",
            Some(id.to_string()),
            json!({"priority":priority}),
            expected,
            ctx,
            |_, c, _| {
                c.branch_priorities.insert(id, priority);
                Ok(())
            },
        )
    }
    pub fn set_budget(
        &self,
        project: ProjectId,
        budget: ResearchBudget,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        let params = json!({"budget":budget});
        self.directive_apply(
            project,
            "AdjustBudget",
            None,
            params,
            expected,
            ctx,
            |_, c, _| {
                c.budget = budget;
                Ok(())
            },
        )
    }
    pub fn add_constraint(
        &self,
        project: ProjectId,
        text_value: String,
        kind: ConstraintKind,
        excluded_source: Option<SourceId>,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        text(&text_value, "constraint")?;
        if kind == ConstraintKind::Hard && excluded_source.is_none() {
            return Err(Error::Validation(
                "hard constraints currently require a typed excluded_source rule".into(),
            ));
        }
        if let Some(id) = excluded_source {
            let state = self.state(project)?;
            if !state.sources.contains_key(&id) {
                return Err(Error::NotFound("source".into()));
            }
        }
        let params = json!({"text":text_value,"kind":kind,"excluded_source":excluded_source});
        let constraint_id = EventId::new().to_string();
        self.directive_apply(
            project,
            "AddConstraint",
            None,
            params,
            expected,
            ctx.clone(),
            move |_, c, _| {
                c.constraints.push(ResearchConstraint {
                    id: constraint_id,
                    text: text_value,
                    kind,
                    excluded_source,
                    created_by: ctx.actor,
                    created_at: Utc::now(),
                });
                Ok(())
            },
        )
    }
    pub fn set_branch_paused(
        &self,
        project: ProjectId,
        id: BranchId,
        paused: bool,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        let state = self.state(project)?;
        if !state.branches.contains_key(&id) {
            return Err(Error::NotFound("branch".into()));
        }
        let kind = if paused {
            "PauseBranch"
        } else {
            "ResumeBranch"
        };
        self.directive_apply(
            project,
            kind,
            Some(id.to_string()),
            Value::Null,
            expected,
            ctx,
            |_, c, _| {
                if paused {
                    c.paused_branches.insert(id);
                } else {
                    c.paused_branches.remove(&id);
                }
                Ok(())
            },
        )
    }
    pub fn request_investigation(
        &self,
        project: ProjectId,
        contradiction_search: bool,
        focus: Option<String>,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        if let Some(value) = &focus {
            text(value, "investigation focus")?;
        }
        let kind = if contradiction_search {
            "RequestContradictionSearch"
        } else {
            "RequestDeeperInvestigation"
        };
        self.directive_apply(
            project,
            kind,
            focus,
            json!({"requested":true}),
            expected,
            ctx,
            |_, _, _| Ok(()),
        )
    }
    pub fn cancel_branch(
        &self,
        project: ProjectId,
        id: BranchId,
        expected: Option<u64>,
        ctx: CommandContext,
    ) -> Result<HumanDirective> {
        let state = self.state(project)?;
        if !state.branches.contains_key(&id) {
            return Err(Error::NotFound("branch".into()));
        }
        self.directive_apply(
            project,
            "CancelBranch",
            Some(id.to_string()),
            Value::Null,
            expected,
            ctx,
            |_, c, _| {
                c.cancelled_branches.insert(id);
                Ok(())
            },
        )
    }
    pub fn action_requires_gate(
        &self,
        project: ProjectId,
        action: &ProposedAction,
        impact: ActionImpact,
    ) -> Result<bool> {
        let state = self.state(project)?;
        Ok(impact == ActionImpact::NonDelegable || policy_requires_gate(&state, action))
    }
}

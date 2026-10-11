use super::*;

fn normalize_work_text(value: &str) -> String {
    value
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}
fn plan_children(s: &ProjectState, parent: WorkNodeId) -> impl Iterator<Item = &WorkNode> {
    s.work_edges
        .values()
        .filter(move |e| e.from == parent && e.kind == WorkEdgeKind::Contains)
        .filter_map(|e| s.work_nodes.get(&e.to))
}
fn work_plan_selection(
    s: &ProjectState,
    parent: WorkNodeId,
    steps: &[WorkPlanStep],
) -> Vec<Option<WorkNodeId>> {
    let mut selected = Vec::new();
    for step in steps {
        let expected: Option<std::collections::BTreeSet<_>> = step
            .depends_on
            .iter()
            .map(|i| selected.get(*i).copied().flatten())
            .collect();
        let existing = expected.and_then(|expected| {
            plan_children(s, parent)
                .find(|n| {
                    !matches!(n.status, WorkStatus::Cancelled | WorkStatus::Superseded)
                        && !selected.contains(&Some(n.id))
                        && normalize_work_text(&n.objective) == normalize_work_text(&step.objective)
                        && n.capability_requirements == vec![format!("tool:{}", step.required_tool)]
                        && s.work_edges
                            .values()
                            .filter(|e| e.from == n.id && e.kind == WorkEdgeKind::DependsOn)
                            .map(|e| e.to)
                            .collect::<std::collections::BTreeSet<_>>()
                            == expected
                })
                .map(|n| n.id)
        });
        selected.push(existing);
    }
    selected
}
pub(crate) fn work_plan_is_unchanged(
    s: &ProjectState,
    parent: WorkNodeId,
    steps: &[WorkPlanStep],
) -> bool {
    let selected = work_plan_selection(s, parent, steps);
    selected.iter().all(Option::is_some)
        && plan_children(s, parent)
            .filter(|n| {
                !matches!(
                    n.status,
                    WorkStatus::Succeeded | WorkStatus::Cancelled | WorkStatus::Superseded
                )
            })
            .all(|n| selected.contains(&Some(n.id)))
}
fn apply_work_plan(
    s: &mut ProjectState,
    parent: WorkNodeId,
    expected: u64,
    steps: &[WorkPlanStep],
    ctx: &CommandContext,
) -> Result<()> {
    let root = s.work_nodes[&parent].clone();
    let selected = work_plan_selection(s, parent, steps);
    let retired: Vec<_> = plan_children(s, parent)
        .filter(|n| {
            !selected.contains(&Some(n.id))
                && !matches!(
                    n.status,
                    WorkStatus::Succeeded | WorkStatus::Cancelled | WorkStatus::Superseded
                )
        })
        .map(|n| n.id)
        .collect();
    for id in retired {
        let n = s.work_nodes.get_mut(&id).unwrap();
        n.meta.update(n.meta.version)?;
        n.status = WorkStatus::Superseded;
        audit(
            s,
            ctx,
            "WorkSupersededByStrategy",
            None,
            None,
            vec![],
            json!({"work_node_id":id,"parent":parent}),
        )?;
    }
    let mut ids = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let id = if let Some(id) = selected[index] {
            id
        } else {
            let id = WorkNodeId::new();
            let ready = step
                .depends_on
                .iter()
                .all(|i| s.work_nodes[&ids[*i]].status == WorkStatus::Succeeded);
            let n = WorkNode {
                id,
                meta: Meta::new(root.meta.project_id),
                branch_id: root.branch_id,
                kind: "research-step".into(),
                granularity: WorkGranularity::Atomic,
                title: step.title.clone(),
                objective: step.objective.clone(),
                acceptance_criteria: OutputContract {
                    required_entities: root.acceptance_criteria.required_entities.clone(),
                    required_artifacts: root.acceptance_criteria.required_artifacts.clone(),
                    minimum_children: 0,
                },
                status: if ready {
                    WorkStatus::Ready
                } else {
                    WorkStatus::Proposed
                },
                owner: root.owner.clone(),
                capability_requirements: vec![format!("tool:{}", step.required_tool)],
                context_policy_id: root.context_policy_id.clone(),
                budget: root.budget.clone(),
                created_by: ctx.actor.clone(),
                outcome: None,
            };
            s.work_nodes.insert(id, n);
            let edge = WorkEdge {
                id: WorkEdgeId::new(),
                meta: Meta::new(root.meta.project_id),
                branch_id: root.branch_id,
                from: parent,
                to: id,
                kind: WorkEdgeKind::Contains,
            };
            s.work_edges.insert(edge.id, edge);
            for dependency in &step.depends_on {
                let edge = WorkEdge {
                    id: WorkEdgeId::new(),
                    meta: Meta::new(root.meta.project_id),
                    branch_id: root.branch_id,
                    from: id,
                    to: ids[*dependency],
                    kind: WorkEdgeKind::DependsOn,
                };
                s.work_edges.insert(edge.id, edge);
            }
            audit(
                s,
                ctx,
                "WorkCreatedByStrategy",
                None,
                None,
                root.acceptance_criteria.required_entities.clone(),
                json!({"work_node_id":id,"parent":parent,"step":index}),
            )?;
            id
        };
        ids.push(id);
    }
    s.work_nodes
        .get_mut(&parent)
        .unwrap()
        .meta
        .update(expected)?;
    super::runtime::revision(s, root.branch_id, ctx)?;
    Ok(())
}

pub(crate) fn action_dependencies(action: &ProposedAction) -> Vec<EntityRef> {
    match action {
        ProposedAction::ReviseWorkObjective { .. } | ProposedAction::ReviseWorkPlan { .. } => {
            vec![]
        }
        ProposedAction::CreateHypothesis(v) => {
            v.question_id.map(EntityRef::Question).into_iter().collect()
        }
        ProposedAction::ReviseHypothesis { hypothesis_id, .. }
        | ProposedAction::ChangeHypothesisStatus { hypothesis_id, .. } => {
            vec![EntityRef::Hypothesis(*hypothesis_id)]
        }
        ProposedAction::CreateExperiment(v) => vec![EntityRef::Hypothesis(v.hypothesis_id)],
        ProposedAction::ActivateExperiment { experiment_id, .. } => {
            vec![EntityRef::Experiment(*experiment_id)]
        }
        ProposedAction::CreateFinding(v) => vec![EntityRef::ExperimentRun(v.experiment_run_id)],
        ProposedAction::CreateRelation(v) => vec![v.from_entity, v.to_entity],
        ProposedAction::ArchiveEntity { entity, .. } => vec![*entity],
    }
}
pub(crate) fn check_entity_policy(s: &ProjectState, entity: EntityRef) -> Result<()> {
    s.check(entity)?;
    if let EntityRef::Source(source) = entity {
        if s.human_control.as_ref().is_some_and(|c| {
            c.constraints.iter().any(|constraint| {
                constraint.kind == ConstraintKind::Hard
                    && constraint.excluded_source == Some(source)
            })
        }) {
            return Err(Error::Conflict(
                "source is excluded by a hard research constraint".into(),
            ));
        }
    }
    Ok(())
}
fn version(s: &ProjectState, entity: EntityRef, expected: u64) -> Result<()> {
    let actual = s.meta(entity)?.version;
    if actual != expected {
        Err(Error::StaleVersion { expected, actual })
    } else {
        Ok(())
    }
}
pub(crate) fn validate_action(s: &ProjectState, envelope: &ActionEnvelope) -> Result<()> {
    if envelope.schema_version != 1 {
        return Err(Error::Validation(
            "unsupported action schema version".into(),
        ));
    }
    safe_metadata(&serde_json::to_value(envelope).map_err(|e| Error::Validation(e.to_string()))?)?;
    for entity in action_dependencies(&envelope.action) {
        check_entity_policy(s, entity)?;
    }
    match &envelope.action {
        ProposedAction::ReviseWorkPlan {
            work_node_id,
            expected_version,
            expected_revision,
            steps,
        } => {
            let work = s
                .work_nodes
                .get(work_node_id)
                .ok_or_else(|| Error::NotFound("work".into()))?;
            if work.meta.version != *expected_version {
                return Err(Error::StaleVersion {
                    expected: *expected_version,
                    actual: work.meta.version,
                });
            }
            if s.branches[&work.branch_id].head_revision_id != Some(*expected_revision) {
                return Err(Error::Conflict("work plan changed before review".into()));
            }
            if work.granularity != WorkGranularity::Composite
                || !matches!(
                    work.status,
                    WorkStatus::Proposed
                        | WorkStatus::Ready
                        | WorkStatus::Running
                        | WorkStatus::Blocked
                )
            {
                return Err(Error::InvalidTransition(
                    "work strategy requires a nonterminal Composite".into(),
                ));
            }
            if steps.is_empty() || steps.len() > 8 {
                return Err(Error::Validation(
                    "work strategy must contain 1-8 steps".into(),
                ));
            }
            if work.acceptance_criteria.required_entities.is_empty()
                && work.acceptance_criteria.required_artifacts.is_empty()
            {
                return Err(Error::Validation(
                    "planned atomic work needs verifiable output references".into(),
                ));
            }
            for (index, step) in steps.iter().enumerate() {
                for (value, name, max) in [
                    (&step.title, "step title", 120),
                    (&step.objective, "step objective", 500),
                    (&step.required_tool, "required tool", 100),
                ] {
                    text(value, name)?;
                    if value.len() > max {
                        return Err(Error::Validation(format!("{name} too long")));
                    }
                }
                let deps: std::collections::BTreeSet<_> = step.depends_on.iter().copied().collect();
                if deps.len() != step.depends_on.len() || deps.iter().any(|d| *d >= index) {
                    return Err(Error::Validation(
                        "step dependencies must be distinct earlier steps".into(),
                    ));
                }
                if steps[..index].iter().any(|other| {
                    normalize_work_text(&other.objective) == normalize_work_text(&step.objective)
                        && other.required_tool == step.required_tool
                }) {
                    return Err(Error::Validation(
                        "duplicate strategy step; use justified replication".into(),
                    ));
                }
            }
            let selected = work_plan_selection(s, *work_node_id, steps);
            let kept: std::collections::BTreeSet<_> = selected.iter().flatten().copied().collect();
            let scoped_children: std::collections::BTreeSet<_> = plan_children(s, *work_node_id)
                .map(|child| child.id)
                .collect();
            for child in plan_children(s, *work_node_id).filter(|child| {
                !kept.contains(&child.id)
                    && !matches!(
                        child.status,
                        WorkStatus::Succeeded | WorkStatus::Cancelled | WorkStatus::Superseded
                    )
            }) {
                if s.work_edges.values().any(|edge| {
                    (edge.kind == WorkEdgeKind::DependsOn
                        && edge.to == child.id
                        && !scoped_children.contains(&edge.from))
                        || (edge.kind == WorkEdgeKind::Blocks
                            && edge.from == child.id
                            && !scoped_children.contains(&edge.to))
                }) {
                    return Err(Error::Conflict(
                        "strategy prerequisite is used outside its parent scope".into(),
                    ));
                }
                if matches!(
                    child.status,
                    WorkStatus::Running | WorkStatus::WaitingForHuman
                ) || s.execution_bindings.values().any(|b| {
                    b.work_node_id == child.id
                        && s.runs.get(&b.id).is_some_and(|r| {
                            matches!(r.status, RunStatus::Running | RunStatus::WaitingForHuman)
                        })
                }) {
                    return Err(Error::Conflict(
                        "cannot supersede active child execution".into(),
                    ));
                }
                if s.work_edges
                    .values()
                    .any(|e| e.from == child.id && e.kind == WorkEdgeKind::Contains)
                {
                    return Err(Error::Conflict(
                        "cannot replace a nested child plan with an atomic strategy".into(),
                    ));
                }
                if super::runtime::has_pending_retrieval(
                    s,
                    &std::collections::BTreeSet::from([child.id]),
                ) {
                    return Err(Error::Conflict(
                        "cannot supersede unresolved retrieval".into(),
                    ));
                }
            }
            let additions = selected.iter().filter(|id| id.is_none()).count();
            if s.human_control
                .as_ref()
                .and_then(|c| c.budget.max_steps)
                .is_some_and(|limit| s.work_nodes.len() as u64 + additions as u64 > limit)
            {
                return Err(Error::Conflict("project step budget exhausted".into()));
            }
        }
        ProposedAction::ReviseWorkObjective {
            work_node_id,
            expected_version,
            objective,
        } => {
            text(objective, "revised work objective")?;
            if objective.len() > 2000 {
                return Err(Error::Validation("revised objective too long".into()));
            }
            let work = s
                .work_nodes
                .get(work_node_id)
                .ok_or_else(|| Error::NotFound("work".into()))?;
            if work.meta.version != *expected_version {
                return Err(Error::StaleVersion {
                    expected: *expected_version,
                    actual: work.meta.version,
                });
            }
            if !matches!(
                work.status,
                WorkStatus::Ready | WorkStatus::Running | WorkStatus::Blocked
            ) {
                return Err(Error::InvalidTransition(
                    "terminal work cannot be replanned".into(),
                ));
            }
        }
        ProposedAction::CreateHypothesis(v) => {
            text(&v.title, "hypothesis title")?;
            text(&v.statement, "hypothesis statement")?;
        }
        ProposedAction::ReviseHypothesis {
            hypothesis_id,
            expected_version,
            statement,
        } => {
            text(statement, "hypothesis statement")?;
            version(s, EntityRef::Hypothesis(*hypothesis_id), *expected_version)?;
        }
        ProposedAction::ChangeHypothesisStatus {
            hypothesis_id,
            expected_version,
            status,
        } => {
            version(s, EntityRef::Hypothesis(*hypothesis_id), *expected_version)?;
            s.hypotheses
                .get(hypothesis_id)
                .ok_or_else(|| Error::NotFound("hypothesis".into()))?
                .status
                .transition(*status)?;
        }
        ProposedAction::CreateExperiment(v) => {
            version(
                s,
                EntityRef::Hypothesis(v.hypothesis_id),
                v.expected_version,
            )?;
            text(&v.title, "experiment title")?;
            text(&v.objective, "objective")?;
            text(&v.methodology, "methodology")?;
            text(&v.protocol, "protocol")?;
        }
        ProposedAction::ActivateExperiment {
            experiment_id,
            expected_version,
        } => {
            version(s, EntityRef::Experiment(*experiment_id), *expected_version)?;
            let status = s
                .experiments
                .get(experiment_id)
                .ok_or_else(|| Error::NotFound("experiment".into()))?
                .status;
            if status == ExperimentStatus::Draft || status == ExperimentStatus::Proposed {
                status.transition(ExperimentStatus::Approved)?;
                ExperimentStatus::Approved.transition(ExperimentStatus::Ready)?;
            } else {
                status.transition(ExperimentStatus::Ready)?;
            }
        }
        ProposedAction::CreateFinding(v) => {
            text(&v.title, "finding title")?;
            text(&v.summary, "finding summary")?;
            confidence(v.confidence)?;
            if s.experiment_runs
                .get(&v.experiment_run_id)
                .ok_or_else(|| Error::NotFound("experiment run".into()))?
                .status
                != RunStatus::Completed
            {
                return Err(Error::InvalidTransition(
                    "findings require a completed experiment run".into(),
                ));
            }
        }
        ProposedAction::CreateRelation(v) => {
            confidence(v.confidence)?;
            validate_relation(v.from_entity, v.to_entity, v.kind)?;
        }
        ProposedAction::ArchiveEntity {
            entity,
            expected_version,
        } => {
            version(s, *entity, *expected_version)?;
            if let EntityRef::Experiment(id) = entity {
                if s.experiments
                    .get(id)
                    .is_some_and(|e| e.status == ExperimentStatus::Running)
                {
                    return Err(Error::InvalidTransition(
                        "running experiment cannot be archived".into(),
                    ));
                }
            }
            if let EntityRef::ExperimentRun(id) = entity {
                if s.experiment_runs
                    .get(id)
                    .is_some_and(|e| !e.status.terminal())
                {
                    return Err(Error::InvalidTransition(
                        "active experiment run cannot be archived".into(),
                    ));
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn result_entity(result: &ActionResult) -> Option<EntityRef> {
    match result {
        ActionResult::Entity(e) => Some(*e),
        _ => None,
    }
}
pub(crate) fn execute_action(
    s: &mut ProjectState,
    envelope: ActionEnvelope,
    ctx: &CommandContext,
    gate: Option<GateId>,
) -> Result<ActionResult> {
    if let Some(receipt) = s.receipts.get(&envelope.id) {
        if receipt.action != envelope {
            return Err(Error::Conflict(
                "action ID reused with different payload".into(),
            ));
        }
        return Ok(receipt.result.clone());
    }
    validate_action(s, &envelope)?;
    let project = s.project()?.id;
    let sources = action_dependencies(&envelope.action);
    let (result, event_type) = match &envelope.action {
        ProposedAction::ReviseWorkPlan {
            work_node_id,
            expected_version,
            steps,
            ..
        } => {
            apply_work_plan(s, *work_node_id, *expected_version, steps, ctx)?;
            (ActionResult::WorkNode(*work_node_id), "WorkStrategyRevised")
        }
        ProposedAction::ReviseWorkObjective {
            work_node_id,
            expected_version,
            objective,
        } => {
            let work = s
                .work_nodes
                .get_mut(work_node_id)
                .ok_or_else(|| Error::NotFound("work".into()))?;
            work.meta.update(*expected_version)?;
            work.objective = objective.clone();
            let branch = work.branch_id;
            super::runtime::revision(s, branch, ctx)?;
            (
                ActionResult::WorkNode(*work_node_id),
                "WorkObjectiveRevised",
            )
        }
        ProposedAction::CreateHypothesis(v) => {
            let entity = Hypothesis {
                id: HypothesisId::new(),
                meta: Meta::new(project),
                question_id: v.question_id,
                title: v.title.clone(),
                statement: v.statement.clone(),
                status: HypothesisStatus::Draft,
                confidence: None,
            };
            let r = ActionResult::Entity(EntityRef::Hypothesis(entity.id));
            s.hypotheses.insert(entity.id, entity);
            (r, "HypothesisCreated")
        }
        ProposedAction::ReviseHypothesis {
            hypothesis_id,
            expected_version,
            statement,
        } => {
            let h = s
                .hypotheses
                .get_mut(hypothesis_id)
                .ok_or_else(|| Error::NotFound("hypothesis".into()))?;
            h.meta.update(*expected_version)?;
            h.statement = statement.clone();
            (
                ActionResult::Entity(EntityRef::Hypothesis(*hypothesis_id)),
                "HypothesisRevised",
            )
        }
        ProposedAction::ChangeHypothesisStatus {
            hypothesis_id,
            expected_version,
            status,
        } => {
            let h = s
                .hypotheses
                .get_mut(hypothesis_id)
                .ok_or_else(|| Error::NotFound("hypothesis".into()))?;
            h.meta.update(*expected_version)?;
            h.status = *status;
            h.meta.archived = *status == HypothesisStatus::Archived;
            (
                ActionResult::Entity(EntityRef::Hypothesis(*hypothesis_id)),
                "HypothesisStatusChanged",
            )
        }
        ProposedAction::CreateExperiment(v) => {
            let entity = Experiment {
                id: ExperimentId::new(),
                meta: Meta::new(project),
                hypothesis_id: v.hypothesis_id,
                title: v.title.clone(),
                objective: v.objective.clone(),
                methodology: v.methodology.clone(),
                protocol: v.protocol.clone(),
                status: if gate.is_some() {
                    ExperimentStatus::Approved
                } else {
                    ExperimentStatus::Draft
                },
            };
            let r = ActionResult::Entity(EntityRef::Experiment(entity.id));
            s.experiments.insert(entity.id, entity);
            (r, "ExperimentCreated")
        }
        ProposedAction::ActivateExperiment {
            experiment_id,
            expected_version,
        } => {
            let e = s
                .experiments
                .get_mut(experiment_id)
                .ok_or_else(|| Error::NotFound("experiment".into()))?;
            e.meta.update(*expected_version)?;
            e.status = ExperimentStatus::Ready;
            (
                ActionResult::Entity(EntityRef::Experiment(*experiment_id)),
                "ExperimentActivated",
            )
        }
        ProposedAction::CreateFinding(v) => {
            let entity = Finding {
                id: FindingId::new(),
                meta: Meta::new(project),
                experiment_run_id: v.experiment_run_id,
                title: v.title.clone(),
                summary: v.summary.clone(),
                confidence: v.confidence,
                created_by: ctx.actor.clone(),
            };
            let r = ActionResult::Entity(EntityRef::Finding(entity.id));
            s.findings.insert(entity.id, entity);
            (r, "FindingCreated")
        }
        ProposedAction::CreateRelation(v) => {
            if let Some(existing) = s.relations.values().find(|r| {
                !r.meta.archived
                    && r.from_entity == v.from_entity
                    && r.to_entity == v.to_entity
                    && r.kind == v.kind
            }) {
                let result = ActionResult::Relation(existing.id);
                s.receipts.insert(
                    envelope.id,
                    ActionReceipt {
                        id: envelope.id,
                        project_id: project,
                        action: envelope,
                        result: result.clone(),
                    },
                );
                return Ok(result);
            }
            let relation = Relation {
                id: RelationId::new(),
                meta: Meta::new(project),
                from_entity: v.from_entity,
                to_entity: v.to_entity,
                kind: v.kind,
                confidence: v.confidence,
                created_by: ctx.actor.clone(),
            };
            let r = ActionResult::Relation(relation.id);
            s.relations.insert(relation.id, relation);
            (r, "RelationCreated")
        }
        ProposedAction::ArchiveEntity {
            entity,
            expected_version,
        } => {
            let meta = s.meta_mut(*entity)?;
            meta.update(*expected_version)?;
            meta.archived = true;
            match entity {
                EntityRef::Hypothesis(id) => {
                    if let Some(e) = s.hypotheses.get_mut(id) {
                        e.status = HypothesisStatus::Archived
                    }
                }
                EntityRef::Experiment(id) => {
                    if let Some(e) = s.experiments.get_mut(id) {
                        e.status = ExperimentStatus::Archived
                    }
                }
                EntityRef::Question(id) => {
                    if let Some(e) = s.questions.get_mut(id) {
                        e.status = QuestionStatus::Archived
                    }
                }
                _ => {}
            }
            (ActionResult::Entity(*entity), "EntityArchived")
        }
    };
    // Persist inspectable before/after intent, not provider credentials or raw output.
    audit(
        s,
        ctx,
        event_type,
        result_entity(&result),
        gate,
        sources,
        json!({"action_id":envelope.id,"action":envelope.action,"result":result}),
    )?;
    s.receipts.insert(
        envelope.id,
        ActionReceipt {
            id: envelope.id,
            project_id: project,
            action: envelope,
            result: result.clone(),
        },
    );
    Ok(result)
}

use crate::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewWork {
    pub branch_id: BranchId,
    pub kind: String,
    pub granularity: WorkGranularity,
    pub title: String,
    pub objective: String,
    pub acceptance_criteria: OutputContract,
    pub capability_requirements: Vec<String>,
    pub context_policy_id: String,
    pub budget: WorkBudget,
}

fn branch_authorization_denied(
    project: ProjectId,
    requested_name: &str,
    actor: &Actor,
    branch_id: Option<BranchId>,
    parent_branch_id: Option<BranchId>,
    reason: &str,
) -> Error {
    Error::AuthorizationDenied(Box::new(AuthorizationTrace {
        actor: actor.clone(),
        action: "CreateChildBranch".into(),
        resource_type: "Branch".into(),
        // A new branch has no ID until authorization and validation succeed.
        resource_id: None,
        project_id: project.to_string(),
        branch_id: branch_id.map(|id| id.to_string()),
        parent_branch_id: parent_branch_id.map(|id| id.to_string()),
        required_capability: "CreateChildBranch".into(),
        // The current policy is run/task/parent scoped and has no capability registry.
        granted_capabilities: Vec::new(),
        result: "deny".into(),
        reason: format!("{reason}; requested branch: {requested_name}"),
    }))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextCompileRequest {
    pub work_node_id: WorkNodeId,
    pub plan_revision_id: PlanRevisionId,
    pub pinned: Vec<EntityRef>,
    pub instruction_version: String,
    pub skill_versions: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderContext {
    pub schema_version: u32,
    pub snapshot_id: ContextSnapshotId,
    pub objective: String,
    pub entries: Vec<ContextEntry>,
    pub process_notes: Vec<String>,
    pub instruction_version: String,
    pub skill_versions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchProcessGraph {
    pub branch: Branch,
    pub revision: PlanRevision,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchBranchScope {
    pub handle: String,
    pub branch_id: BranchId,
    pub work_node_id: WorkNodeId,
    pub parent_work_node_id: Option<WorkNodeId>,
    pub version: u64,
    pub name: String,
    pub status: WorkStatus,
    pub outcome: Option<ResearchOutcome>,
    pub active: bool,
    pub paused: bool,
}

pub(crate) fn branch_scopes(
    s: &ProjectState,
    current: WorkNodeId,
) -> Result<Vec<ResearchBranchScope>> {
    let current_node = node(s, current)?;
    if current_node.granularity != WorkGranularity::Composite {
        return Ok(vec![]);
    }
    let mut master = current;
    let mut seen = BTreeSet::new();
    while seen.insert(master) {
        if let Some(edge) = s
            .work_edges
            .values()
            .find(|edge| edge.from == master && edge.kind == WorkEdgeKind::BranchesFrom)
        {
            master = edge.to;
        } else {
            break;
        }
    }
    let mut family = BTreeSet::from([master]);
    loop {
        let count = family.len();
        for edge in s
            .work_edges
            .values()
            .filter(|edge| edge.kind == WorkEdgeKind::BranchesFrom)
        {
            if family.contains(&edge.to)
                && s.work_nodes
                    .get(&edge.from)
                    .is_some_and(|work| work.granularity == WorkGranularity::Composite)
            {
                family.insert(edge.from);
            }
        }
        if count == family.len() {
            break;
        }
    }
    let mut works: Vec<_> = family
        .into_iter()
        .filter_map(|id| s.work_nodes.get(&id))
        .collect();
    works.sort_by_key(|work| {
        (
            work.id != master,
            s.branches[&work.branch_id].meta.created_at,
            work.id,
        )
    });
    Ok(works
        .into_iter()
        .enumerate()
        .map(|(index, work)| ResearchBranchScope {
            handle: format!("B{}", index + 1),
            branch_id: work.branch_id,
            work_node_id: work.id,
            parent_work_node_id: s
                .work_edges
                .values()
                .find(|edge| edge.from == work.id && edge.kind == WorkEdgeKind::BranchesFrom)
                .map(|edge| edge.to),
            version: work.meta.version,
            name: s.branches[&work.branch_id].name.clone(),
            status: work.status,
            outcome: work.outcome,
            active: work.id == current,
            paused: s.human_control.as_ref().is_some_and(|control| {
                control.paused_branches.contains(&work.branch_id)
                    || control.cancelled_branches.contains(&work.branch_id)
            }),
        })
        .collect())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionGraph {
    pub runs: Vec<Run>,
    pub bindings: Vec<ExecutionBinding>,
    pub checkpoints: Vec<ExecutionCheckpoint>,
    pub model_invocations: Vec<ModelInvocation>,
    pub tool_invocations: Vec<ToolInvocation>,
    pub events: Vec<DomainEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResumeDescriptor {
    pub checkpoint: ExecutionCheckpoint,
    pub plan: PlanRevision,
    pub context: Option<ContextSnapshot>,
    pub run: Option<Run>,
    pub pending_gates: Vec<GateId>,
    pub uncertain_tools: Vec<ToolInvocationId>,
    pub can_execute: bool,
}

pub(crate) fn revision(
    s: &mut ProjectState,
    branch: BranchId,
    ctx: &CommandContext,
) -> Result<PlanRevision> {
    let b = s
        .branches
        .get(&branch)
        .ok_or_else(|| Error::NotFound("branch".into()))?;
    let r = PlanRevision {
        id: PlanRevisionId::new(),
        meta: Meta::new(s.project()?.id),
        branch_id: branch,
        parent_revision_id: b.head_revision_id,
        nodes: s
            .work_nodes
            .values()
            .filter(|n| n.branch_id == branch)
            .cloned()
            .collect(),
        edges: s
            .work_edges
            .values()
            .filter(|e| e.branch_id == branch)
            .cloned()
            .collect(),
        reason: ctx.reason.clone(),
        created_by: ctx.actor.clone(),
    };
    s.plan_revisions.insert(r.id, r.clone());
    let b = s
        .branches
        .get_mut(&branch)
        .ok_or_else(|| Error::NotFound("branch".into()))?;
    b.meta.update(b.meta.version)?;
    b.head_revision_id = Some(r.id);
    audit(
        s,
        ctx,
        "plan.revised",
        None,
        None,
        vec![],
        json!({"revision":r.id,"branch":branch}),
    )?;
    let checkpoint = ExecutionCheckpoint {
        id: CheckpointId::new(),
        meta: Meta::new(s.project()?.id),
        run_id: None,
        work_node_id: None,
        branch_id: branch,
        plan_revision_id: r.id,
        context_snapshot_id: None,
        boundary: CheckpointBoundary::PlanCreated,
        execution_state: json!({"schema_version":1}),
    };
    s.checkpoints.insert(checkpoint.id, checkpoint.clone());
    audit(
        s,
        ctx,
        "checkpoint.created",
        None,
        None,
        vec![],
        json!({"checkpoint":checkpoint.id}),
    )?;
    Ok(r)
}

pub(crate) fn decision_state(s: &ProjectState, work: WorkNodeId) -> Result<Value> {
    let current = node(s, work)?;
    let children: Vec<_> = s
        .work_edges
        .values()
        .filter(|edge| edge.from == work && edge.kind == WorkEdgeKind::Contains)
        .filter_map(|edge| s.work_nodes.get(&edge.to))
        .collect();
    let mut blockers = Vec::new();
    let scopes = branch_scopes(s, work)?;
    let branch_children: Vec<_> = scopes
        .iter()
        .filter(|scope| scope.parent_work_node_id == Some(work))
        .collect();
    if branch_children.iter().any(|scope| {
        !matches!(
            scope.outcome,
            Some(ResearchOutcome::Completed | ResearchOutcome::Inconclusive)
        )
    }) {
        blockers.push("unfinished independent branch research".into());
    }
    let mut obligated = BTreeSet::from([work]);
    loop {
        let before = obligated.len();
        for edge in s.work_edges.values().filter(|edge| {
            edge.kind == WorkEdgeKind::Contains && edge.branch_id == current.branch_id
        }) {
            if obligated.contains(&edge.from) {
                obligated.insert(edge.to);
            }
        }
        if obligated.len() == before {
            break;
        }
    }
    if has_pending_retrieval(s, &obligated) {
        blockers.push("unresolved retrieval consumption/rejection".into());
    }
    if children.iter().any(|child| {
        !matches!(
            child.status,
            WorkStatus::Succeeded
                | WorkStatus::Cancelled
                | WorkStatus::Superseded
                | WorkStatus::Failed
        )
    }) {
        blockers.push("unfinished child work".to_owned());
    }
    if children
        .iter()
        .filter(|child| child.status == WorkStatus::Succeeded)
        .count()
        < current.acceptance_criteria.minimum_children as usize
    {
        blockers.push("minimum completed children unmet".to_owned());
    }
    for entity in &current.acceptance_criteria.required_entities {
        if super::actions::check_entity_policy(s, *entity).is_err() {
            blockers.push(format!("required entity inaccessible: {entity:?}"));
        }
    }
    for artifact in &current.acceptance_criteria.required_artifacts {
        if !s.artifacts.contains_key(artifact) {
            blockers.push(format!("required artifact missing: {artifact}"));
        }
    }
    if s.gates
        .values()
        .any(|gate| gate.status == GateStatus::Pending)
    {
        blockers.push("unresolved Human Gate".into());
    }
    if s.work_edges
        .values()
        .filter(|edge| edge.from == work && edge.kind == WorkEdgeKind::DependsOn)
        .any(|edge| {
            s.work_nodes
                .get(&edge.to)
                .is_none_or(|dependency| dependency.status != WorkStatus::Succeeded)
        })
    {
        blockers.push("unresolved dependency".into());
    }
    let bounded = |value: &str| value.chars().take(120).collect::<String>();
    // Planning feedback competes with source notes in ordinary packing. Keep the
    // newest bounded observation in the pinned state so a no-op is observable.
    let mut planning: Vec<_> = s
        .process_memories
        .values()
        .filter(|memory| {
            memory.work_node_id == Some(work)
                && memory.namespace == MemoryNamespace::ProcessMemory
                && memory.content.starts_with("Planning observation: ")
        })
        .collect();
    planning.sort_by_key(|memory| std::cmp::Reverse(memory.meta.created_at));
    let parse_observation = |memory: &&ProcessMemory| {
        memory
            .content
            .strip_prefix("Planning observation: ")
            .filter(|value| value.len() <= 1600)
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
    };
    let last_planning_observation = planning.first().and_then(parse_observation);
    let consecutive_planning_noops = planning
        .iter()
        .take_while(|memory| {
            parse_observation(memory).is_some_and(|observation| {
                observation["created"] == false || observation["kind"] == "existing_plan"
            })
        })
        .count();
    // Share a bounded preview budget; full results remain in ProcessMemory.
    let result_count = children
        .iter()
        .filter(|child| child.status == WorkStatus::Succeeded)
        .take(12)
        .count()
        .max(1);
    let result_preview_chars = 800 / result_count;
    let child_result = |child: &WorkNode| {
        if child.status != WorkStatus::Succeeded {
            return None;
        }
        s.process_memories.values()
            .filter(|memory| memory.work_node_id == Some(child.id)
                && memory.namespace == MemoryNamespace::ProcessMemory
                && memory.content.starts_with("Work result observation: "))
            .max_by_key(|memory| memory.meta.created_at)
            .and_then(|memory| {
                let value: Value = serde_json::from_str(memory.content.strip_prefix("Work result observation: ")?).ok()?;
                let summary = value["summary"].as_str().unwrap_or("");
                Some(json!({"memory_id":memory.id,"summary":summary.chars().take(result_preview_chars).collect::<String>(),"summary_truncated":summary.chars().count()>result_preview_chars,"source_artifacts":value["source_artifacts"].as_array().map(|ids| ids.iter().take(1).collect::<Vec<_>>()),"scientific_status":"unreviewed Lead task result"}))
            })
    };
    Ok(
        json!({"work":work,"branch":current.branch_id,"plan_revision":s.branches[&current.branch_id].head_revision_id,
        "branch_children":branch_children.iter().map(|scope|json!({"handle":scope.handle,"name":scope.name,"status":scope.status,"outcome":scope.outcome,"result":s.events.iter().rev().find(|event|event.event_type=="BranchResultAvailable"&&event.payload["source_work_node_id"]==json!(scope.work_node_id)&&event.payload["target_work_node_id"]==json!(work)).map(|event|json!({"event_id":event.id,"summary":event.payload["summary"].as_str().unwrap_or("").chars().take(500).collect::<String>(),"source_artifacts":event.payload["source_artifacts"].as_array().map(|ids|ids.iter().take(8).collect::<Vec<_>>())}))})).collect::<Vec<_>>(),
        "last_planning_observation":last_planning_observation,"consecutive_planning_noops":consecutive_planning_noops,
        "ready_work":executable_children(s,work)?.iter().take(12).map(|child|json!({"id":child.id,"objective":bounded(&child.objective),"required_tools":child.capability_requirements})).collect::<Vec<_>>(),
        "children":children.iter().take(12).map(|child| json!({"id":child.id,"objective":bounded(&child.objective),"status":child.status,"outcome":child.outcome,"required_tools":child.capability_requirements,"result":child_result(child)})).collect::<Vec<_>>(),
        "child_count":children.len(),"plan_truncated":children.len()>12,
        "completion":{"ready":blockers.is_empty(),"blockers":blockers,"requires_explicit_lead_decision":true,"scientific_sufficiency":"Lead must assess actual evidence; execution readiness does not establish truth"},
        "recorded_attempts":s.tool_invocations.values().filter(|inv|inv.work_node_id==work && matches!(inv.status,InvocationStatus::Succeeded|InvocationStatus::Failed)).count(),
        "evidence_stages":{"read_outputs":s.tool_invocations.values().filter(|inv| (inv.work_node_id==work || children.iter().any(|child| child.id==inv.work_node_id)) && inv.tool=="read_source" && super::usable_required_output(s, inv) && inv.output_artifacts.iter().any(|id| s.artifacts.get(id).is_some_and(|artifact| { let metadata=artifact.metadata.get("tool_output_metadata").unwrap_or(&artifact.metadata["schema"]["tool_output_metadata"]);metadata["retrieval_consumption"].is_object() || metadata["source_content_read"].is_object() }))).count(),
        "claims":current.acceptance_criteria.required_entities.iter().filter(|entity|matches!(entity,EntityRef::Claim(_))).count(),"findings":current.acceptance_criteria.required_entities.iter().filter(|entity|matches!(entity,EntityRef::Finding(_))).count(),"read_is_not_claim_or_finding":true},
        "recovery_options":["reuse existing work/result","justify replication with repeat_work_node","propose_work_plan or propose_objective_revision for owner review","finish_research when ready or explain blocked/inconclusive"]}),
    )
}
/// Completion validates persisted obligation metadata independently of adapter names.
pub(crate) fn has_pending_retrieval(s: &ProjectState, work: &BTreeSet<WorkNodeId>) -> bool {
    let mut pending: std::collections::BTreeMap<ToolInvocationId, (WorkNodeId, BTreeSet<String>)> =
        std::collections::BTreeMap::new();
    let mut invocations: Vec<_> = s
        .tool_invocations
        .values()
        .filter(|inv| inv.status == InvocationStatus::Succeeded)
        .collect();
    invocations.sort_by_key(|inv| inv.started_at);
    for inv in invocations {
        for metadata in inv
            .output_artifacts
            .iter()
            .filter_map(|id| s.artifacts.get(id))
            .filter_map(|artifact| {
                artifact
                    .metadata
                    .get("tool_output_metadata")
                    .or_else(|| artifact.metadata["schema"].get("tool_output_metadata"))
            })
            .filter(|metadata| {
                metadata["reference_valid"] != false && metadata["stall_detected"] != true
            })
        {
            if metadata["retrieval_batch"].is_object() && work.contains(&inv.work_node_id) {
                pending.retain(|_, (owner, _)| *owner != inv.work_node_id);
                let batch = &metadata["retrieval_batch"];
                let id = batch["search_invocation_id"]
                    .as_str()
                    .and_then(|id| id.parse().ok())
                    .unwrap_or(inv.id);
                let handles = batch["returned"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|source| source["handle"].as_str().map(str::to_owned))
                    .collect();
                pending.insert(id, (inv.work_node_id, handles));
            }
            if let Some(id) = metadata["refinement"]["previous_search_id"]
                .as_str()
                .and_then(|id| id.parse::<ToolInvocationId>().ok())
            {
                pending.remove(&id);
            }
            if let Some(id) = metadata["retrieval_rejection"]["search_invocation_id"]
                .as_str()
                .and_then(|id| id.parse::<ToolInvocationId>().ok())
            {
                pending.remove(&id);
            }
            if let Some(id) = metadata["retrieval_consumption"]["search_invocation_id"]
                .as_str()
                .and_then(|id| id.parse::<ToolInvocationId>().ok())
            {
                if let Some((_, handles)) = pending.get_mut(&id) {
                    if let Some(handle) = metadata["retrieval_consumption"]["handle"].as_str() {
                        if handles.remove(handle) && handles.is_empty() {
                            pending.remove(&id);
                        }
                    }
                }
            }
        }
    }
    !pending.is_empty()
}
fn node(s: &ProjectState, id: WorkNodeId) -> Result<&WorkNode> {
    s.work_nodes
        .get(&id)
        .ok_or_else(|| Error::NotFound("work node".into()))
}
fn dependency(kind: WorkEdgeKind) -> bool {
    matches!(
        kind,
        WorkEdgeKind::DependsOn | WorkEdgeKind::Contains | WorkEdgeKind::Blocks
    )
}
fn reachable(s: &ProjectState, start: WorkNodeId, target: WorkNodeId) -> bool {
    let mut todo = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(id) = todo.pop() {
        if id == target {
            return true;
        }
        if seen.insert(id) {
            todo.extend(s.work_edges.values().filter_map(|e| {
                if e.kind == WorkEdgeKind::Blocks && e.to == id {
                    Some(e.from)
                } else if e.kind != WorkEdgeKind::Blocks && dependency(e.kind) && e.from == id {
                    Some(e.to)
                } else {
                    None
                }
            }));
        }
    }
    false
}
fn satisfied(s: &ProjectState, n: &WorkNode) -> bool {
    s.work_edges
        .values()
        .filter(|e| {
            (e.from == n.id && e.kind == WorkEdgeKind::DependsOn)
                || (e.to == n.id && e.kind == WorkEdgeKind::Blocks)
        })
        .all(|e| {
            node(s, if e.from == n.id { e.to } else { e.from })
                .is_ok_and(|n| n.status == WorkStatus::Succeeded)
        })
}

fn executable_children(s: &ProjectState, parent: WorkNodeId) -> Result<Vec<WorkNode>> {
    let root = node(s, parent)?;
    if matches!(
        root.status,
        WorkStatus::Succeeded | WorkStatus::Failed | WorkStatus::Cancelled | WorkStatus::Superseded
    ) || s
        .human_control
        .as_ref()
        .is_some_and(|c| matches!(c.status, ResearchStatus::Paused | ResearchStatus::Cancelled))
    {
        return Ok(vec![]);
    }
    let mut children: Vec<_> = s
        .work_edges
        .values()
        .filter(|edge| edge.from == parent && edge.kind == WorkEdgeKind::Contains)
        .filter_map(|edge| s.work_nodes.get(&edge.to))
        .filter(|work| {
            work.branch_id == root.branch_id
                && matches!(
                    work.status,
                    WorkStatus::Proposed | WorkStatus::Ready | WorkStatus::Running
                )
                && satisfied(s, work)
        })
        .filter(|work| {
            !s.human_control.as_ref().is_some_and(|c| {
                c.paused_branches.contains(&work.branch_id)
                    || c.cancelled_branches.contains(&work.branch_id)
            })
        })
        .cloned()
        .collect();
    children.sort_by_key(|work| {
        (
            work.status != WorkStatus::Running,
            work.meta.created_at,
            work.id,
        )
    });
    Ok(children)
}

impl Core {
    /// Derived from the durable work graph and dependency state, without a second queue.
    pub fn executable_child_work(
        &self,
        project: ProjectId,
        parent: WorkNodeId,
    ) -> Result<Vec<WorkNode>> {
        executable_children(&self.state(project)?, parent)
    }
    /// Read a completed operational model result without re-inference or scientific promotion.
    pub fn model_output(&self, project: ProjectId, id: ModelInvocationId) -> Result<Value> {
        let state = self.state(project)?;
        let invocation = state
            .model_invocations
            .get(&id)
            .ok_or_else(|| Error::NotFound("model invocation".into()))?;
        if invocation.status != InvocationStatus::Succeeded {
            return Err(Error::InvalidTransition(
                "model invocation has no successful result".into(),
            ));
        }
        invocation
            .metadata
            .get("output")
            .cloned()
            .ok_or_else(|| Error::NotFound("persisted model output".into()))
    }
    pub fn process_graph(
        &self,
        project: ProjectId,
        branch: BranchId,
        revision_id: Option<PlanRevisionId>,
    ) -> Result<ResearchProcessGraph> {
        let s = self.state(project)?;
        let branch = s
            .branches
            .get(&branch)
            .cloned()
            .ok_or_else(|| Error::NotFound("branch".into()))?;
        let id = revision_id
            .or(branch.head_revision_id)
            .ok_or_else(|| Error::NotFound("plan".into()))?;
        let revision = s
            .plan_revisions
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::NotFound("plan".into()))?;
        if revision.branch_id != branch.id {
            return Err(Error::ProjectMismatch);
        }
        Ok(ResearchProcessGraph { branch, revision })
    }
    pub fn execution_graph(&self, project: ProjectId) -> Result<ExecutionGraph> {
        let s = self.state(project)?;
        let mut checkpoints: Vec<_> = s.checkpoints.into_values().collect();
        checkpoints.sort_by_key(|c| c.meta.created_at);
        let mut model_invocations: Vec<_> = s.model_invocations.into_values().collect();
        model_invocations.sort_by_key(|i| i.started_at);
        let mut tool_invocations: Vec<_> = s.tool_invocations.into_values().collect();
        tool_invocations.sort_by_key(|i| i.started_at);
        Ok(ExecutionGraph {
            runs: s.runs.into_values().collect(),
            bindings: s.execution_bindings.into_values().collect(),
            checkpoints,
            model_invocations,
            tool_invocations,
            events: s.events,
        })
    }
    pub fn set_work_contract(
        &self,
        project: ProjectId,
        id: WorkNodeId,
        expected: u64,
        contract: OutputContract,
        ctx: CommandContext,
    ) -> Result<WorkNode> {
        ctx.actor.require_human()?;
        self.mutate(project, |s| {
            active(s)?;
            ctx.validate(s)?;
            let w = node(s, id)?;
            if matches!(
                w.status,
                WorkStatus::Succeeded | WorkStatus::Cancelled | WorkStatus::Superseded
            ) {
                return Err(Error::InvalidTransition(
                    "terminal contract immutable".into(),
                ));
            }
            if w.granularity == WorkGranularity::Atomic
                && contract.required_entities.is_empty()
                && contract.required_artifacts.is_empty()
            {
                return Err(Error::Validation(
                    "atomic output contract cannot be empty".into(),
                ));
            }
            for entity in &contract.required_entities {
                s.check(*entity)?;
            }
            for artifact in &contract.required_artifacts {
                s.check(EntityRef::Artifact(*artifact))?;
            }
            let w = s
                .work_nodes
                .get_mut(&id)
                .ok_or_else(|| Error::NotFound("work".into()))?;
            w.meta.update(expected)?;
            w.acceptance_criteria = contract;
            let w = w.clone();
            revision(s, w.branch_id, &ctx)?;
            audit(
                s,
                &ctx,
                "work.contract_revised",
                None,
                None,
                vec![],
                json!({"work":id,"version":w.meta.version}),
            )?;
            Ok(w)
        })
    }
    /// Reattach at the exact persisted boundary without generating a new plan or calling a model.
    pub fn resume_checkpoint(
        &self,
        project: ProjectId,
        id: CheckpointId,
    ) -> Result<ResumeDescriptor> {
        self.mutate(project, |s| {
            active(s)?;
            let checkpoint = s
                .checkpoints
                .get(&id)
                .cloned()
                .ok_or_else(|| Error::NotFound("checkpoint".into()))?;
            let plan = s
                .plan_revisions
                .get(&checkpoint.plan_revision_id)
                .cloned()
                .ok_or_else(|| Error::NotFound("plan".into()))?;
            let context = checkpoint
                .context_snapshot_id
                .map(|id| {
                    s.context_snapshots
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| Error::NotFound("snapshot".into()))
                })
                .transpose()?;
            let run = checkpoint
                .run_id
                .map(|id| {
                    s.runs
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| Error::NotFound("run".into()))
                })
                .transpose()?;
            let ctx = CommandContext {
                actor: Actor::System,
                reason: "Recover committed execution boundary".into(),
                run_id: checkpoint.run_id,
                correlation_id: None,
                causation_id: None,
            };
            let interrupted: Vec<_> = s
                .model_invocations
                .values()
                .filter(|i| {
                    Some(i.run_id) == checkpoint.run_id && i.status == InvocationStatus::Running
                })
                .map(|i| i.id)
                .collect();
            for id in interrupted {
                let invocation = s
                    .model_invocations
                    .get_mut(&id)
                    .ok_or_else(|| Error::NotFound("invocation".into()))?;
                invocation.status = InvocationStatus::Failed;
                invocation.meta.update(invocation.meta.version)?;
                invocation.finished_at = Some(Utc::now());
                invocation.latency_ms = Some(
                    (Utc::now() - invocation.started_at)
                        .num_milliseconds()
                        .max(0) as u64,
                );
                let strong_route = invocation.metadata["route"] == "strong";
                invocation.metadata =
                    json!({"schema_version":1,"error_code":"process_interrupted"});
                if strong_route {
                    invocation.metadata["route"] = json!("strong");
                }
                audit(
                    s,
                    &ctx,
                    "ModelInvocationInterrupted",
                    None,
                    None,
                    vec![],
                    json!({"id":id}),
                )?;
            }
            // An interrupted tool may have performed a side effect. Never replay it implicitly.
            let uncertain_tools: Vec<_> = s
                .tool_invocations
                .values()
                .filter(|i| {
                    Some(i.run_id) == checkpoint.run_id && i.status == InvocationStatus::Running
                })
                .map(|i| i.id)
                .collect();
            let pending_gates: Vec<_> = s
                .gates
                .values()
                .filter(|g| g.run_id == checkpoint.run_id && g.status == GateStatus::Pending)
                .map(|g| g.id)
                .collect();
            let control = s.human_control.clone().unwrap_or_default();
            let branch_blocked = control.paused_branches.contains(&checkpoint.branch_id)
                || control.cancelled_branches.contains(&checkpoint.branch_id);
            let can_execute = !matches!(
                control.status,
                ResearchStatus::Paused | ResearchStatus::Cancelled
            ) && !branch_blocked
                && pending_gates.is_empty()
                && uncertain_tools.is_empty()
                && run.as_ref().is_some_and(|r| r.status == RunStatus::Running)
                && checkpoint
                    .work_node_id
                    .and_then(|id| s.work_nodes.get(&id))
                    .is_some_and(|n| matches!(n.status, WorkStatus::Ready | WorkStatus::Running));
            audit(
                s,
                &ctx,
                "checkpoint.resumed",
                None,
                None,
                vec![],
                json!({"checkpoint":id,"can_execute":can_execute}),
            )?;
            Ok(ResumeDescriptor {
                checkpoint,
                plan,
                context,
                run,
                pending_gates,
                uncertain_tools,
                can_execute,
            })
        })
    }
    pub fn record_routing_usage(
        &self,
        project: ProjectId,
        run: RunId,
        usage: Value,
        telemetry: Value,
        ctx: CommandContext,
    ) -> Result<Run> {
        safe_metadata(&usage)?;
        safe_metadata(&telemetry)?;
        self.mutate(project, |s| {
            ctx.validate(s)?;
            if !s.execution_bindings.contains_key(&run) {
                return Err(Error::NotFound("binding".into()));
            }
            if !matches!(ctx.actor, Actor::System | Actor::Human(_)) {
                return Err(Error::PermissionDenied);
            }
            let r = s
                .runs
                .get_mut(&run)
                .ok_or_else(|| Error::NotFound("run".into()))?;
            let mut metadata = r.provider_metadata.as_object().cloned().unwrap_or_default();
            metadata.insert("schema_version".into(), json!(1));
            metadata.insert(
                "runtime_v2".into(),
                json!({"usage":usage,"telemetry":telemetry}),
            );
            r.meta.update(r.meta.version)?;
            r.provider_metadata = Value::Object(metadata);
            let r = r.clone();
            let mut context = ctx.clone();
            context.run_id = Some(run);
            audit(
                s,
                &context,
                "routing.recorded",
                None,
                None,
                vec![],
                json!({"run":run,"usage":usage}),
            )?;
            Ok(r)
        })
    }
    pub fn compile_context(
        &self,
        project: ProjectId,
        work: WorkNodeId,
        revision_id: PlanRevisionId,
        pinned: Vec<EntityRef>,
        ctx: CommandContext,
    ) -> Result<ContextSnapshot> {
        self.compile_context_manifest(
            project,
            ContextCompileRequest {
                work_node_id: work,
                plan_revision_id: revision_id,
                pinned,
                instruction_version: "context-v2.1".into(),
                skill_versions: vec![],
            },
            ctx,
        )
    }
    pub fn provider_context(
        &self,
        project: ProjectId,
        id: ContextSnapshotId,
    ) -> Result<ProviderContext> {
        let s = self.state(project)?;
        let snapshot = s
            .context_snapshots
            .get(&id)
            .ok_or_else(|| Error::NotFound("snapshot".into()))?;
        let plan = s
            .plan_revisions
            .get(&snapshot.plan_revision_id)
            .ok_or_else(|| Error::NotFound("plan".into()))?;
        let work = plan
            .nodes
            .iter()
            .find(|n| n.id == snapshot.work_node_id)
            .ok_or_else(|| Error::NotFound("historical work".into()))?;
        let process_notes = snapshot
            .process_memory_ids
            .iter()
            .map(|id| {
                s.process_memories
                    .get(id)
                    .map(|m| m.content.clone())
                    .ok_or_else(|| Error::NotFound("memory".into()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(ProviderContext {
            schema_version: 1,
            snapshot_id: id,
            objective: work.objective.clone(),
            entries: snapshot.entries.clone(),
            process_notes,
            instruction_version: snapshot.instruction_version.clone(),
            skill_versions: snapshot.skill_versions.clone(),
        })
    }
    pub fn compile_context_manifest(
        &self,
        project: ProjectId,
        request: ContextCompileRequest,
        ctx: CommandContext,
    ) -> Result<ContextSnapshot> {
        let work = request.work_node_id;
        let revision_id = request.plan_revision_id;
        let pinned = request.pinned;
        text(&request.instruction_version, "instruction version")?;
        for version in &request.skill_versions {
            text(version, "skill version")?;
        }
        let initial = self.state(project)?;
        let query = initial
            .plan_revisions
            .get(&revision_id)
            .and_then(|p| p.nodes.iter().find(|n| n.id == work))
            .ok_or_else(|| Error::NotFound("historical work".into()))?
            .objective
            .clone();
        let search_hits = self.storage.search(project, &query, 50)?;
        let retrieval_candidate_count = u32::try_from(search_hits.len()).unwrap_or(u32::MAX);
        self.mutate(project, |s| {
            ctx.validate(s)?;
            active(s)?;
            node(s, work)?;
            let plan = s
                .plan_revisions
                .get(&revision_id)
                .ok_or_else(|| Error::NotFound("plan".into()))?;
            if !plan.nodes.iter().any(|n| n.id == work) {
                return Err(Error::Validation("work not in pinned revision".into()));
            }
            let historical=plan.nodes.iter().find(|n|n.id==work).ok_or_else(||Error::NotFound("historical work".into()))?;
            let budget = historical.budget.context_tokens;
            let terms: BTreeSet<_> = historical
                .objective
                .split_whitespace()
                .map(|w| w.to_lowercase())
                .collect();
            let mut candidates = vec![];
            let mut seen = BTreeSet::new();
            let context_access = |e: EntityRef| -> Result<()> {
                super::actions::check_entity_policy(s, e)?;
                if let EntityRef::Paper(id) = e {
                    for link in s.paper_sources.iter().filter(|link| link.paper_id == id) {
                        super::actions::check_entity_policy(s, EntityRef::Source(link.source_id))?;
                    }
                }
                Ok(())
            };
            let render = |e: EntityRef| -> Result<String> {
                context_access(e)?;
                Ok(match e {
                    EntityRef::Question(id) => format!(
                        "{} {}",
                        s.questions[&id].title, s.questions[&id].description
                    ),
                    EntityRef::Source(id) => {
                        format!("{} {}", s.sources[&id].title, s.sources[&id].locator)
                    }
                    EntityRef::Paper(id) => {
                        format!("{} {}", s.papers[&id].title, s.papers[&id].abstract_text)
                    }
                    EntityRef::Claim(id) => s.claims[&id].text.clone(),
                    EntityRef::Hypothesis(id) => s.hypotheses[&id].statement.clone(),
                    EntityRef::Experiment(id) => s.experiments[&id].objective.clone(),
                    EntityRef::ExperimentRun(id) => {
                        format!("experiment {:?}", s.experiment_runs[&id].status)
                    }
                    EntityRef::Finding(id) => s.findings[&id].summary.clone(),
                    EntityRef::Artifact(id) => format!(
                        "{} {} {}",
                        s.artifacts[&id].name, s.artifacts[&id].path, s.artifacts[&id].content_hash
                    ),
                })
            };
            for e in &pinned {
                if seen.insert(*e) {
                    candidates.push((*e, true, render(*e)?, None));
                }
            }
            for r in s.relations.values() {
                for (a, b) in [(r.from_entity, r.to_entity), (r.to_entity, r.from_entity)] {
                    if pinned.contains(&a) && seen.insert(b) && context_access(b).is_ok() {
                        candidates.push((b, false, render(b)?, None));
                    }
                }
            }
            let all = search_hits.into_iter();
            let mut ranked = vec![];
            for e in all {
                if seen.contains(&e) || context_access(e).is_err() {
                    continue;
                }
                let t = render(e)?;
                let score = t
                    .split_whitespace()
                    .filter(|w| terms.contains(&w.to_lowercase()))
                    .count();
                if score > 0 {
                    ranked.push((score, e, t));
                }
            }
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            for (score, e, t) in ranked.into_iter().take(50) {
                candidates.push((e, false, t, Some(score as u32)));
            }
            let objective=plan.nodes.iter().find(|n|n.id==work).ok_or_else(||Error::NotFound("historical work".into()))?.objective.clone();
            let mut used=u32::try_from(objective.len()+request.instruction_version.len()+request.skill_versions.iter().map(String::len).sum::<usize>()+256).unwrap_or(u32::MAX);
            let constraint_content = s.human_control.as_ref().filter(|control| !control.constraints.is_empty())
                .map(|control| format!("Human constraint snapshot: {}", json!({"version":control.version,"constraints":control.constraints})));
            let mut decision = decision_state(s, work)?;
            let pinned_cost: u32 = candidates.iter().filter(|(_, pinned, _, _)| *pinned)
                .map(|(_, _, text, _)| u32::try_from(text.len()+192).unwrap_or(u32::MAX))
                .fold(0u32, u32::saturating_add);
            let constraint_cost = constraint_content.as_ref().map_or(0, |text| u32::try_from(text.len()+16).unwrap_or(u32::MAX));
            let decision_budget = budget.saturating_sub(used).saturating_sub(pinned_cost).saturating_sub(constraint_cost);
            // Result previews are optional context, unlike the question, Human
            // constraints and lifecycle ledger. Preserve a durable lookup reference
            // when previews would crowd mandatory context out of the snapshot.
            let mut decision_content = format!("Decision state: {}", decision);
            if decision_content.len().saturating_add(16) > decision_budget as usize {
                let latest_result = s.process_memories.values().filter(|memory| {
                    memory.content.starts_with("Work result observation: ")
                        && decision["children"].as_array().is_some_and(|children| children.iter().any(|child| child["result"]["memory_id"] == json!(memory.id)))
                }).max_by_key(|memory| memory.meta.created_at).and_then(|memory| {
                    let value: Value = serde_json::from_str(memory.content.strip_prefix("Work result observation: ")?).ok()?;
                    Some((memory.id, value["summary"].as_str()?.to_owned()))
                });
                if let Some(children) = decision["children"].as_array_mut() {
                    for child in children {
                        if let Some(result) = child["result"].as_object_mut() {
                            result.retain(|key, _| key == "memory_id");
                            result.insert("preview_omitted".into(), json!(true));
                        }
                        if child["status"] == "succeeded" {
                            child["objective"] = json!(child["objective"].as_str().unwrap_or("").chars().take(40).collect::<String>());
                        }
                    }
                }
                decision["completed_objectives_truncated"] = json!(true);
                decision["task_results_scientific_status"] = json!("unreviewed Lead results");
                let compact = decision.clone();
                for preview_chars in [400, 200, 100, 0] {
                    decision = compact.clone();
                    if let Some((id, summary)) = &latest_result {
                        if preview_chars > 0 {
                            if let Some(children) = decision["children"].as_array_mut() {
                                for child in children {
                                    if child["result"]["memory_id"] == json!(id) {
                                        child["result"] = json!({"memory_id":id,"summary":summary.chars().take(preview_chars).collect::<String>(),"summary_truncated":summary.chars().count()>preview_chars});
                                    }
                                }
                            }
                        }
                    }
                    decision_content = format!("Decision state: {}", decision);
                    if decision_content.len().saturating_add(16) <= decision_budget as usize { break; }
                }
            }
            used = used.saturating_add(u32::try_from(decision_content.len()+16).unwrap_or(u32::MAX));
            if let Some(content) = &constraint_content {
                used = used.saturating_add(u32::try_from(content.len()+16).unwrap_or(u32::MAX));
            }
            if used>budget{return Err(Error::Validation("context envelope exceeds budget".into()));}
            let mut entries = vec![];
            let mut retrieval_selected = vec![];
            for (entity, pinned, text, lexical_score) in candidates {
                let tokens=u32::try_from(text.len()+192).unwrap_or(u32::MAX);
                if tokens > budget.saturating_sub(used) {
                    if pinned {
                        return Err(Error::Validation(format!("pinned context exceeds budget: {entity:?} needs {tokens}, remaining {} (envelope {})", budget.saturating_sub(used), decision_content.len())));
                    }
                    continue;
                }
                used += tokens;
                entries.push(ContextEntry {
                    reference: VersionedEntity {
                        entity,
                        version: s.meta(entity)?.version,
                    },
                    text,
                    pinned,
                    estimated_tokens: tokens,
                });
                if let Some(lexical_score) = lexical_score {
                    retrieval_selected.push(ContextRetrievalSelection {
                        entity,
                        lexical_score,
                    });
                }
            }
            let mut memory_work_nodes = BTreeSet::from([work]);
            loop {
                let mut added = false;
                for edge in s.work_edges.values().filter(|e| e.kind == WorkEdgeKind::Contains && e.branch_id == s.work_nodes[&work].branch_id) {
                    if memory_work_nodes.contains(&edge.to) && memory_work_nodes.insert(edge.from) {
                        added = true;
                    }
                }
                if !added { break; }
            }
            // A parent must see the results of completed contained/dependent work,
            // not only the fact that its child finished. Never pull sibling-branch memory.
            let mut result_work_nodes = BTreeSet::from([work]);
            loop {
                let mut added = false;
                for edge in s.work_edges.values().filter(|edge| edge.branch_id == s.work_nodes[&work].branch_id) {
                    let result = match edge.kind {
                        WorkEdgeKind::Contains if result_work_nodes.contains(&edge.from) => Some(edge.to),
                        WorkEdgeKind::DependsOn if result_work_nodes.contains(&edge.from) => Some(edge.to),
                        WorkEdgeKind::Blocks if result_work_nodes.contains(&edge.to) => Some(edge.from),
                        _ => None,
                    };
                    if let Some(id) = result.filter(|id| s.work_nodes.get(id).is_some_and(|node| node.status == WorkStatus::Succeeded)) {
                        added |= result_work_nodes.insert(id);
                    }
                }
                if !added { break; }
            }
            memory_work_nodes.extend(result_work_nodes);
            let mut process_memories: Vec<_> = s.process_memories.values().filter(|m| {
                m.namespace == MemoryNamespace::ProcessMemory && !m.content.starts_with("Human constraint snapshot: ") && !m.content.starts_with("Decision state: ") && m.work_node_id.is_some_and(|id| memory_work_nodes.contains(&id))
            }).collect();
            process_memories.sort_by(|a, b| {
                let a_result = a.content.starts_with("Source content observation: ");
                let b_result = b.content.starts_with("Source content observation: ");
                let a_current = a.work_node_id == Some(work);
                let b_current = b.work_node_id == Some(work);
                if historical.granularity == WorkGranularity::Composite {
                    b_result.cmp(&a_result).then_with(|| b_current.cmp(&a_current)).then_with(|| b.meta.created_at.cmp(&a.meta.created_at))
                } else {
                    b_current.cmp(&a_current).then_with(|| b.meta.created_at.cmp(&a.meta.created_at))
                }
            });
            let mut process_memory_ids = vec![];
            for m in process_memories {
                let tokens=u32::try_from(m.content.len()+16).unwrap_or(u32::MAX);
                if tokens <= budget.saturating_sub(used) {
                    used += tokens;
                    process_memory_ids.push(m.id);
                }
            }
            let decision_index = usize::from(constraint_content.is_some());
            if let Some(content) = constraint_content {
                let memory = ProcessMemory { id: ProcessMemoryId::new(), meta: Meta::new(project), namespace: MemoryNamespace::ProcessMemory,
                    work_node_id: Some(work), content, created_by: ctx.actor.clone() };
                process_memory_ids.insert(0, memory.id);
                s.process_memories.insert(memory.id, memory);
            }
            let memory = ProcessMemory { id: ProcessMemoryId::new(), meta: Meta::new(project), namespace: MemoryNamespace::ProcessMemory,
                work_node_id: Some(work), content: decision_content, created_by: ctx.actor.clone() };
            process_memory_ids.insert(decision_index, memory.id);
            s.process_memories.insert(memory.id, memory);
            let snap = ContextSnapshot {
                id: ContextSnapshotId::new(),
                meta: Meta::new(project),
                work_node_id: work,
                plan_revision_id: revision_id,
                entries,
                process_memory_ids,
                retrieval_query: query.clone(),
                retrieval_candidate_count,
                retrieval_selected,
                instruction_version: request.instruction_version,
                skill_versions: request.skill_versions,
                budget_tokens: budget,
                estimated_tokens: used,
                estimation_method:
                    "UTF-8 bytes plus envelope reserve; conservative byte-token heuristic, not measured provider usage".into(),
            };
            s.context_snapshots.insert(snap.id, snap.clone());
            audit(
                s,
                &ctx,
                "context.compiled",
                None,
                None,
                vec![],
                json!({"snapshot":snap.id,"estimated_tokens":used}),
            )?;
            Ok(snap)
        })
    }
    pub fn record_model_invocation(
        &self,
        project: ProjectId,
        mut record: ModelInvocation,
        ctx: CommandContext,
    ) -> Result<ModelInvocation> {
        self.mutate(project, |s| {
            ctx.validate(s)?;
            safe_metadata(&record.metadata)?;
            safe_metadata(&record.output_schema)?;
            let b = s
                .execution_bindings
                .get(&record.run_id)
                .ok_or_else(|| Error::NotFound("run binding".into()))?;
            let snap = s
                .context_snapshots
                .get(&record.context_snapshot_id)
                .ok_or_else(|| Error::NotFound("context snapshot".into()))?;
            if record.status == InvocationStatus::Running
                && !s.model_invocations.contains_key(&record.id)
            {
                if let Some(limit) = s
                    .human_control
                    .as_ref()
                    .and_then(|c| c.budget.max_lead_calls)
                {
                    if s.model_invocations.len() as u64 >= limit {
                        return Err(Error::Conflict("project Lead call budget exhausted".into()));
                    }
                }
                let provider_name = record.provider.to_ascii_lowercase();
                let strong_call = record.provider == "local-test-lead"
                    || provider_name.contains("strong")
                    || record.metadata["route"] == "strong";
                if strong_call {
                    if let Some(limit) = s
                        .human_control
                        .as_ref()
                        .and_then(|c| c.budget.max_strong_model_calls)
                    {
                        let used = s
                            .model_invocations
                            .values()
                            .filter(|i| {
                                let name = i.provider.to_ascii_lowercase();
                                i.provider == "local-test-lead"
                                    || name.contains("strong")
                                    || i.metadata["route"] == "strong"
                            })
                            .count() as u64;
                        if used >= limit {
                            return Err(Error::Conflict(
                                "StrongModelBudgetExceeded: project strong model call budget exhausted".into(),
                            ));
                        }
                    }
                }
                super::human_control::require_runtime_progress(s, record.run_id)?;
                if let Some(limit) = s
                    .human_control
                    .as_ref()
                    .and_then(|c| c.budget.max_elapsed_seconds)
                {
                    let start = s
                        .human_control
                        .as_ref()
                        .and_then(|c| c.directives.first())
                        .map(|d| d.created_at)
                        .unwrap_or(s.project()?.meta.created_at);
                    if (Utc::now() - start).num_seconds().max(0) as u64 >= limit {
                        return Err(Error::Conflict(
                            "project elapsed-time budget exhausted".into(),
                        ));
                    }
                }
                let work = node(s, record.work_node_id)?;
                if s.runs
                    .get(&record.run_id)
                    .is_none_or(|r| r.status != RunStatus::Running)
                    || s.model_invocations
                        .values()
                        .filter(|i| i.work_node_id == work.id)
                        .count()
                        >= work.budget.max_model_calls as usize
                {
                    return Err(Error::Conflict(
                        "model invocation budget or run lifecycle".into(),
                    ));
                }
            }
            if record.meta.project_id != project
                || b.work_node_id != record.work_node_id
                || snap.work_node_id != b.work_node_id
                || snap.plan_revision_id != b.plan_revision_id
            {
                return Err(Error::ProjectMismatch);
            }
            if let Some(old) = s.model_invocations.get(&record.id) {
                let mut normalized = record.clone();
                normalized.meta = old.meta.clone();
                if normalized == *old {
                    return Ok(old.clone());
                }
                record.meta = old.meta.clone();
                record.meta.update(old.meta.version)?;
                if old.status != InvocationStatus::Running
                    || old.run_id != record.run_id
                    || old.context_snapshot_id != record.context_snapshot_id
                    || old.started_at != record.started_at
                    || old.work_node_id != record.work_node_id
                    || old.provider != record.provider
                    || (old.model != "pending" && old.model != record.model)
                    || (old.model_revision != "pending" && old.model_revision != record.model_revision)
                    || (old.runtime != "pending" && old.runtime != record.runtime)
                    || old.instruction_version != record.instruction_version
                    || (old.metadata["route"] == "strong" && record.metadata["route"] != "strong")
                {
                    return Err(Error::Conflict("invocation history".into()));
                }
            }
            if record.status != InvocationStatus::Running
                && (record.finished_at.is_none() || record.latency_ms.is_none())
            {
                return Err(Error::Validation("finished invocation needs timing".into()));
            }
            if !s.model_invocations.contains_key(&record.id) {
                if record.status != InvocationStatus::Running {
                    return Err(Error::InvalidTransition(
                        "invocation must begin before completion".into(),
                    ));
                }
                record.meta = Meta::new(project);
            }
            s.model_invocations.insert(record.id, record.clone());
            audit(
                s,
                &ctx,
                match record.status {
                    InvocationStatus::Running => "ModelInvocationStarted",
                    InvocationStatus::Succeeded => "ModelInvocationCompleted",
                    InvocationStatus::Failed => "ModelInvocationFailed",
                    InvocationStatus::Cancelled => "ModelInvocationCancelled",
                },
                None,
                None,
                vec![],
                json!({"id":record.id,"status":record.status}),
            )?;
            Ok(record)
        })
    }
    pub fn update_model_invocation_trace(
        &self,
        project: ProjectId,
        id: ModelInvocationId,
        execution: Value,
        ctx: CommandContext,
    ) -> Result<ModelInvocation> {
        self.mutate(project, |s| {
            ctx.validate(s)?;
            safe_metadata(&execution)?;
            let mut invocation = s
                .model_invocations
                .get(&id)
                .cloned()
                .ok_or_else(|| Error::NotFound("model invocation".into()))?;
            if !invocation.metadata["diagnostic_trace"].is_object() {
                return Err(Error::InvalidTransition(
                    "diagnostic trace is disabled".into(),
                ));
            }
            invocation.metadata["diagnostic_trace"]["execution"] = execution;
            invocation.meta.update(invocation.meta.version)?;
            s.model_invocations.insert(id, invocation.clone());
            audit(
                s,
                &ctx,
                "model_invocation.trace_updated",
                None,
                None,
                vec![],
                json!({"id":id}),
            )?;
            Ok(invocation)
        })
    }
    pub fn record_tool_invocation(
        &self,
        project: ProjectId,
        mut record: ToolInvocation,
        ctx: CommandContext,
    ) -> Result<ToolInvocation> {
        self.mutate(project, |s| {
            ctx.validate(s)?;
            let b = s
                .execution_bindings
                .get(&record.run_id)
                .ok_or_else(|| Error::NotFound("binding".into()))?;
            if record.status == InvocationStatus::Running
                && !s.tool_invocations.contains_key(&record.id)
            {
                if let Some(limit) = s
                    .human_control
                    .as_ref()
                    .and_then(|c| c.budget.max_tool_calls)
                {
                    if s.tool_invocations.len() as u64 >= limit {
                        return Err(Error::Conflict("project tool call budget exhausted".into()));
                    }
                }
                super::human_control::require_runtime_progress(s, record.run_id)?;
                if let Some(limit) = s
                    .human_control
                    .as_ref()
                    .and_then(|c| c.budget.max_elapsed_seconds)
                {
                    let start = s
                        .human_control
                        .as_ref()
                        .and_then(|c| c.directives.first())
                        .map(|d| d.created_at)
                        .unwrap_or(s.project()?.meta.created_at);
                    if (Utc::now() - start).num_seconds().max(0) as u64 >= limit {
                        return Err(Error::Conflict(
                            "project elapsed-time budget exhausted".into(),
                        ));
                    }
                }
                let work = node(s, record.work_node_id)?;
                if s.runs
                    .get(&record.run_id)
                    .is_none_or(|r| r.status != RunStatus::Running)
                    || s.tool_invocations
                        .values()
                        .filter(|i| i.work_node_id == work.id)
                        .count()
                        >= work.budget.max_tool_calls as usize
                {
                    return Err(Error::Conflict(
                        "tool invocation budget or run lifecycle".into(),
                    ));
                }
            }
            if record.meta.project_id != project || b.work_node_id != record.work_node_id {
                return Err(Error::ProjectMismatch);
            }
            for a in record
                .input_artifacts
                .iter()
                .chain(&record.output_artifacts)
            {
                s.check(EntityRef::Artifact(*a))?;
            }
            if let Some(old) = s.tool_invocations.get(&record.id) {
                let mut normalized = record.clone();
                normalized.meta = old.meta.clone();
                if normalized == *old {
                    return Ok(old.clone());
                }
                record.meta = old.meta.clone();
                record.meta.update(old.meta.version)?;
                if old.status != InvocationStatus::Running
                    || old.run_id != record.run_id
                    || old.started_at != record.started_at
                    || old.work_node_id != record.work_node_id
                    || old.tool != record.tool
                    || old.tool_version != record.tool_version
                {
                    return Err(Error::Conflict("tool history".into()));
                }
            }
            if record.status != InvocationStatus::Running && record.finished_at.is_none() {
                return Err(Error::Validation("tool timing".into()));
            }
            if !s.tool_invocations.contains_key(&record.id) {
                if record.status != InvocationStatus::Running {
                    return Err(Error::InvalidTransition(
                        "invocation must begin before completion".into(),
                    ));
                }
                record.meta = Meta::new(project);
            }
            s.tool_invocations.insert(record.id, record.clone());
            audit(
                s,
                &ctx,
                match record.status {
                    InvocationStatus::Running => "ToolInvocationStarted",
                    InvocationStatus::Succeeded => "ToolInvocationCompleted",
                    InvocationStatus::Failed => "ToolInvocationFailed",
                    InvocationStatus::Cancelled => "ToolInvocationCancelled",
                },
                None,
                None,
                vec![],
                json!({"id":record.id,"status":record.status}),
            )?;
            Ok(record)
        })
    }
    pub fn create_candidate(
        &self,
        project: ProjectId,
        mut candidate: CandidateClaim,
        ctx: CommandContext,
    ) -> Result<CandidateClaim> {
        text(&candidate.text, "candidate text")?;
        text(&candidate.source_quote, "source quote")?;
        self.mutate(project, |s| {
            ctx.validate(s)?;
            s.check(EntityRef::Paper(candidate.paper_id))?;
            super::actions::check_entity_policy(s, EntityRef::Source(candidate.source_id))?;
            if !s
                .paper_sources
                .iter()
                .any(|p| p.paper_id == candidate.paper_id && p.source_id == candidate.source_id)
            {
                return Err(Error::Validation(
                    "candidate source not attached to paper".into(),
                ));
            }
            if candidate.promoted_claim_id.is_some()
                || s.candidate_claims.contains_key(&candidate.id)
            {
                return Err(Error::Conflict("new candidate required".into()));
            }
            candidate.meta = Meta::new(project);
            s.candidate_claims.insert(candidate.id, candidate.clone());
            audit(
                s,
                &ctx,
                "candidate.created",
                None,
                None,
                vec![],
                json!({"candidate":candidate.id}),
            )?;
            Ok(candidate)
        })
    }
    /// Promotion is human authorized and requires an exact source quote supplied by a trusted source reader.
    pub fn promote_candidate(
        &self,
        project: ProjectId,
        id: CandidateClaimId,
        source_text: &str,
        ctx: CommandContext,
    ) -> Result<Claim> {
        ctx.actor.require_human()?;
        self.mutate(project, |s| {
            ctx.validate(s)?;
            let candidate = s
                .candidate_claims
                .get(&id)
                .cloned()
                .ok_or_else(|| Error::NotFound("candidate".into()))?;
            if let Some(id) = candidate.promoted_claim_id {
                return s
                    .claims
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| Error::NotFound("promoted claim".into()));
            }
            if !source_text.contains(&candidate.source_quote)
                || !candidate.source_quote.contains(&candidate.text)
            {
                return Err(Error::Validation(
                    "candidate not grounded in exact source span".into(),
                ));
            }
            let claim = Claim {
                id: ClaimId::new(),
                meta: Meta::new(project),
                paper_id: candidate.paper_id,
                source_id: Some(candidate.source_id),
                text: candidate.text,
                normalized_text: None,
                location: candidate.location,
                created_by: ctx.actor.clone(),
            };
            s.claims.insert(claim.id, claim.clone());
            let c = s
                .candidate_claims
                .get_mut(&id)
                .ok_or_else(|| Error::NotFound("candidate".into()))?;
            c.meta.update(c.meta.version)?;
            c.promoted_claim_id = Some(claim.id);
            audit(
                s,
                &ctx,
                "candidate.promoted",
                Some(EntityRef::Claim(claim.id)),
                None,
                vec![
                    EntityRef::Paper(claim.paper_id),
                    EntityRef::Source(candidate.source_id),
                ],
                json!({"candidate":id,"claim":claim.id}),
            )?;
            Ok(claim)
        })
    }
    pub fn runtime_state(&self, project: ProjectId) -> Result<ProjectState> {
        self.state(project)
    }
    pub fn create_branch(
        &self,
        project: ProjectId,
        name: String,
        base: Option<CheckpointId>,
        ctx: CommandContext,
    ) -> Result<Branch> {
        text(&name, "branch name")?;
        let lead_run = match &ctx.actor {
            Actor::Human(_) => None,
            Actor::LeadAgent(_) => Some(ctx.run_id.ok_or_else(|| {
                branch_authorization_denied(
                    project,
                    &name,
                    &ctx.actor,
                    None,
                    None,
                    "LeadAgent mutation has no run context",
                )
            })?),
            _ => {
                return Err(branch_authorization_denied(
                    project,
                    &name,
                    &ctx.actor,
                    None,
                    None,
                    "actor kind is not permitted to create branches",
                ))
            }
        };
        self.mutate(project, |s| {
            active(s)?;
            ctx.validate(s)?;
            if matches!(ctx.actor, Actor::LeadAgent(_)) {
                super::human_control::require_agent_progress(s, &ctx.actor)?;
            }
            if let Some(run_id) = lead_run {
                let run = s.runs.get(&run_id).ok_or_else(|| {
                    branch_authorization_denied(
                        project,
                        &name,
                        &ctx.actor,
                        None,
                        None,
                        "run is not in the target project",
                    )
                })?;
                let task = s.tasks.get(&run.task_id).ok_or_else(|| {
                    branch_authorization_denied(
                        project,
                        &name,
                        &ctx.actor,
                        None,
                        None,
                        "run task is not in the target project",
                    )
                })?;
                let binding = s.execution_bindings.get(&run_id).ok_or_else(|| {
                    branch_authorization_denied(
                        project,
                        &name,
                        &ctx.actor,
                        None,
                        None,
                        "run has no active execution binding",
                    )
                })?;
                let parent = Some(binding.branch_id);
                if run.status != RunStatus::Running {
                    return Err(branch_authorization_denied(
                        project,
                        &name,
                        &ctx.actor,
                        parent,
                        parent,
                        "run is not active",
                    ));
                }
                if task.created_by != ctx.actor {
                    return Err(branch_authorization_denied(
                        project,
                        &name,
                        &ctx.actor,
                        parent,
                        parent,
                        "actor did not create the run's task",
                    ));
                }
                let bound_work = s.work_nodes.get(&binding.work_node_id).ok_or_else(|| {
                    branch_authorization_denied(
                        project,
                        &name,
                        &ctx.actor,
                        parent,
                        parent,
                        "execution binding references missing work",
                    )
                })?;
                if binding.branch_id != bound_work.branch_id {
                    return Err(branch_authorization_denied(
                        project,
                        &name,
                        &ctx.actor,
                        parent,
                        parent,
                        "execution binding branch does not match bound work branch",
                    ));
                }
            } else {
                super::human_control::authorize_project_human(s, &ctx.actor)?;
            }
            let checkpoint = base
                .map(|id| {
                    s.checkpoints
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| Error::NotFound("base checkpoint".into()))
                })
                .transpose()?;
            if let (Some(run_id), Some(checkpoint)) = (lead_run, checkpoint.as_ref()) {
                let binding = s.execution_bindings.get(&run_id).ok_or_else(|| {
                    branch_authorization_denied(
                        project,
                        &name,
                        &ctx.actor,
                        Some(checkpoint.branch_id),
                        None,
                        "run binding disappeared during base validation",
                    )
                })?;
                if checkpoint.branch_id != binding.branch_id {
                    return Err(branch_authorization_denied(
                        project,
                        &name,
                        &ctx.actor,
                        Some(binding.branch_id),
                        Some(checkpoint.branch_id),
                        "base checkpoint is outside the authorized parent branch",
                    ));
                }
            }
            let mut branch = Branch {
                id: BranchId::new(),
                meta: Meta::new(project),
                name,
                parent_branch_id: checkpoint.as_ref().map(|c| c.branch_id),
                base_checkpoint_id: base,
                head_revision_id: None,
            };
            s.branches.insert(branch.id, branch.clone());
            if let Some(c) = checkpoint {
                let plan = s
                    .plan_revisions
                    .get(&c.plan_revision_id)
                    .cloned()
                    .ok_or_else(|| Error::NotFound("base plan".into()))?;
                let mut remap = std::collections::BTreeMap::new();
                for mut n in plan.nodes {
                    let old = n.id;
                    n.id = WorkNodeId::new();
                    n.branch_id = branch.id;
                    n.meta = Meta::new(project);
                    remap.insert(old, n.id);
                    s.work_nodes.insert(n.id, n);
                }
                for mut e in plan.edges {
                    // The new fork points to its immediate origin, not copied
                    // lineage edges whose external endpoints are not in remap.
                    if e.kind==WorkEdgeKind::BranchesFrom {continue;}
                    e.id = WorkEdgeId::new();
                    e.branch_id = branch.id;
                    e.meta = Meta::new(project);
                    e.from = remap[&e.from];
                    e.to = remap[&e.to];
                    s.work_edges.insert(e.id, e);
                }
                for (origin,copy) in remap {
                    let edge=WorkEdge{id:WorkEdgeId::new(),meta:Meta::new(project),branch_id:branch.id,from:copy,to:origin,kind:WorkEdgeKind::BranchesFrom};
                    s.work_edges.insert(edge.id,edge);
                }
            }
            let r = revision(s, branch.id, &ctx)?;
            branch = s.branches[&branch.id].clone();
            branch.head_revision_id = Some(r.id);
            let mut human_directive_id=None;
            if let Actor::Human(actor_id)=&ctx.actor {
                let mut control=s.human_control.clone().unwrap_or_default();
                control.owner_id=Some(actor_id.clone());
                control.version=control.version.checked_add(1).ok_or_else(||Error::Validation("control version overflow".into()))?;
                let directive=HumanDirective{id:EventId::new(),project_id:project,actor_id:actor_id.clone(),created_at:Utc::now(),kind:HumanDirectiveKind::CreateBranch,target:Some(branch.id.to_string()),parameters:json!({"name":branch.name,"base_checkpoint_id":base}),reason:Some(ctx.reason.clone()),expected_version:None,version:control.version};
                human_directive_id=Some(directive.id);
                control.directives.push(directive.clone());
                s.human_control=Some(control);
                audit(s,&ctx,"human.directive",None,None,vec![],json!({"directive_id":directive.id,"kind":directive.kind,"target":directive.target}))?;
            }
            audit(
                s,
                &ctx,
                "branch.created",
                None,
                None,
                vec![],
                json!({"branch":branch.id,"base":base,"human_directive_id":human_directive_id}),
            )?;
            Ok(branch)
        })
    }
    pub fn create_work(
        &self,
        project: ProjectId,
        input: NewWork,
        ctx: CommandContext,
    ) -> Result<WorkNode> {
        for (v, k) in [
            (&input.title, "title"),
            (&input.objective, "objective"),
            (&input.kind, "kind"),
            (&input.context_policy_id, "context policy"),
        ] {
            text(v, k)?;
        }
        if input.budget.context_tokens == 0 {
            return Err(Error::Validation("zero context budget".into()));
        }
        self.mutate(project, |s| {
            active(s)?;
            ctx.validate(s)?;
            super::human_control::require_agent_progress(s, &ctx.actor)?;
            if let Some(limit) = s.human_control.as_ref().and_then(|c| c.budget.max_steps) {
                if s.work_nodes.len() as u64 >= limit {
                    return Err(Error::Conflict("project step budget exhausted".into()));
                }
            }
            if !s.branches.contains_key(&input.branch_id) {
                return Err(Error::NotFound("branch".into()));
            }
            if s.human_control.as_ref().is_some_and(|c| {
                c.paused_branches.contains(&input.branch_id)
                    || c.cancelled_branches.contains(&input.branch_id)
            }) {
                return Err(Error::Conflict("branch is paused or cancelled".into()));
            }
            for e in &input.acceptance_criteria.required_entities {
                s.check(*e)?;
            }
            for id in &input.acceptance_criteria.required_artifacts {
                s.check(EntityRef::Artifact(*id))?;
            }
            if input.granularity == WorkGranularity::Atomic
                && input.acceptance_criteria.required_entities.is_empty()
                && input.acceptance_criteria.required_artifacts.is_empty()
            {
                return Err(Error::Validation(
                    "atomic work needs verifiable outputs".into(),
                ));
            }
            let n = WorkNode {
                id: WorkNodeId::new(),
                meta: Meta::new(project),
                branch_id: input.branch_id,
                kind: input.kind,
                granularity: input.granularity,
                title: input.title,
                objective: input.objective,
                acceptance_criteria: input.acceptance_criteria,
                status: WorkStatus::Proposed,
                outcome: None,
                owner: ctx.actor.clone(),
                capability_requirements: input.capability_requirements,
                context_policy_id: input.context_policy_id,
                budget: input.budget,
                created_by: ctx.actor.clone(),
            };
            s.work_nodes.insert(n.id, n.clone());
            revision(s, n.branch_id, &ctx)?;
            audit(
                s,
                &ctx,
                "work.created",
                None,
                None,
                vec![],
                json!({"work":n.id}),
            )?;
            Ok(n)
        })
    }
    pub fn request_plan_revision(
        &self,
        project: ProjectId,
        branch: BranchId,
        expected_control_version: Option<u64>,
        ctx: CommandContext,
    ) -> Result<PlanRevision> {
        ctx.actor.require_human()?;
        self.mutate(project,|s|{
            active(s)?;ctx.validate(s)?;super::human_control::authorize_project_human(s,&ctx.actor)?;
            if !s.branches.contains_key(&branch){return Err(Error::NotFound("branch".into()));}
            let mut control=s.human_control.clone().unwrap_or_default();
            if let Some(expected)=expected_control_version { if control.version!=expected{return Err(Error::StaleVersion{expected,actual:control.version});} }
            control.version=control.version.checked_add(1).ok_or_else(||Error::Validation("control version overflow".into()))?;
            let actor_id=match &ctx.actor{Actor::Human(id)=>id.clone(),_=>unreachable!()};
            control.owner_id=Some(actor_id.clone());
            let directive=HumanDirective{id:EventId::new(),project_id:project,actor_id,created_at:Utc::now(),kind:HumanDirectiveKind::RequestPlanRevision,target:Some(branch.to_string()),parameters:json!({"branch_id":branch}),reason:Some(ctx.reason.clone()),expected_version:expected_control_version,version:control.version};
            control.directives.push(directive.clone());s.human_control=Some(control);
            audit(s,&ctx,"human.directive",None,None,vec![],json!({"directive_id":directive.id,"kind":directive.kind,"target":directive.target}))?;
            revision(s,branch,&ctx)
        })
    }
    pub fn add_work_edge(
        &self,
        project: ProjectId,
        from: WorkNodeId,
        to: WorkNodeId,
        kind: WorkEdgeKind,
        ctx: CommandContext,
    ) -> Result<WorkEdge> {
        self.mutate(project, |s| {
            active(s)?;
            ctx.validate(s)?;
            let a = node(s, from)?;
            let b = node(s, to)?;
            if kind == WorkEdgeKind::BranchesFrom {
                ctx.actor.require_human()?;
                super::human_control::authorize_project_human(s, &ctx.actor)?;
                let branch = s
                    .branches
                    .get(&a.branch_id)
                    .ok_or_else(|| Error::NotFound("branch".into()))?;
                if branch.parent_branch_id != Some(b.branch_id)
                    || branch
                        .base_checkpoint_id
                        .and_then(|id| s.checkpoints.get(&id))
                        .and_then(|c| s.plan_revisions.get(&c.plan_revision_id))
                        .is_none_or(|plan| !plan.nodes.iter().any(|n| n.id == to))
                {
                    return Err(Error::PermissionDenied);
                }
            } else if a.branch_id != b.branch_id {
                return Err(Error::ProjectMismatch);
            }
            if from == to
                || (dependency(kind)
                    && if kind == WorkEdgeKind::Blocks {
                        reachable(s, from, to)
                    } else {
                        reachable(s, to, from)
                    })
            {
                return Err(Error::Validation("process cycle".into()));
            }
            if kind == WorkEdgeKind::Contains && a.granularity != WorkGranularity::Composite {
                return Err(Error::Validation(
                    "atomic work cannot contain children".into(),
                ));
            }
            if s.work_edges
                .values()
                .any(|e| e.from == from && e.to == to && e.kind == kind)
            {
                return Err(Error::Conflict("duplicate edge".into()));
            }
            let e = WorkEdge {
                id: WorkEdgeId::new(),
                meta: Meta::new(project),
                branch_id: a.branch_id,
                from,
                to,
                kind,
            };
            s.work_edges.insert(e.id, e.clone());
            revision(s, e.branch_id, &ctx)?;
            audit(
                s,
                &ctx,
                "work.edge_added",
                None,
                None,
                vec![],
                json!({"edge":e.id}),
            )?;
            Ok(e)
        })
    }
    pub fn transition_work(
        &self,
        project: ProjectId,
        id: WorkNodeId,
        expected: u64,
        target: WorkStatus,
        ctx: CommandContext,
    ) -> Result<WorkNode> {
        self.mutate(project, |s| {
            active(s)?;
            ctx.validate(s)?;
            if matches!(target, WorkStatus::Ready | WorkStatus::Running) {
                super::human_control::require_agent_progress(s, &ctx.actor)?;
            }
            let n = node(s, id)?.clone();
            if matches!(target, WorkStatus::Ready | WorkStatus::Running)
                && s.human_control.as_ref().is_some_and(|c| {
                    c.paused_branches.contains(&n.branch_id)
                        || c.cancelled_branches.contains(&n.branch_id)
                })
            {
                return Err(Error::Conflict("branch is paused or cancelled".into()));
            }
            if n.meta.version != expected {
                return Err(Error::StaleVersion {
                    expected,
                    actual: n.meta.version,
                });
            }
            let allowed =
                matches!(
                    (n.status, target),
                    (WorkStatus::Proposed, WorkStatus::Ready)
                        | (WorkStatus::Proposed, WorkStatus::Blocked)
                        | (WorkStatus::Ready, WorkStatus::Running)
                        | (WorkStatus::Running, WorkStatus::Succeeded)
                        | (WorkStatus::Running, WorkStatus::Failed)
                        | (WorkStatus::Running, WorkStatus::Blocked)
                        | (WorkStatus::Running, WorkStatus::WaitingForHuman)
                        | (WorkStatus::WaitingForHuman, WorkStatus::Running)
                        | (WorkStatus::Blocked, WorkStatus::Ready)
                        | (WorkStatus::Failed, WorkStatus::Ready)
                ) || (!matches!(
                    n.status,
                    WorkStatus::Succeeded | WorkStatus::Cancelled | WorkStatus::Superseded
                ) && matches!(target, WorkStatus::Cancelled | WorkStatus::Superseded));
            if !allowed {
                return Err(Error::InvalidTransition("work lifecycle".into()));
            }
            if matches!(
                target,
                WorkStatus::Ready | WorkStatus::Running | WorkStatus::Succeeded
            ) && !satisfied(s, &n)
            {
                return Err(Error::InvalidTransition("unfinished dependency".into()));
            }
            if target == WorkStatus::Succeeded {
                for e in &n.acceptance_criteria.required_entities {
                    s.check(*e)?;
                }
                for a in &n.acceptance_criteria.required_artifacts {
                    s.check(EntityRef::Artifact(*a))?;
                }
                let children: Vec<_> = s
                    .work_edges
                    .values()
                    .filter(|e| e.from == id && e.kind == WorkEdgeKind::Contains)
                    .collect();
                if children.len() < n.acceptance_criteria.minimum_children as usize
                    || children
                        .iter()
                        .any(|e| !node(s, e.to).is_ok_and(|n| n.status == WorkStatus::Succeeded))
                {
                    return Err(Error::Validation(
                        "child output contract unsatisfied".into(),
                    ));
                }
                if n.granularity == WorkGranularity::Composite && children.is_empty() {
                    return Err(Error::Validation("empty composite cannot succeed".into()));
                }
                if s.execution_bindings
                    .values()
                    .filter(|b| b.work_node_id == id)
                    .any(|b| {
                        s.runs.get(&b.id).is_some_and(|r| {
                            !matches!(
                                r.status,
                                RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
                            )
                        })
                    })
                {
                    return Err(Error::Conflict("work has active runs".into()));
                }
            }
            let n = s
                .work_nodes
                .get_mut(&id)
                .ok_or_else(|| Error::NotFound("work".into()))?;
            let previous_status=n.status;
            n.meta.update(expected)?;
            n.status = target;
            let n = n.clone();
            if let Actor::Human(actor_id)=&ctx.actor {
                super::human_control::authorize_project_human(s,&ctx.actor)?;
                let directive_kind=match (previous_status,target) {
                    (WorkStatus::Running,WorkStatus::WaitingForHuman)=>Some(HumanDirectiveKind::PauseWork),
                    (WorkStatus::WaitingForHuman,WorkStatus::Running)=>Some(HumanDirectiveKind::ResumeWork),
                    (_,WorkStatus::Cancelled)=>Some(HumanDirectiveKind::CancelWork),
                    _=>None,
                };
                if let Some(kind)=directive_kind {
                    let mut control=s.human_control.clone().unwrap_or_default();
                    control.owner_id=Some(actor_id.clone());
                    control.version=control.version.checked_add(1).ok_or_else(||Error::Validation("control version overflow".into()))?;
                    let directive=HumanDirective{id:EventId::new(),project_id:project,actor_id:actor_id.clone(),created_at:Utc::now(),kind,target:Some(id.to_string()),parameters:json!({"from":previous_status,"to":target}),reason:Some(ctx.reason.clone()),expected_version:Some(expected),version:control.version};
                    control.directives.push(directive.clone());s.human_control=Some(control);
                    audit(s,&ctx,"human.directive",None,None,vec![],json!({"directive_id":directive.id,"kind":directive.kind,"target":directive.target,"parameters":directive.parameters}))?;
                }
            }
            revision(s, n.branch_id, &ctx)?;
            audit(
                s,
                &ctx,
                "work.transitioned",
                None,
                None,
                vec![],
                json!({"work":id,"status":target}),
            )?;
            if target == WorkStatus::Succeeded {
                let checkpoint = ExecutionCheckpoint {
                    id: CheckpointId::new(),
                    meta: Meta::new(project),
                    run_id: None,
                    work_node_id: Some(id),
                    branch_id: n.branch_id,
                    plan_revision_id: s.branches[&n.branch_id]
                        .head_revision_id
                        .ok_or_else(|| Error::NotFound("plan".into()))?,
                    context_snapshot_id: None,
                    boundary: CheckpointBoundary::WorkCompleted,
                    execution_state: json!({"schema_version":1,"status":target}),
                };
                s.checkpoints.insert(checkpoint.id, checkpoint.clone());
                audit(
                    s,
                    &ctx,
                    "checkpoint.created",
                    None,
                    None,
                    vec![],
                    json!({"checkpoint":checkpoint.id,"boundary":checkpoint.boundary}),
                )?;
            }
            Ok(n)
        })
    }
    pub fn bind_run(
        &self,
        project: ProjectId,
        run: RunId,
        work: WorkNodeId,
        ctx: CommandContext,
    ) -> Result<ExecutionBinding> {
        self.mutate(project, |s| {
            ctx.validate(s)?;
            active(s)?;
            let n = node(s, work)?.clone();
            super::human_control::require_agent_progress(s, &ctx.actor)?;
            if s.human_control.as_ref().is_some_and(|c| {
                c.paused_branches.contains(&n.branch_id)
                    || c.cancelled_branches.contains(&n.branch_id)
            }) {
                return Err(Error::Conflict("branch is paused or cancelled".into()));
            }
            if !s.runs.contains_key(&run) {
                return Err(Error::NotFound("run".into()));
            }
            if s.execution_bindings.contains_key(&run) {
                return Err(Error::Conflict("run binding immutable".into()));
            }
            if s.runs
                .get(&run)
                .is_none_or(|r| r.status != RunStatus::Running)
                || !matches!(
                    n.status,
                    WorkStatus::Proposed | WorkStatus::Ready | WorkStatus::Running
                )
                || !satisfied(s, &n)
            {
                return Err(Error::InvalidTransition("work is not executable".into()));
            }
            if n.status != WorkStatus::Running {
                let current = s
                    .work_nodes
                    .get_mut(&work)
                    .ok_or_else(|| Error::NotFound("work".into()))?;
                current.meta.update(current.meta.version)?;
                current.status = WorkStatus::Running;
                revision(s, n.branch_id, &ctx)?;
                audit(
                    s,
                    &ctx,
                    "WorkNodeStarted",
                    None,
                    None,
                    vec![],
                    json!({"work":work,"run":run}),
                )?;
            }
            let b = ExecutionBinding {
                id: run,
                meta: Meta::new(project),
                work_node_id: work,
                branch_id: n.branch_id,
                plan_revision_id: s.branches[&n.branch_id]
                    .head_revision_id
                    .ok_or_else(|| Error::NotFound("plan".into()))?,
            };
            s.execution_bindings.insert(run, b.clone());
            audit(
                s,
                &ctx,
                "run.bound",
                None,
                None,
                vec![],
                json!({"binding":b}),
            )?;
            Ok(b)
        })
    }
    pub fn checkpoint(
        &self,
        project: ProjectId,
        mut checkpoint: ExecutionCheckpoint,
        ctx: CommandContext,
    ) -> Result<ExecutionCheckpoint> {
        self.mutate(project, |s| {
            active(s)?;
            ctx.validate(s)?;
            safe_metadata(&checkpoint.execution_state)?;
            let plan = s
                .plan_revisions
                .get(&checkpoint.plan_revision_id)
                .ok_or_else(|| Error::NotFound("plan".into()))?;
            if plan.branch_id != checkpoint.branch_id {
                return Err(Error::ProjectMismatch);
            }
            if let Some(work) = checkpoint.work_node_id {
                if !plan.nodes.iter().any(|n| n.id == work) {
                    return Err(Error::Validation(
                        "work absent from checkpoint revision".into(),
                    ));
                }
            }
            if let Some(run) = checkpoint.run_id {
                let b = s
                    .execution_bindings
                    .get(&run)
                    .ok_or_else(|| Error::NotFound("binding".into()))?;
                if b.plan_revision_id != checkpoint.plan_revision_id
                    || Some(b.work_node_id) != checkpoint.work_node_id
                {
                    return Err(Error::Validation("checkpoint must pin run revision".into()));
                }
            }
            if let Some(id) = checkpoint.context_snapshot_id {
                let snap = s
                    .context_snapshots
                    .get(&id)
                    .ok_or_else(|| Error::NotFound("snapshot".into()))?;
                if snap.plan_revision_id != checkpoint.plan_revision_id
                    || Some(snap.work_node_id) != checkpoint.work_node_id
                {
                    return Err(Error::Validation("checkpoint snapshot mismatch".into()));
                }
            }
            if s.checkpoints.contains_key(&checkpoint.id) {
                return Err(Error::Conflict("checkpoint immutable".into()));
            }
            checkpoint.meta = Meta::new(project);
            s.checkpoints.insert(checkpoint.id, checkpoint.clone());
            audit(
                s,
                &ctx,
                "checkpoint.created",
                None,
                None,
                vec![],
                json!({"checkpoint":checkpoint.id,"boundary":checkpoint.boundary}),
            )?;
            Ok(checkpoint)
        })
    }
    /// Resume consults durable work status; completed work is never scheduled again.
    pub fn resumable_work(&self, project: ProjectId, branch: BranchId) -> Result<Vec<WorkNode>> {
        let s = self.state(project)?;
        if !s.branches.contains_key(&branch) {
            return Err(Error::NotFound("branch".into()));
        }
        Ok(s.work_nodes
            .values()
            .filter(|n| {
                n.branch_id == branch
                    && matches!(
                        n.status,
                        WorkStatus::Ready | WorkStatus::Running | WorkStatus::Failed
                    )
                    && satisfied(&s, n)
            })
            .cloned()
            .collect())
    }
    pub fn write_memory(
        &self,
        project: ProjectId,
        namespace: MemoryNamespace,
        work: Option<WorkNodeId>,
        content: String,
        ctx: CommandContext,
    ) -> Result<ProcessMemory> {
        if namespace == MemoryNamespace::ScientificKnowledge {
            return Err(Error::PermissionDenied);
        }
        text(&content, "memory")?;
        safe_metadata(&json!(content))?;
        self.mutate(project, |s| {
            active(s)?;
            ctx.validate(s)?;
            if let Some(id) = work {
                node(s, id)?;
            }
            let m = ProcessMemory {
                id: ProcessMemoryId::new(),
                meta: Meta::new(project),
                namespace,
                work_node_id: work,
                content,
                created_by: ctx.actor.clone(),
            };
            s.process_memories.insert(m.id, m.clone());
            audit(
                s,
                &ctx,
                "memory.created",
                None,
                None,
                vec![],
                json!({"memory":m.id}),
            )?;
            Ok(m)
        })
    }
}

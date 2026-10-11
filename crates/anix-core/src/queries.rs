use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleSummary {
    pub counts: BTreeMap<String, usize>,
    pub hypothesis_statuses: BTreeMap<String, usize>,
    pub experiment_statuses: BTreeMap<String, usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchState {
    pub project: Project,
    pub questions: Vec<ResearchQuestion>,
    pub sources: Vec<Source>,
    pub papers: Vec<Paper>,
    pub claims: Vec<Claim>,
    pub hypotheses: Vec<Hypothesis>,
    pub experiments: Vec<Experiment>,
    pub findings: Vec<Finding>,
    pub pending_gates: Vec<Gate>,
    pub active_tasks: Vec<Task>,
    pub active_runs: Vec<Run>,
    pub recent_events: Vec<DomainEvent>,
    pub lifecycle: LifecycleSummary,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub relation: Relation,
    pub claim: Option<Claim>,
    pub finding: Option<Finding>,
    pub paper: Option<Paper>,
    pub sources: Vec<Source>,
    pub experiment_run: Option<ExperimentRun>,
    pub experiment: Option<Experiment>,
    pub provenance: Vec<ProvenanceRecord>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceTrace {
    pub hypothesis: Hypothesis,
    pub supporting: Vec<EvidenceItem>,
    pub contradicting: Vec<EvidenceItem>,
    pub qualifying: Vec<EvidenceItem>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchContext {
    pub project: Project,
    pub focus: Option<EntityRef>,
    pub questions: Vec<ResearchQuestion>,
    pub hypotheses: Vec<Hypothesis>,
    pub claims: Vec<Claim>,
    pub findings: Vec<Finding>,
    pub experiments: Vec<Experiment>,
    pub neighborhood: Vec<Relation>,
    pub pending_gates: Vec<Gate>,
    pub recent_events: Vec<DomainEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CurrentResearchActivity {
    pub project: Project,
    pub control: HumanControlState,
    pub active_work: Vec<WorkNode>,
    pub active_branches: Vec<Branch>,
    pub pending_gates: Vec<Gate>,
    pub recent_events: Vec<DomainEvent>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionTrace {
    pub target: String,
    pub events: Vec<DomainEvent>,
    pub provenance: Vec<ProvenanceRecord>,
    pub work_nodes: Vec<WorkNode>,
    pub plan_revisions: Vec<PlanRevision>,
    pub model_invocations: Vec<ModelInvocation>,
    pub tool_invocations: Vec<ToolInvocation>,
    pub gates: Vec<Gate>,
}

impl Core {
    pub fn current_activity(&self, project: ProjectId) -> Result<CurrentResearchActivity> {
        let s = self.state(project)?;
        let control = super::human_control::control(&s);
        let mut active_work: Vec<_> = s
            .work_nodes
            .values()
            .filter(|n| {
                matches!(
                    n.status,
                    WorkStatus::Proposed
                        | WorkStatus::Ready
                        | WorkStatus::Running
                        | WorkStatus::WaitingForHuman
                )
            })
            .cloned()
            .collect();
        active_work.sort_by_key(|n| {
            (
                std::cmp::Reverse(
                    control
                        .work_priorities
                        .get(&n.id)
                        .copied()
                        .unwrap_or(Priority::Normal),
                ),
                n.meta.created_at,
            )
        });
        let mut active_branches: Vec<_> = s
            .branches
            .values()
            .filter(|b| !control.cancelled_branches.contains(&b.id))
            .cloned()
            .collect();
        active_branches.sort_by_key(|b| {
            (
                std::cmp::Reverse(
                    control
                        .branch_priorities
                        .get(&b.id)
                        .copied()
                        .unwrap_or(Priority::Normal),
                ),
                b.meta.created_at,
            )
        });
        Ok(CurrentResearchActivity {
            project: s.project()?.clone(),
            control,
            active_work,
            active_branches,
            pending_gates: s
                .gates
                .values()
                .filter(|g| g.status == GateStatus::Pending)
                .cloned()
                .collect(),
            recent_events: s.events.iter().rev().take(50).cloned().collect(),
        })
    }
    pub fn decision_trace(&self, project: ProjectId, target: &str) -> Result<DecisionTrace> {
        text(target, "decision target")?;
        let s = self.state(project)?;
        let related_event_ids: BTreeMap<_, _> = s
            .events
            .iter()
            .filter(|e| e.payload.to_string().contains(target) || e.event_type.contains(target))
            .map(|e| (e.id, ()))
            .collect();
        let related_entities: Vec<_> = s
            .provenance
            .iter()
            .filter(|p| {
                p.entity.is_some_and(|e| e.id() == target)
                    || p.source_entities.iter().any(|e| e.id() == target)
                    || related_event_ids.contains_key(&p.event_id)
            })
            .collect();
        let work_nodes: Vec<_> = s
            .work_nodes
            .values()
            .filter(|n| {
                n.id.to_string() == target
                    || n.title.contains(target)
                    || related_entities
                        .iter()
                        .any(|p| p.event_id.to_string() == target)
            })
            .cloned()
            .collect();
        let gates: Vec<_> = s
            .gates
            .values()
            .filter(|g| {
                g.id.to_string() == target
                    || g.proposed_action.id.to_string() == target
                    || g.reason.contains(target)
            })
            .cloned()
            .collect();
        let events = s
            .events
            .into_iter()
            .filter(|e| {
                related_event_ids.contains_key(&e.id) || e.payload.to_string().contains(target)
            })
            .collect();
        let provenance = s
            .provenance
            .into_iter()
            .filter(|p| {
                p.event_id.to_string() == target
                    || p.entity.is_some_and(|e| e.id() == target)
                    || p.source_entities.iter().any(|e| e.id() == target)
                    || related_event_ids.contains_key(&p.event_id)
            })
            .collect();
        Ok(DecisionTrace {
            target: target.into(),
            events,
            provenance,
            work_nodes,
            plan_revisions: s
                .plan_revisions
                .values()
                .filter(|r| r.id.to_string() == target || r.reason.contains(target))
                .cloned()
                .collect(),
            model_invocations: s
                .model_invocations
                .values()
                .filter(|i| i.id.to_string() == target || i.run_id.to_string() == target)
                .cloned()
                .collect(),
            tool_invocations: s
                .tool_invocations
                .values()
                .filter(|i| i.id.to_string() == target || i.run_id.to_string() == target)
                .cloned()
                .collect(),
            gates,
        })
    }
    /// Uses the configured storage full-text/BM25 index for bounded entity retrieval.
    pub fn search_entities(
        &self,
        project: ProjectId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<EntityRef>> {
        text(query, "search query")?;
        if limit == 0 || limit > 100 {
            return Err(Error::Validation("search limit must be 1..=100".into()));
        }
        self.state(project)?;
        self.storage.search(
            project,
            query,
            u32::try_from(limit)
                .map_err(|_| Error::Validation("search limit is too large".into()))?,
        )
    }

    pub fn get_research_state(&self, project: ProjectId) -> Result<ResearchState> {
        let s = self.state(project)?;
        let mut counts = BTreeMap::new();
        macro_rules! count{($($field:ident),+)=>{$(counts.insert(stringify!($field).into(),s.$field.values().filter(|e|!e.meta.archived).count());)+}}
        count!(
            questions,
            sources,
            papers,
            claims,
            hypotheses,
            experiments,
            findings,
            artifacts
        );
        let mut hs = BTreeMap::new();
        for h in s.hypotheses.values() {
            *hs.entry(format!("{:?}", h.status)).or_default() += 1;
        }
        let mut es = BTreeMap::new();
        for e in s.experiments.values() {
            *es.entry(format!("{:?}", e.status)).or_default() += 1;
        }
        Ok(ResearchState {
            project: s.project()?.clone(),
            questions: s.questions.values().cloned().collect(),
            sources: s.sources.values().cloned().collect(),
            papers: s.papers.values().cloned().collect(),
            claims: s.claims.values().cloned().collect(),
            hypotheses: s.hypotheses.values().cloned().collect(),
            experiments: s.experiments.values().cloned().collect(),
            findings: s.findings.values().cloned().collect(),
            pending_gates: s
                .gates
                .values()
                .filter(|g| g.status == GateStatus::Pending)
                .cloned()
                .collect(),
            active_tasks: s
                .tasks
                .values()
                .filter(|t| matches!(t.status, TaskStatus::Pending | TaskStatus::Running))
                .cloned()
                .collect(),
            active_runs: s
                .runs
                .values()
                .filter(|r| !r.status.terminal())
                .cloned()
                .collect(),
            recent_events: s
                .events
                .iter()
                .rev()
                .take(50)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
            lifecycle: LifecycleSummary {
                counts,
                hypothesis_statuses: hs,
                experiment_statuses: es,
            },
        })
    }
    pub fn get_project_timeline(&self, project: ProjectId) -> Result<Vec<DomainEvent>> {
        Ok(self.state(project)?.events)
    }
    pub fn get_provenance(
        &self,
        project: ProjectId,
        entity: Option<EntityRef>,
    ) -> Result<Vec<ProvenanceRecord>> {
        let s = self.state(project)?;
        Ok(s.provenance
            .into_iter()
            .filter(|p| {
                entity.is_none()
                    || p.entity == entity
                    || p.source_entities.iter().any(|e| Some(*e) == entity)
            })
            .collect())
    }
    pub fn list_relations(
        &self,
        project: ProjectId,
        entity: Option<EntityRef>,
    ) -> Result<Vec<Relation>> {
        let s = self.state(project)?;
        if let Some(e) = entity {
            s.meta(e)?;
        }
        Ok(s.relations
            .into_values()
            .filter(|r| {
                !r.meta.archived && entity.is_none_or(|e| e == r.from_entity || e == r.to_entity)
            })
            .collect())
    }
    pub fn get_gate(&self, project: ProjectId, id: GateId) -> Result<Gate> {
        self.state(project)?
            .gates
            .remove(&id)
            .ok_or_else(|| Error::NotFound("gate".into()))
    }
    pub fn list_gates(&self, project: ProjectId, pending_only: bool) -> Result<Vec<Gate>> {
        Ok(self
            .state(project)?
            .gates
            .into_values()
            .filter(|g| !pending_only || g.status == GateStatus::Pending)
            .collect())
    }
    pub fn list_tasks(&self, project: ProjectId) -> Result<Vec<Task>> {
        Ok(self.state(project)?.tasks.into_values().collect())
    }
    pub fn list_runs(&self, project: ProjectId) -> Result<Vec<Run>> {
        Ok(self.state(project)?.runs.into_values().collect())
    }
    pub fn get_run(&self, project: ProjectId, id: RunId) -> Result<Run> {
        self.state(project)?
            .runs
            .remove(&id)
            .ok_or_else(|| Error::NotFound("run".into()))
    }
    pub fn get_hypothesis(&self, project: ProjectId, id: HypothesisId) -> Result<Hypothesis> {
        self.state(project)?
            .hypotheses
            .remove(&id)
            .ok_or_else(|| Error::NotFound("hypothesis".into()))
    }
    pub fn list_hypotheses(&self, project: ProjectId) -> Result<Vec<Hypothesis>> {
        Ok(self.state(project)?.hypotheses.into_values().collect())
    }
    pub fn get_experiment(&self, project: ProjectId, id: ExperimentId) -> Result<Experiment> {
        self.state(project)?
            .experiments
            .remove(&id)
            .ok_or_else(|| Error::NotFound("experiment".into()))
    }
    pub fn get_experiment_run(
        &self,
        project: ProjectId,
        id: ExperimentRunId,
    ) -> Result<ExperimentRun> {
        self.state(project)?
            .experiment_runs
            .remove(&id)
            .ok_or_else(|| Error::NotFound("experiment run".into()))
    }
    pub fn list_artifacts(&self, project: ProjectId) -> Result<Vec<Artifact>> {
        Ok(self.state(project)?.artifacts.into_values().collect())
    }
    pub fn trace_evidence(&self, project: ProjectId, id: HypothesisId) -> Result<EvidenceTrace> {
        let s = self.state(project)?;
        let hypothesis = s
            .hypotheses
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::NotFound("hypothesis".into()))?;
        let mut result = EvidenceTrace {
            hypothesis,
            supporting: vec![],
            contradicting: vec![],
            qualifying: vec![],
        };
        for relation in s
            .relations
            .values()
            .filter(|r| !r.meta.archived && r.to_entity == EntityRef::Hypothesis(id))
        {
            if !matches!(
                relation.kind,
                RelationKind::Supports | RelationKind::Contradicts | RelationKind::Qualifies
            ) {
                continue;
            }
            let claim = match relation.from_entity {
                EntityRef::Claim(id) => s.claims.get(&id).cloned(),
                _ => None,
            };
            let finding = match relation.from_entity {
                EntityRef::Finding(id) => s.findings.get(&id).cloned(),
                _ => None,
            };
            let paper = claim
                .as_ref()
                .and_then(|c| s.papers.get(&c.paper_id))
                .cloned();
            let sources = paper
                .as_ref()
                .map(|p| {
                    s.paper_sources
                        .iter()
                        .filter(|l| l.paper_id == p.id)
                        .filter_map(|l| s.sources.get(&l.source_id).cloned())
                        .collect()
                })
                .unwrap_or_default();
            let run = finding
                .as_ref()
                .and_then(|f| s.experiment_runs.get(&f.experiment_run_id))
                .cloned();
            let experiment = run
                .as_ref()
                .and_then(|r| s.experiments.get(&r.experiment_id))
                .cloned();
            let provenance = s
                .provenance
                .iter()
                .filter(|p| {
                    p.entity == Some(relation.from_entity)
                        || p.source_entities.contains(&relation.from_entity)
                        || p.entity == Some(EntityRef::Hypothesis(id))
                })
                .cloned()
                .collect();
            let item = EvidenceItem {
                relation: relation.clone(),
                claim,
                finding,
                paper,
                sources,
                experiment_run: run,
                experiment,
                provenance,
            };
            match relation.kind {
                RelationKind::Supports => result.supporting.push(item),
                RelationKind::Contradicts => result.contradicting.push(item),
                _ => result.qualifying.push(item),
            }
        }
        Ok(result)
    }
    pub fn build_context(
        &self,
        project: ProjectId,
        focus: Option<EntityRef>,
    ) -> Result<ResearchContext> {
        let s = self.state(project)?;
        if let Some(e) = focus {
            s.check(e)?;
        }
        // Selection precedes truncation: focused evidence cannot be crowded out
        // by unrelated older project content. All result collections are bounded.
        let neighborhood: Vec<_> = s
            .relations
            .values()
            .filter(|r| {
                !r.meta.archived && focus.is_none_or(|e| r.from_entity == e || r.to_entity == e)
            })
            .take(50)
            .cloned()
            .collect();
        let relevant = |e: EntityRef| {
            focus.is_none()
                || focus == Some(e)
                || neighborhood
                    .iter()
                    .any(|r| r.from_entity == e || r.to_entity == e)
        };
        Ok(ResearchContext {
            project: s.project()?.clone(),
            focus,
            questions: s
                .questions
                .values()
                .filter(|q| !q.meta.archived)
                .take(20)
                .cloned()
                .collect(),
            hypotheses: s
                .hypotheses
                .values()
                .filter(|h| !h.meta.archived && relevant(EntityRef::Hypothesis(h.id)))
                .take(20)
                .cloned()
                .collect(),
            claims: s
                .claims
                .values()
                .filter(|c| !c.meta.archived && relevant(EntityRef::Claim(c.id)))
                .take(50)
                .cloned()
                .collect(),
            findings: s
                .findings
                .values()
                .filter(|f| !f.meta.archived && relevant(EntityRef::Finding(f.id)))
                .take(50)
                .cloned()
                .collect(),
            experiments: s
                .experiments
                .values()
                .filter(|e| {
                    !e.meta.archived
                        && (relevant(EntityRef::Experiment(e.id))
                            || focus == Some(EntityRef::Hypothesis(e.hypothesis_id)))
                })
                .take(20)
                .cloned()
                .collect(),
            neighborhood,
            pending_gates: s
                .gates
                .values()
                .filter(|g| {
                    g.status == GateStatus::Pending
                        && focus.is_none_or(|e| g.affected_entities.contains(&e))
                })
                .take(20)
                .cloned()
                .collect(),
            recent_events: s
                .events
                .iter()
                .rev()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
        })
    }
}

use anix_agent::*;
use anix_core::{CommandContext, Core, NewPaper, NewSource, NewWork, Submission};
use anix_domain::*;
use anix_integrations::*;
use anix_storage::SqliteStorage;
use futures::executor::block_on;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn ctx() -> CommandContext {
    CommandContext::human("researcher", "v2 verified operation")
}
fn setup() -> Result<(tempfile::TempDir, Core, ProjectId, BranchId, QuestionId)> {
    let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    let p = core
        .create_project("V2".into(), "research runtime".into(), ctx())?
        .id;
    let b = core.create_branch(p, "main".into(), None, ctx())?.id;
    let q = core
        .add_question(
            p,
            "Battery lifetime".into(),
            "Battery measurement".into(),
            ctx(),
        )?
        .id;
    Ok((tmp, core, p, b, q))
}
#[test]
fn duplicate_child_work_reuses_existing_state_despite_title_change() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("dedup-lead")?;
    let task = api.create_task(project, "plan".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let first = api.create_child_work(
        project,
        run.id,
        root.id,
        "Inspect".into(),
        "Inspect supplied mechanism evidence".into(),
        "read_source".into(),
    )?;
    let count = core.runtime_state(project)?.work_nodes.len();
    let second = api.create_child_work(
        project,
        run.id,
        root.id,
        "Different title".into(),
        "  INSPECT supplied mechanism evidence ".into(),
        "read_source".into(),
    )?;
    assert_eq!(first.id, second.id);
    assert_eq!(core.runtime_state(project)?.work_nodes.len(), count);
    let repeated = api.create_child_work_with_repeat(
        project,
        run.id,
        root.id,
        "Replication".into(),
        "Inspect supplied mechanism evidence".into(),
        "read_source".into(),
        Some("Independent replication under a separately recorded observation".into()),
    )?;
    assert_ne!(repeated.id, first.id);
    assert!(core
        .runtime_state(project)?
        .provenance
        .iter()
        .any(|record| record.reason.contains("justified replication")));
    Ok(())
}
#[test]
fn lead_replan_requires_owner_gate_and_preserves_prior_revision() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("replan-lead")?;
    let task = api.create_task(project, "replan".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let before = core.runtime_state(project)?;
    let action = ActionEnvelope::new(ProposedAction::ReviseWorkObjective {
        work_node_id: root.id,
        expected_version: before.work_nodes[&root.id].meta.version,
        objective: "Test an alternative mechanism against the supplied sources".into(),
    });
    let Submission::Gate(gate) = api.submit(
        project,
        run.id,
        action.clone(),
        "Original strategy lacks discriminating evidence".into(),
    )?
    else {
        panic!("plan revision must be gated")
    };
    assert_eq!(
        core.runtime_state(project)?.work_nodes[&root.id].objective,
        before.work_nodes[&root.id].objective
    );
    assert!(core
        .approve_gate(
            project,
            gate,
            CommandContext::human("other", "try approval")
        )
        .is_err());
    core.approve_gate(project, gate, ctx())?;
    let after = core.runtime_state(project)?;
    assert_ne!(
        after.branches[&branch].head_revision_id,
        before.branches[&branch].head_revision_id
    );
    for (id, revision) in before.plan_revisions {
        assert_eq!(after.plan_revisions[&id], revision);
    }
    assert_eq!(after.runs[&run.id].status, RunStatus::Running);
    assert!(core
        .request_plan_revision(
            project,
            branch,
            None,
            CommandContext {
                actor: Actor::LeadAgent("replan-lead".into()),
                reason: "try Human control".into(),
                run_id: Some(run.id),
                correlation_id: None,
                causation_id: None
            }
        )
        .is_err());
    Ok(())
}
#[test]
fn unchanged_plan_proposal_returns_existing_work_without_gate_or_revision() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("nochange-lead")?;
    let task = api.create_task(project, "assess plan".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let before = core.runtime_state(project)?;
    for objective in [
        before.work_nodes[&root.id].objective.clone(),
        format!(
            "  {} ",
            before.work_nodes[&root.id].objective.to_uppercase()
        ),
    ] {
        let submission = api.submit(
            project,
            run.id,
            ActionEnvelope::new(ProposedAction::ReviseWorkObjective {
                work_node_id: root.id,
                expected_version: before.work_nodes[&root.id].meta.version,
                objective,
            }),
            "Reuse the current objective".into(),
        )?;
        assert!(
            matches!(submission,Submission::Applied(ActionResult::WorkNode(id)) if id==root.id)
        );
    }
    let after = core.runtime_state(project)?;
    assert_eq!(after.gates.len(), before.gates.len());
    assert_eq!(after.plan_revisions, before.plan_revisions);
    assert_eq!(after.work_nodes[&root.id], before.work_nodes[&root.id]);
    Ok(())
}

#[test]
fn same_research_objective_can_revise_executable_strategy_through_owner_gate() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("strategy-lead")?;
    let task = api.create_task(project, "compare methods".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let old_child = api.create_child_work(
        project,
        run.id,
        root.id,
        "Review".into(),
        "Compare optimizer evidence".into(),
        "read_source".into(),
    )?;
    let before = core.runtime_state(project)?;
    // The question and research objective stay fixed; the executable method changes.
    let action: ProposedAction = serde_json::from_value(json!({"type":"revise_work_plan","data":{
        "work_node_id":root.id,"expected_version":before.work_nodes[&root.id].meta.version,
        "expected_revision":before.branches[&branch].head_revision_id,
        "steps":[{"title":"Experiment","objective":"Compare optimizer evidence","required_tool":"run_experiment","depends_on":[]}]
    }})).expect("a same-objective strategy change must be expressible");
    let Submission::Gate(gate) = api.submit(
        project,
        run.id,
        ActionEnvelope::new(action),
        "Literature cannot distinguish the alternatives; propose controlled experiments".into(),
    )?
    else {
        panic!("strategy change must require owner review")
    };
    assert_eq!(
        core.runtime_state(project)?.work_nodes[&old_child.id].status,
        WorkStatus::Ready
    );
    assert!(core
        .approve_gate(project, gate, CommandContext::human("other", "approve"))
        .is_err());
    core.approve_gate(project, gate, ctx())?;
    let after = core.runtime_state(project)?;
    assert_eq!(
        after.work_nodes[&root.id].objective,
        before.work_nodes[&root.id].objective
    );
    assert_eq!(
        after.work_nodes[&old_child.id].status,
        WorkStatus::Superseded
    );
    assert!(after
        .work_nodes
        .values()
        .any(|w| w.status == WorkStatus::Ready
            && w.capability_requirements == vec!["tool:run_experiment"]));
    for (id, revision) in before.plan_revisions {
        assert_eq!(after.plan_revisions[&id], revision);
    }
    let stable = core.runtime_state(project)?;
    let repeat: ProposedAction=serde_json::from_value(json!({"type":"revise_work_plan","data":{
        "work_node_id":root.id,"expected_version":stable.work_nodes[&root.id].meta.version,
        "expected_revision":stable.branches[&branch].head_revision_id,
        "steps":[{"title":"New label","objective":"Compare optimizer evidence","required_tool":"run_experiment","depends_on":[]}]
    }})).unwrap();
    let submission = api.submit(
        project,
        run.id,
        ActionEnvelope::new(repeat),
        "Same executable strategy".into(),
    )?;
    assert!(matches!(submission,Submission::Applied(ActionResult::WorkNode(id)) if id==root.id));
    let final_state = core.runtime_state(project)?;
    assert_eq!(final_state.plan_revisions, stable.plan_revisions);
    assert_eq!(final_state.gates.len(), stable.gates.len());
    Ok(())
}

#[test]
fn approved_plan_dependencies_become_executable_after_prerequisite_completion() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("dependency-lead")?;
    let task = api.create_task(project, "plan analysis".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let state = core.runtime_state(project)?;
    let action = ProposedAction::ReviseWorkPlan {
        work_node_id: root.id,
        expected_version: state.work_nodes[&root.id].meta.version,
        expected_revision: state.branches[&branch].head_revision_id.unwrap(),
        steps: vec![
            WorkPlanStep {
                title: "Calculate".into(),
                objective: "Calculate from supplied data".into(),
                required_tool: "calculate".into(),
                depends_on: vec![],
            },
            WorkPlanStep {
                title: "Check".into(),
                objective: "Check the calculation".into(),
                required_tool: "verify".into(),
                depends_on: vec![0],
            },
        ],
    };
    let Submission::Gate(gate) = api.submit(
        project,
        run.id,
        ActionEnvelope::new(action),
        "Compare two analytical steps".into(),
    )?
    else {
        panic!("Gate required")
    };
    core.approve_gate(project, gate, ctx())?;
    let state = core.runtime_state(project)?;
    let first = state
        .work_nodes
        .values()
        .find(|n| n.title == "Calculate")
        .unwrap()
        .clone();
    let second = state
        .work_nodes
        .values()
        .find(|n| n.title == "Check")
        .unwrap()
        .clone();
    assert!(api.start_work(project, second.id).is_err());
    let running = core.transition_work(
        project,
        first.id,
        first.meta.version,
        WorkStatus::Running,
        ctx(),
    )?;
    // Human acceptance exercises lifecycle/dependency mechanics only; the separate
    // controlled Lead tests check actual tool-output persistence and research quality.
    core.transition_work(
        project,
        first.id,
        running.meta.version,
        WorkStatus::Succeeded,
        ctx(),
    )?;
    assert_eq!(
        api.start_work(project, second.id)?.status,
        WorkStatus::Running
    );
    Ok(())
}

#[test]
fn work_strategy_cannot_retire_a_prerequisite_used_outside_its_parent() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let external = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("scoped-plan")?;
    let task = api.create_task(project, "replan".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let child = api.create_child_work(
        project,
        run.id,
        root.id,
        "Prior method".into(),
        "Inspect evidence".into(),
        "read_source".into(),
    )?;
    core.add_work_edge(
        project,
        external.id,
        child.id,
        WorkEdgeKind::DependsOn,
        ctx(),
    )?;
    let state = core.runtime_state(project)?;
    let action = ProposedAction::ReviseWorkPlan {
        work_node_id: root.id,
        expected_version: state.work_nodes[&root.id].meta.version,
        expected_revision: state.branches[&branch].head_revision_id.unwrap(),
        steps: vec![WorkPlanStep {
            title: "Calculate".into(),
            objective: "Analyse supplied data".into(),
            required_tool: "calculate".into(),
            depends_on: vec![],
        }],
    };
    assert!(api
        .submit(
            project,
            run.id,
            ActionEnvelope::new(action),
            "Change strategy".into()
        )
        .is_err());
    assert_eq!(core.runtime_state(project)?.gates.len(), state.gates.len());
    assert_eq!(core.runtime_state(project)?.work_nodes[&child.id], child);
    Ok(())
}

#[test]
fn strategy_approval_rechecks_revision_and_does_not_mutate_on_staleness() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("stale-plan")?;
    let task = api.create_task(project, "plan".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let state = core.runtime_state(project)?;
    let action = ProposedAction::ReviseWorkPlan {
        work_node_id: root.id,
        expected_version: state.work_nodes[&root.id].meta.version,
        expected_revision: state.branches[&branch].head_revision_id.unwrap(),
        steps: vec![WorkPlanStep {
            title: "Calculate".into(),
            objective: "Analyse supplied data".into(),
            required_tool: "calculate".into(),
            depends_on: vec![],
        }],
    };
    let Submission::Gate(gate) = api.submit(
        project,
        run.id,
        ActionEnvelope::new(action),
        "Propose strategy".into(),
    )?
    else {
        panic!("Gate required")
    };
    core.request_plan_revision(project, branch, None, ctx())?;
    let before = serde_json::to_value(core.runtime_state(project)?).unwrap();
    assert!(core.approve_gate(project, gate, ctx()).is_err());
    assert_eq!(
        serde_json::to_value(core.runtime_state(project)?).unwrap(),
        before
    );
    Ok(())
}

#[test]
fn work_strategy_scope_and_active_child_execution_are_protected() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let other = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("protected-plan")?;
    let task = api.create_task(project, "plan".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let child = api.create_child_work(
        project,
        run.id,
        root.id,
        "Read".into(),
        "Inspect material".into(),
        "read_source".into(),
    )?;
    api.start_work(project, child.id)?;
    let before = core.runtime_state(project)?;
    for target in [root.id, other.id] {
        let action = ProposedAction::ReviseWorkPlan {
            work_node_id: target,
            expected_version: before.work_nodes[&target].meta.version,
            expected_revision: before.branches[&branch].head_revision_id.unwrap(),
            steps: vec![WorkPlanStep {
                title: "Calculate".into(),
                objective: "Analyse data".into(),
                required_tool: "calculate".into(),
                depends_on: vec![],
            }],
        };
        assert!(api
            .submit(
                project,
                run.id,
                ActionEnvelope::new(action),
                "Different strategy".into()
            )
            .is_err());
    }
    assert_eq!(core.runtime_state(project)?.gates.len(), before.gates.len());
    assert_eq!(core.runtime_state(project)?.work_nodes, before.work_nodes);
    Ok(())
}

#[test]
fn child_work_preserves_supplied_source_references_without_a_search_prerequisite() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let source = core.add_source(
        project,
        NewSource {
            title: "Provided observations".into(),
            source_kind: SourceKind::Dataset,
            locator: "data/observations.csv".into(),
            checksum: None,
            metadata: json!({}),
        },
        ctx(),
    )?;
    let root = core.create_work(
        project,
        NewWork {
            branch_id: branch,
            kind: "provided-data-analysis".into(),
            granularity: WorkGranularity::Composite,
            title: "Compare supplied data".into(),
            objective: "Analyse the provided observations without searching".into(),
            acceptance_criteria: OutputContract {
                required_entities: vec![
                    EntityRef::Question(question),
                    EntityRef::Source(source.id),
                ],
                required_artifacts: vec![],
                minimum_children: 1,
            },
            capability_requirements: vec![],
            context_policy_id: "bounded".into(),
            budget: WorkBudget::default(),
        },
        ctx(),
    )?;
    let api = core.agent_api("data-lead")?;
    let task = api.create_task(project, "read supplied observations".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let child = api.create_child_work(
        project,
        run.id,
        root.id,
        "Read supplied data".into(),
        "Inspect the provided observations".into(),
        "read_source".into(),
    )?;
    assert_eq!(
        child.acceptance_criteria.required_entities,
        root.acceptance_criteria.required_entities
    );
    let handles = api.work_source_handles(project, child.id)?;
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0].0.source_id, source.id);
    Ok(())
}
#[test]
fn completed_research_requires_evidence_and_inconclusive_persists_separately() -> Result<()> {
    let (tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("termination-lead")?;
    let task = api.create_task(project, "assess conclusion".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let before = serde_json::to_value(core.runtime_state(project)?).unwrap();
    assert!(api
        .finish_research(
            project,
            run.id,
            root.id,
            ResearchOutcome::Completed,
            "Unsupported answer".into()
        )
        .is_err());
    assert_eq!(
        serde_json::to_value(core.runtime_state(project)?).unwrap(),
        before
    );
    api.finish_research(
        project,
        run.id,
        root.id,
        ResearchOutcome::Inconclusive,
        "There are no grounded observations sufficient for a supported answer".into(),
    )?;
    drop(api);
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    let state = core.runtime_state(project)?;
    assert_eq!(
        state.work_nodes[&root.id].outcome,
        Some(ResearchOutcome::Inconclusive)
    );
    assert_ne!(state.work_nodes[&root.id].status, WorkStatus::Succeeded);
    Ok(())
}
#[test]
fn independent_branch_results_require_scoped_selection_and_explicit_review() -> Result<()> {
    let (tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("branch-lead")?;
    let task = api.create_task(project, "compare independent paths".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let fork = api.create_branch(project, run.id, "alternative analysis".into())?;
    let scopes = api.research_branch_scopes(project, root.id)?;
    let target = scopes
        .iter()
        .find(|scope| scope.branch_id == fork.id)
        .unwrap();
    assert_eq!(target.parent_work_node_id, Some(root.id));
    assert!(core
        .agent_api("other-lead")?
        .select_research_branch(
            project,
            run.id,
            root.id,
            target.work_node_id,
            target.version
        )
        .is_err());
    assert!(api
        .select_research_branch(
            project,
            run.id,
            root.id,
            target.work_node_id,
            target.version + 1
        )
        .is_err());
    api.select_research_branch(
        project,
        run.id,
        root.id,
        target.work_node_id,
        target.version,
    )?;
    assert!(api
        .finish_research(
            project,
            run.id,
            root.id,
            ResearchOutcome::Inconclusive,
            "Missing independent result".into()
        )
        .is_err());
    let branch_task = api.create_task(project, "assess alternative".into())?;
    let branch_run = api.start_run(project, branch_task.id, Some(run.id))?;
    api.bind_run(project, branch_run.id, target.work_node_id)?;
    api.finish_research(
        project,
        branch_run.id,
        target.work_node_id,
        ResearchOutcome::Inconclusive,
        "No discriminating observations were supplied to this branch".into(),
    )?;
    assert!(api
        .finish_research(
            project,
            run.id,
            root.id,
            ResearchOutcome::Inconclusive,
            "No review receipt".into()
        )
        .is_err());
    api.finish_research_with_branch_reviews(
        project,
        run.id,
        root.id,
        ResearchOutcome::Inconclusive,
        "Both analysis paths lack discriminating observations".into(),
        vec![target.work_node_id],
    )?;
    let state = core.runtime_state(project)?;
    assert!(state
        .events
        .iter()
        .any(|event| event.event_type == "BranchResultAvailable"));
    assert!(state
        .events
        .iter()
        .any(|event| event.event_type == "BranchResultReconciled"));
    drop(api);
    drop(core);
    let restarted = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(
        restarted.runtime_state(project)?.work_nodes[&root.id].outcome,
        Some(ResearchOutcome::Inconclusive)
    );
    Ok(())
}
#[test]
fn legitimate_experiment_replications_remain_distinct_and_gated() -> Result<()> {
    let (_tmp, core, project, _branch, question) = setup()?;
    let ActionResult::Entity(EntityRef::Hypothesis(hypothesis)) = core.hypotheses().create(
        project,
        NewHypothesis {
            question_id: Some(question),
            title: "Replication hypothesis".into(),
            statement: "Compare repeated controlled measurements".into(),
        },
        ctx(),
    )?
    else {
        panic!("hypothesis")
    };
    let api = core.agent_api("replication-lead")?;
    let mut ids = std::collections::BTreeSet::new();
    for number in 1..=2 {
        let task = api.create_task(
            project,
            format!("Independent experiment replication {number}"),
        )?;
        let run = api.start_run(project, task.id, None)?;
        let action = ProposedAction::CreateExperiment(NewExperiment {
            hypothesis_id: hypothesis,
            expected_version: 1,
            title: "Controlled replication".into(),
            objective: "Measure independently recorded replicate".into(),
            methodology: "Same controlled protocol; separate run identity".into(),
            protocol: "Synthetic fixture; no external effects".into(),
        });
        let Submission::Gate(gate) = api.submit(
            project,
            run.id,
            ActionEnvelope::new(action),
            format!("Independent replication {number} requires Human approval"),
        )?
        else {
            panic!("gate")
        };
        let ActionResult::Entity(EntityRef::Experiment(experiment)) =
            core.approve_gate(project, gate, ctx())?
        else {
            panic!("experiment")
        };
        assert!(ids.insert(experiment));
        core.execute(
            project,
            ActionEnvelope::new(ProposedAction::ActivateExperiment {
                experiment_id: experiment,
                expected_version: 1,
            }),
            ctx(),
        )?;
        let attempt = core.start_experiment_run(project, experiment, 2, vec![], ctx())?;
        core.finish_experiment_run(
            project,
            anix_core::FinishExperimentRun {
                id: attempt.id,
                expected_version: 1,
                status: RunStatus::Completed,
                outputs: vec![],
                metadata: json!({"replicate":number}),
            },
            ctx(),
        )?;
    }
    assert_eq!(core.runtime_state(project)?.experiment_runs.len(), 2);
    assert_eq!(core.runtime_state(project)?.gates.len(), 2);
    Ok(())
}
#[test]
fn compiled_decision_context_pins_child_plan_and_completion_readiness() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("ledger-lead")?;
    let task = api.create_task(project, "plan".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let child = api.create_child_work(
        project,
        run.id,
        root.id,
        "Inspect".into(),
        "Inspect mechanism".into(),
        "read_source".into(),
    )?;
    let (_, context) = api.compile_context(
        project,
        run.id,
        root.id,
        vec![EntityRef::Question(question)],
    )?;
    let ledger = context
        .process_notes
        .iter()
        .find_map(|note| note.strip_prefix("Decision state: "))
        .expect("immutable decision state missing");
    let ledger: Value = serde_json::from_str(ledger).unwrap();
    assert_eq!(ledger["children"][0]["id"], json!(child.id));
    assert_eq!(ledger["completion"]["ready"], false);
    assert!(ledger["completion"]["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item.as_str().unwrap().contains("unfinished")));
    Ok(())
}

#[test]
fn latest_no_progress_planning_feedback_survives_context_packing() -> Result<()> {
    let (_tmp, core, project, branch, question) = setup()?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("feedback-lead")?;
    let task = api.create_task(project, "review existing work".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    for _ in 0..4 {
        api.write_process_memory(
            project,
            run.id,
            root.id,
            format!("Source content observation: {}", "source text ".repeat(100)),
        )?;
    }
    let observation = json!({"kind":"existing_work","created":false,"status":"succeeded","instruction":"Use the completed result or explicitly assess research termination."});
    api.write_process_memory(
        project,
        run.id,
        root.id,
        format!("Planning observation: {observation}"),
    )?;
    let (_, context) = api.compile_context(
        project,
        run.id,
        root.id,
        vec![EntityRef::Question(question)],
    )?;
    let state: Value = serde_json::from_str(
        context
            .process_notes
            .iter()
            .find_map(|n| n.strip_prefix("Decision state: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(state["last_planning_observation"], observation);
    Ok(())
}
fn budget_lead<'a>(core: &'a Core, provider: Arc<FakeModelProvider>) -> Result<LeadAgent<'a>> {
    Ok(LeadAgent::new(
        core.agent_api("budget-restart-test")?,
        provider,
        Arc::new(FixedModelSelector {
            selection: ModelSelection {
                provider: "budget-test".into(),
                model: "offline".into(),
            },
        }),
    ))
}

#[test]
fn human_control_policy_pause_budget_and_directives_survive_restart() -> Result<()> {
    let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let path = tmp.path().join("control.sqlite");
    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    let project = core
        .create_project("Control".into(), "HITL".into(), ctx())?
        .id;
    assert_eq!(
        core.get_human_control(project)?.autonomy.mode,
        AutonomyMode::Supervised
    );
    let before = core.get_human_control(project)?.version;
    core.set_autonomy(project, AutonomyMode::Manual, Some(before), ctx())?;
    assert!(core
        .set_autonomy(project, AutonomyMode::Autonomous, Some(before), ctx())
        .is_err());
    core.set_budget(
        project,
        ResearchBudget {
            max_steps: Some(150),
            max_strong_model_calls: Some(4),
            ..Default::default()
        },
        Some(before + 1),
        ctx(),
    )?;
    core.pause_research(project, Some(before + 2), ctx())?;
    assert_eq!(
        core.get_human_control(project)?.status,
        ResearchStatus::Paused
    );
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    let state = core.get_human_control(project)?;
    assert_eq!(state.autonomy.mode, AutonomyMode::Manual);
    assert_eq!(state.budget.max_steps, Some(150));
    assert_eq!(state.budget.max_strong_model_calls, Some(4));
    assert_eq!(state.status, ResearchStatus::Paused);
    assert_eq!(state.directives.len(), 3);
    let resumed = core.resume_research(project, None, ctx())?;
    let count = core.get_human_control(project)?.directives.len();
    assert_eq!(core.resume_research(project, None, ctx())?.id, resumed.id);
    assert_eq!(core.get_human_control(project)?.directives.len(), count);
    assert_eq!(
        core.get_human_control(project)?.status,
        ResearchStatus::Running
    );
    assert!(core.cancel_research(project, None, ctx()).is_ok());
    assert!(core.resume_research(project, None, ctx()).is_err());
    Ok(())
}

#[test]
fn autonomy_policy_enforces_gate_matrix_and_agent_cannot_change_it() -> Result<()> {
    let (_tmp, core, p, _b, _q) = setup()?;
    let action = ProposedAction::CreateHypothesis(NewHypothesis {
        question_id: None,
        title: "H".into(),
        statement: "S".into(),
    });
    assert!(core.action_requires_gate(p, &action, ActionImpact::Meaningful)?);
    let agent = CommandContext {
        actor: Actor::LeadAgent("lead".into()),
        reason: "attempt escalation".into(),
        run_id: None,
        correlation_id: None,
        causation_id: None,
    };
    assert!(core
        .set_autonomy(p, AutonomyMode::Autonomous, None, agent)
        .is_err());
    core.set_autonomy(p, AutonomyMode::Manual, None, ctx())?;
    assert!(core.action_requires_gate(p, &action, ActionImpact::Meaningful)?);
    core.set_autonomy(p, AutonomyMode::Autonomous, None, ctx())?;
    assert!(!core.action_requires_gate(p, &action, ActionImpact::Meaningful)?);
    assert!(core.action_requires_gate(p, &action, ActionImpact::NonDelegable)?);
    Ok(())
}

#[test]
fn hard_source_constraint_is_enforced_by_core_policy() -> Result<()> {
    let (_tmp, core, p, _b, q) = setup()?;
    let source = core.add_source(
        p,
        NewSource {
            title: "Excluded source".into(),
            source_kind: SourceKind::Manual,
            locator: "local:test".into(),
            checksum: None,
            metadata: json!({}),
        },
        ctx(),
    )?;
    core.add_constraint(
        p,
        "Do not use this source".into(),
        ConstraintKind::Hard,
        Some(source.id),
        None,
        ctx(),
    )?;
    let action = ActionEnvelope::new(ProposedAction::CreateRelation(NewRelation {
        from_entity: EntityRef::Source(source.id),
        to_entity: EntityRef::Question(q),
        kind: RelationKind::References,
        confidence: None,
    }));
    assert!(core
        .submit_proposal(
            p,
            action,
            CommandContext {
                actor: Actor::LeadAgent("lead".into()),
                reason: "using excluded source".into(),
                run_id: None,
                correlation_id: None,
                causation_id: None
            },
            "constraint test".into()
        )
        .is_err());
    assert!(core
        .add_constraint(
            p,
            "Ambiguous hard rule".into(),
            ConstraintKind::Hard,
            None,
            None,
            ctx()
        )
        .is_err());
    let soft_source = core.add_source(
        p,
        NewSource {
            title: "Soft preference source".into(),
            source_kind: SourceKind::Manual,
            locator: "local:soft".into(),
            checksum: None,
            metadata: json!({}),
        },
        ctx(),
    )?;
    core.add_constraint(
        p,
        "Prefer not to use this source".into(),
        ConstraintKind::Soft,
        Some(soft_source.id),
        None,
        ctx(),
    )?;
    let soft_action = ActionEnvelope::new(ProposedAction::CreateRelation(NewRelation {
        from_entity: EntityRef::Source(soft_source.id),
        to_entity: EntityRef::Question(q),
        kind: RelationKind::References,
        confidence: None,
    }));
    assert!(core
        .submit_proposal(
            p,
            soft_action,
            CommandContext {
                actor: Actor::LeadAgent("lead".into()),
                reason: "soft constraint is advisory".into(),
                run_id: None,
                correlation_id: None,
                causation_id: None
            },
            "soft preference".into()
        )
        .is_ok());
    Ok(())
}

#[test]
fn project_step_and_tool_budgets_are_enforced_and_versioned() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let d = core.set_budget(
        p,
        ResearchBudget {
            max_steps: Some(1),
            max_tool_calls: Some(0),
            ..Default::default()
        },
        None,
        ctx(),
    )?;
    assert_eq!(d.parameters["old"]["max_steps"], Value::Null);
    assert_eq!(d.parameters["new"]["max_steps"], 1);
    let node = work(&core, p, b, q, WorkGranularity::Atomic)?;
    assert!(work(&core, p, b, q, WorkGranularity::Atomic).is_err());
    let task = core.create_task(p, "tool budget".into(), "boundary".into(), ctx())?;
    let run = core.start_run(p, task.id, None, ctx())?;
    core.bind_run(p, run.id, node.id, ctx())?;
    let invocation = ToolInvocation {
        id: ToolInvocationId::new(),
        meta: Meta::new(p),
        run_id: run.id,
        work_node_id: node.id,
        tool: "read".into(),
        tool_version: "1".into(),
        input_artifacts: vec![],
        output_artifacts: vec![],
        started_at: chrono::Utc::now(),
        finished_at: None,
        status: InvocationStatus::Running,
        error_code: None,
    };
    let system = CommandContext {
        actor: Actor::System,
        reason: "record tool".into(),
        run_id: Some(run.id),
        correlation_id: None,
        causation_id: None,
    };
    assert!(core.record_tool_invocation(p, invocation, system).is_err());
    Ok(())
}

#[test]
fn strong_call_budget_is_reserved_before_provider_invocation() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    core.set_budget(
        p,
        ResearchBudget {
            max_strong_model_calls: Some(0),
            ..Default::default()
        },
        None,
        ctx(),
    )?;
    let node = work(&core, p, b, q, WorkGranularity::Composite)?;
    let node = core.transition_work(p, node.id, node.meta.version, WorkStatus::Ready, ctx())?;
    let api = core.agent_api("budget-test-lead")?;
    let task = api.create_task(p, "strong-call budget regression".into())?;
    let run = api.start_run(p, task.id, None)?;
    api.bind_run(p, run.id, node.id)?;
    let provider = Arc::new(FakeModelProvider::new(vec![ModelResponse {
        schema_version: 1,
        decision: AgentDecision::NoChange {
            summary: "unused".into(),
        },
        metadata: serde_json::json!({}),
    }]));
    let lead = LeadAgent::new(
        api,
        provider.clone(),
        Arc::new(FixedModelSelector {
            selection: ModelSelection {
                provider: "budget-test".into(),
                model: "offline".into(),
            },
        }),
    );
    let error = block_on(lead.run_step(
        p,
        "run no provider call when strong budget is zero".into(),
        Some(EntityRef::Question(q)),
        Some(run.id),
        node.id,
    ))
    .unwrap_err();
    assert!(error.to_string().contains("StrongModelBudgetExceeded"));
    assert!(provider.requests()?.is_empty());
    assert!(core.runtime_state(p)?.model_invocations.is_empty());
    Ok(())
}

#[test]
fn failed_strong_call_counts_across_restart_and_human_can_extend() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let path = tmp.path().join("state.sqlite");
    core.set_budget(
        p,
        ResearchBudget {
            max_strong_model_calls: Some(1),
            ..Default::default()
        },
        None,
        ctx(),
    )?;
    let node = work(&core, p, b, q, WorkGranularity::Composite)?;
    let node = core.transition_work(p, node.id, node.meta.version, WorkStatus::Ready, ctx())?;
    let api = core.agent_api("budget-restart-test")?;
    let task = api.create_task(p, "first failing strong call".into())?;
    let run = api.start_run(p, task.id, None)?;
    api.bind_run(p, run.id, node.id)?;
    let failed_provider = Arc::new(FakeModelProvider::new(vec![]));
    let lead = budget_lead(&core, failed_provider.clone())?;
    assert!(block_on(lead.run_step(
        p,
        "provider fails after invocation starts".into(),
        Some(EntityRef::Question(q)),
        Some(run.id),
        node.id,
    ))
    .is_err());
    assert_eq!(core.runtime_state(p)?.model_invocations.len(), 1);
    assert_eq!(
        core.runtime_state(p)?
            .model_invocations
            .values()
            .next()
            .unwrap()
            .status,
        InvocationStatus::Failed
    );
    api.finish(p, run.id, RunStatus::Failed)?;
    drop(lead);
    drop(api);
    drop(core);

    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    assert_eq!(core.runtime_state(p)?.model_invocations.len(), 1);
    let api = core.agent_api("budget-restart-test")?;
    let task = api.create_task(p, "call while exhausted after restart".into())?;
    let exhausted_run = api.start_run(p, task.id, None)?;
    api.bind_run(p, exhausted_run.id, node.id)?;
    let blocked_provider = Arc::new(FakeModelProvider::new(vec![ModelResponse {
        schema_version: 1,
        decision: AgentDecision::Process {
            action: ProcessAction::FinishResearch {
                reviewed_branches: vec![],
                outcome: ResearchOutcome::Inconclusive,
                summary: "should not reach provider".into(),
            },
            rationale: "unused".into(),
            expected_result: None,
        },
        metadata: serde_json::json!({}),
    }]));
    let blocked_lead = budget_lead(&core, blocked_provider.clone())?;
    let exhausted = block_on(blocked_lead.run_step(
        p,
        "budget must hold after restart".into(),
        Some(EntityRef::Question(q)),
        Some(exhausted_run.id),
        node.id,
    ));
    assert!(exhausted
        .as_ref()
        .is_err_and(|error| error.to_string().contains("StrongModelBudgetExceeded")));
    assert!(blocked_provider.requests()?.is_empty());
    api.finish(p, exhausted_run.id, RunStatus::Failed)?;
    drop(blocked_lead);
    drop(api);
    let control = core.get_human_control(p)?;
    core.set_budget(
        p,
        ResearchBudget {
            max_strong_model_calls: Some(2),
            ..Default::default()
        },
        Some(control.version),
        ctx(),
    )?;
    let api = core.agent_api("budget-restart-test")?;
    let task = api.create_task(p, "human extended strong call budget".into())?;
    let run = api.start_run(p, task.id, None)?;
    api.bind_run(p, run.id, node.id)?;
    let provider = Arc::new(FakeModelProvider::new(vec![ModelResponse {
        schema_version: 1,
        decision: AgentDecision::Process {
            action: ProcessAction::FinishResearch {
                reviewed_branches: vec![],
                outcome: ResearchOutcome::Inconclusive,
                summary: "bounded continuation".into(),
            },
            rationale: "The budget-extension test closes the run safely".into(),
            expected_result: None,
        },
        metadata: serde_json::json!({}),
    }]));
    let lead = budget_lead(&core, provider.clone())?;
    let continued = block_on(lead.run_step(
        p,
        "continue after explicit human budget extension".into(),
        Some(EntityRef::Question(q)),
        Some(run.id),
        node.id,
    ));
    assert!(continued.is_ok(), "{continued:?}");
    assert_eq!(provider.requests()?.len(), 1);
    assert_eq!(core.runtime_state(p)?.model_invocations.len(), 2);
    Ok(())
}

#[test]
fn human_in_the_loop_long_horizon_scenario_survives_restart() -> Result<()> {
    let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let path = tmp.path().join("hitl-long-horizon.sqlite");
    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    let p = core
        .create_project(
            "HITL long horizon".into(),
            "human controlled research".into(),
            ctx(),
        )?
        .id;
    let branch = core.create_branch(p, "main".into(), None, ctx())?;
    let question = core.add_question(
        p,
        "Research question".into(),
        "Question description".into(),
        ctx(),
    )?;
    core.set_budget(
        p,
        ResearchBudget {
            max_steps: Some(30),
            max_lead_calls: Some(20),
            max_tool_calls: Some(20),
            ..Default::default()
        },
        None,
        ctx(),
    )?;
    core.set_autonomy(p, AutonomyMode::Manual, None, ctx())?;
    let first = ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
        question_id: Some(question.id),
        title: "Initial hypothesis".into(),
        statement: "Initial proposition".into(),
    }));
    let lead = || CommandContext {
        actor: Actor::LeadAgent("lead".into()),
        reason: "Lead proposal".into(),
        run_id: None,
        correlation_id: None,
        causation_id: None,
    };
    let Submission::Gate(gate) =
        core.submit_proposal(p, first.clone(), lead(), "manual mode review".into())?
    else {
        panic!("manual mode must gate")
    };
    core.resolve_gate(
        p,
        gate,
        GateStatus::Rejected,
        CommandContext::human(
            "researcher",
            "Reject and preserve the recommendation for review",
        ),
    )?;
    core.set_autonomy(p, AutonomyMode::Autonomous, None, ctx())?;
    let accepted = ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
        question_id: Some(question.id),
        title: "Accepted hypothesis".into(),
        statement: "Alternative proposition".into(),
    }));
    assert!(matches!(
        core.submit_proposal(p, accepted, lead(), "autonomous continuation".into())?,
        Submission::Applied(_)
    ));
    core.set_branch_priority(p, branch.id, Priority::High, None, ctx())?;
    core.pause_research(p, None, ctx())?;
    let blocked = ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
        question_id: Some(question.id),
        title: "Blocked".into(),
        statement: "Must not start".into(),
    }));
    assert!(core
        .submit_proposal(p, blocked, lead(), "paused".into())
        .is_err());
    core.resume_research(p, None, ctx())?;
    let resumed = ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
        question_id: Some(question.id),
        title: "Resumed".into(),
        statement: "Continued after resume".into(),
    }));
    assert!(matches!(
        core.submit_proposal(p, resumed, lead(), "resumed autonomous work".into())?,
        Submission::Applied(_)
    ));
    let before = core.get_human_control(p)?;
    assert_eq!(before.status, ResearchStatus::Running);
    assert_eq!(before.autonomy.mode, AutonomyMode::Autonomous);
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    let after = core.get_human_control(p)?;
    assert_eq!(after.version, before.version);
    assert_eq!(after.directives.len(), before.directives.len());
    assert_eq!(core.get_gate(p, gate)?.status, GateStatus::Rejected);
    assert!(core
        .get_project_timeline(p)?
        .iter()
        .any(|e| e.event_type == "GateRejected"
            && e.payload["proposed_action"]["action"]["type"] == "create_hypothesis"));
    assert!(core
        .current_activity(p)?
        .active_branches
        .iter()
        .any(|b| b.id == branch.id));
    Ok(())
}
fn work(
    core: &Core,
    p: ProjectId,
    b: BranchId,
    q: QuestionId,
    granularity: WorkGranularity,
) -> Result<WorkNode> {
    core.create_work(
        p,
        NewWork {
            branch_id: b,
            kind: "evidence".into(),
            granularity,
            title: "Measure battery".into(),
            objective: "battery measurement".into(),
            acceptance_criteria: OutputContract {
                required_entities: vec![EntityRef::Question(q)],
                required_artifacts: vec![],
                minimum_children: 0,
            },
            capability_requirements: vec!["structured_output".into()],
            context_policy_id: "bounded-v1".into(),
            budget: WorkBudget::default(),
        },
        ctx(),
    )
}

#[test]
fn no_search_provider_returns_actionable_block_without_model_call() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let node = core.create_work(
        p,
        NewWork {
            branch_id: b,
            kind: "evidence".into(),
            granularity: WorkGranularity::Atomic,
            title: "Inspect source content".into(),
            objective: "Inspect evidence for battery lifetime".into(),
            acceptance_criteria: OutputContract {
                required_entities: vec![EntityRef::Question(q)],
                required_artifacts: vec![],
                minimum_children: 0,
            },
            capability_requirements: vec!["tool:read_source".into()],
            context_policy_id: "bounded-v1".into(),
            budget: WorkBudget::default(),
        },
        ctx(),
    )?;
    move_to(&core, p, node.id, WorkStatus::Ready)?;
    let provider = Arc::new(FakeModelProvider::new(vec![]));
    let lead = budget_lead(&core, provider.clone())?.with_retrieval_protocol(RetrievalProtocol {
        search_tool: "search_sources".into(),
        refine_tool: "refine_search".into(),
        read_tools: vec!["read_source".into()],
        reject_tool: "reject_retrieved_sources".into(),
    });
    let error = block_on(lead.run_step(
        p,
        "Inspect source content".into(),
        Some(EntityRef::Question(q)),
        None,
        node.id,
    ))
    .unwrap_err();
    assert!(error.to_string().contains("ActionPrerequisitesUnavailable"));
    assert!(provider.requests()?.is_empty());
    let state = core.runtime_state(p)?;
    assert!(state.model_invocations.is_empty());
    assert_eq!(state.work_nodes[&node.id].status, WorkStatus::Blocked);
    assert!(state
        .runs
        .values()
        .all(|run| run.status == RunStatus::Failed));
    drop(lead);
    drop(core);
    let reopened = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(state, reopened.runtime_state(p)?);
    move_to(&reopened, p, node.id, WorkStatus::Ready)?;
    Ok(())
}

#[test]
fn invocation_identity_and_strong_route_cannot_be_retagged_at_completion() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let node = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let api = core.agent_api("audit-provider")?;
    let task = api.create_task(p, "audit identity".into())?;
    let run = api.start_run(p, task.id, None)?;
    api.bind_run(p, run.id, node.id)?;
    let (snapshot, _) = api.compile_context(p, run.id, node.id, vec![EntityRef::Question(q)])?;
    let id = api.start_model_invocation(
        p,
        run.id,
        node.id,
        snapshot.id,
        "custom-provider".into(),
        "custom-model".into(),
        chrono::Utc::now(),
    )?;
    let before = core.runtime_state(p)?;
    let original = before.model_invocations[&id].clone();
    assert_eq!(original.provider, "custom-provider");
    for kind in ["provider", "model", "route"] {
        let mut changed = original.clone();
        changed.status = InvocationStatus::Succeeded;
        changed.finished_at = Some(chrono::Utc::now());
        changed.latency_ms = Some(1);
        match kind {
            "provider" => changed.provider = "micro".into(),
            "model" => changed.model = "different".into(),
            _ => changed.metadata["route"] = json!("local"),
        }
        let mut context = ctx();
        context.run_id = Some(run.id);
        assert!(core.record_model_invocation(p, changed, context).is_err());
        assert_eq!(before, core.runtime_state(p)?);
    }
    api.finish_model_invocation(p, id, 1, TokenUsage::default(), json!({}), false)?;
    assert_eq!(
        core.runtime_state(p)?.model_invocations[&id].metadata["route"],
        "strong"
    );
    Ok(())
}

#[test]
fn invalid_source_observation_cannot_satisfy_a_required_read() -> Result<()> {
    struct InvalidRead;
    impl Tool for InvalidRead {
        fn descriptor(&self) -> ToolDescriptor {
            ToolDescriptor {
                name: "read_source".into(),
                description: "Invalid reference fixture".into(),
                read_only: true,
            }
        }
        fn execute(&self, _input: ToolInput, _context: ToolContext) -> ToolFuture<'_> {
            Box::pin(async {
                Ok(ToolOutput {
                    schema_version: 1,
                    data: json!({"observation":"InvalidSourceHandle"}),
                    metadata: json!({"reference_valid":false}),
                })
            })
        }
    }
    let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let workspace = Arc::new(Workspace::open(temp.path())?);
    let core = Arc::new(Core::new(Arc::new(SqliteStorage::open(
        workspace.database_path()?,
    )?)));
    let p = core
        .create_project("Invalid read".into(), "semantic failure".into(), ctx())?
        .id;
    let b = core.create_branch(p, "main".into(), None, ctx())?.id;
    let q = core
        .add_question(
            p,
            "Inspect evidence".into(),
            "No fabricated references".into(),
            ctx(),
        )?
        .id;
    let node = core.create_work(
        p,
        NewWork {
            branch_id: b,
            kind: "evidence".into(),
            granularity: WorkGranularity::Atomic,
            title: "Inspect source".into(),
            objective: "Inspect source evidence".into(),
            acceptance_criteria: OutputContract {
                required_entities: vec![EntityRef::Question(q)],
                required_artifacts: vec![],
                minimum_children: 0,
            },
            capability_requirements: vec!["tool:read_source".into()],
            context_policy_id: "bounded-v1".into(),
            budget: WorkBudget::default(),
        },
        ctx(),
    )?;
    let api = core.agent_api("invalid-read")?;
    let task = api.create_task(p, "record failed read".into())?;
    let run = api.start_run(p, task.id, None)?;
    api.bind_run(p, run.id, node.id)?;
    let tool = RecordedTool {
        core: core.clone(),
        inner: Arc::new(InvalidRead),
        offloader: ArtifactOffloader {
            workspace,
            threshold_bytes: 8192,
        },
        version: "1".into(),
    };
    block_on(tool.execute(
        ToolInput {
            schema_version: 1,
            parameters: json!({"handle":"S1"}),
        },
        ToolContext {
            project_id: p,
            run_id: Some(run.id),
            context_snapshot_id: None,
            tool_invocation_id: None,
        },
    ))?;
    assert!(!api.required_tool_output_exists(p, node.id)?);
    assert!(api.validate_work_completion(p, run.id, node.id).is_err());
    assert_eq!(
        api.lead_progress_snapshot(p, node.id)?["successful_source_reads"],
        0
    );
    assert_eq!(
        core.runtime_state(p)?.work_nodes[&node.id].status,
        WorkStatus::Running
    );
    Ok(())
}
#[test]
fn direct_completion_cannot_ignore_a_pending_retrieval_batch() -> Result<()> {
    struct PendingSearch(SourceId);
    impl Tool for PendingSearch {
        fn descriptor(&self) -> ToolDescriptor {
            ToolDescriptor {
                name: "search_sources".into(),
                description: "Pending batch regression".into(),
                read_only: true,
            }
        }
        fn execute(&self, _input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
            Box::pin(async move {
                Ok(ToolOutput {
                    schema_version: 1,
                    data: json!({"candidate":"S1"}),
                    metadata: json!({"retrieval_batch":{"search_invocation_id":context.tool_invocation_id,"query":"mechanism","returned":[{"handle":"S1","source_id":self.0}]}}),
                })
            })
        }
    }
    let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let workspace = Arc::new(Workspace::open(temp.path())?);
    let core = Arc::new(Core::new(Arc::new(SqliteStorage::open(
        workspace.database_path()?,
    )?)));
    let project = core
        .create_project(
            "Pending completion".into(),
            "runtime obligation".into(),
            ctx(),
        )?
        .id;
    let branch = core.create_branch(project, "main".into(), None, ctx())?.id;
    let question = core
        .add_question(
            project,
            "Mechanism".into(),
            "Unresolved retrieval".into(),
            ctx(),
        )?
        .id;
    let source = core.add_source(
        project,
        NewSource {
            title: "Candidate".into(),
            source_kind: SourceKind::Manual,
            locator: "notes/source.txt".into(),
            checksum: None,
            metadata: json!({}),
        },
        ctx(),
    )?;
    let root = work(&core, project, branch, question, WorkGranularity::Composite)?;
    let api = core.agent_api("pending-lead")?;
    let task = api.create_task(project, "plan".into())?;
    let run = api.start_run(project, task.id, None)?;
    api.bind_run(project, run.id, root.id)?;
    let child = api.create_child_work(
        project,
        run.id,
        root.id,
        "Search".into(),
        "Discover candidates".into(),
        "search_sources".into(),
    )?;
    let task = api.create_task(project, "search".into())?;
    let child_run = api.start_run(project, task.id, Some(run.id))?;
    api.bind_run(project, child_run.id, child.id)?;
    let tool = RecordedTool {
        core: core.clone(),
        inner: Arc::new(PendingSearch(source.id)),
        offloader: ArtifactOffloader {
            workspace,
            threshold_bytes: 8192,
        },
        version: "1".into(),
    };
    block_on(tool.execute(
        ToolInput {
            schema_version: 1,
            parameters: json!({"query":"mechanism"}),
        },
        ToolContext {
            project_id: project,
            run_id: Some(child_run.id),
            context_snapshot_id: None,
            tool_invocation_id: None,
        },
    ))?;
    api.finish(project, child_run.id, RunStatus::Completed)?;
    assert!(api.complete_work(project, child_run.id, child.id).is_err());
    Ok(())
}

#[test]
fn source_access_checks_exclusion_archive_and_project_scope() -> Result<()> {
    let (_tmp, core, p, _b, _q) = setup()?;
    let source = core.add_source(
        p,
        NewSource {
            title: "Battery evidence".into(),
            source_kind: SourceKind::Manual,
            locator: "pack://source".into(),
            checksum: None,
            metadata: json!({}),
        },
        ctx(),
    )?;
    let api = core.agent_api("source-policy-test")?;
    api.validate_source_access(p, source.id)?;
    let other = core.create_project("Other".into(), "isolation".into(), ctx())?;
    assert!(api.validate_source_access(other.id, source.id).is_err());
    core.add_constraint(
        p,
        "Exclude this source".into(),
        ConstraintKind::Hard,
        Some(source.id),
        None,
        ctx(),
    )?;
    assert!(api.validate_source_access(p, source.id).is_err());
    let archived = core.add_source(
        p,
        NewSource {
            title: "Archived evidence".into(),
            source_kind: SourceKind::Manual,
            locator: "pack://archived".into(),
            checksum: None,
            metadata: json!({}),
        },
        ctx(),
    )?;
    core.execute(
        p,
        ActionEnvelope::new(ProposedAction::ArchiveEntity {
            entity: EntityRef::Source(archived.id),
            expected_version: archived.meta.version,
        }),
        ctx(),
    )?;
    assert!(api.validate_source_access(p, archived.id).is_err());
    Ok(())
}

#[test]
fn human_constraints_are_pinned_in_immutable_context_and_excluded_papers_are_filtered() -> Result<()>
{
    let (tmp, core, p, b, q) = setup()?;
    let source = core.add_source(
        p,
        NewSource {
            title: "Battery source".into(),
            source_kind: SourceKind::Manual,
            locator: "pack://battery".into(),
            checksum: None,
            metadata: json!({}),
        },
        ctx(),
    )?;
    let paper = core.add_paper(
        p,
        NewPaper {
            title: "Battery measurement".into(),
            authors: vec![],
            publication_year: None,
            doi: None,
            abstract_text: "battery measurement excluded evidence".into(),
            source_ids: vec![source.id],
            metadata: json!({}),
        },
        ctx(),
    )?;
    let node = work(&core, p, b, q, WorkGranularity::Atomic)?;
    core.add_constraint(
        p,
        "Use only independently verified measurements".into(),
        ConstraintKind::Soft,
        None,
        None,
        ctx(),
    )?;
    core.add_constraint(
        p,
        "Excluded source".into(),
        ConstraintKind::Hard,
        Some(source.id),
        None,
        ctx(),
    )?;
    let revision = core.runtime_state(p)?.branches[&b]
        .head_revision_id
        .unwrap();
    let snapshot =
        core.compile_context(p, node.id, revision, vec![EntityRef::Question(q)], ctx())?;
    assert!(!snapshot.entries.iter().any(
        |entry| matches!(entry.reference.entity, EntityRef::Source(id) if id == source.id)
            || matches!(entry.reference.entity, EntityRef::Paper(id) if id == paper.id)
    ));
    let frozen = core.provider_context(p, snapshot.id)?;
    assert!(frozen.process_notes[0].contains("Use only independently verified measurements"));
    core.add_constraint(
        p,
        "Later constraint".into(),
        ConstraintKind::Soft,
        None,
        None,
        ctx(),
    )?;
    assert_eq!(
        serde_json::to_value(&frozen).unwrap(),
        serde_json::to_value(core.provider_context(p, snapshot.id)?).unwrap()
    );
    drop(core);
    let reopened = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(
        serde_json::to_value(frozen).unwrap(),
        serde_json::to_value(reopened.provider_context(p, snapshot.id)?).unwrap()
    );
    Ok(())
}

#[test]
fn child_result_previews_preserve_pinned_question_and_full_persisted_results() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let parent = work(&core, p, b, q, WorkGranularity::Composite)?;
    let large_question = core.add_question(
        p,
        "Detailed supplied question".into(),
        "Measurement constraint. ".repeat(55),
        ctx(),
    )?;
    let mut result_ids = vec![];
    for _ in 0..4 {
        let child = work(&core, p, b, q, WorkGranularity::Atomic)?;
        core.add_work_edge(p, parent.id, child.id, WorkEdgeKind::Contains, ctx())?;
        let memory = core.write_memory(
            p,
            MemoryNamespace::ProcessMemory,
            Some(child.id),
            format!(
                "Work result observation: {}",
                json!({"summary":"Measured evidence. ".repeat(40),"source_artifacts":[]})
            ),
            ctx(),
        )?;
        result_ids.push(memory.id);
        finish_work(&core, p, child.id)?;
    }
    let revision = core.runtime_state(p)?.branches[&b]
        .head_revision_id
        .unwrap();
    let snapshot = core.compile_context(
        p,
        parent.id,
        revision,
        vec![EntityRef::Question(large_question.id)],
        ctx(),
    )?;
    assert!(snapshot
        .entries
        .iter()
        .any(|entry| entry.pinned
            && entry.reference.entity == EntityRef::Question(large_question.id)));
    assert!(snapshot.estimated_tokens <= snapshot.budget_tokens);
    let context = core.provider_context(p, snapshot.id)?;
    let decision: Value = serde_json::from_str(
        context
            .process_notes
            .iter()
            .find_map(|note| note.strip_prefix("Decision state: "))
            .unwrap(),
    )
    .unwrap();
    assert!(
        decision["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|child| child["result"]["summary"]
                .as_str()
                .is_some_and(|summary| summary.starts_with("Measured evidence."))),
        "latest substantive result must remain readable beside the pinned question"
    );
    let state = core.runtime_state(p)?;
    for id in result_ids {
        assert!(state.process_memories[&id]
            .content
            .contains(&"Measured evidence. ".repeat(40)));
    }
    Ok(())
}

#[test]
fn parent_context_receives_completed_child_results_without_sibling_branch_memory() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let parent = work(&core, p, b, q, WorkGranularity::Composite)?;
    let child = work(&core, p, b, q, WorkGranularity::Atomic)?;
    core.add_work_edge(p, parent.id, child.id, WorkEdgeKind::Contains, ctx())?;
    core.write_memory(
        p,
        MemoryNamespace::ProcessMemory,
        Some(child.id),
        format!("Work result observation: {}", json!({"summary":"Measured result from the completed evidence task","source_artifacts":[],"scientific_status":"unreviewed Lead task result"})),
        ctx(),
    )?;
    let before_revision = core.runtime_state(p)?.branches[&b]
        .head_revision_id
        .unwrap();
    let before = core.compile_context(
        p,
        parent.id,
        before_revision,
        vec![EntityRef::Question(q)],
        ctx(),
    )?;
    assert!(core
        .provider_context(p, before.id)?
        .process_notes
        .iter()
        .all(|note| note.starts_with("Decision state: ")));
    finish_work(&core, p, child.id)?;
    let sibling = core.create_branch(p, "isolated".into(), None, ctx())?;
    let other = work(&core, p, sibling.id, q, WorkGranularity::Atomic)?;
    core.write_memory(
        p,
        MemoryNamespace::ProcessMemory,
        Some(other.id),
        "Source content observation: private sibling result".into(),
        ctx(),
    )?;
    finish_work(&core, p, other.id)?;
    let revision = core.runtime_state(p)?.branches[&b]
        .head_revision_id
        .unwrap();
    let snapshot =
        core.compile_context(p, parent.id, revision, vec![EntityRef::Question(q)], ctx())?;
    let context = core.provider_context(p, snapshot.id)?;
    let decision: Value = serde_json::from_str(
        context
            .process_notes
            .iter()
            .find_map(|note| note.strip_prefix("Decision state: "))
            .expect("pinned decision state"),
    )
    .unwrap();
    assert_eq!(
        decision["children"][0]["result"]["summary"],
        "Measured result from the completed evidence task"
    );
    assert!(!serde_json::to_string(&context)
        .unwrap()
        .contains("private sibling result"));
    let restarted = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    let persisted = restarted.provider_context(p, snapshot.id)?;
    let persisted_decision: Value = serde_json::from_str(
        persisted
            .process_notes
            .iter()
            .find_map(|note| note.strip_prefix("Decision state: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        persisted_decision["children"][0]["result"],
        decision["children"][0]["result"]
    );
    assert!(snapshot.estimated_tokens <= snapshot.budget_tokens);
    assert!(core
        .provider_context(p, before.id)?
        .process_notes
        .iter()
        .all(|note| note.starts_with("Decision state: ")));
    Ok(())
}

#[test]
fn pending_gate_and_waiting_status_survive_restart_and_approve_once() -> Result<()> {
    let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let path = tmp.path().join("gate-control.sqlite");
    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    let p = core
        .create_project("Gate restart".into(), "HITL".into(), ctx())?
        .id;
    let action = ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
        question_id: None,
        title: "H".into(),
        statement: "S".into(),
    }));
    let lead = CommandContext {
        actor: Actor::LeadAgent("lead".into()),
        reason: "propose hypothesis".into(),
        run_id: None,
        correlation_id: None,
        causation_id: None,
    };
    let Submission::Gate(gate) =
        core.submit_proposal(p, action, lead, "meaningful action".into())?
    else {
        panic!("supervised action should gate")
    };
    assert_eq!(
        core.get_human_control(p)?.status,
        ResearchStatus::WaitingForHuman
    );
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    assert_eq!(
        core.get_human_control(p)?.status,
        ResearchStatus::WaitingForHuman
    );
    let first = core.approve_gate(p, gate, ctx())?;
    let second = core.approve_gate(p, gate, ctx())?;
    assert_eq!(first, second);
    assert_eq!(core.get_human_control(p)?.status, ResearchStatus::Running);
    Ok(())
}

#[test]
fn branch_pause_blocks_work_and_priorities_order_activity() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let other_branch = core.create_branch(p, "independent".into(), None, ctx())?;
    let paused = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let later = core.create_work(
        p,
        NewWork {
            branch_id: other_branch.id,
            kind: "evidence".into(),
            granularity: WorkGranularity::Atomic,
            title: "Other branch work".into(),
            objective: "independent".into(),
            acceptance_criteria: OutputContract {
                required_entities: vec![EntityRef::Question(q)],
                required_artifacts: vec![],
                minimum_children: 0,
            },
            capability_requirements: vec![],
            context_policy_id: "bounded".into(),
            budget: WorkBudget::default(),
        },
        ctx(),
    )?;
    core.set_branch_paused(p, b, true, None, ctx())?;
    assert!(work(&core, p, b, q, WorkGranularity::Atomic).is_err());
    assert!(work(&core, p, other_branch.id, q, WorkGranularity::Atomic).is_ok());
    core.set_branch_paused(p, b, false, None, ctx())?;
    core.set_work_priority(p, later.id, Priority::High, None, ctx())?;
    let activity = core.current_activity(p)?;
    assert_eq!(activity.active_work[0].id, later.id);
    assert!(activity.active_work.iter().any(|n| n.id == paused.id));
    Ok(())
}

#[test]
fn human_can_pause_resume_and_cancel_work_with_typed_directives() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let node = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let ready = move_to(&core, p, node.id, WorkStatus::Ready)?;
    let running = move_to(&core, p, node.id, WorkStatus::Running)?;
    let paused = core.transition_work(
        p,
        node.id,
        running.meta.version,
        WorkStatus::WaitingForHuman,
        ctx(),
    )?;
    assert_eq!(paused.status, WorkStatus::WaitingForHuman);
    let resumed =
        core.transition_work(p, node.id, paused.meta.version, WorkStatus::Running, ctx())?;
    let cancelled = core.transition_work(
        p,
        node.id,
        resumed.meta.version,
        WorkStatus::Cancelled,
        ctx(),
    )?;
    assert_eq!(cancelled.status, WorkStatus::Cancelled);
    let control = core.get_human_control(p)?;
    assert!(control
        .directives
        .iter()
        .any(|d| d.kind == HumanDirectiveKind::PauseWork));
    assert!(control
        .directives
        .iter()
        .any(|d| d.kind == HumanDirectiveKind::ResumeWork));
    assert!(control
        .directives
        .iter()
        .any(|d| d.kind == HumanDirectiveKind::CancelWork));
    assert!(ready.meta.version < running.meta.version);
    Ok(())
}

#[test]
fn human_created_branch_has_directive_and_provenance() -> Result<()> {
    let (_tmp, core, p, _b, _q) = setup()?;
    let created = core.create_branch(p, "human alternative".into(), None, ctx())?;
    let state = core.runtime_state(p)?;
    let directive = state
        .human_control
        .unwrap()
        .directives
        .into_iter()
        .find(|d| {
            d.kind == HumanDirectiveKind::CreateBranch
                && d.target.as_deref() == Some(&created.id.to_string())
        })
        .expect("human branch directive");
    let directive_event = state
        .events
        .iter()
        .find(|e| e.payload["directive_id"] == directive.id.to_string())
        .expect("directive event");
    assert!(state
        .provenance
        .iter()
        .any(|r| r.event_id == directive_event.id));
    assert!(state
        .events
        .iter()
        .any(|e| e.event_type == "branch.created" && e.actor == Actor::Human("researcher".into())));
    Ok(())
}

#[test]
fn non_owner_cannot_change_controls_resolve_gates_or_cancel_branches() -> Result<()> {
    let (_tmp, core, p, b, _q) = setup()?;
    let attacker = CommandContext::human("untrusted-user", "unauthorized control attempt");
    assert!(core.pause_research(p, None, attacker.clone()).is_err());
    assert!(core
        .set_autonomy(p, AutonomyMode::Autonomous, None, attacker.clone())
        .is_err());
    assert!(core.cancel_branch(p, b, None, attacker.clone()).is_err());
    let action = ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
        question_id: None,
        title: "H".into(),
        statement: "S".into(),
    }));
    let lead = CommandContext {
        actor: Actor::LeadAgent("lead".into()),
        reason: "propose".into(),
        run_id: None,
        correlation_id: None,
        causation_id: None,
    };
    let Submission::Gate(gate) = core.submit_proposal(p, action, lead, "review".into())? else {
        panic!("gate required")
    };
    assert!(core.approve_gate(p, gate, attacker).is_err());
    assert_eq!(core.get_gate(p, gate)?.status, GateStatus::Pending);
    Ok(())
}

#[test]
fn non_owner_cannot_bypass_a_pending_gate_through_direct_execution() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let node = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let api = core.agent_api("gate-bypass-regression")?;
    let task = api.create_task(p, "proposal requiring review".into())?;
    let run = api.start_run(p, task.id, None)?;
    api.bind_run(p, run.id, node.id)?;
    let envelope = ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
        question_id: Some(q),
        title: "Review needed".into(),
        statement: "Requires Human review".into(),
    }));
    let Submission::Gate(gate) =
        api.submit(p, run.id, envelope.clone(), "review proposal".into())?
    else {
        return Err(Error::Validation("expected pending Gate".into()));
    };
    let before = core.runtime_state(p)?;
    let intruder = CommandContext::human("other-human", "try another execution path");
    assert!(matches!(
        core.execute(p, envelope, intruder.clone()),
        Err(Error::PermissionDenied)
    ));
    assert!(matches!(
        core.add_question(
            p,
            "Unauthorized".into(),
            "project mutation".into(),
            intruder
        ),
        Err(Error::PermissionDenied)
    ));
    assert_eq!(before, core.runtime_state(p)?);
    assert_eq!(core.gates().get(p, gate)?.status, GateStatus::Pending);
    core.approve_gate(p, gate, ctx())?;
    assert_eq!(core.gates().get(p, gate)?.status, GateStatus::Approved);
    Ok(())
}

#[test]
fn human_pause_blocks_new_agent_progress_and_directives_are_project_scoped() -> Result<()> {
    let (_tmp, core, p, _b, _q) = setup()?;
    let other = core
        .create_project("Other".into(), "separate project".into(), ctx())?
        .id;
    let before = core.get_human_control(other)?.version;
    core.pause_research(p, None, ctx())?;
    let agent = CommandContext {
        actor: Actor::LeadAgent("lead".into()),
        reason: "continue after pause".into(),
        run_id: None,
        correlation_id: None,
        causation_id: None,
    };
    let action = ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
        question_id: None,
        title: "H".into(),
        statement: "S".into(),
    }));
    assert!(core
        .submit_proposal(p, action, agent, "paused".into())
        .is_err());
    core.set_autonomy(other, AutonomyMode::Manual, Some(before), ctx())?;
    assert_eq!(core.get_human_control(p)?.status, ResearchStatus::Paused);
    Ok(())
}

#[test]
fn pause_writes_suspended_checkpoint_and_resume_reuses_it() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let work = work(&core, p, b, q, WorkGranularity::Atomic)?;
    move_to(&core, p, work.id, WorkStatus::Ready)?;
    move_to(&core, p, work.id, WorkStatus::Running)?;
    let task = core.create_task(
        p,
        "resume at checkpoint".into(),
        "safe boundary".into(),
        ctx(),
    )?;
    let run = core.start_run(p, task.id, None, ctx())?;
    core.bind_run(p, run.id, work.id, ctx())?;
    core.pause_research(p, None, ctx())?;
    let checkpoint = core
        .runtime_state(p)?
        .checkpoints
        .values()
        .find(|c| c.run_id == Some(run.id) && c.boundary == CheckpointBoundary::Suspended)
        .cloned()
        .expect("suspend checkpoint");
    assert!(!core.resume_checkpoint(p, checkpoint.id)?.can_execute);
    core.resume_research(p, None, ctx())?;
    let resumed = core.resume_checkpoint(p, checkpoint.id)?;
    assert!(resumed.can_execute);
    assert_eq!(resumed.checkpoint.id, checkpoint.id);
    Ok(())
}
fn move_to(core: &Core, p: ProjectId, id: WorkNodeId, status: WorkStatus) -> Result<WorkNode> {
    let version = core.runtime_state(p)?.work_nodes[&id].meta.version;
    core.transition_work(p, id, version, status, ctx())
}
fn finish_work(core: &Core, p: ProjectId, id: WorkNodeId) -> Result<()> {
    if core.runtime_state(p)?.work_nodes[&id].status == WorkStatus::Proposed {
        move_to(core, p, id, WorkStatus::Ready)?;
    }
    if core.runtime_state(p)?.work_nodes[&id].status == WorkStatus::Ready {
        move_to(core, p, id, WorkStatus::Running)?;
    }
    move_to(core, p, id, WorkStatus::Succeeded)?;
    Ok(())
}
#[test]
fn dag_rejects_cycles_dependencies_and_atomic_children() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let a = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let z = work(&core, p, b, q, WorkGranularity::Atomic)?;
    core.add_work_edge(p, z.id, a.id, WorkEdgeKind::DependsOn, ctx())?;
    let before = core.runtime_state(p)?;
    assert!(core
        .add_work_edge(p, a.id, z.id, WorkEdgeKind::DependsOn, ctx())
        .is_err());
    assert!(core
        .add_work_edge(p, a.id, z.id, WorkEdgeKind::Contains, ctx())
        .is_err());
    assert!(move_to(&core, p, z.id, WorkStatus::Ready).is_err());
    assert_eq!(before, core.runtime_state(p)?);
    finish_work(&core, p, a.id)?;
    finish_work(&core, p, z.id)?;
    Ok(())
}
#[test]
fn composite_verifies_children() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let root = work(&core, p, b, q, WorkGranularity::Composite)?;
    let child = work(&core, p, b, q, WorkGranularity::Atomic)?;
    core.add_work_edge(p, root.id, child.id, WorkEdgeKind::Contains, ctx())?;
    move_to(&core, p, root.id, WorkStatus::Ready)?;
    move_to(&core, p, root.id, WorkStatus::Running)?;
    assert!(move_to(&core, p, root.id, WorkStatus::Succeeded).is_err());
    finish_work(&core, p, child.id)?;
    move_to(&core, p, root.id, WorkStatus::Succeeded)?;
    Ok(())
}
#[test]
fn crash_resume_skips_a_b_and_resumes_c() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let a = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let z = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let c = work(&core, p, b, q, WorkGranularity::Atomic)?;
    core.add_work_edge(p, c.id, z.id, WorkEdgeKind::DependsOn, ctx())?;
    finish_work(&core, p, a.id)?;
    finish_work(&core, p, z.id)?;
    move_to(&core, p, c.id, WorkStatus::Ready)?;
    move_to(&core, p, c.id, WorkStatus::Running)?;
    let state = core.runtime_state(p)?;
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(state, core.runtime_state(p)?);
    assert_eq!(
        core.resumable_work(p, b)?
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        vec![c.id]
    );
    move_to(&core, p, c.id, WorkStatus::Succeeded)?;
    assert!(core.resumable_work(p, b)?.is_empty());
    assert!(move_to(&core, p, a.id, WorkStatus::Ready).is_err());
    Ok(())
}
#[test]
fn branches_pin_base_and_diverge_without_copying_science() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    work(&core, p, b, q, WorkGranularity::Atomic)?;
    let state = core.runtime_state(p)?;
    let base = state
        .checkpoints
        .values()
        .max_by_key(|c| c.meta.created_at)
        .ok_or_else(|| Error::NotFound("base".into()))?
        .clone();
    let a = core.create_branch(p, "A".into(), Some(base.id), ctx())?;
    let z = core.create_branch(p, "B".into(), Some(base.id), ctx())?;
    let state = core.runtime_state(p)?;
    let a_work = state
        .work_nodes
        .values()
        .find(|n| n.branch_id == a.id)
        .ok_or_else(|| Error::NotFound("A".into()))?
        .id;
    move_to(&core, p, a_work, WorkStatus::Ready)?;
    let state = core.runtime_state(p)?;
    assert!(state
        .work_nodes
        .values()
        .filter(|n| n.branch_id == z.id)
        .all(|n| n.status == WorkStatus::Proposed));
    assert_eq!(state.questions.len(), 1);
    assert_eq!(state.checkpoints[&base.id], base);
    let base_revision = state.plan_revisions[&base.plan_revision_id].clone();
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(
        core.runtime_state(p)?.plan_revisions[&base.plan_revision_id],
        base_revision
    );
    Ok(())
}
#[test]
fn context_is_budgeted_pinned_immutable_and_restartable() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let w = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let revision = core.runtime_state(p)?.branches[&b]
        .head_revision_id
        .ok_or_else(|| Error::NotFound("revision".into()))?;
    let irrelevant = core.add_question(
        p,
        "Unrelated astronomy".into(),
        "Nebula catalogue".into(),
        ctx(),
    )?;
    let source = core.sources().add(
        p,
        NewSource {
            title: "Battery methods".into(),
            source_kind: SourceKind::Manual,
            locator: "pack://battery/methods".into(),
            checksum: None,
            metadata: json!({"schema_version":1}),
        },
        ctx(),
    )?;
    let paper = core.papers().add(
        p,
        NewPaper {
            title: "Battery lifetime measurement".into(),
            authors: vec!["Fixture".into()],
            publication_year: Some(2026),
            doi: None,
            abstract_text: "Battery lifetime measurement used a randomized protocol.".into(),
            source_ids: vec![source.id],
            metadata: json!({"schema_version":1}),
        },
        ctx(),
    )?;
    let snap = core.compile_context(p, w.id, revision, vec![EntityRef::Question(q)], ctx())?;
    assert!(snap.estimated_tokens <= snap.budget_tokens);
    assert_eq!(snap.retrieval_query, "battery measurement");
    assert!(snap.retrieval_candidate_count >= 2);
    assert!(snap
        .retrieval_selected
        .iter()
        .any(|selection| selection.entity == EntityRef::Paper(paper.id)));
    assert!(snap.retrieval_selected.iter().all(|selection| snap
        .entries
        .iter()
        .any(|entry| entry.reference.entity == selection.entity)));
    assert!(snap
        .entries
        .iter()
        .any(|e| e.reference.entity == EntityRef::Question(q) && e.pinned));
    assert!(!snap
        .entries
        .iter()
        .any(|e| e.reference.entity == EntityRef::Question(irrelevant.id)));
    core.write_memory(
        p,
        MemoryNamespace::ProcessMemory,
        Some(w.id),
        "Check calibration first".into(),
        ctx(),
    )?;
    assert!(core
        .write_memory(
            p,
            MemoryNamespace::ScientificKnowledge,
            None,
            "invented fact".into(),
            ctx()
        )
        .is_err());
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(core.runtime_state(p)?.context_snapshots[&snap.id], snap);
    Ok(())
}
#[test]
fn pinned_context_overflow_is_transactional() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let mut input = NewWork {
        branch_id: b,
        kind: "test".into(),
        granularity: WorkGranularity::Atomic,
        title: "tiny budget".into(),
        objective: "battery".into(),
        acceptance_criteria: OutputContract {
            required_entities: vec![EntityRef::Question(q)],
            required_artifacts: vec![],
            minimum_children: 0,
        },
        capability_requirements: vec![],
        context_policy_id: "v1".into(),
        budget: WorkBudget::default(),
    };
    input.budget.context_tokens = 1;
    let w = core.create_work(p, input, ctx())?;
    let before = core.runtime_state(p)?;
    let revision = before.branches[&b]
        .head_revision_id
        .ok_or_else(|| Error::NotFound("revision".into()))?;
    assert!(core
        .compile_context(p, w.id, revision, vec![EntityRef::Question(q)], ctx())
        .is_err());
    assert_eq!(core.runtime_state(p)?, before);
    Ok(())
}
#[test]
fn checkpoint_requires_exact_run_revision_and_rollback() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let w = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let task = core.create_task(p, "task".into(), "task description".into(), ctx())?;
    let run = core.start_run(p, task.id, None, ctx())?;
    let binding = core.bind_run(p, run.id, w.id, ctx())?;
    let snap = core.compile_context(
        p,
        w.id,
        binding.plan_revision_id,
        vec![EntityRef::Question(q)],
        ctx(),
    )?;
    let c = ExecutionCheckpoint {
        id: CheckpointId::new(),
        meta: Meta::new(p),
        run_id: Some(run.id),
        work_node_id: Some(w.id),
        branch_id: b,
        plan_revision_id: binding.plan_revision_id,
        context_snapshot_id: Some(snap.id),
        boundary: CheckpointBoundary::Suspended,
        execution_state: json!({"next_step":3}),
    };
    core.checkpoint(p, c.clone(), ctx())?;
    work(&core, p, b, q, WorkGranularity::Atomic)?;
    let mut wrong = c.clone();
    wrong.id = CheckpointId::new();
    wrong.plan_revision_id = core.runtime_state(p)?.branches[&b]
        .head_revision_id
        .ok_or_else(|| Error::NotFound("head".into()))?;
    let before = core.runtime_state(p)?;
    assert!(core.checkpoint(p, wrong, ctx()).is_err());
    assert_eq!(core.runtime_state(p)?, before);
    assert_eq!(
        before.execution_bindings[&run.id].plan_revision_id,
        binding.plan_revision_id
    );
    Ok(())
}
#[test]
fn artifact_offload_and_verified_range() -> Result<()> {
    let (tmp, core, p, _b, _q) = setup()?;
    let offloader = ArtifactOffloader {
        workspace: Arc::new(Workspace::open(tmp.path())?),
        threshold_bytes: 32,
    };
    let data = json!({"rows":["long payload","second row","third row"]});
    let serialized = data.to_string();
    let output = offloader.store(
        &core,
        p,
        data,
        json!({"type":"object"}),
        "three rows".into(),
        ctx(),
    )?;
    assert_eq!(
        offloader.read_range(&output, 0, 8)?,
        serialized.as_bytes()[..8]
    );
    assert!(offloader.read_range(&output, 0, 2_000_000).is_err());
    assert!(matches!(
        offloader.store(&core, p, json!(1), json!({}), "small".into(), ctx())?,
        OffloadedOutput::Inline { .. }
    ));
    Ok(())
}
#[test]
fn candidate_promotion_requires_grounding_and_human() -> Result<()> {
    let (_tmp, core, p, _b, _q) = setup()?;
    let source = core.add_source(
        p,
        NewSource {
            title: "source".into(),
            source_kind: SourceKind::Manual,
            locator: "notes".into(),
            checksum: None,
            metadata: json!({}),
        },
        ctx(),
    )?;
    let paper = core.add_paper(
        p,
        NewPaper {
            title: "study".into(),
            authors: vec!["author".into()],
            publication_year: None,
            doi: None,
            abstract_text: "Trial included 30 participants.".into(),
            metadata: json!({}),
            source_ids: vec![source.id],
        },
        ctx(),
    )?;
    let c = core.create_candidate(
        p,
        CandidateClaim {
            id: CandidateClaimId::new(),
            meta: Meta::new(p),
            paper_id: paper.id,
            source_id: source.id,
            text: "30 participants".into(),
            source_quote: "Trial included 30 participants.".into(),
            location: ClaimLocation::default(),
            promoted_claim_id: None,
        },
        ctx(),
    )?;
    assert!(core
        .promote_candidate(p, c.id, "different text", ctx())
        .is_err());
    let mut agent = ctx();
    agent.actor = Actor::LeadAgent("micro".into());
    assert!(core
        .promote_candidate(p, c.id, "Trial included 30 participants.", agent)
        .is_err());
    let claim = core.promote_candidate(p, c.id, "Trial included 30 participants.", ctx())?;
    assert_eq!(
        core.promote_candidate(p, c.id, "Trial included 30 participants.", ctx())?
            .id,
        claim.id
    );
    Ok(())
}

fn response(output: Value) -> Result<RuntimeResponse> {
    Ok(RuntimeResponse {
        output,
        usage: TokenUsage {
            input: 20,
            output: 5,
            estimated: false,
        },
        model: "fake".into(),
        revision: "1".into(),
        runtime: "fake".into(),
        timings: None,
    })
}
fn requirements() -> TaskRequirements {
    TaskRequirements {
        task_kind: "classification".into(),
        micro_task: Some(MicroTask::Classification),
        reasoning: ReasoningLevel::Simple,
        impact: ImpactClass::LowRisk,
        context_tokens: 100,
        structured_output: true,
        required_capabilities: vec!["structured_output".into()],
        private: false,
        latency_budget_ms: 1000,
        quality_threshold: 0.97,
    }
}
fn router(local: Arc<dyn ModelRuntime>, strong: Arc<dyn ModelRuntime>) -> IntelligenceRouter {
    let mut registry = CapabilityRegistry::default();
    registry
        .local_quality
        .insert(MicroTask::Classification, 1.0);
    registry
        .providers
        .insert("local".into(), ["structured_output".into()].into());
    IntelligenceRouter {
        deterministic: Arc::new(BuiltinDeterministic),
        local,
        strong,
        templates: Arc::new(BuiltinMicroTemplates),
        registry,
        telemetry: Mutex::new(vec![]),
        usage: Mutex::new(RoutingUsage::default()),
    }
}
#[test]
fn consequential_impact_routes_strong_even_for_allowlisted_tasks() -> Result<()> {
    for impact in [ImpactClass::Consequential, ImpactClass::HighImpact] {
        let local = Arc::new(FakeModelRuntime::new(vec![]));
        let strong = Arc::new(FakeModelRuntime::new(vec![response(
            json!({"label":"physics"}),
        )]));
        let r = router(local.clone(), strong.clone());
        let mut req = requirements();
        req.impact = impact;
        let input = json!({"text":"photon","labels":["physics"]});
        block_on(r.execute(
            &req,
            input.clone(),
            task_schema(MicroTask::Classification, &input),
        ))?;
        assert!(local.requests.lock().unwrap().is_empty());
        assert_eq!(strong.requests.lock().unwrap().len(), 1);
    }
    Ok(())
}
#[test]
fn successful_local_never_calls_strong() -> Result<()> {
    let local = Arc::new(FakeModelRuntime::new(vec![response(
        json!({"label":"physics"}),
    )]));
    let strong = Arc::new(FakeModelRuntime::new(vec![]));
    let r = router(local.clone(), strong.clone());
    let input = json!({"text":"photon","labels":["physics","biology"]});
    assert_eq!(
        block_on(r.execute(
            &requirements(),
            input.clone(),
            task_schema(MicroTask::Classification, &input)
        ))?,
        json!({"label":"physics"})
    );
    assert_eq!(local.requests.lock().unwrap().len(), 1);
    assert!(strong.requests.lock().unwrap().is_empty());
    Ok(())
}
#[test]
fn every_micro_allowlisted_task_can_route_locally() -> Result<()> {
    let local = Arc::new(FakeModelRuntime::new(vec![
        response(json!({"query":"battery lifespan"})),
        response(json!({"candidates":[]})),
        response(json!({"tags":["physics"]})),
        response(json!({"summary":"Concise."})),
        response(json!({"context":"Concise."})),
        response(json!({"label":"physics"})),
    ]));
    let strong = Arc::new(FakeModelRuntime::new(vec![]));
    let mut r = router(local.clone(), strong.clone());
    for task in all_micro_tasks() {
        r.registry.local_quality.insert(task, 1.0);
    }
    let cases = [
        (
            MicroTask::QueryRewrite,
            json!({"query":"battery","mandatory_terms":[]}),
        ),
        (
            MicroTask::CandidateExtraction,
            json!({"text":"No evidence.","maximum_candidates":4}),
        ),
        (
            MicroTask::Tagging,
            json!({"text":"photon","allowed_tags":["physics"]}),
        ),
        (
            MicroTask::ShortSummary,
            json!({"text":"","mandatory_facts":[],"maximum_characters":20}),
        ),
        (
            MicroTask::ContextCompression,
            json!({"text":"","mandatory_facts":[],"maximum_characters":20}),
        ),
        (
            MicroTask::Classification,
            json!({"text":"photon","labels":["physics"]}),
        ),
    ];
    for (task, input) in cases {
        let req = TaskRequirements {
            task_kind: serde_json::to_value(task)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned(),
            micro_task: Some(task),
            ..requirements()
        };
        block_on(r.execute(&req, input.clone(), task_schema(task, &input)))?;
    }
    assert_eq!(local.requests.lock().unwrap().len(), 6);
    assert!(strong.requests.lock().unwrap().is_empty());
    Ok(())
}
#[test]
fn consequential_tasks_stay_off_micro_despite_capability_spoofing() -> Result<()> {
    let tasks = [
        "research_planning",
        "lead_decision",
        "next_action_selection",
        "worknode_decomposition",
        "plan_revision",
        "branch_decision",
        "hypothesis_decision",
        "scientific_conclusion",
        "evidence_sufficiency",
        "contradiction_resolution",
        "experiment_design",
        "experiment_activation",
        "finding_approval",
        "final_synthesis",
        "gate_decision",
        "high_impact_proposed_action",
        "canonical_claim_approval",
        "canonical_finding_creation",
        "direct_db_mutation",
        "knowledge_graph_mutation",
        "research_completion_decision",
    ];
    for task in tasks {
        let local = Arc::new(FakeModelRuntime::new(vec![]));
        let strong = Arc::new(FakeModelRuntime::new(vec![response(json!({}))]));
        let mut r = router(local.clone(), strong.clone());
        r.registry.providers.insert(
            "local".into(),
            [
                "structured_output".into(),
                "tool_use".into(),
                "reasoning".into(),
                "high_impact_authority".into(),
                "benchmark_score=1.0".into(),
            ]
            .into(),
        );
        r.registry
            .local_quality
            .insert(MicroTask::Classification, 1.0);
        let mut req = requirements();
        req.task_kind = task.into();
        req.micro_task = None;
        req.required_capabilities = vec![
            "structured_output".into(),
            "tool_use".into(),
            "reasoning".into(),
            "high_impact_authority".into(),
            "benchmark_score=1.0".into(),
        ];
        block_on(r.execute(
            &req,
            json!({"instruction":"perform consequential research decision"}),
            json!({"type":"object","properties":{},"required":[]}),
        ))?;
        assert!(local.requests.lock().unwrap().is_empty(), "task {task}");
        assert_eq!(strong.requests.lock().unwrap().len(), 1, "task {task}");
    }
    Ok(())
}
#[test]
fn micro_schema_rejects_extra_fields_and_duplicate_array_items() -> Result<()> {
    let object = json!({
        "type":"object",
        "properties":{"label":{"type":"string","enum":["physics"]}},
        "required":["label"],
        "additionalProperties":false
    });
    assert!(validate_schema(&json!({"label":"physics","confidence":0.99}), &object).is_err());
    let array = json!({"type":"array","items":{"type":"string"},"uniqueItems":true});
    assert!(validate_schema(&json!(["physics", "physics"]), &array).is_err());
    Ok(())
}
#[test]
fn high_impact_and_complex_skip_local() -> Result<()> {
    for level in [ReasoningLevel::Complex, ReasoningLevel::HighImpact] {
        let local = Arc::new(FakeModelRuntime::new(vec![]));
        let strong = Arc::new(FakeModelRuntime::new(vec![response(
            json!({"label":"physics"}),
        )]));
        let r = router(local.clone(), strong.clone());
        let mut req = requirements();
        req.reasoning = level;
        let input = json!({"text":"photon","labels":["physics"]});
        block_on(r.execute(
            &req,
            input.clone(),
            task_schema(MicroTask::Classification, &input),
        ))?;
        assert!(local.requests.lock().unwrap().is_empty());
        assert_eq!(strong.requests.lock().unwrap().len(), 1);
    }
    Ok(())
}
#[test]
fn bad_schema_retries_once_and_escalates() -> Result<()> {
    let local = Arc::new(FakeModelRuntime::new(vec![
        response(json!({"label":"invented"})),
        response(json!({"other":1})),
    ]));
    let strong = Arc::new(FakeModelRuntime::new(vec![response(
        json!({"label":"physics"}),
    )]));
    let r = router(local.clone(), strong.clone());
    let input = json!({"text":"photon","labels":["physics"]});
    block_on(r.execute(
        &requirements(),
        input.clone(),
        task_schema(MicroTask::Classification, &input),
    ))?;
    assert_eq!(local.requests.lock().unwrap().len(), 2);
    assert_eq!(strong.requests.lock().unwrap().len(), 1);
    assert!(r
        .telemetry
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .escalations
        .contains(&EscalationReason::RetryExhausted));
    Ok(())
}
#[test]
fn local_runtime_failure_retries_once_then_uses_strong() -> Result<()> {
    let local = Arc::new(FakeModelRuntime::new(vec![
        Err(Error::Model("timeout".into())),
        Err(Error::Model("timeout".into())),
    ]));
    let strong = Arc::new(FakeModelRuntime::new(vec![response(
        json!({"label":"physics"}),
    )]));
    let r = router(local.clone(), strong.clone());
    let input = json!({"text":"photon","labels":["physics"]});
    block_on(r.execute(
        &requirements(),
        input.clone(),
        task_schema(MicroTask::Classification, &input),
    ))?;
    assert_eq!(local.requests.lock().unwrap().len(), 2);
    assert_eq!(strong.requests.lock().unwrap().len(), 1);
    assert!(r
        .telemetry
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .escalations
        .contains(&EscalationReason::RetryExhausted));
    Ok(())
}
#[test]
fn unavailable_local_escalates_and_private_never_leaks() -> Result<()> {
    let local = Arc::new(FakeModelRuntime::new(vec![]));
    local
        .enabled
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let strong = Arc::new(FakeModelRuntime::new(vec![response(
        json!({"label":"physics"}),
    )]));
    let r = router(local, strong.clone());
    let input = json!({"text":"photon","labels":["physics"]});
    block_on(r.execute(
        &requirements(),
        input.clone(),
        task_schema(MicroTask::Classification, &input),
    ))?;
    let mut req = requirements();
    req.private = true;
    assert!(block_on(r.execute(
        &req,
        input.clone(),
        task_schema(MicroTask::Classification, &input)
    ))
    .is_err());
    assert_eq!(strong.requests.lock().unwrap().len(), 1);
    Ok(())
}
#[test]
fn hundred_operations_avoid_cloud_for_ninety_five() -> Result<()> {
    let local = Arc::new(FakeModelRuntime::new(
        (0..35)
            .map(|_| response(json!({"label":"physics"})))
            .collect(),
    ));
    let strong = Arc::new(FakeModelRuntime::new(
        (0..5)
            .map(|_| response(json!({"label":"physics"})))
            .collect(),
    ));
    let r = router(local.clone(), strong.clone());
    for i in 0..100 {
        let mut req = requirements();
        let input;
        if i < 60 {
            req.task_kind = "normalize_doi".into();
            input = json!({"doi":"https://doi.org/10.1234/EXAMPLE"});
        } else {
            input = json!({"text":"photon","labels":["physics"]});
            if i >= 95 {
                req.reasoning = ReasoningLevel::HighImpact;
            }
        }
        block_on(r.execute(
            &req,
            input.clone(),
            task_schema(MicroTask::Classification, &input),
        ))?;
    }
    assert_eq!(r.usage.lock().unwrap().deterministic_operations, 60);
    assert_eq!(local.requests.lock().unwrap().len(), 35);
    assert_eq!(strong.requests.lock().unwrap().len(), 5);
    assert!(r.usage.lock().unwrap().estimated_cloud_tokens_avoided > 0);
    Ok(())
}
#[test]
fn all_six_typed_intelligence_contracts() -> Result<()> {
    let local = RuntimeIntelligence {
        runtime: Arc::new(FakeModelRuntime::new(vec![
            response(json!({"label":"x"})),
            response(json!({"query":"BRCA1 search"})),
            response(
                json!({"candidates":[{"text":"30 participants","source_quote":"30 participants"}]}),
            ),
            response(json!({"tags":["x"]})),
            response(json!({"summary":"30 participants"})),
            response(json!({"context":"30 participants"})),
        ])),
        templates: Arc::new(BuiltinMicroTemplates),
    };
    block_on(local.classify(ClassificationRequest {
        text: "x".into(),
        labels: vec!["x".into()],
    }))?;
    block_on(local.rewrite_query(RewriteRequest {
        query: "BRCA1".into(),
        mandatory_terms: vec!["BRCA1".into()],
    }))?;
    block_on(local.extract_candidates(ExtractionRequest {
        text: "30 participants".into(),
        maximum_candidates: 1,
    }))?;
    block_on(local.tag(TaggingRequest {
        text: "x".into(),
        allowed_tags: vec!["x".into()],
    }))?;
    let summary = SummaryRequest {
        text: "30 participants".into(),
        mandatory_facts: vec!["30".into()],
        maximum_characters: 100,
    };
    block_on(local.summarize_short(summary.clone()))?;
    block_on(local.compress_context(summary))?;
    Ok(())
}
#[test]
fn scheduler_serializes_and_cancels_queue() -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let scheduler = Arc::new(LocalInferenceScheduler::default());
    let cancelled = AtomicBool::new(false);
    let permit = scheduler.acquire(&cancelled)?;
    let s = scheduler.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    let waiter = std::thread::spawn(move || s.acquire(&c).is_err());
    std::thread::sleep(std::time::Duration::from_millis(30));
    cancel.store(true, Ordering::Relaxed);
    assert!(waiter.join().unwrap());
    drop(permit);
    let _permit = scheduler.acquire(&cancelled)?;
    Ok(())
}
#[test]
fn apple_boundary_unavailable_on_windows() -> Result<()> {
    let apple = AppleFoundationModelRuntime { bridge: None };
    assert!(!apple.available());
    assert!(block_on(apple.generate(RuntimeRequest {
        instruction: "x".into(),
        input: "x".into(),
        schema: json!({}),
        max_output_tokens: 10
    }))
    .is_err());
    Ok(())
}
#[test]
fn bootstrap_fixture_verifies_atomic_install_and_idempotence() -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| Error::Storage(e.to_string()))?;
    let address = listener
        .local_addr()
        .map_err(|e| Error::Storage(e.to_string()))?;
    let bytes = b"tiny GGUF fixture";
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let _ = stream.read(&mut request);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        )
        .unwrap();
        stream.write_all(bytes).unwrap();
    });
    let mut manifest = ModelManifest::read(std::path::Path::new("../../models/manifest.toml"))?;
    manifest.size = bytes.len() as u64;
    manifest.sha256 = format!("{:x}", Sha256::digest(bytes));
    manifest.url = format!("http://{address}/model");
    let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let path = bootstrap(tmp.path(), &manifest)?;
    server.join().unwrap();
    manifest.verify(&path)?;
    assert_eq!(bootstrap(tmp.path(), &manifest)?, path);
    std::fs::write(&path, b"corrupt").map_err(|e| Error::Storage(e.to_string()))?;
    assert!(bootstrap(tmp.path(), &manifest).is_err());
    Ok(())
}

#[test]
fn recorded_model_failure_and_success_survive_restart() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let w = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let task = core.create_task(p, "task".into(), "execution".into(), ctx())?;
    let run = core.start_run(p, task.id, None, ctx())?;
    let binding = core.bind_run(p, run.id, w.id, ctx())?;
    let snap = core.compile_context(
        p,
        w.id,
        binding.plan_revision_id,
        vec![EntityRef::Question(q)],
        ctx(),
    )?;
    let core = Arc::new(core);
    let observed = RecordedRuntime::new(
        core.clone(),
        Arc::new(FakeModelRuntime::new(vec![
            Err(Error::Model("timeout".into())),
            response(json!({"label":"physics"})),
        ])),
        p,
        run.id,
        snap.id,
        "local".into(),
        "test-v1".into(),
    )?;
    let request = RuntimeRequest {
        instruction: "classify".into(),
        input: "photon".into(),
        schema: object_schema(json!({"label":{"type":"string"}}), &["label"]),
        max_output_tokens: 20,
    };
    assert!(block_on(observed.generate(request.clone())).is_err());
    block_on(observed.generate(request))?;
    let state = core.runtime_state(p)?;
    assert_eq!(state.model_invocations.len(), 2);
    assert!(state
        .model_invocations
        .values()
        .any(|i| i.status == InvocationStatus::Failed));
    assert!(state
        .events
        .iter()
        .any(|e| e.event_type == "ModelInvocationFailed"));
    drop(observed);
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(core.runtime_state(p)?, state);
    let invocation = state
        .model_invocations
        .values()
        .find(|i| i.status == InvocationStatus::Succeeded)
        .unwrap();
    assert_eq!(
        core.model_output(p, invocation.id)?,
        json!({"label":"physics"})
    );
    Ok(())
}
#[test]
fn gate_boundaries_are_atomic_and_pin_run_revision() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let w = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let task = core.create_task(p, "proposal".into(), "proposal execution".into(), ctx())?;
    let run = core.start_run(p, task.id, None, ctx())?;
    let binding = core.bind_run(p, run.id, w.id, ctx())?;
    let mut context = ctx();
    context.run_id = Some(run.id);
    context.actor = Actor::LeadAgent("lead".into());
    let submission = core.submit_proposal(
        p,
        ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
            question_id: Some(q),
            title: "H".into(),
            statement: "A extends battery lifetime".into(),
        })),
        context,
        "scientific hypothesis".into(),
    )?;
    let anix_core::Submission::Gate(gate) = submission else {
        return Err(Error::Validation("gate expected".into()));
    };
    let state = core.runtime_state(p)?;
    assert!(state.checkpoints.values().any(|c| c.run_id == Some(run.id)
        && c.boundary == CheckpointBoundary::BeforeGate
        && c.plan_revision_id == binding.plan_revision_id));
    core.approve_gate(p, gate, ctx())?;
    let state = core.runtime_state(p)?;
    assert!(state.checkpoints.values().any(|c| c.run_id == Some(run.id)
        && c.boundary == CheckpointBoundary::AfterGate
        && c.plan_revision_id == binding.plan_revision_id));
    Ok(())
}
#[test]
fn failed_gate_checkpoint_commit_rolls_back_all_records() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let w = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let task = core.create_task(p, "proposal".into(), "proposal".into(), ctx())?;
    let run = core.start_run(p, task.id, None, ctx())?;
    core.bind_run(p, run.id, w.id, ctx())?;
    let connection = rusqlite::Connection::open(tmp.path().join("state.sqlite"))
        .map_err(|e| Error::Storage(e.to_string()))?;
    connection.execute_batch("CREATE TRIGGER fail_checkpoint BEFORE INSERT ON checkpoints BEGIN SELECT RAISE(ABORT,'injected failure'); END;").map_err(|e|Error::Storage(e.to_string()))?;
    let before = core.runtime_state(p)?;
    let mut context = ctx();
    context.run_id = Some(run.id);
    assert!(core
        .submit_proposal(
            p,
            ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {
                question_id: Some(q),
                title: "H".into(),
                statement: "A extends battery lifetime".into()
            })),
            context,
            "scientific hypothesis".into()
        )
        .is_err());
    assert_eq!(before, core.runtime_state(p)?);
    Ok(())
}
#[test]
fn mixed_blocks_and_depends_on_cycles_are_rejected() -> Result<()> {
    let (_tmp, core, p, b, q) = setup()?;
    let a = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let z = work(&core, p, b, q, WorkGranularity::Atomic)?;
    core.add_work_edge(p, a.id, z.id, WorkEdgeKind::DependsOn, ctx())?;
    assert!(core
        .add_work_edge(p, a.id, z.id, WorkEdgeKind::Blocks, ctx())
        .is_err());
    Ok(())
}
#[test]
fn model_quality_and_context_overflow_route_directly_to_strong() -> Result<()> {
    for quality in [true, false] {
        let local = Arc::new(FakeModelRuntime::new(vec![]));
        let strong = Arc::new(FakeModelRuntime::new(vec![response(
            json!({"label":"physics"}),
        )]));
        let mut r = router(local.clone(), strong);
        let mut req = requirements();
        if quality {
            r.registry
                .local_quality
                .insert(MicroTask::Classification, 0.60);
        } else {
            req.context_tokens = 9000;
        }
        let input = json!({"text":"photon","labels":["physics"]});
        block_on(r.execute(
            &req,
            input.clone(),
            task_schema(MicroTask::Classification, &input),
        ))?;
        assert!(local.requests.lock().unwrap().is_empty());
    }
    Ok(())
}
#[test]
fn malformed_local_runtime_response_escalates() -> Result<()> {
    let local = Arc::new(FakeModelRuntime::new(vec![Err(Error::Model(
        "timeout".into(),
    ))]));
    let strong = Arc::new(FakeModelRuntime::new(vec![response(
        json!({"label":"physics"}),
    )]));
    let r = router(local.clone(), strong.clone());
    let input = json!({"text":"photon","labels":["physics"]});
    block_on(r.execute(
        &requirements(),
        input.clone(),
        task_schema(MicroTask::Classification, &input),
    ))?;
    assert_eq!(local.requests.lock().unwrap().len(), 2);
    assert_eq!(strong.requests.lock().unwrap().len(), 1);
    assert!(r
        .telemetry
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .escalations
        .contains(&EscalationReason::RetryExhausted));
    Ok(())
}
#[test]
fn eval_fixture_distribution_and_architecture_boundaries() -> Result<()> {
    let root = std::path::Path::new("../..");
    let fixtures = std::fs::read_to_string(root.join("eval/micro-v1.jsonl"))
        .map_err(|e| Error::Storage(e.to_string()))?;
    let mut counts = std::collections::BTreeMap::new();
    for line in fixtures.lines() {
        let v: Value = serde_json::from_str(line).map_err(|e| Error::Validation(e.to_string()))?;
        *counts
            .entry(v["task"].as_str().unwrap().to_string())
            .or_insert(0) += 1;
    }
    for (task, count) in [
        ("classification", 25),
        ("query_rewrite", 15),
        ("candidate_extraction", 20),
        ("tagging", 15),
        ("short_summary", 15),
        ("context_compression", 10),
    ] {
        assert_eq!(counts[task], count);
    }
    for crate_name in ["anix-domain", "anix-core", "anix-agent"] {
        let manifest =
            std::fs::read_to_string(root.join(format!("crates/{crate_name}/Cargo.toml")))
                .map_err(|e| Error::Storage(e.to_string()))?;
        assert!(!manifest.contains("rusqlite"));
        assert!(!manifest.contains("anix-storage"));
        assert!(!manifest.contains("reqwest"));
    }
    let ignored = std::fs::read_to_string(root.join(".gitignore"))
        .map_err(|e| Error::Storage(e.to_string()))?;
    for pattern in ["*.gguf", "*.bin", "*.safetensors"] {
        assert!(ignored.contains(pattern));
    }
    Ok(())
}
#[test]
fn realistic_micro_eval_has_required_distribution_and_unique_ids() -> Result<()> {
    let root = std::path::Path::new("../..");
    let fixtures = std::fs::read_to_string(root.join("eval/real-micro-eval.jsonl"))
        .map_err(|e| Error::Storage(e.to_string()))?;
    let mut counts = std::collections::BTreeMap::new();
    let mut ids = std::collections::BTreeSet::new();
    for line in fixtures.lines() {
        let value: Value =
            serde_json::from_str(line).map_err(|e| Error::Validation(e.to_string()))?;
        assert!(ids.insert(value["id"].as_str().unwrap().to_owned()));
        *counts
            .entry(value["task"].as_str().unwrap().to_owned())
            .or_insert(0) += 1;
    }
    assert_eq!(fixtures.lines().count(), 140);
    assert_eq!(counts["query_rewrite"], 25);
    assert_eq!(counts["candidate_extraction"], 30);
    assert_eq!(counts["tagging"], 20);
    assert_eq!(counts["short_summary"], 20);
    assert_eq!(counts["context_compression"], 20);
    assert_eq!(counts["classification"], 25);
    Ok(())
}

#[test]
fn complete_v2_research_runtime_end_to_end() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let root = work(&core, p, b, q, WorkGranularity::Composite)?;
    let evidence = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let design = work(&core, p, b, q, WorkGranularity::Atomic)?;
    core.add_work_edge(p, root.id, evidence.id, WorkEdgeKind::Contains, ctx())?;
    core.add_work_edge(p, root.id, design.id, WorkEdgeKind::Contains, ctx())?;
    core.add_work_edge(p, design.id, evidence.id, WorkEdgeKind::DependsOn, ctx())?;
    let parent_task = core.create_task(
        p,
        "research goal".into(),
        "coordinate child work".into(),
        ctx(),
    )?;
    let parent = core.start_run(p, parent_task.id, None, ctx())?;
    core.bind_run(p, parent.id, root.id, ctx())?;
    let source = core.add_source(
        p,
        NewSource {
            title: "trial".into(),
            source_kind: SourceKind::Manual,
            locator: "notes/trial.txt".into(),
            checksum: None,
            metadata: json!({}),
        },
        ctx(),
    )?;
    let paper = core.add_paper(
        p,
        NewPaper {
            title: "trial paper".into(),
            authors: vec!["Researcher".into()],
            publication_year: Some(2024),
            doi: Some("10.1234/study".into()),
            abstract_text: "Trial included 30 participants.".into(),
            metadata: json!({}),
            source_ids: vec![source.id],
        },
        ctx(),
    )?;
    let task = core.create_task(
        p,
        "evidence".into(),
        "extract grounded evidence".into(),
        ctx(),
    )?;
    let run = core.start_run(p, task.id, Some(parent.id), ctx())?;
    let binding = core.bind_run(p, run.id, evidence.id, ctx())?;
    let snapshot = core.compile_context(
        p,
        evidence.id,
        binding.plan_revision_id,
        vec![
            EntityRef::Question(q),
            EntityRef::Paper(paper.id),
            EntityRef::Source(source.id),
        ],
        ctx(),
    )?;
    let core = Arc::new(core);
    let local: Arc<dyn ModelRuntime> = Arc::new(RecordedRuntime::new(
        core.clone(),
        Arc::new(FakeModelRuntime::new(vec![
            response(json!({"label":"physics"})),
            response(
                json!({"candidates":[{"text":"Trial included 30 participants.","source_quote":"Trial included 30 participants."}]}),
            ),
        ])),
        p,
        run.id,
        snapshot.id,
        "local".into(),
        "micro-v3".into(),
    )?);
    let strong = Arc::new(FakeModelRuntime::new(vec![]));
    let mut routing = router(local, strong);
    routing
        .registry
        .local_quality
        .insert(MicroTask::CandidateExtraction, 1.0);
    let durable = DurableRouter {
        core: core.clone(),
        router: routing,
        project: p,
        run: run.id,
    };
    let mut req = requirements();
    req.task_kind = "normalize_doi".into();
    block_on(durable.execute(
        &req,
        json!({"doi":"https://doi.org/10.1234/Study"}),
        object_schema(json!({"doi":{"type":"string"}}), &["doi"]),
    ))?;
    let input = json!({"text":"Photon measurement","labels":["physics","biology"]});
    block_on(durable.execute(
        &requirements(),
        input.clone(),
        task_schema(MicroTask::Classification, &input),
    ))?;
    let mut req = requirements();
    req.task_kind = "candidate_extraction".into();
    req.micro_task = Some(MicroTask::CandidateExtraction);
    let input = json!({"text":"Trial included 30 participants.","maximum_candidates":1});
    let extracted: ExtractedCandidates = serde_json::from_value(block_on(durable.execute(
        &req,
        input.clone(),
        task_schema(MicroTask::CandidateExtraction, &input),
    ))?)
    .map_err(|e| Error::Validation(e.to_string()))?;
    let candidate = core.create_candidate(
        p,
        CandidateClaim {
            id: CandidateClaimId::new(),
            meta: Meta::new(p),
            paper_id: paper.id,
            source_id: source.id,
            text: extracted.candidates[0].text.clone(),
            source_quote: extracted.candidates[0].source_quote.clone(),
            location: ClaimLocation::default(),
            promoted_claim_id: None,
        },
        ctx(),
    )?;
    let claim =
        core.promote_candidate(p, candidate.id, "Trial included 30 participants.", ctx())?;
    let hypothesis = core.hypotheses().create(
        p,
        NewHypothesis {
            question_id: Some(q),
            title: "H".into(),
            statement: "A improves battery lifetime".into(),
        },
        ctx(),
    )?;
    let ActionResult::Entity(EntityRef::Hypothesis(hid)) = hypothesis else {
        return Err(Error::Validation("hypothesis expected".into()));
    };
    core.execute(
        p,
        ActionEnvelope::new(ProposedAction::CreateRelation(NewRelation {
            from_entity: EntityRef::Claim(claim.id),
            to_entity: EntityRef::Hypothesis(hid),
            kind: RelationKind::Supports,
            confidence: None,
        })),
        ctx(),
    )?;
    core.set_work_contract(
        p,
        evidence.id,
        core.runtime_state(p)?.work_nodes[&evidence.id].meta.version,
        OutputContract {
            required_entities: vec![EntityRef::Claim(claim.id)],
            required_artifacts: vec![],
            minimum_children: 0,
        },
        ctx(),
    )?;
    core.finish_run(p, run.id, RunStatus::Completed, ctx())?;
    finish_work(&core, p, evidence.id)?;
    let task = core.create_task(
        p,
        "design".into(),
        "prepare experiment proposal".into(),
        ctx(),
    )?;
    let child = core.start_run(p, task.id, Some(parent.id), ctx())?;
    let binding = core.bind_run(p, child.id, design.id, ctx())?;
    let snapshot = core.compile_context(
        p,
        design.id,
        binding.plan_revision_id,
        vec![EntityRef::Hypothesis(hid), EntityRef::Claim(claim.id)],
        ctx(),
    )?;
    let action = ProposedAction::CreateExperiment(NewExperiment {
        hypothesis_id: hid,
        expected_version: 1,
        title: "Controlled comparison".into(),
        objective: "Measure lifetime".into(),
        methodology: "matched groups".into(),
        protocol: "Run ten charge cycles".into(),
    });
    let strong = RecordedRuntime::new(
        core.clone(),
        Arc::new(FakeModelRuntime::new(vec![response(
            serde_json::to_value(&action).map_err(|e| Error::Validation(e.to_string()))?,
        )])),
        p,
        child.id,
        snapshot.id,
        "strong".into(),
        "experiment-preparation-v1".into(),
    )?;
    let response = block_on(strong.generate(RuntimeRequest {
        instruction: "prepare gated experiment".into(),
        input: serde_json::to_string(&snapshot).map_err(|e| Error::Validation(e.to_string()))?,
        schema: json!({"type":"object"}),
        max_output_tokens: 1024,
    }))?;
    let action =
        serde_json::from_value(response.output).map_err(|e| Error::Validation(e.to_string()))?;
    let mut context = ctx();
    context.run_id = Some(child.id);
    let anix_core::Submission::Gate(gate) = core.submit_proposal(
        p,
        ActionEnvelope::new(action),
        context,
        "experiment design".into(),
    )?
    else {
        return Err(Error::Validation("gate expected".into()));
    };
    let ActionResult::Entity(EntityRef::Experiment(experiment)) =
        core.approve_gate(p, gate, ctx())?
    else {
        return Err(Error::Validation("experiment expected".into()));
    };
    assert_eq!(
        core.runtime_state(p)?.runs[&child.id].status,
        RunStatus::Running
    );
    assert_eq!(
        core.runtime_state(p)?.work_nodes[&design.id].status,
        WorkStatus::Running
    );
    core.execute(
        p,
        ActionEnvelope::new(ProposedAction::ActivateExperiment {
            experiment_id: experiment,
            expected_version: 1,
        }),
        ctx(),
    )?;
    let attempt = core.start_experiment_run(p, experiment, 2, vec![], ctx())?;
    core.finish_experiment_run(
        p,
        anix_core::FinishExperimentRun {
            id: attempt.id,
            expected_version: 1,
            status: RunStatus::Completed,
            outputs: vec![],
            metadata: json!({"cycles":10}),
        },
        ctx(),
    )?;
    core.execute(
        p,
        ActionEnvelope::new(ProposedAction::CreateFinding(NewFinding {
            experiment_run_id: attempt.id,
            title: "Measured lifetime".into(),
            summary: "Ten controlled cycles completed".into(),
            confidence: None,
        })),
        ctx(),
    )?;
    let finding = core
        .runtime_state(p)?
        .findings
        .values()
        .next()
        .cloned()
        .ok_or_else(|| Error::NotFound("finding".into()))?;
    core.set_work_contract(
        p,
        design.id,
        core.runtime_state(p)?.work_nodes[&design.id].meta.version,
        OutputContract {
            required_entities: vec![EntityRef::Finding(finding.id)],
            required_artifacts: vec![],
            minimum_children: 0,
        },
        ctx(),
    )?;
    core.finish_run(p, child.id, RunStatus::Completed, ctx())?;
    finish_work(&core, p, design.id)?;
    core.finish_run(p, parent.id, RunStatus::Completed, ctx())?;
    finish_work(&core, p, root.id)?;
    let state = core.runtime_state(p)?;
    assert_eq!(state.model_invocations.len(), 3);
    assert_eq!(
        state.runs[&run.id].provider_metadata["runtime_v2"]["usage"]["deterministic_operations"],
        1
    );
    assert_eq!(state.findings.len(), 1);
    assert!(core.resumable_work(p, b)?.is_empty());
    drop(strong);
    drop(durable);
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(state, core.runtime_state(p)?);
    assert!(!core.trace_evidence(p, hid)?.supporting.is_empty());
    Ok(())
}

#[test]
#[ignore = "subprocess fixture; invoked by crash_process_checkpoint_resume"]
fn crash_child_fixture() -> Result<()> {
    let root = std::env::var("ANIX_CRASH_FIXTURE").map_err(|e| Error::Validation(e.to_string()))?;
    let root = std::path::Path::new(&root);
    let core = Core::new(Arc::new(SqliteStorage::open(root.join("crash.sqlite"))?));
    let p = core
        .create_project("crash test".into(), "durable execution".into(), ctx())?
        .id;
    let b = core.create_branch(p, "main".into(), None, ctx())?.id;
    let q = core
        .add_question(p, "question".into(), "battery".into(), ctx())?
        .id;
    let a = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let z = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let c = work(&core, p, b, q, WorkGranularity::Atomic)?;
    finish_work(&core, p, a.id)?;
    finish_work(&core, p, z.id)?;
    move_to(&core, p, c.id, WorkStatus::Ready)?;
    move_to(&core, p, c.id, WorkStatus::Running)?;
    let task = core.create_task(p, "C".into(), "unfinished C".into(), ctx())?;
    let run = core.start_run(p, task.id, None, ctx())?;
    let binding = core.bind_run(p, run.id, c.id, ctx())?;
    let snapshot = core.compile_context(
        p,
        c.id,
        binding.plan_revision_id,
        vec![EntityRef::Question(q)],
        ctx(),
    )?;
    let checkpoint = core.checkpoint(
        p,
        ExecutionCheckpoint {
            id: CheckpointId::new(),
            meta: Meta::new(p),
            run_id: Some(run.id),
            work_node_id: Some(c.id),
            branch_id: b,
            plan_revision_id: binding.plan_revision_id,
            context_snapshot_id: Some(snapshot.id),
            boundary: CheckpointBoundary::Suspended,
            execution_state: json!({"schema_version":1,"next_step":"C","completed":[a.id,z.id]}),
        },
        ctx(),
    )?;
    std::fs::write(root.join("ids.json"),json!({"project":p,"branch":b,"a":a.id,"b":z.id,"c":c.id,"checkpoint":checkpoint.id,"run":run.id}).to_string()).map_err(|e|Error::Storage(e.to_string()))?;
    // No destructors, SQLite connection close, or executor cleanup are allowed to run.
    std::process::exit(73)
}
#[test]
fn crash_process_checkpoint_resume() -> Result<()> {
    let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let status = std::process::Command::new(
        std::env::current_exe().map_err(|e| Error::Storage(e.to_string()))?,
    )
    .args(["--ignored", "--exact", "crash_child_fixture"])
    .env("ANIX_CRASH_FIXTURE", tmp.path())
    .status()
    .map_err(|e| Error::Storage(e.to_string()))?;
    assert_eq!(status.code(), Some(73));
    let ids: Value = serde_json::from_slice(
        &std::fs::read(tmp.path().join("ids.json")).map_err(|e| Error::Storage(e.to_string()))?,
    )
    .map_err(|e| Error::Validation(e.to_string()))?;
    let p: ProjectId = serde_json::from_value(ids["project"].clone()).unwrap();
    let b: BranchId = serde_json::from_value(ids["branch"].clone()).unwrap();
    let c: WorkNodeId = serde_json::from_value(ids["c"].clone()).unwrap();
    let run: RunId = serde_json::from_value(ids["run"].clone()).unwrap();
    let checkpoint: CheckpointId = serde_json::from_value(ids["checkpoint"].clone()).unwrap();
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("crash.sqlite"),
    )?));
    let descriptor = core.resume_checkpoint(p, checkpoint)?;
    assert!(descriptor.can_execute);
    assert_eq!(
        descriptor.context.as_ref().unwrap().plan_revision_id,
        descriptor.plan.id
    );
    assert_eq!(
        core.resumable_work(p, b)?
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        vec![c]
    );
    core.finish_run(p, run, RunStatus::Completed, ctx())?;
    move_to(&core, p, c, WorkStatus::Succeeded)?;
    assert!(!core.resume_checkpoint(p, checkpoint)?.can_execute);
    assert!(core.resumable_work(p, b)?.is_empty());
    Ok(())
}
#[test]
fn bootstrap_partial_and_wrong_checksum_never_install() -> Result<()> {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    for partial in [true, false] {
        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|e| Error::Storage(e.to_string()))?;
        let address = listener
            .local_addr()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                if partial { 100 } else { 4 }
            )
            .unwrap();
            stream.write_all(b"GGUF").unwrap();
        });
        let mut manifest = ModelManifest::read(std::path::Path::new("../../models/manifest.toml"))?;
        manifest.size = if partial { 100 } else { 4 };
        manifest.url = format!("http://{address}/model");
        let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
        assert!(bootstrap(tmp.path(), &manifest).is_err());
        server.join().unwrap();
        assert!(!manifest.model_path(tmp.path()).exists());
        assert_eq!(
            std::fs::read_dir(tmp.path().join("models").join(&manifest.directory))
                .map_err(|e| Error::Storage(e.to_string()))?
                .count(),
            0
        );
    }
    Ok(())
}
#[test]
fn upgrade_existing_v1_database_preserves_knowledge_and_builds_fts() -> Result<()> {
    use sha2::{Digest, Sha256};
    let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let path = tmp.path().join("v1.sqlite");
    let p = ProjectId::new();
    let q = QuestionId::new();
    let db = rusqlite::Connection::open(&path).map_err(|e| Error::Storage(e.to_string()))?;
    let v1 = include_str!("../migrations/0001_initial.sql");
    db.execute_batch(v1)
        .map_err(|e| Error::Storage(e.to_string()))?;
    db.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL,checksum TEXT NOT NULL,applied_at TEXT NOT NULL);").map_err(|e|Error::Storage(e.to_string()))?;
    db.execute(
        "INSERT INTO schema_migrations VALUES(1,'initial',?,'2026-01-01T00:00:00Z')",
        [format!("{:x}", Sha256::digest(v1.as_bytes()))],
    )
    .map_err(|e| Error::Storage(e.to_string()))?;
    db.execute("INSERT INTO projects VALUES(?,?, 'legacy','legacy notes','active',1,0,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",[p.to_string(),p.to_string()]).map_err(|e|Error::Storage(e.to_string()))?;
    db.execute("INSERT INTO research_questions VALUES(?,?,'battery','measurement','active',1,0,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",[q.to_string(),p.to_string()]).map_err(|e|Error::Storage(e.to_string()))?;
    drop(db);
    let storage = SqliteStorage::open(&path)?;
    assert_eq!(storage.migration_versions()?, vec![1, 2, 3, 4, 5]);
    use anix_core::Storage;
    assert_eq!(storage.read(p)?.questions[&q].title, "battery");
    assert_eq!(
        storage.search(p, "battery", 10)?,
        vec![EntityRef::Question(q)]
    );
    Ok(())
}

#[test]
fn recorded_tools_offload_and_survive_restart() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let workspace = Arc::new(Workspace::open(tmp.path())?);
    workspace.write_new("notes/large.txt", &vec![b'a'; 2048])?;
    let w = work(&core, p, b, q, WorkGranularity::Atomic)?;
    let task = core.create_task(p, "read".into(), "read source".into(), ctx())?;
    let run = core.start_run(p, task.id, None, ctx())?;
    core.bind_run(p, run.id, w.id, ctx())?;
    let core = Arc::new(core);
    let tool = RecordedTool {
        core: core.clone(),
        inner: Arc::new(ReadFileTool::new(p, workspace.clone())),
        offloader: ArtifactOffloader {
            workspace,
            threshold_bytes: 100,
        },
        version: "1".into(),
    };
    let output = block_on(tool.execute(
        anix_core::ToolInput {
            schema_version: 1,
            parameters: json!({"path":"notes/large.txt"}),
        },
        anix_core::ToolContext {
            project_id: p,
            run_id: Some(run.id),
            context_snapshot_id: None,
            tool_invocation_id: None,
        },
    ))?;
    assert_eq!(output.data["type"], "artifact");
    assert!(output.data["summary"]
        .as_str()
        .unwrap()
        .contains("bounded JSON preview"));
    assert!(output.data["summary"].as_str().unwrap().len() < 1600);
    assert!(block_on(tool.execute(
        anix_core::ToolInput {
            schema_version: 1,
            parameters: json!({"path":"../outside.txt"})
        },
        anix_core::ToolContext {
            project_id: p,
            run_id: Some(run.id),
            context_snapshot_id: None,
            tool_invocation_id: None,
        }
    ))
    .is_err());
    let state = core.runtime_state(p)?;
    assert_eq!(state.tool_invocations.len(), 2);
    assert!(state
        .tool_invocations
        .values()
        .any(|i| i.status == InvocationStatus::Failed));
    let offloaded_id: ArtifactId = output.data["id"].as_str().unwrap().parse().unwrap();
    let artifact = state
        .artifacts
        .get(&offloaded_id)
        .expect("registered offload artifact");
    assert_eq!(artifact.name, "offloaded tool output");
    assert!(!artifact.path.is_empty());
    assert_eq!(
        artifact.content_hash,
        output.data["content_hash"].as_str().unwrap()
    );
    assert!(state
        .tool_invocations
        .values()
        .any(|invocation| invocation.output_artifacts.contains(&offloaded_id)));
    assert!(state
        .checkpoints
        .values()
        .any(|c| c.boundary == CheckpointBoundary::BeforeSideEffect));
    assert!(state
        .checkpoints
        .values()
        .any(|c| c.boundary == CheckpointBoundary::AfterSideEffect));
    drop(tool);
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(state, core.runtime_state(p)?);
    Ok(())
}
#[test]
#[ignore = "real model and llama.cpp binary required; explicit non-CI smoke"]
fn real_llama_smoke_missing_bad_model_concurrency_shutdown() -> Result<()> {
    let root = std::path::Path::new("../..");
    let manifest = ModelManifest::read(&root.join("models/manifest.toml"))?;
    let executable = root.join(".runtime/llama-b11307/llama-server.exe");
    let path = manifest.model_path(root);
    manifest.verify(&path)?;
    assert!(LlamaCppRuntime::start(
        &executable,
        &root.join("missing.gguf"),
        "missing".into(),
        "missing".into()
    )
    .is_err());
    let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let bad = tmp.path().join("bad.gguf");
    std::fs::write(&bad, b"invalid model").map_err(|e| Error::Storage(e.to_string()))?;
    assert!(LlamaCppRuntime::start(&executable, &bad, "bad".into(), "bad".into()).is_err());
    let runtime = Arc::new(LlamaCppRuntime::start(
        &executable,
        &path,
        manifest.logical_name,
        manifest.revision,
    )?);
    let pid = runtime.pid()?;
    let input = json!({"text":"Photons propagate in a vacuum.","labels":["biology","physics"]});
    let request = RuntimeRequest {
        instruction: BuiltinMicroTemplates.instruction(MicroTask::Classification),
        input: input.to_string(),
        schema: task_schema(MicroTask::Classification, &input),
        max_output_tokens: 32,
    };
    let r = runtime.clone();
    let req = request.clone();
    let thread = std::thread::spawn(move || block_on(r.generate(req)));
    let response = block_on(runtime.generate(request))?;
    assert_eq!(response.output["label"], "physics");
    assert_eq!(thread.join().unwrap()?.output["label"], "physics");
    drop(runtime);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = format!(
            "if(Get-Process -Id {pid} -ErrorAction SilentlyContinue){{exit 1}}else{{exit 0}}"
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(0x08000000)
            .status()
            .map_err(|e| Error::Storage(e.to_string()))?;
        assert!(status.success());
    }
    Ok(())
}

#[test]
fn measured_model_profile_routes_only_passed_tasks_locally() -> Result<()> {
    let root = std::path::Path::new("../..");
    let manifest = ModelManifest::read(&root.join("models/manifest.toml"))?;
    let registry = measured_capabilities(
        &root.join("eval/results-qwen3-1.7b-micro-v1-rerun.json"),
        &root.join("eval/results-qwen3-1.7b-real-micro-eval.json"),
        &manifest,
    )?;
    assert_eq!(registry.local_quality[&MicroTask::Classification], 0.44);
    assert_eq!(registry.local_quality[&MicroTask::Tagging], 0.4);
    assert_eq!(registry.local_quality[&MicroTask::QueryRewrite], 1.0);
    assert!(!registry
        .local_quality
        .contains_key(&MicroTask::CandidateExtraction));
    let local = Arc::new(FakeModelRuntime::new(vec![response(
        json!({"query":"search for photon studies"}),
    )]));
    let strong = Arc::new(FakeModelRuntime::new(vec![
        response(json!({"tags":["Physics"]})),
        response(json!({"label":"physics"})),
        response(json!({"candidates":[]})),
    ]));
    let mut r = router(local.clone(), strong.clone());
    r.registry = registry;
    let mut req = requirements();
    req.task_kind = "query_rewrite".into();
    req.micro_task = Some(MicroTask::QueryRewrite);
    req.quality_threshold = 0.0;
    let input = json!({"query":"photon studies","mandatory_terms":[]});
    block_on(r.execute(
        &req,
        input.clone(),
        task_schema(MicroTask::QueryRewrite, &input),
    ))?;
    assert_eq!(local.requests.lock().unwrap().len(), 1);
    assert!(strong.requests.lock().unwrap().is_empty());
    let mut tagging = requirements();
    tagging.task_kind = "tagging".into();
    tagging.micro_task = Some(MicroTask::Tagging);
    tagging.quality_threshold = 0.0;
    let input = json!({"text":"photon","allowed_tags":["Physics"]});
    block_on(r.execute(
        &tagging,
        input.clone(),
        task_schema(MicroTask::Tagging, &input),
    ))?;
    assert_eq!(strong.requests.lock().unwrap().len(), 1);
    let mut classify = requirements();
    classify.task_kind = "classification".into();
    classify.micro_task = Some(MicroTask::Classification);
    let input = json!({"text":"photon","labels":["biology","physics"]});
    block_on(r.execute(
        &classify,
        input.clone(),
        task_schema(MicroTask::Classification, &input),
    ))?;
    assert_eq!(strong.requests.lock().unwrap().len(), 2);
    let mut extraction = requirements();
    extraction.task_kind = "candidate_extraction".into();
    extraction.micro_task = Some(MicroTask::CandidateExtraction);
    let input = json!({"text":"A fact.","maximum_candidates":1});
    block_on(r.execute(
        &extraction,
        input.clone(),
        task_schema(MicroTask::CandidateExtraction, &input),
    ))?;
    assert_eq!(strong.requests.lock().unwrap().len(), 3);
    Ok(())
}
#[test]
fn versioned_context_provider_payload_survives_restart() -> Result<()> {
    let (tmp, core, p, b, q) = setup()?;
    let w = work(&core, p, b, q, WorkGranularity::Atomic)?;
    core.write_memory(
        p,
        MemoryNamespace::ProcessMemory,
        Some(w.id),
        "calibration remains pending".into(),
        ctx(),
    )?;
    let revision = core.runtime_state(p)?.branches[&b]
        .head_revision_id
        .unwrap();
    let snapshot = core.compile_context_manifest(
        p,
        anix_core::ContextCompileRequest {
            work_node_id: w.id,
            plan_revision_id: revision,
            pinned: vec![EntityRef::Question(q)],
            instruction_version: "research-instructions@2".into(),
            skill_versions: vec!["evidence_extraction@1".into()],
        },
        ctx(),
    )?;
    let context = core.provider_context(p, snapshot.id)?;
    assert!(context
        .process_notes
        .iter()
        .any(|note| note == "calibration remains pending"));
    assert!(context
        .process_notes
        .iter()
        .any(|note| note.starts_with("Decision state: ")));
    assert_eq!(context.skill_versions, vec!["evidence_extraction@1"]);
    assert!(serde_json::to_vec(&context).unwrap().len() <= snapshot.budget_tokens as usize);
    let frozen = serde_json::to_value(context).unwrap();
    drop(core);
    let core = Core::new(Arc::new(SqliteStorage::open(
        tmp.path().join("state.sqlite"),
    )?));
    assert_eq!(
        serde_json::to_value(core.provider_context(p, snapshot.id)?).unwrap(),
        frozen
    );
    Ok(())
}

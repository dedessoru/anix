use anix_agent::*;
use anix_core::{
    CommandContext, Core, NewClaim, NewPaper, NewSource, Submission, Tool, ToolContext,
    ToolDescriptor, ToolFuture, ToolInput, ToolOutput,
};
use anix_domain::*;
use anix_integrations::Workspace;
use anix_storage::SqliteStorage;
use futures::executor::block_on;
use serde_json::json;
use std::sync::Arc;

struct EchoResearchTool;
impl Tool for EchoResearchTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "test_echo".into(),
            description: "Return a bounded synthetic observation".into(),
            read_only: true,
        }
    }
    fn execute(&self, input: ToolInput, _context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            Ok(ToolOutput {
                schema_version: 1,
                data: json!({"observation":input.parameters["value"]}),
                metadata: json!({"schema_version":1}),
            })
        })
    }
}

fn human() -> CommandContext {
    CommandContext::human("researcher", "Explicit research decision")
}

fn response(decision: AgentDecision) -> ModelResponse {
    ModelResponse {
        schema_version: 1,
        decision,
        metadata: json!({"input_tokens":20,"output_tokens":18}),
    }
}
fn hypothesis(core: &Core, p: ProjectId) -> Result<HypothesisId> {
    match core.hypotheses().create(
        p,
        NewHypothesis {
            question_id: None,
            title: "H1".into(),
            statement: "A improves B".into(),
        },
        human(),
    )? {
        ActionResult::Entity(EntityRef::Hypothesis(id)) => Ok(id),
        _ => Err(Error::Validation("unexpected result".into())),
    }
}
fn selector() -> Arc<dyn ModelSelector> {
    Arc::new(FixedModelSelector {
        selection: ModelSelection {
            provider: "fake".into(),
            model: "offline".into(),
        },
    })
}

#[test]
fn complete_question_to_finding_and_restart() -> Result<()> {
    let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let workspace = Workspace::open(temp.path())?;
    let db_path = workspace.database_path()?;
    let project;
    let hid;
    let finding_id;
    let experiment_id;
    let gate_id;
    let attempt_id;
    let events;
    let provenance;
    {
        let storage = Arc::new(SqliteStorage::open(&db_path)?);
        assert_eq!(storage.migration_versions()?, vec![1, 2, 3, 4, 5]);
        let core = Core::new(storage);
        let rx = core.subscribe()?;
        project = core
            .projects()
            .create(
                "Battery study".into(),
                "Compare electrode A".into(),
                human(),
            )?
            .id;
        let question = core.questions().add(
            project,
            "Does A improve lifetime?".into(),
            "Controlled comparison".into(),
            human(),
        )?;
        let source = core.sources().add(
            project,
            NewSource {
                title: "Paper PDF".into(),
                source_kind: SourceKind::Pdf,
                locator: "sources/study.pdf".into(),
                checksum: None,
                metadata: json!({"schema_version":1}),
            },
            human(),
        )?;
        let paper = core.papers().add(
            project,
            NewPaper {
                title: "Electrode lifetime".into(),
                authors: vec!["A. Researcher".into()],
                publication_year: Some(2025),
                doi: Some("10.0000/example".into()),
                abstract_text: "Lifetime comparison".into(),
                source_ids: vec![source.id],
                metadata: json!({"schema_version":1}),
            },
            human(),
        )?;
        let claim = core.claims().add(
            project,
            NewClaim {
                paper_id: paper.id,
                source_id: Some(source.id),
                text: "A increases lifetime by 10%".into(),
                location: ClaimLocation {
                    page: Some(3),
                    section: Some("Results".into()),
                    ..Default::default()
                },
            },
            human(),
        )?;
        hid = match core.hypotheses().create(
            project,
            NewHypothesis {
                question_id: Some(question.id),
                title: "H1".into(),
                statement: "A improves lifetime".into(),
            },
            human(),
        )? {
            ActionResult::Entity(EntityRef::Hypothesis(id)) => id,
            _ => return Err(Error::Validation("hypothesis result".into())),
        };
        core.graph().create(
            project,
            NewRelation {
                from_entity: EntityRef::Claim(claim.id),
                to_entity: EntityRef::Hypothesis(hid),
                kind: RelationKind::Supports,
                confidence: Some(0.8),
            },
            human(),
        )?;
        let provider = Arc::new(FakeModelProvider::new(vec![ModelResponse {
            schema_version: 1,
            decision: AgentDecision::Propose {
                action: ProposedAction::CreateExperiment(NewExperiment {
                    hypothesis_id: hid,
                    expected_version: 1,
                    title: "Controlled cycle test".into(),
                    objective: "Measure lifetime".into(),
                    methodology: "Compare A and baseline".into(),
                    protocol: "1000 charge cycles".into(),
                }),
                reason: "Test the literature claim experimentally".into(),
            },
            metadata: json!({"schema_version":1,"tokens":100}),
        }]));
        let outcome = block_on(
            LeadAgent::new(core.agent_api("lead-1")?, provider.clone(), selector()).run(
                project,
                "Propose an experiment".into(),
                Some(EntityRef::Hypothesis(hid)),
                None,
            ),
        )?;
        gate_id = match outcome.submission {
            Some(Submission::Gate(id)) => id,
            _ => return Err(Error::Validation("agent did not create gate".into())),
        };
        assert_eq!(
            core.runs().get(project, outcome.run_id)?.status,
            RunStatus::WaitingForHuman
        );
        assert_eq!(core.gates().list(project, true)?.len(), 1);
        let requests = provider.requests()?;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].context.claims[0].id, claim.id);
        assert_eq!(requests[0].context.hypotheses[0].id, hid);
        experiment_id = match core.gates().approve(project, gate_id, human())? {
            ActionResult::Entity(EntityRef::Experiment(id)) => id,
            _ => return Err(Error::Validation("experiment result".into())),
        };
        let gate = core.gates().get(project, gate_id)?;
        assert_eq!(gate.created_by, Actor::LeadAgent("lead-1".into()));
        assert_eq!(gate.resolved_by, Some(Actor::Human("researcher".into())));
        assert_eq!(
            core.runs().get(project, outcome.run_id)?.status,
            RunStatus::Completed
        );
        let event_count = core.events().timeline(project)?.len();
        assert_eq!(
            core.gates().approve(project, gate_id, human())?,
            ActionResult::Entity(EntityRef::Experiment(experiment_id))
        );
        assert_eq!(core.events().timeline(project)?.len(), event_count);
        core.execute(
            project,
            ActionEnvelope::new(ProposedAction::ActivateExperiment {
                experiment_id,
                expected_version: 1,
            }),
            human(),
        )?;
        let attempt = core.start_experiment_run(project, experiment_id, 2, vec![], human())?;
        attempt_id = attempt.id;
        let output = workspace.write_new("data/results.csv", b"group,lifetime\nA,110\nB,100\n")?;
        assert!(workspace.verify(&output)?);
        let artifact = core.register_artifact(
            project,
            anix_core::NewArtifact {
                name: "Cycle results".into(),
                artifact_kind: ArtifactKind::Csv,
                path: output.path,
                content_hash: output.content_hash,
                byte_size: output.byte_size,
                metadata: json!({"schema_version":1}),
            },
            human(),
        )?;
        core.finish_experiment_run(
            project,
            anix_core::FinishExperimentRun {
                id: attempt.id,
                expected_version: 1,
                status: RunStatus::Completed,
                outputs: vec![artifact.id],
                metadata: json!({"schema_version":1,"cycles":1000}),
            },
            human(),
        )?;
        finding_id = match core.findings().add(
            project,
            NewFinding {
                experiment_run_id: attempt.id,
                title: "Lifetime improvement".into(),
                summary: "A lasted 10% longer".into(),
                confidence: Some(0.9),
            },
            human(),
        )? {
            ActionResult::Entity(EntityRef::Finding(id)) => id,
            _ => return Err(Error::Validation("finding result".into())),
        };
        core.graph().create(
            project,
            NewRelation {
                from_entity: EntityRef::Finding(finding_id),
                to_entity: EntityRef::Hypothesis(hid),
                kind: RelationKind::Supports,
                confidence: Some(0.9),
            },
            human(),
        )?;
        let trace = core.evidence().trace(project, hid)?;
        assert_eq!(trace.supporting.len(), 2);
        assert!(trace.supporting.iter().any(|i| i
            .paper
            .as_ref()
            .is_some_and(|p| p.id == paper.id)
            && i.sources.iter().any(|s| s.id == source.id)));
        assert!(trace.supporting.iter().any(|i| i
            .finding
            .as_ref()
            .is_some_and(|f| f.id == finding_id)
            && i.experiment_run
                .as_ref()
                .is_some_and(|r| r.id == attempt.id)));
        let state = core.research().state(project)?;
        assert_eq!(state.lifecycle.counts["findings"], 1);
        assert!(state.active_runs.is_empty());
        workspace.rebuild_projections(&state.hypotheses, &state.findings)?;
        events = core.events().timeline(project)?;
        provenance = core.provenance().list(project, None)?;
        assert!(provenance.iter().any(|p| p.action == "GateCreated"
            && p.actor == Actor::LeadAgent("lead-1".into())
            && p.gate_id == Some(gate_id)));
        assert!(provenance.iter().any(|p| p.action == "ExperimentCreated"
            && p.actor == Actor::Human("researcher".into())
            && p.gate_id == Some(gate_id)));
        let live: Vec<_> = rx.try_iter().collect();
        assert_eq!(live, events);
    }
    let core = Core::new(Arc::new(SqliteStorage::open(&db_path)?));
    assert_eq!(core.events().timeline(project)?, events);
    assert_eq!(core.provenance().list(project, None)?, provenance);
    assert_eq!(
        core.gates().get(project, gate_id)?.status,
        GateStatus::Approved
    );
    assert_eq!(
        core.get_experiment_run(project, attempt_id)?.status,
        RunStatus::Completed
    );
    assert_eq!(
        core.experiments().get(project, experiment_id)?.status,
        ExperimentStatus::Completed
    );
    assert!(core
        .evidence()
        .trace(project, hid)?
        .supporting
        .iter()
        .any(|i| i.finding.as_ref().is_some_and(|f| f.id == finding_id)));
    Ok(())
}

#[test]
fn iterative_step_persists_context_tool_and_work_plan() -> Result<()> {
    let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let workspace = Arc::new(Workspace::open(temp.path())?);
    let db_path = workspace.database_path()?;
    let core = Arc::new(Core::new(Arc::new(SqliteStorage::open(&db_path)?)));
    let project = core
        .projects()
        .create(
            "Long horizon fixture".into(),
            "Persist one bounded decision per Lead call".into(),
            human(),
        )?
        .id;
    let question = core.questions().add(
        project,
        "Does X affect Y?".into(),
        "Synthetic long-horizon question".into(),
        human(),
    )?;
    let branch = core.create_branch(project, "main".into(), None, human())?;
    let root = core.create_work(
        project,
        anix_core::NewWork {
            branch_id: branch.id,
            kind: "research-goal".into(),
            granularity: WorkGranularity::Composite,
            title: "Investigate X and Y".into(),
            objective: "Find and assess evidence about X and Y".into(),
            acceptance_criteria: OutputContract {
                required_entities: vec![EntityRef::Question(question.id)],
                required_artifacts: vec![],
                minimum_children: 1,
            },
            capability_requirements: vec![],
            context_policy_id: "long-horizon-test".into(),
            budget: WorkBudget {
                context_tokens: 8192,
                max_model_calls: 40,
                max_tool_calls: 60,
            },
        },
        human(),
    )?;
    let root = core.transition_work(
        project,
        root.id,
        root.meta.version,
        WorkStatus::Ready,
        human(),
    )?;

    let api = core.agent_api("long-horizon-test-lead")?;
    let root_task = api.create_task(project, "Long-horizon research root".into())?;
    let root_run = api.start_run(project, root_task.id, None)?;
    api.bind_run(project, root_run.id, root.id)?;

    let mut traced_tool_response = response(AgentDecision::UseTool {
        name: "test_echo".into(),
        input: ToolInput {
            schema_version: 1,
            parameters: json!({"value":"Study A observed a 12 percent change"}),
        },
    });
    let rendered_schema = json!({"type":"object","properties":{"action":{"const":"use_tool"}}});
    traced_tool_response.metadata["final_action"] = json!("use_tool");
    traced_tool_response.metadata["diagnostic_trace"] = json!({
        "trace_version":1,
        "rendered_messages":[{"role":"user","content":"actual rendered diagnostic request"}],
        "action_schema":rendered_schema,
        "allowed_tool_names":["test_echo"],
        "raw_completion":"{\"action\":{\"type\":\"test_echo\"}}",
        "finish_reason":"stop",
        "parsed_action":{"type":"use_tool","name":"test_echo"},
    });
    let provider = Arc::new(FakeModelProvider::new(vec![
        response(AgentDecision::Process {
            action: ProcessAction::CreateWorkNode {
                title: "Inspect one evidence source".into(),
                objective: "Read a source and preserve its observation".into(),
                required_tool: "test_echo".into(),
            },
            rationale: "The root needs a bounded evidence task".into(),
            expected_result: Some("A durable tool observation".into()),
        }),
        traced_tool_response,
    ]));
    let mut tools = ApprovedTools::default();
    tools.register(Arc::new(anix_integrations::RecordedTool {
        core: core.clone(),
        inner: Arc::new(EchoResearchTool),
        offloader: anix_integrations::ArtifactOffloader {
            workspace: workspace.clone(),
            threshold_bytes: 16_384,
        },
        version: "1".into(),
    }))?;
    let lead = LeadAgent::new(
        core.agent_api("long-horizon-test-lead")?,
        provider.clone(),
        selector(),
    )
    .with_tools(tools);
    let planned = block_on(lead.run_step(
        project,
        "Plan one evidence step".into(),
        Some(EntityRef::Question(question.id)),
        Some(root_run.id),
        root.id,
    ))?;
    assert!(matches!(
        planned.decision,
        AgentDecision::Process {
            action: ProcessAction::CreateWorkNode { .. },
            ..
        }
    ));
    let state = core.runtime_state(project)?;
    let child = state
        .work_edges
        .values()
        .find(|e| e.from == root.id && e.kind == WorkEdgeKind::Contains)
        .unwrap()
        .to;
    let child_task = api.create_task(project, "Attempt premature completion".into())?;
    let child_run = api.start_run(project, child_task.id, Some(root_run.id))?;
    api.bind_run(project, child_run.id, child)?;
    assert!(api.complete_work(project, child_run.id, child).is_err());
    assert_ne!(
        core.agent_api("long-horizon-test-lead")?
            .work_node(project, child)?
            .status,
        WorkStatus::Succeeded
    );
    api.finish(project, child_run.id, RunStatus::Failed)?;
    core.agent_api("long-horizon-test-lead")?
        .write_process_memory(
            project,
            root_run.id,
            root.id,
            "Evidence objective: assess the measured change; source search remains open.".into(),
        )?;

    let tool_step = block_on(lead.run_step(
        project,
        "Read the assigned evidence source".into(),
        Some(EntityRef::Question(question.id)),
        Some(root_run.id),
        child,
    ))?;
    assert!(matches!(tool_step.decision, AgentDecision::UseTool { .. }));
    assert_eq!(
        tool_step.tool_output.as_ref().unwrap().data["observation"],
        "Study A observed a 12 percent change"
    );
    let requests = provider.requests()?;
    assert!(requests[1]
        .compiled_context
        .as_ref()
        .unwrap()
        .process_notes
        .iter()
        .any(|m| m.contains("Evidence objective")));
    let state = core.runtime_state(project)?;
    let traced = state
        .model_invocations
        .values()
        .find(|invocation| invocation.metadata["diagnostic_trace"].is_object())
        .unwrap();
    assert_eq!(
        traced.metadata["diagnostic_trace"]["identity"]["model_invocation_id"],
        json!(traced.id)
    );
    assert_eq!(
        traced.metadata["diagnostic_trace"]["identity"]["context_snapshot_id"],
        json!(traced.context_snapshot_id)
    );
    assert_eq!(traced.output_schema, rendered_schema);
    assert_eq!(
        traced.metadata["diagnostic_trace"]["raw_completion"],
        "{\"action\":{\"type\":\"test_echo\"}}"
    );
    assert_eq!(
        traced.metadata["diagnostic_trace"]["execution"]["validation"]["step_contract"],
        true
    );
    assert_eq!(
        traced.metadata["diagnostic_trace"]["execution"]["execution_status"],
        "completed"
    );
    assert_eq!(
        traced.metadata["diagnostic_trace"]["execution"]["progress_before"]
            ["successful_source_reads"],
        0
    );
    assert_eq!(
        traced.metadata["diagnostic_trace"]["execution"]["progress_after"]["work_status"],
        "running"
    );
    assert_eq!(
        traced.metadata["diagnostic_trace"]["execution"]["tool_output"]["data"]["observation"],
        "Study A observed a 12 percent change"
    );
    assert_eq!(state.tool_invocations.len(), 1);
    assert_eq!(
        state.tool_invocations.values().next().unwrap().status,
        InvocationStatus::Succeeded
    );
    assert!(!state
        .tool_invocations
        .values()
        .next()
        .unwrap()
        .output_artifacts
        .is_empty());

    drop(lead);
    let state = core.runtime_state(project)?;
    assert_eq!(state.model_invocations.len(), 2);
    assert_eq!(
        state
            .process_memories
            .values()
            .filter(|memory| memory.content.starts_with("Evidence objective: "))
            .count(),
        1
    );
    assert_eq!(
        state
            .process_memories
            .values()
            .filter(|memory| memory.content.starts_with("Decision state: "))
            .count(),
        2
    );
    assert_eq!(
        state
            .process_memories
            .values()
            .filter(|memory| memory.content.starts_with("Planning observation: "))
            .count(),
        1
    );
    assert!(state.context_snapshots.len() >= 2);
    Ok(())
}

#[test]
fn lead_created_branch_is_checkpoint_based_and_survives_restart() -> Result<()> {
    let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let workspace = Arc::new(Workspace::open(temp.path())?);
    let db_path = workspace.database_path()?;
    let core = Arc::new(Core::new(Arc::new(SqliteStorage::open(&db_path)?)));
    let project = core
        .projects()
        .create(
            "Branch capability fixture".into(),
            "Independent paths".into(),
            human(),
        )?
        .id;
    let question = core.questions().add(
        project,
        "Which of two mechanisms best explains the observation?".into(),
        "The mechanisms require independent evidence review.".into(),
        human(),
    )?;
    let main = core.create_branch(project, "main".into(), None, human())?;
    let root = core.create_work(
        project,
        anix_core::NewWork {
            branch_id: main.id,
            kind: "research-goal".into(),
            granularity: WorkGranularity::Composite,
            title: "Compare independent mechanisms".into(),
            objective: "Investigate two competing mechanisms independently".into(),
            acceptance_criteria: OutputContract {
                required_entities: vec![EntityRef::Question(question.id)],
                required_artifacts: vec![],
                minimum_children: 0,
            },
            capability_requirements: vec![],
            context_policy_id: "branch-capability-test".into(),
            budget: WorkBudget::default(),
        },
        human(),
    )?;
    let root = core.transition_work(
        project,
        root.id,
        root.meta.version,
        WorkStatus::Ready,
        human(),
    )?;
    let api = core.agent_api("branch-capability-lead")?;
    let parent_task = api.create_task(project, "Branch capability root".into())?;
    let parent_run = api.start_run(project, parent_task.id, None)?;
    api.bind_run(project, parent_run.id, root.id)?;
    let provider = Arc::new(FakeModelProvider::new(vec![response(
        AgentDecision::Process {
            action: ProcessAction::CreateBranch {
                name: "mechanism-a".into(),
            },
            rationale: "The evidence question has independent mechanisms".into(),
            expected_result: Some("An isolated mechanism branch".into()),
        },
    )]));
    let lead = LeadAgent::new(api, provider, selector());
    let step = block_on(lead.run_step(
        project,
        "Investigate each mechanism on an independent research branch".into(),
        Some(EntityRef::Question(question.id)),
        Some(parent_run.id),
        root.id,
    ))?;
    assert!(matches!(
        step.decision,
        AgentDecision::Process {
            action: ProcessAction::CreateBranch { .. },
            ..
        }
    ));
    let state = core.runtime_state(project)?;
    let child = state
        .branches
        .values()
        .find(|branch| branch.name == "mechanism-a")
        .ok_or_else(|| Error::NotFound("Lead-created branch".into()))?;
    assert_eq!(child.parent_branch_id, Some(main.id));
    let base = child
        .base_checkpoint_id
        .ok_or_else(|| Error::NotFound("branch base checkpoint".into()))?;
    assert_eq!(state.checkpoints[&base].branch_id, main.id);
    let branch_work: Vec<_> = state
        .work_nodes
        .values()
        .filter(|work| work.branch_id == child.id)
        .collect();
    assert_eq!(branch_work.len(), 1);
    assert_ne!(branch_work[0].id, root.id);
    let branch_work_id = branch_work[0].id;
    drop(lead);
    drop(core);
    let restored = Core::new(Arc::new(SqliteStorage::open(&db_path)?));
    let state = restored.runtime_state(project)?;
    assert_eq!(state.branches[&child.id].parent_branch_id, Some(main.id));
    assert_eq!(state.work_nodes[&branch_work_id].branch_id, child.id);
    assert_eq!(state.checkpoints[&base].branch_id, main.id);
    Ok(())
}

#[test]
fn child_branch_authorization_is_run_project_and_parent_scoped() -> Result<()> {
    let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let core = Core::new(Arc::new(SqliteStorage::open(
        temp.path().join("auth.sqlite"),
    )?));
    let project_a = core
        .projects()
        .create("Auth A".into(), "".into(), human())?
        .id;
    let project_b = core
        .projects()
        .create("Auth B".into(), "".into(), human())?
        .id;
    let identity = "scoped-branch-lead";
    let api = core.agent_api(identity)?;

    let start_branch_run =
        |project: ProjectId, name: &str| -> Result<(BranchId, RunId, CheckpointId)> {
            let branch = core.create_branch(project, name.into(), None, human())?;
            let work = core.create_work(
                project,
                anix_core::NewWork {
                    branch_id: branch.id,
                    kind: "authorization-test".into(),
                    granularity: WorkGranularity::Composite,
                    title: format!("{name} research root"),
                    objective: "Exercise branch creation scope".into(),
                    acceptance_criteria: OutputContract {
                        required_entities: vec![],
                        required_artifacts: vec![],
                        minimum_children: 0,
                    },
                    capability_requirements: vec![],
                    context_policy_id: "branch-auth-test".into(),
                    budget: WorkBudget::default(),
                },
                human(),
            )?;
            let work = core.transition_work(
                project,
                work.id,
                work.meta.version,
                WorkStatus::Ready,
                human(),
            )?;
            let task = api.create_task(project, format!("{name} task"))?;
            let run = api.start_run(project, task.id, None)?;
            api.bind_run(project, run.id, work.id)?;
            let binding = core.runtime_state(project)?.execution_bindings[&run.id].clone();
            let checkpoint = core.checkpoint(
                project,
                ExecutionCheckpoint {
                    id: CheckpointId::new(),
                    meta: Meta::new(project),
                    run_id: Some(run.id),
                    work_node_id: Some(work.id),
                    branch_id: branch.id,
                    plan_revision_id: binding.plan_revision_id,
                    context_snapshot_id: None,
                    boundary: CheckpointBoundary::Suspended,
                    execution_state: serde_json::json!({"fixture":true}),
                },
                CommandContext {
                    actor: Actor::Human("researcher".into()),
                    reason: "create scoped authorization fixture checkpoint".into(),
                    run_id: Some(run.id),
                    correlation_id: None,
                    causation_id: None,
                },
            )?;
            Ok((branch.id, run.id, checkpoint.id))
        };

    let (branch_a, run_a, checkpoint_a) = start_branch_run(project_a, "main-a")?;
    let (branch_b, run_b, checkpoint_b) = start_branch_run(project_a, "other-parent")?;
    let unauthorized = core.create_branch(
        project_a,
        "tool-forbidden".into(),
        Some(checkpoint_a),
        CommandContext {
            actor: Actor::Tool("untrusted-tool".into()),
            reason: "attempt branch mutation".into(),
            run_id: Some(run_a),
            correlation_id: None,
            causation_id: None,
        },
    );
    assert!(
        matches!(unauthorized, Err(Error::AuthorizationDenied(trace)) if trace.action == "CreateChildBranch" && trace.result == "deny")
    );

    let cross_project = api.create_branch(project_b, run_a, "cross-project".into());
    assert!(cross_project.is_err());

    let wrong_parent = core.create_branch(
        project_a,
        "wrong-parent".into(),
        Some(checkpoint_b),
        CommandContext {
            actor: Actor::LeadAgent(identity.into()),
            reason: "attempt unrelated parent split".into(),
            run_id: Some(run_a),
            correlation_id: None,
            causation_id: None,
        },
    );
    assert!(
        matches!(wrong_parent, Err(Error::AuthorizationDenied(trace)) if trace.branch_id == Some(branch_a.to_string()) && trace.parent_branch_id == Some(branch_b.to_string()) && trace.reason.contains("outside the authorized parent branch"))
    );

    api.finish(project_a, run_b, RunStatus::Completed)?;
    api.finish(project_a, run_a, RunStatus::Completed)?;
    let stale = core.create_branch(
        project_a,
        "stale-run".into(),
        Some(checkpoint_a),
        CommandContext {
            actor: Actor::LeadAgent(identity.into()),
            reason: "attempt stale split".into(),
            run_id: Some(run_a),
            correlation_id: None,
            causation_id: None,
        },
    );
    assert!(
        matches!(stale, Err(Error::AuthorizationDenied(trace)) if trace.reason.contains("run is not active"))
    );
    Ok(())
}

#[test]
fn stale_gate_is_rejected_without_audit_or_live_events() -> Result<()> {
    let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let path = temp.path().join("state.sqlite");
    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    let p = core.create_project("Study".into(), "".into(), human())?.id;
    let h = hypothesis(&core, p)?;
    let action = ActionEnvelope::new(ProposedAction::ReviseHypothesis {
        hypothesis_id: h,
        expected_version: 1,
        statement: "Proposal".into(),
    });
    let gate = match core.submit_proposal(p, action, human(), "semantic edit".into())? {
        Submission::Gate(id) => id,
        _ => return Err(Error::Validation("gate".into())),
    };
    // Independent connection models a concurrent human client.
    let second = Core::new(Arc::new(SqliteStorage::open(&path)?));
    second.execute(
        p,
        ActionEnvelope::new(ProposedAction::ReviseHypothesis {
            hypothesis_id: h,
            expected_version: 1,
            statement: "New human state".into(),
        }),
        human(),
    )?;
    let before = core.events().timeline(p)?;
    let rx = core.subscribe()?;
    assert!(matches!(
        core.approve_gate(p, gate, human()),
        Err(Error::StaleProposal(_))
    ));
    assert_eq!(core.events().timeline(p)?, before);
    assert_eq!(core.get_gate(p, gate)?.status, GateStatus::Pending);
    assert_eq!(core.get_hypothesis(p, h)?.statement, "New human state");
    assert!(rx.try_recv().is_err());
    Ok(())
}

#[test]
fn failed_persistence_rolls_back_gate_state_events_and_provenance() -> Result<()> {
    let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let path = temp.path().join("state.sqlite");
    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    let p = core
        .create_project("Rollback".into(), "".into(), human())?
        .id;
    let h = hypothesis(&core, p)?;
    let action = ActionEnvelope::new(ProposedAction::ReviseHypothesis {
        hypothesis_id: h,
        expected_version: 1,
        statement: "Changed".into(),
    });
    let gate = match core.submit_proposal(p, action, human(), "edit".into())? {
        Submission::Gate(id) => id,
        _ => return Err(Error::Validation("gate".into())),
    };
    let db = rusqlite::Connection::open(&path).map_err(|e| Error::Storage(e.to_string()))?;
    db.execute_batch("CREATE TRIGGER reject_event BEFORE INSERT ON domain_events WHEN NEW.event_type='GateApproved' BEGIN SELECT RAISE(ABORT,'injected write failure'); END;").map_err(|e|Error::Storage(e.to_string()))?;
    let before = core.events().timeline(p)?;
    let provenance = core.provenance().list(p, None)?;
    let rx = core.subscribe()?;
    assert!(matches!(
        core.approve_gate(p, gate, human()),
        Err(Error::Storage(_))
    ));
    assert_eq!(core.get_hypothesis(p, h)?.meta.version, 1);
    assert_eq!(core.get_hypothesis(p, h)?.statement, "A improves B");
    assert_eq!(core.get_gate(p, gate)?.status, GateStatus::Pending);
    assert_eq!(core.events().timeline(p)?, before);
    assert_eq!(core.provenance().list(p, None)?, provenance);
    assert!(rx.try_recv().is_err());
    db.execute_batch("DROP TRIGGER reject_event")
        .map_err(|e| Error::Storage(e.to_string()))?;
    core.approve_gate(p, gate, human())?;
    // Receiving a live event must imply that an independent DB reader can see
    // the entity version and approved gate already committed.
    let event = rx.try_recv().map_err(|e| Error::Storage(e.to_string()))?;
    let persisted: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM domain_events WHERE id=?",
            [event.id.to_string()],
            |r| r.get(0),
        )
        .map_err(|e| Error::Storage(e.to_string()))?;
    assert_eq!(persisted, 1);
    let status: String = db
        .query_row(
            "SELECT status FROM gates WHERE id=?",
            [gate.to_string()],
            |r| r.get(0),
        )
        .map_err(|e| Error::Storage(e.to_string()))?;
    assert_eq!(status, "approved");
    Ok(())
}

#[test]
fn migrations_foreign_keys_and_history_integrity() -> Result<()> {
    let temp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let path = temp.path().join("state.sqlite");
    let storage = SqliteStorage::open(&path)?;
    assert_eq!(storage.migration_versions()?, vec![1, 2, 3, 4, 5]);
    drop(storage);
    assert_eq!(
        SqliteStorage::open(&path)?.migration_versions()?,
        vec![1, 2, 3, 4, 5]
    );
    let db = rusqlite::Connection::open(&path).map_err(|e| Error::Storage(e.to_string()))?;
    db.execute_batch("PRAGMA foreign_keys=ON")
        .map_err(|e| Error::Storage(e.to_string()))?;
    assert!(db
        .execute(
            "INSERT INTO paper_sources VALUES(?,?,?)",
            [
                ProjectId::new().to_string(),
                PaperId::new().to_string(),
                SourceId::new().to_string()
            ]
        )
        .is_err());
    let generic: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='entities'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| Error::Storage(e.to_string()))?;
    assert_eq!(generic, 0);
    db.execute("UPDATE schema_migrations SET checksum='tampered'", [])
        .map_err(|e| Error::Storage(e.to_string()))?;
    assert!(SqliteStorage::open(&path).is_err());
    Ok(())
}

#[test]
fn agent_safe_actions_failure_and_permissions() -> Result<()> {
    let core = Core::new(Arc::new(SqliteStorage::open(":memory:")?));
    let p = core.create_project("Agent".into(), "".into(), human())?.id;
    let h = hypothesis(&core, p)?;
    let q = core.add_question(p, "Why?".into(), "".into(), human())?;
    let provider = Arc::new(FakeModelProvider::new(vec![ModelResponse {
        schema_version: 1,
        decision: AgentDecision::Propose {
            action: ProposedAction::CreateRelation(NewRelation {
                from_entity: EntityRef::Hypothesis(h),
                to_entity: EntityRef::Question(q.id),
                kind: RelationKind::RelatedTo,
                confidence: None,
            }),
            reason: "Reference only".into(),
        },
        metadata: json!({}),
    }]));
    let outcome = block_on(
        LeadAgent::new(core.agent_api("lead")?, provider, selector()).run(
            p,
            "Link reference".into(),
            Some(EntityRef::Hypothesis(h)),
            None,
        ),
    )?;
    assert!(matches!(outcome.submission, Some(Submission::Applied(_))));
    assert_eq!(
        core.get_run(p, outcome.run_id)?.status,
        RunStatus::Completed
    );
    assert!(core.list_gates(p, true)?.is_empty());
    let agent_ctx = CommandContext {
        actor: Actor::LeadAgent("lead".into()),
        reason: "attempt bypass".into(),
        run_id: None,
        correlation_id: None,
        causation_id: None,
    };
    assert!(matches!(
        core.execute(
            p,
            ActionEnvelope::new(ProposedAction::ChangeHypothesisStatus {
                hypothesis_id: h,
                expected_version: 1,
                status: HypothesisStatus::Active
            }),
            agent_ctx
        ),
        Err(Error::PermissionDenied)
    ));
    let bad = Arc::new(FakeModelProvider::new(vec![ModelResponse {
        schema_version: 99,
        decision: AgentDecision::NoChange {
            summary: "invalid".into(),
        },
        metadata: json!({}),
    }]));
    assert!(block_on(
        LeadAgent::new(core.agent_api("lead")?, bad, selector()).run(
            p,
            "Invalid provider response".into(),
            None,
            None
        )
    )
    .is_err());
    assert!(core
        .list_runs(p)?
        .iter()
        .any(|r| r.status == RunStatus::Failed));
    Ok(())
}

#[test]
fn idempotency_cross_project_and_version_conflicts() -> Result<()> {
    let core = Core::new(Arc::new(SqliteStorage::open(":memory:")?));
    let p = core.create_project("P1".into(), "".into(), human())?.id;
    let p2 = core.create_project("P2".into(), "".into(), human())?.id;
    let h = hypothesis(&core, p)?;
    let h2 = hypothesis(&core, p2)?;
    let envelope = ActionEnvelope::new(ProposedAction::ReviseHypothesis {
        hypothesis_id: h,
        expected_version: 1,
        statement: "Revised".into(),
    });
    let first = core.execute(p, envelope.clone(), human())?;
    let count = core.get_project_timeline(p)?.len();
    assert_eq!(core.execute(p, envelope.clone(), human())?, first);
    assert_eq!(core.get_project_timeline(p)?.len(), count);
    let mut reused = envelope;
    reused.action = ProposedAction::ReviseHypothesis {
        hypothesis_id: h,
        expected_version: 2,
        statement: "different".into(),
    };
    assert!(matches!(
        core.execute(p, reused, human()),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        core.execute(
            p,
            ActionEnvelope::new(ProposedAction::ReviseHypothesis {
                hypothesis_id: h,
                expected_version: 1,
                statement: "stale".into()
            }),
            human()
        ),
        Err(Error::StaleVersion { .. })
    ));
    assert!(core
        .graph()
        .create(
            p,
            NewRelation {
                from_entity: EntityRef::Hypothesis(h),
                to_entity: EntityRef::Hypothesis(h2),
                kind: RelationKind::RelatedTo,
                confidence: None
            },
            human()
        )
        .is_err());
    let q = core.add_question(p, "Q?".into(), "".into(), human())?;
    let relation = NewRelation {
        from_entity: EntityRef::Hypothesis(h),
        to_entity: EntityRef::Question(q.id),
        kind: RelationKind::Addresses,
        confidence: None,
    };
    let first = core.graph().create(p, relation.clone(), human())?;
    assert_eq!(core.graph().create(p, relation, human())?, first);
    assert_eq!(core.list_relations(p, None)?.len(), 1);
    Ok(())
}

#[test]
fn approved_read_tools_are_workspace_bound() -> Result<()> {
    use anix_core::{ToolContext, ToolInput};
    use anix_integrations::ReadFileTool;
    let temp = tempfile::tempdir().map_err(|e| Error::Tool(e.to_string()))?;
    let workspace = Arc::new(Workspace::open(temp.path())?);
    workspace.write_new("data/note.txt", b"research input")?;
    let project = ProjectId::new();
    let mut tools = ApprovedTools::default();
    tools.register(Arc::new(ReadFileTool::new(project, workspace)))?;
    let context = ToolContext {
        project_id: project,
        run_id: None,
        context_snapshot_id: None,
        tool_invocation_id: None,
    };
    let input = ToolInput {
        schema_version: 1,
        parameters: json!({"path":"data/note.txt"}),
    };
    assert_eq!(
        block_on(tools.execute("read_file", input, context.clone()))?.data,
        "research input"
    );
    for path in ["../secret", ".anix/state.sqlite", "C:/outside"] {
        assert!(block_on(tools.execute(
            "read_file",
            ToolInput {
                schema_version: 1,
                parameters: json!({"path":path})
            },
            context.clone()
        ))
        .is_err());
    }
    assert!(block_on(tools.execute(
        "unregistered",
        ToolInput {
            schema_version: 1,
            parameters: json!({})
        },
        context
    ))
    .is_err());
    Ok(())
}

#[test]
fn retries_hierarchy_gate_decisions_and_archive() -> Result<()> {
    let core = Core::new(Arc::new(SqliteStorage::open(":memory:")?));
    let p = core
        .projects()
        .create("Runtime".into(), "".into(), human())?
        .id;
    let h = hypothesis(&core, p)?;
    let task = core
        .tasks()
        .create(p, "Retry operation".into(), "".into(), human())?;
    let parent = core.runs().start(p, task.id, None, human())?;
    let child_task = core.tasks().create(p, "Child".into(), "".into(), human())?;
    let child = core
        .runs()
        .start(p, child_task.id, Some(parent.id), human())?;
    assert!(matches!(
        core.runs()
            .finish(p, parent.id, RunStatus::Completed, human()),
        Err(Error::Conflict(_))
    ));
    core.runs()
        .finish(p, child.id, RunStatus::Completed, human())?;
    core.runs()
        .finish(p, parent.id, RunStatus::Failed, human())?;
    let retry = core.runs().start(p, task.id, None, human())?;
    assert_ne!(retry.id, parent.id);
    core.runs()
        .finish(p, retry.id, RunStatus::Completed, human())?;
    let count = core.events().timeline(p)?.len();
    core.runs()
        .finish(p, retry.id, RunStatus::Completed, human())?;
    assert_eq!(core.events().timeline(p)?.len(), count);
    for status in [
        GateStatus::Rejected,
        GateStatus::Cancelled,
        GateStatus::Expired,
    ] {
        let action = ActionEnvelope::new(ProposedAction::ReviseHypothesis {
            hypothesis_id: h,
            expected_version: 1,
            statement: "Proposal".into(),
        });
        let gate = match core
            .gates()
            .propose(p, action, human(), "Revision".into())?
        {
            Submission::Gate(g) => g,
            _ => return Err(Error::Validation("gate".into())),
        };
        assert!(core.projects().archive(p, 1, human()).is_err());
        core.resolve_gate(p, gate, status, human())?;
        let count = core.events().timeline(p)?.len();
        core.resolve_gate(p, gate, status, human())?;
        assert_eq!(core.events().timeline(p)?.len(), count);
        assert!(matches!(
            core.approve_gate(p, gate, human()),
            Err(Error::GateAlreadyResolved)
        ));
    }
    assert_eq!(core.get_hypothesis(p, h)?.meta.version, 1);
    core.projects().archive(p, 1, human())?;
    assert_eq!(core.get_project(p)?.status, ProjectStatus::Archived);
    assert!(core
        .questions()
        .add(p, "Archived?".into(), "".into(), human())
        .is_err());
    assert!(!core.events().timeline(p)?.is_empty());
    assert!(serde_json::from_str::<ProjectId>("\"00000000-0000-0000-0000-000000000000\"").is_err());
    Ok(())
}

#[test]
fn lead_agent_can_use_only_registered_read_tools() -> Result<()> {
    use anix_core::ToolInput;
    use anix_integrations::ReadFileTool;
    let temp = tempfile::tempdir().map_err(|e| Error::Tool(e.to_string()))?;
    let workspace = Arc::new(Workspace::open(temp.path())?);
    workspace.write_new("data/measurements.txt", b"A=110, B=100")?;
    let core = Core::new(Arc::new(SqliteStorage::open(":memory:")?));
    let p = core.create_project("Tools".into(), "".into(), human())?.id;
    let mut tools = ApprovedTools::default();
    tools.register(Arc::new(ReadFileTool::new(p, workspace)))?;
    let provider = Arc::new(FakeModelProvider::new(vec![
        ModelResponse {
            schema_version: 1,
            decision: AgentDecision::UseTool {
                name: "read_file".into(),
                input: ToolInput {
                    schema_version: 1,
                    parameters: json!({"path":"data/measurements.txt"}),
                },
            },
            metadata: json!({}),
        },
        ModelResponse {
            schema_version: 1,
            decision: AgentDecision::NoChange {
                summary: "A has a larger measurement".into(),
            },
            metadata: json!({}),
        },
    ]));
    let outcome = block_on(
        LeadAgent::new(core.agent_api("lead")?, provider.clone(), selector())
            .with_tools(tools)
            .run(p, "Inspect measurements".into(), None, None),
    )?;
    assert_eq!(
        outcome.summary.as_deref(),
        Some("A has a larger measurement")
    );
    assert_eq!(
        core.get_run(p, outcome.run_id)?.status,
        RunStatus::Completed
    );
    let requests = provider.requests()?;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].tool_results[0].output.data, "A=110, B=100");
    let forbidden = Arc::new(FakeModelProvider::new(vec![ModelResponse {
        schema_version: 1,
        decision: AgentDecision::UseTool {
            name: "write_database".into(),
            input: ToolInput {
                schema_version: 1,
                parameters: json!({}),
            },
        },
        metadata: json!({}),
    }]));
    assert!(matches!(
        block_on(
            LeadAgent::new(core.agent_api("lead")?, forbidden, selector()).run(
                p,
                "Try unregistered tool".into(),
                None,
                None
            )
        ),
        Err(Error::PermissionDenied)
    ));
    let secret = Arc::new(FakeModelProvider::new(vec![ModelResponse {
        schema_version: 1,
        decision: AgentDecision::NoChange {
            summary: "analysis".into(),
        },
        metadata: json!({"api_key":"sample-secret"}),
    }]));
    assert!(block_on(
        LeadAgent::new(core.agent_api("lead")?, secret, selector()).run(
            p,
            "Check metadata".into(),
            None,
            None
        )
    )
    .is_err());
    assert!(!serde_json::to_string(&core.list_runs(p)?)
        .map_err(|e| Error::Validation(e.to_string()))?
        .contains("sample-secret"));
    assert!(!serde_json::to_string(&core.get_project_timeline(p)?)
        .map_err(|e| Error::Validation(e.to_string()))?
        .contains("sample-secret"));
    Ok(())
}

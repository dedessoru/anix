//! Isolated target-contract diagnostics. Expected red on the unmodified baseline.
use anix_agent::*;
use anix_core::{CommandContext, Core, NewWork, Storage, Tool, ToolContext, ToolDescriptor, ToolFuture, ToolInput, ToolOutput};
use anix_domain::*;
use anix_integrations::*;
use anix_storage::SqliteStorage;
use futures::executor::block_on;
use serde_json::{json, Value};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

fn human() -> CommandContext { CommandContext::human("p0-owner", "isolated V2 P0 diagnostic") }
fn trace(name: &str, value: Value) {
    println!("{}: {}", name, value);
    std::fs::write(format!("reports/p0-v2-2026-10-12/{name}.json"), serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}
fn fixture() -> Result<(tempfile::TempDir, Arc<Core>, ProjectId, BranchId, QuestionId)> {
    let dir = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let core = Arc::new(Core::new(Arc::new(SqliteStorage::open(dir.path().join("state.sqlite"))?)));
    let p = core.create_project("P0 V2".into(), "diagnostic".into(), human())?.id;
    let b = core.create_branch(p, "main".into(), None, human())?.id;
    let q = core.add_question(p, "Supplied question".into(), "Check the requested result".into(), human())?.id;
    Ok((dir, core, p, b, q))
}
fn work(core: &Core, p: ProjectId, b: BranchId, q: QuestionId, granularity: WorkGranularity) -> Result<WorkNode> {
    core.create_work(p, NewWork {branch_id:b, kind:"diagnostic".into(), granularity,
        title:"Work".into(), objective:"Original objective v1".into(),
        acceptance_criteria:OutputContract {required_entities:vec![EntityRef::Question(q)], required_artifacts:vec![], minimum_children:0},
        capability_requirements:vec!["tool:compute".into()], context_policy_id:"bounded-v1".into(), budget:WorkBudget::default()}, human())
}
fn selector() -> Arc<FixedModelSelector> { Arc::new(FixedModelSelector {selection:ModelSelection {provider:"p0-local".into(),model:"scripted".into()}}) }
fn start(core: &Core, p: ProjectId, w: WorkNodeId) -> Result<RunId> {
    let api=core.agent_api("p0-lead")?;
    api.start_work(p,w)?;
    let task=api.create_task(p,"diagnostic".into())?;
    let run=api.start_run(p,task.id,None)?;
    api.bind_run(p,run.id,w)?;
    Ok(run.id)
}
struct Compute;
impl Tool for Compute {
    fn descriptor(&self)->ToolDescriptor {ToolDescriptor {name:"compute".into(),description:"Deterministic fixture".into(),read_only:true}}
    fn execute(&self,_:ToolInput,_:ToolContext)->ToolFuture<'_> {Box::pin(async {Ok(ToolOutput {schema_version:1,data:json!({"value":2}),metadata:json!({})})})}
}

#[test]
fn f02_scientific_record_is_reachable_from_composite_step() -> Result<()> {
    let (_dir,core,p,b,q)=fixture()?;
    let w=work(&core,p,b,q,WorkGranularity::Composite)?;
    let action=ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {question_id:Some(q),title:"Observed hypothesis".into(),statement:"An attributable scientific statement".into()}));
    let provider=Arc::new(FakeModelProvider::new(vec![ModelResponse {schema_version:1,decision:AgentDecision::Propose {action:action.action.clone(),reason:"Record a scientific interpretation".into()},metadata:json!({})}]));
    let lead=LeadAgent::new(core.agent_api("p0-lead")?,provider.clone(),selector());
    let result=block_on(lead.run_step(p,"Record the hypothesis".into(),Some(EntityRef::Question(q)),None,w.id));
    assert_eq!(provider.requests()?.len(),1,"fixture reached provider");
    let err=result.as_ref().err().map(ToString::to_string);
    // Positive control: same trusted identity/action through Core is applicable.
    let run= start(&core,p,w.id)?;
    let direct=core.agent_api("p0-lead")?.submit(p,run,action,"same action positive control".into());
    assert!(direct.is_ok(),"direct action fixture invalid: {direct:?}");
    if let Ok(anix_core::Submission::Gate(gate)) = &direct {core.approve_gate(p,*gate,human())?;}
    assert_eq!(core.runtime_state(p)?.hypotheses.len(),1,"same action is scientifically writable via Core");
    trace("F02",json!({"expected":"scientific Record reaches Core from run_step","actual_error":err,"provider_calls":1,"direct_core_submission":format!("{direct:?}")}));
    assert!(result.is_ok(),"F02: scientific action rejected before Core: {err:?}");
    Ok(())
}

#[test]
fn f04_old_tool_output_cannot_satisfy_revised_objective() -> Result<()> {
    let (dir,core,p,b,q)=fixture()?;
    let w=work(&core,p,b,q,WorkGranularity::Atomic)?;
    let run=start(&core,p,w.id)?;
    let tool=RecordedTool {core:core.clone(),inner:Arc::new(Compute),offloader:ArtifactOffloader {workspace:Arc::new(Workspace::open(dir.path())?),threshold_bytes:8192},version:"p0-1".into()};
    let output=block_on(tool.execute(ToolInput {schema_version:1,parameters:json!({"input_version":1})},ToolContext {project_id:p,run_id:Some(run),context_snapshot_id:None,tool_invocation_id:None}))?;
    let api=core.agent_api("p0-lead")?;
    assert!(api.required_tool_output_exists(p,w.id)?);
    assert!(api.validate_work_completion(p,run,w.id).is_ok());
    let old=core.runtime_state(p)?.work_nodes[&w.id].clone();
    let revised=core.execute(p,ActionEnvelope::new(ProposedAction::ReviseWorkObjective {work_node_id:w.id,expected_version:old.meta.version,objective:"Compute a DIFFERENT quantity for new input v2".into()}),human())?;
    let exists=api.required_tool_output_exists(p,w.id)?;
    let validation=api.validate_work_completion(p,run,w.id);
    let state=core.runtime_state(p)?;
    let reopened=Core::new(Arc::new(SqliteStorage::open(dir.path().join("state.sqlite"))?));
    assert_eq!(state,reopened.runtime_state(p)?);
    trace("F04",json!({"old_objective":old.objective,"new_objective":state.work_nodes[&w.id].objective,"revision_result":revised,"original_output":output,"old_output_still_satisfies":exists,"completion_validation_ok":validation.is_ok(),"restart_equal":true,"invocations":state.tool_invocations}));
    assert!(!exists && validation.is_err(),"F04: output v1 satisfies revised objective v2");
    Ok(())
}

#[test]
fn f05_incoming_blocks_is_shared_by_readiness_and_finish() -> Result<()> {
    let (dir,core,p,b,q)=fixture()?;
    let root=work(&core,p,b,q,WorkGranularity::Composite)?;
    let run=start(&core,p,root.id)?;
    let blocker=work(&core,p,b,q,WorkGranularity::Atomic)?;
    core.add_work_edge(p,blocker.id,root.id,WorkEdgeKind::Blocks,human())?;
    let api=core.agent_api("p0-lead")?;
    let (_,context)=api.compile_context(p,run,root.id,vec![EntityRef::Question(q)])?;
    let view:Value=serde_json::from_str(context.process_notes.iter().find_map(|s|s.strip_prefix("Decision state: ")).expect("decision state fixture")).unwrap();
    assert_eq!(view["completion"]["ready"],true,"baseline CE must expose the false ready state");
    let v=core.runtime_state(p)?.work_nodes[&root.id].meta.version;
    let direct=core.transition_work(p,root.id,v,WorkStatus::Succeeded,human());
    assert!(direct.as_ref().err().is_some_and(|e|e.to_string().contains("unfinished dependency")),"fixture must reach incoming dependency guard: {direct:?}");
    let finished=api.finish_research(p,run,root.id,ResearchOutcome::Inconclusive,"Bounded uncertainty diagnostic".into());
    let state=core.runtime_state(p)?;
    assert_eq!(state,Core::new(Arc::new(SqliteStorage::open(dir.path().join("state.sqlite"))?)).runtime_state(p)?);
    trace("F05",json!({"expected":"incoming Blocks visible and scientific finish rejected","view":view,"direct_error":direct.err().map(|e|e.to_string()),"finish_accepted":finished.is_ok(),"root_status":state.work_nodes[&root.id].status,"blocker_status":state.work_nodes[&blocker.id].status,"restart_equal":true}));
    assert!(view["completion"]["ready"]==false && finished.is_err(),"F05: readiness and finish bypass incoming Blocks");
    Ok(())
}

#[test]
fn f08_snapshot_renderer_never_reads_current_entity_version() -> Result<()> {
    let (dir,core,p,b,q)=fixture()?;
    let h=core.execute(p,ActionEnvelope::new(ProposedAction::CreateHypothesis(NewHypothesis {question_id:Some(q),title:"Hypothesis".into(),statement:"SNAPSHOT_V1".into()})),human())?;
    let ActionResult::Entity(EntityRef::Hypothesis(id))=h else {panic!("hypothesis fixture")};
    let w=work(&core,p,b,q,WorkGranularity::Atomic)?;
    let run=start(&core,p,w.id)?;
    let api=core.agent_api("p0-lead")?;
    let (snapshot,frozen)=api.compile_context(p,run,w.id,vec![EntityRef::Hypothesis(id)])?;
    let initial=ContextBuilder::build_for_snapshot(&api,p,Some(EntityRef::Hypothesis(id)),&frozen)?;
    assert!(initial.hypotheses.iter().any(|v|v.id==id && v.statement=="SNAPSHOT_V1"));
    let version=core.runtime_state(p)?.hypotheses[&id].meta.version;
    core.hypotheses().revise(p,id,version,"LIVE_V2".into(),human())?;
    let rendered=ContextBuilder::build_for_snapshot(&api,p,Some(EntityRef::Hypothesis(id)),&frozen)?;
    let replay=core.provider_context(p,snapshot.id)?;
    assert_eq!(serde_json::to_value(&frozen).unwrap(),serde_json::to_value(&replay).unwrap(),"stored snapshot itself must stay frozen");
    let reopened=Core::new(Arc::new(SqliteStorage::open(dir.path().join("state.sqlite"))?));
    assert_eq!(serde_json::to_value(&replay).unwrap(),serde_json::to_value(reopened.provider_context(p,snapshot.id)?).unwrap());
    trace("F08",json!({"snapshot":snapshot,"frozen":frozen,"rendered_before":initial,"rendered_after":rendered,"snapshot_restart_equal":true}));
    assert_eq!(initial.hypotheses[0].statement,rendered.hypotheses[0].statement,"F08: frozen snapshot paired with live v2 typed context");
    Ok(())
}

struct FailStorage {inner:SqliteStorage, countdown:AtomicUsize}
impl Storage for FailStorage {
    fn read(&self,p:ProjectId)->Result<ProjectState>{self.inner.read(p)}
    fn list_projects(&self)->Result<Vec<Project>>{self.inner.list_projects()}
    fn transaction(&self,p:ProjectId,op:&mut dyn FnMut(&mut ProjectState)->Result<()>)->Result<Vec<DomainEvent>>{
        let n=self.countdown.load(Ordering::SeqCst);
        if n>0 && self.countdown.fetch_sub(1,Ordering::SeqCst)==1 {return Err(Error::Storage("P0 injected failure BEFORE second commit".into()));}
        self.inner.transaction(p,op)
    }
}
#[test]
fn f11_add_child_failure_never_leaves_orphan_or_duplicate() -> Result<()> {
    let dir=tempfile::tempdir().map_err(|e|Error::Storage(e.to_string()))?;
    let path=dir.path().join("state.sqlite");
    let storage=Arc::new(FailStorage {inner:SqliteStorage::open(&path)?,countdown:AtomicUsize::new(0)});
    let core=Core::new(storage.clone());
    let p=core.create_project("Fault fixture".into(),"P0".into(),human())?.id;
    let b=core.create_branch(p,"main".into(),None,human())?.id;
    let q=core.add_question(p,"Question".into(),"Required".into(),human())?.id;
    let root=work(&core,p,b,q,WorkGranularity::Composite)?;
    let run=start(&core,p,root.id)?;
    let api=core.agent_api("p0-lead")?;
    let before=core.runtime_state(p)?;
    storage.countdown.store(2,Ordering::SeqCst);
    let failed=api.create_child_work(p,run,root.id,"Child".into(),"Compute required quantity".into(),"compute".into());
    assert!(failed.as_ref().err().is_some_and(|e|e.to_string().contains("P0 injected")),"fixture must hit injected storage failure: {failed:?}");
    let partial=core.runtime_state(p)?;
    let restored=Core::new(Arc::new(SqliteStorage::open(&path)?));
    assert_eq!(partial,restored.runtime_state(p)?);
    let retried=restored.agent_api("p0-lead")?.create_child_work(p,run,root.id,"Child".into(),"Compute required quantity".into(),"compute".into())?;
    let after=restored.runtime_state(p)?;
    let children:Vec<_>=after.work_nodes.values().filter(|w|w.id!=root.id).map(|w|w.id).collect();
    let orphan:Vec<_>=partial.work_nodes.values().filter(|w|w.id!=root.id && !partial.work_edges.values().any(|e|e.kind==WorkEdgeKind::Contains && e.to==w.id)).map(|w|w.id).collect();
    trace("F11",json!({"before_nodes":before.work_nodes.len(),"failure":failed.err().map(|e|e.to_string()),"partial_nodes":partial.work_nodes,"partial_edges":partial.work_edges,"orphans":orphan,"retry_child":retried.id,"children_after_retry":children,"restart_equal":true}));
    assert!(orphan.is_empty() && children.len()==1,"F11: durable orphan plus duplicate after retry");
    Ok(())
}

#[test]
fn f14_one_call_cap_bounds_actual_provider_attempts() -> Result<()> {
    use std::{io::{Read,Write},net::TcpListener};
    let (_dir,core,p,b,q)=fixture()?;
    let root=work(&core,p,b,q,WorkGranularity::Composite)?;
    let control=core.get_human_control(p)?;
    core.set_budget(p,ResearchBudget {max_strong_model_calls:Some(1),..Default::default()},Some(control.version),human())?;
    let listener=TcpListener::bind("127.0.0.1:0").map_err(|e|Error::Storage(e.to_string()))?;
    listener.set_nonblocking(true).unwrap();
    let addr=listener.local_addr().unwrap();
    let server=std::thread::spawn(move || {
        let mut requests=Vec::new();
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
        while requests.len()<2 && std::time::Instant::now()<deadline {
            let Ok((mut stream,_))=listener.accept() else {std::thread::sleep(std::time::Duration::from_millis(10));continue};
            stream.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
            let mut bytes=Vec::new(); let mut buf=[0;4096];
            loop {
                let n=stream.read(&mut buf).unwrap(); if n==0 {break}; bytes.extend_from_slice(&buf[..n]);
                if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {
                    let headers=String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let size=headers.lines().find_map(|s|s.strip_prefix("content-length:").and_then(|v|v.trim().parse::<usize>().ok())).unwrap_or(0);
                    if bytes.len()>=end+4+size {break;}
                }
            }
            let end=bytes.windows(4).position(|w|w==b"\r\n\r\n").unwrap();
            requests.push(serde_json::from_slice::<Value>(&bytes[end+4..]).unwrap());
            let raw=if requests.len()==1 {"not json"} else {r#"{"action":{"type":"finish_research","outcome":"blocked","summary":"Diagnostic only"},"short_reason":"diagnostic"}"#};
            let body=json!({"choices":[{"message":{"content":raw},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":12}}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
        requests
    });
    let mut config=TestLeadConfig::local_default();
    config.endpoint=format!("http://{addr}/v1"); config.timeout_seconds=3;
    config.capabilities=["structured_agent_decisions".into()].into();
    config.diagnostic_traces=true;
    let provider=Arc::new(OpenAiCompatibleModelProvider::new(config)?);
    let lead=LeadAgent::new(core.agent_api("p0-lead")?,provider.clone(),selector());
    let result=block_on(lead.run_step(p,"Bound provider attempts".into(),Some(EntityRef::Question(q)),None,root.id));
    let requests=server.join().unwrap();
    let state=core.runtime_state(p)?;
    assert_eq!(state.model_invocations.len(),1,"one logical invocation fixture");
    assert!(!requests.is_empty(),"must reach local HTTP transport");
    trace("F14",json!({"cap":1,"http_attempts":requests.len(),"requests":requests,"logical_invocations":state.model_invocations,"step_result":format!("{result:?}"),"http_metrics":provider.http_metrics()}));
    assert!(requests.len()<=1,"F14: one invocation reservation allows multiple HTTP sends");
    Ok(())
}

#[test]
fn p0_positive_backup_hydration_and_restore() -> Result<()> {
    let backup=SqliteStorage::open("reports/p0-v2-2026-10-12/private/baseline-backup/state.sqlite")?;
    let restore=SqliteStorage::open("reports/p0-v2-2026-10-12/private/baseline-restore/state.sqlite")?;
    assert_eq!(backup.migration_versions()?,vec![1,2,3,4,5]);
    assert_eq!(restore.migration_versions()?,vec![1,2,3,4,5]);
    let projects=backup.list_projects()?;
    assert_eq!(projects,restore.list_projects()?);
    let mut artifacts=0;
    let mut works=0;
    let mut invocations=0;
    for p in &projects {
        let a=backup.read(p.id)?;
        let b=restore.read(p.id)?;
        assert_eq!(a,b,"restored aggregate differs for {}",p.id);
        artifacts+=a.artifacts.len();works+=a.work_nodes.len();
        invocations+=a.model_invocations.len()+a.tool_invocations.len();
    }
    trace("backup-rust-hydration",json!({"projects":projects.len(),"artifacts":artifacts,"work_nodes":works,"invocations":invocations,"every_aggregate_equal":true,"migration_versions":[1,2,3,4,5],"scientific_reassessment":"NOT RUN; historical records unchanged"}));
    Ok(())
}

#[test]
fn p0_positive_unblocked_closure_and_atomic_child_control() -> Result<()> {
    let (dir,core,p,b,q)=fixture()?;
    let root=work(&core,p,b,q,WorkGranularity::Composite)?;
    let run=start(&core,p,root.id)?;
    let api=core.agent_api("p0-lead")?;
    let child=api.create_child_work(p,run,root.id,"Child".into(),"Compute once".into(),"compute".into())?;
    let duplicate=api.create_child_work(p,run,root.id,"Renamed".into(),"Compute once".into(),"compute".into())?;
    assert_eq!(child.id,duplicate.id);
    let state=core.runtime_state(p)?;
    assert_eq!(state.work_nodes.len(),2);
    assert_eq!(state.work_edges.values().filter(|e|e.from==root.id && e.to==child.id && e.kind==WorkEdgeKind::Contains).count(),1);
    assert_eq!(child.status,WorkStatus::Ready);
    let superseded=core.transition_work(p,child.id,child.meta.version,WorkStatus::Superseded,human())?;
    let result=api.finish_research(p,run,root.id,ResearchOutcome::Inconclusive,"Explicit bounded uncertainty control".into())?;
    let state=core.runtime_state(p)?;
    assert_eq!(state,Core::new(Arc::new(SqliteStorage::open(dir.path().join("state.sqlite"))?)).runtime_state(p)?);
    trace("positive-controls",json!({"dedup_child":child.id,"contains_edges":1,"child_ready_before_retire":true,"retired_status":superseded.status,"unblocked_finish":result.outcome,"restart_equal":true,"limit":"existing legacy behavior; not G2 scientific acceptance"}));
    Ok(())
}

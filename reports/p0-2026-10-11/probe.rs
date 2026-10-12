//! External P0 diagnostic; compiled separately, never added to the application.
use anix_core::{CommandContext, Core, NewSource, NewWork};
use anix_domain::*;
use anix_storage::SqliteStorage;
use serde_json::json;
use std::sync::Arc;

fn main() -> Result<()> {
    let tmp = tempfile::tempdir().map_err(|e| Error::Storage(e.to_string()))?;
    let path = tmp.path().join("probe.sqlite");
    let core = Core::new(Arc::new(SqliteStorage::open(&path)?));
    let human = || CommandContext::human("probe-owner", "isolated diagnostic fixture");
    let p = core.create_project("Blocked admission probe".into(), "P0 negative contract case".into(), human())?;
    let source = core.add_source(p.id, NewSource { title: "Supplied source".into(), source_kind: SourceKind::Manual, locator: "fixture://p0".into(), checksum: None, metadata: json!({}) }, human())?;
    let b = core.create_branch(p.id, "main".into(), None, human())?;
    let root = core.create_work(p.id, NewWork { branch_id: b.id, kind: "probe".into(), granularity: WorkGranularity::Composite, title: "Research".into(), objective: "Read supplied source".into(), acceptance_criteria: OutputContract { required_entities: vec![EntityRef::Source(source.id)], required_artifacts: vec![], minimum_children: 1 }, capability_requirements: vec![], context_policy_id: "bounded-v1".into(), budget: WorkBudget::default() }, human())?;
    core.transition_work(p.id, root.id, root.meta.version, WorkStatus::Ready, human())?;
    let api = core.agent_api("diagnostic-lead")?;
    let task = api.create_task(p.id, "Research".into())?;
    let run = api.start_run(p.id, task.id, None)?;
    api.bind_run(p.id, run.id, root.id)?;
    api.start_work(p.id, root.id)?;
    let child = api.create_child_work(p.id, run.id, root.id, "Read supplied source".into(), "Inspect supplied evidence".into(), "read_source".into())?;
    let children = core.executable_child_work(p.id, root.id)?;
    let handles = api.work_source_handles(p.id, child.id)?;
    let before = api.lead_progress_snapshot(p.id, root.id)?;
    let blocked = api.finish_research(p.id, run.id, root.id, ResearchOutcome::Blocked, "Diagnostic: pretending supplied evidence is unavailable".into());
    let after = core.runtime_state(p.id)?;
    drop(api);
    drop(core);
    let reopened = Core::new(Arc::new(SqliteStorage::open(&path)?));
    let restored = reopened.runtime_state(p.id)?;
    println!("{}", json!({"experiment":"P0-I13-I23-false-blocked", "project_id":p.id,"root_id":root.id,"child_id":child.id,"source_id":source.id,"run_id":run.id,"executable_children_before":children.iter().map(|n| n.id).collect::<Vec<_>>(),"supplied_handles_before":handles.iter().map(|(h,_)| &h.handle).collect::<Vec<_>>(),"progress_before":before,"expected":"reject Blocked while registered required continuation is executable", "actual":"Core accepted Blocked", "accepted":blocked.is_ok(),"error":blocked.err().map(|e|e.to_string()),"root_status_after":after.work_nodes[&root.id].status,"root_outcome_after":after.work_nodes[&root.id].outcome,"child_status_after":after.work_nodes[&child.id].status,"restart_equal":after==restored,"oracle_limit":"structural registered continuation; no claim of scientific sufficiency or transport availability"}));
    Ok(())
}

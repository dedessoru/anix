use anix_core::Storage;
use anix_domain::*;
use rusqlite::{
    params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension,
    TransactionBehavior,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Mutex, time::Duration};

fn storage(error: impl std::fmt::Display) -> Error {
    Error::Storage(error.to_string())
}
pub struct SqliteStorage {
    db: Mutex<Connection>,
}
impl SqliteStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut db = Connection::open(path).map_err(storage)?;
        db.busy_timeout(Duration::from_secs(5)).map_err(storage)?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )
        .map_err(storage)?;
        migrate(&mut db)?;
        Ok(Self { db: Mutex::new(db) })
    }
    pub fn migration_versions(&self) -> Result<Vec<u32>> {
        let db = self.db.lock().map_err(storage)?;
        let mut stmt = db
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .map_err(storage)?;
        let rows = stmt.query_map([], |r| r.get(0)).map_err(storage)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(storage)
    }
}
fn migrate(db: &mut Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL,checksum TEXT NOT NULL,applied_at TEXT NOT NULL)").map_err(storage)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let migrations = [
        (
            1u32,
            "initial",
            include_str!("../../../migrations/0001_initial.sql"),
        ),
        (
            2u32,
            "runtime",
            include_str!("../../../migrations/0002_runtime.sql"),
        ),
        (
            3u32,
            "context_retrieval_telemetry",
            include_str!("../../../migrations/0003_context_retrieval_telemetry.sql"),
        ),
        (
            4u32,
            "human_control",
            include_str!("../../../migrations/0004_human_control.sql"),
        ),
        (
            5u32,
            "work_outcome",
            include_str!("../../../migrations/0005_work_outcome.sql"),
        ),
    ];
    let newest: u32 = tx
        .query_row(
            "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if newest > migrations.len() as u32 {
        return Err(Error::Storage(
            "database schema is newer than this binary".into(),
        ));
    }
    for (version, name, sql) in migrations {
        let checksum = format!("{:x}", Sha256::digest(sql.as_bytes()));
        let existing = tx.query_row(
            "SELECT checksum FROM schema_migrations WHERE version=?",
            [version],
            |r| r.get::<_, String>(0),
        );
        match existing {
            Ok(old) if old == checksum => {}
            Ok(_) => return Err(Error::Storage("applied migration checksum changed".into())),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                tx.execute_batch(sql).map_err(storage)?;
                tx.execute("INSERT INTO schema_migrations VALUES(?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![version,name,checksum]).map_err(storage)?;
            }
            Err(e) => return Err(storage(e)),
        }
    }
    tx.commit().map_err(storage)
}

// Explicit column layouts ensure structural fields remain queryable SQL data.
// The serializer is shared infrastructure, not a generic research-entity table.
struct Table {
    name: &'static str,
    fields: &'static str,
    json: &'static [&'static str],
}
const COMMON: &str = "id,project_id,created_at,updated_at,version,archived";
macro_rules! table {($name:ident,$sql:literal,$fields:literal,[$($json:literal),*])=>{const $name:Table=Table{name:$sql,fields:$fields,json:&[$($json),*]};}}
table!(PROJECTS, "projects", "name,description,status", []);
table!(
    QUESTIONS,
    "research_questions",
    "title,description,status",
    []
);
table!(
    SOURCES,
    "sources",
    "title,source_kind,locator,checksum,metadata",
    ["metadata"]
);
table!(
    PAPERS,
    "papers",
    "title,publication_year,doi,abstract_text,metadata",
    ["metadata"]
);
table!(
    CLAIMS,
    "claims",
    "paper_id,source_id,text,normalized_text,location_page,location_section,location_paragraph,location_text_span,location_offset,location_table,location_figure,created_by",
    ["created_by"]
);
table!(
    HYPOTHESES,
    "hypotheses",
    "question_id,title,statement,status,confidence",
    []
);
table!(
    EXPERIMENTS,
    "experiments",
    "hypothesis_id,title,objective,methodology,protocol,status",
    []
);
table!(
    EXPERIMENT_RUNS,
    "experiment_runs",
    "experiment_id,run_id,status,started_at,finished_at,metadata",
    ["metadata"]
);
table!(
    FINDINGS,
    "findings",
    "experiment_run_id,title,summary,confidence,created_by",
    ["created_by"]
);
table!(
    ARTIFACTS,
    "artifacts",
    "name,artifact_kind,path,content_hash,byte_size,metadata",
    ["metadata"]
);
table!(
    RELATIONS,
    "relations",
    "from_id,from_kind,to_id,to_kind,kind,confidence,created_by",
    ["created_by"]
);
table!(
    TASKS,
    "tasks",
    "title,description,status,created_by",
    ["created_by"]
);
table!(
    RUNS,
    "runs",
    "task_id,parent_run_id,status,started_at,finished_at,error,provider_metadata",
    ["provider_metadata"]
);
table!(GATES,"gates","status,proposed_action,affected_entities,expected_versions,reason,risk,created_by,run_id,resolved_at,resolved_by,result",["proposed_action","affected_entities","expected_versions","created_by","resolved_by","result"]);
const EVENTS:Table=Table{name:"domain_events",fields:"id,project_id,event_type,schema_version,actor,run_id,correlation_id,causation_id,occurred_at,payload",json:&["actor","payload"]};
const PROVENANCE:Table=Table{name:"provenance_records",fields:"id,project_id,actor,action,entity,run_id,gate_id,reason,source_entities,event_id,created_at",json:&["actor","entity","source_entities"]};
const RECEIPTS: Table = Table {
    name: "action_receipts",
    fields: "id,project_id,action,result",
    json: &["action", "result"],
};

table!(WORK_NODES,"work_nodes","branch_id,kind,granularity,title,objective,acceptance_criteria,status,owner,capability_requirements,context_policy_id,budget,created_by,outcome",["acceptance_criteria","owner","capability_requirements","budget","created_by"]);
table!(WORK_EDGES, "work_edges", "branch_id,from,to,kind", []);
table!(
    PLAN_REVISIONS,
    "plan_revisions",
    "branch_id,parent_revision_id,nodes,edges,reason,created_by",
    ["nodes", "edges", "created_by"]
);
table!(
    BRANCHES,
    "branches",
    "name,parent_branch_id,base_checkpoint_id,head_revision_id",
    []
);
table!(
    CHECKPOINTS,
    "checkpoints",
    "run_id,work_node_id,branch_id,plan_revision_id,context_snapshot_id,boundary,execution_state",
    ["execution_state"]
);
table!(CONTEXT_SNAPSHOTS,"context_snapshots","work_node_id,plan_revision_id,entries,process_memory_ids,retrieval_query,retrieval_candidate_count,retrieval_selected,instruction_version,skill_versions,budget_tokens,estimated_tokens,estimation_method",["entries","process_memory_ids","retrieval_selected","skill_versions"]);
table!(
    PROCESS_MEMORIES,
    "process_memories",
    "namespace,work_node_id,content,created_by",
    ["created_by"]
);
table!(
    EXECUTION_BINDINGS,
    "execution_bindings",
    "work_node_id,branch_id,plan_revision_id",
    []
);
table!(MODEL_INVOCATIONS,"model_invocations","run_id,work_node_id,context_snapshot_id,provider,model,model_revision,runtime,instruction_version,output_schema,started_at,finished_at,latency_ms,status,usage,metadata",["output_schema","usage","metadata"]);
table!(TOOL_INVOCATIONS,"tool_invocations","run_id,work_node_id,tool,tool_version,input_artifacts,output_artifacts,started_at,finished_at,status,error_code",["input_artifacts","output_artifacts"]);
table!(
    CANDIDATE_CLAIMS,
    "candidate_claims",
    "paper_id,source_id,text,source_quote,location,promoted_claim_id",
    ["location"]
);

fn columns(table: &Table, common: bool) -> Vec<&str> {
    let mut cols = if common {
        COMMON.split(',').collect::<Vec<_>>()
    } else {
        vec![]
    };
    cols.extend(table.fields.split(','));
    cols
}
fn load<T: DeserializeOwned>(
    db: &Connection,
    table: &Table,
    project: ProjectId,
    common: bool,
) -> Result<Vec<T>> {
    let cols = columns(table, common);
    let order = if table.name == "domain_events" {
        "sequence"
    } else {
        "rowid"
    };
    let sql = format!(
        "SELECT {} FROM {} WHERE project_id=? ORDER BY {order}",
        cols.iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(","),
        table.name
    );
    let mut statement = db.prepare(&sql).map_err(storage)?;
    let mut rows = statement.query([project.to_string()]).map_err(storage)?;
    let mut result = vec![];
    while let Some(row) = rows.next().map_err(storage)? {
        let mut object = Map::new();
        for (index, field) in cols.iter().enumerate() {
            let value: SqlValue = row.get(index).map_err(storage)?;
            let value = match value {
                SqlValue::Null => Value::Null,
                SqlValue::Integer(v) if *field == "archived" => Value::Bool(v != 0),
                SqlValue::Integer(v) => Value::from(v),
                SqlValue::Real(v) => Value::from(v),
                SqlValue::Text(v) if table.json.contains(field) => {
                    serde_json::from_str(&v).map_err(storage)?
                }
                SqlValue::Text(v) => Value::String(v),
                SqlValue::Blob(_) => return Err(Error::Storage("unexpected binary column".into())),
            };
            object.insert((*field).into(), value);
        }
        hydrate(db, table, &mut object)?;
        let item: T = serde_json::from_value(Value::Object(object)).map_err(storage)?;
        result.push(item);
    }
    Ok(result)
}
fn sql_value(value: &Value, json: bool) -> Result<SqlValue> {
    if value.is_null() {
        return Ok(SqlValue::Null);
    }
    if json {
        return Ok(SqlValue::Text(value.to_string()));
    }
    match value {
        Value::String(v) => Ok(SqlValue::Text(v.clone())),
        Value::Bool(v) => Ok(SqlValue::Integer(i64::from(*v))),
        Value::Number(v) => {
            if let Some(v) = v.as_i64() {
                Ok(SqlValue::Integer(v))
            } else if v.as_u64().is_some() {
                Err(Error::Storage("integer exceeds SQLite range".into()))
            } else if let Some(v) = v.as_f64() {
                Ok(SqlValue::Real(v))
            } else {
                Err(Error::Storage("numeric overflow".into()))
            }
        }
        _ => Err(Error::Storage("unexpected structured SQL field".into())),
    }
}
fn save<T: Serialize>(
    db: &Connection,
    table: &Table,
    item: &T,
    common: bool,
    insert_only: bool,
) -> Result<()> {
    let mut object = serde_json::to_value(item)
        .map_err(storage)?
        .as_object()
        .cloned()
        .ok_or_else(|| Error::Storage("non-object entity".into()))?;
    let cols = columns(table, common);
    if table.name == "claims" {
        let location = object
            .get("location")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| Error::Storage("claim location".into()))?;
        for (name, value) in location {
            object.insert(format!("location_{name}"), value);
        }
    }
    if table.name == "relations" {
        for prefix in ["from", "to"] {
            let reference: EntityRef =
                serde_json::from_value(object[&format!("{prefix}_entity")].clone())
                    .map_err(storage)?;
            object.insert(format!("{prefix}_id"), Value::String(reference.id()));
            object.insert(
                format!("{prefix}_kind"),
                Value::String(reference.kind().into()),
            );
        }
    }
    let values = cols
        .iter()
        .map(|field| {
            let value = object
                .get(*field)
                .ok_or_else(|| Error::Storage(format!("missing column {field}")))?;
            sql_value(value, table.json.contains(field))
        })
        .collect::<Result<Vec<_>>>()?;
    let bindings = vec!["?"; cols.len()].join(",");
    let update = cols
        .iter()
        .filter(|c| **c != "id")
        .map(|c| format!("\"{c}\"=excluded.\"{c}\""))
        .collect::<Vec<_>>()
        .join(",");
    let suffix = if insert_only {
        "".into()
    } else {
        format!(" ON CONFLICT(id) DO UPDATE SET {update}")
    };
    let sql = format!(
        "INSERT INTO {}({}) VALUES({bindings}){suffix}",
        table.name,
        cols.iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    db.execute(&sql, params_from_iter(values))
        .map_err(storage)?;
    if table.name == "papers" {
        let paper = object["id"]
            .as_str()
            .ok_or_else(|| Error::Storage("paper ID".into()))?;
        db.execute("DELETE FROM paper_authors WHERE paper_id=?", [paper])
            .map_err(storage)?;
        for (position, name) in object["authors"]
            .as_array()
            .ok_or_else(|| Error::Storage("paper authors".into()))?
            .iter()
            .enumerate()
        {
            db.execute(
                "INSERT INTO paper_authors VALUES(?,?,?)",
                params![
                    paper,
                    position as i64,
                    name.as_str()
                        .ok_or_else(|| Error::Storage("author name".into()))?
                ],
            )
            .map_err(storage)?;
        }
    }
    if table.name == "experiment_runs" {
        let id = object["id"]
            .as_str()
            .ok_or_else(|| Error::Storage("experiment run ID".into()))?;
        let project = object["project_id"]
            .as_str()
            .ok_or_else(|| Error::Storage("project ID".into()))?;
        db.execute(
            "DELETE FROM experiment_run_artifacts WHERE experiment_run_id=?",
            [id],
        )
        .map_err(storage)?;
        for direction in ["input", "output"] {
            for (position, artifact) in object[&format!("{direction}_artifacts")]
                .as_array()
                .ok_or_else(|| Error::Storage("artifact list".into()))?
                .iter()
                .enumerate()
            {
                db.execute(
                    "INSERT INTO experiment_run_artifacts VALUES(?,?,?,?,?)",
                    params![
                        id,
                        project,
                        artifact
                            .as_str()
                            .ok_or_else(|| Error::Storage("artifact ID".into()))?,
                        direction,
                        position as i64
                    ],
                )
                .map_err(storage)?;
            }
        }
    }
    Ok(())
}

fn hydrate(db: &Connection, table: &Table, object: &mut Map<String, Value>) -> Result<()> {
    if table.name == "claims" {
        let mut location = Map::new();
        for field in [
            "page",
            "section",
            "paragraph",
            "text_span",
            "offset",
            "table",
            "figure",
        ] {
            location.insert(
                field.into(),
                object
                    .remove(&format!("location_{field}"))
                    .unwrap_or(Value::Null),
            );
        }
        object.insert("location".into(), Value::Object(location));
    }
    if table.name == "relations" {
        for prefix in ["from", "to"] {
            object.insert(format!("{prefix}_entity"),serde_json::json!({"type":object[&format!("{prefix}_kind")],"id":object[&format!("{prefix}_id")]}));
        }
    }
    if table.name == "papers" {
        let mut stmt = db
            .prepare("SELECT name FROM paper_authors WHERE paper_id=? ORDER BY position")
            .map_err(storage)?;
        let rows = stmt
            .query_map(
                [object["id"]
                    .as_str()
                    .ok_or_else(|| Error::Storage("paper ID".into()))?],
                |r| r.get::<_, String>(0),
            )
            .map_err(storage)?;
        let authors = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(storage)?;
        object.insert("authors".into(), serde_json::json!(authors));
    }
    if table.name == "experiment_runs" {
        for direction in ["input", "output"] {
            let mut stmt=db.prepare("SELECT artifact_id FROM experiment_run_artifacts WHERE experiment_run_id=? AND direction=? ORDER BY position").map_err(storage)?;
            let rows = stmt
                .query_map(
                    params![
                        object["id"]
                            .as_str()
                            .ok_or_else(|| Error::Storage("experiment run ID".into()))?,
                        direction
                    ],
                    |r| r.get::<_, String>(0),
                )
                .map_err(storage)?;
            let artifacts = rows
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(storage)?;
            object.insert(
                format!("{direction}_artifacts"),
                serde_json::json!(artifacts),
            );
        }
    }
    Ok(())
}

fn read_state(db: &Connection, project: ProjectId) -> Result<ProjectState> {
    let mut s = ProjectState {
        project: load::<Project>(db, &PROJECTS, project, true)?
            .into_iter()
            .next(),
        ..Default::default()
    };
    s.human_control = db
        .query_row(
            "SELECT state FROM human_control WHERE project_id=?",
            [project.to_string()],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(storage)?
        .map(|value| serde_json::from_str(&value).map_err(storage))
        .transpose()?;
    macro_rules! collection {
        ($field:ident,$ty:ty,$table:ident) => {
            for item in load::<$ty>(db, &$table, project, true)? {
                s.$field.insert(item.id, item);
            }
        };
    }
    collection!(questions, ResearchQuestion, QUESTIONS);
    collection!(sources, Source, SOURCES);
    collection!(papers, Paper, PAPERS);
    collection!(claims, Claim, CLAIMS);
    collection!(hypotheses, Hypothesis, HYPOTHESES);
    collection!(experiments, Experiment, EXPERIMENTS);
    collection!(experiment_runs, ExperimentRun, EXPERIMENT_RUNS);
    collection!(findings, Finding, FINDINGS);
    collection!(artifacts, Artifact, ARTIFACTS);
    collection!(relations, Relation, RELATIONS);
    collection!(tasks, Task, TASKS);
    collection!(runs, Run, RUNS);
    collection!(gates, Gate, GATES);
    collection!(work_nodes, WorkNode, WORK_NODES);
    collection!(work_edges, WorkEdge, WORK_EDGES);
    collection!(plan_revisions, PlanRevision, PLAN_REVISIONS);
    collection!(branches, Branch, BRANCHES);
    collection!(checkpoints, ExecutionCheckpoint, CHECKPOINTS);
    collection!(context_snapshots, ContextSnapshot, CONTEXT_SNAPSHOTS);
    collection!(process_memories, ProcessMemory, PROCESS_MEMORIES);
    collection!(execution_bindings, ExecutionBinding, EXECUTION_BINDINGS);
    collection!(model_invocations, ModelInvocation, MODEL_INVOCATIONS);
    collection!(tool_invocations, ToolInvocation, TOOL_INVOCATIONS);
    collection!(candidate_claims, CandidateClaim, CANDIDATE_CLAIMS);
    let mut stmt = db
        .prepare("SELECT paper_id,source_id FROM paper_sources WHERE project_id=? ORDER BY rowid")
        .map_err(storage)?;
    let links = stmt
        .query_map([project.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage)?;
    for link in links {
        let (paper, source) = link.map_err(storage)?;
        s.paper_sources.push(PaperSource {
            project_id: project,
            paper_id: paper.parse()?,
            source_id: source.parse()?,
        });
    }
    s.events = load(db, &EVENTS, project, false)?;
    s.provenance = load(db, &PROVENANCE, project, false)?;
    for receipt in load::<ActionReceipt>(db, &RECEIPTS, project, false)? {
        s.receipts.insert(receipt.id, receipt);
    }
    Ok(s)
}
fn write_state(db: &Connection, s: &ProjectState, before: &ProjectState) -> Result<()> {
    let project = s.project()?;
    if before.project.as_ref() != Some(project) {
        save(db, &PROJECTS, project, true, false)?;
    }
    if s.human_control != before.human_control {
        if let Some(control) = &s.human_control {
            let encoded = serde_json::to_string(control).map_err(storage)?;
            db.execute("INSERT INTO human_control(project_id,state) VALUES(?,?) ON CONFLICT(project_id) DO UPDATE SET state=excluded.state", params![project.id.to_string(), encoded]).map_err(storage)?;
        } else if before.human_control.is_some() {
            return Err(Error::Conflict("human control deletion forbidden".into()));
        }
    }
    macro_rules! registry {
        ($field:ident,$kind:literal) => {
            for e in s.$field.values() {
                if e.meta.project_id != project.id {
                    return Err(Error::ProjectMismatch);
                }
                if !before.$field.contains_key(&e.id) {
                    db.execute(
                        "INSERT INTO entity_registry(id,project_id,kind) VALUES(?,?,?)",
                        params![e.id.to_string(), project.id.to_string(), $kind],
                    )
                    .map_err(storage)?;
                }
            }
        };
    }
    registry!(questions, "question");
    registry!(sources, "source");
    registry!(papers, "paper");
    registry!(claims, "claim");
    registry!(hypotheses, "hypothesis");
    registry!(experiments, "experiment");
    registry!(experiment_runs, "experiment_run");
    registry!(findings, "finding");
    registry!(artifacts, "artifact");
    macro_rules! persist {
        ($field:ident,$table:ident) => {
            for item in s.$field.values() {
                if item.meta.project_id != project.id {
                    return Err(Error::ProjectMismatch);
                }
                if before.$field.get(&item.id) != Some(item) {
                    save(db, &$table, item, true, false)?;
                }
            }
        };
    }
    persist!(questions, QUESTIONS);
    persist!(sources, SOURCES);
    persist!(papers, PAPERS);
    for p in &s.paper_sources {
        if before.paper_sources.contains(p) {
            continue;
        }
        db.execute(
            "INSERT INTO paper_sources VALUES(?,?,?) ON CONFLICT(paper_id,source_id) DO NOTHING",
            params![
                p.project_id.to_string(),
                p.paper_id.to_string(),
                p.source_id.to_string()
            ],
        )
        .map_err(storage)?;
    }
    persist!(claims, CLAIMS);
    persist!(hypotheses, HYPOTHESES);
    persist!(experiments, EXPERIMENTS);
    persist!(tasks, TASKS);
    persist!(runs, RUNS);
    persist!(artifacts, ARTIFACTS);
    persist!(experiment_runs, EXPERIMENT_RUNS);
    persist!(findings, FINDINGS);
    persist!(relations, RELATIONS);
    persist!(gates, GATES);
    macro_rules! persist_runtime {
        ($field:ident,$table:ident,$immutable:expr) => {
            for (id, old) in &before.$field {
                if !s.$field.contains_key(id) {
                    return Err(Error::Conflict("runtime deletion forbidden".into()));
                }
                if $immutable && s.$field.get(id) != Some(old) {
                    return Err(Error::Conflict("immutable runtime record".into()));
                }
            }
            for item in s.$field.values() {
                if item.meta.project_id != project.id {
                    return Err(Error::ProjectMismatch);
                }
                if before.$field.get(&item.id) != Some(item) {
                    save(db, &$table, item, true, $immutable)?;
                }
            }
        };
    }
    persist_runtime!(work_nodes, WORK_NODES, false);
    persist_runtime!(work_edges, WORK_EDGES, false);
    persist_runtime!(plan_revisions, PLAN_REVISIONS, true);
    persist_runtime!(branches, BRANCHES, false);
    persist_runtime!(checkpoints, CHECKPOINTS, true);
    persist_runtime!(context_snapshots, CONTEXT_SNAPSHOTS, true);
    persist_runtime!(process_memories, PROCESS_MEMORIES, true);
    persist_runtime!(execution_bindings, EXECUTION_BINDINGS, true);
    persist_runtime!(model_invocations, MODEL_INVOCATIONS, false);
    persist_runtime!(tool_invocations, TOOL_INVOCATIONS, false);
    persist_runtime!(candidate_claims, CANDIDATE_CLAIMS, false);
    for receipt in s.receipts.values() {
        match before.receipts.get(&receipt.id) {
            Some(old) if old == receipt => {}
            Some(_) => return Err(Error::Conflict("action receipt is immutable".into())),
            None => save(db, &RECEIPTS, receipt, false, true)?,
        }
    }
    for event in &s.events[before.events.len()..] {
        save(db, &EVENTS, event, false, true)?;
    }
    for record in &s.provenance[before.provenance.len()..] {
        save(db, &PROVENANCE, record, false, true)?;
    }
    Ok(())
}
impl Storage for SqliteStorage {
    fn search(&self, project: ProjectId, query: &str, limit: u32) -> Result<Vec<EntityRef>> {
        let terms: Vec<_> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .take(32)
            .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
            .collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let db = self.db.lock().map_err(storage)?;
        let mut stmt=db.prepare("SELECT kind,entity_id FROM research_fts WHERE research_fts MATCH ? AND project_id=? ORDER BY bm25(research_fts),entity_id LIMIT ?").map_err(storage)?;
        let rows = stmt
            .query_map(
                params![terms.join(" OR "), project.to_string(), limit.min(100)],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .map_err(storage)?;
        let mut hits = vec![];
        for row in rows {
            let (kind, id) = row.map_err(storage)?;
            hits.push(match kind.as_str() {
                "claim" => EntityRef::Claim(id.parse()?),
                "paper" => EntityRef::Paper(id.parse()?),
                "question" => EntityRef::Question(id.parse()?),
                _ => return Err(Error::Storage("unknown search kind".into())),
            });
        }
        Ok(hits)
    }
    fn read(&self, project: ProjectId) -> Result<ProjectState> {
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db.transaction().map_err(storage)?;
        let state = read_state(&tx, project)?;
        tx.commit().map_err(storage)?;
        Ok(state)
    }
    fn list_projects(&self) -> Result<Vec<Project>> {
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db.transaction().map_err(storage)?;
        let mut stmt = tx
            .prepare("SELECT id FROM projects ORDER BY created_at,id")
            .map_err(storage)?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(storage)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(storage)?;
        let mut projects = vec![];
        for id in ids {
            let p = load::<Project>(&tx, &PROJECTS, id.parse()?, true)?
                .into_iter()
                .next()
                .ok_or_else(|| Error::Storage("project disappeared".into()))?;
            projects.push(p);
        }
        drop(stmt);
        tx.commit().map_err(storage)?;
        Ok(projects)
    }
    fn transaction(
        &self,
        project: ProjectId,
        operation: &mut dyn FnMut(&mut ProjectState) -> Result<()>,
    ) -> Result<Vec<DomainEvent>> {
        let mut db = self.db.lock().map_err(storage)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let mut state = read_state(&tx, project)?;
        let before = state.clone();
        operation(&mut state)?;
        if state.project()?.id != project || state.project()?.meta.project_id != project {
            return Err(Error::ProjectMismatch);
        }
        if !state.events.starts_with(&before.events)
            || !state.provenance.starts_with(&before.provenance)
        {
            return Err(Error::Conflict("history is append-only".into()));
        }
        write_state(&tx, &state, &before)?;
        let events = state.events[before.events.len()..].to_vec();
        tx.commit().map_err(storage)?;
        Ok(events)
    }
}

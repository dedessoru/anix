# Persistence and transaction semantics

V2 migration `0002_runtime.sql` adds dedicated work nodes/edges, revisions, branches, checkpoints, snapshots, memory, run bindings, model/tool invocations and candidate claims. Project-scoped foreign keys are deferred to commit for structural cycles; immutable records have SQL update/delete guards. FTS5 indexes claims, papers and questions with BM25 search through the Storage port. A V1 upgrade fixture verifies retained knowledge and rebuilt search. See [runtime-v2.md](runtime-v2.md).

`anix-storage::SqliteStorage::open` enables foreign keys, WAL, a five-second busy timeout, and FULL synchronous durability. Migrations are embedded from versioned SQL files, applied under an immediate transaction, and recorded with version/name/SHA256/UTC timestamp in `schema_migrations`. A changed applied migration or a database newer than the binary is rejected. Future schema changes require a new migration.

The database has dedicated tables for projects, questions, sources, papers, paper-source links, claims, hypotheses, experiments, experiment runs, findings, artifacts, relations, gates, tasks, runs, events, provenance, and action receipts. Authors and attempt/artifact links use child tables; claim locations use explicit columns. Structural status, version, ownership, and lineage are queryable columns. JSON is limited to extensible metadata and structured audit/action payloads, including typed actor and provenance/action references.

`entity_registry` contains only ID/kind/project ownership for polymorphic relation foreign keys. It contains no research content or arbitrary JSON and does not replace dedicated entity tables. Composite foreign keys enforce project membership. Indexes cover ownership, graph endpoints, claim publications, hypothesis experiments, pending gates, active runs, and chronological history.

Application mutations follow this sequence:

1. Acquire the Core mutation coordinator and `BEGIN IMMEDIATE` in SQLite.
2. Load a snapshot of the project aggregate.
3. Validate context, entity ownership, state transitions, versions, and action policy.
4. Apply typed changes and append provenance/events.
5. Persist changed entity rows, action receipts, and new history rows.
6. Commit; then publish the committed events through the local event bus.

Exceptions and constraint failures drop the transaction and roll back everything. The injected failure test rejects the final GateApproved event after entity/gate writes, demonstrating rollback of state, action receipts, provenance, and earlier event writes. An independent database reader verifies that a received live event is already persisted with the approved state.

Domain events have stable IDs, schema versions, UTC occurrence time, actor, optional Run, correlation/causation IDs, and payload. Provenance links the change to an entity, reason, source entities, Run, Gate, and event. The application supplies default causal links to earlier project/run events. SQL triggers prevent update/delete of events and provenance.

Artifact files and SQLite do not form a distributed transaction. Write a new file through Workspace, then register its verified metadata. A registration failure can leave an unregistered file; it cannot overwrite an existing artifact or publish a false successful database mutation. Projections live under `notes/generated` and are disposable. SQLite plus real artifact bytes are the sources of truth.

Back up a live WAL database using SQLite's backup facilities or stop writers and checkpoint it; copying only `state.sqlite` while writers run can omit committed WAL content. Model credentials belong in provider configuration outside domain payloads. Metadata validation rejects credential keys/recognizable tokens; raw provider errors/output are not persisted.

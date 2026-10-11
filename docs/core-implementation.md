# ANIX Core implementation

This document describes the V1 milestone. Subsequent V2 changes and measured results are documented in [runtime-v2.md](runtime-v2.md) and [v2-results.md](v2-results.md).

The updated greenfield specification superseded the initial single-crate iteration. The implementation now uses six workspace crates, dedicated relational persistence and migrations, typed entities/actions/policies, a transactional application API, graph/evidence queries, task/run and experiment attempts, human gates, and a restricted Lead Agent capability.

The generic entity table and obsolete initial implementation were removed. SQLite stores canonical state; workspace files store artifact bytes; append-only events and provenance preserve history; generated Markdown can be rebuilt.

The central acceptance scenario covers the complete Question → Literature → Hypothesis → Agent proposal → Human approval → ExperimentRun → Finding → evidence trace → restart path. Additional checks cover stale gates, write failure rollback, post-commit live events, migration/FK integrity, idempotency, agent/tool permissions, and CLI behavior.

The project aggregate transaction port is a deliberate local V1 design choice. Only changed rows are persisted, but reads load an aggregate and history. Larger datasets can introduce narrower repository operations and pagination behind the same application API. External models/scientific tools and graphical/terminal interfaces remain extension work explicitly outside this specification.

See `architecture.md`, `domain-model.md`, `persistence.md`, and `agent-architecture.md` for the final boundaries and practical limits. `implementation-plan.md` records the implementation sequence.

Final verification on Windows/MSVC with the consistent rustup Rust 1.96 toolchain: formatter check, workspace check, Clippy with warnings denied, and workspace tests all passed. Sixteen tests passed: three domain cases, one workspace/files case, nine acceptance cases, one CLI case, and two compile-fail capability cases. The unfinished-path scan found no matches in production code, migrations, or tests. Domain/Core/Agent manifests contain no SQLite/storage/integration dependency. `scripts/check.ps1` reproduces the verification despite competing toolchains on PATH.

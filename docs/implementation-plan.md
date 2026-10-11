# Implementation plan

The starting code was a single crate with a generic entity table, string statuses, and a minimal gate test. The updated specification supersedes those choices. No legacy API or database compatibility is required.

1. Establish six workspace crates and typed entities, policies, commands, events, and errors.
2. Implement the application API against a transactional storage port. A project is the local aggregate boundary.
3. Add dedicated SQL tables, migration history, foreign keys, and atomic state/audit writes.
4. Add graph/evidence, gates/version checks, task/run and experiment execution services.
5. Implement a restricted agent API, deterministic context, provider/selector abstractions, and fake provider.
6. Implement bounded workspace files, hashes, and rebuildable projections.
7. Connect the development CLI to the same API.
8. Verify the complete research loop, stale gates, rollback, live events, restart, migrations, and boundaries; complete documentation.

Storage loads a typed project aggregate for application transactions and persists it into dedicated relational tables. This keeps the replaceable storage interface small and the transaction boundary explicit. It is appropriate for local V1 research projects; large projects may later need narrower repository operations without changing the public client API.

All eight phases are implemented and verified. The final implementation and verification results are recorded in `core-implementation.md`.

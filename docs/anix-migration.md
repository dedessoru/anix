# ANIX platform migration

ANIX names the entire platform. Seven packages remain: `anix-domain`, `anix-core`, `anix-storage`, `anix-agent`, `anix-integrations`, `anix-evals`, and `anix-cli`. The sole current CLI is `anix`. The former application/Core naming split is withdrawn.

## Backup

Before data changes, an online SQLite backup and copies of application research-file directories/configuration/model profiles were created with source/backup hashes: [manifest](anix-platform-backup.json). Prior audit/trace/fingerprint documents are preserved in `docs/history/2026-10-09-engine-audit/`. Models, source IDs, artifact content and historical events/instruction versions are preserved.

## Storage

Canonical storage is `.anix/state.sqlite`. `Workspace::open` checks legacy `.rn` before creating the canonical store. If only legacy exists, a single directory rename moves database, WAL, shared-memory file and operational artifacts together. No competing database copy is created. A locked/failed rename returns an error instead of opening an empty replacement database.

If both store directories exist, opening fails with an explicit conflict. Neither store is automatically selected, merged, deleted or overwritten. The resolver rejects symlink/out-of-workspace paths and migration is idempotent. File tools protect canonical and legacy paths, including case/trailing-dot aliases and nested workspaces.

Recorded artifact paths beginning `.rn/` keep their spelling/hash. A temporary resolver maps those references to `.anix/` after migration; other source/artifact paths stay in place. Historical provenance is not rewritten.

## Schema and API

SQL migrations 1-4 retain their checksums. Additive migration 5 adds nullable `work_nodes.outcome`; old rows/serialized PlanRevisions deserialize with outcome None. No table is rebuilt or record deleted. Inconclusive is separate from lifecycle status: the unmet work contract remains `failed`, its outcome is `inconclusive`, and execution can complete normally. It never counts as Succeeded/completed research.

New Lead instructions use `anix-lead-step-v2`. The shared Application API is `anix_core::Core` and its project, research, Human Control, Gate, work/context/query services. `AgentApi` is its restricted Lead capability. Future TUI/GUI/MCP remain separate clients of this API. No interface contains research business logic.

Version-checked objective changes always require a Human Gate. Proposals equal to the current normalized objective return the existing work without a Gate or new revision. Lead receives no Human-only capability. Direct Human mutations require the creator. Termination runs through Core, replacing the driver-specific Human acceptance path.

## Commands

```powershell
cargo build -p anix-cli
.\target\debug\anix.exe --help
cargo run -p anix-cli -- --workspace ./my-research project create --name "Study"
.\scripts\check.ps1
```

With the binary directory on PATH, `anix --help` launches the same CLI. Environment names are `ANIX_TEST_LEAD_CONFIG` and `ANIX_CRASH_FIXTURE`. Scripts/examples use canonical packages. Actual model profile IDs, filenames, revisions and weight checksums are preserved.

## Allowed legacy occurrences

- `.rn` in migration resolution, conflict checks, protected-file policy and regressions.
- Old product/package spellings in immutable historical data, dated reports, the archived audit and negative-test explanations.
- Historical compiled outputs/runtime case directories/backup manifests are evidence, not current products. The parent workspace directory is user-selected.

Current manifests, imports, executable, environment, scripts, help and architecture use ANIX. Exact compatibility and legacy-occurrence evidence is linked from the [audit](anix-audit.md).

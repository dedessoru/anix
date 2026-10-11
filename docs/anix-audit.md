# ANIX platform rename and Lead orchestration audit

Date: 2026-10-09. Final verdict: **ANIX READY FOR FIRST REAL RESEARCH PILOT: NOT YET**.

## ANIX rename

| Item | Result |
|---|---|
| Complete project rename | YES for active platform components |
| Canonical project name | ANIX |
| CLI executable / package | anix / anix-cli |
| Rust workspace | Seven ANIX packages; Cargo manifests, imports and binary updated |
| Application API | anix_core::Core application services / restricted AgentApi |
| TUI / GUI / MCP architecture | Separate clients of the ANIX application API; none created |
| Old data compatibility | PASS; 207 main and 2 historical runtime projects |
| Canonical storage | .anix/state.sqlite |
| Remaining legacy names | Migration alias/protection tests, backup/baseline manifests and immutable historical reports; see anix-legacy-occurrences.json |

[Old CLI build outputs were archived with hashes](anix-legacy-build-archive.json). The user-selected outer directory is unchanged. Linux model launch scripts now use anix-cli, as do Windows scripts. Profile/model identifiers and Qwen3-1.7B trust boundaries remain compatible. The current CLI was invoked as `anix --help`; integration tests also exercise creation, state, Human Control and JSON. No UI or framework was added.

Backups precede edits/migration: [backup manifest](anix-platform-backup.json). The legacy store moves as one directory including SQLite WAL/SHM and artifacts; dual stores cause a conflict. Historical stored paths resolve through a temporary alias. No IDs, checkpoints, context snapshots, audit events or provenance were rewritten. [Compatibility evidence](anix-platform-compatibility.json) compares every original table's old columns and rows with online SQLite backups. Additive schema migration 5 stores research outcomes; [SQL migrations 1-4 are unchanged](anix-schema-compatibility.json).

## Lead orchestration

### Confirmed root causes and fixes

| Mechanism | Affected files | Root cause / implemented fix |
|---|---|---|
| Decision context | anix-core/src/runtime.rs; anix-integrations/src/test_lead.rs | Child results existed but no compact child plan/readiness. Pin immutable decision ledger, evidence stages, dependencies and blockers in existing context budget. |
| Duplicate work | anix-core/src/runtime.rs; anix-agent/src/lib.rs | Every matching objective created new work. Reuse normalized objective/capability per parent/branch, emit persisted observation; explicit justified repeat preserves replication. |
| Repeated retrieval | anix-evals/src/long_horizon.rs | Reject reset transient search guard. Compare persisted query/source/paper/work versions; return existing invocation without false progress. |
| Plan revision | anix-core/src/actions.rs, runtime.rs, human_control.rs; anix-domain/src/runtime.rs | Lead lacked compact authorized proposal. Actual changes use versioned owner Gates and immutable revisions. Same objective returns existing work without Gate/revision. |
| Research termination | anix-core/src/runtime.rs; anix-agent/src/lib.rs; migrations/0005_work_outcome.sql | Driver-only acceptance and false Succeeded for Inconclusive. Shared Core validates explicit finish, contracts, evidence, Gates/dependencies and active runs; persists distinct outcome. |
| Retrieval completion bypass | anix-core/src/runtime.rs | Direct API could finish with pending retrieval. Core reconstructs obligations from persisted invocation metadata independently of adapter names. |
| Owner projection | anix-core/src/queries.rs, human_control.rs | Current-status owner null while authorization inferred creator. Projection now uses the same control resolution; no new permission granted. |
| Instructions | anix-integrations/src/test_lead.rs; anix-evals/src/long_horizon.rs | Goal/system text repeatedly directed child creation even on Atomic work. Decisions now use actual plan/results; no fixed scientific pipeline. |

Raw baseline Reject decisions sometimes described reading. The parser preserved the requested Reject action: no demonstrated remapping bug, no inference about hidden reasoning and no forced read. Discovery, consumption, Claim extraction and Finding production remain distinct. Reading alone does not establish scientific quality; Completed still requires an explicit source-based Lead assessment.

Regressions were added for duplicates, justified gated experiment replication, plan/result context, branch isolation, valid completion, Inconclusive persistence, Gate authorization, rejected-search/no-progress, direct pending-retrieval bypass, migration/conflict and CLI/Human JSON. No-op plan and pending retrieval regressions were observed failing before their fixes.

## Seed 664

Model: Qwen3-8B Q4_K_M; RTX 2060 12 GB; CUDA 37 layers; context 8192; temperature 0.2; LocalOnlyTest. Model/profile/server/runner/source/report/trace hashes: [fingerprint](anix-model-fingerprint.json).

| Metric | Old baseline | Intermediate new run | Final new run |
|---|---:|---:|---:|
| Capability | FAIL | FAIL | FAIL |
| Lead calls | 40 | 5 | 5 |
| Search | 13 | 0 | 0 |
| ReadSource | 6 | 0 | 0 |
| Reject | 11 | 0 | 0 |
| Branch records | 2 | 2 | 2 |
| Duplicate-work observations | not recorded | 0 | 0 |
| Same-objective plan proposals | not offered | 4 | 4 |
| Gates for those proposals | not applicable | 4 | 0 |
| Final report state | budget_exceeded | blocked | blocked |
| Integrity | PASS: 286 checks | PASS: 43 checks | PASS: 27 checks |
| Recovery | PASS | PASS | PASS |

Final report: `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/20261009T162401325/branching-000664/report.json`.
Trace Artifact: `74bb8313-7a30-46c4-adb9-df6feb79eeaf`.
Final run: 24,355 ms; 2 WorkNodes; 11 checkpoints; 0 invalid references, structured failures, tool failures or repairs; context max/p95 3743 tokens. Duplicate-work counter counts reuse observations, not proof of comprehensive semantic uniqueness; no child was requested in this run.

Turn 1 creates a second Branch record. Turns 2-5 copy the current objective into propose_plan_revision. Runtime supplies existing_plan/no_progress, creates no Gate/revision and retains authorization checks. The Lead repeats that action despite feedback. Unchanged four-no-progress guard stops execution; 40-call limit was not exhausted. Report Blocked describes harness stagnation; cleanup leaves root **Failed, outcome null**. There is no explicit Lead research termination, no source utilization, and no independent research in both branches. This is FAIL, not a safe-terminal capability PASS.

[Exact final timeline](anix-seed-664-trace.md), [diagnosis](anix-lead-liveness.md). Full before/after loop reconstructions: `.runtime/anix-loop-before-reconstruction.json` and `.runtime/anix-loop-after-reconstruction.json`. All old raw traces/reports remain unchanged; previous audit documents are preserved under `docs/history/2026-10-09-engine-audit/`. No larger-model/retry/budget change was made. Seed is a corpus seed, not a deterministic sampling seed.

## Seed 654 and campaigns

| Gate | Result |
|---|---|
| Seed 654 | NOT RUN; seed 664 capability gate failed |
| Targeted model regression | NOT RUN; prior gates closed |
| Mixed 20 | NOT RUN; prior gates closed |
| 200-run campaign | NOT RUN; prior gates closed |
| Real pilot | NOT RUN |

Deterministic workspace capability/restart tests passed; these do not replace the gated real-model campaigns. No previous campaign result is presented as verification of this source version.

## Tests

Command: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check.ps1`.

| Check | Result |
|---|---|
| Workspace tests | 157 passed; 0 failed; 2 ignored |
| Doc tests | 3 passed; 0 failed |
| Format / check / build | PASS |
| Clippy (warnings denied) | PASS |
| CLI / JSON / Human Control / migration | PASS |
| Old SQLite workspaces | PASS: original rows/columns preserved |

The two ignored test functions are the subprocess crash fixture (exercised by its parent test) and optional small-model integration smoke. The requested 8B model was actually loaded and exercised. Windows Rust/Cargo stable 1.96.0 uses a matching RUSTDOC. [Verification and log hashes](anix-verification.json); [source inventory](anix-inventory.json). Shell scripts were inspected/renamed; Linux execution was not performed on this Windows host.

## Remaining issues

| Severity | Issue / affected files | Observable cause | Recommended next fix |
|---|---|---|---|
| BLOCKER B01 | Planning stagnation: anix-integrations/src/test_lead.rs; anix-agent/src/lib.rs; anix-evals/src/long_horizon.rs | Final raw action repeats an unchanged objective despite displayed blockers/no-progress; no child/source action. Runtime invariants are safe, research contract remains unmet. | Reproduce action selection from preserved rendered inputs/schema, inspect compact action and feedback presentation, add a regression for a demonstrated interface defect before changing the common mechanism. Do not attribute automatically to model capacity or force a read. |
| HIGH | Branch capability assertion: anix-evals/src/long_horizon.rs | Driver counts Branch records without proving independent branch work/reconciliation. Final run performed neither. | Require branch-specific work/evidence and reconciliation assertions; expose authorized branch targeting if the actual decision interface cannot express it. No automatic evidence duplication. |
| HIGH | Scientific completion evaluation: anix-evals/src/long_horizon.rs; anix-core/src/runtime.rs | Evidence-stage/provenance checks protect execution but do not independently score support, contradictions or terminal synthesis. | Add source-grounded outcome evaluation across varied research questions; retain explicit Lead judgment. |
| MEDIUM | Semantic duplicate coverage: anix-core/src/runtime.rs; anix-evals/src/long_horizon.rs | Work matching uses normalized exact objective/capability; retrieval fingerprint/version equality. Paraphrases are not a scientific equivalence oracle. | Evaluate demonstrated paraphrase repetitions, present existing work clearly and require repeat provenance; avoid merging legitimate independent studies. |
| LOW | Non-Windows script execution: scripts/bootstrap-model.sh, start-test-model.sh | Package references inspected, no Linux environment exercised. | Run the canonical launch workflow on supported Linux host before release. |

## Final verdict

**ANIX READY FOR FIRST REAL RESEARCH PILOT: NOT YET.** Platform rename and data compatibility pass. Shared orchestration repairs and deterministic regressions pass. B01 and real-model capability gates remain open. No research completion was fabricated.

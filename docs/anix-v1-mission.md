# ANIX Research Engine v1 engineering mission

Status: active. Definition of Done and gates are in todo.md. Persistent machine state: `.runtime/anix-v1-mission-state.json`.

## Initial independent diagnosis

Final seed 664 renders no available-action schema in the messages. The response-format grammar constrains syntax but the Lead is asked to follow an action catalog it cannot read. Composite exposes an empty tools list although child capabilities exist. Hypothesis V1-01: missing model-facing action descriptions contributes to replan selection; deterministic request regression and controlled real-model comparison will test it.

V1-02 is independently confirmed in code: ProposePlanRevision maps only to ReviseWorkObjective. Objective equality is not a strategy identity; an alternative work graph cannot be expressed by this action. Add a structured work-plan change proposal with existing authorization, immutable revisions and explicit no-op semantics.

No claim of model incapacity follows from these traces. Existing runtime tests and historical model runs are baselines, not acceptance of this mission. Source files are backed up with hashes before edits (path in machine state). User research stores and original traces remain preserved.

## Acceptance order

1. Standard checks, SQLite/Human/authorization/recovery.
2. Controlled diverse methodology execution through real Core/SQLite/tools.
3. Known seeds 664 and 654; diagnose and repair failures autonomously.
4. Targeted including incomplete evidence and valid Inconclusive.
5. Mixed 20: >=19 completion, all integrity/recovery/checked safety.
6. 200: >=95% completion, all integrity/recovery/checked safety.
7. Held-out scientific quality plus real CLI/API end-to-end acceptance.

No threshold expansion, forced-source selection, fake completion or paid APIs. Update machine state after each substantive iteration.

## Iteration 1: action interface

Regression failed because messages had no action schema, then all 20 adapter tests passed after adding the exact dynamic schema, semantic action descriptions and child capabilities. Real 8B seed 664 `20261009T170748999` now creates useful tasks and performs 3 searches / 6 reads / 3 consumed batches, without replan or parser errors. It still fails at 40 calls because it requests one completed task 24 times. Integrity 212 checks and recovery pass. Runner is preserved as `.runtime/anix-v1-lead-runner.iteration-01.exe`.

## Iteration 2: strategy, feedback and dispatch

Four regressions were observed failing before their repairs:

- Same research objective with a different executable method was unexpressible. ReviseWorkPlan now proposes typed tool steps and dependency indexes; owner-approved Gate updates the existing Work Graph while preserving objective, completed results and immutable prior revisions. Identity considers work/tool/dependencies, not objective alone. Identical executable work returns the existing plan without Gate/revision. Deliberate replication retains its separate reason-bearing action.
- Duplicate child reuse reset the fixture idle counter (36 repetitions / 40 calls in reproduction). It now reaches the unchanged four-no-progress guard after four duplicates.
- Planning feedback could lose context priority to source observations. Its latest bounded payload is now retained in pinned Decision state along with consecutive no-op count.
- A dependency task in Proposed could not start after its prerequisite completed. Agent start_work now checks the existing Ready transition before Running. Core derives executable child work from durable nodes/dependencies; the driver reads this instead of maintaining a separate queue. Actual strategy approvals count as execution progress, identical proposals do not.

Full standard verification is running; strategy authorization/graph safety and compact protocol tests are next. These repairs do not prove real-model completion or scientific quality. Independent branch-work/reconciliation remains an explicit next capability gap.

## Iteration 3: constrained grammar orders the decision incorrectly

Exact raw completions in iteration 1 put objective/required_tool/title before type, and branch name before type (even the name becomes create_branch). Rust serde_json sorted map keys. The local b11307 llama.cpp source preserves property insertion order (common/json.h), builds schema properties in that order (common/json-schema.cpp), and builds required grammar properties in that order (common/json-schema-to-grammar.cpp). Thus generating arguments selects an action family before the intended discriminator is generated.

The property-order regression failed before repair. serde_json now preserves insertion order and compact_action inserts type first. All 22 adapter tests pass. An isolated local 8B comparison reuses exact completed-work input from turn 17 and changes only property order in its schema/catalog. At temperature 0.2, three paired sampling seeds gave CreateWorkNode for all arguments-first requests and FinishResearch for all type-first requests. Full requests/raw outputs/usage are preserved in `.runtime/anix-v1-grammar-order-comparison.json`. This is diagnostic inference without Core execution, not a capability PASS or independent science-quality score.

The prior full check passed 166 ordinary / 3 doc tests, 2 ignored. A fresh full check is running after the serialization feature change. Additional plan tests protect versioned approval, active execution, parent scope and external prerequisites; the external-prerequisite reproduction failed before its guard. Objective changes are now propose_objective_revision; propose_work_plan represents strategy under a fixed objective. The old compact name is a deserialize-only compatibility alias.

## Iteration 4: stop a false branch capability PASS

The negative test for two empty Branch records failed against the old assertion. The report now measures completed atomic evidence work per actual Branch and explicit result reconciliation; both are required. A capability failure is separate from the primary execution failure and cannot erase budget/stall diagnostics. The campaign summary uses the stronger predicate. Actual branch targeting/reconciliation is still absent in the compact interface and remains work to implement, rather than a model-capacity conclusion.

The fresh full check is running after formatting and these assertions. Next actual 664 run will distinguish workflow completion from genuine Branch capability. Additional hypotheses queued for controlled reproductions: inherited input-contract truncation; source-consumption checks tied to a single tool name; durable fork lineage and branch selection.

## Iteration 5: resume incomplete branch integration

The actual iteration-04 report `20261009T174633321/branching-000664/report.json` completed its execution in 12 Lead calls (2 searches, 4 reads, 2 consumed batches) but failed the stronger branch capability: one completed evidence branch, no reconciliation. Integrity (84 checks) and recovery passed. This is not acceptance of branching.

At resumption, source changes for branch lineage/selection/results and supplied-source inheritance were present but not fully integrated or recorded. `cargo check` failed on an unhandled SwitchResearchBranch variant in the HTTP adapter. Sources were copied to `.runtime/anix-v1-source-backup-20261010-branch` before edits.

The adapter now renders executable, unpaused branch handles and explicit review handles in the dynamic action schema; the driver schedules the selected composite scope, returns completed branch results to its parent, and only treats the root's explicit termination as overall completion. Branch selection itself does not count as research progress. The focused SQLite test `independent_branch_results_require_scoped_selection_and_explicit_review` passes: foreign identity/stale version are denied, unresolved/unreviewed branches prevent termination, and Inconclusive plus result/review provenance survives restart. This is a runtime lifecycle test, not scientific evidence or model capability acceptance.

Runner `.runtime/anix-v1-lead-runner.iteration-05.exe` is running actual local 8B seed 664. Full standard verification is recorded in `.runtime/anix-v1-check-iteration-05.log`. Inspect both results before advancing; all downstream gates remain pending.

Iteration-05 is now complete: report/trace `20261009T211736585/branching-000664` has 14 calls, 2 searches, 4 reads, 2 consumed batches, two completed evidence branches and explicit reconciliation, integrity (93 checks) and recovery PASS. The root explicitly selected Inconclusive; this is not Completed acceptance. Full iteration-05b checks passed 173 ordinary and 3 doc tests, 2 ignored; format/check/clippy PASS. The driver regression independently proves branch execution/reconciliation. Source inventory and fresh runner are `.runtime/anix-v1-source-inventory.iteration-05b.json` and `.runtime/anix-v1-lead-runner.iteration-05b.exe`.

## Iteration 6: investigate semantic outcome before another full run

The final summary gives both P and Q as plausible but declares uncertainty about which is more accurate. The pack asks which mechanism *could* explain Y and its ground truth expects two plausible paths, not a unique causal proof. The model strengthened the objective in its created work. The final schema offers all three outcomes, readiness is true and two source reads are visible, so an action prohibition does not explain the outcome.

`.runtime/anix-v1-scope-comparison.json` replays the exact final request/schema with paired sampling seed 92001 at temperature 0.2: original and an added generic scope/status clarification both return Inconclusive. No prompt repair was applied on this evidence. The comparison is not Core execution, capability acceptance or proof of model incapacity. A legitimate uncertain outcome must remain distinct from Completed.

The available 14B file was verified against its recorded SHA256. The owned 8B server was stopped after all calls completed; diagnostic 14B uses PID 32608, loopback 8084, context 8192 and the same 37-layer setting. `.runtime/anix-v1-14b-comparison.py` will compare the identical original final input/schema/temperature/seed. Do not count that replay as a full research workflow.

The GPU37 replay timed out after 900 seconds with no completion. Only about 93 tokens were decoded, under high shared GPU/RAM load. Exact request/error and resource snapshots are in `.runtime/anix-v1-14b-comparison.gpu37-timeout.json`, `.runtime/anix-v1-resource-diagnostics.json` and `.runtime/anix-v1-resource-after-timeout.json`. The owned server was stopped; no unrelated applications were stopped. This is an infrastructure failure, not proof of model incapacity.

## Iteration 7: balance diagnostic compute before declaring a blocker

After stopping the model, about 9 GB VRAM and 4.5 GB RAM were available. A fresh server on loopback8084 uses 24 GPU layers (PID32536 at this checkpoint), preserving context8192, temperature0.2, the original input/schema/sampling seed and timeout900. VRAM occupancy fell from about11.8 to10 GB. Startup completed in about11 seconds; prompt processing uses CPU layers and takes longer, but generation has advanced to72 tokens well before the former timeout. The fresh result will be `.runtime/anix-v1-14b-comparison.gpu24.json`. The change in offload configuration must remain explicit in any model comparison.

Real CLI smoke create/project question/read persisted state passed; `.runtime/anix-v1-cli-smoke.json` preserves outputs. `agent --help` and `main.rs:493` confirm only the deterministic demo is exposed by that subcommand, so this smoke does not establish an interactive model-backed project-to-result CLI path.

## Iteration 10: substantive atomic task results (verification in progress)

Actual 8B long-context seed654 finished Blocked:14calls,2searches,4reads,2consumed,4duplicate work proposals,integrity92,recoveryPASS. Report: `.runtime/lead-runtime-cleanup-seeds/long-context-654/eval-results/long-horizon/runs/20261009T221022273/long-context-000654/report.json`. This was a cross-question diagnosis, not a passed gate. Traces showed successful atomic completion carried no produced analysis, only a status receipt; parent repeatedly requested the already-succeeded analysis. New adapter regression reproduced missing summary before repair.

Minimal repair adds bounded explicit task result on completion, stored in existing ProcessMemory and pinned for completed direct children. Source artifact references come from successful bound-work tool invocations. The result remains unreviewed Lead output, not Claim/Finding/truth. Completion, stall, budget, Human Gates and scientific outcome thresholds are preserved. Full checks in progress in `.runtime/anix-v1-check-iteration10.*.log`. Previous173ordinary+3docPASS is historical, not verification of this patch.

User confirmed memory freed. Owned8B18468 stopped;14B10604 started on8084/GPU37/ctx8192 for fresh identical664-final-turn replay. New artifact target `.runtime/anix-v1-14b-comparison.freememory-gpu37.json`. Prior GPU24 replay completed Inconclusive; prior GPU37 timeout was resource failure, not capability evidence.
# Iteration 11 — bounded completed-work result previews (2026-10-10)

Resumed the existing iteration10 checkpoint after reading the updated todo.md. Its full check had failed: `tool_and_step_counts_cannot_finish_research_without_lead_decision` reached only 15 meaningful steps because completed-work result previews crowded the pinned question out of the 4096 context budget. A first shared preview limit still reproduced failure (12 steps; Question needed 359 bytes, only 346 remained). Diagnostics are retained in `.runtime/anix-v1-result-packing-diagnostic.log`.

Core now shares a constant preview budget across completed children and reserves pinned entities and Human constraints before including optional result previews. If they cannot fit, each result retains its ProcessMemory ID and an explicit preview_omitted marker. Full summaries/artifact lists remain in the original persisted memory; ordinary context packing can include them. Lifecycle ledger, readiness, explicit termination, scientific status and existing budgets remain unchanged.

Focused checks PASS: original no-auto-completion test (`.runtime/anix-v1-result-packing-reserved.log`) and new SQLite regression `child_result_previews_preserve_pinned_question_and_full_persisted_results` (`.runtime/anix-v1-result-preview-test.log`). Full standard check and fresh 8B seed664 are underway; do not infer acceptance from focused tests. Backup: `.runtime/anix-v1-runtime-before-result-packing.rs`; previous machine checkpoint preserved as `.runtime/anix-v1-mission-state.before-session11.json`.

# Iteration 12 — branch review and recovery contracts (2026-10-10)

Fresh CUDA 8B seed664 `20261010T075239700` is Blocked: 20 calls, 3 searches, 6 reads, 3 consumed batches, one completed evidence branch and no reconciliation; integrity130/recovery PASS. Three invalid finishes referenced an unreturned branch B2, followed by a valid first switch to B2. Driver counted that first dispatch as the fourth consecutive idle turn and stopped before branch execution. Adapter also advertised B2 as reviewable despite its unfinished outcome. This is a contract/driver defect, not evidence of model incapacity.

Two regressions failed before repair: `first_branch_dispatch_after_rejections_can_recover_without_false_research_progress` and the corrected `branch_catalog_exposes_only_executable_scopes_and_requires_review_handles`. Adapter now offers only returned Completed/Inconclusive branch outcomes for review. Driver resets execution idleness on first dispatch to a new executable scope; switching has no research-progress credit, and subsequent switches cannot reset idleness. Negative regression `repeated_branch_switches_do_not_disable_the_stall_guard` preserves the existing threshold.

Before/after logs: `.runtime/anix-v1-branch-recovery-before.log`, `.runtime/anix-v1-branch-recovery-after.log`, `.runtime/anix-v1-branch-reviews-before.log`, `.runtime/anix-v1-branch-reviews-after.log`. Backups: `.runtime/anix-v1-long-horizon.before-iteration12.rs`, `.runtime/anix-v1-test-lead.before-iteration12.rs`. Standard checks and a new model-backed run are required; previous PASS does not validate this new source version.

# Iteration 13 — preserve a readable result under context pressure (2026-10-10)

8B seed654 `20261010T080520969`: Blocked after37calls,18reads,5consumed/offloaded batches and4duplicate-work observations; integrity253/recovery PASS. Exact final rendered messages show all5completed child results reduced to memory_id/preview_omitted. No substantive result or source observation survived packing; only the decision ledger and generic completed-task note remained. Parent readiness was true and Completed was available. Persisted result IDs alone were insufficient because the current compact interface cannot read those memories by ID.

Strengthened the pinned-question/result SQLite regression to require at least a readable substantive result; it FAILED on iteration12. Pressure packing now shortens completed objective previews (IDs/statuses/full objectives remain persisted), retains unreviewed-result labeling and includes the most recent completed result at a size that fits the existing mandatory context reserve. Full result memories/artifacts are unchanged. The regression passes; full checks and fresh654 evaluation are underway. Logs `.runtime/anix-v1-result-visibility-before.log`/`after.log`; backup `.runtime/anix-v1-runtime.before-iteration13.rs`.

# Latest session checkpoint — iteration13 model comparison

Standard checks PASS177ordinary+3docs,2ignored. 8B654 `20261010T081048725` still repeats completed work despite readable results and available Completed (14calls,4duplicates,integrity92/recoveryPASS). Full14B comparison on the same corrected runner/corpus/protocol:654 `20261010T081319661` Completed in6calls,2reads,integrity44/recoveryPASS;664 `20261010T081513351` Blocked in9calls,only1evidence branch/reconciliation,integrity58/recoveryPASS. Branch Inconclusive was returned; root had no completed main-branch child and demanded a more likely/unique mechanism. Neither known-seed gate nor later campaigns pass. These cases do not prove universal model incapacity.

All session-owned model servers are stopped. Final source/runner/check hashes: `.runtime/anix-v1-session13-manifest.json`; complete workflow comparison `.runtime/anix-v1-session13-full-model-comparison.json`. See rewritten final-report/resume documents for exact next steps. Mission remains unfinished, with no automatic continuation configured.


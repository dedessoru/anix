# ANIX source audit and readiness report - 2026-10-09

**ANIX READY FOR FIRST REAL RESEARCH PILOT: NOT YET.** The pre-retrieval deadlock and reproduced runtime defects below are fixed and covered by regressions. The final Qwen3-8B seed 664 still fails to produce an explicit valid research termination within the unchanged 40-call budget. Integrity and recovery pass. Seed 654 and later model campaigns remain gated by todo.md.

This report describes the actual edited source and executed checks. A passing endpoint smoke or a collection of read calls is not treated as research completion or scientific accuracy.

## A. Naming and reproducibility

ANIX naming implemented: **YES**. The six engine crates/directories are now `anix-domain`, `anix-core`, `anix-storage`, `anix-agent`, `anix-integrations`, and `anix-evals`. Workspace keys, Cargo.lock, imports, examples, tests, scripts and current architecture documents use the canonical names. Public imports are `anix_domain`, `anix_core`, `anix_storage`, `anix_agent`, `anix_integrations`, `anix_evals`; `anix_core::domain` remains the Domain reexport. `Core` and other descriptive public type names are unchanged. Invocation start now accepts the selected provider name.

Researcher's Notes remains the application name; its `rn-cli` package, `rn` executable and application environment/profile identifiers are preserved. It is the composition/client layer, not an independent engine crate. No extra crate was added. No package aliases were necessary for a published/in-repository downstream consumer. See [migration](anix-migration.md).

There is no Git repository. [Pre-edit baseline](anix-baseline.json) records 57 source/configuration/test/manifest files and hashes. [Final inventory](anix-inventory.json) records 65 source/build/profile files with paths, sizes and hashes. The inventory includes seven manifests, the actual Rust modules, tests, scripts, configuration, model profiles and four SQL migrations; model weights/build output/history are not treated as source. A clean pre-edit test run was not executed in this audit; historical reported test totals are not presented as new evidence.

The actual dependency map, module inventory, public service boundaries and persistence layout are in [architecture](anix-architecture.md). Core/Domain have no UI, SQLite, HTTP or model-vendor dependency. Storage and Integrations implement their ports; CLI composes clients through Core; Evals uses the same Lead/Application entrypoints. Seven crates remain.

Storage compatibility: **PASS**. SQL migrations 1-4 are byte-for-byte unchanged from the baseline; no database/schema/ID/event/artifact rename was made. SQLite online backups of the existing application database (207 projects) and a historical runtime database (2 projects) opened through the final `rn` binary. All projects were listed; one application project and both historical runtime projects had process/control projections read. Full logical database dump SHA-256 and every table count remained identical before/after reopening. See [compatibility evidence](anix-storage-compatibility.json). Migration/upgrade, checkpoint/restart, pending retrieval, Gates, Human controls, branch/budget/context/artifact references also pass their ordinary regressions. Compatibility checks verify stored records/references, not the content hash of every historical artifact file.

## B. Source audit coverage

The audit combined workspace/module/API inventory, inspection of mutation/validation/storage/model/tool/context entrypoints, negative regressions and the final real-model boundary trace. This is engineering evidence for the checked paths, not a certification of every possible input or an independent security review.

| Area | Status | Issues found | Issues fixed | Remaining |
|---|---|---|---|---|
| Domain Core | Reviewed; tests PASS | A12 | Creator authorization on all Human command mutations | General scientific quality unverified |
| Orchestration | Runtime regressions PASS; real workflow FAIL | A01, A02, A07, A13 | Executable prerequisites, pending enforcement, valid-output acceptance | B01: Lead repeats tasks without termination |
| Work Graph | Reviewed; tests PASS | A07 | Invalid observation cannot satisfy a required read | Lead-generated objectives remain generic in the failing trace |
| Context Compiler | Regressions PASS | A03, A04, A05 | Exclusions, pinned constraints, completed child/dependency results | Conservative byte-token estimates; bounded summaries |
| Retrieval | Regressions PASS | A02, A03 | Partial-batch binding/superseding; authorized handles/access | Model rejection intent is a scientific decision, not a Rust text classifier |
| Execution Runtime | Regressions PASS | A08 | Accurate provider name; immutable identity/strong route | Adapter-reported model revisions remain configurable/placeholders on the Lead path |
| Persistence | Reviewed; compatibility/rollback/restart PASS | No new migration defect reproduced | SQL bytes and logical old data preserved | Larger concurrent/fault campaigns not performed under closed gates |
| Authorization | Negative regressions PASS | A03, A09, A10, A12 | Creator-only Human mutations; source policy; no redirect/proxy escape; storage aliases denied | Independent security review remains a release task |
| Human Control | Regressions PASS | A03, A04, A12, A13 | Exclusions and visible immutable constraints | Reserved flags and branch budgets; creator-only grants |
| Model Routing | Reviewed; regressions PASS | A08, A09 | Restart-safe strong call classification and local HTTP boundary | Qwen3-8B decision quality gate fails |
| Harness | Regressions PASS | A06, A07 | Explicit termination; distinct outcomes; no false read progress | Scientific synthesis is not independently scored |

## C. Confirmed defects and repairs

All tests named below passed in the final workspace run. A substantial fix has an executable regression; the rename also retains the existing architecture, CLI and migration/restart acceptance suite.

### A01 - Pre-retrieval capability deadlock (BLOCKER)

`anix-agent/src/lib.rs -> LeadAgent::run_step`: Atomic filtering selected only its required tool. With `tool:read_source` and no retrieval handles, no legal search prerequisite was available. `anix-integrations/src/test_lead.rs -> step_response_schema` then advertised unconstrained read parameters despite an empty argument domain.

Fix: `RetrievalProtocol::executable_tools` derives the current executable set from approved registered tools, required capability, pending state and actual handles. A required read may search only to resolve missing handles through its registered approved provider. A read without handles is omitted. Execution validates the same set and issued handle enum. Existing source handles do not require a new search. Explicit Source/Paper references supplied by the WorkNode contract receive scoped E1/E2 handles, a bounded title catalog and a runtime-bound source identity. The source adapter rejects stale bindings and unavailable supplied content; no retrieval batch is fabricated for a direct read. When prerequisites are unavailable, the Run closes and the WorkNode becomes Blocked with an actionable reason before inference; a Human can resume/revise the plan.

Regressions: `required_read_without_source_allows_prerequisite_resolution`, `existing_source_can_be_read_without_new_search`, `action_prerequisite_matrix_preserves_scope_and_argument_domains`, `read_without_source_handle_is_not_advertised`, `no_search_provider_returns_actionable_block_without_model_call`, `atomic_read_task_can_search_then_read_and_complete_through_real_runtime`, `supplied_source_is_read_without_search_through_the_lead_boundary`. The supplied-source regression uses actual Core/Lead/RecordedTool/SQLite boundaries, has no search provider, verifies inspected content and completion, and verifies that a forged model-supplied source ID is overwritten and later exclusions remove the handle. The final timeline demonstrates Search -> bounded Read S1/S2, chosen by the Lead.

### A02 - Inconsistent unresolved retrieval handling (HIGH)

`anix-agent/src/lib.rs -> resolve_pending_search_id`, `anix-domain/src/runtime.rs -> derive_retrieval_batches`, and `anix-evals/src/long_horizon.rs -> RefineSearchTool/RejectRetrievedSourcesTool/SearchEntitiesTool` disagreed about Pending versus PartiallyConsumed. The action set admitted refinement/rejection after one read, while ID binding/providers could reject it; superseding left a partial old batch unresolved. Atomic completion also needed an independent pending obligation check.

Fix: all unresolved paths recognize Pending and PartiallyConsumed; a new batch supersedes either. Plain search, Atomic completion and research termination cannot bypass an unresolved batch. Empty batches still have refinement/rejection options; no fabricated read enum is emitted. Child tool declarations are constrained to attainable prerequisites; completed Atomic work advertises completion without extra tool descriptors.

Regressions: `partially_consumed_batch_can_be_refined_or_rejected`, `persisted_retrieval_events_derive_pending_consumed_rejected_and_superseded`, `pending_retrieval_rejects_repeated_plain_search`, rendered-schema empty/single/multiple handle assertions, and the action prerequisite matrix. Branch/root-run scoping remains covered by retrieval and true-capability regressions.

### A03 - Source exclusion bypass in retrieval/context (HIGH)

`anix-evals/src/long_horizon.rs -> SearchEntitiesTool/ReadSourceTool` used source records without the hard source exclusion check used by canonical mutation services. Context compilation also selected excluded source/attached paper content.

Fix: the existing Core source policy is exposed through narrow `AgentApi::validate_source_access`, used for search candidate filtering and read validation; AgentApi handle observations filter inaccessible/archived/missing sources. Context rendering applies that policy and checks attached sources for Papers. The read boundary rechecks policy after advertisement, so a later exclusion cannot be bypassed.

Regressions: `source_access_checks_exclusion_archive_and_project_scope`, `human_constraints_are_pinned_in_immutable_context_and_excluded_papers_are_filtered`, and existing hard-source/authorization tests. No permissions, Gate policy or source exclusions were weakened.

### A04 - Human constraints missing from replayable context (HIGH)

`anix-core/src/runtime.rs -> compile_context_manifest/provider_context` packed entities and process notes without preserving active Human constraints in the snapshot.

Fix: active constraints and control version are reserved inside the context budget and stored as immutable ProcessMemory referenced by the snapshot. Later control changes do not change replayed context. Old snapshots and SQL storage layouts are unchanged.

Regression: `human_constraints_are_pinned_in_immutable_context_and_excluded_papers_are_filtered` verifies visible constraints, exclusion, old snapshot stability after a new directive, and restart equality.

### A05 - Completed child results lost to parent context (HIGH)

`anix-core/src/runtime.rs -> compile_context_manifest` collected current/ancestor ProcessMemory only. Parent decisions saw completion receipts but not child source observations. The pre-fix 664 parent turns 9/20/33 show that gap; Paper entries alone did not identify the completed source-read result.

Fix: completed contained/prerequisite work on the same branch contributes result memory. Composite context prioritizes bounded source-content observations; unfinished work and sibling branches are excluded. The harness labels actual consumed content separately from ordinary tool observations. Budget packing and immutable snapshots remain intact.

Regression: `parent_context_receives_completed_child_results_without_sibling_branch_memory`. Final real parent decisions contain completed child source-content notes; exact turn/count evidence is recorded in the final timeline. This fixes the observation gap but does not make the repeated strategy scientifically useful.

### A06 - Harness inferred completion from counts and discarded outcomes (HIGH)

`anix-evals/src/long_horizon.rs -> run_one` automatically marked a research root successful after 20 steps/5 tool calls without a FinishResearch decision. The explicit finish arm also ignored Completed/Inconclusive/Blocked distinctions. This could manufacture workflow success without a Lead result.

Fix: automatic count-based termination is removed. Completion requires explicit Lead termination, consumed source content, completed child work and Core output contracts. Blocked and Inconclusive remain distinct; inconclusive work is a valid explicit execution result but is not counted as completed research. Mandatory minimum-step language was removed from task instructions. The maximum budgets, integrity/recovery checks, known-seed gates and mixed/200 completion targets were retained. Historical completion percentages are not comparable to this changed acceptance path.

Regressions: `tool_and_step_counts_cannot_finish_research_without_lead_decision` crosses both old count thresholds and still fails to complete without a finish decision; `blocked_and_inconclusive_lead_outcomes_are_not_counted_as_completed`; `atomic_read_task_can_search_then_read_and_complete_through_real_runtime`.

### A07 - Invalid observations could satisfy output/progress accounting (HIGH)

`anix-core/src/lib.rs -> required_tool_output_exists/validate_work_completion/lead_progress_snapshot` treated a succeeded invocation with an output artifact as a valid result even if that artifact described an invalid reference or a stalled search. The harness also needed to distinguish source consumption from an arbitrary read observation.

Fix: a shared valid-output predicate excludes known invalid-reference/stall observations from acceptance and read-progress snapshots. The harness increments read progress only for actual consumption metadata with a valid reference. Execution steps and repeated source reads remain execution telemetry; they are not a scientific novelty/accuracy score.

Regression: `invalid_source_observation_cannot_satisfy_a_required_read` uses a real RecordedTool/SQLite output, rejects completion, and verifies zero successful source-read progress.

### A08 - Invocation identity and strong-call provenance could drift (HIGH)

`anix-core/src/lib.rs -> start_model_invocation` hardcoded local-test-lead provenance regardless of selected provider. Completion/history checks also allowed provider/tool identity changes; a strong classification could be lost on completion/interruption.

Fix: the selected provider is persisted, Lead calls carry a stable strong route, completion retains that route, and interrupted calls preserve strong classification. Provider/tool/work/instruction identities are checked across completion. RecordedRuntime's initial `pending` model/revision/runtime placeholders may be filled once by the adapter response; known identities cannot be retagged. Failed calls remain counted after restart.

Regressions: `invocation_identity_and_strong_route_cannot_be_retagged_at_completion`, `failed_strong_call_counts_across_restart_and_human_can_extend`, `recorded_model_failure_and_success_survive_restart`, and the full V2 runtime acceptance. Invalid retagging leaves state unchanged.

### A09 - Loopback locality could escape through HTTP behavior (HIGH)

`anix-integrations/src/test_lead.rs -> OpenAiCompatibleModelProvider::new` used the client's default redirect/proxy policy. Checking only the original loopback URL did not constrain redirect destinations or an ambient proxy.

Fix: the Test Lead client disables redirects and ambient proxies. The existing LocalOnlyTest provider still refuses external initial endpoints.

Regression: `loopback_provider_does_not_follow_redirects_to_another_endpoint` receives a real HTTP 302 and verifies that a separate destination listener receives no request. Local-only refusal tests also pass.

### A10 - Windows storage aliases escaped the file-tool prefix check (HIGH)

`anix-integrations/src/lib.rs -> ReadFileTool::execute` rejected only a literal lowercase `.rn/` prefix. Windows case/trailing-dot aliases and nested workspace storage were not covered.

Fix: normalized protected components and canonical resolved paths are checked before reading, including nested `.rn` storage. Existing traversal, symlink, alternate-stream, size and project boundary checks remain enforced.

Regression: `file_tool_rejects_storage_case_aliases_and_nested_databases`, `boundaries_and_hashes`, and `approved_read_tools_are_workspace_bound`.

### A11 - Credentials embedded in serialized trace text (MEDIUM)

`anix-integrations/src/test_lead.rs -> redact_trace_secrets` handled object keys and credential prefixes, but JSON contained inside message/completion strings and embedded token patterns needed redaction.

Fix: embedded JSON objects/arrays are sanitized, and recognized credential-bearing strings are redacted before bounded diagnostic persistence. Nonsecret trace inputs remain exact; secrets are deliberately not replayable.

Regression: `trace_payload_is_bounded_and_secrets_are_redacted` includes serialized password fields and embedded Bearer/API-key patterns. This is coverage of recognized credential patterns, not a claim to detect arbitrary secrets in prose.

### A12 - Direct Human execution bypassed pending Gate authorization (BLOCKER)

`anix-core/src/lib.rs -> CommandContext::validate/Core::execute` validated Human actor syntax without checking the project's creator. Gate resolution and Human controls already checked the creator, but a different Human could apply the pending proposal through direct execution or add knowledge through other mutation entrypoints. A negative regression reproduced this bypass before the fix.

Fix: the shared command validator applies the existing `authorize_project_human` policy to every Human mutation. New-project creation still works before a creator exists. Read projections and Lead/System contracts remain intact; no collaborator grants were invented.

Regression: `non_owner_cannot_bypass_a_pending_gate_through_direct_execution` verifies direct execution and knowledge mutation return PermissionDenied, state remains identical, the original Gate stays Pending, and its rightful owner can approve. Existing owner/control/Gate regressions pass. The experiment-proposal eval fixture now consistently uses its actual creator `eval` for pause/resume, covered by `experiment_fixture_uses_project_owner_for_human_controls_and_gate`; this corrects a fixture identity mismatch rather than granting new authority.

### A13 - Resolved Gate prematurely completed unfinished bound work (HIGH)

`anix-core/src/lib.rs -> resolve_waiting_run` marked a waiting Run and Task Completed after the last Gate resolution even when their bound WorkNode was still Running. The experiment-proposal regression reproduced a terminal parent error before any source search.

Fix: a resolved final Gate resumes a Run/Task bound to Ready or Running work and records RunResumed/TaskResumed. Completion remains governed by the WorkNode contract. Unbound legacy task completion semantics remain intact; pending Gates and unfinished child runs still prevent this transition.

Regression: `experiment_fixture_uses_project_owner_for_human_controls_and_gate` uses the real Core/Lead/RecordedTool/SQLite path with a fake deterministic provider, exercises one approved Gate and Human pause/resume, consumes sources, explicitly finishes research and verifies integrity/recovery. Fixture Human identities are explicit project creators, including the corrected acceptance checkpoint creator. No model-backed targeted capability campaign is claimed by this offline regression.

## D. Main blocker and contracts

| Contract | Result | Evidence |
|---|---|---|
| Pre-retrieval deadlock | FIXED | Final 664 searches before handles exist, then reads real S1/S2 |
| Action prerequisites | VERIFIED for configured retrieval protocol | Shared approved set, handle validation, unavailable-provider blocking and state matrix |
| WorkNode execution contracts | VERIFIED in regressions | Required output, pending obligations, history, missing-output rejection |
| Model-facing schemas | VERIFIED for current registered retrieval actions | Compact schema tests, exact final trace, valid dynamic handle enums |
| Context consistency | VERIFIED in affected regressions/final trace | Pinned goal/constraints, source observations, child results, branch isolation, snapshot restart |
| Research strategy quality | FAILED in final seed | Duplicate child objectives/actions; no explicit termination |

Generic Tool ports still rely on the registered adapter's declared input contract; there is no general-purpose argument-domain prover for every future third-party tool. Rust does not decide that source previews establish truth, force AutoRead, accept hypotheses automatically or prescribe a universal research pipeline. Model-generated `Investigate branching` remains a generic objective, although the actual question, WorkNode contract, source observations and completed results are supplied.

## E. Model-backed regression

Default Test Lead: Qwen3-8B Q4_K_M; RTX 2060 12 GB; CUDA 37-layer offload; context 8192; temperature 0.2; LocalOnlyTest; existing launcher with explicit GPU parameters. Weights were verified by bootstrap. Structured endpoint smoke returned `{"ready":true}`; it is not a research-readiness result. Model/runtime/profile/runner/report hashes are in [fingerprint](anix-model-fingerprint.json).

Several generalized-fix reruns of seed 664 were performed; no successful historical run was reused. Earlier new runs ended at the unchanged call budget, with 13/14/14/14 retrievals and 6 reads each. The final result after all source, authorization and Gate lifecycle repairs is below. The intermediate post-authorization run also failed (13 searches, 5 reads, 12 rejections, 40 calls); its report is preserved separately. For exact actions and evidence, see [final timeline](anix-seed-664-trace.md).

| Metric | Final seed 664 |
|---|---:|
| Result | **FAIL - budget_exceeded** |
| Lead calls / strong calls | 40 / 40 |
| Search calls | 13 |
| ReadSource calls | 6 |
| Consumed batches (any consumption) | 4 |
| Rejections / refinements | 11 / 0 |
| Real Branch entities | 2 |
| WorkNodes / checkpoints | 7 / 130 |
| Tool failures / invalid references | 0 / 0 |
| Structured failures / repairs | 0 / 0 |
| Largest snapshot / context p95 | 4080 / 4010 |
| Elapsed | 232117 ms |
| Integrity | PASS - 286 checks, 0 violations |
| Recovery | PASS |
| Explicit valid research termination | **NO** |

Final report: `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/20261009T142816099/branching-000664/report.json`. Diagnostic JSONL sits beside it and is registered as Artifact `8896a084-7cdf-404c-be16-ddd414b63c38`.

The final trace shows no impossible read action space or invalid handles. Actual read content and completed-child result notes are present. The Lead repeatedly creates generic duplicate evidence tasks, searches the same material, and sometimes selects `reject_retrieved_sources` while its reason says to read S1/S2. It never selects FinishResearch. Classification: **model decision/liveness failure after verified runtime repairs**. The record does not establish the model's hidden reasoning. Two Branch entities do not by themselves establish independent branch research/reconciliation; that remains a separate capability gate.

Seed 654: **NOT RUN after this audit's fixes** because 664 did not pass. Search/Read/auto-offload/final-state/integrity/recovery for 654 on this source version are therefore **UNVERIFIED**, not historical PASS. No larger Lead model was substituted, no seed-specific rule was added and no retry/stall budget was raised. Scenario seeds vary the corpus; the provider does not pass a deterministic generation seed to llama.cpp, so reruns are not claimed as bitwise replays.

## F. Validation campaigns

| Campaign | New model-backed execution | Completion | Integrity / recovery |
|---|---|---|---|
| Known seed 664 | RUN, final FAIL | No valid research termination | PASS / PASS |
| Known seed 654 | NOT RUN - 664 gate closed | Unverified | Unverified |
| Targeted capabilities | NOT RUN - both seeds required | Unverified | Offline capability assertions PASS |
| Fresh mixed 20 | NOT RUN - targeted gate closed | No new percentage claimed | No new 20/20 claim |
| 200-run campaign | NOT RUN - mixed gate closed | No new percentage claimed | No new 200/200 claim |

The exact thresholds remain at least 19/20 mixed completion and at least 95% 200-run completion, with 100% integrity/recovery/safety. The [research-quality protocol](anix-research-quality.md) is prepared separately; scientific quality evaluation has not been executed.

## G. Final checks

`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check.ps1`: **PASS**, exit 0. Toolchain: rustc/Cargo stable **1.96.0**, Windows/MSVC.

| Suite | Passed | Failed | Ignored |
|---|---:|---:|---:|
| anix-agent | 6 | 0 | 0 |
| anix-domain | 4 | 0 | 0 |
| anix-evals | 46 | 0 | 0 |
| anix-integrations | 16 | 0 | 0 |
| acceptance | 12 | 0 | 0 |
| runtime_v2 | 59 | 0 | 2 |
| CLI | 2 | 0 | 0 |
| **Workspace tests** | **145** | **0** | **2** |
| **Doc tests** | **3** | **0** | **0** |

Format: **PASS**. Compilation: **PASS**. Clippy with `-D warnings`: **PASS**. Build: **PASS** (workspace tests compile CLI/examples; the final Lead example was also built explicitly). [Machine-readable check result](anix-verification.json); full logs: `.runtime/anix-final-check.stdout.log` / `.runtime/anix-final-check.stderr.log`.

Ignored `crash_child_fixture` is invoked by the passing `crash_process_checkpoint_resume` subprocess regression and is not a standalone test. Ignored `real_llama_smoke_missing_bad_model_concurrency_shutdown` requires the explicit product-micro-model/CPU-runtime smoke environment; it was not separately run here. The actual Qwen3-8B CUDA Lead loading, structured calls, action execution and recovery were run separately above. Preliminary failures from implementation debugging, an active Windows example executable lock, and source/library rebuild concurrency were resolved; only the final stable-source logs/totals are reported as final verification.

## H. Remaining issues

- **BLOCKER B01: model-backed completion gate.** Final 664 repeats evidence tasks and misselects rejection before exhausting 40 calls. Impact: first pilot cannot meet the required gate; 654/targeted/mixed/200 remain unrun. Next step: use the frozen exact trace/schema/content boundary for an independent Lead decision-quality evaluation and improve general task/decision competence. Do not raise budgets, force reads, hardcode the seed or implement scientific strategy as a Rust sequence.
- **HIGH B02: genuine end-to-end research quality remains unverified.** Source/Claim provenance and output contracts pass offline acceptance, but final real synthesis, valid citations, contradiction treatment and unsupported findings are not independently scored. Impact: workflow metrics cannot establish scientific accuracy. Next step: complete the gated execution regressions and apply the prepared quality protocol to held-out, independently annotated research outputs.
- **MEDIUM M01: reserved autonomy flags.** `allow_branch_creation`, `allow_plan_revision` and `review_termination` are serialized but not enforced as additional gates. Current run/task/project/parent authorization and documented proposal/Gate policy are the implemented controls. Impact: clients must not present these flags as active guarantees. Next step: define explicit grants/termination review contracts through the existing Application/Gate path before exposing those controls; do not silently change defaults or bypass Gates.
- **MEDIUM M02: planning and tool contract scope.** The compact Lead step API exposes bounded child creation/branching/completion/termination and approved tools; it does not expose the entire Human plan-revision API or a general tool prerequisite language. Humans can revise contracts/plans and resume blocked work. Impact: autonomous recovery from an unsuitable research plan is limited. Next step: add policy-aware, provenance-preserving plan proposal contracts when the existing Lead quality gates justify expanding that surface.
- **MEDIUM M03: operational capabilities.** Branch-specific budgets, dedicated Batch scheduling, collaborator grants, native Apple bridge, external scientific source/PDF/experiment adapters and precise per-provider tokenizer/cost calibration remain incomplete or outside this validation. Impact: the present local synthetic workflow has narrower operational scope than a public research platform. Next step: prioritize demonstrated pilot needs, retain creator control and independently review new adapters.
- **LOW L01: historical naming/results.** Old reports contain RN engine names and obsolete metrics by design. Impact: older commands need the documented crate-name migration. Next step: use README/current ANIX documents for new work; preserve historical provenance instead of rewriting it.

No remaining reproduced state-corruption, Gate-bypass, source-exclusion-bypass or duplicate consequential-side-effect defect was found in the checked paths. This does not assert exhaustive safety for untested future integrations.

## I. Readiness by layer

| Layer | Assessment |
|---|---|
| ANIX Core infrastructure | Audited boundaries; final unit/integration/migration/compatibility checks PASS |
| Agent orchestration | Pre-retrieval/context defects repaired; real Lead workflow completion FAIL |
| Safety / Recovery | Final ordinary regressions and all new 664 runs PASS; larger gated campaigns not run |
| Research quality | UNVERIFIED; execution telemetry is not scientific accuracy |
| First real pilot | **NOT YET** - B01, closed downstream gates, B02 unverified |
| TUI | API/projection foundations exist; not validated for a TUI release; no TUI added |
| Public release | Not ready; operational, independent security and research-quality work remains |

## J. Final verdict and applicable completion status

**ANIX READY FOR FIRST REAL RESEARCH PILOT: NOT YET.** Naming, inventory, architectural mapping, the reproduced execution/context/security repairs, regression coverage, old-data compatibility, documentation and final standard checks are completed. The final seed 664 was rerun on the repaired source with exact tracing and failed at the unchanged call budget. Seed 654 and targeted/mixed/200 gates are explicitly unfulfilled. Those items are not marked done, and no production scientific accuracy is claimed. Remaining work is recorded in [roadmap](anix-roadmap.md).

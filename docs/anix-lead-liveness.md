# Lead decision-loop repair

## Observable baseline

The preserved trace is `20261009T142816099/branching-000664/lead-decision-trace.jsonl` in `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/`. Its report is FAIL at 40 calls: 13 searches, 6 reads, 11 rejections, 2 Branch entities, integrity/recovery PASS. All 40 decisions were reconstructed into `.runtime/anix-loop-before-reconstruction.json`: question, active WorkNode/branch, evidence, child receipts, plan contract, offered schema, rendered input, raw output, parsed action, validation/execution, next context and progress. Original traces remain unchanged.

## Confirmed causes

1. **Missing plan/result/readiness ledger.** Parent turns 7/18/27/36 contain 2/4/5/6 source-content notes respectively, but no child objective/status ledger or completion blockers. FinishResearch is already offered: the earlier failure does not prove its absence. Raw contracts lack an explanation of current satisfaction.
2. **Unconditional child creation.** Those turns repeat normalized objective `Investigate branching` with `read_source`, sometimes only changing titles. Each request previously created another WorkNode. The duplicate regression reproduced different IDs before repair.
3. **No permitted replan proposal.** Compact actions exposed creation/completion/branching/termination only. Human planning controls could not be delegated. The Lead lacked an explicit proposal path for an unsuitable strategy.
4. **Rejection resets transient search guarding.** The same query/source versions could create a new retrieval without new evidence after rejection. This differs from pending refinement, whose bounded guard remains.
5. **Directional instruction bias.** Repeated goals/system text directed Composite child creation on each call, including Atomic steps. Revised text requests decisions from the question, actual state and results without a prescribed scientific pipeline.
6. **Termination was driver-specific intent.** The driver closed runs and impersonated a Human to accept the root; other clients did not share acceptance. Inconclusive used Succeeded. Core now validates and persists the explicit outcome.

Raw turns 9/11/13 and 29/31/34 select rejection while text describes reading. The parser preserves that action: no observed parser remapping occurred. This is recorded without a Rust rationale classifier or forced read. No hidden-reasoning claim is made.

## General repairs

- ContextCompiler pins immutable decision state inside the existing budget: child IDs/objectives/status/outcomes, branch/head revision, contracts/dependencies/Gates, recorded attempts and distinct read/Claim/Finding stages. Human constraints retain priority; child source observations remain available. Old snapshots keep their original content.
- Same normalized objective/capability under one parent/branch returns existing work plus a persisted structured observation. Completed work is not scheduled again. No-op creation does not increment progress. Deliberate replication uses `repeat_work_node` with an explicit bounded reason in provenance.
- `propose_plan_revision` binds work/version and submits `ReviseWorkObjective` through an owner-approved Gate. Approval creates a new immutable PlanRevision, retaining old revisions. Lead gets no Human control capability.
- Search compares persisted fingerprints, candidate/source/paper versions and work revision. A duplicate returns an old invocation reference without a new batch/acceptable output. Changed query/material/approved objective remains executable. Stored artifacts support restart reconstruction.
- Atomic Inconclusive requires a recorded attempt, resolved retrieval/Gates and no competing run. Its unsatisfied lifecycle contract and distinct outcome survive restart.
- `AgentApi::finish_research` validates binding/identity, contracts, dependencies/Gates and active runs. Completed additionally requires consumed-source metadata or grounded Claim/Finding references. The finish enum reflects current readiness/evidence. Explicit Lead decisions remain necessary; counts never auto-complete research.

## Evidence and limits

Discovery, actual consumption, extracted Claim and produced Finding have different meanings. Search snippets are candidate triage. Read observations retain tool/source artifact provenance; receiving them does not independently establish scientific accuracy. No evidence promotion or forced reading was added.

Tests cover duplicates/justified repeats, immutable plan context, child results and branch isolation, Gate replan authorization, explicit research completion, rejected-search/no-progress feedback, Inconclusive persistence, CLI help/Human JSON and legacy migration/conflict protection. Existing budgets/retries/model size and Qwen3-1.7B trust boundaries are retained.

New metrics and exact before/after trace evidence are in the [audit](anix-audit.md) and [seed timeline](anix-seed-664-trace.md). Blocked/Inconclusive is not capability PASS. The long-horizon driver still needs independent branch-work/reconciliation validation beyond counting Branch records; dedicated true-capabilities tests already require those checks. Scientific terminal synthesis also needs separate evidence/citation/contradiction scoring.

## New no-op planning failure and repair

Intermediate run `20261009T160649627` created four owner-approved Gates/revisions for the exact same root objective. The regression `unchanged_plan_proposal_returns_existing_work_without_gate_or_revision` failed before repair. Core now validates binding/version/identity, detects normalized equality and returns the existing WorkNode with an idempotent receipt and PlanRevisionNoChange audit. No Gate or revision is created. The agent emits persisted existing_plan/no_progress feedback and finishes that step correctly. Actual changed objectives still require owner approval.

Final run `20261009T162401325` repeats four no-op proposals but creates zero Gates: the mechanism repair is confirmed, liveness is not. Turn 1 creates a Branch; no child/evidence/tool decisions follow. All five action validations pass; raw objectives are free strings in the schema, not forced current-goal constants. Turns 3-5 copy the existing-plan instruction into the reason while retaining propose_plan_revision. This is observable planning stagnation, with no inference about hidden reasoning or automatic blame of model size.

The unchanged no-progress guard stops after five calls. Report Blocked is a fixture guard result, not explicit FinishResearch. Cleanup leaves Failed lifecycle with null outcome. Integrity 27 checks and recovery PASS. Search/read/reject zero; two Branch records do not establish independent branch research. Seed 654, targeted/mixed20/200 and real pilot remain NOT RUN. Full loop reconstruction is `.runtime/anix-loop-after-reconstruction.json`; see the final timeline for raw trace location and artifact ID.

The separate direct pending-retrieval regression also reproduced unsafe completion before repair. Core now derives unresolved obligations from recorded output metadata for the work/descendants, regardless of tool naming, before completion or Atomic Inconclusive. The final workspace check passes 157 ordinary and 3 documentation tests, with 2 ignored fixture/smoke functions.

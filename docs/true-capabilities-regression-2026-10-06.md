# Authorization and true capability regression — 2026-10-06

## PermissionDenied root cause

The real Test Lead branching trace proposed `CreateWorkNode` with
`required_tool = research_report_retriever`. That tool was not registered for
the project; its tools were `read_artifact_range`, `read_source`, and
`search_sources`. The model request schema accepted an arbitrary tool name and
the composite Lead request did not clearly distinguish available child tools
from actions the Lead itself could execute. The invalid action was surfaced as
a permission failure. This was a capability/action-contract mismatch, not
evidence that the legitimate child-branch authorization policy should be
broadened.

The implementation now lists registered child tools in model requests,
constrains process actions per step, omits `CreateWorkNode` when no child tool
is available, and revalidates the constraint before applying the decision.
Search results also expose validated `source_ids`, distinct from question and
paper entity IDs. Authorization denials retain a structured, secret-free trace
with actor, action, resource, project/branch/parent scope, capabilities, and
reason.

## Security regression

| Check | Result |
|---|---|
| Unauthorized actor cannot create a child branch | PASS |
| Cross-project branch creation is denied | PASS |
| Wrong-parent checkpoint and stale-run mutations are rejected | PASS |
| Valid Lead-created child branch persists across restart | PASS |
| Stale Human Gate decision is rejected | PASS |
| Authorization bypass, admin test actor, or policy widening introduced | NO |

These acceptance checks passed in the workspace suite. No auth denial occurred
in the fresh model-backed capability runs.

## Focused branching regression

| Seed | Workflow/capability | Child branches | Completed WorkNodes per child | Reconciled | Restart/recovery | Integrity |
|---:|---|---:|---|---|---|---|
| 810 | completed / pass | 2 | 3, 3 | yes | pass | pass |
| 811 | completed / pass | 2 | 3, 3 | yes | pass | pass |
| 812 | completed / pass | 2 | 3, 3 | yes | pass | pass |
| 813 | completed / pass | 2 | 3, 3 | yes | pass | pass |
| 814 | completed / pass | 2 | 3, 3 | yes | pass | pass |

Capability **5/5**; integrity **5/5**; recovery **5/5**. Each result contains
two persisted real child Branches, isolated completed work, reconciled branch
results, and one restart.

## Focused long-context regression

Every run used a 46,954-token estimated corpus against an 8,192-token context
limit. Each run completed three retrievals, offloaded one source into an
Artifact, and successfully range-read that same Artifact. No execution-history
dump entered the prompts.

| Seed | Workflow/capability | Retrievals | Offloaded sources / range reads | Snapshots | Max prompt tokens | Restart/recovery | Integrity |
|---:|---|---:|---:|---:|---:|---|---|
| 820 | completed / pass | 3 | 1 / 1 | 17 | 3,143 | 1 / pass | pass |
| 821 | completed / pass | 3 | 1 / 1 | 16 | 3,126 | 1 / pass | pass |
| 822 | completed / pass | 3 | 1 / 1 | 16 | 3,119 | 1 / pass | pass |
| 823 | completed / pass | 3 | 1 / 1 | 16 | 3,125 | 1 / pass | pass |
| 824 | completed / pass | 3 | 1 / 1 | 16 | 3,130 | 1 / pass | pass |

Capability **5/5**; integrity **5/5**; recovery **5/5**. Every audit had zero
violations and every run had zero duplicate consequential operations.

Detailed per-run JSON is in `eval-results/true-capabilities/`. The focused
summary is [focused-10.md](../eval-results/true-capabilities/focused-10.md)
with its machine-readable companion [focused-10.json](../eval-results/true-capabilities/focused-10.json).

## Mixed campaign

The valid mixed variation batch ran all 20 seeds (650–669) across the 11
scenario kinds. It used the unchanged local Test Lead model/profile and the
8,192-token context limit.

| Measure | Result |
|---|---:|
| Status `completed` | 15/20 (75%) |
| Wall-time budget exceeded | 3/20 |
| Capability failed | 2/20 |
| Core integrity audits | 20/20, zero violations |
| Recovery preserved | 20/20 |
| Branch capability | 2/2 runs created two branches; one run exceeded its wall-time budget |
| Long-context retrieval/offload capability | 0/2; both runs had one retrieval, zero offloaded sources, and four tool failures |
| Total tool failures | 9 (eight invalid long-context reads, one injected failure) |
| Injected one-shot tool-failure recovery | seed 660: 1 failure, workflow completed, recovery passed |
| Gate records in mixed batch | 0; stale-gate rejection passed in acceptance tests |

The three timeouts were branching seed 653, sparse-evidence seed 655, and
experiment-proposal seed 656. Long-context seeds 654 and 665 failed because
the model did not complete the required retrieval and source offload path.
Branching seed 664 completed with two persisted branches; seed 653 created two
branches but exceeded its wall-time budget before completion. All 20 reports
passed integrity and recovery.

The status field counts 15 completed workflows, but basic-evidence seed 650
also records an unresolved truncated structured-output recovery error. Thus
only 14 of the 20 reports are both `completed` and free of a recorded error.
The injected tool-failure scenario on seed 660 recorded its expected error,
recovered, and completed.

**Readiness: NOT YET.** The focused suites passed, and mixed integrity/recovery
are 100%, but mixed workflow completion is 75%, long-context capability is
0/2, there are three wall-time overruns, and seed 650 has an unresolved model
output failure. Do not start the 200-run campaign.

The first launch attempt was stopped before it could produce a campaign result
because the local model server had not been running. Its partial run folders
are excluded. The valid campaign was restarted against the same Test Lead
profile after confirming the server reported `n_ctx = 8192`.

## Workspace checks

`scripts/check.ps1` passed after the current source changes: **98 passed, 2
ignored, 2 doc tests passed**, with format, check, and clippy stages passing.
The full source now includes a dedicated injected tool-failure scenario and
permanent tests for capability assertions.

The workspace has no Git metadata, so a commit hash is unavailable. The
200-run campaign has not been started.

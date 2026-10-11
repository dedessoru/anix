# True branching and long-context validation

> **Current regression:** The fresh 2026-10-06 authorization and focused
> capability results are in [the updated regression report](true-capabilities-regression-2026-10-06.md).
> The focused tables below match that fresh regression; the mixed table records
> its completed 20-run batch.

## Environment and safety

- Test Lead remained the configured local Qwen3-4B Q4_K_M through llama.cpp,
  with an 8,192-token context. The product micro-model, routing, and Test Lead
  profile were not changed.
- The repository has no Git metadata, so there is no revision hash to record.
- `scripts/check.ps1` passes after the implementation and source-ID fixture
  correction: 98 tests passed, 2 ignored, 2 doc tests passed; formatting,
  check, and clippy stages passed.
- The 200-run campaign has not been started.

## Branching focused runs

| Seed | Workflow | Child branches | Work per child | Reconciled | Restart recovery | Integrity |
|---:|---|---:|---:|---|---|---|
| 810 | completed | 2 | 3 | yes | pass | pass |
| 811 | completed | 2 | 3 | yes | pass | pass |
| 812 | completed | 2 | 3 | yes | pass | pass |
| 813 | completed | 2 | 3 | yes | pass | pass |
| 814 | completed | 2 | 3 | yes | pass | pass |

Capability: **5/5**. Reports: `eval-results/true-capabilities/branching-810.json`
through `branching-814.json`.

## Long-context focused runs

The synthetic corpus is 46,954 estimated tokens against an 8,192-token context.
Each run performed 3 retrieval calls and one offloaded source read. All runs
restarted, and the persisted-state recovery and Core integrity audits passed.

| Seed | Workflow/capability | Retrieval | Offloaded read | Successful range reads | Snapshots | Largest prompt tokens | Recovery / integrity |
|---:|---|---:|---:|---:|---:|---:|---|
| 820 | completed / pass | 3 | 1 | 1 | 17 | 3,143 | pass / pass |
| 821 | completed / pass | 3 | 1 | 1 | 16 | 3,126 | pass / pass |
| 822 | completed / pass | 3 | 1 | 1 | 16 | 3,119 | pass / pass |
| 823 | completed / pass | 3 | 1 | 1 | 16 | 3,125 | pass / pass |
| 824 | completed / pass | 3 | 1 | 1 | 16 | 3,130 | pass / pass |

Capability: **5/5**. Every seed completed with recovery and Core integrity
passing. Reports are
`eval-results/true-capabilities/long-context-820.json` through
`long-context-824.json`.

## Mixed batch

Eligible after the focused gates (branching 5/5, long-context 5/5, with
integrity and recovery passing 100%). The corrected 20-run variation campaign
with seed 650 completed. Reports are under
`eval-results/long-horizon/runs/` in separate timestamped folders, named by
scenario and seeds `000650` through `000669`.

| Seed | Scenario | Status | Branches | Retrieval / offload | Integrity / recovery |
|---:|---|---|---:|---:|---|
| 650 | basic-evidence | completed | 1 | 5 / 0 | pass / pass |
| 651 | contradictory-evidence | completed | 1 | 5 / 0 | pass / pass |
| 652 | duplicate-sources | completed | 1 | 5 / 0 | pass / pass |
| 653 | branching | budget_exceeded | 2 | 4 / 0 | pass / pass |
| 654 | long-context | capability_failed | 1 | 1 / 0 | pass / pass |
| 655 | sparse-evidence | budget_exceeded | 1 | 3 / 0 | pass / pass |
| 656 | experiment-proposal | budget_exceeded | 1 | 4 / 0 | pass / pass |
| 657 | failed-experiment | completed | 1 | 5 / 0 | pass / pass |
| 658 | revision | completed | 1 | 5 / 0 | pass / pass |
| 659 | interrupted-research | completed | 1 | 5 / 0 | pass / pass |
| 660 | tool-failure | completed | 1 | 6 / 0 | pass / pass |
| 661 | basic-evidence | completed | 1 | 5 / 0 | pass / pass |
| 662 | contradictory-evidence | completed | 1 | 5 / 0 | pass / pass |
| 663 | duplicate-sources | completed | 1 | 5 / 0 | pass / pass |
| 664 | branching | completed | 2 | 5 / 0 | pass / pass |
| 665 | long-context | capability_failed | 1 | 1 / 0 | pass / pass |
| 666 | sparse-evidence | completed | 1 | 5 / 0 | pass / pass |
| 667 | experiment-proposal | completed | 1 | 5 / 0 | pass / pass |
| 668 | failed-experiment | completed | 1 | 5 / 0 | pass / pass |
| 669 | revision | completed | 1 | 5 / 0 | pass / pass |

Overall: **15/20 completed (75%)**, **20/20 Core integrity**, and **20/20
recovery**. Seed 650 is counted as completed by its status field but records an
unresolved truncated structured-output recovery error, leaving **14/20
completed without a recorded error**. All reports record zero gates, so gate
behavior was not exercised by this batch; stale-gate denial remains covered by
acceptance tests. No authorization/permission errors occurred.

Both long-context runs failed their capability assertion. Seeds 654 and 665
each made one retrieval, had zero offloaded sources, and recorded four failed
tool calls. Branching seed 664 completed with two child branches; seed 653
also created two branches but exceeded its wall-time budget. Thus branching
created two branches in **2/2** mixed runs, while long-context passed **0/2**.

The dedicated injected tool-failure scenario on seed 660 recorded one expected
failure and completed with recovery preserved. There were nine tool failures
in total: that injected failure plus eight failed reads across the two
long-context runs.

## Readiness

Focused regressions meet the thresholds (branching 5/5; long-context 5/5), and
the mixed batch has completed with 100% integrity/recovery. Readiness remains
**NOT YET**: workflow completion is 75% (14/20 without recorded errors), both
mixed long-context runs failed, three scenarios exceeded their wall-time
budgets, and seed 650 recorded an unresolved structured-output failure. The
200-run campaign has not been started.

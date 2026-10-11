# Long-horizon campaign protocol

This iteration's runner is `rn-evals::long_horizon` and is exposed as:

```powershell
$env:RN_TEST_LEAD_CONFIG = (Resolve-Path 'config/local-test-lead-4b.toml').Path
cargo run -p rn-cli -- --workspace . eval research long-horizon --count 10 --seed 410 --stage initial
```

The runner uses `LeadAgent::run_step`, which performs one typed decision call and persists its ContextSnapshot, ModelInvocation, Run, and checkpoint. `CreateWorkNode` creates an Atomic child whose capability names the single required tool. The approved local tools are Core FTS/BM25 `search_sources` and local `read_source`; `RecordedTool` persists invocation and output artifacts. A child can transition to Succeeded only after its required tool has a successful invocation and output artifact. The Lead cannot mutate SQLite or approve a Gate.

Each research has hard bounds of 100 meaningful steps, 40 Lead calls, and 60 tool calls in the intended protocol. Completion requires at least 20 meaningful steps, five successful tool calls, one completed Atomic child, and a passing Core audit. Four consecutive failed/no-progress decisions end as Blocked. Runs that do not meet completion conditions are reported as `blocked` or `failed_safely`; they are not counted as completed.

The ten deterministic pack names currently map to the existing synthetic scenario catalog:

| Run | Scenario | Intended stress |
| ---: | --- | --- |
| 1 | `basic-evidence` | Retrieve and inspect evidence across documents |
| 2 | `contradictory-evidence` | Reassess after conflicting evidence |
| 3 | `duplicate-sources` | Avoid duplicate claims and work |
| 4 | `branching` | Create and compare research branches |
| 5 | `long-context` | Keep snapshots bounded as source count grows |
| 6 | `sparse-evidence` | Stop with uncertainty when evidence is insufficient |
| 7 | `experiment-proposal` | Exercise a safe proposal and human gate |
| 8 | `failed-experiment` | Preserve an experiment failure without inventing a finding |
| 9 | `revision` | Reassess in light of revised evidence |
| 10 | `interrupted-research` | Restart from a persisted checkpoint |

Machine-readable per-research reports are written under `eval-results/long-horizon/`. The aggregate first-pass data is `primary-10.json`; same-seed regression replaces this summary after review. A 20-run variation is rejected unless the current 10-run summary has ten intact audits and at least 80% completed researches.

## Known coverage gaps before interpreting results

The scripted acceptance test exercises a multi-dozen meaningful-step loop, recorded search tools, process memory, child WorkNode acceptance, checkpoints, and final completion. The local-model campaign is required to validate actual Test Lead behavior. Fault injection for tool timeout, artifact-read failure, restart recovery during step 10, stale gates, and cancellation remains separate follow-up coverage; the scripted test is not evidence for those fault paths. The micro-model CandidateExtraction route is not yet wired into this long-horizon runner. These gaps prevent a positive stress-campaign verdict until they are implemented and measured.

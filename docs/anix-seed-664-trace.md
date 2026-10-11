# Seed 664: final observable decision loop

Report: `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/20261009T162401325/branching-000664/report.json`. Trace Artifact: `74bb8313-7a30-46c4-adb9-df6feb79eeaf`.

| Turn | Parsed action | Validation / execution | State progress |
|---:|---|---|---|
| 1 | `create_branch` | contracts valid; completed | second Branch record |
| 2 | `propose_plan_revision` | contracts valid; completed | existing_plan; no_progress; no Gate/revision |
| 3 | `propose_plan_revision` | contracts valid; completed | existing_plan; no_progress; no Gate/revision |
| 4 | `propose_plan_revision` | contracts valid; completed | existing_plan; no_progress; no Gate/revision |
| 5 | `propose_plan_revision` | contracts valid; completed | existing_plan; no_progress; no Gate/revision |

Turns 2-5 repeat the entire unchanged objective. On turns 3-5 the raw reason repeats the Runtime instruction telling the Lead to execute evidence work or materially change the objective. The action remains propose_plan_revision: the parser did not remap it. Schemas permit a free objective; they do not force the current objective. The decision ledger and concrete child-creation action are present, but the Lead does not choose a child or a tool.

The unchanged four-no-progress guard stops the run. Report status is Blocked; fixture cleanup leaves root WorkStatus Failed with outcome null. No explicit FinishResearch decision occurred. Two Branch records exist, but independent research in both branches was not performed. Zero reads means source utilization and scientific completion fail. Integrity and restart Recovery pass; this is not capability PASS.

Full loop reconstruction: `.runtime/anix-loop-after-reconstruction.json`. Row work status is the final serialized fixture state; progress_before/progress_after and rendered input preserve the actual state at each decision. Original JSONL and all earlier reports remain unchanged. The scenario seed varies the corpus, not a deterministic generation seed.

# ANIX Research Engine v1 — acceptance checkpoint

2026-10-10. **NOT YET ACCEPTED. READY FOR FIRST REAL RESEARCH PILOT: NO.**

This is a session checkpoint, not a completed engineering mission. Machine state: `.runtime/anix-v1-mission-state.json`; detailed history: [mission](anix-v1-mission.md); exact continuation: [resume](anix-v1-resume.md). No automatic resume is configured.

## Architecture and root causes

The existing seven-crate Core/Agent/SQLite/Application API architecture is retained. No UI, new framework, migration or product micro-model authority change was introduced in this session.

- `anix-core/src/runtime.rs`, context compilation: completed-work results crowded the pinned question out of the 4096 budget. Constant preview budgeting and reservation of pinned entities/Human constraints fix overflow. Under pressure, completed objective previews shrink and the newest readable result is retained; full ProcessMemory records remain unchanged. A stronger regression also reproduced that reference-only results were unavailable to a Lead without memory-ID read actions.
- `anix-integrations/src/test_lead.rs`, branch schema: unfinished branch handles were advertised as reviewable results. Only terminal Completed/Inconclusive branch results are now offered. No authorization was relaxed.
- `anix-evals/src/long_horizon.rs`, branch dispatch: a first valid branch switch after three rejected actions triggered the idle guard before the new scope could execute. First dispatch to a new executable scope resets execution idleness without research-progress credit; revisiting scopes cannot repeatedly reset the guard. Negative switching/deduplication tests preserve existing thresholds.

Each defect has a before-failure/after-pass regression. Exact logs and backups are in the mission record. Source, runner and standard-check hashes: `.runtime/anix-v1-session13-manifest.json`.

## Iteration history

Resumed the existing iteration10 checkpoint, then performed iterations11–13. Iteration11 restored pinned-question capacity; iteration12 corrected branch review/first dispatch; iteration13 reproduced and corrected lost substantive results under pressure. Raw model outputs were preserved; no seed-specific rules, forced reading/outcomes, raised budgets, hidden failures or altered corpus were used.

## Tests

| Check | Current evidence |
|---|---|
| Workspace tests | PASS: 177 ordinary tests, 2 ignored, zero failures |
| Doc tests | PASS: 3 |
| Format / compilation / Clippy | PASS: `scripts/check.ps1`, iteration13 logs |
| Build | PASS: fresh `lead_runtime_seed` runner |
| SQLite integration / Human Gates / authorization / integrity / recovery | PASS in standard deterministic tests; fresh model-run integrity/recovery results below |
| Main/historical database row compatibility audit | Historical PASS; independent complete row comparison NOT RUN in this session; no migration introduced |
| CLI | Help and standard CLI integration tests PASS; full model-backed arbitrary-project user acceptance NOT RUN |

Logs: `.runtime/anix-v1-session13-check.stdout.log`, `.stderr.log`. PowerShell native stderr redirection reports a nonzero outer shell status despite complete successful command/test logs; individual test and compiler results are recorded rather than hiding this distinction.

## Model-backed evaluation

Local llama.cpp b11307 CUDA; RTX2060 12GB; ctx8192; temperature0.2; GPU37; timeout900. Product Qwen3-1.7B remains unchanged. Corpus seeds are not fixed sampling seeds. This is a controlled workflow comparison, not a universal model ranking.

| Source/profile/scenario | Result |
|---|---|
| Iteration12 8B seed664, `20261010T075935935` | Inconclusive: 24calls,8reads,4consumed batches,2evidence branches,reconciled,integrity163/recovery PASS; not Completed acceptance |
| Iteration13 8B seed654, `20261010T081048725` | Blocked: 14calls,4duplicates,integrity92/recovery PASS; final actual input contains readable completed results and offers Completed |
| Iteration13 14B seed654, `20261010T081319661` | Completed: 6calls,2reads,1consumed/offloaded batch,integrity44/recovery PASS; explicit source-backed summary references Local paper18 |
| Iteration13 14B seed664, `20261010T081513351` | Blocked: 9calls,1evidence branch with reconciliation,integrity58/recovery PASS; branch Inconclusive followed by explicit root Blocked; missing second evidence branch |
| Iteration13 8B seed664 | NOT RUN on final iteration13 source |
| Targeted / Mixed20 / 200-run | NOT RUN on this source; known-seed gate unmet |

Reports and JSONL traces remain under `.runtime/lead-runtime-cleanup-seeds/<scenario>-<seed>/eval-results/long-horizon/runs/<run>/`. Full comparison manifest: `.runtime/anix-v1-session13-full-model-comparison.json`. The initial session11 CPU-only launch was interrupted to use the CUDA build and is retained as an infrastructure failure, not model-capability evidence.

14B demonstrates successful execution of654 where8B repeats completed work. It still does not satisfy664: it demands a more likely/unique mechanism although the evidence describes possible pathways, and does not execute the required second branch. Previous paired final-turn diagnostics also chose Inconclusive; neither those replays nor these individual workflows prove universal model incapacity. Further diagnosis must distinguish scientific question scope, branch strategy and capability without changing outcomes merely to achieve PASS.

## Research quality

The654 14B summary matches the synthetic source's seven-percent pilot endpoint change and retains bounded-sample/calibration/uncertainty language. This is a fixture sanity check, not held-out scientific validation. Independent evidence/citation correctness, contradiction handling, unsupported finding rate, completeness, uncertainty calibration and reproducibility evaluation: **NOT RUN**. Stored provenance/traces support replay but do not establish scientific truth. Long-context offload receipts alone do not prove targeted range-read capability for arbitrary corpora.

## Remaining limitations and operational readiness

- BLOCKER: known-seed gate3 remains unmet; no selected profile has passed both seeds. Later reliability/quality gates remain unopened.
- HIGH: full user-facing CLI/API project-to-result path, diverse methodology acceptance, held-out scientific quality, and large-corpus targeted source range access need actual acceptance.
- MEDIUM: scope and termination calibration varies by model; recorded corpus seeds do not fix sampling.
- LOW: historical documents retain earlier checkpoints; use machine state/latest resume first.

Core integrity: deterministic PASS. Agent orchestration: focused deterministic and some real branch evidence, no general acceptance. Research reliability and quality: unaccepted. CLI/API: existing API/integration checks pass, complete product path unaccepted. Future TUI/GUI must remain clients of the shared Application API.

Next work is specified in the resume document. No ordinary failure is claimed as successful research, and no further work is claimed to run automatically after this session.

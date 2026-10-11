# Pre-pilot hardening — 2026-10-07

## Lead decision trace and retrieval observation — 2026-10-09

Added opt-in LeadDecision traces with exact rendered messages and per-call schemas, raw completions, validation/execution outcomes, progress snapshots, and links to ModelInvocation, ContextSnapshot, WorkNode, Branch and ToolInvocation records. Diagnostic payloads are bounded and redacted; the JSONL trace is also registered as a Core Artifact. See [lead-decision-trace.md](lead-decision-trace.md) for baseline and post-change timelines, artifacts, fingerprints and root-cause evidence.

The pre-change trace confirmed a retrieval observation gap: candidate handles were available, but previews and handle-to-source mapping were absent from the model-facing observation. Search observation v2 now supplies mapped titles, bounded previews, unread state, objectives, consumed sources and conditional next actions. It does not force reads.

The controlled Qwen3-8B seed 664 rerun still **failed**. It persisted two branches, but after creating an atomic WorkNode requiring `read_source`, the per-call schema allowed only `read_source` despite no retrieval batch or source handles. The model attempted `S1` four times; each was rejected as an invalid source reference. Reported status is blocked, consumed reads 0, integrity PASS and recovery PASS. This reveals a remaining pre-retrieval action-schema/protocol blocker. Seed 654 and later campaigns were not run under the todo gate; first-pilot readiness remains **NO**. Historical 8B/14B metrics above are unchanged and are not replaced by this new run.

## Baseline

Before source changes, `scripts/check.ps1` passed: **111 tests passed, 2 ignored, and 2 doc tests passed**. The ignored tests require a real model/llama.cpp binary or are subprocess fixtures. The workspace has no `.git` directory, so no commit or diff base is available; this was not treated as a blocker.

The configured Test Lead profiles are `config/local-test-lead.toml` (loopback `127.0.0.1:8082`, 8,192-token context) and `config/local-test-lead-4b.toml` (loopback `127.0.0.1:8083`, currently configured for 16,384 tokens). The last comparable mixed campaign used `local-test-lead-4b` with an 8,192-token context. Routing policy remains unchanged: only measured, allowlisted low-risk micro tasks may use the local micro route; consequential and complex reasoning routes to the strong model. Human control remains creator-only, with Batch using the existing autonomous proposal policy. See [human-control-autonomy.md](human-control-autonomy.md).

The previous mixed campaign (seeds 650–669) completed **15/20** workflows; 20/20 integrity audits and 20/20 recovery checks passed. Only 14/20 were completed without a recorded error. There were three time-budget overruns, two long-context capability failures, and no Gate actions. The previous run therefore failed the 19/20 completion target.

## Post-retrieval changes

- Search output now contains a typed observation with query, bounded `S1…` handles, typed `SourceId`s, and source titles.
- The Test Lead harness resolves handles and rejects any source reference that was not returned through its retrieval path. Wrong entity IDs receive a structured `invalid_reference` observation before source access.
- The harness tracks unconsumed results and deterministic normalized-query fingerprints. Two searches without consumption are allowed; the next search returns `search_stall_detected`. A distinct refined query is allowed within that bound. A stall terminates the harness case explicitly.
- The harness progress counter no longer increments for a model call, a search, or a generic tool invocation. It counts source reads and persisted work/process changes.
- Structured-output errors distinguish truncated, malformed JSON, and schema-invalid responses. Failure records include the initial finish reason and byte length, failure class, repair attempt/result, full-retry status, and final outcome. Repair remains bounded.
- Strong-call budget is reserved before the local Test Lead provider boundary. Failed invocations remain persisted and count across restart. Human budget changes use the existing versioned API; AgentApi has no budget mutation capability. Branch-specific budgets remain a documented future improvement.
- The `experiment-proposal` mixed scenario now contains a deterministic Pause/Resume and Lead-proposal → Human Gate → approval path. Its capability assertion fails if no Gate is recorded.

## Post-Retrieval State Machine

Retrieval state is derived from successful persisted tool invocations and their output artifact metadata. Each batch stores its search invocation ID, query, bounded handles and resolved SourceIds, consumed handles, originating Run and WorkNode, and Branch. The state is reconstructed from SQLite artifacts after a Core restart.

The state transitions are:

```text
successful SearchSources → Pending
ReadSource on a returned, unconsumed handle → PartiallyConsumed or Consumed
RejectRetrievedSources on the current search ID → Rejected
justified RefineSearch on the current search ID → prior Pending batch Superseded; new batch Pending
```

Pending state is scoped to the current root Run and Branch, with WorkNode recorded on each batch. Another Branch remains independent. While Pending, the Lead schema exposes only `read_source`, `reject_retrieved_sources`, and `refine_search`; the ordinary search name is absent from the `use_tool.name` enum and tool list. Runtime validation independently rejects a manually formed ordinary Search with `PendingRetrievalMustBeResolved`. The Lead chooses the source to read, or decides to reject/refine; runtime does not auto-read.

Read progress is persisted as a consumption event linked to the batch ID and handle. One successful read can return the Lead to the normal action space while the batch remains auditable as `PartiallyConsumed`; a fully consumed batch is `Consumed`. Search alone does not resolve a pending batch. The existing stall guard remains a fallback for repeated or no-progress queries.

## Runtime-bound retrieval identity

Models choose intent and semantic arguments; Runtime supplies deterministic execution identity. The model-facing Refine action now contains only `reason` and `query`, while Reject contains only `reason`. Lead Runtime resolves exactly one pending batch in the current Run/WorkNode/Branch scope and injects its typed `ToolInvocationId` before the approved tool executes. The recorded invocation input and output metadata retain the bound ID for audit. A missing pending batch returns `NoPendingRetrieval`; multiple candidates return `AmbiguousPendingRetrieval` rather than selecting one.

Runtime-bound values include the current search invocation, run, project, and work context. Model-selected values include source handle, query, reason, and whether to read, refine, or reject. `ReadSource` remains handle-based so the Lead chooses which source to inspect; Runtime does not auto-read or auto-refine. Branch filtering and persisted invocation artifacts make the same binding recoverable after restart.

## Known failing seeds

| Seed | Before | Current model-backed result |
|---:|---|---|
| 654 | `long-context` capability failed; one search, no offloaded source read, four failed tool calls; integrity and recovery passed. | Runtime-bound pass: 1 Search, 2 Refine, 1 successful refinement, 0 refinement validation failures, 0 Read/Reject, 0 sources consumed, 2 batches created, 1 superseded, 1 stall, 4 Lead calls; `capability_failed`; integrity/recovery PASS. The model refined once, then did not consume a source before the stall bound. |
| 664 | `branching` completed with two branches but made five searches and no offloaded source read; integrity and recovery passed. | Runtime-bound pass: 1 Search, 4 Refine, 0 successful refinements, 4 validation failures (missing `reason`), 0 Read/Reject, 0 sources consumed, 1 batch created, 0 superseded/stalls, 7 Lead calls; `blocked`; two branches; integrity/recovery PASS. |

### Runtime used for the new runs

- Test Lead: Qwen3-4B Q4_K_M, `local-test-lead-4b`, pinned model SHA-256 `7485fe6f11af29433bc51cab58009521f205840f5b4ae3a32fa7f92e8534fdf5`.
- Backend: local llama.cpp b11307, loopback `http://127.0.0.1:8083/v1`, CPU inference, 6 threads, `--n-gpu-layers 0`; no GPU layers were offloaded. Context 8,192, temperature 0.2, request timeout 900 seconds.
- The runtime-bound pass used 8,192 context, temperature 0.2, CPU inference, 6 threads, `--n-gpu-layers 0`, and a 900-second request timeout. Lead latency p50/p95 was 41.9/64.3 seconds for 654 and 35.7/39.6 seconds for 664. Runtime remained local-only; both reports record zero micro calls.
- Runtime health passed. Peak observed server working set was about 5.6 GiB RAM; VRAM use was zero because inference ran on CPU.

The protocol acceptance gate passed before these model runs: the pending schema's tool name enum was restricted to `read_source`, `reject_retrieved_sources`, and `refine_search`, and a manually returned `search_sources` was rejected with `PendingRetrievalMustBeResolved`. A SQLite reopen regression reconstructed the pending batch and confirmed a different Branch sees no such batch. Runtime injected the current typed search ID into Refine and Reject invocations; no missing-ID error occurred. Qwen3-4B still failed to consume sources: seed 654 refined once then stalled, while seed 664 omitted the required semantic `reason` on four Refine attempts.

## Targeted regression

| Check | Result |
|---|---|
| Pending schema hides ordinary Search; manually formed Search is rejected | PASS, schema and runtime regressions |
| Pending retrieval survives database reopen and remains Branch-scoped | PASS, SQLite reopen regression |
| Seed 654 long-context model run | FAIL (`capability_failed`); one successful refinement, no read/consumption, then stall; integrity/recovery PASS |
| Seed 664 branching model run | FAIL (`blocked`); two branches, four Refine validation failures for missing `reason`, no read/consumption; integrity/recovery PASS |
| Structured-output repair/parser regressions | PASS, integration tests |
| Strong budget zero prevents provider call | PASS |
| Failed strong call counts across restart | PASS |
| Human can extend the budget and continue | PASS |
| Human Gate lifecycle | Existing acceptance coverage passes; deterministic mixed fixture is implemented, not model-backed in this validation |

After the state-machine and acceptance regressions, `scripts/check.ps1` passed: **116 tests passed, 2 ignored, and 3 doc tests passed**; formatting, compilation, and Clippy stages passed. Historical focused suites and prior aggregate numbers are not counted as current-version results.

## Qwen3-4B CPU vs Qwen3-8B GPU Test Lead A/B

This is a quality/capability A/B, not a hardware performance A/B. Both exact seeds, scenario definitions, context size, temperature, tool schemas, and runtime policies were held constant. No Core or Test Lead logic was changed for the 8B pass. The project has no `.git` directory, so a commit SHA for the RN working tree cannot be supplied; `scripts/check.ps1` passed immediately before the model run.

### 8B model and runtime

- Model: [Qwen3-8B Q4_K_M](../models/qwen3-8b/Qwen3-8B-Q4_K_M.gguf); profile [qwen3-8b.toml](../models/profiles/qwen3-8b.toml), revision `7c41481f57cb95916b40956ab2f0b139b296d974`, SHA-256 `d98cdcbd03e17ce47681435b5150e34c1417f50b5c0019dd560e4882c5745785`, size **5,027,783,488 bytes**.
- Runtime: llama.cpp b11307, commit `4453b535f`, built with CUDA 13.4 for RTX 2060 architecture sm_75. Loopback endpoint `127.0.0.1:8083`, CPU thread count 6, parallelism 1, context 8,192, temperature 0.2, timeout 900 seconds, `n_gpu_layers=-1`, Flash Attention on.
- CUDA was confirmed in startup logs. All **37/37 layers** were offloaded. The CUDA allocations were 4,455.34 MiB model, 1,152 MiB KV cache, and 104.01 MiB compute buffer. `nvidia-smi` showed about **7.2 GiB total GPU memory in use** while loaded, including desktop processes. RAM working set was about 5.1 GB shortly after load and reached about 9.1 GB after the seed runs. Health and JSON-schema smoke passed. The server became healthy within the 15-second polling interval. Across 27 logged timings, median prompt processing was **1,354.59 tokens/s** and median generation was **43.28 tokens/s**.
- The runs were local-only: provider locality `local`, zero micro-model calls, and only the loopback llama.cpp server was configured.

### Seed results

| Metric | 4B CPU, 654 | 8B GPU, 654 | 4B CPU, 664 | 8B GPU, 664 |
|---|---:|---:|---:|---:|
| Status | `capability_failed` | `capability_failed` | `blocked` | `blocked` |
| Lead calls | 4 | 12 | 7 | 7 |
| Searches | 1 | 1 | 1 | 1 |
| Refines / successful | 2 / 1 | 1 / 0 | 4 / 0 | 3 / 0 |
| Reads / consumed batches | 0 / 0 | 3 / 1 | 0 / 0 | 1 / 0 |
| Refinement validation failures | 0 | 1 | 4 | 3 |
| Structured repairs | 0 | 5 / 5 | 0 | 2 / 2 |
| Stalls | 1 | 0 | 0 | 0 |
| Branches | 1 | 1 | 2 | 2 |
| Integrity / recovery | PASS / PASS | PASS / PASS | PASS / PASS | PASS / PASS |

Seed 654 improved: 8B read three times and consumed one retrieval batch, but two reads repeated an invalid handle, the refinement failed validation, and no detailed source was offloaded to an Artifact. The long-context assertion therefore failed. Seed 664 remained blocked: both branches existed, but the read attempt had invalid arguments and all three refinements omitted the required semantic `reason`; no source was consumed. The 8B runs had five successful structured-output repairs in 654 and two in 664; these repaired truncation rather than resolving the semantic/tool-selection failures.

The archived seed runner's source file is absent from this workspace. Its reports retain the configured model label `local-test-lead-4b`; the label is stale metadata. During both runs, the only server bound to that report's endpoint had loaded the verified 8B file, and its startup log confirms 37/37 CUDA layers. Context snapshots, provider metadata, tool inputs/outputs, and run reports were preserved in [the A/B trace](../eval-results/long-horizon/qwen3-8b-gpu-ab-20261009-traces.json). The runner does not preserve raw completion text or the rendered per-call action schema, so those cannot be reconstructed exactly.

**Diagnosis: `PARTIAL_MODEL_CAPABILITY_IMPROVEMENT`.** Qwen3-8B made more progress on 654 than 4B, but neither known seed passed. This does not confirm that model size alone resolves the Test Lead capability gap.

After documenting the A/B, `scripts/check.ps1` passed: **119 tests passed, 2 ignored, and 3 doc tests passed**; formatting, compilation, and Clippy passed.

## Mixed 20

**Not run.** Both 8B GPU seeds failed the end-to-end gate. Seed 654 consumed one batch but did not offload a detailed source; seed 664 consumed none and repeatedly omitted Refine's semantic `reason`. The targeted six-case regression and mixed 20 were not started. The last comparable result remains the historical 15/20 and is not a current-version result.

The previous mixed results remain: integrity **20/20**, recovery **20/20**, true branching **2/2** mixed cases, long-context **0/2**, Gate **0**, and deterministic tool-failure recovery **1/1**. These are historical measurements, not results for the new code.

## 200-run campaign

**NOT RUN.** The required fresh mixed 20 did not pass its prerequisite gate. No 200-run campaign or real research pilot was started.

## Remaining readiness gaps

- **PARTIAL_MODEL_CAPABILITY_IMPROVEMENT:** identity binding, schema, branch scoping, restart recovery, and stall guard passed. Qwen3-8B on GPU made source-reading progress in 654, but failed the required artifact offload; in 664 it preserved both branches but did not consume a source and repeatedly omitted Refine's `reason`.
- The six-case targeted suite, fresh mixed 20, and conditional 200-run campaign remain unrun because the known-seed gate failed. Gate, tool-failure, strong-budget, and human-control behavior are therefore not revalidated with the current model in this iteration.

## Verdict

**READY FOR FIRST REAL RESEARCH PILOT: NOT YET.** Both Qwen3-8B GPU seeds failed despite the post-retrieval runtime gate passing. Targeted regression, mixed 20, and the 200-run campaign were not run. No real pilot was started. The next step is a separate protocol/observation review using the preserved context and tool traces; do not redesign Core before reviewing that evidence.

## Lead/Runtime Boundary Cleanup

The current implementation assigns deterministic mechanics to Runtime while keeping research choices with Lead:

| Lead owns | Runtime owns |
|---|---|
| Source choice through current batch handles; read, refine, or reject intent; query content; branch and research decisions | Opaque source/invocation IDs; current run/work/branch scope; handle resolution; automatic artifact offload at the existing 8 KiB threshold; hashes, paths, artifact metadata, invocation relations, provenance; persistence, budgets, and checkpointing |

`RecordedTool` automatically offloads oversized tool output and leaves the invocation-to-artifact relation and registered artifact record intact. The model receives the artifact reference plus a bounded JSON preview (at most 1,400 characters), while the full result remains hash-verifiable in the workspace.

`RefineSearch.reason` is optional audit metadata; omitting it does not block a valid, meaningfully different query or require structured repair. `RejectRetrievedSources.reason` remains required because rejection is a consequential research judgment: the caller must state why the current results are unusable. It is not merely execution bookkeeping.

For a pending batch, the generated `read_source` schema enumerates only its current bounded handles. An empty batch removes `read_source`; a single-source batch contains only `S1`. Runtime still rejects unknown handles and does not resolve raw source IDs from model input. The handle list is scoped using the existing run, work-node, and branch retrieval scope.

### Cleanup iteration results

This addendum supersedes the earlier historical verdict and seed results in this document.

The same Qwen3-8B Q4_K_M GPU server profile was used (RTX 2060, 37 GPU layers, 8,192 context, temperature 0.2). Seed workspaces and reports are under `.runtime/lead-runtime-cleanup-seeds`.

| Seed | Result | Steps / Lead calls | Search | Read attempts / successful | Invalid handles | Consumed batches | Auto-offloads | Refine / successful | Repairs | Branches | Integrity / recovery |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 654, long-context | PASS: `completed` | 20 / 25 | 5 | 10 / 10 | 0 | 5 | 5 | 0 / 0 | 2 / 2 | 1 | PASS / PASS |
| 664, branching | FAIL: `budget_exceeded` | 17 / 21 | 4 | 8 / 8 | 0 | 4 | 0 | 0 / 0 | 5 / 5 | 2 | PASS / PASS |

The final 654 read results exceeded the 8 KiB threshold and were automatically stored as artifacts; each invocation retained its artifact relation, path, hash, and metadata, and the Lead received the bounded preview without source or paper UUIDs. Preliminary 654 reruns used a stale final telemetry matcher and undercounted offloads; the reported row is the final rerun after correcting both detection sites and tightening partially consumed batch handling.

Seed 664 preserved two branches and consumed four batches. Its five repairs were successful recoveries from truncated initial model output (`finish_reason=length`); it then reached the existing repair budget and did not enter a valid terminal state. It had no missing-reason failures and did not attempt refinement. The structured repair policy and retry limits were not expanded.

**Diagnosis: `QWEN3_8B_TEST_LEAD_LIMITATION_CONFIRMED`.** Seed 654 passed with bounded handles and automatic offload. Seed 664 progressed across both branches and consumed sources, but repeated output truncation exhausted the existing bounded repair budget before a valid terminal state. No boundary or runtime binding failures remained; repair policy and limits were unchanged. Seed 664 did not need refinement, so the optional-reason path is covered by schema/parser and runtime contract tests rather than claimed as model behavior.

**Targeted: NOT RUN. Mixed 20: NOT RUN.** They require both seeds to pass. The readiness criteria remain unchanged, and the pilot is **NOT YET**.

## Compact Lead Decision Protocol

### Preserved pre-compact baseline

This records the 8B result before the compact decision protocol; it is not rewritten by later experiments. Seed 654 passed and seed 664 failed after 17 meaningful steps and 21 Lead calls. The final baseline 664 report is under `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/20261009T112412429`.

The old model-facing response required a versioned/metadata envelope and nested `decision → process/use_tool → action/input → data/parameters`. Process required both `rationale` and `expected_result`; completion repeated the current `work_node_id`. Tool calls repeated `schema_version` and a `parameters` wrapper.

Five initial 664 outputs ended with `finish_reason=length`; the stored byte lengths were 1,026, 1,618, 1,339, 1,618, and 682. Each had one successful targeted repair; repair exhaustion occurred at the existing five-repair scenario budget. The archived baseline does not contain raw completion text, initial generated token counts, repair request/response sizes, or the rendered per-call schema. Its stored `output_tokens` describes the repaired response, so it cannot be used as the initial output-token count. The truncated field and exact baseline schema depth are therefore unknown.

### Compact wire contract

Bound `current_work` decisions now use a single root `action` object with a flat `type` and semantic fields. They omit schema version, provider metadata, runtime IDs, `expected_result`, and the nested action/input/data wrappers. `short_reason` is optional and capped at 160 characters. A completed action receives its current WorkNode ID from Runtime. Refine, read, search, reject, work-node, branch, completion, and finish actions normalize to the existing internal decision types; research lifecycle semantics are unchanged.

The schema is built only from actions allowed at the current step. In the branching regression, once the requested independent split exists, Runtime removes further branch creation from the action enum while allowing work-node creation or completion. This enforces the scenario's existing one-time split intent; it does not increase any retry or stall budget.

### Qwen3-8B compact seed 664

Only seed 664 was repeated, using the same Qwen3-8B Q4_K_M, RTX 2060, 37 GPU layers, 8,192 context, and temperature 0.2. Report: `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/20261009T114805944`.

| Metric | Result |
|---|---:|
| Lead calls | 26 |
| Initial output tokens | 23–53 per call; 1,019 total |
| Truncated outputs / repairs | 0 / 0 |
| Compact schema depth | 8–9 |
| Search / Read / Refine / Reject | 5 / 0 / 7 / 4 |
| Successful refinements / consumed batches | 0 / 0 |
| Branches | 2 |
| Final status | `blocked` |
| Integrity / recovery | PASS / PASS |

The model repeatedly proposed meaninglessly identical refinements, which Runtime rejected with `NoProgressSearchRefinement`; it never selected a source to read. Compact serialization removed truncation, but seed 664 still failed the unchanged behavioral gate. The per-call initial tokens, finish reason, validity, repair fields, final action, and schema depth are now recorded in each durable model-invocation record.

**8B verdict: INSUFFICIENT.** This is the clean failure required before trying 14B. Seed 654 remains the preserved 8B PASS and was not repeated in this iteration.

## Qwen3-8B vs Qwen3-14B Test Lead A/B

The 14B experiment is authorized by the clean compact-protocol 8B failure. It uses official `Qwen3-14B-Q4_K_M.gguf` from `Qwen/Qwen3-14B-GGUF`, immutable repository revision `530227a7d994db8eca5ab5ced2fb692b614357fd`. The published file size is 9,001,752,960 bytes and SHA-256 is `500a8806e85ee9c83f3ae08420295592451379b4f8cf2d0f41c15dffeb6b81f0`. See [the official Qwen GGUF file](https://huggingface.co/Qwen/Qwen3-14B-GGUF/blob/530227a7d994db8eca5ab5ced2fb692b614357fd/Qwen3-14B-Q4_K_M.gguf).

The A/B used seed 664 and the same compact schema, prompts, temperature, context, branch constraint, stall policy, repair budget, tools, and Runtime. Context remained 8,192. The 14B file was verified locally at 9,001,752,960 bytes with SHA-256 `500a8806e85ee9c83f3ae08420295592451379b4f8cf2d0f41c15dffeb6b81f0`.

### Qwen3-14B runtime and result

- Runtime: the same llama.cpp b11307 CUDA build, CUDA enabled, Flash Attention on, loopback port 8083, parallelism 1, context 8,192, temperature 0.2, same Test Lead profile and prompt, and the same Runtime/policies as the 8B compact run. The evaluation run used 37 GPU layers, the same offload count as the 8B comparison; llama.cpp loaded it successfully. VRAM use was about 10.1 GiB of 12 GiB during evaluation, and server working set was about 11.3 GiB RAM. A subsequent load-only check with 40 GPU layers at the same context also became healthy, using about 10.5 GiB VRAM. This confirms 40 layers are stable, but the model metadata's exact total layer count was not independently archived; 40 is the highest verified setting in this experiment.
- The available llama.cpp timing samples during the run showed prompt processing around 571–667 tokens/s and generation around 12.6–14.2 tokens/s. Per-request timings were emitted to the live terminal session rather than an archived log, so a full-run median cannot be reconstructed.
- Seed 664 report: `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/20261009T120545497/branching-000664/report.json`. Result: **FAIL**, status `blocked`, 10 meaningful steps, 24 Lead calls, 5 Search, 0 Read, 4 Refine (0 successful), 4 Reject, 2 persisted branches, 0 consumed batches, 0 truncations, 0 repairs, integrity PASS, recovery PASS. Final error: `NoProgressSearchRefinement`. Per-call initial output was valid in all 24 calls, 26–92 tokens (1,121 total, median 35.5); compact schema depth was 8–9. The model repeatedly chose refinements that did not make valid progress.
- Seed 654 was not rerun because 664 did not pass. Targeted scenarios, mixed 20, and 200 were not run. No pilot was started.

**Diagnosis: `MODEL_SIZE_ALONE_DOES_NOT_RESOLVE_TEST_LEAD_FAILURE`.** The 8B compact run eliminated truncation but failed to progress past repeated invalid refinements; the 14B run also avoided truncation but repeated the same no-progress refinement pattern. This leaves a decision/protocol/observation issue to review, not evidence that another model-size escalation is warranted. The compact protocol implementation remains in place; do not expand repair budgets or change the product micro-model as part of this experiment.

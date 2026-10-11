# Lead Decision Trace and Retrieval Observation

## Baseline

Before this turn's changes, `scripts/check.ps1` passed: 52 runtime-v2 tests passed, 2 were ignored, 3 rustdoc compile-fail tests passed, and formatting/build/Clippy/workspace checks passed. The repository has no `.git` directory, so source hashes are used for this run's fingerprint.

Historical seed 664 results remain separate from the new run:

| Model | Lead calls | Search | Read | Other retrieval actions | Outcome |
|---|---:|---:|---:|---|---|
| Qwen3-8B Q4_K_M | 26 | 5 | 0 | 2 branches | blocked |
| Qwen3-14B Q4_K_M | 24 | 5 | 0 | 4 Refine, 4 Reject, 2 branches | blocked; integrity and recovery passed |

The pre-semantics diagnostic reproduction for 8B seed 664 was recorded at `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/20261009T122922487/branching-000664/`. It ended blocked after 7 calls: 1 Search, 4 invalid Refine attempts, 0 Read, 2 branches; integrity and recovery passed. The exact trace showed valid `S1`/`S2` handles and an available `read_source` action, but the model-facing Search observation omitted source mapping, previews and unread status. The rendered context separately contained paper descriptions without connecting them to handles. The model repeated an identical refinement query despite the tool description. This confirmed an observation gap and showed that the objective/action guidance needed clearer task context.

## Trace implementation

Diagnostic mode is opt-in (`diagnostic_traces = true` in the local Test Lead profile; false by default). It stores the exact rendered request messages, the concrete per-call JSON action schema passed to the provider, allowed tool names, raw completion(s), finish reason, output tokens, parsed decision, provider profile, validation and execution outcome, progress snapshots, retrieval state, and links to the run, work node, branch, model invocation and ContextSnapshot. Execution outcomes update the existing ModelInvocation metadata and retain the associated ToolInvocation/checkpoint records. Diagnostic rows are emitted as JSONL under the eval run directory and registered through the existing Core Artifact mechanism. Trace metadata is bounded and credential-like values are redacted; raw trace payloads are not put into ordinary status output. No hidden chain-of-thought is recorded.

Focused tests verify request/schema/completion capture, redaction and bounds, execution outcome linkage, and the updated retrieval observation. `cargo test -p rn-integrations -p rn-agent -p rn-evals --quiet` passed (2 agent, 41 eval, 13 integrations tests); `cargo test --test acceptance --quiet` passed (12 tests); `cargo fmt --all -- --check` passed.

## First diagnostic run before protocol/observation changes

The original-protocol diagnostic run was completed before changing prompt or retrieval semantics. Its trace and Artifact are in the directory noted above. It confirmed:

- `read_source` was allowed while retrieval was pending, with the correct dynamic enum handles `S1` and `S2`.
- The visible Search observation contained generic titles and a result count, but no candidate previews, handle-to-source mapping, unread markers, objective, or explicit actions.
- The actual ContextSnapshot had paper titles and short descriptions, but those entries were not tied to `S1`/`S2` in the rendered request.
- The WorkNode objective and eval wording did not clearly frame the current evidence task. The model saw target-step language that could bias its choices.
- Four identical Refine attempts were rejected by Runtime. This is observable behavior; the trace does not establish the model's internal reason.

Confirmed initial classifications: `RETRIEVAL_OBSERVATION_GAP` and `WORKNODE_OBJECTIVE_AMBIGUITY`, plus repeated invalid model decisions observable in the trace. No progress-accounting defect was established.

## Changes

After capturing that baseline, Search now returns observation version 2 with the research objective, current WorkNode objective, query, mapped source titles and bounded previews, unread status, previously consumed sources, unresolved evidence needs and available actions. Search previews are explicitly described as triage; ReadSource inspects actual content; Refine and Reject descriptions state the conditions under which they are appropriate. The Lead guidance is conditional and does not force reads. The synthetic question and root WorkNode no longer advertise a minimum step count.

The observation changes are generalized and seed-independent. They preserve Read, materially changed Refine, and justified Reject as Lead choices. They do not force AutoRead or alter acceptance thresholds.

## Post-change seed 664: decision timeline and result

Run directory: `.runtime/lead-runtime-cleanup-seeds/branching-664/eval-results/long-horizon/runs/20261009T124328210/branching-000664/`.

| Turn | WorkNode | Retrieval state / actions | Lead decision | Result | Progress |
|---:|---|---|---|---|---|
| 1 | Root composite | No pending retrieval; branch/finish schema | Create branch for pathways P and Q | 2 branches persisted | meaningful steps 1 |
| 2 | Root composite | No pending retrieval; create-work schema | Create child `Investigate branching`, `required_tool=read_source` | Child persisted | meaningful steps 2 |
| 3 | Atomic `Investigate branching` | No retrieval batch; only `read_source` allowed; no handles | Read `S1` | Rejected: `invalid source reference` | no source reads |
| 4 | Same atomic WorkNode | No retrieval batch; only `read_source` allowed; no handles | Read `S1` | Rejected: `invalid source reference` | no source reads |
| 5 | Same atomic WorkNode | No retrieval batch; only `read_source` allowed; no handles | Read `S1` | Rejected: `invalid source reference` | no source reads |
| 6 | Same atomic WorkNode | No retrieval batch; only `read_source` allowed; no handles | Read `S1` | Rejected: `invalid source reference` | no source reads |

The concrete schema on turns 3–6 had a single `oneOf` option, `read_source`, with a `parameters` object; the exact allowed-tools list was `read_source`; the available-handle list was empty. Each raw completion selected `read_source` with `handle: S1`. The short rationale claimed it was inspecting a retrieved candidate, but no retrieval batch existed. The rendered task objective was the generic `Investigate branching`. These exact requests, schemas and completions are in the JSONL trace, linked to each invocation and ContextSnapshot.

The post-change report says `blocked`, `meaningful_steps=2`, `lead_calls=6`, `retrieval_calls=0`, `read_source_calls=4`, `consumed=0`, `branches=2`, `integrity=true`, `recovery=true`, error `validation: invalid source reference`. The report's `tool_calls=0` counts persisted tool invocations; the four failed ReadSource attempts are reflected in `read_source_calls` and each has a diagnostic execution result. No search candidates were produced in this run, so candidate relevance could not be evaluated.

## Root cause from the post-change trace

The observation redesign was not reached: no Search action ran. The decisive protocol defect is that an atomic WorkNode whose capability requirement is `tool:read_source`, while retrieval is not pending, receives a dynamic schema and allowlist containing only `read_source`. There is no permitted `search_sources` prerequisite in that state. This creates an impossible sequence: the model must read, but the runtime has issued no bounded handles. The repeated invented `S1` choice follows the only allowed action but is invalid because no batch exists.

Confirmed classifications:

- `ACTION_SCHEMA_AMBIGUITY`: the required-tool contract fails to represent the Search-before-Read prerequisite; the specific runtime schema is overly narrow.
- `WORKNODE_OBJECTIVE_AMBIGUITY`: `Investigate branching` does not express which evidence should be retrieved/inspected.
- `MODEL_DECISION_FAILURE`: after the first invalid reference, the model repeated `S1` four times instead of changing course. The trace proves the repeated output, not its internal cause.

The earlier `RETRIEVAL_OBSERVATION_GAP` remains fixed in code and covered by tests, but this failed run exposes a separate blocker before that observation can be exercised. This run does not prove the updated observation can drive a complete research cycle.

## Reproducibility fingerprint

- Scenario/seed: `branching` / `664`; Qwen3-8B Q4_K_M; RTX 2060 12 GB; 37 GPU layers; context 8192; temperature 0.2; local-only loopback; one slot.
- llama.cpp: `0.5.0-dev`, build `11307`, commit `4453b535f`.
- Model SHA-256: `D98CDCBD03E17CE47681435B5150E34C1417F50B5C0019DD560E4882C5745785`.
- llama-server SHA-256: `D0415A4ADED0BF16FB6AAF56E01C1607FE0DD0E2E319F369FA8C127C54107D7B`.
- Diagnostic profile SHA-256: `0A537AA9EBC09EFCD853093E3E52B6B008CFE063B1DB357FC65DB4E469E0392E`.
- Eval runner SHA-256: `2D5BE2B5F8FB5E15386447E9D1C9CCC4348C400776ED55517E580E85E6A175AF`.
- Rust: `rustc 1.98.1`; Cargo `1.98.1`.
- Observation version: 2. The baseline and post-change prompts/protocol differ, so the runs are not claimed to be bitwise comparable. llama.cpp's generation request does not receive a deterministic seed; this is a same-scenario/config regression run, not a deterministic replay.

Relevant source SHA-256 values:

| File | SHA-256 |
|---|---|
| `crates/rn-agent/src/lib.rs` | `BAEC9AC582FEC48275AAED716C7B905C1FE24706ECD243B72408E54A7A018FEE` |
| `crates/rn-integrations/src/test_lead.rs` | `D5C2BDA18C133DA7CD6B7E853FDFDB594D081C77B6940BBC488FFC6CEDD5AFAF` |
| `crates/rn-evals/src/long_horizon.rs` | `A97506D4807AC557A0A457C174F5497C3932D6058776A1E0CBB01E30811FE208` |

## Gate decision

Seed 664 failed the required criteria. Per the evaluation gate, seed 654 and targeted/mixed/200 campaigns were not run. The next code change should address the generalized pre-retrieval tool contract so a `read_source` work item can search before reading, and ensure the WorkNode objective specifies the evidence task. No forced reads, seed-specific branches, or weakened assertions are warranted. First-pilot readiness remains **NO**.

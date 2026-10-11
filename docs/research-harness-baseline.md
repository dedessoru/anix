# Research harness baseline — 2026-10-01

## Batch and configuration

The first real baseline batch used seed `700`, the ten offline scenario packs, and the local `local-test-lead-4b` profile (Qwen3-4B Q4_K_M). The project store was reopened through a new Core instance after each scenario and the persisted checkpoint was resumed. No cloud provider was configured.

| Measure | Result |
| --- | ---: |
| Research workflows completed | 0/10 |
| Invariant audits passed | 10/10 |
| Checkpoint resumes succeeded | 10/10 |
| Agent output/schema rejections | 9/10 |
| Long-context preflight rejections | 1/10 |
| Loop detections | 0 |
| Model calls attempted | 10 (1 per run) |
| Tool calls | 0 |
| Context estimate, mean | 2,066.8 tokens |
| Runtime p50 / p95 | 222.5 / 262.8 seconds |
| Peak observed Test Lead process RAM | about 12.9 GiB |

All ten failed runs produced structured audit reports with no invariant violations. The restart check preserved completed WorkNodes and committed claims in all ten. This demonstrates state integrity and the exercised resume path, but does not demonstrate successful completion of the agent workflows.

## Scenario results

| Scenario | Outcome | Observation |
| --- | --- | --- |
| Basic Evidence Research | Failed safely | `ProposedAction` representation rejected; audit and resume passed. |
| Contradictory Evidence | Failed safely | Same tagged-enum representation error; audit and resume passed. |
| Duplicate Sources | Failed safely | Same representation error; synthetic duplicate claims were deduplicated; audit and resume passed. |
| Branching | Failed safely | Same representation error; branch/checkpoint state audited; resume passed. |
| Long Context | Failed safely | Provider preflight rejected an 8,179-token estimate against an 8,192-token limit; no inference was sent. Audit and resume passed. Compression/offload was not exercised. |
| Sparse Evidence | Failed safely | Same representation error; audit and resume passed. |
| Experiment Proposal | Failed safely | Same representation error; no Gate was reached. Separate harness GateService tests are not a substitute for this real workflow. |
| Failed Experiment | Failed safely | Same representation error; real model workflow did not reach experiment execution. |
| Revision | Failed safely | Same representation error; audit and resume passed. |
| Interrupted Research | Failed safely | Same representation error; audit and resume passed. |

## Manual review and issue classification

1. **HIGH — PROMPT_OR_SKILL_PROBLEM.** Nine responses failed with `invalid type: string "revise_hypothesis", expected adjacently tagged enum ProposedAction`. The system prompt named `ProposedAction` but did not show its actual adjacent-tag representation (`{"type":"revise_hypothesis","data":{...}}`). Since the same failure blocked nine of ten workflows, clarify the contract with concrete valid and `no_change` examples, then rerun the original seeds.
2. **HIGH — TEST_HARNESS_PROBLEM.** The long-context fixture reached 8,179 estimated tokens before inference, leaving effectively no room under the 8,192 profile limit. The guard correctly rejected it without corrupting state, but the scenario failed to test retrieval/compression/offload. Adjust the fixture or context preparation so the test exercises those paths while remaining within the configured model window.
3. **HIGH — TEST_HARNESS_PROBLEM.** The reports recorded zero malformed outputs and zero schema failures despite nine provider parse failures. The classification relied on matching the words `JSON`, `structured`, and `schema`, while the actual error says `invalid Test Lead response`. Correct failure classification before using these rates.
4. **MEDIUM — PERFORMANCE_ISSUE.** Qwen3-4B Q4_K_M required roughly 216–263 seconds per model-backed run on this host; peak observed process RAM was about 12.9 GiB. The 8B profile had passed a short completion smoke but its first real research request exceeded the 300-second timeout after prefill. Keep 8B as the configurable default, and use 4B for the local campaign on this machine.

No BLOCKER was observed: no state corruption, provenance loss, gate bypass, duplicate consequential side effect, invalid graph, or failed restart audit. The campaign did not establish scientific/model quality, successful gate transitions, or successful completion: the model outputs were rejected before actions were committed.

## Targeted fixes before regression

- Show the exact serde-tagged `ProposedAction` representation in the Test Lead system prompt, constrain the outer decision and nested action with llama.cpp JSON Schema, and keep action fields aligned with the Core types.
- Count provider response parsing/version/shape errors as malformed/schema failures in run and batch metrics. Preserve a safe normalization marker when optional provider metadata is absent.
- Use llama.cpp `/apply-template` plus `/tokenize` to measure the actual prompt and reserve 384 tokens for output before sending the request.
- Configure the 4B fallback at a 16K context window so the persisted long-context corpus fits. Raise its timeout to 900 seconds after the targeted scenario reached the 600-second timeout while processing 13,390 prompt tokens.
- Scope loop fingerprints to one research run. The first offline full-suite check exposed false LoopDetected results across independent projects; after scoping the guard, all ten fake-backed scenario tests completed.
- Preserve the structured audit and checkpoint-resume assertions; the first batch showed both remain safe under rejected model output and context-budget rejection.

## Exact-seed regression after targeted fixes

The original ten scenarios were rerun with seed `700` and `local-test-lead-4b` using a 16,384-token profile and 900-second timeout. Prompt preflight used llama.cpp template application and tokenization; the Long Context request reached 13,386 tokens at the server (13,320 reported by provider usage) without truncation.

| Measure | Result |
| --- | ---: |
| Research workflows completed | 10/10 |
| Invariant audits passed | 10/10 (all violations lists empty) |
| Checkpoint resumes succeeded | 10/10 |
| Malformed model outputs / schema failures | 0 / 0 |
| Loop detections | 0 |
| Duplicate consequential actions prevented | 2 across the batch |
| Gate records created | 8 |
| Experiments / experiment runs | 2 / 2 |
| Model calls / tool calls | 10 / 0 |
| Mean actual Test Lead preflight / usage input | 7,729 / 7,729 tokens |
| Mean Core context estimate | 2,030 tokens |
| Runtime p50 / p95 | 247.7 / 552.7 seconds |
| 4B server RSS during 16K Long Context request | about 10.4 GiB |

All ten research runs completed with no error. Every audit passed and every run resumed after constructing a new Core instance while preserving committed work. Experiment Proposal and Failed Experiment created two experiments and three Gate records respectively; the harness checked the actual resulting state and Gate records. The completed 50-run variation campaign and its architecture verdict are documented in [research-harness-50.md](research-harness-50.md).

The first completed report files came from a binary that did not aggregate the newly added preflight counter. The metric was backfilled from each persisted Core run's `provider_metadata.response.preflight_input_tokens`; report and summary values now match the stored records. The CLI was rebuilt from the updated source before the next batch.

## Issue classification and resolution

The initial 9/10 tagged-enum parse failures were a model contract/prompt integration issue, not state corruption in Core. The single long-context rejection identified an undersized test profile and token estimate mismatch. The false malformed/schema counters were an evaluation classification defect. The offline fake-model suite also exposed a per-project loop guard lifetime error. These targeted issues were fixed and the identical ten seeds then passed end to end. No BLOCKER or HIGH state-integrity, provenance, Gate-bypass, duplicate-side-effect, graph, or restart defect remained in the regression batch.

The 8B Q4_K_M profile remains the configurable Test Lead default and its real smoke completion works, but its first full research request on this CPU host exceeded the 300-second prefill timeout. The 4B Q4_K_M profile is the measured local campaign fallback; in the exact-seed regression it needed up to 553 seconds for Long Context. The 50-run campaign completed 46/50 workflows; all 50 audits and resumes passed. Because three responses were truncated and one Long Context request exceeded the 900-second timeout, the verdict is **NOT YET** for a 200–500-run campaign. See [research-harness-50.md](research-harness-50.md) for the full measurements and failure review.

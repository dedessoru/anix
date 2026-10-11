# Research harness 50-run report — 2026-10-01

## Campaign

- Batch: `eval-results/2026-10-01-052351`
- Requested and executed: 50 runs, seeds 900–949
- Scenarios: the same 10 synthetic scenario packs, repeated five times
- Test Lead: `local-test-lead-4b` (Qwen3-4B Q4_K_M), local llama.cpp OpenAI-compatible server; revision `bc640142c66e1fdd12af0bd68f40445458f3869b`, SHA-256 `7485fe6f11af29433bc51cab58009521f205840f5b4ae3a32fa7f92e8534fdf5`, weight file 2.50 GB
- Profile: 16,384-token context, 900-second request timeout
- Host: Windows x86_64, Ryzen 5 3600, 32 GiB RAM; Test Lead working set observed around 7–15 GiB during the sequential campaign
- llama.cpp server timing samples: prompt prefill about 22–48 tokens/s depending on context length; one logged decode sample was 2.65 tokens/s. These are observed samples, not campaign-wide averages.
- Cloud calls: none configured; mean tool invocations: 0
- Mean model invocations: 1.00 per run

The report files and `summary.json` are the machine-readable source of these values. Each run used persisted project state. The harness constructed a new Core instance, resumed the saved checkpoint, and audited the resulting state.

## Results

| Measure | Result |
| --- | ---: |
| Completed workflows | 46/50 (92%) |
| Failed workflows | 4/50 |
| Invariant audits passed | 50/50 (100%) |
| Checkpoint resumes succeeded | 50/50 (100%) |
| Completed work preserved after resume | 50/50 |
| Loop detections | 0 |
| Duplicate consequential actions prevented | 10; reported duplicate-work rate 7.14% |
| Malformed/schema-invalid model outputs | 3/50 (6%) |
| Mean model calls / tool calls | 1.00 / 0.00 |
| Mean reported Test Lead input / output | 7,096.5 / 119.8 tokens per requested run |
| Test Lead preflight input | 7,392.1 tokens per requested run, with unavailable measurements counted as zero; 47 measured requests averaged 7,863.9 tokens |
| Mean Core context estimate | 2,142.2 tokens |
| Runtime p50 / p95 | 272.7 / 785.0 seconds |
| Mean runtime | 335.9 seconds |
| Longest request timeout | 901.7 seconds |

The input and preflight means include zero-valued report fields for requests whose response was truncated before usage metadata was returned. The separately reported 47-request preflight mean avoids treating those missing measurements as zero.

## Failed runs

| Run | Scenario | Result |
| --- | --- | --- |
| 011 (seed 910) | Basic Evidence Research | The model returned incomplete JSON (`EOF while parsing a string`, column 917). Classified as one malformed output and one schema failure. |
| 028 (seed 927) | Failed Experiment | The model returned incomplete JSON (column 1200). One malformed output and one schema failure. The persisted project still contains the failed experiment and its Gate records. |
| 035 (seed 934) | Long Context | The client request timed out after 901.7 seconds at a 14,781-token preflight. The audit checked 25 invariants and passed; resume succeeded. |
| 042 (seed 941) | Contradictory Evidence | The model returned incomplete JSON (column 957). One malformed output and one schema failure. |

Every failure was contained: it did not produce an invariant violation or break checkpoint recovery. The three malformed responses and the long-context timeout are still real workflow failures, so the 92% completion rate does not meet the bar for a larger campaign.

## State and scenario coverage

All 50 persisted projects passed their invariant audit and resumed with work preserved. Across the batch, reports recorded 140 claims, 140 relations, 50 hypotheses, 42 Gate records, 60 branches, and 310 checkpoints. Ten duplicate consequential actions were prevented. The experiment scenarios created ten experiments; five experiment runs were recorded and all five were classified as failed experiments. No finding records were produced. No tool calls were made.

Each of the ten scenarios completed in four or five of its five repetitions. The failures covered three distinct scenario families (Basic Evidence, Failed Experiment, and Long Context, plus Contradictory Evidence), rather than a single fixture-only case.

## Issue review

No BLOCKER or HIGH Core state-integrity issue was found in this campaign: all audits passed, all resumes preserved work, there were no loops, and duplicate actions were prevented. The observed failures were at the Test Lead response/transport boundary. Three responses ended as incomplete JSON without a retry; the Long Context call exceeded the configured 900-second timeout. These outcomes are safely reported, but they remain unresolved model reliability and latency limits for this host/profile.

## Architecture verdict

**NOT YET** ready for a 200–500-run research stress campaign.

Evidence for Core safety is strong in the exercised paths: 50/50 invariant audits and checkpoint resumes passed, with zero loops and duplicate consequential actions blocked. The end-to-end completion rate was only 46/50 (92%), with three malformed model responses (6%) and one request exceeding 900 seconds. A larger batch would amplify these known reliability and runtime limits. The correct next step is to improve bounded retry/continuation handling for truncated structured responses and provide a long-context execution budget or faster local profile, then rerun the 50-run variation batch before expanding its size.

This verdict is about the tested local workflow and profile. It is not a judgment of the scientific quality of the model's conclusions.

The configurable Test Lead default remains Qwen3-8B Q4_K_M (revision `7c41481f57cb95916b40956ab2f0b139b296d974`, SHA-256 `d98cdcbd03e17ce47681435b5150e34c1417f50b5c0019dd560e4882c5745785`). Its short smoke completion worked, but the first full CPU research request exceeded a 300-second timeout, so this 8B profile was not used for the 50-run measurements. Full-workflow RAM and throughput for 8B were not established.

# Micro model A/B evaluation

## Decision

Keep **Qwen3-1.7B Q8_0** as the product micro-model. MiniCPM5-2B Q6_K and Q8_0 remain registered evaluation candidates and are not promoted. Neither candidate meets the existing task quality and structured-output gates. MiniCPM remains a low-risk helper only; it is never a Lead Agent or a source of research decisions or canonical state changes.

## Method

All six tasks used the same `micro-v3` prompts, temperature 0, structured schemas, and llama.cpp/b11307 CPU runtime. Models ran with 4096 context on the same 100-case `micro-v1` set and 140-case `real-micro-eval` set. The realistic fixtures are generated, RN-shaped examples without user or sensitive data. The realistic set covers query rewrite (25), candidate extraction (30), tagging (20), short summary (20), context compression (20), and classification (25), including negation, contradictory statements, source qualifications, irrelevant and empty input, Unicode, mixed language, technical notation, and long input.

The count columns below are **quality-valid / schema-valid / total**. Extraction quality requires exact source quotes; its schema count is reported separately. Classification, tagging, summary, rewrite, and compression quality includes task-specific checks. A malformed response can be retried once in normal routing and then escalated.

## Same 100-case regression set

| Model | Extraction | Classification | Compression | Rewrite | Summary | Tagging | Schema | p50 / p95 latency | Resident / peak RAM after load / during eval |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Qwen3-1.7B Q8_0 | 20/20/20 | 24/25/25 | 10/10/10 | 15/15/15 | 15/15/15 | 15/15/15 | 100/100 | 2.306 / 10.668 s | 2.21 / 3.01 GiB |
| MiniCPM5-2B Q6_K | 20/20/20 | 20/25/25 | 10/10/10 | 15/15/15 | 6/15/15 | 0/15/15 | 100/100 | 1.639 / 6.764 s | 1.98 / 2.34 GiB |
| MiniCPM5-2B Q8_0 | 20/20/20 | 20/25/25 | 10/10/10 | 15/15/15 | 4/15/15 | 0/15/15 | 100/100 | 2.445 / 9.256 s | 2.45 / 2.80 GiB |

Qwen's 24/25 classification is below the existing 97% gate, so classification stays on the strong route. MiniCPM's perfect schema-valid rewrite/extraction/compression cases in this small set do not compensate for weak summary/tagging and classification.

## Realistic 140-case evaluation

| Model | Extraction | Classification | Compression | Rewrite | Summary | Tagging | Schema | p50 / p95 latency | Resident / peak RAM after load / during eval |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Qwen3-1.7B Q8_0 | 20/30 quality; 23/30 schema | 11/25 | 19/20 | 25/25 | 16/20 | 8/20 | 133/140 | 2.673 / 36.537 s | 2.21 / 3.43 GiB |
| MiniCPM5-2B Q6_K | 23/30 quality; 24/30 schema | 20/25 | 20/20 | 25/25 | 19/20 | 1/20 | 134/140 | 2.383 / 29.026 s | 1.98 / 2.64 GiB |
| MiniCPM5-2B Q8_0 | 24/30 quality; 24/30 schema | 21/25 | 20/20 | 25/25 | 19/20 | 1/20 | 134/140 | 2.298 / 36.665 s | 2.45 / 3.07 GiB |

Qwen malformed JSON was caught on seven realistic cases, primarily truncated extraction output. MiniCPM produced six malformed extraction responses at either quantization. The evaluator also found schema-valid quality failures: notably MiniCPM returned incorrect/empty tags on 19/20 tagging examples, and only reached 80–84% classification. Those errors are not detectable from schema validity alone. The paired quality gate therefore records the lower score across datasets and requires 100% schema validity on both before a task can be advertised as locally eligible.

Q8 gained one exact extraction case and one classification case over Q6 on the realistic set, with no change in summary, tagging, rewrite, or compression. It retained the same 24/30 extraction schema-valid count, used about 0.47 GiB more resident memory at 4K, and had no p95 advantage. **Q8 does not justify its memory cost.** Q6 is the preferable MiniCPM quantization only if it is useful for future experiments; neither qualifies for default promotion.

## Context and resource checks

All candidate models passed a real one-case classification smoke test. Q6 also passed at 8192 and 16384 context. Its post-load resident working set was approximately 1.98 GiB at 4096, 2.14 GiB at 8192, and 2.47 GiB at 16384 in these process samples; 16K therefore costs roughly 0.49 GiB over 4K. Full realistic evaluations were run at 4096 and completed, including the long-input fixture. Keep 4K as the micro context default; larger contexts did not establish a quality benefit here. Peak eval RAM varies with the OS and process history, so these figures describe this Windows CPU run, not a device-wide guarantee.

The 100-case Q6 and Q8 averages were respectively 60.3/15.1 and 45.3/10.6 prompt/decode tokens per second. On the realistic set they were 51.8/11.3 and 46.3/11.7. Q8's larger weights and memory use bring no reliable overall speed or quality advantage.

## Routing and regression boundary

The explicit local task allowlist remains `QueryRewrite`, `CandidateExtraction`, `Tagging`, `ShortSummary`, `ContextCompression`, and `Classification`. Micro requests require `LowRisk`, a supported schema and context, and measured task quality. Consequential and high-impact requests always use the strong route. Unknown tasks also use strong. A local runtime error or invalid output is retried at most once, then escalated; private work fails closed rather than being sent to another provider. Confidence or self-reported model capabilities cannot relax these rules.

The paired capability gate compares the pinned synthetic and realistic report identities, prompt/fixture versions, and counts. A task is omitted if either dataset has less than 100% schema validity; its quality is the lower score across the datasets. With the Qwen reports, only QueryRewrite clears schema validity and quality. Classification (96% synthetic and 44% realistic), tagging, summaries, compression, and extraction stay off the local route under this conservative paired gate. This is a measured capability filter, not a change to the product's default model or authority boundary.

The router/runtime regression suite covers all six allowlisted task kinds, consequential/high-impact escalation even when a local provider claims extra capabilities, explicit impact-class rejection, malformed schema, unique-array constraints, one retry then strong fallback, and paired-report capability filtering. A three-scenario individual integration regression, its safe long-context preflight rejection, and restart/audit results are documented in `eval-results/long-horizon/micro-integration-regression-902.md`. The dedicated campaign CLI accepts only 10 or 20 runs, so the requested small follow-up was run as individual research scenarios without changing campaign policy. The earlier long-horizon campaign remains preserved at 20/20 completed and audited, with zero micro invocations. `scripts/check.ps1` is the complete validation command.

## Reproducible artifacts

- Synthetic fixtures: `eval/micro-v1.jsonl`; realistic fixtures: `eval/real-micro-eval.jsonl` and `scripts/generate-real-micro-eval.py`.
- Reports: `eval/results-qwen3-1.7b-micro-v1-rerun.json`, `eval/results-minicpm5-q6-micro-v1.json`, `eval/results-minicpm5-q8-micro-v1.json`, plus the three `*-real-micro-eval.json` reports.
- Model profiles: `models/profiles/minicpm5-2b-q6.toml`, `models/profiles/minicpm5-2b-q8.toml`, and `models/profiles/qwen3-1.7b-micro.toml`. Weight files remain outside Git and are checksum-verified by bootstrap.
- Qwen's manifest/default remains unchanged. No Test Lead or strong model profile was modified.

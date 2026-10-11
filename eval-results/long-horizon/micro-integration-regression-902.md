# Micro iteration integration regression

The repository's long-horizon campaign command is intentionally limited to 10-run `initial`/`regression` campaigns and gated 20-run variations. To keep this follow-up small, run three individual research scenarios through the same local Test Lead path; no campaign count or harness policy was changed. Test Lead was Qwen3-4B Q4_K_M on llama.cpp/b11307, loopback port 8082. The product micro default remained Qwen3-1.7B Q8_0, and these scenarios recorded zero micro calls.

| Scenario | Seed | Status | Context preflight / observed max | Restart/resume | Audit |
| --- | ---: | --- | ---: | --- | --- |
| `basic-evidence` | 902 | completed | 7107 / 1189 tokens | 1 / 1; work preserved | pass, 10 checks |
| `long-context` | 903 | rejected before model call | 15147 / 8051 tokens | 1 / 1; work preserved | pass, 25 checks |
| `interrupted-research` | 904 | completed | 6964 / 1467 tokens | 1 / 1; work preserved | pass, 10 checks |

The long-context fixture still exceeds the configured 8192-token Test Lead budget at preflight. The runtime rejected it safely, and the integrity audit passed; it did not exercise model-backed retrieval/compression for that case. This is the already documented fixture/context-budget issue, not a micro-model regression. No micro model was invoked in these runs, consistent with the unchanged routing/default configuration. The checks give a small integration signal for execution, restart/resume, and integrity; they do not estimate Lead Agent quality.

The standard batch command accepts only 10 or 50 cases; the dedicated long-horizon command accepts 10 or 20. The three individually selected scenarios are therefore reported transparently as a small integration regression rather than a reduced campaign.

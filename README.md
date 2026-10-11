# ANIX research platform

ANIX V2 adds a persistent work DAG, immutable plan revisions and context snapshots, branches, checkpoints, execution records and measured local/strong routing. See [runtime-v2.md](docs/runtime-v2.md) and [evaluation results](docs/v2-results.md). The existing V1 scientific API remains available.

ANIX is a local research platform with a Rust Core, Research Runtime, Agent Orchestration, model/tool adapters, a shared Application API and the `anix` CLI. It provides durable structured state, evidence graphs, provenance, human decisions, tasks/runs, and a replaceable agent execution plane. All clients use the same application API. No UI or network credentials are required.

Rust stable with Cargo, Clippy, rustfmt, and a C compiler for bundled SQLite is required. See the current audit for the toolchain and verification results on Windows/MSVC.

```sh
cargo build --workspace
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If Windows has both Chocolatey and rustup installations, use `./scripts/check.ps1`. It selects Cargo, rustc, rustdoc, and Clippy from one rustup toolchain, restores the calling environment afterwards, and uses `target/verified` to avoid mixed compiler artifacts.

Start a workspace and create a project:

```sh
cargo run -p anix-cli -- --workspace ./my-research project create --name "Battery study"
cargo run -p anix-cli -- --workspace ./my-research project list
```

Copy the project `id` from the JSON output and use it as `PROJECT_ID` below:

```sh
cargo run -p anix-cli -- --workspace ./my-research --project PROJECT_ID question add --title "Does electrode A improve lifetime?"
cargo run -p anix-cli -- --workspace ./my-research --project PROJECT_ID source add --title "Study PDF" --kind pdf --locator sources/study.pdf
cargo run -p anix-cli -- --workspace ./my-research --project PROJECT_ID research-state
cargo run -p anix-cli -- --workspace ./my-research --project PROJECT_ID timeline
```

Every command and subcommand supports `--help`. The CLI provides project create/list; question/source/paper/claim add; hypothesis create/show/list/revise/status; experiment create/show/activate/start/complete; finding add; graph show/add; evidence trace; gate list/show/approve/reject/cancel/expire; task create/list; run start/show/list/complete/fail/cancel; timeline, research-state, and projections. Missing arguments and application errors return nonzero exit codes. JSON output uses typed IDs and version fields.

For an offline agent demonstration, create a hypothesis, then pass its ID and current version:

```sh
cargo run -p anix-cli -- --workspace ./my-research --project PROJECT_ID agent demo-experiment --hypothesis HYPOTHESIS_ID --expected-version 1 --title "Cycle comparison" --objective "Measure lifetime" --methodology "Compare A and baseline" --protocol "1000 charge cycles"
cargo run -p anix-cli -- --workspace ./my-research --project PROJECT_ID gate list
cargo run -p anix-cli -- --workspace ./my-research --project PROJECT_ID gate approve GATE_ID
```

The demo uses a deterministic fake provider and creates a persisted gate. Approval creates an approved experiment. A human can explicitly activate it (`experiment activate ID --expected-version 1`), start an attempt (`experiment start ID --expected-version 2`), complete that attempt, and add a finding. This records execution; scientific execution engines are future tool adapters.

See [the current audit](docs/anix-audit.md), [ANIX architecture](docs/anix-architecture.md), [migration](docs/anix-migration.md), and [roadmap](docs/anix-roadmap.md). Historical evaluation reports describe earlier source versions.

## Local research evaluation

The product micro-model defaults to Qwen3-1.7B Q8_0. Qwen3-0.6B is optional (`--profile economy`). The separate Test Lead profile points to a loopback OpenAI-compatible llama-server and can be bootstrapped and started on Windows with:

```powershell
anix model bootstrap --profile test-lead
.\scripts\start-test-model.ps1
anix eval research batch --profile local --count 10 --seed 1
```

`anix eval research run basic-evidence --profile local` runs one scenario. Offline packs are in `tests/research-packs/`; batch reports are written to `eval-results/`. CI uses fake providers and does not download model weights. The separate `anix-evals` crate uses the same Core and Agent APIs as the CLI, and rejects non-loopback providers in local test mode.

Real local harness results: [initial baseline and exact-seed regression](docs/research-harness-baseline.md), [50-run variation batch and campaign verdict](docs/research-harness-50.md). The current Qwen3-versus-MiniCPM micro-model comparison, realistic edge cases, and routing decision are in the [micro-model A/B report](docs/minicpm5-micro-eval.md).

Workspace layout:

```text
.anix/state.sqlite     canonical research state
sources/ papers/     source files
data/ experiments/  inputs and execution artifacts
figures/ reports/   outputs
notes/generated/    rebuildable projections
```

Crates:

| Crate | Responsibility |
| --- | --- |
| `anix-domain` | Typed entities/IDs, actions, policies, events, errors |
| `anix-core` | Application services, transactional port, research/evidence queries, agent capability, tool ports |
| `anix-storage` | Dedicated SQLite repositories, migrations, transaction implementation |
| `anix-agent` | Bounded context, provider/selector interfaces, fake provider, basic Lead Agent, approved tools |
| `anix-integrations` | Workspace files/hashes, projections, bounded file tool |
| `anix-evals` | Offline research packs, local-only Test Lead runs, invariant auditor, replay and diagnostic bundles |
| `anix-cli` | Development client and dependency composition |

The central acceptance test in `tests/acceptance.rs` covers Question -> Source/Paper/Claim -> Hypothesis -> Lead Agent -> Human Gate -> ExperimentRun -> Finding -> Evidence -> restart. Additional tests cover stale proposals, injected persistence failures, post-commit notifications, schema integrity, permissions, idempotency, tool boundaries, and the CLI.

For future changes, follow the Russian [Core development guide](docs/core-development-guide.md), which defines dependency boundaries, mutation rules, agent capabilities, and the implementation/review process.

See [architecture](docs/architecture.md), [domain model](docs/domain-model.md), [persistence](docs/persistence.md), and [agent architecture](docs/agent-architecture.md). Core has no model-vendor or llama.cpp dependency; inference providers are adapters. There is no TUI/Theia, sync server, vector database, scientific search engine, or PDF parser.

## Lead liveness and platform migration

See [Lead liveness](docs/anix-lead-liveness.md), [migration](docs/anix-migration.md), and [current audit](docs/anix-audit.md). Canonical storage is `.anix/state.sqlite`. Existing legacy stores are moved as one directory on first open; historical artifact path spelling is resolved by a temporary migration alias. Inconclusive is a distinct stored outcome and is never counted as completed research.

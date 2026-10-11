# ANIX naming migration

Researcher's Notes remains the application. ANIX is the independent engine. The application package `rn-cli` and binary `rn` remain unchanged because they compose project management, Human controls, model bootstrap and evaluation as an application client.

| Previous crate / directory | Canonical crate / directory | Rust import |
|---|---|---|
| rn-domain | anix-domain | anix_domain |
| rn-core | anix-core | anix_core |
| rn-storage | anix-storage | anix_storage |
| rn-agent | anix-agent | anix_agent |
| rn-integrations | anix-integrations | anix_integrations |
| rn-evals | anix-evals | anix_evals |

Workspace dependency keys, manifests, lockfile, imports, examples, test paths, scripts and current architecture documentation use the new names. Update downstream Cargo dependencies and Rust imports according to this table. No old package alias is provided: the workspace has no published dependent packages or in-repository external client requiring one. Public type names (`Core`, `AgentApi`, `Workspace`, etc.) remain descriptive; names unrelated to the old engine brand were retained. `AgentApi::start_model_invocation` now takes the actual selected provider name for accurate provenance.

No database or artifact path migration is required. `.rn/state.sqlite`, persisted IDs, table names, checksums for SQL migrations 1-4, events, instruction versions in old snapshots, model profile IDs, `.runtime` paths and RN application environment variables are compatibility interfaces. Do not rename them to change the brand. Existing databases are opened by the same SQLite adapter. Current invocation instructions use `anix-lead-step-v1`; old immutable records retain their original version.

Historical reports retain the old package names and measurements as historical evidence. Follow README and anix-audit.md for current names/results. Shell and PowerShell bootstrap commands continue using `-p rn-cli`; evaluation examples now use `-p anix-evals`.

Windows CUDA regression launch:

```powershell
.\scripts\start-test-model.ps1 -GpuLayers 37 -RuntimePath "$PWD/.runtime/llama-b11307-cuda-build/bin/Release/llama-server.exe"
rustup run stable cargo run --target-dir target/verified -p anix-evals --example lead_runtime_seed -- branching 664 .runtime/anix-audit-test-lead.toml
.\scripts\check.ps1
```

The model launcher retains CPU defaults. GPU execution requires an explicit compatible runtime path and layer count. An occupied port is rejected rather than silently attaching to an unknown model process.

Human mutations now consistently require the recorded project creator, including direct knowledge execution. Existing records/read projections are preserved; previously unauthorized foreign-Human writes return PermissionDenied. Resolving the final Gate resumes unfinished bound WorkNodes rather than completing their Run prematurely. Unbound legacy task completion retains its prior behavior.

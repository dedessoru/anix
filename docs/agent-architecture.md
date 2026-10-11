# Agent architecture

`LeadAgent` (also exported as AgentService) receives an AgentApi capability, ModelProvider, and ModelSelector. AgentApi fixes the LeadAgent identity and exposes bounded context, own tasks/runs, structured proposal submission, safe usage metadata, and runtime completion. It exposes no database, repository writer, Core handle, or human decision method. Applications retain the human API separately.

ContextBuilder uses deterministic graph retrieval with a focus entity. Focused evidence is selected before truncation. Questions/hypotheses/active designs, claims/findings, graph neighbors, relevant pending gates, and recent events are bounded; long text is clipped on UTF-8 boundaries. Provider requests receive a context DTO rather than a database snapshot or connection.

Execution:

```text
goal -> Task -> Run -> bounded context -> model selection -> provider
     -> validated structured decision -> ActionEnvelope -> GatePolicy
     -> Gate + WaitingForHuman, or safe action + completed Run
```

A provider response is a schema-versioned Propose, NoChange, or UseTool decision. The runtime allows up to four model steps; tool results remain ephemeral request data and only registered read tools are available. NoChange returns an analysis summary. Semantic actions require a gate; only non-destructive References/RelatedTo relation creation is automatically executable. Task/run operations are automatic. Approval revalidates the entire action and every captured entity version, applies the action with the human actor, records the proposer through GateCreated provenance, records approval/application through the Gate ID, and finishes the waiting Run atomically.

ModelProvider uses an asynchronous boxed future so external adapters can implement it without tying Core to a runtime/provider. FixedModelSelector chooses a configured provider/model. FakeModelProvider consumes a deterministic response queue and records requests for offline tests. Provider selection/usage metadata is versioned and checked before persistence. Raw provider error text and model response text are not stored in Run errors.

ApprovedTools accepts explicitly registered read-only tool adapters. A model cannot register tools or grant itself write privileges. Tool and CodingExecutor are application ports in `anix-core`; integrations implement them. The supplied read_file adapter binds one ProjectId and Workspace, caps reads at 1 MiB, rejects workspace escapes and `.anix` access, and exposes no database handle. Consequential/external tool adapters require a future application-approved execution capability; V1's registry rejects them.

Workspace rejects absolute paths, parent/current-directory components, alternate data streams, and symlink/reparse escapes. Writes use create_new, retain file hashes, and never overwrite artifacts. Local processes changing directories concurrently are outside this library's capability boundary; an OS sandbox would be a separate execution adapter.

Real provider integrations, Codex execution, Python/search/scientific tool engines, complex routing, and orchestration of multiple agents are intentionally future work. The V1 runtime supports the complete offline research loop without any of those dependencies.

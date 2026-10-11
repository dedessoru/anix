# Human control and autonomy

This document describes the current application layer for project-level human control. It is a JSON API and CLI surface; no TUI is included.

## Autonomy modes

| Mode | Current policy |
|---|---|
| Manual | Agent proposals with meaningful impact wait at a Human Gate. Routine actions can proceed. |
| Supervised | Default. Existing consequential proposals wait at a Human Gate; safe reference relations may apply directly. |
| Autonomous | Agent proposals proceed without a gate, except experiment activation when its capability is not explicitly enabled and non-delegable archive actions. |
| Batch | Uses the autonomous proposal policy. Campaign budgets can be stored, but global runtime consumption limits are not yet connected to every executor. |

Policy is stored per project in SQLite. Human changes increment both the project control version and, for autonomy changes, the policy version. AgentApi exposes no autonomy or budget mutation. All gate resolutions retain the existing version check and exactly-once receipt behavior.

## Human controls

Core/Application API and the `rn research` CLI support autonomy changes, pause, resume, cancel, budget updates, work and branch priority, branch pause/resume/cancel, and activity/decision queries. Every mutation records a typed HumanDirective and an event/provenance record in the same SQLite transaction. Optional expected control versions reject stale commands.

Pause prevents new agent proposals and new work progression at the next command boundary. It does not interrupt an in-flight model call. Cancel is terminal: resume is rejected. Branch pause and cancel prevent new WorkNodes and transitions to Ready/Running on that branch. Priorities reorder the activity projection only; they do not change evidence or claim validity.

Hard constraints currently support typed source exclusion. The Core rejects use of an excluded source in proposed actions. Unstructured hard rules are rejected until deterministic validators exist. Soft constraints are persisted for the Lead to consume; they are not hard denials.

## Persistence and authorization

The `human_control` table was added in migration 4. Policy, directives, status, priorities, branch controls, budgets, and constraints survive restart. Commands require a Human actor and run through Core transactions. Project IDs scope all reads and writes. Existing gate authorization, stale-gate protection, and exactly-once approval remain in place.

Current CLI commands always emit JSON:

```text
rn --project <id> research status
rn --project <id> research pause|resume|cancel
rn --project <id> research autonomy manual|supervised|autonomous|batch
rn --project <id> research budget-show
rn --project <id> research budget-set --max-steps 150
rn --project <id> research branch-pause|branch-resume|branch-cancel <branch-id>
rn --project <id> research branch-prioritize <branch-id> high
rn --project <id> research explain <event-or-entity-id>
```

The API exposes `Core::human_control()`, `Core::research().activity()`, and `Core::research().decision_trace()` for future TUI/MCP clients.

## Current limits

Project budgets are versioned and persistent. `max_steps`, `max_lead_calls`, `max_tool_calls`, and `max_elapsed_seconds` are enforced at WorkNode creation and recorded model/tool invocation boundaries. `max_strong_model_calls` is reserved and enforced before the local Test Lead provider call; failed calls remain counted in durable ModelInvocation records. A Human can extend the limit through the versioned budget API. Branch-specific budgets are not implemented. Batch currently shares autonomous proposal semantics and is not yet a separate campaign scheduler. Decision trace returns related persisted records using IDs and rationale text; it does not yet build every cross-record causal edge. Restart during pause and a pending Gate, plus a multi-step HITL scenario with rejection, autonomy change, pause/resume, and restart, are covered by runtime tests.

All Human mutations, including direct knowledge execution, human-control commands and Gate resolution, require the project creator identity recorded by `ProjectCreated`. The shared command validator enforces this before applying changes; an unauthorized Human cannot bypass a pending Gate by directly executing its action. The project has no collaborator-grant API yet.

## ANIX audit clarification

Typed source exclusions are now enforced at retrieval/read boundaries and context compilation, including attached paper context. Soft and hard constraints are pinned in immutable context memory within the budget. `allow_branch_creation`, `allow_plan_revision` and `review_termination` are reserved fields: current code does not enforce these fields as additional gates. The documented run/task/project/parent authorization applies to branch creation. Consumers must not represent those reserved fields as implemented permission or termination guarantees. See [ANIX audit](anix-audit.md).

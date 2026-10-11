# Permission boundary regression

## Root cause

The real Test Lead branching trace selected `CreateWorkNode` and named
`research_report_retriever` as its required tool. That tool was not registered
for the project; the available local tools were `read_artifact_range`,
`read_source`, and `search_sources`. The model request schema accepted any
string as `required_tool`, and the composite request did not clearly
distinguish child tools that were available from actions the Lead could
execute. This presented an invalid action as a permission failure.

## Fix

- Model requests now expose registered child tools separately from executable
  tools, and process actions can be constrained for a specific step.
- Test Lead schemas enumerate registered tool names. `CreateWorkNode` is
  omitted when there are no child tools, and `LeadAgent` revalidates any
  process-action constraint before applying a decision.
- The long-horizon branching regression constrains the initial step to
  `CreateBranch`, then removes that temporary constraint.
- Search results expose validated `source_ids`; the read tool description and
  long-context prompt require copying those IDs and treating `entities[].id` as
  a different identifier space.
- Authorization denials carry a structured trace with actor, action, resource,
  project/branch/parent scope, capabilities, and reason. User-facing display
  stays generic, and the trace excludes credentials and tokens.

## Security evidence

Core branch creation still checks Lead identity, active run, task ownership,
project scope, parent branch, and checkpoint. No admin identity, authorization
bypass, or broadened production policy was introduced.

Acceptance coverage passes for an unauthorized actor, cross-project branch
creation, wrong-parent checkpoint, stale run, and a valid Lead-created child
branch that survives restart. Stale gate rejection remains covered.

## Verification

The baseline `scripts/check.ps1` run passed before this iteration's evaluation
assertion and tool-failure scenario changes: 76 ordinary tests passed, 2 were
ignored, 2 doc tests passed, and formatting/clippy passed. The follow-up full
check result and current capability campaign are recorded in
[the 2026-10-06 regression report](true-capabilities-regression-2026-10-06.md).

## Historical mixed-campaign issue

The prior 20-run campaign (seed 650, run on 2026-10-05) had two long-context
capability failures: seed 654 passed a question entity ID to `read_source`,
while seed 664 searched repeatedly without reading an offloaded source. Both
preserved Core integrity and restart recovery. This is historical evidence;
the current exact-seed rerun and any subsequent focused fix are tracked in the
2026-10-06 report. Neither issue was an authorization denial, and neither
justifies relaxing authorization.

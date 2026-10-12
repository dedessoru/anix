# P0 V2 — реестр исходных дефектов

Сопоставление с Astra выполнено по всем 50 файлам source manifest: LF-normalized digest совпал. Поэтому исходные CODE/STATIC CE основания F01–F22 относятся к актуальному содержимому кода. Номера строк не используются как замена символам. Этот реестр **не** объявляет все контрпримеры исполненными и не закрывает ни один дефект.

| ID | Актуальные owners / anchors | Свидетельство P0 | Целевые проверки и этап |
| --- | --- | --- | --- |
| F01 | `long_horizon::run_one_with_policy`, `LeadAgent::run_step`, CLI `ResearchCommand` | CODE: source snapshot совпадает; CLI help подтверждает controls без общего start/tick | T01/T07/T29, P2 |
| F02 | Agent `step_contract_ok`; provider `step_response_schema` | EXEC-RUST: корректный scientific Propose отвергнут до Core; тот же action проходит Core/Gate/approval | T01–T03, P1/P2 |
| F03 | Agent required-tool preflight; `validate_work_completion`; `finish_research_with_branch_reviews` | CODE/STATIC CE; новое no-search/no-tool воспроизведение не выполнялось | T02–T04, P2 |
| F04 | `OutputContract`, `required_tool_output_exists`, `validate_work_completion` | EXEC-RUST: compute v1 удовлетворяет owner-revised objective v2 при том же WorkId | T05/T06, P1/P2 |
| F05 | `decision_state`, `satisfied`, `transition_work`, `finish_research` | EXEC-RUST: incoming Blocks запрещает direct transition, отсутствует в readiness и не мешает Inconclusive finish | T06/T08/T24, P1/P2 |
| F06 | `HumanControlState.status`, `transition_work`, `finish_research` | CODE/STATIC CE; новые lifecycle/reopen тесты не выполнены | T09/T10, P3 |
| F07 | `RecordedTool` preview, harness observation packing, eval-only `ReadArtifactRangeTool` | CODE/STATIC CE; helper/storage не считается доказательством доступа Lead | T07/T11/T12, P2/P5 |
| F08 | `compile_context_manifest`, `ContextBuilder::build_for_snapshot`, provider request rendering | EXEC-RUST: immutable snapshot v1 сохраняется, typed context содержит current v2 | T13/T14, P1/P2 |
| F09 | memory selection в compiler; `ProcessMemory` без source dependencies | CODE/STATIC CE; derived-memory exclusion не проверен новым тестом | T15/T16, P2/P4/P5 |
| F10 | admission/record paths без общего RelevantBasis | CODE/STATIC CE; исправленные source-specific guards сохраняются | T14/T17/T18, P2/P4 |
| F11 | `create_child_work_with_repeat`; последовательность atomic completion в Agent | EXEC-RUST для AddChild: второй commit падает, orphan durable, retry создаёт новый child | T19/T20, P1/P2 |
| F12 | `create_branch`; branch-result review в `finish_research_with_branch_reviews` | CODE/STATIC CE; новый fork/review version тест не выполнен | T10/T21/T22, P3 |
| F13 | `AutonomyPolicy` flags; `policy_requires_gate` | CODE/STATIC CE; baseline guards/owner tests PASS, enforcement всех flags ещё отсутствует | T23–T25, P3 |
| F14 | provider `generate` retries/repair; `start_model_invocation` reservation | EXEC-RUST + независимый loopback HTTP journal: cap=1, два sends, одна invocation row | T26/T27, P2/P4 |
| F15 | `RecordedTool`, `durable`, `resume_checkpoint` | CODE/STATIC CE; unknown-effect осторожность сохраняется; consequential replay не исполнялся | T17/T18/T28, P2/P4 |
| F16 | scientific entities/Relations, current-head updates, `EvidenceTrace` | CODE/STATIC CE; нет новой versioned Assessment suite | T03/T16/T30/T31, P2/P5 |
| F17 | `promote_candidate(source_text, source_quote)` | CODE/STATIC CE; trusted Human boundary не называется model exploit | T12/T30, P5 |
| F18 | Domain `derive_retrieval_batches`, Core `has_pending_retrieval` | CODE/STATIC CE; конкурентный CE ещё не исполнялся; последовательные regressions PASS | T32/T33, P3 |
| F19 | `current_activity`, `executable_children`, replicated `WorkBudget` | CODE/STATIC CE; priority/fairness и scope budget campaign не выполнены | T27/T34, P3 |
| F20 | Storage `read_state`/`transaction`, runtime `revision` | CODE: структура роста; latency/OOM деградация не измерена и не утверждается | T35/T36, P4/P5/P8 |
| F21 | `persist_artifact`, Workspace `write_new`, offload `read_range` | CODE/STATIC CE; **896 существующих baseline artifacts verified**, что не исключает declared-missing-file CE | T12/T28/T36, P2/P4 |
| F22 | acceptance manual chain; harness interventions/cleanup, capability oracle | CODE/STATIC CE; passing baseline не объявляется scientific acceptance | T01/T29–T31/T37–T40, P1/P2/P5–P7 |

Новые диагностические fixtures покрывают I03/I04/I05/I12/I17/I22 в ограниченном scope и готовят T05/T06/T08/T13/T19/T26. Это не выполненная финальная T01–T40 acceptance suite. Production изменений нет; все F остаются открыты до соответствующего системного исправления и positive/negative приёмки.

# ANIX P0 — аудит текущего репозитория

Дата: 11 октября 2026. Задание: `anix-stage1-integrated-architecture-plan-v1.1.md`, §8. Production source, migrations, prompts, profiles, policies, budgets и существующие tests не менялись. Добавлены только этот audit bundle и внешние диагностические программы.

**Статус: P0_INCOMPLETE для полного объёма §8. BASELINE_ACCEPTED: YES для существующих deterministic workspace checks; NO для готовности полного research/product path.** Один новый counterexample воспроизведён. Полная проверка поздних external effects, всех consumers и доставки arbitrary-source ranges не выполнена; по этим границам дальнейшие архитектурные выводы не разрешены. Это отчёт о выполненной работе, а не утверждение, что весь P0 или двигатель принят.

## Основные результаты

1. Текущий checkout успешно проходит format/check/Clippy/build, **183 ordinary tests + 3 doc tests**, 0 failures, 2 ignored. Старое число 177 не является результатом текущего запуска.
2. Новый внешний probe через настоящий Core/SQLite воспроизводит **false Blocked**: родитель принимает `Blocked`, хотя обязательная дочерняя работа исполнима и supplied-source handle `E1` доступен. Состояние сохраняется после повторного открытия DB. Это нарушение I13/I23 относительно нового целевого контракта, а не доказательство неправильности всех прежних Blocked.
3. Три флага `allow_branch_creation`, `allow_plan_revision`, `review_termination` объявлены/сериализуются, но не используются в проверках. `allow_experiment_activation` имеет реальный consumer. Исправления на P0 не выполнялись.
4. Существенный same-objective replan **уже существует**: `propose_work_plan` → `ReviseWorkPlan` → owner Gate → новый граф steps/dependencies. Утверждение «модель не может предложить смену стратегии» для текущего кода неверно. Делегированная автономная ревизия без Gate отдельно не реализована.
5. Сохранение child summaries, isolated branch results, supplied-source чтение без search, metadata validity guards и first branch dispatch после rejections покрыты существующими тестами. Переписывать эти механизмы из-за нового названия контракта не требуется.
6. Есть verified artifact byte-range helper, но нет соответствующего memory/result range reader в compact Lead меню. `read_source` в long-horizon fixture читает `Paper.abstract_text`; это не arbitrary PDF/full-source parser.
7. Общий Core владеет записями, но полный driver, private retrieval adapters, подача source observations в memory и переключение research scope находятся в `anix-evals`. CLI имеет offline Agent demo, а не такой же произвольный проект → исследовательский результат.
8. **ReportMismatch:** hashes `long_horizon.rs` и `test_lead.rs` не совпадают с manifest iteration13. Hash старого runner совпадает. Поэтому сохранённые модельные исходы — исторические evidence, не live acceptance текущих исходников.

## Baseline, команды и неизменность

Workspace: `C:\Users\leks\Documents\RN\anix`. Git: `fef6518f76fd0654dc2e1707f48adaa27530394c`, branch `main`. В начале application tree чистый; файл нового плана untracked. В момент фиксации environment также появились audit files. Applicable `AGENTS.md` в workspace и проверенных родительских каталогах не найдены. Existing mission/resume прочитаны как checkpoint; новый §8 определяет текущий read-only P0 scope.

Windows 11 build 26100, stable MSVC Rust/Cargo 1.96.0; точная compiler identity в [environment.json](environment.json). CPU AMD Ryzen 5 3600 (6 cores/12 logical), RAM 34,281,345,024 bytes, NVIDIA RTX2060; driver 32.0.16.1692. VRAM в этой сессии отдельно не измерена: исторические 12GB не выдаются за новое измерение.

`scripts/check.ps1` прочитан до запуска. Он использует `fmt --check` и компилятор из stable rustup, но не добавляет `--locked` и прекращает цепочку на первом failure. Для P0 использован безопасный эквивалент: одинаковые `RUSTC`, `RUSTDOC`, `RUSTUP_TOOLCHAIN`, PATH; отдельный `target/p0-audit`, `--locked`; каждая команда сохраняет независимые stdout/stderr/exit code. Это **не точный повтор исторического PowerShell wrapper**.

```powershell
python reports/p0-2026-10-11/collect.py
python reports/p0-2026-10-11/inspect_evidence.py
python reports/p0-2026-10-11/finalize_evidence.py
python reports/p0-2026-10-11/trace_diagnostics.py
python reports/p0-2026-10-11/matrix.py
```

| Проверка | Команда Cargo, общий toolchain выше | Exit | Результат |
| --- | --- | ---: | --- |
| Formatting | `fmt --all --check` | 0 | PASS |
| Compilation | `check --workspace --locked --target-dir target/p0-audit` | 0 | PASS |
| Clippy | `clippy --workspace --all-targets --locked --target-dir target/p0-audit -- -D warnings` | 0 | PASS |
| Ordinary/doc tests | `test --workspace --locked --target-dir target/p0-audit` | 0 | 183 + 3 PASS; 2 ignored |
| Build / examples | `build --workspace --examples --locked --target-dir target/p0-audit` | 0 | PASS; fresh runner создан |
| Workspace metadata | `metadata --locked --no-deps --format-version 1` | 0 | 7 crates |

Evidence: [checks.json](checks.json), [build.json](build.json), [test-results.json](test-results.json), `*.stdout.log`, `*.stderr.log`. Test list содержит actual symbols и results. `crash_child_fixture` ignored в обычном перечислении, но **запущен существующим `crash_process_checkpoint_resume` как subprocess**, ожидаемый exit 73; родительский тест PASS. `real_llama_smoke_missing_bad_model_concurrency_shutdown` явно ignored: требует manifest-verified model и конкретный llama runtime, не обычный CI. Он не запускался; никакой Test Lead успех не подменяет этот smoke.

[source-before.json](source-before.json) и [source-after.json](source-after.json) включают 73 application/config/migration/test/script/model-profile файла. [source-comparison.json](source-comparison.json): `unchanged=true`, `changed=[]`, включая Cargo.lock. Audit files и build output отделены от этого manifest. Dependencies не обновлялись.

## Инвентаризация и связь с историей

Фактические crates: `anix-domain`, `anix-core`, `anix-storage`, `anix-agent`, `anix-integrations`, `anix-evals`, `anix-cli`; executable называется **`anix`**, не `rn-cli`. Cargo targets, examples, feature flags и прямые dependencies сохранены в [workspace-inventory.json](workspace-inventory.json); полный Cargo.lock hash и bytes в source manifest. `build.rs` в application tree не обнаружен. Root `tests/acceptance.rs` и `tests/runtime_v2.rs` подключаются как integration targets пакета `anix-storage`; они действительно вошли в workspace test run.

Domain — pure types/policy/refs. Core — application services и Storage port. Storage → Core port + rusqlite. Agent → Core/Domain. Integrations → Core/Agent/Domain и transport/files. Evals → реальные adapters/runtime плюс fixtures. CLI → composition всех этих пакетов. SQL/HTTP не перенесены в Domain.

Фактических migrations **пять**: 0001 initial, 0002 runtime, 0003 context/retrieval/telemetry, 0004 human control, 0005 work outcome. Storage проверяет checksum applied migrations, отвергает newer schema, сохраняет отдельные canonical tables и append-only audit. Следующий номер нельзя выбирать исходя из старого предположения о четырёх migrations.

Пользовательская `.anix/state.sqlite` открыта Python SQLite **read-only** и скопирована backup API, включая WAL consistency, в `isolated/state.sqlite`. Все диагностические чтения integrity/schema/counts выполнены на копии. В ней **207 projects**, migrations 1–5, `PRAGMA integrity_check=ok`. Copy DB hash отличается от original file hash, что ожидаемо для logical SQLite backup и не означает row mismatch. Evidence: [isolated-db-inventory.json](isolated-db-inventory.json). Production DB не открывалась приложением для экспериментов. Полного восстановительного копирования её referenced artifact bytes не выполнено; DB integrity не означает DB+artifact restore.

Runtime configs 8B/14B скопированы в `isolated/config` и hashes сравнены с историческим manifest. Local weights доступны по путям/размерам в [runtime-assets.json](runtime-assets.json); **weights не перехэшированы**, runtime не запускался и health/model identity не проверялись. Live 654/664 в этой сессии NOT RUN: для анализа использованы допустимые §8.4 frozen raw traces; profile qualification не выполнялась. Никаких remote/paid calls, скачивания моделей и увеличения budgets.

[historical-checkpoint-comparison.json](historical-checkpoint-comparison.json) сопоставляет старые абсолютные пути `researcher-s-notes-core` с новым workspace по relative path, без переименования исходников:

| Связь | Факт |
| --- | --- |
| Core runtime и runtime_v2 tests | hashes совпадают с session13 |
| Evals long_horizon и Integrations test_lead | hashes **не совпадают**; ReportMismatch |
| Retained session13 runner | hash совпадает: `6b0981edea62efc5a6f64ad1dc5ef86233904062a833dc456906c14f02806bfe` |
| Session13 check stdout/stderr и оба profiles | hashes совпадают |
| Fresh current-source runner | `608b29dcbe285dc35d9598974fb514a852ba03420d25a39caafcd00934de59f9`; не запускался на модели |

Следовательно, preserved old inputs/schema/decisions можно анализировать, но нельзя объявить old run execution результатом newly-built runner. Причина каждого изменения исходников после session13 в Git history отсутствует: текущий Git начался одним import commit. Before-failure logs — историческое evidence, не newly reproduced before-state.

## Четыре causal traces

### T-A: supplied source → чтение → child result → parent frame → outcome → restart

Actual definitions: `Core.create_project/add_source/add_paper/create_work` [Core](../../crates/anix-core/src/lib.rs), `LeadAgent.run_step` line653 → `AgentApi.work_source_handles` line1523 → `RetrievalProtocol.executable_tools` Agent line133. Actor/identity принадлежит host; модель получает `E1`, а `runtime_source_ref` runtime перезаписывает своим SourceId. Read не требует search.

`supplied_source_is_read_without_search_through_the_lead_boundary` (Evals line2487) создаёт отдельную temp DB/Workspace, настоящий Core/RecordedTool и scoped private `ReadSourceTool`. Первый Fake Lead turn читает source; второй CompleteWork с substantive summary → `validate_work_completion` Core line2196 → shared `usable_required_output` line1471 → `transition_work` + `record_work_result` line2293. Bytes пишутся Workspace до Artifact registration, invocation begin/end атомарно записываются в разных коротких транзакциях; tool I/O вне transaction. Сохранённый summary явно **unreviewed Lead task result**.

Parent route: `decision_state` runtime line247 выбирает succeeded children и их Work result memory; `compile_context_manifest` line911 фиксирует versioned snapshot, pinned Human constraints и immutable ProcessMemory records; `provider_context` line872 → `ContextBuilder.build_for_snapshot` → ModelRequest → provider actual request. `parent_context_receives_completed_child_results_without_sibling_branch_memory`, `child_result_previews_preserve_pinned_question_and_full_persisted_results` PASS проверяют parent delivery/pressure в своих отдельных fixtures.

Final route: `AgentApi.finish_research_with_branch_reviews` line2040 → current readiness + actual read outputs + review set → ResearchTerminated + work.outcome в transaction. Existing `complete_question_to_finding_and_restart` и `complete_v2_research_runtime_end_to_end` PASS проверяют result/restart в своих сценариях.

**Предел:** это связанная source-level трасса с несколькими воспроизведёнными tests, не один новый supplied-source+parent+assessment+scientific-certificate episode. В supplied fixture материал хранится в paper.abstract_text. Required material delivery/Addressed/certificate ledger отсутствует; full combined S02/S05 episode NOT RUN. Нельзя суммировать эти тесты в доказательство всего T00–T16.

### T-B: стратегия → replan → две ветви → reconciliation → outcome

Goal refs в `WorkNode.acceptance_criteria`; strategy proposals в `WorkPlanStep`/`ReviseWorkPlan`. Agent compact `propose_work_plan` → Core submit_proposal line476 → scope/run ownership → expected work/plan versions → owner Gate → `actions.apply_work_plan` line64. Existing work reuse сохраняет состояние; retired unsucceeded tactics становятся Superseded с audit; new steps копируют protected references и зависимости. Это не stable independent obligation coverage ledger.

`same_research_objective_can_revise_executable_strategy_through_owner_gate`, `strategy_approval_rechecks_revision_and_does_not_mutate_on_staleness`, `approved_plan_dependencies_become_executable_after_prerequisite_completion` PASS. Branch route: Core create_branch line1602 → checkpoint/base/revision → Agent scope selection → actual branch child tools → independent FinishResearch → `BranchResultAvailable` exact source work/branch/target + summary/artifacts → parent frame → reviewed_branches → `BranchResultReconciled` → parent termination.

`independent_branches_execute_return_results_and_reconcile_through_real_driver` и **`seed664_reference_reconciles_two_evidence_paths_without_inventing_unique_causality`** PASS на текущем source. Второй использует corpus664, scripted ReferenceEvidenceLead, 2 evidence branches, reconciliation, reopened SQLite и финальный summary с ограничением «не unique causality». Доказано достижимое scripted execution в synthetic driver; не свободный Lead, не scientific review и не arbitrary-project CLI acceptance.

Отличие historical14B664: после одного branched path и его Inconclusive модель возвращается к parent и выбирает Blocked вместо создания/исполнения main-path child. Parent minimum_children=1 ещё не удовлетворён. Ни scheduled private driver, ни Core не создали скрытую вторую evidence branch для достижения PASS.

### T-C: authority/Gate → resume → recovery/control → ledger

Proposal → `policy_requires_gate` human_control line63 → Gate expected_versions + checkpoints → owner approval в Core line602 → execute_action validation → `resolve_waiting_run` line1322. Bound Ready/Running work получает RunResumed/TaskResumed, а не work success. Legacy unbound gate-run может Completed: это отдельная семантика, не certificate результата.

Reproduced fixtures: `non_owner_cannot_bypass_a_pending_gate_through_direct_execution`, `pending_gate_and_waiting_status_survive_restart_and_approve_once`, `gate_boundaries_are_atomic_and_pin_run_revision`, `failed_gate_checkpoint_commit_rolls_back_all_records`, `human_pause_blocks_new_agent_progress_and_directives_are_project_scoped`, `crash_process_checkpoint_resume`, `recorded_model_failure_and_success_survive_restart`. Crash fixture делает process exit73 без destructors; reopen → resume_checkpoint line679 восстанавливает plan/context descriptor и не вызывает provider. This is process-crash recovery, not power-loss durability.

Model/tool execution route: Running invocation → short Core transaction → provider/inner tool outside transaction → bytes/terminal metadata → next transaction. Running model call считается в budget до send и после failure/restart; one-time pending model identity разрешена, поздняя retagging/strong-route drift запрещены.

**Missing end-to-end evidence:** cancel между admission/send и late/duplicate authenticated receipt, independent external fake effect journal, attempt-level unknown liability/stream reconciliation. Operation/outbox/lease/EffectUnknown не обнаружены в reviewed types/schema. No-resurrection полного semantic epoch не воспроизведена. Точная минимальная будущая reproduction: accepted fake operation; fake journal records effect; lose ack; cancel/restart; receive same success twice; проверить один logical result, все real costs, отсутствие semantic resurrection. Experiment NOT RUN: отсутствует требуемый operation/intake contract; P0 его не добавлял.

### T-D: frozen 654/664 divergence и новый Core counterexample

Original reports/JSONL доступны; existence/hashes сохранены в [historical-run-inventory.json](historical-run-inventory.json). [diagnostic-points.json](diagnostic-points.json) содержит turn/invocation/work/snapshot/plan IDs, actual parsed action, verdict, handles, child results и raw completion. [frozen-diagnostic-summary.json](frozen-diagnostic-summary.json) — небольшой индекс с hashes serialized stored provider messages/schema. Это hashes представления сохранённой JSON-трассы, не raw HTTP wire bytes и не inference replay.

**8B654**, run `20261010T081048725`, 14 turns: completed child results уже находятся в следующем provider user message; turns11–14 повторяют existing work. После четвёртого no-progress шага **harness idle guard**, а не explicit Lead termination, ставит report.status=Blocked. `completion.ready=true` на final parent; actual completion choices включают Completed. В этой frozen точке гипотеза «никакого результата во входе» не объясняет stop. Здесь особенно виден I23: strategy stall представлен как Blocked. Содержательная достаточность и влияние truncation не доказаны этим фактом.

**14B654**, run `20261010T081319661`, 6 turns: supplied corpus search/read (включая offload), completed child, explicit Completed с source-backed synthetic summary. Это preserved исторический успешный workflow, не новый model run/independent science acceptance.

**14B664**, run `20261010T081513351`, 9 turns:

| Turn | Input / decision / persisted consequence |
| --- | --- |
| 1–2 | create_branch `Pathway Analysis` → switch B2; parent ещё без succeeded child |
| 3 | B2 creates read_source task для pathways P/Q |
| 4–6 | search_sources → read S1 → read S2, оба actual fixture contents |
| 7 | CompleteWork summary уже добавляет требование выбрать **more likely** mechanism, признавая оба pathways possible explanations |
| 8 | B2 FinishResearch Inconclusive; readiness true; Core seals result и пишет BranchResultAvailable |
| 9 | Parent input содержит returned B2 summary и `minimum completed children unmet`; модель FinishResearch Blocked с reviewed B2; Core принимает. Main branch не исполнена |

Первый наблюдаемый scope escalation — turn7: original question «could explain» против требования определить более вероятный mechanism. Это attribution по frozen input/response, **не причинное доказательство**, что одной prompt-правки достаточно. Отдельно branch capability требует два completed evidence paths; это oracle requirement в Evals `branch_capability`/`capability_assertion_failure`, не protected stable obligation ledger Core. Original requirement и независимый научный oracle нужно adjudicate; нельзя relabel old failure.

**8B664**, run `20261010T075935935`, 24 turns, — iteration12. Не substitute для отсутствующего iteration13/current-source8B664.

Новый probe [probe.rs](probe.rs) не меняет приложение/тесты, не использует fixture DB shortcut и не исполняет внешний эффект. Он через Core создаёт isolated project/source/root, actual AgentApi child, показывает `executable_child_work` и `E1`, затем просит Blocked и повторно открывает SQLite. Expected от I13/I23: reject structural false Blocked. Actual в [false-blocked-probe.json](false-blocked-probe.json): accepted=true, parent=blocked, child=ready, restart_equal=true. Путь transaction тот же, что у обычного API.

Compiler command для reproducer (использует fresh workspace build artifacts):

```powershell
$probeRustc = (& rustup which --toolchain stable rustc).Trim()
$probeArgs = @('--edition=2021','reports/p0-2026-10-11/probe.rs','-L','dependency=target/p0-audit/debug/deps','-o','reports/p0-2026-10-11/probe.exe')
foreach ($crateName in @('anix_core','anix_domain','anix_storage','serde_json','tempfile')) {
    $crateLibrary = Get-ChildItem -LiteralPath target/p0-audit/debug/deps -Filter "lib$crateName-*.rlib" | Select-Object -First 1
    $probeArgs += @('--extern', "$crateName=$($crateLibrary.FullName)")
}
& $probeRustc @probeArgs
if ($LASTEXITCODE -ne 0) { throw 'Diagnostic build failed' }
& reports/p0-2026-10-11/probe.exe
```

Compile и execution exit0; это **успешное воспроизведение нарушения**, не PASS целевого инварианта. Result знает только structural availability; scientific capability/remote provider availability не выдумана.

## Shared и duplicated semantic owners

| Семантика | Canonical writer / consumers | Вывод |
| --- | --- | --- |
| Application state/history | Core.mutate → SqliteStorage.transaction line767 (BEGIN IMMEDIATE) → rows/events; typed services/AgentApi | Один canonical writer; no callback I/O у рассмотренных transport calls |
| Work output validity | Core.usable_required_output → required_tool_output_exists / validate_work_completion / decision evidence counts | Shared metadata guard; не verifier bytes или entailment |
| Retrieval state | Domain.derive_retrieval_batches line354 → Agent/Evals; Core.has_pending_retrieval runtime line412 отдельно восстанавливает pending по metadata; Evals RetrievalState cache | **Несколько derivations**, разные inputs/scope. Existing lifecycle tests PASS; полного equivalence conformance между всеми тремя нет |
| Tool availability | Agent.RetrievalProtocol.executable_tools + handle checks; schema renderer; direct Core public methods | Shared Agent offered-tool set защищает read witnesses; не общая Core admission ActionView |
| Closure/readiness | Core.decision_state / finish_research; schema.finish_outcomes; Evals branch_capability и idle stop | Не единая полная semantic owner. Blocked bypass воспроизведён; Evals criterion сильнее row-level Core branch contract |
| Result delivery | Core decision_state/compile_context + Evals write_process_memory после tool_output | Bytes stored ≠ Packed/Sent/Addressed. Mandatory handoff cursor отсутствует |
| Controls | Core HumanControlState/guards; CLI services; Evals explicit synthetic Human resolver | Mode/owner/experiment gates работают; три named flags без consumers; test Human approvals не автономность модели |
| Budget/accounting | Core invocation counts; adapter retry metadata; Evals recovery_calls/idle/campaign budget; DurableRouter routing_usage | Real failed calls accounted at invocation level; полного reserve/stream/unknown liability ledger нет |
| Recovery | Canonical rows/checkpoint descriptor; Workspace generated projections | Event-only rebuild и external exactly-once не обеспечены этой проверкой |
| Micro | Six public typed tasks → IntelligenceRouter → local validate/return; RecordedRuntime/DurableRouter telemetry | Semantic local outputs допустимы старой routing policy. No StrictResearch version; no live consumer доказан в CLI/long_horizon main loop; external consumers Unknown |

## Preserved fixes, hypotheses и MissingEvidence

Воспроизведённые текущие regressions A перечислены с actual symbols/results в test-results.json: prerequisite matrix, supplied-source no-search, pending/partially-consumed/refine/reject lifecycle, source exclusion/archive/project scope, pinned constraints, scoped parent results, false acceptance metadata, invocation identity/strong failure accounting, non-owner Gate bypass, loopback redirects, file/storage aliases, trace bounds/redaction, workspace-bound tools, crash/checkpoint/v1 upgrade/CLI.

Historical F before failures действительно доступны: `.runtime/anix-v1-result-visibility-before.log`, `anix-v1-branch-reviews-before.log`, `anix-v1-branch-recovery-before.log`; для visibility есть corresponding after log. Current exact tests PASS: `child_result_previews_preserve_pinned_question_and_full_persisted_results`, `branch_catalog_exposes_only_executable_scopes_and_requires_review_handles`, `first_branch_dispatch_after_rejections_can_recover_without_false_research_progress`, `repeated_branch_switches_do_not_disable_the_stall_guard`, `reusing_completed_work_does_not_reset_the_stall_guard`. Before source was not reconstructed/executed; full original provenance remains historical.

Отдельные hypotheses: all-consumer retrieval derivations могут расходиться в редко встречающихся histories; adapter metadata может дать technically-valid output без usable bytes; stale read/late-result acceptance может обойти цель без общего fence; local schema validation может принять семантически неверный alias/summary. Это **не новые reproduced bugs** и не основание сразу переписывать runtime. На P0 только false Blocked получил новый executable counterexample.

MissingEvidence/NOT RUN для полного G0:

- Полный provenance current-vs-session13 diff: manifest hash mismatch установлен, но история преобразований и full exact old source/binary mapping отсутствуют в Git. Historical logs/runner существуют; current linked binary создан отдельно.
- Один полный supplied-source → parent actual provider input → attributed assessment/required-use → scientific acceptance → restart эпизод с независимым oracle, вместо объединения отдельных tests.
- Required offloaded old negative result/middle/end/footnote ranges → reachable reader → exact next input/use under pressure. Существование read_range helper и newest summary недостаточно.
- Cancel/lost ack/late/duplicate receipt trace с **независимым external journal**, explicit epoch и attempt liability; stronger contract нельзя реализовать временным patch на P0.
- Полный граф downstream consumers публичных intelligence ports вне репозитория, active operator/cache/UI noninterference и измеренная полная request/billing стоимость, включая всех failed repair attempts.
- Independent original-source/scientific oracle и minimum second-path requirement adjudication 664. Synthetic reference sentence задаёт scripted ожидаемый ответ, не независимую scientific truth.
- Полный current DB+artifact backup/restore и repeat historical 207+2 row comparison. Есть main DB copy с207projects; два historical projects и сохранённые compatibility claims не превращены в новый209PASS.
- Свежие live model trials, measured current weights hashes/server health/tokenizer/runtime/VRAM. Они здесь NOT RUN, не неизвестный «успешный» профиль. Exact six appendix bytes из §1.2 и их cited source hashes отдельно не переданы; local A/F доступны с другими именами и не объявлены автоматически идентичными приложениям.

Secrets не включались намеренно. Полные original traces и raw local-only diagnostic copy остаются в workspace и исключены local `.gitignore`; публикуемые derived points содержат synthetic fixture text. Retained trace redaction/size limits могут исключать byte-exact replay; stored message/schema hashes не устраняют это ограничение. Isolated user DB/config copies не публикуются.

## Gap matrix и корректировка P1–P8 для этапа3

[gap-matrix.md](gap-matrix.md) и [gap-matrix.json](gap-matrix.json) содержат **все I01–I26 и M18–M24**: owner, definitions/callers/consumer chain, basis, inspected/reproduced behavior, expected contract, coverage, classification/limits, proposed minimal change, cross-component impact и полный system acceptance. `ImplementedAndReproduced` относится только к указанному существующему subset, а не к universal invariant. Missing/Unknown не повышаются до proved defect.

| Этап | Удалить как уже существующее / сохранить | Изменить, перенести или заблокировать |
| --- | --- | --- |
| P1 | Семь crates, три графа, canonical rows+audit, existing work/plan/refs/version checks | Уточнить obligation protection/MethodAnswer mapping, Blocked/frontier/technical stop, handoff/reader, requirement origin664. Не утверждать все новые types заранее |
| P2 | Supplied E-handles, shared usable-output metadata guard, pinned constraints, substantive child summaries | Нужен полный public product path и actual required result reader/delivery; использовать existing offloader/records. Single combined API+CLI E2E ещё не принят |
| P3 | Same-objective work-plan proposal, dependencies, Gate staleness, independent branch selection/review/return, dedup/repeat guard | Scope delegation/coverage transfer/final-vs-partial и root frontier вместо blanket replan rewrite. Не ослаблять second-path oracle до adjudication |
| P4 | SQLite atomic rows+history, call reservation/counts, owner Gates, checkpoint/resume, workspace write+hash reader | Qualify outbox/unknown effects/epoch, explicit three flags, reserve/liability ledger и DB+artifact restore. New migration после **0005**, historical migrations неизменны |
| P5 | Existing scripted synthetic fixtures/branch smoke | Добавить позднее method-specific independent fixtures/mutation oracles и arbitrary ranges. Existing abstracts не объявлять PDF/full-text reader |
| P6 | Preserved raw654/664 evidence и current scripted664 success | Live gates blocked до различения source versions и contract/delivery/oracle gaps; не Mixed20/200 на P0 и не ranking8B/14B по одиночным traces |
| P7 | Typed CLI services и3integration tests | Product research loop отсутствует на CLI. General driver mechanics отделить от scenario forcing/Human fixture approval; полноценный новый проект обязателен |
| P8 | Existing optional routing assets/adapters и Micro OFF long-horizon composition | StrictResearch policy/noninterference квалифицировать отдельно. Apple bridge не строить без measured workload; zero useful local tasks — допустимый итог |

Production patches, P1–P8, миграции, изменение flags/микромодели или benchmark не выполнялись. Выполненный audit bundle передаётся пользователю для разбора этапа3; внешних сообщений Astra или другим людям не отправлялось. По критическим Unknown/NOT RUN этот bundle остаётся **P0_INCOMPLETE**, перечисляет конкретные границы и не выдаёт частичный evidence за полный G0.

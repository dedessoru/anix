# ANIX Core V2 — отчёт P0, 12 октября 2026

**Этап P0 завершён. Gate G0: PASS для baseline и исполняемой диагностики. P1 не начат.**

Это решение означает, что исходники и binary установлены, baseline выполнен на неизменённом source, обязательные дефекты воспроизведены и сохранены, доступные данные имеют проверенный backup. Оно не означает исправление архитектуры или научную готовность ANIX. Шесть тестов целевых контрактов остаются **FAIL**; их ошибки являются результатом диагностики, а не зелёной приёмкой будущего runtime.

Нормативная спецификация: `ANIX_CORE_REBUILD_PLAN_V2.md`, версия 2.0, 1977 строк; изучены все §1–22, включая контракты, I01–I24, T01–T40, S01–S12, P0–P8/G0–G8 и ограничения модели. SHA-256 документа: `4d04c3f1ea2294b41e7f114b033a978f6430b73bd3f5c1406af2fbef488a5db3`. Предыдущий отчёт `reports/p0-2026-10-11` сохранён как исторический материал, а его старое P0_INCOMPLETE не перенесено автоматически на новые критерии V2.

## 1. Source, Git и binary

| Проверка | Результат |
| --- | --- |
| Git HEAD | `fef6518f76fd0654dc2e1707f48adaa27530394c`, Initial import of anix |
| Исходный dirty state | Untracked: V2 plan, прежний architecture plan, RAP, Zero-Loss, `reports/`; tracked diff пуст |
| Workspace | Семь crates, `anix-cli` / binary `anix` |
| Source manifest | 50 файлов: 35 Rust, пять SQL migrations, семь crate manifests, root Cargo.toml/lock/toolchain |
| Astra digest, LF | `a68937c8037a48ba71171194772701a94f2205358af417a0299e2aedd963974e` |
| Current digest после CRLF→LF | **Точно совпадает с Astra** |
| Current raw-byte digest | `473bb5766623c7317a8c593c8779b45add18e78e9350a1192eb70bb1b604eacb` |
| Git HEAD vs working source | Все 50 LF-normalized файлов совпадают |
| Source до/после P0 | Hashes всех 50 файлов неизменны, changed=[] |
| Binary | `target/p0-audit/debug/anix.exe`, Cargo workspace build exit 0 |
| Binary SHA-256 | `9944fb25d3ad4cca110d7ffae21defb078f0c1c14cc1fa8cdc9c3156fec65c1e` |

Первичное несовпадение manifest оказалось отличием Windows CRLF от LF. Нормализация применялась **только при вычислении сравнительного digest**; исходные файлы не переписывались. Дополнительно сравнен нормализованный Git object для каждого пути. Это устанавливает соответствие содержимого всего исследованного source manifest, а не только похожие числа файлов/тестов.

Сам `anix-main.zip` в workspace/родительском каталоге не найден; его заявленный ZIP hash и полные 1258 файлов архива заново не проверены. Не утверждается побайтовая идентичность всего дерева архиву. Наличие `.git`, `.runtime`, `.anix`, моделей и новых документов — дополнительные локальные материалы. Содержательных отличий исследованного набора source не обнаружено; основания F01–F22 сохранены, номера строк заменяются символами.

Evidence: [source-before.json](source-before.json), [source-normalized.json](source-normalized.json), [astra-comparison.json](astra-comparison.json), [source-comparison.json](source-comparison.json), [git-status.stdout.log](git-status.stdout.log), [git-diff.stdout.log](git-diff.stdout.log), [binary.json](binary.json). По ancestry применимых AGENTS.md нет. Найден и прочитан `.runtime/llama-b11307-src/AGENTS.md`: его scope — сторонний llama.cpp, внутри которого ничего не менялось; к диагностическим файлам ANIX он не применяется. Inventory: [repository-instructions.json](repository-instructions.json).

## 2. Среда и замороженные профили

| Поле | Значение |
| --- | --- |
| OS | Windows 11 Enterprise LTSC, 10.0.26100 |
| CPU | AMD Ryzen 5 3600, 6 cores / 12 logical processors |
| Видимая RAM | 33 477 876 KiB, около 31.9 GiB |
| GPU | NVIDIA GeForce RTX 2060, **12288 MiB по nvidia-smi**, driver 616.92 |
| rustc | 1.96.0, `ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96`, host x86_64-pc-windows-msvc, LLVM 22.1.2 |
| cargo | 1.96.0, `30a34c682`, 2026-05-25 |
| Toolchain declaration | stable + rustfmt/clippy; exact реально использованный путь записан |
| Python SQLite | 3.50.4; отдельно от bundled rusqlite |
| Build target | `target/p0-audit`; root lockfile неизменён; `--locked` |

WMI `AdapterRAM` даёт около 4 GiB и не используется как достоверный VRAM size; сохранены оба источника. Hardware manifest: [hardware-cpu.json](hardware-cpu.json), [hardware-os.json](hardware-os.json), [hardware-gpu.json](hardware-gpu.json), [nvidia-smi.stdout.log](nvidia-smi.stdout.log). Первое чтение CIM было запрещено sandbox; после разрешения чтение выполнено. Это ограничение среды, не дефект ANIX.

Hashes конфигураций, модели manifest и пяти неизменённых миграций: [profiles-migrations.json](profiles-migrations.json). Проверенные локальные hashes семи GGUF и двух llama-server binaries: [runtime-assets.json](runtime-assets.json). Они идентифицируют установленные assets, но не доказывают, какие weights обслуживали исторический HTTP endpoint. Модели и servers не запускались, profiles не менялись, downloads не выполнялись.

Legacy defaults: WorkBudget 4096 context / 4 model calls / 8 tool calls; project ResearchBudget fields optional. Long-horizon 654/664: **40 Lead decisions / 60 tool calls**, context 4096. Provider configured output/context/schema остаются в замороженном `test_lead.rs`; JSON request/schema диагностического provider сохранены в [F14.json](F14.json), параметры — в [diagnostic-profile.json](diagnostic-profile.json). Эти defaults не переименованы в измеренные tokens или полный attempts/spend ledger.

## 3. Baseline проверки

Все команды выполнены из frozen repository root. Запускался конкретный `cargo.exe` установленного stable toolchain; `RUSTC`/`RUSTDOC` и PATH заданы на тот же bindir. Python subprocess сохраняет настоящий native `returncode`; PowerShell/Python wrapper status не подменяет exit code Rust test process.

| Команда | Native exit | Результат |
| --- | --- | --- |
| rustc --version --verbose; cargo --version | 0 / 0 | Exact versions записаны |
| cargo metadata --locked --no-deps --format-version 1 | 0 | Семь crates и реальные targets |
| cargo test --workspace --locked -- --list | 0 | 185 ordinary tests + 3 doctests discovered |
| cargo fmt --all -- --check | 0 | PASS |
| cargo check --workspace --all-targets --locked | 0 | PASS |
| cargo build --workspace --locked | 0 | PASS |
| cargo test --workspace --locked | 0 | **183 ordinary PASS, 3 doctests PASS, 2 ignored, 0 FAIL** |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 0 | PASS |
| anix --help; research --help; eval --help | 0 | Реальное CLI surface зафиксировано |

Build/check/tests используют явный `--target-dir target/p0-audit`, не меняющий семантику команды. Полные command arrays, duration и stdout/stderr filenames: [checks.json](checks.json), [supplement-checks.json](supplement-checks.json). Discovery/logs: [discovery.stdout.log](discovery.stdout.log), [test.stdout.log](test.stdout.log), [test.stderr.log](test.stderr.log), [test-summary.json](test-summary.json). Tests `acceptance` и `runtime_v2` подключены manifest `anix-storage`, а не выдуманным root package.

Ignored cases:

- `crash_child_fixture` — служебный subprocess fixture. Ordinary harness его игнорирует; passing `crash_process_checkpoint_resume` явно запускает его в дочернем процессе, ожидая crash. Он не посчитан самостоятельным обычным PASS и не запускался вручную без env.
- `real_llama_smoke_missing_bad_model_concurrency_shutdown` — требует реальных model + llama.cpp. **NOT RUN**; hashes assets не заменяют выполнение model smoke.

Сохранены существующие working regressions: graph/atomic-child guards, stale Gate и external-consumer scope, dedup/replication, owner/policy/pause/budget, failed-call reservation/identity, valid source observation, pending/refine/reject retrieval, source exclusions, immutable context и child result previews, branch scope/review/isolation, workspace/redirect controls, checkpoints/crash/upgrade/CLI. Их PASS имеет прежний ограниченный смысл; он не подтверждает новые I/T contracts автоматически.

## 4. Обязательные архитектурные воспроизведения

Diagnostic crate в workspace не добавлен. [probe.rs](probe.rs) отдельно скомпилирован `rustc --test` и связан с **однозначными rlib** из baseline Cargo build. Он пользуется публичным Core/Agent/Integrations API; данные создаются в isolated temporary workspaces. FailStorage оборачивает настоящий SqliteStorage и не подменяет его доменными fake rows.

Компиляция diagnostic executable: exit **0**. Выполнение: native exit **101**, **2 controls PASS / 6 target-contract tests FAIL / 0 ignored**. Полные аргументы linkage/run: [probe-checks.json](probe-checks.json); raw [stdout](probe-target-contracts.stdout.log) / [stderr](probe-target-contracts.stderr.log). Каждая финальная ошибка — именно ожидаемое нарушение контракта; precondition assertions исключают ранние ошибки fixture. Эти tests намеренно не помечены `ignore` или `should_panic` и не переведены в PASS.

| Дефект | Воспроизведённый путь и фактический результат | Связь с целевой архитектурой |
| --- | --- | --- |
| F02 | Composite `run_step` получает валидный CreateHypothesis Propose; provider вызван, но `step_contract_ok` отвергает до Core. Тот же action напрямую создаёт Gate, owner approve сохраняет hypothesis | Доступный scientific Record + общий admission; I03/I14, T01–T03 |
| F04 | RecordedTool compute возвращает actual output v1; validity guard PASS. Owner меняет objective при том же WorkId. Старый invocation всё ещё даёт required_tool_output_exists=true и validate_work_completion=Ok, включая restart | Work/input/output versions, RelevantBasis и accepted ResultSnapshot; I04, T05/T06 |
| F05 | После старта root добавлен incoming Blocks от незавершённой работы. Direct Succeeded отвергается **unfinished dependency**; compiled Decision state показывает ready=true/blockers=[]; finish_research **Inconclusive принят**, blocker остаётся Proposed; результат durable | Один closure predicate для всех outcomes/API; I03/I12, T06/T08/T24 |
| F08 | Snapshot pinned hypothesis v1. После owner revise сохранённый ProviderContext/restart остаётся v1, но build_for_snapshot перечитывает typed Hypothesis v2 | Immutable DecisionFrame и actual payload, без post-freeze live lookups; I04/I22, T13/T14 |
| F11 | AddChild: create_work committed; storage failure **до второго commit**, edge не создан. После reopening orphan всё ещё существует; retry тех же аргументов создаёт второй child | Atomic RevisePlan + receipt/idempotency; I05/I15, T19/T20 |
| F14 | Project max_strong_model_calls=1; loopback server возвращает malformed JSON, затем valid repaired response. Независимый server journal содержит **два POST**; Core хранит **один ModelInvocation** | Logical operation отдельно от attempt/reservation, каждый send учтён; I16/I17, T26/T27 |

Трассы: [F02.json](F02.json), [F04.json](F04.json), [F05.json](F05.json), [F08.json](F08.json), [F11.json](F11.json), [F14.json](F14.json).

Границы доказательства:

- F02 — action-space defect, а не разрешение выполнять любой model proposal. Fake provider намеренно предъявляет scientific Domain action; production schema тоже не рекламирует этот путь. Owner Gate остаётся необходимым при соответствующей policy.
- F04 использует deterministic compute adapter и реальную запись output bytes. Он доказывает отсутствие привязки к новой objective, а не произвольный entailment или успешную науку.
- F05 реализован минимальный incoming-Blocks counterexample **для Inconclusive closure**, не отдельная кампания всех Completed/superseded-child комбинаций. Это тот же readiness/finish bypass; не делается заявление, что все T06/T08/T24 выполнены.
- F08 проверяет несовместимое сочетание frozen/live representations. Stored snapshot сам по себе не повреждён; actual live LLM reasoning не оценивалось.
- F11 — детерминированный Storage error между commits, а не тест power loss. Restart и duplicate после retry проверены фактически.
- F14 — реальный локальный HTTP transport **к fake server**, без model inference/денежного bill. Metadata может агрегировать usage/repair; отсутствие attempt-level durable reservation всё равно воспроизведено. Provider `http_metrics` non-Groq остаётся zero, поэтому счётчик server journal является независимым источником фактических sends.

Дополнительные positive controls прошли: healthy AddChild создаёт edge+Ready и дедуплицирует повтор; допустимый unblocked legacy Inconclusive сохраняется при reopening; та же scientific action проходит Core/Gate/owner approval; исходный tool completion guard принимает output до objective revision; snapshot сам остаётся immutable. Полные изменения scientific chain до certificate ещё не существуют — это gap P2, а не выполненный P0 E2E.

В первых drafts fixtures были две ошибки компиляции: `AgentDecision::Propose` ожидал ProposedAction, а не ActionEnvelope; реальное budget field — `max_strong_model_calls`. Также `lead_progress_snapshot` оказался activity summary, а не Decision state: F05 исправлен на чтение **реального compiled context**. Эти ошибки постановки исправлены до финальных выводов; production code не менялся. Final raw logs соответствуют финальному fixture. Ни один baseline test failure этим не скрыт — baseline FAIL отсутствуют.

## 5. Backup, restore и compatibility baseline

Доступная `.anix/state.sqlite` снята через **SQLite online backup API из согласованного read snapshot**, а не копированием main file без WAL. Затем скопированы все 896 зарегистрированных artifact paths с pre/post source hash, declared size/hash и copied hash. Все 896 имеют статус **verified**. Private data не помещались в публичные traces.

Backup: `private/baseline-backup/state.sqlite` плюс artifact tree. Independent restored workspace: `private/baseline-restore/state.sqlite` плюс то же tree. Оба находятся внутри отчёта и исключены local `.gitignore`. Manifest каждого artifact находится в `private/artifact-backup-manifest.json`; публичный [backup-summary.json](backup-summary.json) содержит counts/hashes/результаты.

Результат:

- 207 projects; пять migration records. Все таблицы имеют одинаковые counts после restore.
- Backup и restore: integrity_check=ok, foreign_key_check=[]; все restored artifact bytes совпадают.
- Через реальный **Rust SqliteStorage** загружены и сравнены **все 207 ProjectState**: equality PASS, 896 artifacts, 680 WorkNodes, 1893 model+tool invocations. Migration versions [1,2,3,4,5] совпадают; [backup-rust-hydration.json](backup-rust-hydration.json).
- Baseline migration/upgrade/crash tests прошли; новые migrations не создавались, старые SQL не редактировались, исторические completions не превращались в certified results.

Scope backup — основная доступная `.anix` DB и все её зарегистрированные artifacts. Другие исторические `.runtime` databases, не зарегистрированные raw source files и весь пользовательский filesystem этим backup не покрыты. Полное operation reconciliation/continuation и platform power-loss guarantee не заявлены; они относятся к G4/G7. В backup preserved legacy outcomes, даже если они архитектурно недостаточны.

Для возврата данных использовать этот **отдельный согласованный snapshot** вместе с artifact tree, после остановки writer и сверки manifest. P0 не требует отката source, поскольку production files неизменны. Автоматический overwrite текущей пользовательской БД не выполнялся. После будущих external effects rollback требует reconciliation по §13.6.

## 6. Старые 654/664 и причины сбоев

Raw diagnostic JSONL, которых не было у Astra в ZIP, доступны локально. Наличие, hashes, count turns, rendered_messages и raw_completion заново проверены: [historical-evidence.json](historical-evidence.json). Все восемь report/trace файлов совпадают с hashes отчёта 11 октября.

| Historical run | Доступный trace | Что можно установить |
| --- | --- | --- |
| iteration13 8B/654 | 14 turns | Сохранены фактические message/action/result diagnostics; исходный report Blocked остаётся historical |
| iteration13 14B/654 | 6 turns | Исторический Completed не переносится на текущий кандидат или scientific acceptance |
| iteration13 14B/664 | 9 turns | Сохранены branch-return/parent-final decisions; одна path и Blocked не relabelled |
| iteration12 8B/664 | 24 turns | Это другой iteration, не substitute для iteration13 |
| iteration13 8B/664 | Не найден в доступном index | NOT RUN / MissingEvidence, не выдуман |

Private derived points заново получены из оригинальных JSONL, а не восстановлены из текста старого отчёта. Serialized rendered-message hashes — не raw HTTP wire bytes. Соответствие исторического executable/config конкретным исходникам не доказано одним наличием trace. **Единственная причина конкретного historical failure остаётся Unknown**; для причинного вывода нужны frozen comparisons §18.2 в P6. Новые code defects доказаны независимо, поэтому эти неизвестные не блокируют G0 или их исправление.

Live повтор 654/664, current-version model diagnostics, mixed20/200 и final model qualification — **NOT RUN на P0**. Их нельзя заменить mock HTTP или historical PASS.

## 7. Что изменено, что осталось открытым

Изменения только в `reports/p0-v2-2026-10-12`: read-only collectors, manifests, raw logs, standalone diagnostics, bounded-model copy, local backup и этот отчёт. Точечная карта F01–F22: [gap-register.md](gap-register.md). Source/profile/migration hashes зафиксированы; no production patch, no new schema, no policy/budget relaxation, no new dependencies.

Абстрактная Python model §22 исполнена **без изменения**: 1413 states / 5080 transitions / zero checked violations; positive closure и late receipt после cancel reachable; no_fence и weak_close mutations обнаружены. [bounded-model.stdout.log](bounded-model.stdout.log). Это verification маленькой DESIGN abstraction, не ANIX reducers и не G1/G4.

Сквозной baseline trace показывает реальные разрывы: question → Work/plan → bound Run → recorded tool output → historical snapshot → next request representations → локальные completion guards. Versioned assessments, required-use delivery, единый certificate и production session runner ещё отсутствуют. Они не сфабрикованы ради заполнения stage report. Полный T01/T29 путь должен появиться в P2.

Открыто: все F01–F22 до соответствующих system fixes; полный versioned scientific review, method/reader/derived-source-access tests, durable attempts/late receipts/concurrency, новую migration/restore campaign, current qualified model/science/cost campaigns, product pilot. Observed P0 execution cost: no external inference, no paid provider/tool effects; wall times отдельных commands сохранены. Cost будущего research неизвестен, а не zero. Рабочие механизмы baseline продолжают проходить прежнюю suite.

Фундаментального противоречия нормативной V2 архитектуре на P0 не обнаружено. CRLF, доступные дополнительные historical traces и WMI VRAM discrepancy — уточнения evidence/environment, не причины перепроектирования. Изменений architectural semantics по §20.3 не предложено; сообщения Astra или другим людям не отправлялись.

## 8. Решение G0 и граница следующего этапа

| Критерий V2 G0 / P0 | Решение |
| --- | --- |
| Source и binary идентифицированы | PASS: source digest соответствует Astra после явной CRLF нормализации; HEAD, native build, binary hash |
| Checks имеют stdout/stderr/native exit status | PASS: metadata/discovery/fmt/check/build/tests/Clippy и standalone Rust probes |
| Baseline failures сохранены и объяснены | PASS: baseline FAIL=0; шесть contract failures имеют traces, controls и установленную границу дефекта |
| Основания необходимых production changes | PASS: F02/F04/F05/F08/F11/F14 EXEC-RUST; остальной matched CODE/STATIC CE register не повышен до EXEC |
| Доступные DB/artifacts backup | PASS: coherent snapshot, 896 verified objects, SQL integrity + Rust hydration equality 207 projects |
| Исторические traces/limits/profiles/ignored cases | PASS: наличие и границы записаны; неизвестные не заменены fabricated request/cause |
| Production behavior до G0 | PASS: 50 source hashes неизменны, tracked diff пуст |

**G0 PASS.** Неизвестные вне scope P0 не превращены в PASS поздних gates. Красные целевые tests показывают работу, которая должна быть исправлена P1/P2/P4; требовать их зелёного результата на P0 означало бы начать запрещённую переработку до baseline.

Следующий допустимый этап — **P1 после рассмотрения этого результата пользователем**: contracts/reference predicates, ownership/storage ADR, независимые fixtures и расширенная safety/liveness model по §17. P1 в текущем запуске не реализован. План P0→P1→P2→P3→P4→P5→P6→P7 сохранён; P8 optional.

## 9. Воспроизводимость

Из repository root на установленном toolchain:

```powershell
python reports/p0-v2-2026-10-12/collect.py
python reports/p0-v2-2026-10-12/evidence.py
python reports/p0-v2-2026-10-12/run_probes.py
python reports/p0-v2-2026-10-12/supplement.py
python reports/p0-v2-2026-10-12/verify.py
```

`run_probes.py` — collector: его Python exit 0 означает только завершение capture. Native test exit **101** записан в probe-checks и остаётся FAIL. `verify.py` проверяет полноту/целостность baseline bundle и наличие точных ожидаемых красных воспроизведений; он не объявляет целевые инварианты выполненными. Private backup может содержать пользовательские данные; он сохранён локально, без публикации.

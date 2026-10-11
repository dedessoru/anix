# Результаты Core V2

> Исторический отчёт V2. Решение о default micro-модели и оценки из этой итерации пересмотрены в iteration 3; актуальные результаты находятся в [research harness baseline](research-harness-baseline.md) и [50-run report](research-harness-50.md). Qwen3-1.7B теперь default product micro-model.

## Реализованные подсистемы

WorkNode DAG, Atomic/Composite, output contracts, версии планов, ветви, checkpoint/resume, immutable context snapshots, FTS5/BM25, process memory, artifact offloading, ModelInvocation/ToolInvocation, execution graph, typed LocalIntelligence, ModelRuntime, CapabilityRegistry, SkillRegistry, WorkerRequest, routing/retry/escalation, usage и CLI для runtime.

Добавлена отдельная SQLite migration 0002 с project-scoped foreign keys и immutable triggers; migration 0001 не изменена. Исходный научный API V1 сохранён. Архитектура, граф работ, контекст, способы запуска и правила независимости — в [runtime-v2.md](runtime-v2.md) и [core-development-guide.md](core-development-guide.md).

## Восстановление и сквозной сценарий

Run закрепляется за WorkNode, Branch и неизменяемой PlanRevision. Checkpoint хранит ту же ревизию, snapshot и версионированное execution state. `resume_checkpoint` не вызывает модель, возвращает исторический контекст и pending gates, отмечает прерванные модельные обращения. Неизвестные side effects незавершённых инструментов автоматически не повторяются. Завершённые WorkNodes исключаются из `resumable_work`.

Отдельный тест запускает дочерний процесс: A/B завершены, C начата, checkpoint записан, процесс завершён через `process::exit` без деструкторов и закрытия SQLite. Родитель восстанавливает C и не повторяет A/B.

Сквозной V2 тест создаёт исследовательскую цель и child works/runs, выполняет deterministic operation, local classification и extraction, сохраняет CandidateClaim, подтверждает Claim, связывает evidence с гипотезой, получает experiment proposal через strong fake, разрешает Gate, записывает Experiment/ExperimentRun/Finding, завершает детей и родителя и сравнивает состояние после restart. Output contracts заканчиваются ссылками на Claim и Finding.

## Настоящие модели

Основная модель действительно скачана через Rust bootstrap и проверена:

```text
models/qwen3-0.6b/Qwen3-0.6B-Q8_0.gguf
repository: Qwen/Qwen3-0.6B-GGUF
revision: 23749fefcc72300e3a2ad315e1317431b06b590a
size: 639446688 bytes
SHA-256: 9465e63a22add5354d9bb4b99e90117043c7124007664907259bd16d043bb031
quantization: Q8_0
license: Apache-2.0
```

Manifest фиксирует официальный [Qwen repository](https://huggingface.co/Qwen/Qwen3-0.6B-GGUF) и [конкретный файл](https://huggingface.co/Qwen/Qwen3-0.6B-GGUF/blob/23749fefcc72300e3a2ad315e1317431b06b590a/Qwen3-0.6B-Q8_0.gguf). Runtime — [официальный llama.cpp b11307](https://github.com/ggml-org/llama.cpp/releases/tag/b11307), Windows CPU archive SHA-256 `558e7563b9816ab7e0ce3cbd91e8f76507a64dc96abc8871989fb67c5f525d30`.

Для сравнения также скачана и проверена `models/qwen3-1.7b/Qwen3-1.7B-Q8_0.gguf`: revision `90862c4b9d2787eaed51d12237eafdfe7c5f6077`, 1834426016 bytes, SHA-256 `061b54daade076b5d3362dac252678d17da8c68f07560be70818cace6590cb1a`. Отдельный manifest — `models/comparison-qwen-1.7b.toml`. Основной manifest не заменён.

GGUF/bin/safetensors, staging и runtime binaries исключены через `.gitignore`. Bootstrap проверяет существующие файлы, размер и checksum перед атомарной установкой, не использует повреждённые загрузки. Unit tests скачивают только крошечный HTTP fixture.

## Реальная оценка качества

`eval/micro-v1.jsonl` содержит ровно 100 fixtures: 25 classification, 15 query rewrite, 20 extraction, 15 tagging, 15 summary, 10 compression. Проверяются schema, enums, точные source quotes, обязательные факты, намерение запроса и длина; LLM judge не используется.

После двух итераций настройки инструкций текущего `micro-v3`:

| Задача | 0.6B | 1.7B |
|---|---:|---:|
| Classification | 25/25, 100% | 24/25, 96% |
| Query rewrite | 15/15 | 15/15 |
| Extraction: grounding и обязательные spans | 20/20 | 20/20 |
| Tagging: точный набор | 0/15 | 15/15 |
| Short summary: факты и длина | 15/15 | 15/15 |
| Context compression: факты и длина | 10/10 | 10/10 |
| Корректная JSON schema | 100/100 | 100/100 |

**0.6B не прошла порог tagging 97%.** `measured_capabilities` передаёт фактическую оценку маршрутизатору; задача сразу эскалирует с `LowModelScore`. Снижение caller quality threshold не отменяет минимальный порог. Успех JSON schema не приравнивается к качеству.

1.7B решает tagging лучше, но её classification 96% ниже порога 97%, а задержка и память выше. Автоматическая замена основной 0.6B не выполнена. Strong runtime — заменяемый порт; offline tests используют fake и не требуют cloud credentials.

Все реальные прогоны сохранены: `eval/results-qwen-0.6b.json`, `eval/results-qwen-0.6b-prompt-v2.json`, `eval/results-qwen-0.6b-prompt-v3.json`, `eval/results-qwen-1.7b-prompt-v3.json`.

Corpus небольшой и синтетический, с повторениями тематических классов; настройка использовала эти примеры. Это не независимый held-out benchmark и не доказательство качества на произвольной научной литературе. Grounding проверяет соответствие данному тексту, а не научную истинность утверждения.

## CPU latency и RAM

Windows x86_64, AMD Ryzen 5 3600, 6 cores / 12 threads, около 32 GiB RAM, llama.cpp b11307, CPU, Q8_0. GPU не использовалась.

| Измерение | 0.6B | 1.7B |
|---|---:|---:|
| Новый runtime готов | 1269 ms | 1636 ms |
| Peak working set | 1211 MiB | 2378 MiB |
| Classification, median 3 calls | 433 ms | 595 ms |
| Extraction, median 3 calls | 2407 ms | 10921 ms |
| Summary, median 3 calls | 728 ms | 2281 ms |
| Output tokens / request wall second, classification | 32,3 | 11,8 |
| Output tokens / request wall second, extraction | 30,7 | 11,9 |
| Output tokens / request wall second, summary | 31,6 | 12,3 |

Benchmark-файлы: `eval/benchmark-qwen-0.6b.json` и `eval/benchmark-qwen-1.7b.json`. CPU не резервировалась эксклюзивно. Cold load означает новый процесс, а не принудительное очищение OS file cache. Throughput включает полный запрос и обработку prompt, не только decode.

В evaluation 100 requests 0.6B дала p50 1041 ms / p95 3992 ms; 1.7B — p50 2184 ms / p95 10389 ms. Эти замеры тоже не выполнялись на эксклюзивно зарезервированной CPU.

## Routing и проверки

Детерминированно: реализованные DOI normalization, exact deduplication, SHA-256, schema validation, а также core graph/transition/version/path/budget checks и SQLite retrieval. Локально: простые разрешённые задачи с достаточной measured quality. Сильной модели: complex/high-impact, научные выводы, experiment design, hypothesis status, Gates, final synthesis, plan approval, canonical claim и локальные ошибки/низкое качество.

В тесте 100 смешанных операций: **60 deterministic, 35 local, 5 strong**. Успешная локальная задача strong runtime не вызывает. Usage содержит токены и явно обозначенную оценку предотвращённых cloud tokens; API цены не выдумываются. Telemetry автоматически не отправляется наружу.

`scripts/check.ps1`: formatter, compilation, Clippy с `-D warnings`, **48 обычных проверок, включая 2 compile-fail doctests**, проходят. Сохранены все 16 исходных проверок V1. Два ignored entries — crash child fixture, выполняемый родительским тестом, и model smoke для явного запуска.

Реальный smoke отдельно прошёл: отсутствующий файл, повреждённый GGUF, загрузка 0.6B, structured classification, два конкурентных запроса и остановка процесса. Итого выполнено **49 разных проверок**, если считать этот smoke сверх обычного набора.

## Границы текущей реализации

Apple adapter boundary и unavailable/fallback на Windows реализованы; native Foundation Models SDK bridge и проверка на поддерживаемом Mac не выполнены. GPU-профили не проверялись. Tagging 0.6B эскалирует вместо объявления всех шести задач качественно решёнными.

Будущие расширения: native Apple host, GPU profiles, точные provider tokenizers, полный JSON Schema validator, автоматическое согласование неизвестных side effects, внешний inference daemon при необходимости и независимый corpus научных документов. Эти ограничения не скрываются под результатами fake tests. TUI не добавлен.

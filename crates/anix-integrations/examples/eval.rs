//! Non-CI evaluation: <root> [model-path] [report-path] [manifest-path] [fixtures-path] [context: 4096|8192|16384]
use anix_agent::*;
use anix_integrations::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
#[derive(Deserialize)]
struct Fixture {
    id: String,
    task: MicroTask,
    input: Value,
    expected: Value,
}
#[derive(Serialize)]
struct Measurement {
    id: String,
    task: MicroTask,
    schema_valid: bool,
    constraints_valid: bool,
    quality_valid: bool,
    latency_ms: u64,
    output: Value,
    input_tokens: u64,
    output_tokens: u64,
    extraction_precision: Option<f64>,
    extraction_recall: Option<f64>,
    prefill_tokens_per_second: Option<f64>,
    decode_tokens_per_second: Option<f64>,
    error: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let root = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("."));
    let manifest = ModelManifest::read(
        &args
            .get(4)
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("models/manifest.toml")),
    )?;
    let path = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.model_path(&root));
    if path == manifest.model_path(&root) {
        manifest.verify(&path)?;
    }
    let context_tokens = args
        .get(6)
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(4096);
    let runtime = LlamaCppRuntime::start_with_context(
        &root.join(".runtime/llama-b11307/llama-server.exe"),
        &path,
        manifest.logical_name.clone(),
        manifest.revision.clone(),
        context_tokens,
    )?;
    let templates = BuiltinMicroTemplates;
    let memory_start = memory_sample(runtime.pid()?);
    let mut results = vec![];
    let mut counts = BTreeMap::new();
    let fixtures_path = args
        .get(5)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("eval/micro-v1.jsonl"));
    let fixtures = std::fs::read_to_string(&fixtures_path)?;
    let fixture_lines: Vec<_> = fixtures.lines().collect();
    for line in &fixture_lines {
        let fixture: Fixture = serde_json::from_str(line)?;
        let schema = task_schema(fixture.task, &fixture.input);
        let start = Instant::now();
        let result = futures::executor::block_on(runtime.generate_structured(RuntimeRequest {
            instruction: templates.instruction(fixture.task),
            input: fixture.input.to_string(),
            schema: schema.clone(),
            max_output_tokens: 384,
        }));
        let latency_ms = start.elapsed().as_millis() as u64;
        let (output, usage, timings, error) = match result {
            Ok(r) => (r.output, r.usage, r.timings, None),
            Err(e) => (Value::Null, Default::default(), None, Some(e.to_string())),
        };
        let schema_valid = validate_schema(&output, &schema).is_ok();
        let constraints_valid = validate_micro(fixture.task, &fixture.input, &output).is_ok();
        let quality_valid = constraints_valid
            && match fixture.task {
                MicroTask::Classification => output == fixture.expected,
                MicroTask::Tagging => {
                    let mut a: Vec<String> =
                        serde_json::from_value(output["tags"].clone()).unwrap_or_default();
                    let mut b: Vec<String> =
                        serde_json::from_value(fixture.expected["tags"].clone())?;
                    a.sort();
                    b.sort();
                    a == b
                }
                MicroTask::CandidateExtraction => serde_json::from_value::<ExtractedCandidates>(
                    output.clone(),
                )
                .is_ok_and(|candidates| {
                    (fixture.expected["allow_empty"].as_bool() == Some(true)
                        && candidates.candidates.is_empty()
                        || !candidates.candidates.is_empty())
                        && fixture.expected["mandatory_spans"]
                            .as_array()
                            .is_some_and(|spans| {
                                spans.iter().all(|span| {
                                    candidates.candidates.iter().any(|c| {
                                        c.text.contains(span.as_str().unwrap_or("INVALID"))
                                    })
                                })
                            })
                }),
                MicroTask::QueryRewrite => output["query"].as_str().is_some_and(|s| {
                    fixture.expected["intent_terms"]
                        .as_array()
                        .is_some_and(|terms| {
                            terms.iter().all(|t| {
                                s.to_lowercase()
                                    .contains(&t.as_str().unwrap_or("INVALID").to_lowercase())
                            })
                        })
                }),
                MicroTask::ShortSummary => fixture.expected["required_spans"]
                    .as_array()
                    .is_none_or(|spans| {
                        spans.iter().all(|span| {
                            output["summary"].as_str().is_some_and(|text| {
                                text.contains(span.as_str().unwrap_or("INVALID"))
                            })
                        })
                    }),
                MicroTask::ContextCompression => fixture.expected["required_spans"]
                    .as_array()
                    .is_none_or(|spans| {
                        spans.iter().all(|span| {
                            output["context"].as_str().is_some_and(|text| {
                                text.contains(span.as_str().unwrap_or("INVALID"))
                            })
                        })
                    }),
            };
        let (extraction_precision, extraction_recall) =
            if fixture.task == MicroTask::CandidateExtraction {
                let expected: Vec<String> = fixture.expected["candidate_quotes"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                let predicted: Vec<String> = output["candidates"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item["source_quote"].as_str())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                let true_positives = predicted
                    .iter()
                    .filter(|quote| expected.contains(quote))
                    .count();
                (
                    Some(if predicted.is_empty() {
                        if expected.is_empty() {
                            1.0
                        } else {
                            0.0
                        }
                    } else {
                        true_positives as f64 / predicted.len() as f64
                    }),
                    Some(if expected.is_empty() {
                        if predicted.is_empty() {
                            1.0
                        } else {
                            0.0
                        }
                    } else {
                        true_positives as f64 / expected.len() as f64
                    }),
                )
            } else {
                (None, None)
            };
        let counter = counts
            .entry(format!("{:?}", fixture.task))
            .or_insert((0u32, 0u32, 0u32));
        counter.0 += 1;
        counter.1 += u32::from(quality_valid);
        counter.2 += u32::from(schema_valid);
        results.push(Measurement {
            id: fixture.id,
            task: fixture.task,
            schema_valid,
            constraints_valid,
            quality_valid,
            latency_ms,
            output,
            input_tokens: usage.input,
            output_tokens: usage.output,
            extraction_precision,
            extraction_recall,
            prefill_tokens_per_second: timings.as_ref().and_then(|t| {
                (t.prompt_ms > 0.0).then_some(t.prompt_tokens as f64 * 1000.0 / t.prompt_ms)
            }),
            decode_tokens_per_second: timings.as_ref().and_then(|t| {
                (t.generation_ms > 0.0)
                    .then_some(t.generated_tokens as f64 * 1000.0 / t.generation_ms)
            }),
            error,
        });
        if results.len() % 10 == 0 {
            eprintln!("evaluated {}/{}", results.len(), fixture_lines.len());
        }
    }
    let mut latency: Vec<_> = results.iter().map(|r| r.latency_ms).collect();
    latency.sort();
    let memory_end = memory_sample(runtime.pid()?);
    let prefill: Vec<f64> = results
        .iter()
        .filter_map(|measurement| measurement.prefill_tokens_per_second)
        .collect();
    let decode: Vec<f64> = results
        .iter()
        .filter_map(|measurement| measurement.decode_tokens_per_second)
        .collect();
    let mean = |samples: &[f64]| {
        (!samples.is_empty()).then(|| samples.iter().sum::<f64>() / samples.len() as f64)
    };
    let report = json!({
        "fixture_version": fixtures_path.file_stem().and_then(|v| v.to_str()).unwrap_or("unknown"),
        "fixture_path": fixtures_path,
        "prompt_version": templates.version(),
        "manifest": manifest,
        "model_path": path,
        "model_file_bytes": manifest.size,
        "context_tokens": context_tokens,
        "runtime": "llama.cpp/b11307 CPU",
        "os": std::env::consts::OS,
        "cold_load_ms": runtime.cold_load_ms,
        "pid": runtime.pid()?,
        "memory_after_load": memory_start,
        "memory_after_eval": memory_end,
        "prefill_tokens_per_second_mean": mean(&prefill),
        "decode_tokens_per_second_mean": mean(&decode),
        "prefill_timing_samples": prefill.len(),
        "decode_timing_samples": decode.len(),
        "sample_count": results.len(),
        "counts": counts,
        "latency_p50_ms": latency[latency.len() / 2],
        "latency_p95_ms": latency[latency.len() * 95 / 100],
        "measurements": results
    });
    let report_path = args
        .get(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("eval/results-qwen-0.6b.json"));
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    println!(
        "{}",
        json!({"report":report_path,"counts":report["counts"],"cold_load_ms":report["cold_load_ms"],"p50_ms":report["latency_p50_ms"],"p95_ms":report["latency_p95_ms"]})
    );
    Ok(())
}

fn memory_sample(pid: u32) -> Value {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script=format!("Get-Process -Id {pid} | Select-Object WorkingSet64,PeakWorkingSet64,PrivateMemorySize64 | ConvertTo-Json -Compress");
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(0x08000000)
            .output()
            .ok()
            .and_then(|r| serde_json::from_slice(&r.stdout).ok())
            .unwrap_or(Value::Null)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Value::Null
    }
}

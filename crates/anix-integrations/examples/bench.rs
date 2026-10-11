//! Explicit non-CI CPU benchmark; pass root, manifest, output JSON.
use anix_agent::*;
use anix_integrations::*;
use serde_json::{json, Value};
use std::{path::PathBuf, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let root = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("."));
    let manifest = ModelManifest::read(
        &args
            .get(2)
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("models/manifest.toml")),
    )?;
    let path = manifest.model_path(&root);
    manifest.verify(&path)?;
    let runtime = LlamaCppRuntime::start(
        &root.join(".runtime/llama-b11307/llama-server.exe"),
        &path,
        manifest.logical_name.clone(),
        manifest.revision.clone(),
    )?;
    let templates = BuiltinMicroTemplates;
    let mut measurements = vec![];
    for task in [
        MicroTask::Classification,
        MicroTask::CandidateExtraction,
        MicroTask::ShortSummary,
    ] {
        let input = match task {
            MicroTask::Classification => {
                json!({"text":"Photons propagate in a vacuum.","labels":["biology","physics","chemistry"]})
            }
            MicroTask::CandidateExtraction => {
                json!({"text":"Trial included 30 participants. The observation period was 7 days.","maximum_candidates":2})
            }
            _ => {
                json!({"text":"Project Alpha analysed 40 samples. Researchers described the data repeatedly. There is no causal conclusion.","mandatory_facts":["Project Alpha","40 samples","no causal conclusion"],"maximum_characters":180})
            }
        };
        for repetition in 0..3 {
            let start = Instant::now();
            let r = futures::executor::block_on(runtime.generate_structured(RuntimeRequest {
                instruction: templates.instruction(task),
                input: input.to_string(),
                schema: task_schema(task, &input),
                max_output_tokens: 384,
            }))?;
            let elapsed = start.elapsed();
            measurements.push(json!({"task":task,"repetition":repetition,"latency_ms":elapsed.as_millis(),"usage":r.usage,"output_tokens_per_wall_second":r.usage.output as f64/elapsed.as_secs_f64(),"valid":validate_micro(task,&input,&r.output).is_ok()}));
        }
    }
    let pid = runtime.pid()?;
    #[cfg(windows)]
    let machine = {
        use std::os::windows::process::CommandExt;
        let script=format!("$p=Get-Process -Id {pid}; $c=Get-CimInstance Win32_Processor; $s=Get-CimInstance Win32_ComputerSystem; @{{cpu=$c.Name;cores=$c.NumberOfCores;logicalProcessors=$c.NumberOfLogicalProcessors;physicalRamBytes=$s.TotalPhysicalMemory;workingSetBytes=$p.WorkingSet64;peakWorkingSetBytes=$p.PeakWorkingSet64;privateBytes=$p.PrivateMemorySize64}} | ConvertTo-Json -Compress");
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(0x08000000)
            .output()
            .ok()
            .and_then(|r| serde_json::from_slice::<Value>(&r.stdout).ok())
            .unwrap_or(Value::Null)
    };
    #[cfg(not(windows))]
    let machine = {
        let _ = pid;
        Value::Null
    };
    let report = json!({"model":manifest,"backend":"CPU","runtime":"llama.cpp/b11307","os":std::env::consts::OS,"gpu_used":false,"machine":machine,"cold_load_ms":runtime.cold_load_ms,"measurements":measurements,"throughput_note":"output tokens divided by complete request wall time, includes prompt processing; not isolated decode throughput","isolation_note":"No exclusive CPU reservation; these are local engineering measurements, not rigorous hardware comparisons."});
    let output = args
        .get(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("eval/benchmark-qwen-0.6b.json"));
    std::fs::write(&output, serde_json::to_string_pretty(&report)?)?;
    println!(
        "{}",
        json!({"report":output,"cold_load_ms":report["cold_load_ms"],"machine":report["machine"]})
    );
    Ok(())
}

use anix_agent::ModelProvider;
use anix_evals::{
    long_horizon::{run_one, run_one_cloud_synthetic},
    HarnessConfig,
};
use anix_integrations::{OpenAiCompatibleModelProvider, TestLeadConfig};
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let scenario = args
        .next()
        .ok_or("usage: lead_runtime_seed <scenario> <seed> <config>")?;
    let seed: u64 = args.next().ok_or("missing seed")?.parse()?;
    let config_path = PathBuf::from(args.next().ok_or("missing Test Lead TOML config")?);
    if args.next().is_some() {
        return Err("usage: lead_runtime_seed <scenario> <seed> <config>".into());
    }
    let profile = TestLeadConfig::read(&config_path)?;
    let provider = Arc::new(OpenAiCompatibleModelProvider::new(profile.clone())?);
    let cloud = profile.is_groq();
    let case_workspace = PathBuf::from(if cloud {
        ".runtime/groq-synthetic-seeds"
    } else {
        ".runtime/lead-runtime-cleanup-seeds"
    })
    .join(format!("{scenario}-{seed}"));
    let config = HarnessConfig {
        workspace: case_workspace,
        endpoint: profile.endpoint,
        model_name: profile.model_name,
        context_size: profile.context_size,
        temperature: profile.temperature,
        timeout_seconds: profile.timeout_seconds,
        capabilities: profile.capabilities,
        budget: Default::default(),
    };
    let lead: Arc<dyn ModelProvider> = provider.clone();
    let report = if cloud {
        run_one_cloud_synthetic(&config, &scenario, seed, lead)?
    } else {
        run_one(&config, &scenario, seed, lead)?
    };
    println!("provider_metrics={}", provider.http_metrics());
    println!("scenario={} seed={} status={} consumed={} offloaded={} refinements={}/{} repairs={}/{} integrity={} recovery={} report={}",
        report.scenario, report.seed, report.status, report.consumed_batches,
        report.offloaded_source_reads, report.refine_search_calls,
        report.successful_refinements, report.repair_attempts, report.repair_successes,
        report.audit.passed, report.recovery_passed, report.report_path);
    Ok(())
}

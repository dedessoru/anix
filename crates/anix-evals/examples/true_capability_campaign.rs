use anix_evals::{run_capability_case, write_capability_campaign, HarnessConfig};
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = env::current_dir()?;
    let mut count = 5usize;
    let mut single: Option<(String, u64)> = None;
    let mut branch_seed = 810u64;
    let mut context_seed = 820u64;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--workspace" => workspace = PathBuf::from(args.next().ok_or("missing workspace")?),
            "--count" => count = args.next().ok_or("missing count")?.parse()?,
            "--branch-seed" => branch_seed = args.next().ok_or("missing branch seed")?.parse()?,
            "--context-seed" => {
                context_seed = args.next().ok_or("missing context seed")?.parse()?
            }
            "--single" => {
                let capability = args.next().ok_or("missing capability")?;
                let seed = args.next().ok_or("missing seed")?.parse()?;
                single = Some((capability, seed));
            }
            "--help" | "-h" => {
                println!("Usage: cargo run -p anix-evals --example true_capability_campaign -- [--workspace PATH] [--count 5] [--branch-seed N] [--context-seed N]");
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }
    let config = HarnessConfig::read(workspace)?;
    if config.context_size > 8192 {
        return Err(format!(
            "focused capability evaluation must keep context at or below 8192; configured {}",
            config.context_size
        )
        .into());
    }
    let results = if let Some((capability, seed)) = single {
        vec![run_capability_case(&config, &capability, seed)?]
    } else {
        write_capability_campaign(&config, branch_seed, context_seed, count)?
    };
    let passed = results
        .iter()
        .filter(|report| report.capability_status == "passed")
        .count();
    println!(
        "{{\"runs\":{},\"passed\":{},\"report\":\"eval-results/true-capabilities/focused-10.md\"}}",
        results.len(),
        passed
    );
    Ok(())
}

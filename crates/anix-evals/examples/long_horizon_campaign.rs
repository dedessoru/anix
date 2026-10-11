use anix_evals::{run_long_horizon_campaign, HarnessConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let count = args
        .next()
        .ok_or("usage: long_horizon_campaign <count> <seed> <stage>")?
        .parse()?;
    let seed = args.next().ok_or("missing seed")?.parse()?;
    let stage = args.next().ok_or("missing stage")?;
    if args.next().is_some() {
        return Err("usage: long_horizon_campaign <count> <seed> <stage>".into());
    }

    let config = HarnessConfig::read(".")?;
    let reports = run_long_horizon_campaign(&config, count, seed, &stage)?;
    let completed = reports
        .iter()
        .filter(|report| report.status == "completed")
        .count();
    let intact = reports.iter().filter(|report| report.audit.passed).count();
    println!(
        "runs={} completed={} integrity_passed={}",
        reports.len(),
        completed,
        intact
    );
    Ok(())
}

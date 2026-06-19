use std::path::PathBuf;
use anyhow::Context as _;
use clap::{Parser, ValueEnum};

use gas_schedule::GasSchedule;
use repricer_evm::{fixture::Fixture, runner};

#[derive(Parser)]
#[command(name = "harness", about = "Gas repricing impact harness")]
struct Args {
    /// Path to prestate fixture JSON produced by harvest_prestate.py
    #[arg(long)]
    fixture: PathBuf,

    /// Gas schedule to use
    #[arg(long, default_value = "baseline")]
    schedule: ScheduleName,

    /// Override the block gas limit from the fixture (e.g. 200000000 for 200 M)
    #[arg(long)]
    block_gas_limit: Option<u64>,

    /// Override the transaction gas limit from the fixture (e.g. 30000000).
    /// Use a high value so inner CALLs don't starve under expensive SLOAD schedules,
    /// giving identical execution paths across all schedules.
    #[arg(long)]
    tx_gas_limit: Option<u64>,
}

#[derive(ValueEnum, Clone, Debug)]
enum ScheduleName {
    Baseline,
    Eip7904,
    Eip8038,
    Eip7904Plus8038,
    /// EIP-8038 SLOAD costs scaled for a 200 M gas-limit block (PRAGUE spec)
    Eip8038_200m,
    /// EIP-7904 compute + EIP-8038 costs scaled for a 200 M gas-limit block
    Eip7904Plus8038_200m,
    /// EIP-8037 state gas only (AMSTERDAM spec; no cold-access override)
    Eip8037,
    /// EIP-8037 + EIP-8038 3x cold access repricing (AMSTERDAM spec)
    Eip8038Sstore,
    /// EIP-8037 + EIP-8038 scaled for 200 M gas-limit block (AMSTERDAM spec)
    Eip8038Sstore200m,
    /// EIP-8038 PR #11802 merged values, 60 M block (AMSTERDAM spec)
    Eip8038Pr11802,
    /// EIP-8038 PR #11802 values scaled to 200 M block (AMSTERDAM spec)
    Eip8038Pr11802_200m,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let json = std::fs::read_to_string(&args.fixture)
        .with_context(|| format!("reading {:?}", args.fixture))?;
    let fixture: Fixture = serde_json::from_str(&json)
        .with_context(|| "parsing fixture JSON")?;

    let (schedule, name) = match args.schedule {
        ScheduleName::Baseline              => (GasSchedule::baseline(),                "baseline"),
        ScheduleName::Eip7904               => (GasSchedule::eip7904(),                "eip7904"),
        ScheduleName::Eip8038               => (GasSchedule::eip8038(),                "eip8038"),
        ScheduleName::Eip7904Plus8038       => (GasSchedule::eip7904_plus_8038(),      "eip7904_plus_8038"),
        ScheduleName::Eip8038_200m          => (GasSchedule::eip8038_200m(),           "eip8038_200m"),
        ScheduleName::Eip7904Plus8038_200m  => (GasSchedule::eip7904_plus_8038_200m(), "eip7904_plus_8038_200m"),
        ScheduleName::Eip8037               => (GasSchedule::eip8037(),                "eip8037"),
        ScheduleName::Eip8038Sstore         => (GasSchedule::eip8038_sstore(),         "eip8038_sstore"),
        ScheduleName::Eip8038Sstore200m     => (GasSchedule::eip8038_sstore_200m(),    "eip8038_sstore_200m"),
        ScheduleName::Eip8038Pr11802        => (GasSchedule::eip8038_pr11802(),        "eip8038_pr11802"),
        ScheduleName::Eip8038Pr11802_200m   => (GasSchedule::eip8038_pr11802_200m(),   "eip8038_pr11802_200m"),
    };

    let result = runner::run_fixture(&fixture, &schedule, name, args.block_gas_limit, args.tx_gas_limit)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

/// Acceptance tests — all seven criteria from CLAUDE.md / s6-addition.md must pass.
use gas_schedule::GasSchedule;
use repricer_evm::{
    fixture::{parse_u64_hex, Fixture},
    runner::{self, run_db},
    synthetic,
};
use revm_context_interface::cfg::gas_params::{GasId, GasParams};
use revm_primitives::hardfork::SpecId;

const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/0x7b53e92ca7971da61aaee1e33666e7f21f58b573d0b5a52fcedc913d6cc92629.json"
);

fn load_fixture() -> Fixture {
    let json = std::fs::read_to_string(FIXTURE_PATH).expect("fixture file exists");
    serde_json::from_str(&json).expect("fixture parses")
}

// ── Criterion 1: Baseline fidelity ───────────────────────────────────────────

#[test]
fn test_baseline_fidelity() {
    let fixture = load_fixture();
    let expected = parse_u64_hex(&fixture.receipt.gas_used).expect("receipt gas_used parses");

    let schedule = GasSchedule::baseline();
    let result = runner::run_fixture(&fixture, &schedule, "baseline", None, None).expect("run succeeds");

    println!(
        "Baseline fidelity: expected={expected}, got={} — {}",
        result.gas_used,
        if result.gas_used == expected { "MATCH" } else { "MISMATCH" }
    );
    assert_eq!(
        result.gas_used, expected,
        "baseline must reproduce receipt.gas_used exactly"
    );
}

// ── Criterion 2: Mechanism correctness (compute) ─────────────────────────────

#[test]
fn test_mechanism_correctness() {
    // 10 compute iterations, no SLOADs.
    // Each iteration: 1×DIV + 1×SDIV + 1×MOD + 1×KECCAK256(32 bytes).
    let iters = 10u64;
    let base = GasSchedule::baseline();
    let eip = GasSchedule::eip7904();

    // Hand-computed delta per iteration:
    //   DIV:      15 - 5  = 10
    //   SDIV:     20 - 5  = 15
    //   MOD:      12 - 5  =  7
    //   KECCAK256: 45 - 30 = 15  (base only; per-word unchanged)
    // total per iter = 47, × 10 iters = 470
    let expected_delta: u64 = iters
        * ((eip.div - base.div)
            + (eip.sdiv - base.sdiv)
            + (eip.r#mod - base.r#mod)
            + (eip.keccak256_base - base.keccak256_base));

    println!("Expected EIP-7904 compute delta: {expected_delta} gas");

    let (cfg_base, blk, tx) = synthetic::build_synthetic_envs(iters, 0, 0, 0, &base);
    let (cfg_eip, _, _) = synthetic::build_synthetic_envs(iters, 0, 0, 0, &eip);

    let (gas_base, _) = run_db(synthetic::build_synthetic_db(), cfg_base, blk.clone(), tx.clone(), &base, "baseline")
        .expect("baseline run");
    let (gas_eip, _) = run_db(synthetic::build_synthetic_db(), cfg_eip, blk, tx, &eip, "eip7904")
        .expect("eip7904 run");

    let actual_delta = gas_eip.saturating_sub(gas_base);
    println!("Baseline={gas_base}, EIP-7904={gas_eip}, delta={actual_delta} (expected={expected_delta})");

    assert_eq!(actual_delta, expected_delta, "compute delta must match hand-computed value");
}

// ── Criterion 3: Real-tx demonstration ───────────────────────────────────────

#[test]
fn test_real_tx_demonstration() {
    let fixture = load_fixture();
    let base = GasSchedule::baseline();
    let eip7904 = GasSchedule::eip7904();

    let r_base   = runner::run_fixture(&fixture, &base,   "baseline", None, None).expect("baseline");
    let r_eip7904 = runner::run_fixture(&fixture, &eip7904, "eip7904", None, None).expect("eip7904");

    let delta = r_eip7904.gas_used.saturating_sub(r_base.gas_used);
    let pct = delta as f64 / r_base.gas_used as f64 * 100.0;

    println!(
        "Real liquidation tx:\n  baseline  = {}\n  eip7904   = {}\n  delta     = {} ({:.2}%)\n  compute   = {} / other = {} (baseline breakdown)",
        r_base.gas_used, r_eip7904.gas_used, delta, pct,
        r_base.breakdown.compute, r_base.breakdown.other,
    );

    assert!(r_eip7904.gas_used >= r_base.gas_used, "eip7904 should not reduce gas");
}

// ── Criterion 4: SLOAD mechanism correctness ─────────────────────────────────

#[test]
fn test_sload_mechanism() {
    // 5 cold reads (slots 0..4), 3 warm re-reads of slot 0, 0 compute iters.
    let cold = 5u64;
    let warm = 3u64;
    let base   = GasSchedule::baseline();
    let eip8038 = GasSchedule::eip8038();

    // Hand-computed delta:
    //   cold SLOAD: (6300 - 2100) * 5 = 4200 * 5 = 21000
    //   warm SLOAD: (300  - 100)  * 3 = 200  * 3 =   600
    let expected_delta: u64 =
        cold * (eip8038.cold_sload_total - base.cold_sload_total)
        + warm * (eip8038.warm_access_cost - base.warm_access_cost);

    println!("Expected EIP-8038 SLOAD delta: {expected_delta} gas");

    let (cfg_base, blk, tx) = synthetic::build_synthetic_envs(0, cold, warm, 0, &base);
    let (cfg_eip, _, _) = synthetic::build_synthetic_envs(0, cold, warm, 0, &eip8038);

    let (gas_base, ctr_base) = run_db(
        synthetic::build_synthetic_db(), cfg_base, blk.clone(), tx.clone(), &base, "baseline",
    ).expect("baseline");
    let (gas_eip, ctr_eip) = run_db(
        synthetic::build_synthetic_db(), cfg_eip, blk, tx, &eip8038, "eip8038",
    ).expect("eip8038");

    let actual_delta = gas_eip.saturating_sub(gas_base);
    println!(
        "Baseline={gas_base}, EIP-8038={gas_eip}, delta={actual_delta} (expected={expected_delta})"
    );
    println!("SLOAD counts: baseline={}, eip8038={}", ctr_base.sload_count(), ctr_eip.sload_count());

    assert_eq!(actual_delta, expected_delta, "SLOAD delta must match hand-computed value");
}

// ── Criterion 5: Thesis preview ───────────────────────────────────────────────

#[test]
fn test_thesis_preview() {
    let fixture = load_fixture();
    let base    = GasSchedule::baseline();
    let eip7904 = GasSchedule::eip7904();
    let eip8038 = GasSchedule::eip8038();

    let gas_base  = runner::run_fixture(&fixture, &base,    "baseline", None, None).expect("baseline").gas_used;
    let gas_7904  = runner::run_fixture(&fixture, &eip7904, "eip7904", None, None).expect("eip7904").gas_used;
    let gas_8038  = runner::run_fixture(&fixture, &eip8038, "eip8038", None, None).expect("eip8038").gas_used;

    let delta_7904 = gas_7904.saturating_sub(gas_base);
    let delta_8038 = gas_8038.saturating_sub(gas_base);

    println!(
        "Thesis preview — Aave v3 liquidation:\n  baseline    = {gas_base}\n  +EIP-7904   = {gas_7904}  (delta {delta_7904})\n  +EIP-8038   = {gas_8038}  (delta {delta_8038})\n  SLOAD > compute: {}",
        delta_8038 > delta_7904
    );

    assert!(
        delta_8038 > delta_7904,
        "EIP-8038 SLOAD delta ({delta_8038}) should exceed EIP-7904 compute delta ({delta_7904}) \
         for a DeFi liquidation tx"
    );
}

// ── Criterion 6: EIP-8037 SSTORE new-slot mechanism correctness ──────────────

#[test]
fn test_eip8037_sstore_new_slot() {
    let n = 5u64; // 5 new storage slots written (0->nonzero)

    let base    = GasSchedule::baseline();
    let eip8037 = GasSchedule::eip8037();

    // Hand-compute expected delta by reading GasParams for each spec.
    // Under AMSTERDAM: cost per new warm slot = sstore_static(100)
    //                  + sstore_set_without_load(2800) + state_gas(97920) = 100820
    // Under PRAGUE:    cost per new warm slot = sstore_static(100)
    //                  + sstore_set_without_load(19900)                   =  20000
    // Delta per slot = 100820 - 20000 = 80820
    // (equivalent to: state_gas + set_without_load_ams - set_without_load_pra)
    let base_gp = GasParams::new_spec(SpecId::PRAGUE);
    let ams_gp  = GasParams::new_spec(SpecId::AMSTERDAM);
    let expected_delta = n * (
        ams_gp.get(GasId::sstore_set_state_gas())
        + ams_gp.get(GasId::sstore_set_without_load_cost())
        - base_gp.get(GasId::sstore_set_without_load_cost())
    );

    println!("Expected EIP-8037 SSTORE new-slot delta: {expected_delta} gas ({} per slot)", expected_delta / n);

    // No compute or SLOAD iters; all gas delta comes from the new-slot SSTOREs.
    let (cfg_base, blk, tx) = synthetic::build_synthetic_envs(0, 0, 0, n, &base);
    let (cfg_eip, _, _)     = synthetic::build_synthetic_envs(0, 0, 0, n, &eip8037);

    let (gas_base, ctr_base) = run_db(
        synthetic::build_synthetic_db(), cfg_base, blk.clone(), tx.clone(), &base, "baseline",
    ).expect("baseline");
    let (gas_eip, ctr_eip) = run_db(
        synthetic::build_synthetic_db(), cfg_eip, blk, tx, &eip8037, "eip8037",
    ).expect("eip8037");

    let actual_delta = gas_eip.saturating_sub(gas_base);
    println!(
        "Baseline={gas_base}, EIP-8037={gas_eip}, delta={actual_delta} (expected={expected_delta})"
    );
    println!("SSTORE counts: baseline={}, eip8037={}", ctr_base.sstore_count(), ctr_eip.sstore_count());

    assert_eq!(actual_delta, expected_delta, "SSTORE new-slot delta must match hand-computed value");
}

// ── Criterion 7: EIP-8037 SSTORE impact on real liquidation tx ───────────────

#[test]
fn test_sstore_impact_liquidation() {
    let fixture = load_fixture();
    let base    = GasSchedule::baseline();
    let eip8037 = GasSchedule::eip8037();
    let eip8038 = GasSchedule::eip8038(); // PRAGUE-based SLOAD repricing (S2 result)

    let gas_base   = runner::run_fixture(&fixture, &base,    "baseline", None, None).expect("baseline").gas_used;
    let gas_8037   = runner::run_fixture(&fixture, &eip8037, "eip8037",  None, None).expect("eip8037").gas_used;
    let gas_8038   = runner::run_fixture(&fixture, &eip8038, "eip8038",  None, None).expect("eip8038").gas_used;

    let delta_8037 = gas_8037.saturating_sub(gas_base);
    let delta_8038 = gas_8038.saturating_sub(gas_base);

    println!(
        "Real liquidation — SSTORE vs SLOAD repricing:\n  baseline      = {gas_base}\n  +EIP-8037     = {gas_8037}  (SSTORE state gas delta {delta_8037})\n  +EIP-8038     = {gas_8038}  (SLOAD repricing delta  {delta_8038})\n  SSTORE > SLOAD impact: {}",
        delta_8037 > delta_8038
    );

    // The delta must be non-negative (AMSTERDAM is at least as expensive as PRAGUE
    // for any tx because the SSTORE set cost changes from 20000 to 100820 for new slots,
    // but reset cost stays the same — so a tx with zero new slots has delta=0).
    assert!(
        gas_8037 >= gas_base,
        "EIP-8037 (AMSTERDAM) must not be cheaper than baseline (PRAGUE)"
    );
}

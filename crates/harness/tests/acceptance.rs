/// Acceptance tests — all seven criteria from CLAUDE.md / s6-addition.md must pass.
use gas_schedule::GasSchedule;
use repricer_evm::{
    fixture::{parse_u64_hex, Fixture},
    runner::{self, run_db, run_db_detailed},
    synthetic,
};
use repricer_evm::inspector::GasBreakdown;
use revm_context_interface::cfg::gas_params::{GasId, GasParams};
use revm_primitives::hardfork::SpecId;

const TX_CAP: Option<u64> = Some(16_777_216);

const COMPLEX_AMM_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/0x8687c5e125eb7d4b42640ffba788c175e3d0daadf51d58c3531099d2f720c875.json"
);

const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/0x7b53e92ca7971da61aaee1e33666e7f21f58b573d0b5a52fcedc913d6cc92629.json"
);

fn load_fixture() -> Fixture {
    let json = std::fs::read_to_string(FIXTURE_PATH).expect("fixture file exists");
    serde_json::from_str(&json).expect("fixture parses")
}

// ── Gas category breakdown: baseline vs eip8037 ──────────────────────────────

#[test]
fn test_gas_category_breakdown_complex_amm() {
    let json = std::fs::read_to_string(COMPLEX_AMM_FIXTURE_PATH).expect("complex AMM fixture");
    let fixture: Fixture = serde_json::from_str(&json).expect("fixture parses");

    let base = GasSchedule::baseline();
    let eip8037 = GasSchedule::eip8037();

    let (gas_base, bd_base) = runner::run_fixture_gas_breakdown(
        &fixture, &base, Some(60_000_000), Some(16_777_216)
    ).expect("baseline run");
    let (gas_8037, bd_8037) = runner::run_fixture_gas_breakdown(
        &fixture, &eip8037, Some(60_000_000), Some(16_777_216)
    ).expect("eip8037 run");

    println!("=== Gas category breakdown — baseline vs eip8037 ===");
    println!("  Note: CALL gas = CALL static(100) + CALL dynamic (cold/new_account/value)");
    println!("        SSTORE gas = SSTORE static(100) + SSTORE dynamic (set/reset/cold)");
    println!("        (CALL gas does NOT include inner call's gas — only outer overhead)");
    println!();
    println!("  {:<12}  {:>14}  {:>14}  {:>10}",
        "category", "baseline", "eip8037", "Δ");
    println!("  {:<12}  {:>14}  {:>14}  {:>10}",
        "sstore_gas", bd_base.sstore_gas, bd_8037.sstore_gas,
        bd_8037.sstore_gas - bd_base.sstore_gas);
    println!("  {:<12}  {:>14}  {:>14}  {:>10}",
        "call_gas", bd_base.call_gas, bd_8037.call_gas,
        bd_8037.call_gas - bd_base.call_gas);
    println!("  {:<12}  {:>14}  {:>14}  {:>10}",
        "other_gas", bd_base.other_gas, bd_8037.other_gas,
        bd_8037.other_gas - bd_base.other_gas);
    let total_base = bd_base.sstore_gas + bd_base.call_gas + bd_base.other_gas;
    let total_8037 = bd_8037.sstore_gas + bd_8037.call_gas + bd_8037.other_gas;
    println!("  {:<12}  {:>14}  {:>14}  {:>10}", "total(check)", total_base, total_8037, total_8037 - total_base);
    println!("  actual gas_used: baseline={gas_base}, eip8037={gas_8037}, Δ={}", gas_8037 as i64 - gas_base as i64);
}

// ── Full GasParams diff: PRAGUE vs AMSTERDAM ─────────────────────────────────

#[test]
fn test_gas_params_diff_prague_amsterdam() {
    let prague = GasParams::new_spec(SpecId::PRAGUE);
    let amsterdam = GasParams::new_spec(SpecId::AMSTERDAM);
    println!("=== GasParams differences: AMSTERDAM vs PRAGUE ===");
    // Try all known GasId names
    let ids: &[GasId] = &[
        GasId::exp_byte_gas(),
        GasId::extcodecopy_per_word(),
        GasId::copy_per_word(),
        GasId::logdata(),
        GasId::logtopic(),
        GasId::mcopy_per_word(),
        GasId::memory_linear_cost(),
        GasId::memory_quadratic_reduction(),
        GasId::initcode_per_word(),
        GasId::create(),
        GasId::call_stipend_reduction(),
        GasId::transfer_value_cost(),
        GasId::cold_account_additional_cost(),
        GasId::new_account_cost(),
        GasId::warm_storage_read_cost(),
        GasId::sstore_static(),
        GasId::sstore_set_without_load_cost(),
        GasId::sstore_reset_without_cold_load_cost(),
        GasId::sstore_clearing_slot_refund(),
        GasId::selfdestruct_refund(),
        GasId::call_stipend(),
        GasId::cold_storage_additional_cost(),
        GasId::cold_storage_cost(),
        GasId::new_account_cost_for_selfdestruct(),
        GasId::code_deposit_cost(),
        GasId::tx_token_non_zero_byte_multiplier(),
        GasId::tx_token_cost(),
        GasId::tx_floor_cost_per_token(),
        GasId::tx_floor_cost_base_gas(),
        GasId::tx_access_list_address_cost(),
        GasId::tx_access_list_storage_key_cost(),
        GasId::tx_base_stipend(),
        GasId::tx_create_cost(),
        GasId::tx_initcode_cost(),
        GasId::sstore_set_refund(),
        GasId::sstore_reset_refund(),
        GasId::sstore_set_state_gas(),
        GasId::new_account_state_gas(),
        GasId::code_deposit_state_gas(),
        GasId::create_state_gas(),
        GasId::tx_floor_token_zero_byte_multiplier(),
        GasId::tx_access_list_floor_byte_multiplier(),
    ];
    let mut diffs = 0usize;
    for id in ids {
        let pv = prague.get(*id);
        let av = amsterdam.get(*id);
        let tag = if pv != av { " ***" } else { "" };
        println!("  {:50}: PRAGUE={:8}, AMSTERDAM={:8}{}", id.name(), pv, av, tag);
        if pv != av { diffs += 1; }
    }
    println!("  Total differing GasParams: {diffs}");
}

// ── Gas table diff: PRAGUE vs AMSTERDAM ──────────────────────────────────────

#[test]
fn test_gas_table_diff_prague_amsterdam() {
    use revm_interpreter::instructions::gas_table_spec;
    let prague = gas_table_spec(SpecId::PRAGUE);
    let amsterdam = gas_table_spec(SpecId::AMSTERDAM);
    println!("=== Gas table differences: AMSTERDAM vs PRAGUE ===");
    let mut diffs = 0usize;
    for i in 0usize..256 {
        if prague[i] != amsterdam[i] {
            println!("  opcode 0x{i:02x}: PRAGUE={}, AMSTERDAM={}", prague[i], amsterdam[i]);
            diffs += 1;
        }
    }
    if diffs == 0 {
        println!("  (no differences — all static costs are identical)");
    }
    println!("  Total differing opcodes: {diffs}");
    println!();
    println!("  Key opcode table entries (PRAGUE):");
    println!("    SSTORE (0x55): {}", prague[0x55]);
    println!("    SLOAD  (0x54): {}", prague[0x54]);
    println!("    CALL   (0xf1): {}", prague[0xf1]);
    println!("    DELEGATECALL (0xf4): {}", prague[0xf4]);
    println!("    STATICCALL (0xfa): {}", prague[0xfa]);
}

// ── Criterion 1: Baseline fidelity ───────────────────────────────────────────

#[test]
fn test_baseline_fidelity() {
    let fixture = load_fixture();
    let expected = parse_u64_hex(&fixture.receipt.gas_used).expect("receipt gas_used parses");

    let schedule = GasSchedule::baseline();
    let result = runner::run_fixture(&fixture, &schedule, "baseline", None, TX_CAP).expect("run succeeds");

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

    let r_base   = runner::run_fixture(&fixture, &base,   "baseline", None, TX_CAP).expect("baseline");
    let r_eip7904 = runner::run_fixture(&fixture, &eip7904, "eip7904", None, TX_CAP).expect("eip7904");

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

    let gas_base  = runner::run_fixture(&fixture, &base,    "baseline", None, TX_CAP).expect("baseline").gas_used;
    let gas_7904  = runner::run_fixture(&fixture, &eip7904, "eip7904", None, TX_CAP).expect("eip7904").gas_used;
    let gas_8038  = runner::run_fixture(&fixture, &eip8038, "eip8038", None, TX_CAP).expect("eip8038").gas_used;

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

    let gas_base   = runner::run_fixture(&fixture, &base,    "baseline", None, TX_CAP).expect("baseline").gas_used;
    let gas_8037   = runner::run_fixture(&fixture, &eip8037, "eip8037",  None, TX_CAP).expect("eip8037").gas_used;
    let gas_8038   = runner::run_fixture(&fixture, &eip8038, "eip8038",  None, TX_CAP).expect("eip8038").gas_used;

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

// ── Analysis: SSTORE classification for complex AMM arb ──────────────────────

#[test]
fn test_sstore_classification_complex_amm() {
    let json = std::fs::read_to_string(COMPLEX_AMM_FIXTURE_PATH).expect("complex AMM fixture exists");
    let fixture: Fixture = serde_json::from_str(&json).expect("fixture parses");

    let schedule = GasSchedule::baseline();
    let stats = runner::classify_sstores_fixture(&fixture, &schedule, None, TX_CAP)
        .expect("classify run succeeds");

    let total = stats.total();
    println!("=== SSTORE classification — complex AMM arb (0x55738c59...) ===");
    println!("  set_in_prestate    (orig==0, new!=0, slot in prestate as 0x0): {}", stats.set_in_prestate);
    println!("  set_not_in_prestate(orig==0, new!=0, slot absent from prestate): {}", stats.set_not_in_prestate);
    println!("  reset              (orig!=0, new!=0, first write in tx):        {}", stats.reset);
    println!("  clear              (orig!=0, new==0, first write in tx):        {}", stats.clear);
    println!("  re_dirty           (slot already written earlier in tx):        {}", stats.re_dirty);
    println!("  noop               (new == present, no cost change):            {}", stats.noop);
    println!("  total SSTOREs:                                                  {total}");
    println!();
    println!("  total set (EIP-8037 state-gas candidates): {}", stats.total_set());
    println!("  expected eip8037 delta from set writes: {} gas",
        stats.total_set() * 80_820);
    println!("  expected PRAGUE sstore_set cost: {} gas",
        stats.total_set() * 20_000 + stats.reset * 5_000);
    println!("  expected AMSTERDAM sstore_set savings vs PRAGUE: {} gas",
        stats.total_set() * 17_100);

    // The total_set count must be consistent with observed gas deltas:
    // eip8037 - baseline ≈ total_set × 80_820  (state gas counted in tx_gas_used)
    // OR eip8038_sstore - eip8038 ≈ -total_set × 17_100  (if state gas NOT in tx_gas_used)
    assert!(total > 0, "complex AMM tx must execute at least one SSTORE");
}

#[test]
fn test_opcode_counts_complex_amm() {
    let json = std::fs::read_to_string(COMPLEX_AMM_FIXTURE_PATH).expect("complex AMM fixture exists");
    let fixture: Fixture = serde_json::from_str(&json).expect("fixture parses");

    let db8038 = runner::build_db(&fixture).unwrap();
    let db8038s = runner::build_db(&fixture).unwrap();

    let s8038  = GasSchedule::eip8038();
    let s8038s = GasSchedule::eip8038_sstore();

    let (cfg8038,  blk, tx) = runner::build_envs(&fixture, &s8038,  None, None).unwrap();
    let (cfg8038s, _,   _ ) = runner::build_envs(&fixture, &s8038s, None, None).unwrap();

    let (gas8038,  ctr8038)  = run_db(db8038,  cfg8038,  blk.clone(), tx.clone(), &s8038,  "eip8038").unwrap();
    let (gas8038s, ctr8038s) = run_db(db8038s, cfg8038s, blk,         tx,         &s8038s, "eip8038_sstore").unwrap();

    println!("=== Opcode count comparison: eip8038 vs eip8038_sstore ===");
    println!("  gas_used:        eip8038={gas8038}, eip8038_sstore={gas8038s}, diff={}", gas8038 as i64 - gas8038s as i64);
    println!("  SLOAD  (0x54):   eip8038={}, eip8038_sstore={}", ctr8038.sload_count(),  ctr8038s.sload_count());
    println!("  SSTORE (0x55):   eip8038={}, eip8038_sstore={}", ctr8038.sstore_count(), ctr8038s.sstore_count());
    println!("  compute gas:     eip8038={}, eip8038_sstore={}", ctr8038.compute_gas_used(&s8038), ctr8038s.compute_gas_used(&s8038s));

    let sload_diff_gas = ctr8038.sload_count() as i64 * 300 - ctr8038s.sload_count() as i64 * 300;
    println!("  SLOAD gas (both warm=300):  eip8038={}, eip8038_sstore={}, diff={}",
        ctr8038.sload_count() * 300, ctr8038s.sload_count() * 300, sload_diff_gas);

    // CALL-family and CREATE — these differ under AMSTERDAM (new_account_cost 25000→0, create 32000→9000)
    println!("  CALL    (0xf1): eip8038={}, eip8038_sstore={}", ctr8038.count(0xf1), ctr8038s.count(0xf1));
    println!("  CALLCODE(0xf2): eip8038={}, eip8038_sstore={}", ctr8038.count(0xf2), ctr8038s.count(0xf2));
    println!("  DELEGAT (0xf4): eip8038={}, eip8038_sstore={}", ctr8038.count(0xf4), ctr8038s.count(0xf4));
    println!("  STATIC  (0xfa): eip8038={}, eip8038_sstore={}", ctr8038.count(0xfa), ctr8038s.count(0xfa));
    println!("  CREATE  (0xf0): eip8038={}, eip8038_sstore={}", ctr8038.count(0xf0), ctr8038s.count(0xf0));
    println!("  CREATE2 (0xf5): eip8038={}, eip8038_sstore={}", ctr8038.count(0xf5), ctr8038s.count(0xf5));
    println!("  SELFDES (0xff): eip8038={}, eip8038_sstore={}", ctr8038.count(0xff), ctr8038s.count(0xff));

    // Also run eip8037 (AMSTERDAM, natural SLOAD costs) to isolate SSTORE vs SLOAD effects
    let db8037 = runner::build_db(&fixture).unwrap();
    let s8037 = GasSchedule::eip8037();
    let (cfg8037, blk8037, tx8037) = runner::build_envs(&fixture, &s8037, None, None).unwrap();
    let (gas8037, ctr8037) = run_db(db8037, cfg8037, blk8037, tx8037, &s8037, "eip8037").unwrap();
    println!("\n  eip8037 gas={gas8037} (AMSTERDAM, baseline SLOAD costs)");
    println!("  SSTORE (0x55): eip8037={}", ctr8037.sstore_count());

    // Differences:
    // eip8038 - eip8037 should be pure SLOAD repricing effect (PRAGUE vs AMSTERDAM at same SLOAD costs? No...)
    // Actually eip8037 uses AMSTERDAM + baseline SLOAD costs
    // eip8038_sstore uses AMSTERDAM + repriced SLOAD costs + cold SSTORE costs
    // eip8037 vs baseline: AMSTERDAM spec alone (SSTORE set 19900→2800, state gas)
    // eip8038_sstore vs eip8037: adds SLOAD repricing + cold SSTORE repricing
    let baseline_db = runner::build_db(&fixture).unwrap();
    let sbase = GasSchedule::baseline();
    let (cfgbase, blkbase, txbase) = runner::build_envs(&fixture, &sbase, None, None).unwrap();
    let (gas_base, _) = run_db(baseline_db, cfgbase, blkbase, txbase, &sbase, "baseline").unwrap();

    // Detailed breakdown: total_spent and inner_refunded for each schedule
    let (_, spent_base, refund_base, _) = run_db_detailed(
        runner::build_db(&fixture).unwrap(),
        runner::build_envs(&fixture, &GasSchedule::baseline(), None, None).unwrap().0,
        runner::build_envs(&fixture, &GasSchedule::baseline(), None, None).unwrap().1,
        runner::build_envs(&fixture, &GasSchedule::baseline(), None, None).unwrap().2,
        &GasSchedule::baseline(),
    ).unwrap();

    let (_, spent_8037, refund_8037, _) = run_db_detailed(
        runner::build_db(&fixture).unwrap(),
        runner::build_envs(&fixture, &s8037, None, None).unwrap().0,
        runner::build_envs(&fixture, &s8037, None, None).unwrap().1,
        runner::build_envs(&fixture, &s8037, None, None).unwrap().2,
        &s8037,
    ).unwrap();

    let (_, spent_8038, refund_8038, _) = run_db_detailed(
        runner::build_db(&fixture).unwrap(),
        runner::build_envs(&fixture, &s8038, None, None).unwrap().0,
        runner::build_envs(&fixture, &s8038, None, None).unwrap().1,
        runner::build_envs(&fixture, &s8038, None, None).unwrap().2,
        &s8038,
    ).unwrap();

    let (_, spent_8038s, refund_8038s, _) = run_db_detailed(
        runner::build_db(&fixture).unwrap(),
        runner::build_envs(&fixture, &s8038s, None, None).unwrap().0,
        runner::build_envs(&fixture, &s8038s, None, None).unwrap().1,
        runner::build_envs(&fixture, &s8038s, None, None).unwrap().2,
        &s8038s,
    ).unwrap();

    println!("\n  Summary with gas breakdown (natural tx / block limits):");
    println!("  {:<20}  {:>12}  {:>14}  {:>12}  {:>12}",
        "schedule", "gas_used", "exec_gas_spent", "state_gas", "Δgas_used");
    println!("  {:<20}  {:>12}  {:>14}  {:>12}  {:>12}", "baseline", gas_base, spent_base, refund_base, "-");
    println!("  {:<20}  {:>12}  {:>14}  {:>12}  {:>12}", "eip8037",
        gas8037, spent_8037, refund_8037, gas8037 as i64 - gas_base as i64);
    println!("  {:<20}  {:>12}  {:>14}  {:>12}  {:>12}", "eip8038",
        gas8038, spent_8038, refund_8038, gas8038 as i64 - gas_base as i64);
    println!("  {:<20}  {:>12}  {:>14}  {:>12}  {:>12}", "eip8038_sstore",
        gas8038s, spent_8038s, refund_8038s, gas8038s as i64 - gas_base as i64);
    println!("\n  exec_gas baseline vs eip8037: Δ={}", spent_8037 as i64 - spent_base as i64);
    println!("  state_gas eip8037: {}", refund_8037);

    // Now compare ALL opcode counts for baseline vs eip8037 (to spot any execution path diff)
    let db_base2 = runner::build_db(&fixture).unwrap();
    let db8037b = runner::build_db(&fixture).unwrap();
    let sbase2 = GasSchedule::baseline();
    let s8037b = GasSchedule::eip8037();
    let (cfgbase2, blkbase2, txbase2) = runner::build_envs(&fixture, &sbase2, None, None).unwrap();
    let (cfg8037b, blk8037b, tx8037b) = runner::build_envs(&fixture, &s8037b, None, None).unwrap();
    let (_, ctr_base2) = run_db(db_base2, cfgbase2, blkbase2, txbase2, &sbase2, "baseline2").unwrap();
    let (_, ctr_8037b) = run_db(db8037b, cfg8037b, blk8037b, tx8037b, &s8037b, "eip8037b").unwrap();

    println!("\n  ALL opcode count differences (baseline vs eip8037, only diffs shown):");
    let mut count_diffs = 0usize;
    for op in 0u8..=255u8 {
        let b = ctr_base2.count(op);
        let a = ctr_8037b.count(op);
        if b != a {
            println!("    opcode 0x{op:02x}: baseline={b}, eip8037={a}, Δ={}", a as i64 - b as i64);
            count_diffs += 1;
        }
    }
    if count_diffs == 0 {
        println!("    (all opcode counts are identical!)");
    }
    println!("  Total opcodes with different counts: {count_diffs}");

    // Since opcode counts are identical, compute total static gas from table and counts
    {
        use revm_interpreter::instructions::gas_table_spec;
        let table_base = gas_table_spec(SpecId::PRAGUE);
        let table_8037 = gas_table_spec(SpecId::AMSTERDAM);
        // tables are the same, so this gives one value
        let mut total_static_gas = 0u64;
        for op in 0u8..=255u8 {
            total_static_gas += ctr_base2.count(op) * table_base[op as usize] as u64;
        }
        println!("\n  Total static gas (both specs identical): {total_static_gas}");
        let intrinsic_gas = 21_000u64 + 2788 * 16; // 21000 + calldata (rough estimate)
        println!("  Approximate intrinsic gas: ~{intrinsic_gas}");
        let dynamic_gas_base = spent_base as i64 - total_static_gas as i64;
        let dynamic_gas_8037 = spent_8037 as i64 - total_static_gas as i64;
        println!("  Dynamic gas (exec_spent - static): baseline={dynamic_gas_base}, eip8037={dynamic_gas_8037}");
        println!("  Dynamic gas difference: {}", dynamic_gas_8037 - dynamic_gas_base);

        // Intrinsic gas breakdown
        // Also check SSTORE/SLOAD static contribution
        let sstore_static_total = ctr_base2.count(0x55) * 100;
        let sload_static = table_base[0x54] as u64;
        let sload_static_total = ctr_base2.count(0x54) * sload_static;
        println!("  SSTORE static gas total (100 per SSTORE): {sstore_static_total}");
        println!("  SLOAD static gas total ({}×{}): {}", ctr_base2.count(0x54), sload_static, sload_static_total);
    }
}

// ── Which contracts own the new-slot SSTOREs in the complex AMM tx? ─────────

#[test]
fn test_complex_amm_sstore_addresses() {
    let json = std::fs::read_to_string(COMPLEX_AMM_FIXTURE_PATH).expect("fixture");
    let fixture: Fixture = serde_json::from_str(&json).expect("parse");

    let sched = GasSchedule::eip8037();
    let entries = runner::run_fixture_sstore_addresses(&fixture, &sched, None, TX_CAP)
        .expect("sstore address trace");

    // Cross-reference with GasBreakdown charges to identify which SSTOREs pay new-slot
    // state gas (charge >= 100 820).
    let (_, bd) = runner::run_fixture_gas_breakdown(&fixture, &sched, None, TX_CAP)
        .expect("gas breakdown");

    println!("=== Per-SSTORE address/key/charge (eip8037, natural limit) ===");
    println!("  {:>4}  {:>42}  {:>66}  {:>9}", "#", "address", "key", "charge");
    for (i, ((addr, key, _new_val), &charge)) in entries.iter().zip(bd.sstore_charges.iter()).enumerate() {
        let marker = if charge >= 100_000 { " ← NEW SLOT" } else { "" };
        println!("  {:>4}  {addr:?}  {key:#066x}  {:>9}{marker}", i+1, charge);
    }
    println!("  Total SSTOREs: {}, new-slot count: {}",
        entries.len(),
        bd.sstore_charges.iter().filter(|&&c| c >= 100_000).count());
}

// ── Execution completeness at TX_GAS_LIMIT_CAP for all fixtures ──────────────

const SIMPLE_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/0x7ab274aaa28033af85e1eee5a312113672ab5fc2c335b09cd802cce31fc86c0a.json"
);

#[test]
fn test_all_fixtures_completeness() {
    // For each fixture + schedule, run at TX_GAS_LIMIT_CAP and check whether the
    // execution path is identical to baseline (compute gas matches).
    let fixtures: &[(&str, &str, u64)] = &[
        // (label, path, receipt_gas)
        ("liquidation",  FIXTURE_PATH,            781_399),
        ("simple",       SIMPLE_FIXTURE_PATH,      194_977),
        ("intermediate", COMPLEX_AMM_FIXTURE_PATH, 1_884_138),
    ];

    let schedules: &[(&str, GasSchedule, Option<u64>)] = &[
        ("eip8037",            GasSchedule::eip8037(),             None),
        ("eip8038",            GasSchedule::eip8038(),             None),
        ("eip8038_sstore",     GasSchedule::eip8038_sstore(),      None),
        ("eip8038_200m",       GasSchedule::eip8038_200m(),        Some(200_000_000)),
        ("eip8038_sstore200m", GasSchedule::eip8038_sstore_200m(), Some(200_000_000)),
    ];

    println!("=== Execution at TX_GAS_LIMIT_CAP (16 777 216): path same as baseline? ===");
    println!("  (path check: compute_gas at TX_CAP baseline == compute_gas at TX_CAP repriced)");
    println!();

    for (label, path, receipt_gas) in fixtures {
        println!("  [{label}]  receipt={receipt_gas}");
        let json = std::fs::read_to_string(path).expect("fixture");
        let fixture: Fixture = serde_json::from_str(&json).expect("parse");

        let base = GasSchedule::baseline();
        let (_, ctr_base) = run_db(
            runner::build_db(&fixture).unwrap(),
            runner::build_envs(&fixture, &base, None, TX_CAP).unwrap().0,
            runner::build_envs(&fixture, &base, None, TX_CAP).unwrap().1,
            runner::build_envs(&fixture, &base, None, TX_CAP).unwrap().2,
            &base, "baseline",
        ).unwrap();
        let base_compute = ctr_base.compute_gas_used(&base);

        for (sname, sched, block_limit) in schedules {
            let result = runner::run_fixture(&fixture, sched, sname, *block_limit, TX_CAP);
            match result {
                Err(e) => println!("    {sname:<22}  ERROR: {e}"),
                Ok(r) => {
                    let (_, ctr) = run_db(
                        runner::build_db(&fixture).unwrap(),
                        runner::build_envs(&fixture, sched, *block_limit, TX_CAP).unwrap().0,
                        runner::build_envs(&fixture, sched, *block_limit, TX_CAP).unwrap().1,
                        runner::build_envs(&fixture, sched, *block_limit, TX_CAP).unwrap().2,
                        sched, sname,
                    ).unwrap();
                    let compute = ctr.compute_gas_used(sched);
                    let path_ok = compute == base_compute;
                    println!("    {sname:<22}  gas={:>12}  path_same={}",
                        r.gas_used,
                        if path_ok { "YES" } else { "NO (diverged)" },
                    );
                }
            }
        }
        println!();
    }
}

// ── All schedules on the intermediate AMM tx at TX_GAS_LIMIT_CAP ─────────────

#[test]
fn test_intermediate_amm_all_schedules() {
    let json = std::fs::read_to_string(COMPLEX_AMM_FIXTURE_PATH).expect("complex AMM fixture");
    let fixture: Fixture = serde_json::from_str(&json).expect("fixture parses");

    let schedules: &[(&str, GasSchedule, Option<u64>)] = &[
        ("baseline",          GasSchedule::baseline(),           None),
        ("eip8037",           GasSchedule::eip8037(),            None),
        ("eip8038(60M)",      GasSchedule::eip8038(),            None),
        ("eip8038_sstore",    GasSchedule::eip8038_sstore(),     None),
        ("eip8038_200m",      GasSchedule::eip8038_200m(),       Some(200_000_000)),
        ("eip8038_sstore200m",GasSchedule::eip8038_sstore_200m(), Some(200_000_000)),
    ];

    println!("=== Intermediate AMM — all schedules at TX_GAS_LIMIT_CAP (16 777 216) ===");
    println!("  {:<22}  {:>12}  {:>10}  {:>10}", "schedule", "gas_used", "Δ_abs", "Δ%");

    let mut baseline_gas = 0u64;
    for (name, sched, block_limit) in schedules {
        let result = runner::run_fixture(&fixture, sched, name, *block_limit, TX_CAP);
        match result {
            Ok(r) => {
                let delta = r.gas_used as i64 - baseline_gas as i64;
                let pct = if baseline_gas > 0 {
                    delta as f64 / baseline_gas as f64 * 100.0
                } else { 0.0 };
                if baseline_gas == 0 {
                    baseline_gas = r.gas_used;
                    println!("  {:<22}  {:>12}  {:>10}  {:>10}", name, r.gas_used, "-", "-");
                } else {
                    println!("  {:<22}  {:>12}  {:>+10}  {:>+9.2}%", name, r.gas_used, delta, pct);
                }
            }
            Err(e) => {
                println!("  {:<22}  OOG/error: {e}", name);
            }
        }
    }
}

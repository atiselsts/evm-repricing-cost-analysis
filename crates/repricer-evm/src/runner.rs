use anyhow::Context as _;
use gas_schedule::GasSchedule;
use revm::{
    Context, MainBuilder, MainContext,
    context::{BlockEnv, CfgEnv, TxEnv},
    context_interface::transaction::{AccessList, AccessListItem},
    database::{CacheDB, EmptyDB},
    primitives::{
        hardfork::SpecId,
        Address, Bytes, TxKind, B256, U256,
    },
    state::{AccountInfo, Bytecode},
    InspectEvm,
};
use revm_context_interface::cfg::gas_params::{GasId, GasParams};

use crate::{
    fixture::{Fixture, parse_address_hex, parse_b256_hex, parse_bytes_hex, parse_u64_hex, parse_u128_hex},
    inspector::OpcodeCounter,
};

// ── public result type ────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct RunResult {
    pub gas_used: u64,
    pub schedule: String,
    pub breakdown: Breakdown,
    pub status: String,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Breakdown {
    /// Gas charged for compute opcodes under the active schedule (static portion).
    pub compute: u64,
    /// All remaining gas (state access, calls, memory, intrinsic, etc.).
    pub other: u64,
}

// ── main entry points ─────────────────────────────────────────────────────────

pub fn run_fixture(
    fixture: &Fixture,
    schedule: &GasSchedule,
    name: &str,
    block_gas_limit_override: Option<u64>,
    tx_gas_limit_override: Option<u64>,
) -> anyhow::Result<RunResult> {
    let db = build_db(fixture)?;
    let (cfg, block, tx) = build_envs(fixture, schedule, block_gas_limit_override, tx_gas_limit_override)?;
    let mut counter = OpcodeCounter::new();
    let gas_used = execute(db, cfg, block, tx, schedule, &mut counter)?;
    let compute = counter.compute_gas_used(schedule);
    Ok(RunResult {
        gas_used,
        schedule: name.to_string(),
        breakdown: Breakdown { compute, other: gas_used.saturating_sub(compute) },
        status: "success".to_string(),
    })
}

/// Run with a pre-built CacheDB — used for synthetic in-memory fixtures.
pub fn run_db(
    db: CacheDB<EmptyDB>,
    cfg: CfgEnv,
    block: BlockEnv,
    tx: TxEnv,
    schedule: &GasSchedule,
    name: &str,
) -> anyhow::Result<(u64, OpcodeCounter)> {
    let _ = name;
    let mut counter = OpcodeCounter::new();
    let gas_used = execute(db, cfg, block, tx, schedule, &mut counter)?;
    Ok((gas_used, counter))
}

// ── CacheDB builder ──────────────────────────────────────────────────────────

pub fn build_db(fixture: &Fixture) -> anyhow::Result<CacheDB<EmptyDB>> {
    let mut db = CacheDB::new(EmptyDB::default());

    for (addr_hex, state) in &fixture.prestate {
        let addr = Address::from(parse_address_hex(addr_hex)
            .with_context(|| format!("bad address {addr_hex}"))?);

        let balance = {
            let s = state.balance.strip_prefix("0x").unwrap_or(&state.balance);
            U256::from_str_radix(s, 16)
                .with_context(|| format!("bad balance for {addr_hex}"))?
        };

        let nonce: u64 = match &state.nonce {
            None => 0,
            Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(0),
            Some(serde_json::Value::String(s)) => parse_u64_hex(s)?,
            Some(_) => 0,
        };

        let code_bytes = match &state.code {
            None => vec![],
            Some(s) if s == "0x" || s.is_empty() => vec![],
            Some(s) => parse_bytes_hex(s).with_context(|| format!("bad code for {addr_hex}"))?,
        };

        let bytecode = if code_bytes.is_empty() {
            Bytecode::default()
        } else {
            Bytecode::new_raw(Bytes::from(code_bytes))
        };

        let info = AccountInfo {
            balance,
            nonce,
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode),
            account_id: None,
        };

        db.insert_account_info(addr, info);

        if let Some(storage) = &state.storage {
            for (slot_hex, val_hex) in storage {
                let slot = B256::from(parse_b256_hex(slot_hex)
                    .with_context(|| format!("bad slot {slot_hex}"))?);
                let val = B256::from(parse_b256_hex(val_hex)
                    .with_context(|| format!("bad val {val_hex}"))?);
                db.insert_account_storage(
                    addr,
                    U256::from_be_bytes(slot.0),
                    U256::from_be_bytes(val.0),
                )?;
            }
        }
    }

    Ok(db)
}

// ── environment builders ─────────────────────────────────────────────────────

pub fn build_envs(
    fixture: &Fixture,
    schedule: &GasSchedule,
    block_gas_limit_override: Option<u64>,
    tx_gas_limit_override: Option<u64>,
) -> anyhow::Result<(CfgEnv, BlockEnv, TxEnv)> {
    let bh = &fixture.block_header;
    let tx = &fixture.transaction;

    // spec drives SpecId and default GasParams; AMSTERDAM activates EIP-8037 state gas
    let mut cfg = CfgEnv::new_with_spec(schedule.spec);
    cfg.disable_nonce_check = true;
    cfg.disable_balance_check = true;
    cfg.disable_base_fee = true;
    cfg.chain_id = 1;
    apply_gas_params(&mut cfg.gas_params, schedule);

    // BlockEnv — blob price always uses PRAGUE fraction to avoid u128 overflow
    let blob_excess_gas_and_price = bh.excess_blob_gas.as_deref()
        .map(parse_u64_hex)
        .transpose()?
        .map(|excess| {
            revm_context_interface::block::BlobExcessGasAndPrice::new_with_spec(
                excess, SpecId::PRAGUE,
            )
        });

    let block = BlockEnv {
        number: U256::from(parse_u64_hex(&bh.number)?),
        beneficiary: Address::from(parse_address_hex(&bh.coinbase)?),
        timestamp: U256::from(parse_u64_hex(&bh.timestamp)?),
        gas_limit: block_gas_limit_override.unwrap_or(parse_u64_hex(&bh.gas_limit)?),
        basefee: parse_u64_hex(&bh.base_fee_per_gas)?,
        difficulty: U256::ZERO,
        prevrandao: Some(B256::from(parse_b256_hex(&bh.prev_randao)?)),
        blob_excess_gas_and_price,
        slot_num: 0,
    };

    // TxEnv
    let caller = Address::from(parse_address_hex(&tx.from)?);
    let kind = match &tx.to {
        None => TxKind::Create,
        Some(to) => TxKind::Call(Address::from(parse_address_hex(to)?)),
    };
    let data = Bytes::from(parse_bytes_hex(&tx.input)?);
    let value = {
        let s = tx.value.strip_prefix("0x").unwrap_or(&tx.value);
        U256::from_str_radix(s, 16)?
    };
    let gas_limit = tx_gas_limit_override.unwrap_or(parse_u64_hex(&tx.gas)?);
    let gas_price = if let Some(p) = &tx.max_fee_per_gas {
        parse_u128_hex(p)?
    } else if let Some(p) = &tx.gas_price {
        parse_u128_hex(p)?
    } else {
        0u128
    };
    let gas_priority_fee = tx.max_priority_fee_per_gas.as_deref()
        .map(parse_u128_hex).transpose()?;
    let nonce = parse_u64_hex(&tx.nonce)?;

    let access_list = AccessList(
        tx.access_list.iter()
            .map(|item| -> anyhow::Result<AccessListItem> {
                let address = Address::from(parse_address_hex(&item.address)?);
                let storage_keys = item.storage_keys.iter()
                    .map(|k| parse_b256_hex(k).map(B256::from))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(AccessListItem { address, storage_keys })
            })
            .collect::<Result<Vec<_>, _>>()?
    );

    let tx_env = TxEnv {
        caller,
        kind,
        data,
        value,
        gas_limit,
        gas_price,
        gas_priority_fee,
        nonce,
        access_list,
        chain_id: Some(1),
        ..TxEnv::default()
    };

    Ok((cfg, block, tx_env))
}

/// Override GasParams entries for EIP-8038 SLOAD + SSTORE repricing.
pub fn apply_gas_params(gp: &mut GasParams, schedule: &GasSchedule) {
    gp.override_gas([
        // cold SLOAD surcharge (index 23): warm_base + this = cold_sload_total
        (GasId::cold_storage_additional_cost(), schedule.cold_sload_surcharge()),
        // cold SSTORE total (index 24): shared cold cost for SSTORE cold access
        (GasId::cold_storage_cost(),            schedule.cold_sstore_cost()),
    ]);
}

// ── EVM execution ────────────────────────────────────────────────────────────

pub fn execute(
    db: CacheDB<EmptyDB>,
    cfg: CfgEnv,
    block: BlockEnv,
    tx: TxEnv,
    schedule: &GasSchedule,
    counter: &mut OpcodeCounter,
) -> anyhow::Result<u64> {
    let spec = cfg.spec;

    let mut ctx = Context::mainnet()
        .with_db(db)
        .modify_cfg_chained(|c| *c = cfg);
    ctx.block = block;

    // Build EVM with inspector (uses default EthInstructions internally).
    let mut evm = ctx.build_mainnet_with_inspector(counter);

    // Override static gas table for compute opcodes (EIP-7904)
    // and SLOAD warm base (EIP-8038) via EthInstructions::insert_gas.
    {
        use revm_interpreter::instructions::gas_table_spec;
        let default_table = gas_table_spec(spec);
        let mut reprice = |opcode: u8, new_cost: u64| {
            if new_cost as u16 != default_table[opcode as usize] {
                evm.instruction.insert_gas(opcode, new_cost as u16);
            }
        };
        reprice(0x02, schedule.mul);          // MUL
        reprice(0x04, schedule.div);          // DIV
        reprice(0x05, schedule.sdiv);         // SDIV
        reprice(0x06, schedule.r#mod);        // MOD
        reprice(0x07, schedule.smod);         // SMOD
        reprice(0x08, schedule.addmod);       // ADDMOD
        reprice(0x09, schedule.mulmod);       // MULMOD
        reprice(0x0a, schedule.exp_base);     // EXP base
        reprice(0x20, schedule.keccak256_base); // KECCAK256 base
        reprice(0x54, schedule.warm_access_cost); // SLOAD warm base (EIP-8038)
    }

    let result = evm.inspect_one_tx(tx)?;

    Ok(result.tx_gas_used())
}

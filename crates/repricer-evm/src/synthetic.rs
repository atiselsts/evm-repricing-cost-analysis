/// Build an in-memory synthetic fixture for the RepriceProbe contract.
///
/// The deployed bytecode is embedded from the Foundry artifact at compile time.
use gas_schedule::GasSchedule;
use revm::{
    context::{BlockEnv, CfgEnv, TxEnv},
    context_interface::transaction::AccessList,
    database::{CacheDB, EmptyDB},
    primitives::{
        hardfork::SpecId, Address, Bytes, TxKind, B256, U256, KECCAK_EMPTY,
    },
    state::{AccountInfo, Bytecode},
};
use revm_context_interface::block::BlobExcessGasAndPrice;

use crate::runner::apply_gas_params;

/// Deployed bytecode of RepriceProbe (optimizer OFF, via_ir=false).
const PROBE_DEPLOYED_BYTECODE_HEX: &str =
    "608060405234801561000f575f5ffd5b5060043610610029575f3560e01c806315509d2b1461002d575b5f5ffd5b61004760048036038101906100429190610132565b61005d565b60405161005491906101a5565b60405180910390f35b5f60079050805f5b86811015610099576003810183049150600782059150600d818301069150815f5260205f2083019250600181019050610065565b505f5b858110156100b55780548301925060018101905061009c565b505f5b848110156100d1575f54830192506001810190506100b8565b505f5b838110156100f15760018301816103e801556001810190506100d4565b5050949350505050565b5f5ffd5b5f819050919050565b610111816100ff565b811461011b575f5ffd5b50565b5f8135905061012c81610108565b92915050565b5f5f5f5f6080858703121561014a576101496100fb565b5b5f6101578782880161011e565b94505060206101688782880161011e565b93505060406101798782880161011e565b925050606061018a8782880161011e565b91505092959194509250565b61019f816100ff565b82525050565b5f6020820190506101b85f830184610196565b9291505056fea2646970667358221220c046bfb1400f8b82fff72852f7cf41941134583e68f931408fcebb8dc680cc0a64736f6c63430008230033";

/// Deterministic address for the probe contract.
pub const PROBE_ADDRESS: Address = Address::new([
    0xc0, 0xde, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01,
]);

/// Caller address funded with ample ETH.
pub const CALLER_ADDRESS: Address = Address::new([
    0xca, 0x11, 0xee, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01,
]);

/// `run(uint256,uint256,uint256,uint256)` selector.
pub const RUN_SELECTOR: [u8; 4] = [0x15, 0x50, 0x9d, 0x2b];

/// Build the CacheDB for the synthetic probe fixture.
pub fn build_synthetic_db() -> CacheDB<EmptyDB> {
    let mut db = CacheDB::new(EmptyDB::default());

    let code_bytes = hex::decode(PROBE_DEPLOYED_BYTECODE_HEX).expect("valid bytecode hex");
    let bytecode = Bytecode::new_raw(Bytes::from(code_bytes));
    db.insert_account_info(
        PROBE_ADDRESS,
        AccountInfo {
            balance: U256::ZERO,
            nonce: 1,
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode),
            account_id: None,
        },
    );

    db.insert_account_info(
        CALLER_ADDRESS,
        AccountInfo {
            balance: U256::from(u128::MAX),
            nonce: 0,
            code_hash: KECCAK_EMPTY,
            code: None,
            account_id: None,
        },
    );

    db
}

/// Build CfgEnv / BlockEnv / TxEnv for a synthetic probe call.
///
/// * `compute_iters`   — iterations of the compute loop (1×DIV+1×SDIV+1×MOD+1×KECCAK256 each)
/// * `cold_reads`      — distinct storage slots read once (cold SLOAD each)
/// * `warm_reads`      — re-reads of slot 0 (warm SLOAD; requires cold_reads >= 1)
/// * `new_slot_writes` — 0→nonzero SSTORE to virgin slots 1000..1000+n (EIP-8037 state gas)
pub fn build_synthetic_envs(
    compute_iters: u64,
    cold_reads: u64,
    warm_reads: u64,
    new_slot_writes: u64,
    schedule: &GasSchedule,
) -> (CfgEnv, BlockEnv, TxEnv) {
    let mut cfg = CfgEnv::new_with_spec(schedule.spec);
    cfg.disable_nonce_check = true;
    cfg.disable_balance_check = true;
    cfg.disable_base_fee = true;
    apply_gas_params(&mut cfg.gas_params, schedule);

    let block = BlockEnv {
        number: U256::from(21_000_000u64),
        beneficiary: Address::ZERO,
        timestamp: U256::from(1_700_000_000u64),
        gas_limit: 30_000_000,
        basefee: 0,
        difficulty: U256::ZERO,
        prevrandao: Some(B256::ZERO),
        blob_excess_gas_and_price: Some(BlobExcessGasAndPrice::new_with_spec(0, SpecId::PRAGUE)),
        slot_num: 0,
    };

    let calldata = abi_encode_run(compute_iters, cold_reads, warm_reads, new_slot_writes);
    let tx = TxEnv {
        caller: CALLER_ADDRESS,
        kind: TxKind::Call(PROBE_ADDRESS),
        data: Bytes::from(calldata),
        value: U256::ZERO,
        gas_limit: 30_000_000,
        gas_price: 0u128,
        gas_priority_fee: None,
        access_list: AccessList(vec![]),
        ..TxEnv::default()
    };

    (cfg, block, tx)
}

fn abi_encode_run(compute_iters: u64, cold_reads: u64, warm_reads: u64, new_slot_writes: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + 4 * 32);
    data.extend_from_slice(&RUN_SELECTOR);
    for v in [compute_iters, cold_reads, warm_reads, new_slot_writes] {
        let mut word = [0u8; 32];
        word[24..].copy_from_slice(&v.to_be_bytes());
        data.extend_from_slice(&word);
    }
    data
}

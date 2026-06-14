use std::collections::HashMap;

use gas_schedule::GasSchedule;
use revm_interpreter::{interpreter_types::{InputsTr, Jumps, StackTr}, Interpreter, InterpreterTypes};
use revm_primitives::{Address, U256};

/// Observing inspector that counts opcode executions for gas breakdown.
///
/// Does NOT charge gas — only records execution counts.
#[derive(Debug, Clone)]
pub struct OpcodeCounter {
    counts: [u64; 256],
}

impl Default for OpcodeCounter {
    fn default() -> Self {
        Self { counts: [0u64; 256] }
    }
}

impl OpcodeCounter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, opcode: u8) {
        self.counts[opcode as usize] += 1;
    }

    pub fn count(&self, opcode: u8) -> u64 {
        self.counts[opcode as usize]
    }

    /// Total SLOAD executions (cold + warm combined).
    pub fn sload_count(&self) -> u64 {
        self.count(0x54)
    }

    /// Total SSTORE executions (all categories combined).
    pub fn sstore_count(&self) -> u64 {
        self.count(0x55)
    }

    /// Gas attributable to compute opcodes under the given schedule (static
    /// GasTable portion only — does not include per-word/per-byte dynamic costs).
    pub fn compute_gas_used(&self, schedule: &GasSchedule) -> u64 {
        self.count(0x02) * schedule.mul
            + self.count(0x04) * schedule.div
            + self.count(0x05) * schedule.sdiv
            + self.count(0x06) * schedule.r#mod
            + self.count(0x07) * schedule.smod
            + self.count(0x08) * schedule.addmod
            + self.count(0x09) * schedule.mulmod
            + self.count(0x0a) * schedule.exp_base
            + self.count(0x20) * schedule.keccak256_base
    }
}

impl<CTX, INTR: InterpreterTypes> revm_inspector::Inspector<CTX, INTR> for OpcodeCounter
where
    INTR::Bytecode: Jumps,
{
    fn step(&mut self, interp: &mut Interpreter<INTR>, _ctx: &mut CTX) {
        self.record(interp.bytecode.opcode());
    }
}

// ── SSTORE classifier ─────────────────────────────────────────────────────────

/// Per-category SSTORE counts for a single transaction replay.
///
/// EIP-8037 state gas fires only on "set" writes (first write, original == 0, new != 0).
/// The split between `set_in_prestate` / `set_not_in_prestate` distinguishes slots that
/// were captured by prestateTracer (accessed in the original tx) from slots that were
/// absent from the fixture (never accessed before this write in the original tx).
#[derive(Debug, Default)]
pub struct SstoreStats {
    /// First write, original == 0, new != 0 — slot WAS in prestate fixture (as 0x0)
    pub set_in_prestate: u64,
    /// First write, original == 0, new != 0 — slot NOT in prestate fixture at all
    pub set_not_in_prestate: u64,
    /// First write, original != 0, new != 0 (nonzero → nonzero)
    pub reset: u64,
    /// First write, original != 0, new == 0 (clearing — gives refund)
    pub clear: u64,
    /// Re-write: slot was already written earlier in this tx (present != original)
    pub re_dirty: u64,
    /// No-op write: new == present (writing the same value that's already there)
    pub noop: u64,
}

impl SstoreStats {
    /// All writes where original == 0 and new != 0 (EIP-8037 state gas candidates).
    pub fn total_set(&self) -> u64 {
        self.set_in_prestate + self.set_not_in_prestate
    }

    pub fn total(&self) -> u64 {
        self.set_in_prestate + self.set_not_in_prestate + self.reset + self.clear + self.re_dirty + self.noop
    }
}

/// Inspector that classifies each SSTORE by its EIP-8037 type.
///
/// Must be constructed with the pre-tx initial storage values extracted from the
/// prestate fixture (see `build_initial_storage`).
pub struct SstoreClassifier {
    /// (contract_address, slot_key) → initial value (before this tx started).
    /// Slots absent from this map have initial value 0 and were not in the prestate.
    initial: HashMap<(Address, U256), U256>,
    /// Tracks the most recently written value for each slot in this tx.
    ever_written: HashMap<(Address, U256), U256>,
    pub stats: SstoreStats,
}

impl SstoreClassifier {
    pub fn new(initial: HashMap<(Address, U256), U256>) -> Self {
        Self { initial, ever_written: HashMap::new(), stats: SstoreStats::default() }
    }

    fn record_sstore(&mut self, addr: Address, key: U256, new_val: U256) {
        let initial_val = self.initial.get(&(addr, key)).copied().unwrap_or(U256::ZERO);
        let in_prestate = self.initial.contains_key(&(addr, key));

        if let Some(&present_val) = self.ever_written.get(&(addr, key)) {
            if new_val == present_val {
                self.stats.noop += 1;
            } else {
                self.stats.re_dirty += 1;
            }
        } else if initial_val.is_zero() {
            if new_val.is_zero() {
                self.stats.noop += 1;
            } else if in_prestate {
                self.stats.set_in_prestate += 1;
            } else {
                self.stats.set_not_in_prestate += 1;
            }
        } else if new_val.is_zero() {
            self.stats.clear += 1;
        } else {
            self.stats.reset += 1;
        }

        self.ever_written.insert((addr, key), new_val);
    }
}

impl<CTX, INTR: InterpreterTypes> revm_inspector::Inspector<CTX, INTR> for SstoreClassifier
where
    INTR::Bytecode: Jumps,
{
    fn step(&mut self, interp: &mut Interpreter<INTR>, _ctx: &mut CTX) {
        if interp.bytecode.opcode() != 0x55 {
            return;
        }
        let d = interp.stack.data();
        if d.len() >= 2 {
            let key = d[d.len() - 1];
            let new_val = d[d.len() - 2];
            let addr = interp.input.target_address();
            self.record_sstore(addr, key, new_val);
        }
    }
}

// ── Per-SSTORE address tracer ─────────────────────────────────────────────────

/// Records (contract_address, storage_key, new_value) for every SSTORE opcode.
#[derive(Debug, Default)]
pub struct SstoreAddressTracer {
    pub entries: Vec<(Address, U256, U256)>,
}

impl SstoreAddressTracer {
    pub fn new() -> Self { Self::default() }
}

impl<CTX, INTR: InterpreterTypes> revm_inspector::Inspector<CTX, INTR> for SstoreAddressTracer
where
    INTR::Bytecode: Jumps,
{
    fn step(&mut self, interp: &mut Interpreter<INTR>, _ctx: &mut CTX) {
        if interp.bytecode.opcode() != 0x55 { return; }
        let d = interp.stack.data();
        if d.len() >= 2 {
            self.entries.push((
                interp.input.target_address(),
                d[d.len() - 1],
                d[d.len() - 2],
            ));
        }
    }
}

// ── Gas-tracking inspector ────────────────────────────────────────────────────

/// Records gas remaining BEFORE each SSTORE/CALL and AFTER (via step_end),
/// to break down exact gas charges per opcode category.
#[derive(Debug, Default)]
pub struct GasBreakdown {
    pub sstore_gas: i64,
    pub call_gas: i64,
    pub other_gas: i64,
    pub sstore_charges: Vec<i64>,
    gas_before: u64,
    pending_opcode: u8,
}

impl GasBreakdown {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<CTX, INTR: InterpreterTypes> revm_inspector::Inspector<CTX, INTR> for GasBreakdown
where
    INTR::Bytecode: Jumps,
{
    fn step(&mut self, interp: &mut Interpreter<INTR>, _ctx: &mut CTX) {
        self.pending_opcode = interp.bytecode.opcode();
        self.gas_before = interp.gas.remaining();
    }

    fn step_end(&mut self, interp: &mut Interpreter<INTR>, _ctx: &mut CTX) {
        let gas_after = interp.gas.remaining();
        let gas_charged = self.gas_before.saturating_sub(gas_after) as i64;
        match self.pending_opcode {
            0x55 => {
                self.sstore_gas += gas_charged;
                self.sstore_charges.push(gas_charged);
            }
            0xf1 | 0xf2 | 0xf4 | 0xfa | 0xf0 | 0xf5 => self.call_gas += gas_charged,
            _ => self.other_gas += gas_charged,
        }
    }
}

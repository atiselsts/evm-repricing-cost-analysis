use gas_schedule::GasSchedule;
use revm_interpreter::{interpreter_types::Jumps, Interpreter, InterpreterTypes};

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

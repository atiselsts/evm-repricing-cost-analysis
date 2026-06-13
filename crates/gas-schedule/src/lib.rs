/// All repriceable gas parameters for a single EVM schedule.
///
/// Baseline values reproduce the current mainnet gas schedule exactly.
/// EIP presets overwrite the relevant entries; all others stay at baseline.
#[derive(Clone, Debug)]
pub struct GasSchedule {
    // ── EIP-7904 compute opcodes (static gas table entries) ─────────────────
    /// MUL cost (baseline 5)
    pub mul: u64,
    /// DIV cost (baseline 5, eip7904 15)
    pub div: u64,
    /// SDIV cost (baseline 5, eip7904 20)
    pub sdiv: u64,
    /// MOD cost (baseline 5, eip7904 12)
    pub r#mod: u64,
    /// SMOD cost (baseline 5)
    pub smod: u64,
    /// ADDMOD cost (baseline 8)
    pub addmod: u64,
    /// MULMOD cost (baseline 8)
    pub mulmod: u64,
    /// EXP base cost (baseline 10; per-byte cost 50 is unchanged)
    pub exp_base: u64,
    /// KECCAK256 base cost (baseline 30, eip7904 45; per-word cost 6 unchanged)
    pub keccak256_base: u64,

    // ── EIP-8038 state-read (SLOAD) ──────────────────────────────────────────
    /// Warm SLOAD base in the static gas table (baseline 100)
    pub warm_access_cost: u64,
    /// Cold SLOAD total (baseline 2100 = 100 warm + 2000 surcharge)
    pub cold_sload_total: u64,
}

impl GasSchedule {
    /// Current mainnet gas schedule — reproduces receipt.gas_used exactly.
    pub fn baseline() -> Self {
        Self {
            mul: 5,
            div: 5,
            sdiv: 5,
            r#mod: 5,
            smod: 5,
            addmod: 8,
            mulmod: 8,
            exp_base: 10,
            keccak256_base: 30,
            warm_access_cost: 100,
            cold_sload_total: 2_100,
        }
    }

    /// EIP-7904 compute repricing only. Values from CLAUDE.md project spec.
    pub fn eip7904() -> Self {
        Self {
            div: 15,
            sdiv: 20,
            r#mod: 12,
            keccak256_base: 45,
            ..Self::baseline()
        }
    }

    /// EIP-8038 SLOAD repricing only — EF worst-case 3× scenario.
    pub fn eip8038() -> Self {
        Self {
            warm_access_cost: 300,
            cold_sload_total: 6_300,
            ..Self::baseline()
        }
    }

    /// Combined EIP-7904 + EIP-8038 repricing.
    pub fn eip7904_plus_8038() -> Self {
        Self {
            div: 15,
            sdiv: 20,
            r#mod: 12,
            keccak256_base: 45,
            warm_access_cost: 300,
            cold_sload_total: 6_300,
            ..Self::baseline()
        }
    }

    /// EIP-8038 SLOAD costs scaled for a 200 M gas-limit block.
    ///
    /// Rationale: if EIP-8038 costs (warm=300, cold=6300) are calibrated for
    /// the 60 M gas limit at which the fixture was captured, a 200 M gas limit
    /// increases state I/O pressure per block by ≈ 200/60 = 3.33×, requiring
    /// proportionally higher per-op costs to keep the same validator load bound.
    /// Rounded to nice numbers: warm 300 × 3.33 ≈ 1000, cold 6300 × 3.33 ≈ 21000
    /// (≈ 10× baseline; values remain TBD in the draft EIP).
    pub fn eip8038_200m() -> Self {
        Self {
            warm_access_cost: 1_000,
            cold_sload_total: 21_000,
            ..Self::baseline()
        }
    }

    /// Combined EIP-7904 + EIP-8038 costs scaled for a 200 M gas-limit block.
    pub fn eip7904_plus_8038_200m() -> Self {
        Self {
            div: 15,
            sdiv: 20,
            r#mod: 12,
            keccak256_base: 45,
            warm_access_cost: 1_000,
            cold_sload_total: 21_000,
            ..Self::baseline()
        }
    }

    /// Cold SLOAD surcharge = total cold cost minus warm base.
    pub fn cold_sload_surcharge(&self) -> u64 {
        self.cold_sload_total - self.warm_access_cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_surcharge() {
        let b = GasSchedule::baseline();
        assert_eq!(b.cold_sload_surcharge(), 2_000);
    }

    #[test]
    fn eip8038_surcharge() {
        let s = GasSchedule::eip8038();
        assert_eq!(s.cold_sload_surcharge(), 6_000);
    }
}

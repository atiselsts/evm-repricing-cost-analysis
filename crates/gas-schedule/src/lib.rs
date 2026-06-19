use revm_primitives::hardfork::SpecId;

/// All repriceable gas parameters for a single EVM schedule.
///
/// Baseline values reproduce the current mainnet gas schedule exactly.
/// EIP presets overwrite the relevant entries; all others stay at baseline.
#[derive(Clone, Debug)]
pub struct GasSchedule {
    /// EVM spec to use when constructing CfgEnv. PRAGUE for all mainnet-compatible
    /// schedules; AMSTERDAM activates EIP-8037 state gas (new slot creation).
    pub spec: SpecId,

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

    // ── EIP-8038 state-access (cold SSTORE) ─────────────────────────────────
    /// Cold SSTORE total — overrides GasId::cold_storage_cost, which is shared
    /// with SLOAD cold access. Kept equal to cold_sload_total when repricing both.
    pub cold_sstore_total: u64,

    // ── EIP-8038 SSTORE write cost (PR #11802) ───────────────────────────────
    /// STORAGE_WRITE — the per-write cost for every SSTORE (warm or cold),
    /// charged in addition to the warm static and any cold access surcharge.
    /// Overrides GasId::sstore_reset_without_cold_load_cost and
    /// GasId::sstore_set_without_load_cost (both equal under the new model).
    /// None = use the spec default (2,800 for BERLIN+/AMSTERDAM).
    pub sstore_write_cost: Option<u64>,

    /// STORAGE_CLEAR_REFUND — refund for clearing a storage slot (nonzero→zero).
    /// None = use the spec default (4,800 for LONDON/PRAGUE/AMSTERDAM).
    pub sstore_clearing_refund: Option<u64>,
}

impl GasSchedule {
    /// Current mainnet gas schedule — reproduces receipt.gas_used exactly.
    pub fn baseline() -> Self {
        Self {
            spec: SpecId::PRAGUE,
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
            cold_sstore_total: 2_100,
            sstore_write_cost: None,
            sstore_clearing_refund: None,
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

    /// EIP-8038 SLOAD repricing only — EF worst-case 3× scenario (PRAGUE spec).
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

    /// EIP-8038 SLOAD costs scaled for a 200 M gas-limit block (PRAGUE spec).
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

    // ── S6: AMSTERDAM-based presets ──────────────────────────────────────────

    /// EIP-8037 state gas only: AMSTERDAM spec, no GasParams overrides.
    ///
    /// Switching to AMSTERDAM automatically enables the state gas model:
    /// each new storage slot (0→nonzero SSTORE) charges an additional
    /// 97,920 gas (SSTORE_SET_BYTES=64 × CPSB_GLAMSTERDAM=1530).
    pub fn eip8037() -> Self {
        Self {
            spec: SpecId::AMSTERDAM,
            ..Self::baseline()
        }
    }

    /// EIP-8037 + EIP-8038 3× cold access (AMSTERDAM spec).
    ///
    /// cold_sload_total and cold_sstore_total both raised to 6300 so that
    /// cold SLOAD and cold SSTORE are repriced symmetrically.
    pub fn eip8038_sstore() -> Self {
        Self {
            spec: SpecId::AMSTERDAM,
            warm_access_cost: 300,
            cold_sload_total: 6_300,
            cold_sstore_total: 6_300,
            ..Self::baseline()
        }
    }

    /// EIP-8037 + EIP-8038 scaled for a 200 M gas-limit block (AMSTERDAM spec).
    pub fn eip8038_sstore_200m() -> Self {
        Self {
            spec: SpecId::AMSTERDAM,
            warm_access_cost: 1_000,
            cold_sload_total: 21_000,
            cold_sstore_total: 21_000,
            ..Self::baseline()
        }
    }

    /// EIP-8038 PR #11802 merged values, 60 M block (AMSTERDAM spec).
    ///
    /// COLD_STORAGE_ACCESS 2100→3000, WARM_ACCESS 100 (unchanged),
    /// STORAGE_WRITE 2800→10000 (applies to every SSTORE write),
    /// STORAGE_CLEAR_REFUND 4800→12480.
    pub fn eip8038_pr11802() -> Self {
        Self {
            spec: SpecId::AMSTERDAM,
            warm_access_cost: 100,
            cold_sload_total: 3_000,
            cold_sstore_total: 3_000,
            sstore_write_cost: Some(10_000),
            sstore_clearing_refund: Some(12_480),
            ..Self::baseline()
        }
    }

    /// EIP-8038 PR #11802 values scaled to 200 M block (AMSTERDAM spec).
    ///
    /// All costs scaled by 200/60 ≈ 3.33× from the 60 M values:
    /// WARM_ACCESS 300, COLD_STORAGE_ACCESS 10000, STORAGE_WRITE 33000,
    /// STORAGE_CLEAR_REFUND 41500.
    pub fn eip8038_pr11802_200m() -> Self {
        Self {
            spec: SpecId::AMSTERDAM,
            warm_access_cost: 300,
            cold_sload_total: 10_000,
            cold_sstore_total: 10_000,
            sstore_write_cost: Some(33_000),
            sstore_clearing_refund: Some(41_500),
            ..Self::baseline()
        }
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Cold SLOAD surcharge = total cold SLOAD cost minus warm base.
    pub fn cold_sload_surcharge(&self) -> u64 {
        self.cold_sload_total - self.warm_access_cost
    }

    /// Cold SSTORE cost — used to override GasId::cold_storage_cost.
    ///
    /// revm always charges `sstore_static` (the warm access) and *adds*
    /// `cold_storage_cost` on top when the slot is cold. Two models exist:
    ///
    /// * Legacy / EIP-2929 (baseline, EIP-8037): the cold surcharge is added on
    ///   top of the warm-inclusive operation cost, so `cold_storage_cost` is the
    ///   full cold total (e.g. mainnet 2100). Reproduces `receipt.gas_used`.
    /// * EIP-8038 restructured model (`sstore_write_cost` set): access cost is
    ///   `COLD_STORAGE_ACCESS` *or* `WARM_ACCESS` (mutually exclusive), so the
    ///   total cold access must be `cold_sstore_total`; subtract the
    ///   always-charged warm static, mirroring `cold_sload_surcharge`.
    pub fn cold_sstore_cost(&self) -> u64 {
        if self.sstore_write_cost.is_some() {
            self.cold_sstore_total - self.warm_access_cost
        } else {
            self.cold_sstore_total
        }
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

    #[test]
    fn eip8037_uses_amsterdam() {
        assert_eq!(GasSchedule::eip8037().spec, SpecId::AMSTERDAM);
    }

    #[test]
    fn eip8038_sstore_symmetric() {
        let s = GasSchedule::eip8038_sstore();
        assert_eq!(s.cold_sload_total, s.cold_sstore_total);
    }
}

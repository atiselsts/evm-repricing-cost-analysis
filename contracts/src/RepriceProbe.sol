// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title RepriceProbe
/// @notice Synthetic probe for the gas-repricing harness. Executes a TUNABLE,
///         KNOWN number of repriced opcodes so the harness can verify the
///         repriced gas delta exactly against a hand-computable target.
///
/// Repriced opcodes exercised:
///   EIP-7904 (compute):   DIV, SDIV, MOD, KECCAK256 (fixed 1-word input)
///   EIP-8038 (state read): cold SLOAD, warm SLOAD
///
/// BUILD RULES (critical — otherwise counts drift):
///   * OPTIMIZER OFF and via_ir = false. The legacy optimizer hoists loop
///     invariants, applies common-subexpression elimination to SLOAD, and
///     unrolls loops — any of which breaks the opcode counts. With it off,
///     the assembly below emits one opcode per source occurrence per iteration.
///   * KECCAK256 input length is fixed at 32 bytes (1 word) so its dynamic
///     per-word cost is constant; only the base (30->45) moves under 7904.
///
/// REPLAY RULES (for the harness building the synthetic tx):
///   * Build the calling tx with an EMPTY access list, or the "cold" slots get
///     pre-warmed and the cold/warm split is wrong.
///   * Storage need NOT be pre-populated: SLOAD gas is independent of the
///     stored value, so reading zeroed slots costs the same cold/warm gas.
///   * Call with coldReads >= 1 so slot 0 is already warm before the warm phase.
contract RepriceProbe {
    /// @param computeIters compute-loop iterations; each iteration runs exactly
    ///        one DIV, one SDIV, one MOD, and one KECCAK256(32 bytes)
    /// @param coldReads    number of DISTINCT slots read once each -> cold SLOADs
    /// @param warmReads    number of repeat reads of slot 0 -> warm SLOADs
    /// @return acc derived from every op so nothing is dead-code eliminated
    function run(uint256 computeIters, uint256 coldReads, uint256 warmReads)
        external
        view
        returns (uint256 acc)
    {
        acc = 7;
        assembly {
            // --- compute phase: DIV, SDIV, MOD, KECCAK256 per iteration ---
            let a := acc
            for { let i := 0 } lt(i, computeIters) { i := add(i, 1) } {
                a := div(acc, add(i, 3))                 // DIV   (5 -> 15)
                a := sdiv(a, 7)                          // SDIV  (5 -> 20)
                a := mod(add(a, i), 13)                  // MOD   (5 -> 12)
                mstore(0x00, a)                          // scratch word, constant cost
                acc := add(acc, keccak256(0x00, 0x20))   // KECCAK256 base (30 -> 45)
            }
            // --- cold SLOAD phase: distinct slots 0..coldReads-1, first touch ---
            for { let i := 0 } lt(i, coldReads) { i := add(i, 1) } {
                acc := add(acc, sload(i))                // cold (each slot new)
            }
            // --- warm SLOAD phase: slot 0 repeatedly (warm if coldReads >= 1) ---
            for { let i := 0 } lt(i, warmReads) { i := add(i, 1) } {
                acc := add(acc, sload(0))                // warm (already accessed)
            }
        }
    }
}

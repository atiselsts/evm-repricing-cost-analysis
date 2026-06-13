// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Test.sol";
import "../src/RepriceProbe.sol";

/// Tests for RepriceProbe — focus on loop termination and opcode-count linearity.
///
/// Each test calls run() with a bounded gas limit so an infinite loop causes an
/// explicit "out of gas" failure rather than hanging the test runner.
contract RepriceProbeTest is Test {
    // Generous but finite cap per call; a correct 100-iter run uses << 500k gas.
    uint256 constant GAS_CAP = 5_000_000;

    RepriceProbe probe;

    function setUp() public {
        probe = new RepriceProbe();
    }

    // ── smoke: zero iterations ──────────────────────────────────────────────

    function test_zero_iters_returns() public view {
        uint256 acc = probe.run{gas: GAS_CAP}(0, 0, 0);
        // acc starts at 7 and no ops mutate it → must still be 7
        assertEq(acc, 7, "zero-iter acc should equal initial value 7");
    }

    // ── compute loop termination ────────────────────────────────────────────

    function test_compute_1_iter() public view {
        probe.run{gas: GAS_CAP}(1, 0, 0);
    }

    function test_compute_10_iters() public view {
        probe.run{gas: GAS_CAP}(10, 0, 0);
    }

    function test_compute_100_iters() public view {
        probe.run{gas: GAS_CAP}(100, 0, 0);
    }

    /// Gas grows roughly linearly with compute iterations.
    /// We measure the per-iteration cost and assert it is within [50, 1000] gas —
    /// loose enough to survive Solidity loop overhead, tight enough to catch
    /// an infinite loop (which would hit GAS_CAP immediately).
    function test_compute_gas_linear() public view {
        // Warmup: first call is cold (2600 gas CALL overhead) which would skew g0;
        // subsequent calls are warm (100 gas), making the delta meaningful.
        probe.run{gas: GAS_CAP}(0, 0, 0);

        uint256 g0 = _gasFor(0, 0, 0);
        uint256 g1 = _gasFor(1, 0, 0);
        uint256 g10 = _gasFor(10, 0, 0);

        // Each iteration should cost between 50 and 1 000 gas.
        uint256 costPer1 = g1 - g0;
        assertGt(costPer1, 50,  "single compute iter costs too little (loop may not run)");
        assertLt(costPer1, 1_000, "single compute iter costs too much (possible loop bug)");

        // 10-iter minus 1-iter ≈ 9 × per-iteration cost.
        uint256 delta = g10 - g1;
        assertApproxEqRel(delta, costPer1 * 9, 0.20e18, "gas should scale ~linearly with iters");
    }

    // ── cold SLOAD loop termination ─────────────────────────────────────────

    function test_cold_1_slot() public view {
        probe.run{gas: GAS_CAP}(0, 1, 0);
    }

    function test_cold_5_slots() public view {
        probe.run{gas: GAS_CAP}(0, 5, 0);
    }

    function test_cold_50_slots() public view {
        probe.run{gas: GAS_CAP}(0, 50, 0);
    }

    /// Gas grows linearly with cold-SLOAD count (≈ 2100 gas each in baseline).
    function test_cold_sload_gas_linear() public view {
        probe.run{gas: GAS_CAP}(0, 0, 0); // warm up the address

        uint256 g0  = _gasFor(0, 0, 0);
        uint256 g1  = _gasFor(0, 1, 0);
        uint256 g10 = _gasFor(0, 10, 0);

        uint256 costPer1 = g1 - g0;
        // A cold SLOAD costs 2100; loop overhead adds a bit more.
        assertGt(costPer1, 2_000, "first cold SLOAD too cheap (loop may not execute)");
        assertLt(costPer1, 5_000, "first cold SLOAD too expensive (unexpected overhead)");

        uint256 delta = g10 - g1;
        assertApproxEqRel(delta, costPer1 * 9, 0.20e18, "cold SLOAD gas should scale ~linearly");
    }

    // ── warm SLOAD loop termination ─────────────────────────────────────────

    /// Requires coldReads >= 1 so slot 0 is warm before the warm phase.
    function test_warm_sload_3_reads() public view {
        probe.run{gas: GAS_CAP}(0, 1, 3);
    }

    function test_warm_sload_10_reads() public view {
        probe.run{gas: GAS_CAP}(0, 1, 10);
    }

    /// Warm SLOADs (100 gas each) are cheaper than cold ones (2100 gas each).
    function test_warm_cheaper_than_cold() public view {
        // 1 cold + 9 warm vs 10 cold
        uint256 g_mixed = _gasFor(0, 1, 9);   // 1 cold + 9 warm
        uint256 g_cold  = _gasFor(0, 10, 0);  // 10 cold

        assertLt(g_mixed, g_cold, "1 cold + 9 warm should be cheaper than 10 cold");
    }

    // ── combined loops ───────────────────────────────────────────────────────

    function test_all_loops_combined() public view {
        probe.run{gas: GAS_CAP}(10, 5, 3);
    }

    // ── fuzz: loop always terminates within GAS_CAP ─────────────────────────

    /// Fuzz over small parameter ranges; any OOG would revert and fail the test.
    function testFuzz_terminates(uint8 n, uint8 cold, uint8 warm) public view {
        // cap at 50 iters each to stay well within GAS_CAP
        uint256 ni = bound(n, 0, 50);
        uint256 c  = bound(cold, 0, 50);
        uint256 w  = bound(warm, 0, 50);
        probe.run{gas: GAS_CAP}(ni, c, w);
    }

    // ── helper ───────────────────────────────────────────────────────────────

    function _gasFor(uint256 n, uint256 cold, uint256 warm) internal view returns (uint256) {
        uint256 before = gasleft();
        probe.run{gas: GAS_CAP}(n, cold, warm);
        return before - gasleft();
    }
}

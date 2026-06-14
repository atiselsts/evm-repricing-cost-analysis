# Glamsterdam Gas Repricing Impact

Gas cost impact of EIP-8037 (SSTORE state gas) and EIP-8038 (state-access repricing)
on four Ethereum mainnet transactions — one Aave v3 liquidation and three AMM arbitrage
transactions — replayed with the revm harness.

Raw data: `results/liquidation-repricing.csv`, `results/amm-arb-repricing.csv`.

---

## Fixtures

| Label | Tx hash | Category | Receipt gas |
|---|---|---|---:|
| `liquidation` | `0x7b53e9...629` | Aave v3 liquidationCall | 781 399 |
| `simple` | `0x7ab274...c0a` | Atomic AMM arb | 194 977 |
| `intermediate` | `0x8687c5...875` | Intermediate AMM arb | 1 884 138 |
| `complex` | `0xa5f2e9...bee` | Complex AMM arb | 10 018 653 |

All fixtures captured from Ethereum mainnet (chain id 1, ~60 M block gas limit)
with `harvest_prestate.py` using `debug_traceTransaction` + `prestateTracer`.

---

## Methodology

**Uniform gas limit — TX_GAS_LIMIT_CAP = 16 777 216 (2²⁴, EIP-7825)**

All replays use `--tx-gas-limit 16777216` regardless of the on-chain gas limit in the
fixture. EIP-7825 (Glamsterdam) enforces this cap universally, so the correct
counterfactual is not "would this tx survive on its original budget" but "how much gas
does this tx consume in the EIP-7825 world." Using the natural on-chain limit would give
artificially suppressed numbers for transactions whose repriced cost exceeds their
original budget.

**`path_diverged`**

When gas costs increase enough that the tx saturates `TX_GAS_LIMIT_CAP`, inner subcalls
may fail OOG and the tx takes a different internal code path from baseline. The
`path_diverged` flag is set when the compute-opcode gas differs from the baseline run,
which is a reliable proxy for path divergence. A `path_diverged=yes` result should be
interpreted as "tx completes at the outer level but some inner logic was skipped or
reverted; the reported gas reflects that degraded execution, not the full original logic."

---

## Baseline fidelity

Replaying under the `baseline` PRAGUE schedule at the natural tx gas limit exactly
reproduces `receipt.gas_used` for all four fixtures. (At `TX_GAS_LIMIT_CAP` the numbers
are identical since each fixture's natural limit exceeds its actual usage.)

| Label | Receipt gas | Harness baseline | Match |
|---|---:|---:|:---:|
| `liquidation` | 781 399 | 781 399 | ✓ |
| `simple` | 194 977 | 194 977 | ✓ |
| `intermediate` | 1 884 138 | 1 884 138 | ✓ |
| `complex` | 10 018 653 | 10 018 653 | ✓ |

---

## EIP-8037 — SSTORE state gas (AMSTERDAM spec)

Each new-slot 0→nonzero SSTORE to a slot that does not exist in the world state incurs
an additional 97 920 gas (SSTORE_SET_BYTES=64 × CPSB_GLAMSTERDAM=1 530).
The `sstore_set_without_load_cost` also drops from 19 900 to 2 800 under AMSTERDAM,
giving a net per-*new*-slot delta of **+80 820 gas**. Writes to slots that already
exist in the state trie (even if their value is zero) pay only 2 800 instead of 19 900,
saving 17 100 gas per such write with no offsetting state gas charge.

| Label | baseline gas | eip8037 gas | Δ gas | Δ% |
|---|---:|---:|---:|---:|
| `liquidation` | 781 399 | 781 399 | 0 | 0.00% |
| `simple` | 194 977 | 194 977 | 0 | 0.00% |
| `intermediate` | 1 884 138 | 1 884 138 | 0 | 0.00% |
| `complex` | 10 018 653 | 10 180 293 | +161 640 | +1.61% |

`liquidation`, `simple`, and `intermediate` write only to slots that already exist in the
world state (or have been written earlier in the tx), so the `sstore_set_without_load_cost`
change and the new state gas exactly cancel, giving Δ = 0.

`complex` is the exception: it creates new storage slots (161 640 / 80 820 ≈ 2 new slots),
so EIP-8037 adds a small but nonzero charge. Even so, the +1.61% increase is small relative
to the SLOAD repricing effect shown below.

---

## EIP-8038 — cold SLOAD repricing, 60 M block (3× multiplier, PRAGUE)

warm SLOAD 100 → 300; cold SLOAD 2 100 → 6 300. PRAGUE spec (SSTORE costs unchanged).

| Label | baseline gas | eip8038 gas | Δ gas | Δ% | path_diverged |
|---|---:|---:|---:|---:|:---:|
| `liquidation` | 781 399 | 1 261 199 | +479 800 | +61.40% | no |
| `simple` | 194 977 | 289 177 | +94 200 | +48.31% | no |
| `intermediate` | 1 884 138 | 2 996 938 | +1 112 800 | +59.06% | no |
| `complex` | 10 018 653 | 16 776 237 | +6 757 584 | +67.45% | **yes** |

All four transactions are SLOAD-heavy and see +48–67% increases. `complex` is the only
transaction that hits TX_GAS_LIMIT_CAP under this schedule (16 776 237 vs 16 777 216
cap) — it saturates the limit and some inner logic is skipped. The reported delta for
`complex` therefore reflects a degraded execution, not the full original code path.

---

## EIP-8037 + EIP-8038 combined — SSTORE also repriced, 60 M block (AMSTERDAM)

cold SSTORE 2 100 → 6 300 added on top of the SLOAD changes; AMSTERDAM spec activates
new-slot state gas **and** reduces `sstore_set_without_load_cost` from 19 900 to 2 800.

| Label | baseline gas | eip8038 (PRAGUE) | eip8038_sstore (AMSTERDAM) | Δ% vs baseline | path_diverged |
|---|---:|---:|---:|---:|:---:|
| `liquidation` | 781 399 | 1 261 199 | 1 265 399 | +61.94% | no |
| `simple` | 194 977 | 289 177 | 289 177 | +48.31% | no |
| `intermediate` | 1 884 138 | 2 996 938 | 2 996 938 | +59.06% | no |
| `complex` | 10 018 653 | 16 776 237 | 15 947 493 | +59.18% | no |

**`simple` and `intermediate`**: PRAGUE and AMSTERDAM give identical results. Neither
transaction has cold SSTOREs or new-slot SSTOREs at `TX_GAS_LIMIT_CAP`, so the AMSTERDAM
SSTORE restructuring has no effect.

**`liquidation`**: AMSTERDAM is marginally *more* expensive than PRAGUE (+1 265 399 vs
+1 261 199, a Δ of +4 200). This indicates 1 cold SSTORE (+4 200 surcharge from repricing)
and no new-slot SSTOREs to benefit from the restructuring saving. With `TX_GAS_LIMIT_CAP`
giving full execution budget, the AMSTERDAM restructuring provides no savings here.

**`complex`**: This is the most interesting case. Under PRAGUE eip8038, `complex`
saturates `TX_GAS_LIMIT_CAP` (`path_diverged=yes`, 16 776 237 gas). Under AMSTERDAM
eip8038_sstore, the same transaction uses only 15 947 493 gas (`path_diverged=no`).
The AMSTERDAM SSTORE cost restructuring — primarily the drop in `sstore_set_without_load_cost`
(19 900 → 2 800, saving 17 100 per first-write-to-zero SSTORE) — saves enough gas that
`complex` fits under the `TX_GAS_LIMIT_CAP` and completes its full original code path.
In other words: **for `complex`, switching from PRAGUE to AMSTERDAM can be the difference
between a truncated and a complete execution under EIP-7825**.

---

## EIP-8038 — 200 M block scaling (≈10× multiplier)

warm SLOAD 100 → 1 000; cold SLOAD 2 100 → 21 000; cold SSTORE 2 100 → 21 000
(AMSTERDAM spec for `eip8038_sstore200m`); `--block-gas-limit 200000000`.

| Label | baseline gas | eip8038_200m | Δ% | eip8038_sstore200m | Δ% | path_diverged (200m) |
|---|---:|---:|---:|---:|---:|:---:|
| `liquidation` | 781 399 | 2 940 499 | +276.31% | 2 959 399 | +278.73% | no / no |
| `simple` | 194 977 | 618 877 | +217.41% | 618 877 | +217.41% | no / no |
| `intermediate` | 1 884 138 | 6 891 738 | +265.78% | 6 891 738 | +265.78% | no / no |
| `complex` | 10 018 653 | 16 765 262 | +67.34% | 16 577 952 | +65.47% | **yes / yes** |

At 200 M block gas limit scaling, `liquidation`, `simple`, and `intermediate` all
complete their full execution paths within `TX_GAS_LIMIT_CAP` (path_diverged=no). Gas
increases of +218–278% reflect the full SLOAD-heavy cost of these transactions under
a 10× SLOAD multiplier.

`complex` saturates `TX_GAS_LIMIT_CAP` under both 200 M schedules (path_diverged=yes
for both). At 10× SLOAD costs, `complex` cannot fit within 16.8 M gas even under
AMSTERDAM's SSTORE savings — the transaction's SLOAD-heaviness dominates. The reported
gas figures reflect degraded executions.

---

## Cross-category summary (60 M block, TX_GAS_LIMIT_CAP)

| Schedule | Liquidation Δ% | Simple Δ% | Intermediate Δ% | Complex Δ% | Complex path |
|---|---:|---:|---:|---:|:---:|
| `eip8037` | 0.00% | 0.00% | 0.00% | +1.61% | no |
| `eip8038` (PRAGUE) | +61.40% | +48.31% | +59.06% | +67.45% | **yes** |
| `eip8038_sstore` (AMSTERDAM) | +61.94% | +48.31% | +59.06% | +59.18% | no |

Key observations:

1. **EIP-8037 alone** has negligible impact: 0% for three transactions; +1.61% for
   the complex arb (which creates 2 new storage slots). In all cases, SLOAD repricing
   dominates by a factor of ≥30×.

2. **SLOAD repricing** (EIP-8038, PRAGUE) raises costs by +48–67% across all four
   transactions. The spread reflects each transaction's mix of cold vs warm SLOADs.

3. **AMSTERDAM vs PRAGUE for `complex`**: AMSTERDAM's SSTORE restructuring prevents
   TX_GAS_LIMIT_CAP saturation — the transaction completes its full original code path
   under AMSTERDAM but not under PRAGUE. This is a qualitative difference, not just a
   cost difference.

4. **AMSTERDAM vs PRAGUE for the other three**: The difference is ≤0.5 percentage points
   (no first-write-to-zero SSTOREs to benefit from restructuring, at most 1 cold SSTORE
   for liquidation). For these transactions the spec choice is irrelevant.

5. **200 M block scaling**: `complex` cannot complete its full execution at 10× SLOAD
   costs regardless of spec. The other three complete fully, with +218–278% increases
   that scale approximately linearly with the SLOAD multiplier.

---

## Methodology notes

- All runs use **TX_GAS_LIMIT_CAP = 16 777 216** (EIP-7825, 2^24) as the uniform
  tx gas limit. This is not the transactions' on-chain gas limits.
- 200 M block schedules set `--block-gas-limit 200000000`.
- EIP-8038 multipliers (3× and ≈10×) are hypothetical; the EIP's constants are TBD.
- `path_diverged` is detected by comparing compute-opcode gas between the repriced
  and baseline runs; a difference indicates the tx took a different code path.
- SSTORE classification for EIP-8037: `complex` creates new slots confirmed by the
  `eip8037` − `baseline` delta of +161 640 (= 2 × 80 820 net per-slot cost).
  The other three fixtures show Δ = 0, confirming no new slot creation.

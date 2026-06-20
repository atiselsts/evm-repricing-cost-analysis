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

The `CPSB_GLAMSTERDAM` value of 1 530 is calibrated for a **150 M gas reference block**,
chosen as a forward-looking midpoint between the current ~60 M limit and an anticipated
~300 M future limit.

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

## EIP-8038 PR #11802 — merged values (AMSTERDAM spec)

These are the actual constants from the merged EIP-8038 PR #11802. Costs are derived from
empirical benchmarking at a 100 Mgas/s performance target and are not calibrated to a
specific block gas limit.

| Constant | Old | New |
|---|---:|---:|
| `WARM_ACCESS` | 100 | 100 (unchanged) |
| `COLD_STORAGE_ACCESS` | 2 100 | 3 000 |
| `STORAGE_WRITE` | 2 800 | 10 000 |
| `STORAGE_CLEAR_REFUND` | 4 800 | 12 480 |

| Label | baseline gas | eip8038_pr11802 gas | Δ gas | Δ% | path_diverged |
|---|---:|---:|---:|---:|:---:|
| `liquidation` | 781 399 | 1 122 399 | +341 000 | +43.6% | no |
| `simple` | 194 977 | 285 877 | +90 900 | +46.6% | no |
| `intermediate` | 1 884 138 | 3 073 038 | +1 188 900 | +63.1% | no |
| `complex` (TX_GAS_LIMIT_CAP = 16.8 M) | 10 018 653 | 16 384 620 | +6 365 967 | +63.5% | **yes** |
| `complex` (100 M cap) | 10 018 653 | 16 772 401 | +6 753 748 | +67.4% | yes† |

`complex` completes at the outer level but some inner subcalls run OOG due to the dominant
STORAGE_WRITE cost: 1 261 SSTOREs × 10 000 ≈ 12.6 M gas from writes alone. At TX_GAS_LIMIT_CAP
(16.8 M), the constrained forwarded gas causes major inner path divergence (+63.5%). At a
100 M cap, nearly the full code path runs (+67.4%), revealing the true cost increase.

† Minor residual path divergence at 100 M cap (compute gas 142 871 vs 147 695 at baseline,
a 4 824-gas difference); some deeply nested subcall still receives insufficient forwarded gas.

For a detailed breakdown including refund modeling, see `results/eip8038-pr11802.md`.

---

## EIP-8037 + EIP-8038 combined — hypothetical 3× SLOAD/SSTORE scenario (AMSTERDAM)

The following results use a **hypothetical** uniform 3× multiplier (warm 100→300, cold
2100→6300 for both SLOAD and SSTORE) to stress-test the combined repricing under the
AMSTERDAM spec. This is not derived from any actual EIP — it is a sensitivity scenario.
The merged PR #11802 values above are the authoritative reference.

| Label | baseline gas | eip8038_sstore gas | Δ% vs baseline | path_diverged |
|---|---:|---:|---:|:---:|
| `liquidation` | 781 399 | 1 265 399 | +61.94% | no |
| `simple` | 194 977 | 289 177 | +48.31% | no |
| `intermediate` | 1 884 138 | 2 996 938 | +59.06% | no |
| `complex` | 10 018 653 | 15 947 493 | +59.18% | no |

**`complex` at 3× AMSTERDAM**: the transaction fits within TX_GAS_LIMIT_CAP and completes
its full code path (path_diverged=no). This contrasts with the PRAGUE eip8038 (3×) result
where `complex` saturates the cap (see below). The AMSTERDAM SSTORE restructuring —
primarily the drop in `sstore_set_without_load_cost` (19 900 → 2 800, saving 17 100 per
first-write-to-zero SSTORE) — is the reason `complex` fits under AMSTERDAM but not PRAGUE.

---

## EIP-8038 — SLOAD repricing only (hypothetical 3×, PRAGUE spec)

For comparison: warm SLOAD 100 → 300; cold SLOAD 2 100 → 6 300; PRAGUE spec (SSTORE
costs unchanged). This isolates the SLOAD-only repricing effect.

| Label | baseline gas | eip8038 gas | Δ gas | Δ% | path_diverged |
|---|---:|---:|---:|---:|:---:|
| `liquidation` | 781 399 | 1 261 199 | +479 800 | +61.40% | no |
| `simple` | 194 977 | 289 177 | +94 200 | +48.31% | no |
| `intermediate` | 1 884 138 | 2 996 938 | +1 112 800 | +59.06% | no |
| `complex` | 10 018 653 | 16 776 237 | +6 757 584 | +67.45% | **yes** |

`complex` saturates TX_GAS_LIMIT_CAP under this PRAGUE schedule (16 776 237 vs 16 777 216
cap). The AMSTERDAM SSTORE restructuring (section above) resolves this.

---

## Cross-schedule summary (TX_GAS_LIMIT_CAP)

| Schedule | Liquidation Δ% | Simple Δ% | Intermediate Δ% | Complex Δ% | Complex path |
|---|---:|---:|---:|---:|:---:|
| `eip8037` | 0.00% | 0.00% | 0.00% | +1.61% | no |
| `eip8038_pr11802` (actual, AMSTERDAM) | +43.6% | +46.6% | +63.1% | +63.5% | **yes** |
| `eip8038` (hyp. 3×, PRAGUE) | +61.40% | +48.31% | +59.06% | +67.45% | **yes** |
| `eip8038_sstore` (hyp. 3×, AMSTERDAM) | +61.94% | +48.31% | +59.06% | +59.18% | no |

Key observations:

1. **EIP-8037 alone** has negligible impact: 0% for three transactions; +1.61% for
   the complex arb (which creates 2 new storage slots). SLOAD/SSTORE repricing dominates
   by a factor of ≥30×.

2. **Merged PR #11802 values** produce +44–63% increases. The dominant cost is
   STORAGE_WRITE (10 000 per SSTORE write), not cold access. This causes inner subcall
   OOG in `complex` (path_diverged=yes) without saturating the cap outright.

3. **AMSTERDAM vs PRAGUE for `complex`**: under the hypothetical 3× scenario, AMSTERDAM's
   SSTORE restructuring prevents TX_GAS_LIMIT_CAP saturation — the transaction completes
   its full code path under AMSTERDAM (path_diverged=no) but not under PRAGUE
   (path_diverged=yes, gas = 16 776 237 ≈ cap). This is a qualitative difference.

4. **AMSTERDAM vs PRAGUE for the other three**: the difference is ≤0.5 percentage points.
   For these transactions the spec choice has no material effect.

---

## Methodology notes

- All runs use **TX_GAS_LIMIT_CAP = 16 777 216** (EIP-7825, 2²⁴) as the uniform tx gas limit.
- **EIP-8037** `CPSB_GLAMSTERDAM = 1 530` is calibrated for a **150 M gas reference block**.
- **EIP-8038 PR #11802** constants are derived from empirical benchmarking at 100 Mgas/s
  and are **not tied to a specific block gas limit**.
- The "hypothetical 3×" schedules (`eip8038`, `eip8038_sstore`) are synthetic sensitivity
  scenarios, not based on any actual EIP.
- `path_diverged` is detected by comparing compute-opcode gas between the repriced
  and baseline runs; a difference indicates the tx took a different code path.
- SSTORE classification for EIP-8037: `complex` creates new slots confirmed by the
  `eip8037` − `baseline` delta of +161 640 (= 2 × 80 820 net per-slot cost).
  The other three fixtures show Δ = 0, confirming no new slot creation.

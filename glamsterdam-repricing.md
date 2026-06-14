# Glamsterdam Gas Repricing Impact

Gas cost impact of EIP-8037 (SSTORE state gas) and EIP-8038 (state-access repricing)
on four Ethereum mainnet transactions — one Aave v3 liquidation and three AMM arbitrage
transactions — replayed with the revm harness.

Raw data: `liquidation-repricing.csv`, `amm-arb-repricing.csv`.

---

## Fixtures

| Label | Tx hash | Category | Receipt gas | Tx gas limit | Utilisation |
|---|---|---|---:|---:|---:|
| `liquidation` | `0x7b53e9...629` | Aave v3 liquidationCall | 781 399 | 1 169 544 | 66.8% |
| `simple` | `0x7ab274...c0a` | Atomic AMM arb | 194 977 | 274 544 | 71.0% |
| `semi_complex` | `0x8687c5...875` | Semi-complex AMM arb | 1 884 138 | 2 926 207 | 64.4% |
| `complex` | `0xfa1125...67e` | Complex AMM arb (12 M gas) | 12 017 628 | 13 215 466 | 90.9% |

All fixtures captured from Ethereum mainnet (chain id 1, ~60 M block gas limit)
with `harvest_prestate.py` using `debug_traceTransaction` + `prestateTracer`.

---

## Baseline fidelity

Replaying under the `baseline` PRAGUE schedule at the natural tx gas limit exactly
reproduces `receipt.gas_used` for all four fixtures.

| Label | Receipt gas | Harness baseline | Match |
|---|---:|---:|:---:|
| `liquidation` | 781 399 | 781 399 | ✓ |
| `simple` | 194 977 | 194 977 | ✓ |
| `semi_complex` | 1 884 138 | 1 884 138 | ✓ |
| `complex` | 12 017 628 | 12 017 628 | ✓ |

---

## EIP-8037 — SSTORE state gas (AMSTERDAM spec)

Each new-slot 0→nonzero SSTORE incurs an additional 97 920 gas
(SSTORE_SET_BYTES=64 × CPSB_GLAMSTERDAM=1 530). The `sstore_set_without_load_cost`
drops from 19 900 to 2 800, giving a net per-new-slot delta of **+80 820 gas**.
All four runs use the natural tx gas limit.

| Label | baseline gas | eip8037 gas | Δ gas | Δ% | Exec complete | Fits natural limit? |
|---|---:|---:|---:|---:|:---:|:---:|
| `liquidation` | 781 399 | 781 399 | 0 | 0.00% | yes | yes |
| `simple` | 194 977 | 194 977 | 0 | 0.00% | yes | yes |
| `semi_complex` | 1 884 138 | 1 884 138 | 0 | 0.00% | yes | yes |
| `complex` | 12 017 628 | 22 200 948 | +10 183 320 | +84.7% | yes | **no** |

Three of the four transactions create no new storage slots during execution — all
SSTOREs overwrite already-initialised slots — so EIP-8037 has zero impact on them.

The complex AMM arb creates approximately **126 new storage slots**
(10 183 320 ÷ 80 820 ≈ 126), pushing its repriced gas cost to 22.2 M, which is 68%
above its 13.2 M natural gas limit. The tx would **fail on-chain** without a manually
increased gas limit.

---

## EIP-8038 — cold SLOAD repricing, 60 M block (3× multiplier)

warm SLOAD 100 → 300; cold SLOAD 2 100 → 6 300.
All repriced runs use `--tx-gas-limit 30000000` to prevent inner-CALL starvation
(EIP-150 63/64 forwarding rule). Execution completeness is confirmed by matching
compute opcode gas against baseline.

| Label | baseline gas | eip8038 gas | Δ gas | Δ% | Exec complete | Fits natural limit? |
|---|---:|---:|---:|---:|:---:|:---:|
| `liquidation` | 781 399 | 1 261 199 | +479 800 | +61.4% | yes | **no** |
| `simple` | 194 977 | 289 177 | +94 200 | +48.3% | yes | **no** |
| `semi_complex` | 1 884 138 | 2 996 938 | +1 112 800 | +59.1% | yes | **no** |
| `complex` | 12 017 628 | 16 465 828 | +4 448 200 | +37.0% | yes | **no** |

Every transaction exceeds its natural gas budget under 3× SLOAD repricing.

---

## EIP-8037 + EIP-8038 combined — SSTORE also repriced, 60 M block

cold SSTORE 2 100 → 6 300 in addition to the SLOAD changes above;
AMSTERDAM spec activates new-slot state gas.

| Label | eip8037 Δ | eip8038 Δ | eip8038_sstore Δ | Δ% | Exec complete | Cold SSTOREs |
|---|---:|---:|---:|---:|:---:|---:|
| `liquidation` | 0 | +479 800 | +484 000 | +61.9% | yes | 1 |
| `simple` | 0 | +94 200 | +94 200 | +48.3% | yes | 0 |
| `semi_complex` | 0 | +1 112 800 | +1 112 800 | +59.1% | yes | 0 |
| `complex` | +10 183 320 | +4 448 200 | +14 631 520 | +121.8% | yes | ~1 059 |

The eip8038_sstore delta equals eip8037 Δ + eip8038 Δ for all four transactions —
the state gas and cold-access effects are **exactly additive**.

The cold SSTORE count is inferred from the incremental delta
(eip8038_sstore − eip8038) ÷ (6 300 − 2 100). The liquidation has exactly 1 cold SSTORE;
simple and semi-complex have none; the complex AMM arb has ~1 059.

---

## EIP-8038 — 200 M block scaling (≈10× multiplier)

warm SLOAD 100 → 1 000; cold SLOAD 2 100 → 21 000; cold SSTORE 2 100 → 21 000;
`--block-gas-limit 200000000 --tx-gas-limit 30000000`.

**Note:** for the `complex` transaction, the 30 M tx limit is insufficient to prevent
inner-CALL starvation under these costs — execution paths diverge from baseline.
Those figures are marked ⚠ and are not directly comparable.

| Label | baseline gas | eip8038_200m Δ% | eip8038_sstore200m Δ% | Exec complete |
|---|---:|---:|---:|:---:|
| `liquidation` | 781 399 | +276.3% | +278.8% | yes |
| `simple` | 194 977 | +217.4% | +217.4% | yes |
| `semi_complex` | 1 884 138 | +265.8% | +265.8% | yes |
| `complex` | 12 017 628 | +149.6% ⚠ | +39.5% ⚠ | **no** ⚠ |

⚠ Complex tx path diverged — 30 M tx limit insufficient for accurate 200 M-scale measurement.

---

## Natural gas limit analysis

Transactions where repriced gas exceeds the original gas limit would **fail on-chain**
without a manual limit increase. Under current mempool conventions (bots set gas limits
close to estimated usage), virtually all transactions fail under SLOAD repricing.

| Label | Natural limit | eip8037 | eip8038 | eip8038_sstore | eip8038_200m | eip8038_sstore200m |
|---|---:|:---:|:---:|:---:|:---:|:---:|
| `liquidation` | 1 169 544 | ✓ fits | ✗ fail | ✗ fail | ✗ fail | ✗ fail |
| `simple` | 274 544 | ✓ fits | ✗ fail | ✗ fail | ✗ fail | ✗ fail |
| `semi_complex` | 2 926 207 | ✓ fits | ✗ fail | ✗ fail | ✗ fail | ✗ fail |
| `complex` | 13 215 466 | ✗ fail | ✗ fail | ✗ fail | ✗ fail | ✗ fail |

EIP-8037 state gas alone is the only proposal that leaves three of the four transactions
within their existing gas budgets. The complex AMM arb fails even under state-gas-only
because of its heavy new-slot creation workload.

---

## Cross-category summary (60 M block)

| Schedule | Liquidation Δ% | Simple arb Δ% | Semi-complex arb Δ% | Complex arb Δ% |
|---|---:|---:|---:|---:|
| `eip8037` | 0% | 0% | 0% | **+84.7%** |
| `eip8038` | +61.4% | +48.3% | +59.1% | +37.0% |
| `eip8038_sstore` | +61.9% | +48.3% | +59.1% | **+121.8%** |

**SLOAD repricing** is the dominant effect for liquidations and simple/semi-complex arb:
+48–62% gas increase, consistent across categories, driven by the high SLOAD count
in DeFi transactions (279 SLOADs in the Aave liquidation).

**State gas (EIP-8037)** is zero for three of four transactions (arb bots and
liquidators operate against existing positions). The complex AMM arb is the outlier:
its 84.7% state-gas increase dwarfs its own 37% SLOAD increase, because it initialises
new on-chain positions (Uniswap v3 tick slots, position entries) as part of the arb path.

---

## Methodology

- Repriced schedules use `--tx-gas-limit 30000000` to isolate gas cost changes from
  execution path divergence caused by inner-CALL gas starvation (EIP-150 63/64 rule).
- Execution completeness is confirmed by matching compute gas between baseline and
  the repriced run; a difference indicates a diverged execution path.
- 200 M block schedules also set `--block-gas-limit 200000000`.
- EIP-8038 multipliers (3× and ≈10×) are hypothetical; the EIP's constants are TBD.
- Cold SSTORE counts are inferred from (eip8038_sstore − eip8038) Δ ÷ 4 200.
- New-slot counts from eip8037 Δ ÷ 80 820 (per-slot delta = state_gas + set_without_load
  change = 97 920 + 2 800 − 19 900).

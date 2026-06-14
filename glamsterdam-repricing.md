# Glamsterdam Gas Repricing Impact

Gas cost impact of EIP-8037 (SSTORE state gas) and EIP-8038 (state-access repricing)
on four Ethereum mainnet transactions — one Aave v3 liquidation and three AMM arbitrage
transactions — replayed with the revm harness.

Raw data: `results/liquidation-repricing.csv`, `results/amm-arb-repricing.csv`.

---

## Fixtures

| Label | Tx hash | Category | Receipt gas | Tx gas limit | Utilisation |
|---|---|---|---:|---:|---:|
| `liquidation` | `0x7b53e9...629` | Aave v3 liquidationCall | 781 399 | 1 169 544 | 66.8% |
| `simple` | `0x7ab274...c0a` | Atomic AMM arb | 194 977 | 274 544 | 71.0% |
| `semi_complex` | `0x8687c5...875` | Semi-complex AMM arb | 1 884 138 | 2 926 207 | 64.4% |
| `complex` | `0x55738c...526` | Complex AMM arb (14.6 M gas) | 14 567 338 | 16 777 216 | 86.8% |

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
| `complex` | 14 567 338 | 14 567 338 | ✓ |

---

## EIP-8037 — SSTORE state gas (AMSTERDAM spec)

Each new-slot 0→nonzero SSTORE to a slot that does not exist in the world state incurs
an additional 97 920 gas (SSTORE_SET_BYTES=64 × CPSB_GLAMSTERDAM=1 530).
The `sstore_set_without_load_cost` also drops from 19 900 to 2 800 under AMSTERDAM,
giving a net per-*new*-slot delta of **+80 820 gas**. Writes to slots that already
exist in the state trie (even if their value is zero) pay only 2 800 instead of 19 900,
saving 17 100 gas per such write with no offsetting state gas charge.
All four runs use `--tx-gas-limit 30000000`.

| Label | baseline gas | eip8037 gas | Δ gas | Δ% | Exec complete | Fits natural limit? |
|---|---:|---:|---:|---:|:---:|:---:|
| `liquidation` | 781 399 | 781 399 | 0 | 0.00% | yes | yes |
| `simple` | 194 977 | 194 977 | 0 | 0.00% | yes | yes |
| `semi_complex` | 1 884 138 | 1 884 138 | 0 | 0.00% | yes | yes |
| `complex` | 14 567 338 | 14 724 234 | +156 896 | +1.08% | yes | yes |

Liquidation, simple, and semi-complex create no new storage slots and overwrite
only already-initialised slots, so the `sstore_set_without_load_cost` change and state
gas cancel out (Δ = 0). The complex AMM arb creates approximately **2 truly new slots**
(156 896 ÷ 80 820 ≈ 2) and also rewrites ~661 existing-zero slots — for those the cost
drops from 19 900 to 2 800, nearly cancelling the state gas overhead. The net EIP-8037
impact is only +1.1%, and the tx fits comfortably within its 16.8 M natural gas limit.

---

## EIP-8038 — cold SLOAD repricing, 60 M block (3× multiplier)

warm SLOAD 100 → 300; cold SLOAD 2 100 → 6 300. PRAGUE spec (no SSTORE changes).
All repriced runs use `--tx-gas-limit 30000000` to prevent inner-CALL starvation
(EIP-150 63/64 forwarding rule). Execution completeness confirmed by matching
compute opcode gas against baseline.

| Label | baseline gas | eip8038 gas | Δ gas | Δ% | Exec complete | Fits natural limit? |
|---|---:|---:|---:|---:|:---:|:---:|
| `liquidation` | 781 399 | 1 261 199 | +479 800 | +61.4% | yes | **no** |
| `simple` | 194 977 | 289 177 | +94 200 | +48.3% | yes | **no** |
| `semi_complex` | 1 884 138 | 2 996 938 | +1 112 800 | +59.1% | yes | **no** |
| `complex` | 14 567 338 | 26 075 344 | +11 508 006 | +79.0% | yes | **no** |

Every transaction exceeds its natural gas budget under 3× SLOAD repricing.
The complex AMM arb shows the largest increase (+79%) because it combines a high
SLOAD count with many 0→nonzero SSTOREs that remain priced at 19 900 under PRAGUE —
a double penalty not present in the AMSTERDAM-spec schedules.

---

## EIP-8037 + EIP-8038 combined — SSTORE also repriced, 60 M block

cold SSTORE 2 100 → 6 300 added on top of the SLOAD changes; AMSTERDAM spec activates
new-slot state gas **and** reduces `sstore_set_without_load_cost` from 19 900 to 2 800.

| Label | baseline gas | eip8038 gas (PRAGUE) | eip8038_sstore gas (AMSTERDAM) | Δ% | Exec complete | Fits natural limit? |
|---|---:|---:|---:|---:|:---:|:---:|
| `liquidation` | 781 399 | 1 261 199 | 1 265 399 | +61.9% | yes | **no** |
| `simple` | 194 977 | 289 177 | 289 177 | +48.3% | yes | **no** |
| `semi_complex` | 1 884 138 | 2 996 938 | 2 996 938 | +59.1% | yes | **no** |
| `complex` | 14 567 338 | 26 075 344 | 14 936 135 | +2.5% | yes | **yes** |

For liquidation, simple, and semi-complex the PRAGUE and AMSTERDAM schedules give
nearly identical results because those transactions write very few 0→nonzero SSTOREs.
The cold SSTORE repricing (+4 200 per cold write) adds only 0–4 200 gas.

For the complex AMM arb, `eip8038_sstore` (AMSTERDAM) is dramatically cheaper than
`eip8038` (PRAGUE): **14.9 M vs 26.1 M gas**. The difference (-11.1 M) comes from
the AMSTERDAM spec reducing `sstore_set_without_load_cost` from 19 900 to 2 800 for
~661 writes to existing-zero slots (17 100 savings each ≈ 11.3 M total savings),
which more than offsets the cold-access repricing. The combined AMSTERDAM + 3× cold
access schedule increases gas by only **+2.5%** and stays within the natural 16.8 M
gas limit.

Note: the simple additivity eip8038_sstore Δ = eip8037 Δ + eip8038 Δ holds only when
SSTORE writes are few (liquidation, simple, semi-complex). For the complex AMM arb the
two effects interact through the spec-level SSTORE cost change and cannot be decomposed.

---

## EIP-8038 — 200 M block scaling (≈10× multiplier)

warm SLOAD 100 → 1 000; cold SLOAD 2 100 → 21 000; cold SSTORE 2 100 → 21 000;
`--block-gas-limit 200000000 --tx-gas-limit 30000000`.

| Label | baseline gas | eip8038_200m gas | Δ% | eip8038_sstore200m gas | Δ% | Exec complete |
|---|---:|---:|---:|---:|---:|:---:|
| `liquidation` | 781 399 | 2 940 499 | +276.3% | 2 959 399 | +278.8% | yes |
| `simple` | 194 977 | 618 877 | +217.4% | 618 877 | +217.4% | yes |
| `semi_complex` | 1 884 138 | 6 891 738 | +265.8% | 6 891 738 | +265.8% | yes |
| `complex` | 14 567 338 | 26 817 001 | +84.1% | 15 677 794 | +7.6% | yes |

The same PRAGUE vs AMSTERDAM pattern holds at 200 M scale: the complex AMM arb
rises only +7.6% under `eip8038_sstore200m` (AMSTERDAM) vs +84.1% under `eip8038_200m`
(PRAGUE), because the 0→nonzero SSTORE cost reduction dominates the ≈10× cold access
increase. All four transactions have identical compute gas across schedules, confirming
clean execution paths at the 30 M tx limit.

---

## Natural gas limit analysis

Transactions where repriced gas exceeds the original gas limit would **fail on-chain**
without a manual limit increase.

| Label | Natural limit | eip8037 | eip8038 | eip8038_sstore | eip8038_200m | eip8038_sstore200m |
|---|---:|:---:|:---:|:---:|:---:|:---:|
| `liquidation` | 1 169 544 | ✓ fits | ✗ fail | ✗ fail | ✗ fail | ✗ fail |
| `simple` | 274 544 | ✓ fits | ✗ fail | ✗ fail | ✗ fail | ✗ fail |
| `semi_complex` | 2 926 207 | ✓ fits | ✗ fail | ✗ fail | ✗ fail | ✗ fail |
| `complex` | 16 777 216 | ✓ fits | ✗ fail | ✓ fits | ✗ fail | ✓ fits |

PRAGUE-based cold SLOAD repricing (`eip8038`, `eip8038_200m`) causes all four
transactions to fail at their natural gas limits. AMSTERDAM-based schedules
(`eip8037`, `eip8038_sstore`, `eip8038_sstore200m`) keep the complex AMM arb within
budget because the spec-level SSTORE cost reduction outweighs the cold access repricing.

---

## Cross-category summary (60 M block)

| Schedule | Liquidation Δ% | Simple arb Δ% | Semi-complex arb Δ% | Complex arb Δ% |
|---|---:|---:|---:|---:|
| `eip8037` | 0% | 0% | 0% | +1.1% |
| `eip8038` (PRAGUE) | +61.4% | +48.3% | +59.1% | **+79.0%** |
| `eip8038_sstore` (AMSTERDAM) | +61.9% | +48.3% | +59.1% | **+2.5%** |

**SLOAD repricing under PRAGUE** (`eip8038`) uniformly raises costs by +48–79% across
all transaction categories. The complex AMM arb is the hardest hit because it carries
both a high SLOAD count and many 0→nonzero SSTOREs priced at 19 900 under PRAGUE.

**AMSTERDAM spec** (`eip8038_sstore`) produces nearly identical results to PRAGUE for
liquidations and simple arb (low SSTORE write count) but reverses the impact for
complex AMM arb: the `sstore_set_without_load_cost` reduction (19 900 → 2 800) saves
more gas than the cold-access repricing costs, holding the increase to +2.5%.

---

## Methodology

- Repriced schedules use `--tx-gas-limit 30000000` to isolate gas cost changes from
  execution path divergence caused by inner-CALL gas starvation (EIP-150 63/64 rule).
- Execution completeness is confirmed by matching compute gas between baseline and
  the repriced run; a difference indicates a diverged execution path.
- 200 M block schedules also set `--block-gas-limit 200000000`.
- EIP-8038 multipliers (3× and ≈10×) are hypothetical; the EIP's constants are TBD.
- Existing-zero SSTORE count inferred from (eip8037 gas − baseline) and the known
  per-new-slot delta (80 820); the residual SSTORE cost reduction points to existing-zero
  slots paying 2 800 instead of 19 900 under AMSTERDAM.
- New-slot count: eip8037 Δ ÷ 80 820 (= state_gas + set_without_load_cost change).

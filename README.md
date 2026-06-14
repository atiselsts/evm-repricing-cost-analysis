# Gas Repricing Impact Harness

A [revm](https://github.com/bluealloy/revm)-based tool that replays a single
Ethereum transaction under a **parameterised gas schedule** to quantify how
proposed "Glamsterdam" gas changes affect DeFi transaction costs.

## Thesis

Two repricing proposals are studied:

- **EIP-7904** (compute repricing) — originally a Standards Track proposal to
  raise DIV, SDIV, MOD, KECCAK256 base and similar opcodes because benchmarks
  showed them under-priced. The EIP is now reclassified as Informational
  and is expected to be dropped from Glamsterdam. The gas values encoded here
  (`eip7904` schedule) come from the original Standards Track draft and represent
  a hypothetical scenario this harness was designed to stress-test.

- **EIP-8038** (state-access repricing) — updates gas costs for all
  state-access operations. The full EIP covers SLOAD, SSTORE, CALL account
  access, CREATE, EXTCODESIZE/EXTCODECOPY, SELFDESTRUCT, and access-list
  precomputation costs. **This PoC models only the SLOAD and cold SSTORE
  portions** using a hypothetical 3× multiplier; all other EIP-8038 changes
  are out of scope. See [Scope limitations](#scope-limitations) below.
  The EIP's new values are still TBD at the time of writing and may be
  calibrated to the prevailing block gas limit.

- **EIP-8037** (SSTORE state gas) — each new 0→nonzero storage slot creation
  incurs an additional 97 920 gas (SSTORE_SET_BYTES=64 × CPSB_GLAMSTERDAM=1 530),
  activated automatically by switching the EVM spec to AMSTERDAM.

**Core findings:**
- EIP-7904 compute repricing adds < 1% gas overhead — negligible for DeFi.
- EIP-8038 SLOAD repricing (3× scenario, 60 M block) adds +48–62% gas across
  all measured transactions; scaled to a 200 M block, +218–276%.
- EIP-8037 SSTORE state gas has zero impact on liquidations and simple/semi-complex
  AMM arb (no new slot creation). It adds +85% on a complex 12 M gas AMM arb
  that initialises new on-chain positions — and pushes that tx above its natural
  gas limit, causing it to fail without a manual limit increase.
- SLOAD repricing dominates for DeFi liquidations and simple arb; state gas
  dominates for complex AMM arb that creates new positions.

See [`glamsterdam-repricing.md`](glamsterdam-repricing.md) for full results tables
across all four measured transactions.

## Method

1. **Fixture**: a prestate JSON captured with `harvest_prestate.py`
   (uses `debug_traceTransaction` with `prestateTracer`) supplies every account
   and storage slot touched by the transaction. The harness loads this into an
   in-memory `CacheDB` — no archive node required at replay time.

2. **Gas injection**: two points, no opcode body rewrites:
   - *Static table* (`GasTable = [u16; 256]`) — patched via
     `EthInstructions::insert_gas` after EVM construction. Covers compute
     opcodes and the SLOAD warm base.
   - *Dynamic params* (`GasParams`) — patched via `override_gas` before
     construction. Covers the SLOAD cold surcharge and cold SSTORE total.

3. **Replay**: `evm.inspect_one_tx(tx)` with an `OpcodeCounter` inspector that
   records execution counts per opcode. Gas breakdown is derived post-run by
   multiplying counts by schedule costs.

4. **Output**: structured JSON to stdout:

   ```json
   {
     "gas_used": 781399,
     "schedule": "eip8038",
     "breakdown": { "compute": 9110, "other": 1252089 },
     "status": "success"
   }
   ```

## Pinned versions (Reth v2.3.0 family — do not bump)

| crate                     | version |
|---------------------------|---------|
| `revm`                    | 40.0.3  |
| `revm-interpreter`        | 37.0.3  |
| `revm-handler`            | 20.0.3  |
| `revm-context`            | 18.0.3  |
| `revm-context-interface`  | 19.0.3  |
| `revm-inspector`          | 21.0.3  |
| `revm-primitives`         | 24.0.1  |

## Gas schedules

EIP-7904 (compute) and EIP-8037/8038 (state access) are evaluated **independently**.

**S1/S2 schedules — PRAGUE spec (SLOAD only):**

| Schedule        | warm SLOAD | cold SLOAD | cold SSTORE | SpecId | calibration |
|-----------------|----------:|----------:|----------:|--------|-------------|
| `baseline`      |       100 |     2 100 |     2 100 | PRAGUE | mainnet     |
| `eip7904`       |       100 |     2 100 |     2 100 | PRAGUE | compute opcodes only |
| `eip8038`       |       300 |     6 300 |     2 100 | PRAGUE | 60 M (3× SLOAD only) |
| `eip8038_200m`  |     1 000 |    21 000 |     2 100 | PRAGUE | 200 M (≈10× SLOAD only) |

**S6 schedules — AMSTERDAM spec (EIP-8037 state gas + EIP-8038 SLOAD + SSTORE):**

| Schedule             | warm SLOAD | cold SLOAD | cold SSTORE | SpecId    | calibration |
|----------------------|----------:|----------:|----------:|-----------|-------------|
| `eip8037`            |       100 |     2 100 |     2 100 | AMSTERDAM | state gas only |
| `eip8038_sstore`     |       300 |     6 300 |     6 300 | AMSTERDAM | 60 M (3× SLOAD + SSTORE) |
| `eip8038_sstore_200m`|     1 000 |    21 000 |    21 000 | AMSTERDAM | 200 M (≈10× SLOAD + SSTORE) |

AMSTERDAM activates EIP-8037 state gas automatically: each 0→nonzero SSTORE to a
previously nonexistent slot costs an additional 97 920 gas
(SSTORE_SET_BYTES=64 × CPSB_GLAMSTERDAM=1 530). All EIP-8038 values remain
hypothetical (TBD in the draft).

## Scope limitations

This PoC intentionally models a subset of the proposed changes:

**Implemented:**
| Injection point | Mechanism |
|---|---|
| Warm SLOAD base | static GasTable patch (opcode 0x54) |
| Cold SLOAD surcharge | `GasId::cold_storage_additional_cost` override |
| Cold SSTORE total | `GasId::cold_storage_cost` override |
| SSTORE new-slot state gas (EIP-8037) | `SpecId::AMSTERDAM` — activates automatically |

**EIP-8038 — not yet implemented:**
| Operation | Current cost | Notes |
|---|---|---|
| Cold account access (`COLD_ACCOUNT_ACCESS`) — CALL, BALANCE, EXT* | 2 600 | S5 |
| SSTORE warm reset (`ACCOUNT_WRITE`) | 2 800 | S6 |
| `STORAGE_CLEAR_REFUND` | 4 800 | S6 |
| CREATE / CREATE2 (`CREATE_ACCESS`) | 7 000 | S6 |
| EXTCODESIZE / EXTCODECOPY extra warm-read | +100 | S5 |
| SELFDESTRUCT account-write to empty target | +6 700 | S6 |
| Access-list key / address pre-warm costs | 1 900 / 2 400 | S7 |

All EIP-8038 new values are still TBD in the draft EIP; final costs need to be
calibrated relative to the block gas limit.

**EIP-7904 compute values** are from the original Standards Track draft.
MUL, SMOD, ADDMOD, MULMOD, and EXP base values in the `eip7904` schedule currently
remain at baseline; they would need to be updated once canonical repricing values are
published.

## Future work

- **S5** — cold account access repricing (CALL/BALANCE/EXT*) and
  EXTCODESIZE/EXTCODECOPY extra read charge
- **S6 (remaining)** — SSTORE warm reset repricing, CREATE, SELFDESTRUCT,
  refund changes; multi-fixture survey to measure EIP-8037 impact at position open
- **S7** — intrinsic gas changes (EIP-7976 calldata floor, EIP-7981 access
  list costs)
- **S3/S4/S8** — Reth archive integration, per-category gas inspector,
  batch runner over historical liquidation transactions
- Finalise EIP-8038 values once the TBD constants are set in the draft, and
  update the `eip8038` schedule accordingly

## Usage

```bash
# Capture a fixture (requires an RPC endpoint with debug namespace)
python3 scripts/harvest_prestate.py \
    --rpc $RPC_URL \
    --tx 0x7b53e92... \
    --out fixtures/

# Replay under each schedule (natural tx gas limit)
cargo run --bin harness -- --fixture fixtures/0x7b53e92....json --schedule baseline
cargo run --bin harness -- --fixture fixtures/0x7b53e92....json --schedule eip7904
cargo run --bin harness -- --fixture fixtures/0x7b53e92....json --schedule eip8037

# High tx gas limit — identical execution paths across all schedules (no inner-CALL starvation)
cargo run --bin harness -- --fixture fixtures/0x7b53e92....json --schedule eip8038         --tx-gas-limit 30000000
cargo run --bin harness -- --fixture fixtures/0x7b53e92....json --schedule eip8038-200m    --block-gas-limit 200000000 --tx-gas-limit 30000000
cargo run --bin harness -- --fixture fixtures/0x7b53e92....json --schedule eip8038-sstore  --tx-gas-limit 30000000
cargo run --bin harness -- --fixture fixtures/0x7b53e92....json --schedule eip8038-sstore200m --block-gas-limit 200000000 --tx-gas-limit 30000000

# Run all acceptance tests
cargo test --test acceptance -- --nocapture

# After editing RepriceProbe.sol, rebuild bytecode and sync:
cd contracts && forge build && cd ..
python3 scripts/update_probe_bytecode.py
```

## Repository layout

```
Cargo.toml                          # workspace (resolver = "2")
glamsterdam-repricing.md            # full results tables for all measured transactions
notes                               # tx hashes used as fixtures (plain text)
crates/
  gas-schedule/                     # GasSchedule struct + presets (baseline, eip7904, eip8037/38)
  repricer-evm/                     # CacheDB builder, EVM runner, OpcodeCounter inspector
  harness/                          # CLI binary (--fixture / --schedule / --tx-gas-limit)
    tests/acceptance.rs             # 7 acceptance tests
fixtures/
  0x7b53e92....json                 # Aave v3 liquidationCall prestate (block 24 390 617)
  0x7ab274a....json                 # simple (atomic) AMM arb prestate
  0x8687c5e....json                 # semi-complex AMM arb prestate
  0xfa11258....json                 # complex AMM arb prestate (12 M gas)
contracts/
  src/RepriceProbe.sol              # Synthetic compute + SLOAD + SSTORE probe contract
  test/RepriceProbe.t.sol           # Foundry tests (loop termination, gas linearity, SSTORE)
  foundry.toml                      # optimizer off, via_ir false
results/
  liquidation-repricing.csv         # per-schedule gas data for the Aave v3 liquidation
  amm-arb-repricing.csv             # per-schedule gas data for all three AMM arb transactions
scripts/
  harvest_prestate.py               # captures prestate fixtures via RPC (debug_traceTransaction)
  update_probe_bytecode.py          # syncs compiled RepriceProbe bytecode into synthetic.rs
subtasks.md                         # detailed S1–S8 milestone descriptions
s2-addition.md                      # design notes for the S2 SLOAD repricing addition
s6-addition.md                      # design notes for the S6 SSTORE repricing addition
```

## Acceptance tests

| # | Test | Assertion |
|---|------|-----------|
| 1 | `test_baseline_fidelity` | `baseline` reproduces `receipt.gas_used` exactly (781 399) |
| 2 | `test_mechanism_correctness` | `eip7904` − `baseline` = hand-computed 470 gas for 10 compute iters |
| 3 | `test_real_tx_demonstration` | `eip7904` ≥ `baseline` on the liquidation fixture |
| 4 | `test_sload_mechanism` | `eip8038` − `baseline` = hand-computed 21 600 gas (5 cold + 3 warm) |
| 5 | `test_thesis_preview` | EIP-8038 SLOAD Δ >> EIP-7904 compute Δ on the Aave liquidation |
| 6 | `test_eip8037_sstore_new_slot` | `eip8037` − `baseline` = hand-computed 404 100 gas for 5 new slots (80 820 each) |
| 7 | `test_sstore_impact_liquidation` | `eip8037` gas ≥ `baseline` (Δ = 0: no new slots in liquidation) |

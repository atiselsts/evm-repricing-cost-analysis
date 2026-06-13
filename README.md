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
  precomputation costs. **This PoC models only the SLOAD portion** (warm base
  and cold surcharge) using a hypothetical 3× multiplier; all other EIP-8038
  changes are out of scope. See [Scope limitations](#scope-limitations) below.
  The EIP's new values are still TBD at the time of writing and may be
  calibrated to the prevailing block gas limit.

**Core finding:** on a real Aave v3 `liquidationCall` transaction, compute
repricing (EIP-7904 hypothetical values) adds < 1% gas overhead. State-read
repricing at the 60 M block-limit calibration (EIP-8038 3× scenario) adds
+61% when execution paths are held constant; at a 200 M block-limit
calibration (≈10× baseline) it adds +276%. For liquidation bots, SLOAD cost
dominates; compute cost is negligible.

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
     construction. Covers the SLOAD cold surcharge.

3. **Replay**: `evm.inspect_one_tx(tx)` with an `OpcodeCounter` inspector that
   records execution counts per opcode. Gas breakdown is derived post-run by
   multiplying counts by schedule costs.

4. **Output**: structured JSON to stdout:

   ```json
   {
     "gas_used": 781399,
     "schedule": "eip7904",
     "breakdown": { "compute": 15135, "other": 772289 },
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

EIP-7904 (compute) and EIP-8038 (state access) are evaluated **independently**.

| Schedule        | DIV | SDIV | MOD | KECCAK256 base | warm SLOAD | cold SLOAD | block gas limit calibration |
|-----------------|----:|-----:|----:|---------------:|-----------:|-----------:|----------------------------:|
| `baseline`      |   5 |    5 |   5 |             30 |        100 |      2 100 | — |
| `eip7904`       |  15 |   20 |  12 |             45 |        100 |      2 100 | n/a (compute only) |
| `eip8038`       |   5 |    5 |   5 |             30 |        300 |      6 300 | 60 M (3× baseline) |
| `eip8038_200m`  |   5 |    5 |   5 |             30 |      1 000 |     21 000 | 200 M (≈10× baseline) |

The `eip8038_200m` costs are derived by scaling the `eip8038` values by
200 M / 60 M ≈ 3.33× to reflect the proportionally higher state I/O pressure
at a larger block gas limit. All values remain hypothetical (EIP-8038 new
costs are TBD in the draft).

## Results — Aave v3 liquidationCall (block 24 390 617, tx gas limit 1 169 544)

### EIP-7904 compute repricing (block gas limit unchanged at 60 M)

| Schedule   | gas used | Δ vs baseline | Δ%     |
|------------|--------:|--------------:|-------:|
| `baseline` | 781 399 | —             | —      |
| `eip7904`  | 787 424 | +6 025        | +0.77% |

Baseline breakdown: **9 110 gas compute** / **772 289 gas other** (279 SLOADs).

The compute delta (+6 025) is smaller than the cost of a single cold SLOAD
(2 100). Compute repricing has negligible impact on DeFi liquidations.

### EIP-8038 SLOAD repricing

We used an artificial high gas limit (30 000 000) that eliminates inner-CALL starvation
even when SLOAD costs a lot more, giving identical execution paths across all schedules.
The `compute` breakdown field is exactly 9 110 gas for all three runs,
confirming that the same opcodes execute in the same sequence.

| Schedule       | block gas limit | tx gas limit | gas used  | Δ vs baseline | Δ%      |
|----------------|---------------:|-------------:|----------:|--------------:|--------:|
| `baseline`     | 60 M           | 30 M         | 781 399   | —             | —       |
| `eip8038`      | 60 M           | 30 M         | 1 261 199 | +479 800      | +61.4%  |
| `eip8038_200m` | 200 M          | 30 M         | 2 940 499 | +2 159 100    | +276.3% |

The delta ratio (2 159 100 / 479 800 ≈ 4.50×) matches the cost ratio exactly:
(21 000 − 2 100) / (6 300 − 2 100) = (1 000 − 100) / (300 − 100) = 4.5×,
confirming the gas injection is mechanically correct.

These numbers reflect **SLOAD repricing only**. Full EIP-8038 would add SSTORE,
CALL, and CREATE repricing on top (see [Scope limitations](#scope-limitations)).

## Scope limitations

This PoC (S1 + S2 + S2-addition) intentionally models a subset of the
proposed changes:

**EIP-8038 — implemented (SLOAD only):**
| Injection point | Implemented |
|---|---|
| Warm SLOAD base (`WARM_ACCESS` for storage) | yes — static GasTable patch |
| Cold SLOAD surcharge (`COLD_STORAGE_ACCESS`) | yes — GasParams patch |

**EIP-8038 — not yet implemented:**
| Operation | Current cost | Notes |
|---|---|---|
| Cold account access (`COLD_ACCOUNT_ACCESS`) — CALL, BALANCE, EXT* | 2 600 | S5 |
| SSTORE access + write (`COLD_STORAGE_ACCESS` + `ACCOUNT_WRITE`) | 2 100 + 2 800 | S6 |
| SSTORE state-creation | 7 000 (`STORAGE_CREATE`) | S6 |
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
- **S6** — SSTORE repricing (access + write + state-creation), CREATE,
  SELFDESTRUCT, refund changes
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
# High tx gas limit — identical execution paths across all schedules (no inner-CALL starvation)
cargo run --bin harness -- --fixture fixtures/0x7b53e92....json --schedule eip8038       --tx-gas-limit 30000000
cargo run --bin harness -- --fixture fixtures/0x7b53e92....json --schedule eip8038-200m  --block-gas-limit 200000000 --tx-gas-limit 30000000

# Run all acceptance tests
cargo test --test acceptance -- --nocapture

# After editing RepriceProbe.sol, rebuild bytecode and sync:
cd contracts && forge build && cd ..
python3 scripts/update_probe_bytecode.py
```

## Repository layout

```
Cargo.toml                          # workspace (resolver = "2")
crates/
  gas-schedule/                     # GasSchedule struct + presets
  repricer-evm/                     # CacheDB builder, EVM runner, OpcodeCounter
  harness/                          # CLI binary
    tests/acceptance.rs             # 5 acceptance tests
fixtures/
  0x7b53e92....json                 # Aave v3 liquidationCall prestate
contracts/
  src/RepriceProbe.sol              # Synthetic compute + SLOAD probe
  test/RepriceProbe.t.sol           # Foundry tests (loop termination, gas linearity)
  foundry.toml                      # optimizer off, via_ir false
scripts/
  harvest_prestate.py               # Captures prestate fixtures via RPC
  update_probe_bytecode.py          # Syncs compiled bytecode into synthetic.rs
```

## Acceptance tests

| # | Test | Assertion |
|---|------|-----------|
| 1 | `test_baseline_fidelity` | `baseline` reproduces `receipt.gas_used` exactly (781 399) |
| 2 | `test_mechanism_correctness` | `eip7904` − `baseline` = hand-computed 470 gas for 10 compute iters |
| 3 | `test_real_tx_demonstration` | `eip7904` ≥ `baseline` on the liquidation fixture |
| 4 | `test_sload_mechanism` | `eip8038` − `baseline` = hand-computed 21 600 gas (5 cold + 3 warm) |
| 5 | `test_thesis_preview` | EIP-8038 SLOAD Δ (353 647) >> EIP-7904 compute Δ (6 025) on the DeFi tx (natural tx limit) |

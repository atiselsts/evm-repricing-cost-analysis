#!/usr/bin/env python3
"""Harvest a self-contained replay fixture for one historical Ethereum tx.

Makes a few JSON-RPC calls to an archive endpoint (PROVIDER_URL) to capture
everything the Rust harness needs to replay the tx fully offline:
  - the transaction envelope            -> TxEnv
  - the block header                    -> BlockEnv
  - the receipt gas_used                -> baseline validation anchor
  - prestateTracer output (pre mode)    -> accounts/slots touched, pre-tx values

Writes fixtures/<txhash>.json. No local archive node required: the provider's
archive serves the trace. prestateTracer on a specific tx hash already reflects
the effects of preceding txs in the same block, so no in-block replay is needed.

Usage:
    PROVIDER_URL=https://eth-mainnet.g.alchemy.com/v2/KEY \
        python3 harvest_prestate.py 0x<txhash> [out_dir]
"""
import json
import os
import sys
import urllib.request

PROVIDER_URL = os.environ.get("PROVIDER_URL")


def rpc(method, params):
    payload = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    ).encode()
    req = urllib.request.Request(
        PROVIDER_URL, data=payload, headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=180) as r:
        out = json.loads(r.read())
    if "error" in out:
        raise RuntimeError(f"{method} failed: {out['error']}")
    return out["result"]


def main():
    if not PROVIDER_URL:
        sys.exit("error: set PROVIDER_URL to your archive RPC endpoint")
    if len(sys.argv) < 2:
        sys.exit("usage: harvest_prestate.py <tx_hash> [out_dir]")
    txh = sys.argv[1]
    out_dir = sys.argv[2] if len(sys.argv) > 2 else "fixtures"
    os.makedirs(out_dir, exist_ok=True)

    tx = rpc("eth_getTransactionByHash", [txh])
    if tx is None:
        sys.exit("error: tx not found")
    receipt = rpc("eth_getTransactionReceipt", [txh])
    block = rpc("eth_getBlockByNumber", [tx["blockNumber"], False])
    # prestateTracer, default ("pre") mode: every account/slot the tx reads or
    # writes, with values as of immediately before the tx executed.
    prestate = rpc("debug_traceTransaction", [txh, {"tracer": "prestateTracer"}])

    fixture = {
        "tx_hash": txh,
        "chain_id": tx.get("chainId"),
        "transaction": tx,  # raw envelope: from,to,value,input,gas,type,
        #                      maxFeePerGas, accessList, blobVersionedHashes,
        #                      authorizationList (7702), etc.
        "block_header": {
            "number": block["number"],
            "timestamp": block["timestamp"],
            "coinbase": block["miner"],
            "gas_limit": block["gasLimit"],
            "base_fee_per_gas": block.get("baseFeePerGas"),
            "prev_randao": block.get("mixHash"),
            "excess_blob_gas": block.get("excessBlobGas"),
            "blob_gas_used": block.get("blobGasUsed"),
            "parent_beacon_block_root": block.get("parentBeaconBlockRoot"),
        },
        "receipt": {
            "gas_used": receipt["gasUsed"],
            "status": receipt.get("status"),
            "tx_index": receipt.get("transactionIndex"),
        },
        "prestate": prestate,
    }

    path = os.path.join(out_dir, f"{txh}.json")
    with open(path, "w") as f:
        json.dump(fixture, f, indent=2)

    n_accts = len(prestate)
    n_slots = sum(len(a.get("storage", {})) for a in prestate.values())
    print(f"wrote {path}")
    print(
        f"  block {int(block['number'], 16)}  "
        f"tx_index {int(receipt.get('transactionIndex', '0x0'), 16)}"
    )
    print(f"  prestate: {n_accts} accounts, {n_slots} storage slots")
    print(
        f"  receipt gas_used: {int(receipt['gasUsed'], 16)}  "
        f"(<- baseline validation target)"
    )


if __name__ == "__main__":
    main()

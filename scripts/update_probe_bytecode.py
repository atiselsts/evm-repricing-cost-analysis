#!/usr/bin/env python3
"""Read the Foundry artifact for RepriceProbe and patch the hex constant in synthetic.rs."""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
ARTIFACT = ROOT / "contracts/out/RepriceProbe.sol/RepriceProbe.json"
TARGET   = ROOT / "crates/repricer-evm/src/synthetic.rs"

artifact = json.loads(ARTIFACT.read_text())
bytecode_hex = artifact["deployedBytecode"]["object"].removeprefix("0x")

if not bytecode_hex:
    sys.exit("ERROR: deployedBytecode.object is empty — run `forge build` first")

source = TARGET.read_text()
new_source, n = re.subn(
    r'(const PROBE_DEPLOYED_BYTECODE_HEX: &str =\s*\n\s*")([0-9a-fA-F]*)(")',
    lambda m: m.group(1) + bytecode_hex + m.group(3),
    source,
)

if n == 0:
    sys.exit("ERROR: PROBE_DEPLOYED_BYTECODE_HEX constant not found in synthetic.rs")

TARGET.write_text(new_source)
print(f"Updated {TARGET.relative_to(ROOT)} ({len(bytecode_hex)//2} bytes)")

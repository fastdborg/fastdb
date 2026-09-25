#!/usr/bin/env python3
"""Regenerate native release inventories and checksum-verified source notices."""
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
for short, package, output in [
    ("node", "fastdb-node", "bindings/node/THIRD_PARTY_CRATE_NOTICES.md"),
    ("python", "fastdb-python", "bindings/python/THIRD_PARTY_CRATE_NOTICES.md"),
    ("c", "fastdb-c", "bindings/c/THIRD_PARTY_CRATE_NOTICES.md"),
    ("cli", "fastdb-cli", "docs/cli-crate-notices-linux-x64.md"),
]:
    inventory = f"fastdb/docs/{short}-dependencies-linux-x64.json"
    audit = f"fastdb/docs/{short}-crate-notices-linux-x64.json"
    env = dict(os.environ, FASTDB_INVENTORY_PACKAGE=package)
    for command in [
        ["node", "fastdb/scripts/inventory-node-dependencies.cjs", "x86_64-unknown-linux-gnu", inventory],
        ["python3", "fastdb/scripts/audit-crate-notices.py", inventory, audit],
        ["python3", "fastdb/scripts/bundle-crate-notices.py", inventory, audit, "fastdb/" + output,
         "--supplements", "fastdb/docs/notice-source-supplements.json"],
    ]:
        subprocess.run(command, cwd=ROOT, env=env, check=True)

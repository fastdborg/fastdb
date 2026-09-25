#!/usr/bin/env python3
"""Reject mixed FastDB identities before release packaging; leave Turso untouched."""
import json
from pathlib import Path
import re
import tomllib
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
version = json.loads((ROOT / "fastdb/release.json").read_text())["version"]
if not re.fullmatch(r"[1-9][0-9]*\.[0-9]+\.[0-9]+", version):
    raise ValueError("Release version must be a stable semantic version")
crates = ["frontend", "parser", "cli", "tests", "bindings/node", "bindings/python", "bindings/protocol", "bindings/c"]
lock = tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
for crate in crates:
    manifest = tomllib.loads((ROOT / "fastdb" / crate / "Cargo.toml").read_text())["package"]
    assert manifest["version"] == version, crate
    entries = [p for p in lock if p["name"] == manifest["name"] and "source" not in p]
    assert len(entries) == 1 and entries[0]["version"] == version, crate + " lock"
assert json.loads((ROOT / "fastdb/bindings/php/composer.json").read_text())["version"] == version
node = json.loads((ROOT / "fastdb/bindings/node/package.json").read_text())
assert node["version"] == version
npm = json.loads((ROOT / "fastdb/bindings/node/package-lock.json").read_text())
assert npm["version"] == npm["packages"][""]["version"] == version
python = tomllib.loads((ROOT / "fastdb/bindings/python/pyproject.toml").read_text())
assert python["project"]["version"] == version
assert f'__version__ = "{version}"' in (ROOT / "fastdb/bindings/python/python/fastdb/__init__.py").read_text()
assert ET.parse(ROOT / "fastdb/bindings/csharp/FastDB/FastDB.csproj").findtext("PropertyGroup/Version") == version
module = (ROOT / "fastdb/bindings/go/go.mod").read_text().splitlines()[0]
assert module == f"module github.com/fastdborg/fastdb/fastdb/bindings/go/v{version.split('.')[0]}"
assert '"fastdb/bindings/browser"' not in (ROOT / "Cargo.toml").read_text()
print(f"All eight FastDB crates and native client package identities match {version}")

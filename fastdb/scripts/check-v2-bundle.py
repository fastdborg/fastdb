#!/usr/bin/env python3
"""Verify exact local V2 bundle artifacts; write evidence outside the bundle."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
from shipping_policy import check_manifest, check_profile, check_artifact

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("bundle", type=Path)
parser.add_argument("evidence", type=Path)
parser.add_argument("--node", action="append", required=True, help="Node executable; repeat for each qualified runtime")
parser.add_argument("--python", action="append", required=True, help="Python executable; repeat for each qualified runtime")
parser.add_argument("--uv", default="uv")
parser.add_argument("--v1-package", type=Path, required=True)
parser.add_argument("--v2-package", type=Path, required=True, help="Immutable published 2.0.0 Node package")
args = parser.parse_args()
bundle = args.bundle.resolve()
evidence = args.evidence.resolve()
if evidence.is_relative_to(bundle):
    raise SystemExit("Evidence must be outside the checksummed bundle")
evidence.mkdir(parents=True, exist_ok=False)


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


checksums = {}
for line in (bundle / "SHA256SUMS").read_text().splitlines():
    checksum, name = line.split("  ", 1)
    path = (bundle / name).resolve()
    if not path.is_relative_to(bundle) or name in checksums or sha(path) != checksum:
        raise ValueError(f"Invalid or changed bundle file: {name}")
    checksums[name] = checksum
actual = {p.relative_to(bundle).as_posix() for p in bundle.rglob("*") if p.is_file() and p != bundle / "SHA256SUMS"}
assert actual == checksums.keys(), "Unlisted or missing bundle files"
manifest = json.loads((bundle / "manifest.json").read_text())
check_manifest(manifest)
receipts = {}
for line in (bundle / "evidence/cargo-build.jsonl").read_text().splitlines():
    receipt = json.loads(line)
    if receipt.get("reason") == "compiler-artifact" and receipt["target"]["name"] in manifest["cargoArtifacts"]:
        check_artifact(receipt["profile"])
        receipts[receipt["target"]["name"]] = receipt["profile"]
assert receipts == {name: value["profile"] for name, value in manifest["cargoArtifacts"].items()}, "Manifest does not match Cargo build receipts"
with tarfile.open(bundle / "fastdb-source.tar.gz") as source:
    source_cargo = source.extractfile("fastdb-source/Cargo.toml").read()
    check_profile(source_cargo.decode())
    assert hashlib.sha256(source_cargo).hexdigest() == manifest["sourceCargoTomlSha256"]
    assert hashlib.sha256(source.extractfile("fastdb-source/.cargo/config.toml").read()).hexdigest() == manifest["sourceCargoConfigSha256"]
version = manifest["version"]
release = json.loads((ROOT / "fastdb/release.json").read_text())
assert version == release["version"]
node_versions = [subprocess.check_output([node, "-p", "process.versions.node"], text=True).strip() for node in args.node]
python_versions = [subprocess.check_output([python, "-c", "import platform;print(platform.python_version())"], text=True).strip() for python in args.python]
assert {int(v.split('.')[0]) for v in node_versions} == set(release["requiredNodeMajors"]), "Incomplete Node runtime matrix"
assert {'.'.join(v.split('.')[:2]) for v in python_versions} == set(release["requiredPythonMinors"]), "Incomplete Python runtime matrix"
assert sha(bundle / "fastdb-source.tar.gz") == manifest["sourceArchiveSha256"]
v1 = args.v1_package.resolve()
assert json.loads((v1 / "package.json").read_text())["version"] == "1.0.0"
v2 = args.v2_package.resolve()
assert json.loads((v2 / "package.json").read_text())["version"] == "2.0.0"
results = []


def run(name, command, cwd=ROOT, env=None, input=None):
    with (evidence / f"{name}.log").open("w") as log:
        subprocess.run(command, cwd=cwd, env=env, input=input, text=True,
                       stdout=log, stderr=subprocess.STDOUT, check=True)
    results.append(name)
    print(name + " passed", flush=True)


base_env = dict(os.environ)
base_env["LD_LIBRARY_PATH"] = str(bundle / "lib") + ":" + base_env.get("LD_LIBRARY_PATH", "")
base_env["FASTDB_LIBRARY"] = str(bundle / "lib/libfastdb_c.so")
base_env["CGO_ENABLED"] = "1"
base_env["CGO_LDFLAGS"] = "-L" + str(bundle / "lib")
base_env.pop("FASTDB_FIXTURE", None)
run("c-abi", ["python3", "fastdb/scripts/check-c-abi.py", str(bundle / "lib/libfastdb_c.so")], env=base_env)
run("cli", [str(bundle / "bin/fastdb-cli"), "--script"], input="SELECT geo::cell(geo::point(100,13),7);\n")
cli = json.loads((evidence / "cli.log").read_text())
assert cli["rows"] == [[{"type": "String", "value": "87658b314ffffff"}]]
run("sqlite-adoption", ["python3", "fastdb/scripts/check-sqlite-adoption.py",
                        str(bundle / "bin/fastdb-cli"), str(evidence / "sqlite-adoption.json")], env=base_env)
sqlite_adoption = json.loads((evidence / "sqlite-adoption.json").read_text())
assert sqlite_adoption["cliSha256"] == checksums["bin/fastdb-cli"]
for name, file in [("cli-elf", bundle / "bin/fastdb-cli"), ("c-elf", bundle / "lib/libfastdb_c.so")]:
    run(name, ["python3", "fastdb/scripts/inspect-node-elf.py", str(file), str(evidence / f"{name}.json")])

for index, node in enumerate(args.node):
    env = dict(base_env, FASTDB_PACKAGE_TARBALL=str(bundle / "packages" / f"fastdb-node-{version}.tgz"))
    # pnpm's child Node process must use this runtime too.
    env["PATH"] = str(Path(node).resolve().parent) + ":" + env.get("PATH", "")
    run(f"node-{index}", [node, "fastdb/scripts/check-node-package.cjs"], env=env)
wheel, = (bundle / "packages").glob("*.whl")
for index, python in enumerate(args.python):
    run(f"python-{index}", ["python3", "fastdb/scripts/check-python-wheel.py", str(wheel), "--python", python, "--uv", args.uv])

with tempfile.TemporaryDirectory(prefix="fastdb-v2-bundle-") as temp:
    temporary = Path(temp)
    for language in ["php", "go", "swift"]:
        with tarfile.open(bundle / "packages" / f"fastdb-{language}-{version}.tar.gz") as tar:
            tar.extractall(temporary, filter="data")
    php = temporary / f"fastdb-php-{version}"
    run("php-package", ["php", "-d", "ffi.enable=1", "tests/smoke.php"], cwd=php, env=base_env)
    go = temporary / f"fastdb-go-{version}"
    run("go-package", ["go", "test", "-race", "-v", "./..."], cwd=go, env=base_env)
    swift = temporary / f"fastdb-swift-{version}"
    run("swift-package", ["swift", "test", "-j", "2", "-Xlinker", "-L" + str(bundle / "lib"),
                          "-Xlinker", "-rpath", "-Xlinker", str(bundle / "lib")], cwd=swift, env=base_env)
    consumer = temporary / "node-consumer"
    consumer.mkdir()
    (consumer / "package.json").write_text('{"name":"v2-upgrade-check","private":true}')
    run("node-upgrade-install", ["pnpm", "add", "--offline", "--ignore-scripts", str(bundle / "packages" / f"fastdb-node-{version}.tgz")], cwd=consumer)
    run("v1-upgrade-restore", [args.node[0], "fastdb/scripts/check-v2-upgrade.cjs", str(v1),
                              str(consumer / "node_modules/@fastdb/node"), str(evidence / "upgrade-fixture")])
    installed_node = consumer / "node_modules/@fastdb/node"
    for index, node in enumerate(args.node):
        env = dict(base_env, FASTDB_OWNERSHIP_PACKAGE=str(installed_node))
        run(f"node-ownership-{index}", [node, "--test", "fastdb/bindings/node/ownership.test.cjs"], env=env)
        run(f"v2-upgrade-restore-{index}", [node, "fastdb/scripts/check-v21-upgrade.cjs", str(v2),
                                          str(installed_node), str(evidence / f"v2-upgrade-fixture-{index}")])

    # Test the shipped .nupkg, never a previously cached same-version package.
    dotnet = temporary / "dotnet"
    dotnet.mkdir()
    (dotnet / "Tests.csproj").write_text(f'''<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup>
<OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><Nullable>enable</Nullable>
</PropertyGroup><ItemGroup><PackageReference Include="FastDB.Embedded" Version="{version}"/></ItemGroup></Project>''')
    (dotnet / "Program.cs").write_bytes((ROOT / "fastdb/bindings/csharp/Tests/Program.cs").read_bytes())
    (dotnet / "NuGet.Config").write_text(f'<configuration><packageSources><clear/><add key="bundle" value="{bundle / "packages"}"/></packageSources></configuration>')
    env = dict(base_env, NUGET_PACKAGES=str(temporary / "nuget-cache"), DOTNET_CLI_TELEMETRY_OPTOUT="1")
    run("nuget-package", ["dotnet", "run", "--", str(ROOT / "fastdb/bindings/fixtures/native-client.json")], cwd=dotnet, env=env)

report = {"version": version, "buildProfile": manifest["buildProfile"], "rustProfilePolicy": manifest["rustProfilePolicy"], "bundleManifestSha256": sha(bundle / "manifest.json"),
          "bundleChecksumsSha256": sha(bundle / "SHA256SUMS"), "passed": results, "nodeVersions": node_versions, "pythonVersions": python_versions,
          "sqliteAdoption": sqlite_adoption,
          "scope": "Exact Linux native artifacts on the recorded host; source/CI/publication remain separate gates"}
(evidence / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
print(evidence)

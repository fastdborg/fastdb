#!/usr/bin/env python3
"""Build a retained Linux x64 evaluation bundle from a clean pinned checkout."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
out = Path(sys.argv[1]).resolve()
def run(args, **kwargs):
    return subprocess.check_output(args, cwd=root, text=True, **kwargs)
if platform.system() != 'Linux' or platform.machine() != 'x86_64':
    raise SystemExit('This preview builder supports Linux x64 only')
if run(['git', 'status', '--porcelain']).strip():
    raise SystemExit('Commit tracked changes before building a candidate')
source = run(['git', 'rev-parse', 'HEAD']).strip()
out.mkdir(parents=True, exist_ok=False)
(out/'evidence').mkdir()
run(['cargo', 'build', '--locked', '-p', 'fastdb-cli', '-p', 'fastdb-node'])
shutil.copy2(root/'target/debug/fastdb-cli', out/'fastdb-cli')
shutil.copy2(root/'target/debug/libfastdb_node.so', root/'fastdb/bindings/node/fastdb.node')
packed = json.loads(subprocess.check_output(
    ['npm', 'pack', '--offline', '--ignore-scripts', '--json', '--pack-destination', str(out)],
    cwd=root/'fastdb/bindings/node', text=True))[0]
(out/'tracker.cjs').write_text((root/'fastdb/examples/node-task-tracker/app.cjs').read_text().replace("require('../../bindings/node/index.cjs')", "require('@fastdb/node')"))

run(['git', 'archive', '--format=tar.gz', '--prefix=fastdb-source/', '-o', str(out/'fastdb-source.tar.gz'), source])
for src, dst in [('fastdb/docs/preview-quickstart.md','README.md'), ('fastdb/docs/preview-release.md','LIMITATIONS.md'), ('fastdb/UPSTREAM.md','UPSTREAM.md'), ('fastdb/bindings/node/LICENSE.md','LICENSE.md'), ('fastdb/bindings/node/THIRD_PARTY_NOTICES.md','THIRD_PARTY_NOTICES.md'), ('fastdb/bindings/node/THIRD_PARTY_CRATE_NOTICES.md','NODE_CRATE_NOTICES.md')]:
    shutil.copy2(root/src, out/dst)
for package in ['fastdb-cli', 'fastdb-node']:
    inventory = out/'evidence'/f'{package}-dependencies.json'
    audit = out/'evidence'/f'{package}-notice-audit.json'
    env = dict(os.environ, FASTDB_INVENTORY_PACKAGE=package)
    run(['node','fastdb/scripts/inventory-node-dependencies.cjs','x86_64-unknown-linux-gnu',str(inventory)], env=env)
    run(['python3','fastdb/scripts/audit-crate-notices.py',str(inventory),str(audit)])
    run(['python3','fastdb/scripts/bundle-crate-notices.py',str(inventory),str(audit),str(out/f'{package}-CRATE_NOTICES.md')])
manifest = {'preview': '0.1.0-linux-x64', 'sourceCommit': source, 'buildProfile': 'debug (evaluation, not performance-qualified)', 'platform': platform.platform(), 'rust': run(['rustc','-vV']), 'node': run(['node','--version']).strip(), 'nodePackage': packed['filename'], 'publication': 'local candidate only'}
(out/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
files = sorted(p for p in out.rglob('*') if p.is_file())
(out/'SHA256SUMS').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(out))+'\n' for p in files))
print(out)

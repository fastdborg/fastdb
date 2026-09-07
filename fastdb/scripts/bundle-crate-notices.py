#!/usr/bin/env python3
"""Bundle verified crate notice candidates; does not claim a complete audit."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('inventory', type=Path)
    parser.add_argument('audit', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    inventory_bytes = args.inventory.read_bytes()
    inventory = json.loads(inventory_bytes)
    audit = json.loads(args.audit.read_bytes())
    lock = (ROOT / 'Cargo.lock').read_bytes()
    if inventory['lockfileSha256'] != digest(lock) or audit['inventorySha256'] != digest(inventory_bytes):
        raise ValueError('Stale inventory or notice audit')
    identities = lambda packages: [(p['name'], p['version']) for p in packages]
    if identities(inventory['packages']) != identities(audit['packages']):
        raise ValueError('Audit package identities differ from inventory')
    locked = tomllib.loads(lock.decode())['package']
    cache = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')) / 'registry/cache'
    texts = {}
    entries = []
    unresolved = []
    for package in audit['packages']:
        name, version = package['name'], package['version']
        label = f'{name} {version}'
        files = package.get('noticeCandidates', [])
        if not files:
            unresolved.append(label)
            continue
        matches = [p for p in locked if p['name'] == name and p['version'] == version]
        if len(matches) != 1 or matches[0].get('checksum') != package.get('archiveSha256'):
            raise ValueError(f'Archive is not pinned by lockfile: {label}')
        archives = sorted(cache.glob(f'*/{name}-{version}.crate'))
        if len(archives) != 1:
            raise ValueError(f'Missing or ambiguous cached archive: {label}')
        with archives[0].open('rb') as stream:
            if hashlib.file_digest(stream, 'sha256').hexdigest() != package['archiveSha256']:
                raise ValueError(f'Archive checksum mismatch: {label}')
        entries.append(f'\n### {label}\n')
        with tarfile.open(archives[0], 'r:gz') as crate:
            for item in files:
                member = crate.getmember(f'{name}-{version}/' + item['path'])
                if not member.isfile() or member.size != item['bytes'] or member.size > 4 * 1024 * 1024:
                    raise ValueError(f'Invalid notice candidate: {label}/{item["path"]}')
                with crate.extractfile(member) as stream:
                    data = stream.read(4 * 1024 * 1024 + 1)
                if digest(data) != item['sha256']:
                    raise ValueError(f'Notice checksum mismatch: {label}/{item["path"]}')
                key = item['sha256']
                texts[key] = data.decode('utf8')
                entries.append(f'- `{item["path"]}`: [source text {key}](#text-{key})\n')
    header = (
        '# Crate source notice texts\n\n'
        'Generated from the pinned Cargo dependency inventory and checksummed source archives. '
        'Includes normal and build dependencies; inclusion is not proof of linked code. '
        'Identical source files share a text section. SHA-256 identifiers refer to original file bytes. '
        'Formatting removes trailing whitespace only.\n\n'
        'This is a partial notice collection. It supplements LICENSE.md and THIRD_PARTY_NOTICES.md. '
        'Inline attributions, unconventional filenames, workspace sources and other bundled components '
        'require further review before a complete distribution-notice claim.\n\n'
        f'Target: `{inventory["target"]}`. Cargo.lock SHA-256: `{inventory["lockfileSha256"]}`.\n\n'
        '## Packages without collected filename candidates\n\n' + ', '.join(unresolved) + '\n\n'
        '## Source file index\n'
    )
    sections = []
    for key, text in sorted(texts.items()):
        sections.append(f'\n<a id="text-{key}"></a>\n\n## Source text {key}\n\n'
                        + '\n'.join(line.rstrip() for line in text.splitlines()) + '\n')
    result = header + ''.join(entries) + ''.join(sections)
    if args.check:
        if args.output.read_text() != result:
            raise ValueError('Crate notice bundle is stale; regenerate it')
    else:
        args.output.write_text(result)
    print(f'Verified {len(texts)} distinct notice texts; {len(unresolved)} packages require separate notice review')


if __name__ == '__main__':
    main()

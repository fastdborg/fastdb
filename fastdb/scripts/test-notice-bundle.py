#!/usr/bin/env python3
"""Synthetic offline checks for notice-bundle integrity and failure atomicity."""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import types
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name('bundle-crate-notices.py')


class NoticeBundleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.module = types.ModuleType('notice_bundle_test_module')
        self.module.__file__ = str(SCRIPT)
        exec(compile(SCRIPT.read_text(), str(SCRIPT), 'exec'), self.module.__dict__)
        self.module.ROOT = self.root
        self.cache = self.root / 'cargo/registry/cache/test-registry'
        self.cache.mkdir(parents=True)
        self.archive = self.cache / 'fixture-1.0.0.crate'
        self.text = b'Fixture copyright and notice text.\n'
        files = {'LICENSE': self.text, 'nested/LICENSE.txt': self.text}
        with tarfile.open(self.archive, 'w:gz') as archive:
            for name, data in files.items():
                member = tarfile.TarInfo('fixture-1.0.0/' + name)
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
        checksum = self.hash(self.archive.read_bytes())
        lock = f'[[package]]\nname = "fixture"\nversion = "1.0.0"\nsource = "registry+fixture"\nchecksum = "{checksum}"\n'
        (self.root / 'Cargo.lock').write_text(lock)
        self.inventory = self.root / 'inventory.json'
        self.inventory.write_text(json.dumps({
            'target': 'fixture-target', 'lockfileSha256': self.hash(lock.encode()),
            'packages': [{'name': 'fixture', 'version': '1.0.0'}],
        }))
        self.report = {
            'inventorySha256': self.hash(self.inventory.read_bytes()),
            'packages': [{
                'name': 'fixture', 'version': '1.0.0', 'archiveSha256': checksum,
                'noticeCandidates': [{'path': name, 'bytes': len(data), 'sha256': self.hash(data)} for name, data in files.items()],
            }],
        }
        self.audit = self.root / 'audit.json'
        self.output = self.root / 'bundle.md'
        self.output.write_text('existing output')

    @staticmethod
    def hash(data):
        return hashlib.sha256(data).hexdigest()

    def run_bundle(self, check=False):
        self.audit.write_text(json.dumps(self.report))
        args = [str(SCRIPT), str(self.inventory), str(self.audit), str(self.output)]
        if check:
            args.append('--check')
        with patch.object(sys, 'argv', args), patch.dict('os.environ', {'CARGO_HOME': str(self.root / 'cargo')}), contextlib.redirect_stdout(io.StringIO()):
            self.module.main()

    def reject(self, message, check=False):
        with self.assertRaisesRegex(ValueError, message):
            self.run_bundle(check)
        self.assertEqual(self.output.read_text(), 'existing output')

    def test_reproducible_bundle_deduplicates_text_and_keeps_each_source(self):
        self.run_bundle()
        first = self.output.read_bytes()
        self.run_bundle(check=True)
        self.run_bundle()
        self.assertEqual(self.output.read_bytes(), first)
        text = first.decode()
        self.assertEqual(text.count(self.text.decode()), 1)
        self.assertIn('`LICENSE`', text)
        self.assertIn('`nested/LICENSE.txt`', text)

    def test_stale_inventory_preserves_output(self):
        (self.root / 'Cargo.lock').write_text('changed lockfile')
        self.reject('Stale inventory')

    def test_mismatched_package_identity_preserves_output(self):
        self.report['packages'][0]['version'] = '2.0.0'
        self.reject('package identities differ')

    def test_corrupted_archive_preserves_output(self):
        self.archive.write_bytes(self.archive.read_bytes() + b'altered')
        self.reject('Archive checksum mismatch')

    def test_altered_notice_hash_preserves_output(self):
        self.report['packages'][0]['noticeCandidates'][0]['sha256'] = '0' * 64
        self.reject('Notice checksum mismatch')

    def test_stale_bundle_check_preserves_output(self):
        self.reject('bundle is stale', check=True)


if __name__ == '__main__':
    unittest.main()

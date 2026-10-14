#!/usr/bin/env python3
"""The upgrade table reproduces the groups each published release ran."""
import json
from pathlib import Path
import tempfile
import unittest

import upgrade_matrix as matrix


class UpgradeMatrixTests(unittest.TestCase):
    def test_published_releases_keep_their_upgrade_groups(self):
        expected = {
            "2.3.0": ["v1", "v2", "v21", "v22"],
            "2.4.0": ["v1", "v2", "v21", "v22", "v23"],
            "2.5.0": ["v1", "v2", "v21", "v22", "v23", "v24"],
            "2.5.1": ["v1", "v2", "v21", "v22", "v23", "v24", "v25"],
            "2.5.2": ["v1", "v2", "v21", "v22", "v23", "v24", "v25", "v251"],
        }
        for version, labels in expected.items():
            self.assertEqual([u.label for u in matrix.applicable(version)], labels, version)

    def test_later_candidates_need_no_table_edit(self):
        for version in ["2.5.3", "2.6.0", "2.10.0"]:
            self.assertEqual(matrix.applicable(version), matrix.applicable("2.5.2"), version)
        self.assertTrue(matrix.at_least("2.10.0", "2.9.0"))

    def test_missing_or_mismatched_previous_packages_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "pkg"
            package.mkdir()
            (package / "package.json").write_text(json.dumps({"version": "2.4.0"}))
            with self.assertRaisesRegex(AssertionError, "requires previous Node packages"):
                matrix.previous_packages([f"2.4.0={package}"], "2.5.0")
            with self.assertRaisesRegex(AssertionError, "not 2.3.0"):
                matrix.previous_packages([f"2.3.0={package}"], "2.5.0")


if __name__ == "__main__":
    unittest.main()

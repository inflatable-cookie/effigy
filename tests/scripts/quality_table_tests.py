#!/usr/bin/env python3
"""Focused checks for the quality-table source hashing helpers."""

import importlib.machinery
import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/quality-table.py"
loader = importlib.machinery.SourceFileLoader("quality_table", str(SCRIPT))
spec = importlib.util.spec_from_loader(loader.name, loader)
quality_table = importlib.util.module_from_spec(spec)
loader.exec_module(quality_table)


class QualityTableTests(unittest.TestCase):
    def test_rust_function_hash_uses_the_whole_function_item(self):
        text = "#[test]\nfn sample() { assert_eq!(1, 1); }\n"
        body = quality_table.rust_function(text, "sample")
        self.assertEqual(body, "fn sample() { assert_eq!(1, 1); }")
        self.assertEqual(
            quality_table.sha256("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )

    def test_python_function_lookup_includes_class_test_body(self):
        text = "class Example:\n    def test_value(self):\n        self.assertEqual(1, 1)\n"
        body = quality_table.python_function(text, "Example", "test_value")
        self.assertEqual(body, "def test_value(self):\n        self.assertEqual(1, 1)")

    def test_missing_test_function_is_untracked(self):
        self.assertIsNone(quality_table.rust_function("fn other() {}", "sample"))

    def test_async_tokio_test_is_a_discovered_test_item(self):
        text = (
            "#[tokio::test(flavor = \"multi_thread\", worker_threads = 4)]\n"
            "#[ignore = \"bounded proof\"]\n"
            "async fn sample() { assert_eq!(1, 1); }\n"
        )
        start = text.index("async fn") + len("async ")
        self.assertTrue(quality_table.has_test_attribute(text, start))
        # A plain `async fn` without a test attribute is still not a test.
        self.assertFalse(quality_table.has_test_attribute("async fn other() {}\n", 6))

    def test_sync_test_with_a_body_modifier_gap_is_still_a_test(self):
        text = "#[test]\nconst fn sample<const N: usize>() -> usize { N }\n"
        start = text.index("const fn") + len("const ")
        self.assertTrue(quality_table.has_test_attribute(text, start))


if __name__ == "__main__":
    unittest.main()

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

    def test_raw_string_braces_do_not_end_or_swallow_the_function(self):
        text = (
            "#[test]\n"
            "fn sample() {\n"
            "    let payload = r#\"{\"key\": 1}\"#;\n"
            "    assert_eq!(payload.len(), 9);\n"
            "}\n"
            "\n"
            "#[test]\n"
            "fn neighbour() { assert!(true); }\n"
        )
        body = quality_table.rust_function(text, "sample")
        # The raw string carries both brace characters; the item still ends at
        # its own closing brace and does not absorb the next test.
        self.assertIn("r#\"{\"key\": 1}\"#", body)
        self.assertNotIn("neighbour", body)
        # A hash-terminated raw string may hold an unmatched brace on its own.
        lopsided = "#[test]\nfn sample() { let text = r#\"}\"#; assert!(!text.is_empty()); }\n"
        self.assertEqual(
            quality_table.rust_function(lopsided, "sample"),
            "fn sample() { let text = r#\"}\"#; assert!(!text.is_empty()); }",
        )

    def test_char_literals_and_lifetimes_do_not_break_brace_balance(self):
        text = (
            "#[test]\n"
            "fn sample<'a>(value: &'a str) {\n"
            "    let open = '{';\n"
            "    let close = '}';\n"
            "    let quote = '\\'';\n"
            "    assert_eq!((open, close, quote, value), ('{', '}', '\\'', value));\n"
            "}\n"
            "\n"
            "#[test]\n"
            "fn neighbour() { assert!(true); }\n"
        )
        body = quality_table.rust_function(text, "sample")
        self.assertIsNotNone(body, "a lifetime must not be read as a char literal")
        self.assertIn("let close = '}'", body)
        self.assertNotIn("neighbour", body)
        self.assertEqual(
            quality_table.char_literal_end("&'a str", 1), None,
            "a lifetime is not a char literal",
        )

    def test_block_and_line_comments_are_skipped(self):
        text = (
            "#[test]\n"
            "fn sample() {\n"
            "    // a line comment with an unbalanced } brace\n"
            "    /* a nested /* block */ comment carrying { braces */\n"
            "    assert!(true);\n"
            "}\n"
            "\n"
            "#[test]\n"
            "fn neighbour() { assert!(true); }\n"
        )
        body = quality_table.rust_function(text, "sample")
        self.assertIn("assert!(true)", body)
        self.assertNotIn("neighbour", body)

    def test_raw_string_prefixes_and_escape_sequences_are_skipped(self):
        text = (
            "#[test]\n"
            "fn sample() {\n"
            "    let bytes = br#\"}{\"#;\n"
            "    let c = cr#\"{}\"#;\n"
            "    let escaped = \"a } brace and a \\\" quote\";\n"
            "    assert!(!bytes.is_empty() && !c.is_empty() && !escaped.is_empty());\n"
            "}\n"
            "\n"
            "#[test]\n"
            "fn neighbour() { assert!(true); }\n"
        )
        body = quality_table.rust_function(text, "sample")
        self.assertIn("escaped", body)
        self.assertNotIn("neighbour", body)


if __name__ == "__main__":
    unittest.main()

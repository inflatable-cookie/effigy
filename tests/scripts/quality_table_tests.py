#!/usr/bin/env python3
"""Focused checks for the quality-table source hashing helpers."""

import importlib.machinery
import importlib.util
import contextlib
import csv
import hashlib
import io
import subprocess
import tempfile
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

    def test_duration_mode_reads_junit_and_orders_slow_tests(self):
        fixture = Path(__file__).parent / "fixtures/quality-table-junit.xml"
        result = subprocess.run(
            ["python3", str(SCRIPT), "--durations", str(fixture), "--report-only", "--top", "1"],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("JUnit: 2 testcases from 1 report(s)", result.stdout)
        self.assertIn("2345 ms  quality-table-junit / effigy::slow_case", result.stdout)
        self.assertNotIn("fast_case", result.stdout)

    def test_duration_identity_normalizes_tests_module_and_refuses_ambiguous_leaf(self):
        row = {"id": "effigy::runner::container_command::tests::creator_evidence::named"}
        case = {
            "classname": "effigy",
            "name": "runner::container_command::reserved_input::creator_evidence::tests::named",
        }
        self.assertEqual(quality_table.match_quality_row(case, [row]), row["id"])
        ambiguous = [
            {"id": "effigy::left::same"},
            {"id": "effigy::right::same"},
        ]
        self.assertIsNone(
            quality_table.match_quality_row({"classname": "effigy", "name": "same"}, ambiguous)
        )

    def test_serialized_group_filters_accept_both_toml_quote_styles(self):
        groups = quality_table.group_filters(quality_table.ROOT / "tests/config/kernel-faults-nextest.toml")
        self.assertIn("kernel-fault-drivers", groups)
        limits = quality_table.group_limits(quality_table.ROOT / "tests/config/kernel-faults-nextest.toml")
        self.assertEqual(limits["kernel-fault-drivers"], 2)

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


    def test_code_body_hash_concatenates_sorted_paths_and_refuses_gaps(self):
        blobs = {"b.rs": b"second", "a.rs": b"first"}
        hashed = quality_table.code_body(["b.rs", "a.rs"], blobs)
        self.assertEqual(hashed, quality_table.code_body(["a.rs", "b.rs"], blobs))
        self.assertEqual(hashed, hashlib.sha256(b"firstsecond").hexdigest())
        self.assertIsNone(quality_table.code_body(["a.rs", "missing.rs"], blobs))

    def test_code_drift_classifies_unchanged_changed_and_gone(self):
        steady = quality_table.code_body(["keep.rs"], {"keep.rs": b"same"})
        rows = [
            {"id": "steady", "files": ["keep.rs"], "body_sha": steady},
            {"id": "edited", "files": ["edit.rs"], "body_sha": "0" * 64},
            {"id": "removed", "files": ["old.rs", "old/helper.rs"], "body_sha": "1" * 64},
        ]
        blobs = {"keep.rs": b"same", "edit.rs": b"different", "old/helper.rs": b"orphan"}
        drift = quality_table.code_drift(rows, blobs, {"keep.rs", "edit.rs", "old/helper.rs"})
        self.assertEqual(drift["changed"], ["edited"])
        self.assertEqual(drift["gone"], [("removed", ["old.rs"])])
        self.assertEqual(drift["new"], [])
        self.assertEqual(drift["splits"], [])

    def test_code_drift_reports_new_files_and_split_re_rate_groups(self):
        rows = [
            {"id": "split-row", "files": ["pkg/old.rs"], "body_sha": "2" * 64},
            {"id": "kept", "files": ["pkg/kept.rs"],
             "body_sha": quality_table.code_body(["pkg/kept.rs"], {"pkg/kept.rs": b"k"})},
        ]
        blobs = {"pkg/old.rs": None, "pkg/kept.rs": b"k"}
        product = {"pkg/kept.rs", "pkg/old/part.rs", "pkg/old/extra.rs",
                   "pkg/sibling.rs", "elsewhere/fresh.rs"}
        drift = quality_table.code_drift(rows, blobs, product)
        self.assertEqual(drift["gone"], [("split-row", ["pkg/old.rs"])])
        self.assertEqual(drift["new"], ["elsewhere/fresh.rs", "pkg/old/extra.rs",
                                          "pkg/old/part.rs", "pkg/sibling.rs"])
        # Only files under the gone file's stem directory group as its split;
        # a new sibling in the same parent directory stays a plain new file.
        self.assertEqual(drift["splits"], [("split-row", ["pkg/old/extra.rs", "pkg/old/part.rs"])])

    def test_code_mode_runs_against_a_small_fixture(self):
        root = Path(tempfile.mkdtemp())
        table_dir = Path(tempfile.mkdtemp())
        (root / "src").mkdir()
        (root / "src" / "same.rs").write_bytes(b"same")
        (root / "src" / "changed.rs").write_bytes(b"after")
        body = lambda name, data: quality_table.code_body([name], {name: data})
        with (table_dir / "C1.csv").open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=["id", "files", "loc", "given",
                                                         "corrected", "source", "action",
                                                         "reason", "body_sha"])
            writer.writeheader()
            writer.writerow({"id": "same", "files": "src/same.rs", "loc": "1",
                             "given": "8", "corrected": "8", "source": "offset",
                             "action": "keep", "reason": "r",
                             "body_sha": body("src/same.rs", b"same")})
            writer.writerow({"id": "changed", "files": "src/changed.rs", "loc": "1",
                             "given": "8", "corrected": "8", "source": "offset",
                             "action": "keep", "reason": "r",
                             "body_sha": body("src/changed.rs", b"before")})
            writer.writerow({"id": "gone", "files": "src/gone.rs", "loc": "1",
                             "given": "8", "corrected": "8", "source": "offset",
                             "action": "keep", "reason": "r", "body_sha": "3" * 64})
        stream = io.StringIO()
        with contextlib.redirect_stdout(stream):
            status = quality_table.run_code_mode(False, table_dir, root)
        self.assertEqual(status, 0)
        report = stream.getvalue()
        self.assertIn("code quality: 3 rows; 0 new; 1 changed; 1 gone", report)
        self.assertIn("changed", report)
        stream = io.StringIO()
        with contextlib.redirect_stdout(stream):
            status = quality_table.run_code_mode(True, table_dir, root)
        self.assertEqual(status, 0)
        self.assertIn("code quality: 3 rows, 0 new, 1 changed, 1 gone", stream.getvalue())


if __name__ == "__main__":
    unittest.main()

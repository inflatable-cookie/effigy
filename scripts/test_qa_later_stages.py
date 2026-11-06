#!/usr/bin/env python3
"""Deterministic subprocess controls for the later-stage receipt wrapper."""

import contextlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("qa-later-stages.py")
SPEC = importlib.util.spec_from_file_location("qa_later_stages", SCRIPT)
qa = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = qa
SPEC.loader.exec_module(qa)


class CollectorControls(unittest.TestCase):
    def setUp(self):
        temp_root = Path(os.environ["TMPDIR"])
        self.fixture = Path(tempfile.mkdtemp(prefix="qa-later-control.", dir=temp_root))
        self.temp = self.fixture / "tmp"
        self.temp.mkdir(mode=0o700)
        self.checkout = self.fixture / "checkout"
        self.checkout.mkdir()
        subprocess.run(["git", "init", "-q", str(self.checkout)], check=True)
        subprocess.run(["git", "-C", str(self.checkout), "config", "user.email", "test@example.invalid"], check=True)
        subprocess.run(["git", "-C", str(self.checkout), "config", "user.name", "Fixture"], check=True)
        subprocess.run(["git", "-C", str(self.checkout), "config", "gc.auto", "0"], check=True)
        (self.checkout / "input.txt").write_text("unchanged\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(self.checkout), "add", "input.txt"], check=True)
        subprocess.run(["git", "-C", str(self.checkout), "commit", "-qm", "fixture"], check=True)
        self.output = self.fixture / "evidence"
        self.call_log = self.fixture / "calls.log"
        self.lifecycle = self.fixture / "lifecycle.sh"
        self.lifecycle.write_text(
            "#!/bin/sh\n"
            "echo \"$1\" >> \"$CALL_LOG\"\n"
            "if [ \"$1\" = setup ] && [ \"${FAIL_SETUP:-0}\" = 1 ]; then exit 7; fi\n"
            "if [ \"$1\" = teardown ] && [ \"${FAIL_TEARDOWN:-0}\" = 1 ]; then exit 8; fi\n",
            encoding="utf-8",
        )
        self.effigy = self.fixture / "effigy"
        self.effigy.write_text(
            "#!/usr/bin/env python3\n"
            "import os, signal, sys\n"
            "selector=sys.argv[1]\n"
            "with open(os.environ['CALL_LOG'],'a') as f: f.write(selector+'\\n')\n"
            "print('out:'+selector)\n"
            "print('err:'+selector,file=sys.stderr)\n"
            "if selector == os.environ.get('CANCEL_SELECTOR'): os.kill(os.getpid(), signal.SIGTERM)\n"
            "if selector == os.environ.get('FAIL_SELECTOR'): raise SystemExit(9)\n",
            encoding="utf-8",
        )
        self.lifecycle.chmod(0o700)
        self.effigy.chmod(0o700)
        qa.CANCELLED.clear()

    def tearDown(self):
        qa.CANCELLED.clear()
        import shutil
        shutil.rmtree(self.fixture)

    def run_wrapper(self, extra=None, selected=None):
        env = dict(os.environ)
        env.update({"CALL_LOG": str(self.call_log)})
        if extra:
            env.update(extra)
        with open(os.devnull, "w") as sink, contextlib.redirect_stdout(sink):
            code, receipt = qa.run_collector(
                self.output, self.checkout, self.temp, self.lifecycle,
                self.effigy, env, selected=selected
            )
        return code, receipt

    def test_order_once_continue_after_failure_and_honest_partial_result(self):
        failed = "test:kernel:matrix"
        code, receipt = self.run_wrapper({"FAIL_SELECTOR": failed})
        calls = self.call_log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(calls, ["setup", *qa.STAGES, "teardown"])
        self.assertEqual(receipt["stage_order"], list(qa.STAGES))
        self.assertEqual(receipt["selected_stages"], list(qa.STAGES))
        self.assertEqual(receipt["omitted_stages"], [])
        self.assertEqual(receipt["failed_stages"], [failed])
        self.assertEqual(code, 1)
        self.assertTrue(receipt["diagnostic_only"])
        self.assertTrue(receipt["default_suite_omitted"])
        self.assertFalse(receipt["full_qa_pass"])
        target = next(s for s in receipt["stages"] if s["selector"] == failed)
        self.assertEqual(target["result"]["returncode"], 9)
        self.assertIn("out:" + failed, (self.output / target["stdout"]).read_text())
        self.assertIn("err:" + failed, (self.output / target["stderr"]).read_text())
        self.assertEqual((self.output / target["stdout"]).stat().st_mode & 0o777, 0o600)

    def test_success_is_still_not_full_qa_and_source_is_unchanged(self):
        before = qa.input_snapshot(self.checkout)
        code, receipt = self.run_wrapper()
        after = qa.input_snapshot(self.checkout)
        self.assertEqual(code, 0)
        self.assertTrue(receipt["complete_partial_census"])
        self.assertFalse(receipt["full_qa_pass"])
        self.assertEqual(before["files"], after["files"])
        self.assertEqual(before["index_sha256"], after["index_sha256"])
        self.assertTrue(all(s["status"] == "completed" for s in receipt["stages"]))

    def test_exact_stage_range_records_selected_and_omitted_coverage(self):
        first, last = qa.STAGES[4], qa.STAGES[5]
        selected = qa.select_stages(first, last)
        code, receipt = self.run_wrapper(selected=selected)
        calls = self.call_log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(calls, ["setup", first, last, "teardown"])
        self.assertEqual(code, 0)
        self.assertEqual(receipt["selected_stages"], [first, last])
        self.assertEqual(receipt["omitted_stages"], list(qa.STAGES[:4]) + list(qa.STAGES[6:]))
        self.assertEqual([s["selector"] for s in receipt["stages"]], [first, last])
        self.assertTrue(receipt["complete_selected_slice"])
        self.assertFalse(receipt["complete_partial_census"])

    def test_invalid_or_reversed_ranges_reject_before_collector_effects(self):
        for start, stop, message in (("unknown", None, "unknown start"),
                                     (qa.STAGES[2], qa.STAGES[1], "follows"),
                                     (None, "unknown", "unknown stop")):
            with self.assertRaisesRegex(ValueError, message):
                qa.select_stages(start, stop)
        self.assertFalse(self.output.exists())
        self.assertFalse(self.call_log.exists())

    def test_finalized_stage_receipt_survives_later_cancelled_stage(self):
        selected = qa.STAGES[:2]
        code, receipt = self.run_wrapper({"CANCEL_SELECTOR": selected[1]}, selected=selected)
        events = self.output / "events"
        first = json.loads((events / "stage-01.json").read_text(encoding="utf-8"))
        self.assertEqual(first["selector"], selected[0])
        self.assertEqual(first["status"], "completed")
        self.assertEqual(first["result"]["returncode"], 0)
        self.assertTrue((events / "stage-02.json").is_file())
        self.assertEqual(receipt["selected_stages"], list(selected))
        self.assertEqual(receipt["not_started_stages"], [])
        self.assertEqual(code, 1)
        self.assertTrue((events / "selection.json").is_file())
        self.assertTrue((events / "setup.json").is_file())
        self.assertTrue((events / "teardown.json").is_file())

    def test_setup_failure_starts_no_stages_and_still_attempts_teardown(self):
        code, receipt = self.run_wrapper({"FAIL_SETUP": "1"})
        calls = self.call_log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(calls, ["setup", "teardown"])
        self.assertEqual(code, 1)
        self.assertTrue(all(s["status"] == "not_started" for s in receipt["stages"]))
        self.assertEqual(receipt["setup"]["returncode"], 7)

    def test_teardown_failure_makes_successful_stages_aggregate_failure(self):
        code, receipt = self.run_wrapper({"FAIL_TEARDOWN": "1"})
        self.assertEqual(code, 1)
        self.assertEqual(receipt["teardown"]["returncode"], 8)
        self.assertFalse(receipt["complete_partial_census"])

    def test_signalled_child_stops_and_marks_remaining_not_started(self):
        cancelled = qa.STAGES[2]
        code, receipt = self.run_wrapper({"CANCEL_SELECTOR": cancelled})
        calls = self.call_log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(calls, ["setup", *qa.STAGES[:3], "teardown"])
        self.assertEqual(code, 1)
        statuses = {s["selector"]: s["status"] for s in receipt["stages"]}
        self.assertEqual(statuses[cancelled], "cancelled")
        self.assertTrue(all(statuses[s] == "not_started" for s in qa.STAGES[3:]))

    def test_output_identity_refusal_and_private_directory(self):
        self.output.mkdir()
        with self.assertRaisesRegex(ValueError, "already exists"):
            qa.validate_destinations(self.output, self.temp, self.checkout)
        with self.assertRaisesRegex(ValueError, "outside the checkout"):
            qa.validate_destinations(self.checkout / "nested-output", self.temp, self.checkout)
        symlink = self.fixture / "output-link"
        symlink.symlink_to(self.fixture / "absent", target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlink"):
            qa.validate_destinations(symlink, self.temp, self.checkout)
        with self.assertRaisesRegex(ValueError, "outside TMPDIR"):
            qa.validate_destinations(self.temp / "nested", self.temp, self.checkout)


if __name__ == "__main__":
    raise SystemExit(unittest.main(verbosity=2).result.wasSuccessful() is False)

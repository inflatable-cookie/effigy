#!/usr/bin/env python3
"""Contract tests for the feedback compiler-diagnostic ratchet."""
import importlib.util
import pathlib
import subprocess
import tempfile
import unittest

path = pathlib.Path(__file__).with_name('check-feedback-inventory.py')
spec = importlib.util.spec_from_file_location('feedback_inventory', path)
lint = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lint)


def diagnostic(file, line, text):
    return {'reason': 'compiler-message', 'message': {
        'message': lint.MARKER,
        'spans': [{'is_primary': True, 'file_name': file, 'line_start': line, 'column_start': 1,
                   'text': [{'text': text}]}]}}


class InventoryTests(unittest.TestCase):
    def test_new_literal_and_renamed_helper_are_reported(self):
        observed = lint.entries([
            diagnostic('src/new.rs', 4, 'step("working...")'),
            diagnostic('src/new.rs', 8, 'renamed_static_helper("working...")'),
        ])
        errors = lint.check(observed, [], [])
        self.assertEqual(len(errors), 2)
        self.assertIn('src/new.rs:4', errors[0])
        self.assertIn('migrate to a live progress source', errors[0])
        self.assertIn('src/new.rs:8', errors[1])

    def test_only_one_live_sink_is_exempt_after_a_line_shift(self):
        text = 'let bar = self.multi().add(ProgressBar::new_spinner());'
        moved = diagnostic('crates/effigy-ui/src/steps.rs', 455, text)
        duplicate = diagnostic('crates/effigy-ui/src/steps.rs', 456, text)
        moved['message']['spans'][0]['column_start'] = 36
        duplicate['message']['spans'][0]['column_start'] = 36
        self.assertEqual(lint.entries([moved]), [])
        self.assertEqual(len(lint.entries([moved, duplicate])), 2)

    def test_relocated_site_passes_but_an_additional_site_fails(self):
        relocated = [['src/a.rs', 6, 'step(label)', 1]]
        observed = [('src/a.rs', 'step(label)', 6, 1)]
        self.assertEqual(lint.check(observed, relocated, relocated), [])
        errors = lint.check(observed + [('src/a.rs', 'step(label)', 7, 1)], relocated, relocated)
        self.assertTrue(any('src/a.rs:7' in error for error in errors), errors)

    def test_event_source_emits_no_legacy_diagnostic(self):
        observed = lint.entries([{'reason': 'compiler-artifact'}])
        self.assertEqual(lint.check(observed, [], []), [])

    def test_compiler_resolves_a_renamed_static_helper(self):
        # Build a standalone fixture in an isolated directory through Oxide.
        # Its API carries the same deprecation marker as the checked API.
        fixture = r"""
#[deprecated(note = "feedback legacy spinner: migrate to a live progress source")]
fn step(_: &str) {}
use step as renamed_static_helper;
struct Source;
fn live_step(_: Source) {}
fn fixture() {
    step("working...");
    let fixed = "fixed wait";
    step(fixed);
    renamed_static_helper("waiting...");
    live_step(Source);
}
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            source = pathlib.Path(directory) / 'fixture.rs'
            source.write_text(fixture)
            result = subprocess.run(
                ['oxide', 'exec', '--', 'rustc', '--crate-type', 'lib', '--error-format=json',
                 '--emit=metadata', '-o', str(pathlib.Path(directory) / 'fixture.rmeta'), str(source)],
                text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True,
            )
            messages = []
            for line in result.stderr.splitlines():
                try:
                    message = __import__('json').loads(line)
                except ValueError:
                    continue
                if isinstance(message, dict):
                    messages.append({'reason': 'compiler-message', 'message': message})
            found = lint.entries(messages)
            texts = [item[1] for item in found]
            self.assertTrue(any('step("working...")' in text for text in texts), texts)
            self.assertTrue(any('step(fixed)' in text for text in texts), texts)
            self.assertTrue(any('renamed_static_helper("waiting...")' in text for text in texts), texts)
            self.assertFalse(any('live_step(Source)' in text for text in texts), texts)

    def test_baseline_cannot_grow_and_removed_sites_must_leave_it(self):
        baseline = [['src/a.rs', 5, 'step(label)', 1]]
        self.assertIn('grew', lint.check([], baseline, [])[0])
        self.assertIn('removed sites', lint.check([], baseline, baseline)[0])
        self.assertEqual(lint.check([('src/a.rs', 'step(label)', 5, 1)], baseline, baseline), [])
        self.assertTrue(lint.check([('src/a.rs', 'step(label)', 6, 1)], baseline, baseline))


if __name__ == '__main__':
    unittest.main()

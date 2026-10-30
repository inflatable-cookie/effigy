#!/usr/bin/env python3
"""Contract tests for the feedback compiler-diagnostic gate."""
import importlib.util
import json
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
    def test_every_legacy_entry_point_is_reported_with_its_migration(self):
        observed = lint.entries([
            diagnostic('src/new.rs', 4, 'step("working...")'),
            diagnostic('src/new.rs', 8, 'let child = parent.step("working...");'),
            diagnostic('src/new.rs', 12, 'let step = TimedStep::new("working...");'),
            diagnostic('src/new.rs', 16, 'effigy_containers::exec::phase("working...")'),
        ])
        errors = lint.check(observed)
        self.assertEqual(len(errors), 4)
        self.assertIn('src/new.rs:4', errors[0])
        self.assertIn('live_step(ProgressSource::waiting_for', errors[0])
        self.assertIn('parent.live_step', errors[1])
        self.assertIn('TimedStep::live', errors[2])
        self.assertIn('runtime_wait', errors[3])

    def test_there_is_no_allowlist_so_a_renamed_helper_still_fails(self):
        observed = lint.entries([diagnostic('src/wrapped.rs', 21, 'renamed_static_helper("working...")')])
        errors = lint.check(observed)
        self.assertEqual(len(errors), 1)
        self.assertIn('src/wrapped.rs:21', errors[0])
        self.assertIn('migrate to a live progress source', errors[0])

    def test_only_one_live_sink_is_exempt_after_a_line_shift(self):
        text = 'let bar = self.multi().add(ProgressBar::new_spinner());'
        moved = diagnostic('crates/effigy-ui/src/steps.rs', 455, text)
        duplicate = diagnostic('crates/effigy-ui/src/steps.rs', 456, text)
        moved['message']['spans'][0]['column_start'] = 36
        duplicate['message']['spans'][0]['column_start'] = 36
        self.assertEqual(lint.entries([moved]), [])
        self.assertEqual(len(lint.entries([moved, duplicate])), 2)

    def test_a_clean_tree_reports_nothing(self):
        self.assertEqual(lint.check(lint.entries([{'reason': 'compiler-artifact'}])), [])

    def test_the_inventory_builds_every_workspace_package(self):
        # A `-p` list omits a crate that is only another package's dependency,
        # and an omitted crate is never linted as a package (finding: a
        # fixed-label wrapper there would escape the inventory).
        command = lint.clippy_command()
        self.assertIn('--workspace', command)
        self.assertIn('--all-targets', command)
        self.assertNotIn('-p', command)

    def test_compiler_resolves_a_renamed_static_helper(self):
        # Build a standalone fixture in an isolated directory through Oxide.
        # Its API carries the same deprecation marker as the checked API.
        fixture = r"""
#[deprecated(note = "feedback legacy spinner: migrate to a live progress source")]
pub fn step(_: &str) {}
use step as renamed_static_helper;
pub struct Source;
pub fn live_step(_: Source) {}
mod wrapper;
fn fixture() {
    step("working...");
    let fixed = "fixed wait";
    step(fixed);
    renamed_static_helper("waiting...");
    live_step(Source);
}
"""
        # A wrapper in another module, standing in for another crate's helper:
        # the diagnostic must name that file and line, not this one.
        wrapper = r"""
pub fn show(label: &str) {
    crate::step(label);
}
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            source = pathlib.Path(directory) / 'fixture.rs'
            source.write_text(fixture)
            (pathlib.Path(directory) / 'wrapper.rs').write_text(wrapper)
            result = subprocess.run(
                ['oxide', 'exec', '--', 'rustc', '--crate-type', 'lib', '--error-format=json',
                 '--emit=metadata', '-o', str(pathlib.Path(directory) / 'fixture.rmeta'), str(source)],
                text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True,
            )
            messages = []
            for line in result.stderr.splitlines():
                try:
                    message = json.loads(line)
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
            wrapped = [item for item in found if item[0].endswith('wrapper.rs')]
            self.assertEqual(len(wrapped), 1, found)
            self.assertIn('step(label)', wrapped[0][1])
            errors = lint.check(wrapped)
            self.assertIn('wrapper.rs:', errors[0])
            self.assertIn('live_step(ProgressSource::waiting_for', errors[0])


if __name__ == '__main__':
    unittest.main()

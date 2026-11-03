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
            diagnostic('src/new.rs', 20, 'steps::step("working...");'),
        ])
        errors = lint.check(observed)
        self.assertEqual(len(errors), 5)
        self.assertIn(lint.MARKER, errors[0])
        self.assertIn('src/new.rs:4', errors[0])
        self.assertIn('live_step(ProgressSource::waiting_for', errors[0])
        self.assertIn('parent.live_step', errors[1])
        self.assertIn('TimedStep::live', errors[2])
        self.assertIn('runtime_wait', errors[3])
        self.assertIn('steps::finish_line', errors[4])

    def test_a_test_target_diagnostic_is_not_a_production_obligation(self):
        # Only production call sites owe the migration: the same marker under
        # a workspace test target is skipped, the one under src is kept.
        errors = lint.entries(
            [diagnostic('crates/effigy-ui/src/render.rs', 9, 'step("working...")')])
        self.assertEqual([entry[0] for entry in errors], ['crates/effigy-ui/src/render.rs'])
        self.assertEqual(lint.entries(
            [diagnostic('crates/effigy-ui/tests/render.rs', 9, 'step("working...")')]), [])
        self.assertEqual(lint.entries(
            [diagnostic('crates/effigy-ui/src/tests.rs', 9, 'step("working...")')]), [])

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

    def test_an_alternative_spinner_constructor_is_reported(self):
        # The finding's fixture: a fixed-label spinner built without any
        # deprecated entry point, through indicatif's other constructors.
        wrapper = """use indicatif::{ProgressBar, ProgressStyle};

pub fn show(label: &str) -> ProgressBar {
    let bar = ProgressBar::no_length();
    bar.set_style(ProgressStyle::default_spinner());
    bar.set_message(label.to_owned());
    bar.enable_steady_tick(std::time::Duration::from_millis(80));
    bar
}
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            root = pathlib.Path(directory)
            (root / 'src').mkdir()
            (root / 'src' / 'wrapper.rs').write_text(wrapper)
            errors = lint.indicatif_errors(root)
        self.assertTrue(errors, 'an alternative constructor must not pass')
        for fragment in ('src/wrapper.rs:1:', 'src/wrapper.rs:4:',
                         'src/wrapper.rs:5:', 'src/wrapper.rs:7:'):
            self.assertTrue(any(fragment in error for error in errors), (fragment, errors))
        self.assertTrue(any('ProgressBar::no_length' in error for error in errors), errors)
        self.assertTrue(
            any('ProgressStyle::default_spinner' in error for error in errors), errors
        )
        self.assertTrue(
            any('live_step(ProgressSource::waiting_for' in error for error in errors), errors
        )

    def test_the_sink_holds_only_the_reviewed_construction(self):
        # Without the sink file the gate names it instead of passing quietly.
        # The reviewed sink (its import, its one allowed non-import mention,
        # the one construction) is clean; a second construction is reported at
        # its own lines.
        reviewed = """use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

impl indicatif::TermLike for FrameTerminal {
    fn width(&self) -> u16 {
        120
    }
}

struct Sink;

impl Sink {
    fn start_bar(&self, label: &str) -> ProgressBar {
        let bar = self.multi().add(ProgressBar::new_spinner());
        if let Ok(style) = ProgressStyle::with_template(SPINNER_TEMPLATE) {
            bar.set_style(style);
        }
        bar.set_message(label.to_owned());
        bar.enable_steady_tick(TICK);
        bar
    }
}
"""
        extra = """fn a_second_spinner() -> ProgressBar {
    let bar = ProgressBar::no_length();
    bar.enable_steady_tick(TICK);
    bar
}
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            root = pathlib.Path(directory)
            missing = lint.sink_errors(root)
            self.assertEqual(len(missing), 1, missing)
            self.assertIn('the reviewed step sink is missing', missing[0])
            sink = root / 'crates/effigy-ui/src/steps.rs'
            sink.parent.mkdir(parents=True)
            sink.write_text(reviewed)
            self.assertEqual(lint.sink_errors(root), [])
            self.assertEqual(lint.indicatif_errors(root), [])
            sink.write_text(reviewed + extra)
            errors = lint.sink_errors(root)
        self.assertEqual(len(errors), 2, errors)
        self.assertIn('crates/effigy-ui/src/steps.rs:23:', errors[0])
        self.assertIn('ProgressBar::no_length', errors[0])
        self.assertIn('crates/effigy-ui/src/steps.rs:24:', errors[1])
        self.assertIn('enable_steady_tick', errors[1])

    def test_the_sink_cannot_re_export_the_spinner_crate(self):
        reviewed = """use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub use indicatif::ProgressBar;

struct Sink;
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            root = pathlib.Path(directory)
            sink = root / 'crates/effigy-ui/src/steps.rs'
            sink.parent.mkdir(parents=True)
            sink.write_text(reviewed)
            errors = lint.sink_errors(root)
        self.assertEqual(len(errors), 1, errors)
        self.assertIn('crates/effigy-ui/src/steps.rs:3:', errors[0])
        self.assertIn('re-export', errors[0])

    def test_only_code_text_is_a_use(self):
        # `code_only` decides what counts as code: literal contents (plain,
        # raw, with embedded quotes — a raw string's contents, inner quote
        # included, hide the code-shaped text after the quote) and comments
        # (line, block, nested block) are blanked, so a message naming a
        # constructor and commented-out code are not uses, delimiters inside
        # strings hide nothing, and every newline is kept, so the one real
        # call is reported at its exact source line.
        source = """/* a leading block
   comment */
pub const HELP: &str = "Use indicatif to draw a ProgressBar";

pub fn message() -> &'static str {
    "call ProgressBar::no_length() and enable_steady_tick"
}

pub fn raw() -> &'static str {
    r#"indicatif ProgressStyle "quoted" /* not a comment */"#
}

pub const RAW_CODE: &str = r#"a " ProgressBar::no_length(); "#;

fn commented_out() {
    // let bar = ProgressBar::no_length();
    /* let other = ProgressBar::no_length();
       /* nested */ let third = ProgressBar::no_length(); */
    let opener = "/*";
    let closer = "*/";
    let slashes = "//";
}

fn show() {
    let bar = ProgressBar::no_length();
}
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            root = pathlib.Path(directory)
            (root / 'src').mkdir()
            (root / 'src' / 'code_only.rs').write_text(source)
            errors = lint.indicatif_errors(root)
        self.assertEqual(len(errors), 1, errors)
        self.assertIn('src/code_only.rs:25:', errors[0])
        self.assertIn('let bar = ProgressBar::no_length();', errors[0])
        self.assertIn('live_step(ProgressSource::waiting_for', errors[0])

    def test_char_literals_and_lifetimes_do_not_blank_later_code(self):
        # A char literal is not a string opener in either spelling, and a
        # lifetime is not a char-literal opener: the plain `'"'` survives only
        # through the plain arm, the escaped `'\"'` only through the escape
        # arm, and a lifetime through neither. If a lifetime is read as an
        # opener, everything up to the next apostrophe goes dark — so the
        # second file puts its next apostrophe after the real call. A gate
        # must not read the rest of the file as one literal and go quiet.
        source = r"""const PLAIN_QUOTE: char = '"';
const ESCAPED_QUOTE: char = '\"';

fn show<'a>(text: &'a str) {
    let bar = ProgressBar::no_length();
    let _ = text;
}
"""
        spread = r"""fn f(x: &'static str) {
    let bar = ProgressBar::no_length();
    let _ = (x, bar, &'static ());
}
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            root = pathlib.Path(directory)
            (root / 'src').mkdir()
            (root / 'src' / 'chars.rs').write_text(source)
            (root / 'src' / 'lifetimes.rs').write_text(spread)
            errors = lint.indicatif_errors(root)
        self.assertEqual(len(errors), 2, errors)
        self.assertIn('src/chars.rs:5:', errors[0])
        self.assertIn('let bar = ProgressBar::no_length();', errors[0])
        self.assertIn('src/lifetimes.rs:2:', errors[1])
        self.assertIn('let bar = ProgressBar::no_length();', errors[1])

    def test_a_suppression_in_a_comment_or_literal_is_not_a_suppression(self):
        # The suppression scan used to read raw text, so a doc or message
        # string mentioning the attribute failed an unrelated change.
        source = """// #[allow(deprecated)] would hide a real use
/* #![allow(warnings)] is not written here */
pub const NOTE: &str = "#[allow(deprecated)] is forbidden";
pub fn message() -> &'static str {
    r#"use #[allow(disallowed_methods)] never"#
}
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            root = pathlib.Path(directory)
            (root / 'src').mkdir()
            (root / 'src' / 'notes.rs').write_text(source)
            self.assertEqual(lint.suppression_errors(root), [])

    def test_a_real_suppression_attribute_is_reported_at_its_line(self):
        source = """fn unrelated() {}

#[allow(deprecated)]
fn hidden() {}

#[expect(clippy::disallowed_methods)]
fn also_hidden() {}

#![allow(warnings)]
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            root = pathlib.Path(directory)
            (root / 'src').mkdir()
            (root / 'src' / 'hidden.rs').write_text(source)
            errors = lint.suppression_errors(root)
        self.assertEqual(len(errors), 3, errors)
        self.assertIn('src/hidden.rs:3:', errors[0])
        self.assertIn('#[allow(deprecated)]', errors[0])
        self.assertIn('src/hidden.rs:6:', errors[1])
        self.assertIn('src/hidden.rs:9:', errors[2])
        for error in errors:
            self.assertIn('migrate to a live progress source', error)

    def test_a_suppression_of_an_unrelated_lint_is_not_reported(self):
        source = """#[allow(dead_code)]
fn kept_for_later() {}

#[expect(unused_variables)]
fn unused(value: u8) { let _ = value; }
"""
        with tempfile.TemporaryDirectory(dir='/Volumes/Builds/scratch/effigy') as directory:
            root = pathlib.Path(directory)
            (root / 'src').mkdir()
            (root / 'src' / 'ordinary.rs').write_text(source)
            self.assertEqual(lint.suppression_errors(root), [])


if __name__ == '__main__':
    unittest.main()

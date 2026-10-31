#!/usr/bin/env python3
"""Reject every fixed-label spinner entry point in production code.

`effigy-ui` keeps its legacy step constructors as deprecated anchors. Any use
of one is a compiler diagnostic at the call site, resolved by rustc before it
is emitted, so aliasing or wrapping a helper does not hide it. This check
collects those diagnostics and fails on every one of them: there is no
allowlist, and a removed site leaves nothing behind to keep in step (Q-017,
effigy#467).

Each failure names the file and line and the exact migration: a step that can
wait is driven by a live `ProgressSource`, and an immediate outcome is one
`finish_line`. A suppression (`#[allow(deprecated)]` and friends) is itself a
failure, so a site cannot be hidden instead of migrated.

The build covers every workspace target (`--workspace --all-targets`) and not a
chosen package list: a crate compiled only as another package's dependency is
never linted as a package, so a wrapper there would escape a `-p` selection.

A compiler diagnostic only exists for the constructors we mark, so the check
also owns the crate itself: `indicatif` may be named in exactly one production
file (the reviewed step sink), and that file may build exactly one
`ProgressBar` with exactly one style and one steady tick. A fixed-label
spinner built through another constructor (`ProgressBar::no_length`,
`ProgressBar::new`, `ProgressBar::with_draw_target`, `ProgressStyle::
default_spinner`, `enable_steady_tick`) is therefore reported at its own file
and line, whether it is written directly or hidden behind a wrapper. Comments
and literal contents are blanked before the scan, with every newline kept, so a
message or fixture that names a constructor is not read as one and the line
numbers stay exact.
"""
import collections
import json
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
MARKER = 'feedback legacy spinner: migrate to a live progress source'
SINK_FILE = 'crates/effigy-ui/src/steps.rs'
SINK_CONSTRUCTION = 'let bar = self.multi().add(ProgressBar::new_spinner());'
# The one reviewed spinner sink: `Step::start_bar` builds the only
# `ProgressBar` in the workspace. `config/feedback-lint/clippy.toml` disallows
# every other constructor, and the sink file may hold only these lines.
SINK = (SINK_FILE, SINK_CONSTRUCTION, 36)
SINK_LINES = (
    SINK_CONSTRUCTION,
    'if let Ok(style) = ProgressStyle::with_template(SPINNER_TEMPLATE) {',
    'bar.enable_steady_tick(TICK);',
)
# What builds or animates a bar. A type mention (an import, a field or a
# signature) is not a second construction, so only these spellings count here.
SINK_BUILD_TERMS = ('ProgressBar::', 'ProgressStyle::', 'enable_steady_tick')
# The sink may name the crate only in a plain import or the test helper's
# trait impl. A re-export or a type alias would let another file build a bar
# without naming `indicatif` or `ProgressBar`.
SINK_INDICATIF_LINES = (
    'impl indicatif::TermLike for FrameTerminal {',
)
# A spinner needs a `ProgressBar`; these terms name it or the pieces that make
# one animate a fixed label. Only the sink file may spell them.
INDICATIF_TERMS = ('indicatif', 'ProgressBar', 'ProgressStyle', 'enable_steady_tick')
SPINNER_MIGRATION = ('only the reviewed step sink builds progress: use '
                     '`steps::live_step(ProgressSource::waiting_for("the exact wait"))` for a '
                     'wait, or `steps::finish_line(ok, summary)` for an immediate outcome')

# The migration each deprecated entry point needs, chosen from the call site
# rustc reports. A renamed alias of one of these still lands on one of them.
MIGRATIONS = (
    ('.step(', 'use `parent.live_step(ProgressSource::waiting_for("the exact wait"))`'),
    ('TimedStep::new', 'use `TimedStep::live(ProgressSource::waiting_for("the exact wait"))`'),
    ('phase(', 'use `runtime_wait(label)` or a live `TimedStep`'),
    ('steps::step(', 'use `steps::live_step(ProgressSource::waiting_for("the exact wait"))` for a '
                     'wait, or `steps::finish_line(ok, summary)` for an immediate outcome'),
)
DEFAULT_MIGRATION = ('use `live_step(ProgressSource::waiting_for("the exact wait"))` for a wait, or '
                     '`finish_line` for an immediate outcome')


def migration(text):
    for needle, action in MIGRATIONS:
        if needle in text:
            return action
    return DEFAULT_MIGRATION


def clippy_command():
    """The inventory build: every workspace target, no chosen package list.

    `--workspace` is the enforcement, not a convenience: a crate that is only
    compiled as a dependency of a selected package is not linted as a package,
    so a fixed-label wrapper in it would escape a `-p` list.
    """
    return ['sh', 'scripts/oxide-selector.sh', 'clippy', '--workspace', '--all-targets',
            '--message-format=json', '--',
            '-W', 'deprecated', '-W', 'clippy::disallowed_methods']


def entries(messages):
    result = []
    for message in messages:
        diagnostic = message.get('message', {})
        if message.get('reason') != 'compiler-message' or MARKER not in json.dumps(diagnostic):
            continue
        for span in diagnostic.get('spans', []):
            if not span.get('is_primary'):
                continue
            file = span['file_name']
            # Test diagnostics are not a production migration obligation.
            if '/tests/' in file or file.endswith('/tests.rs'):
                continue
            text = '\n'.join(line['text'].strip() for line in span.get('text', []))
            result.append((file, text, span['line_start'], span['column_start']))
    result = list(dict.fromkeys(result))
    # Only the unique infrastructure sink is exempt. A duplicate makes every
    # matching diagnostic visible to the check.
    matching = [entry for entry in result if (entry[0], entry[1], entry[3]) == SINK]
    if len(matching) == 1:
        result.remove(matching[0])
    return result


def check(observed):
    return [
        f'{file}:{line}: {text}; feedback legacy spinner: migrate to a live progress source: '
        f'{migration(text)}'
        for file, text, line, _ in observed
    ]


RAW_STRING = re.compile(r'(?:br|rb|r)(#*)"')


def _raw_string_start(text, index):
    """The hash count and opening quote of a raw string at `index`, if any."""
    if index > 0 and (text[index - 1].isalnum() or text[index - 1] == '_'):
        return None
    match = RAW_STRING.match(text, index)
    return (len(match.group(1)), match.end() - 1) if match else None


def _char_literal_end(text, index):
    """The end of a char literal at `index`, or None for a lifetime."""
    if index + 1 >= len(text):
        return None
    if text[index + 1] == '\\':
        cursor = index + 2
        while cursor < len(text) and text[cursor] != "'":
            cursor += 2 if text[cursor] == '\\' else 1
        return cursor + 1 if cursor < len(text) else None
    if index + 2 < len(text) and text[index + 2] == "'" and text[index + 1] != "'":
        return index + 3
    return None


def _blank(fragment):
    """A same-length fragment with every column but the newlines blanked."""
    return ''.join('\n' if char == '\n' else ' ' for char in fragment)


def code_only(text):
    """Return `text` with comments and literal contents blanked out.

    This is a small Rust lexer, not a regular expression, and it keeps every
    newline, so a reported line is the real source line. Two things follow:

    - a `//` or `/*` inside a string, raw string or char literal is data, not a
      comment, so a literal can never hide the code after it;
    - a literal's contents are blanked, so a message, fixture or doc string that
      names a constructor is not read as one, while code after the literal
      stays visible.

    Nested block comments are handled too.
    """
    kept = []
    index = 0
    length = len(text)
    while index < length:
        char = text[index]
        if char == '/' and text.startswith('//', index):
            end = text.find('\n', index)
            end = length if end < 0 else end
            kept.append(_blank(text[index:end]))
            index = end
            continue
        if char == '/' and text.startswith('/*', index):
            start = index
            depth = 1
            index += 2
            while index < length and depth:
                if text.startswith('/*', index):
                    depth += 1
                    index += 2
                elif text.startswith('*/', index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            kept.append(_blank(text[start:index]))
            continue
        raw = _raw_string_start(text, index)
        if raw:
            hashes, quote = raw
            terminator = '"' + '#' * hashes
            end = text.find(terminator, quote + 1)
            end = length if end < 0 else end + len(terminator)
            kept.append(_blank(text[index:end]))
            index = end
            continue
        if char == '"':
            start = index
            index += 1
            while index < length:
                if text[index] == '\\':
                    index += 2
                    continue
                if text[index] == '"':
                    index += 1
                    break
                index += 1
            kept.append(_blank(text[start:index]))
            continue
        if char == "'":
            end = _char_literal_end(text, index)
            if end is None:
                kept.append(char)
                index += 1
            else:
                kept.append(_blank(text[index:end]))
                index = end
            continue
        kept.append(char)
        index += 1
    return ''.join(kept)


def source_line(text, number):
    """The original line `number`, for a message about the code around it."""
    lines = text.splitlines()
    return lines[number - 1].strip() if 0 < number <= len(lines) else ''


def source_files(root, *directories):
    for directory in directories:
        base = root / directory
        if not base.is_dir():
            continue
        for source in sorted(base.rglob('*.rs')):
            yield source


def indicatif_errors(root):
    """No production file outside the reviewed sink may name indicatif."""
    errors = []
    for source in source_files(root, 'src', 'crates', 'tests'):
        relative = source.relative_to(root).as_posix()
        if relative == SINK_FILE:
            continue
        text = source.read_text()
        for number, line in enumerate(code_only(text).splitlines(), 1):
            if any(term in line for term in INDICATIF_TERMS):
                errors.append(f'{relative}:{number}: {source_line(text, number)}; {SPINNER_MIGRATION}')
    return errors


def sink_errors(root):
    """The sink file may build only the reviewed bar, style and tick."""
    source = root / SINK_FILE
    if not source.is_file():
        return [f'{SINK_FILE}: the reviewed step sink is missing; restore it before gating progress']
    errors = []
    seen = collections.Counter()
    text = source.read_text()
    for number, line in enumerate(code_only(text).splitlines(), 1):
        stripped = line.strip()
        names_indicatif = 'indicatif' in stripped
        plain_import = stripped.startswith('use indicatif::')
        if names_indicatif and not plain_import and stripped not in SINK_INDICATIF_LINES:
            errors.append(
                f'{SINK_FILE}:{number}: {source_line(text, number)}; {SPINNER_MIGRATION} '
                '(the sink may not re-export the spinner crate)'
            )
            continue
        if not any(term in stripped for term in SINK_BUILD_TERMS):
            continue
        if stripped in SINK_LINES:
            seen[stripped] += 1
            if seen[stripped] > SINK_LINES.count(stripped):
                errors.append(
                    f'{SINK_FILE}:{number}: {source_line(text, number)}; {SPINNER_MIGRATION} '
                    '(the reviewed construction appears once)'
                )
            continue
        errors.append(f'{SINK_FILE}:{number}: {source_line(text, number)}; {SPINNER_MIGRATION}')
    return errors


SUPPRESSION = re.compile(
    r'#\s*!?\s*\[[^]]*\b(?:allow|expect)\s*\([^]]*'
    r'\b(?:deprecated|warnings|disallowed_methods|all)\b[^]]*\]'
)


def suppression_errors(root=None):
    """Find real suppression attributes, not comments or literal contents.

    The scan reads `code_only` output, so a comment or a doc/message string
    that mentions `#[allow(deprecated)]` is not a suppression, while the
    attribute in code is still found at its own line.
    """
    root = root or ROOT
    errors = []
    for source in source_files(root, 'src', 'crates', 'tests'):
        text = source.read_text()
        masked = code_only(text)
        for match in SUPPRESSION.finditer(masked):
            line = masked.count('\n', 0, match.start()) + 1
            errors.append(
                f'{source.relative_to(root).as_posix()}:{line}: '
                f'{source_line(text, line)}; do not suppress feedback diagnostics; '
                'migrate to a live progress source'
            )
    return errors


def run():
    env = os.environ.copy()
    env['CLIPPY_CONF_DIR'] = str(ROOT / 'config/feedback-lint')
    # Collect the inventory even if an unrelated Clippy lint fails; the
    # separate lint:feedback Clippy step still enforces it without this flag.
    env['RUSTFLAGS'] = env.get('RUSTFLAGS', '') + ' -A clippy::never_loop'
    command = clippy_command()
    proc = subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, text=True)
    messages = []
    for line in proc.stdout:
        try:
            messages.append(json.loads(line))
        except json.JSONDecodeError:
            print(line, end='', file=sys.stderr)
    code = proc.wait()
    if code:
        for item in messages:
            diagnostic = item.get('message', {})
            if item.get('reason') == 'compiler-message' and diagnostic.get('level') == 'error':
                print(diagnostic.get('rendered') or diagnostic.get('message'), file=sys.stderr)
        print(f'feedback compiler check failed ({code})', file=sys.stderr)
        return code
    observed = entries(messages)
    failures = (
        suppression_errors(ROOT)
        + indicatif_errors(ROOT)
        + sink_errors(ROOT)
        + check(observed)
    )
    for failure in failures:
        print(failure, file=sys.stderr)
    if not failures:
        print('feedback spinner inventory: no fixed-label spinner entry point in production')
    return bool(failures)


if __name__ == '__main__':
    sys.exit(run())

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
"""
import json
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
MARKER = 'feedback legacy spinner: migrate to a live progress source'
# The one reviewed spinner sink: `Step::start_bar` builds the only
# `ProgressBar::new_spinner` in the workspace. `config/feedback-lint/clippy.toml`
# disallows every other one.
SINK = ('crates/effigy-ui/src/steps.rs',
        'let bar = self.multi().add(ProgressBar::new_spinner());', 36)

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


def suppression_errors():
    errors = []
    pattern = re.compile(r'#\s*!?\s*\[[^]]*\b(?:allow|expect)\s*\([^]]*\b(?:deprecated|warnings|disallowed_methods|all)\b[^]]*\]', re.S)
    for root in (ROOT / 'src', ROOT / 'crates'):
        for source in root.rglob('*.rs'):
            text = source.read_text()
            for match in pattern.finditer(text):
                line = text.count('\n', 0, match.start()) + 1
                errors.append(f'{source.relative_to(ROOT)}:{line}: do not suppress feedback diagnostics; migrate to a live progress source')
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
    failures = suppression_errors() + check(observed)
    for failure in failures:
        print(failure, file=sys.stderr)
    if not failures:
        print('feedback spinner inventory: no fixed-label spinner entry point in production')
    return bool(failures)


if __name__ == '__main__':
    sys.exit(run())

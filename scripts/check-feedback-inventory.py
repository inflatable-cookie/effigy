#!/usr/bin/env python3
"""Ratchet compiler-resolved uses of legacy spinner entry points.

The dedicated cfg deprecates the old constructors and their existing wrappers
only during this check. Rust resolves aliases and function references before
emitting diagnostics, so renaming a helper does not hide a use.
"""
import collections
import json
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE = ROOT / 'config/feedback-spinner-baseline.json'
CEILING = ROOT / 'config/feedback-spinner-ceiling.json'
MARKER = 'feedback legacy spinner: migrate to a live progress source'


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
    # Only the unique infrastructure sink is excluded. A duplicate makes
    # every matching diagnostic visible to the ratchet.
    sink = ('crates/effigy-ui/src/steps.rs',
            'let bar = self.multi().add(ProgressBar::new_spinner());', 36)
    matching = [entry for entry in result if (entry[0], entry[1], entry[3]) == sink]
    if len(matching) == 1:
        result.remove(matching[0])
    return result


def key(entry):
    return (entry[0], entry[2], entry[1], entry[3])


def check(observed, baseline, ceiling):
    allowed = collections.Counter(map(tuple, baseline))
    cap = collections.Counter(map(tuple, ceiling))
    excess = allowed - cap
    failures = []
    if excess:
        failures.append('feedback spinner baseline grew beyond the reviewed ceiling')
    remaining = allowed.copy()
    for entry in observed:
        item = key(entry)
        if remaining[item]:
            remaining[item] -= 1
        else:
            failures.append(f'{entry[0]}:{entry[2]}: legacy spinner; migrate to a live progress source')
    if +remaining:
        failures.append('feedback spinner baseline contains removed sites; remove their entries')
    return failures


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
    # Collect the inventory even if this unrelated Clippy lint fails; the
    # separate lint:feedback Clippy step still enforces it without this flag.
    env['RUSTFLAGS'] = env.get('RUSTFLAGS', '') + ' --cfg effigy_feedback_lint --check-cfg=cfg(effigy_feedback_lint) -A clippy::never_loop'
    command = ['sh', 'scripts/oxide-selector.sh', 'clippy', '-p', 'effigy', '-p', 'effigy-cli', '-p', 'effigy-ui', '-p', 'effigy-kernel', '-p', 'effigy-containers', '-p', 'effigy-runtime', '-p', 'effigy-doctor', '-p', 'effigy-manifest', '-p', 'effigy-release', '-p', 'effigy-secrets', '--all-targets', '--message-format=json', '--', '-W', 'deprecated', '-W', 'clippy::disallowed_methods']
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
    failures = suppression_errors() + check(observed, json.loads(BASELINE.read_text()), json.loads(CEILING.read_text()))
    for failure in failures:
        print(failure, file=sys.stderr)
    if not failures:
        print(f'feedback spinner inventory: {len(observed)} legacy uses; no growth')
    return bool(failures)


if __name__ == '__main__':
    sys.exit(run())

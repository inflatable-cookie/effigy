#!/usr/bin/env python3
"""Assert suite fixture cleanup; sweep exact registered directories on timeout."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import signal
import stat
import subprocess
import sys
import tempfile


def unseal(root):
    info = root.lstat()
    if stat.S_ISLNK(info.st_mode):
        return
    root.chmod(stat.S_IMODE(info.st_mode) | (0o700 if root.is_dir() else 0o600))
    if root.is_dir():
        for child in root.iterdir():
            unseal(child)


@contextmanager
def owned_fixture(prefix, authenticated_checkout=None):
    # Contract 049 requires a nested selector's CWD under its parent token root.
    parent = Path(os.environ.get('EFFIGY_TEST_FIXTURE_ROOT', tempfile.gettempdir()))
    if authenticated_checkout is not None and 'HOST_RUN_TOKEN' in os.environ:
        parent = authenticated_checkout / 'crates'
        prefix = 'effigy-parent-fixtures-' + prefix
    with tempfile.TemporaryDirectory(prefix=prefix, dir=parent) as directory:
        root = Path(directory).resolve()
        record = None
        ledger = os.environ.get('EFFIGY_TEST_FIXTURE_LEDGER')
        if ledger:
            info = root.lstat()
            with tempfile.NamedTemporaryFile(mode='w', dir=Path(ledger) / 'directories',
                                             delete=False) as registration:
                json.dump(dict(path=str(root), device=info.st_dev, inode=info.st_ino), registration)
                record = Path(registration.name)
        try:
            yield root
        finally:
            unseal(root)
            shutil.rmtree(root)
            if record:
                record.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--timeout', type=int, default=840)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command[:1] == ['--']:
        command = command[1:]
    if not command:
        parser.error('a test runner command is required')
    checkout = Path(__file__).resolve().parents[2]
    before = set((checkout / 'crates').iterdir())
    with tempfile.TemporaryDirectory(prefix='effigy-fixture-suite-') as temporary:
        root = Path(temporary).resolve()
        (root / 'directories').mkdir()
        (root / 'fixtures').mkdir()
        (root / 'processes').mkdir()
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1',
                   EFFIGY_TEST_FIXTURE_ROOT=str(root / 'fixtures'),
                   EFFIGY_TEST_FIXTURE_LEDGER=str(root))
        if env.get('EFFIGY_TEST_PROCESS_HELPER'):
            env['EFFIGY_TEST_PROCESS_HELPER'] = str(Path(env['EFFIGY_TEST_PROCESS_HELPER']).resolve(strict=True))
        child = subprocess.Popen(command, env=env, start_new_session=True)
        try:
            result = child.wait(timeout=args.timeout)
        except subprocess.TimeoutExpired:
            # Only the exact session we created; never discover targets by name.
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
            result = 124
        leaked = False
        if any((root / 'processes').iterdir()):
            helper = env.get('EFFIGY_TEST_PROCESS_HELPER')
            if not helper:
                raise RuntimeError('process registrations require EFFIGY_TEST_PROCESS_HELPER')
            sweep = subprocess.run([helper, 'sweep', str(root)])
            leaked = sweep.returncode != 0
        for record in (root / 'directories').iterdir():
            registration = json.loads(record.read_text())
            fixture = Path(registration['path'])
            if not fixture.exists():
                continue
            info = fixture.lstat()
            if (info.st_dev, info.st_ino, info.st_uid) != (
                    registration['device'], registration['inode'], os.getuid()):
                print(f'fixture identity changed; refusing cleanup: {fixture}', file=sys.stderr)
                leaked = True
                continue
            print(f'leaked test fixture: {fixture}', file=sys.stderr)
            leaked = True
            unseal(fixture)
            shutil.rmtree(fixture)
        new_entries = set((checkout / 'crates').iterdir()) - before
        if new_entries:
            print(f'new entries under crates/: {sorted(map(str, new_entries))}', file=sys.stderr)
            leaked = True
        if any((root / 'fixtures').iterdir()):
            print('unregistered files left under suite fixture root', file=sys.stderr)
            leaked = True
            unseal(root / 'fixtures')
        return result if result else int(leaked)


if __name__ == '__main__':
    sys.exit(main())

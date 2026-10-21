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
def owned_fixture(prefix, shared_workspace=False):
    parent = Path(os.environ.get('EFFIGY_TEST_FIXTURE_ROOT', tempfile.gettempdir()))
    if shared_workspace and sys.platform == 'darwin':
        parent = Path('/Users/Shared')
    with tempfile.TemporaryDirectory(prefix=prefix, dir=parent) as directory:
        root = Path(directory).resolve()
        record = None
        ledger = os.environ.get('EFFIGY_TEST_FIXTURE_LEDGER')
        if ledger:
            info = root.lstat()
            with tempfile.NamedTemporaryFile(mode='w', dir=ledger,
                                             delete=False) as registration:
                json.dump(dict(path=str(root), device=info.st_dev, inode=info.st_ino), registration)
                pending = Path(registration.name)
            record = Path(ledger) / 'directories' / pending.name
            pending.rename(record)
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
    before = set((checkout / 'crates').rglob('*'))
    root = Path(tempfile.mkdtemp(prefix='effigy-fixture-suite-')).resolve()
    try:
        (root / 'directories').mkdir()
        (root / 'fixtures').mkdir()
        (root / 'processes').mkdir()
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1',
                   EFFIGY_TEST_FIXTURE_ROOT=str(root / 'fixtures'),
                   EFFIGY_TEST_FIXTURE_LEDGER=str(root))
        # These belong to the outer command's launch, not fixture ProcessSpecs.
        # Fixture children receive fresh nonces from their own custody owner.
        env.pop('EFFIGY_CUSTODY_NONCE', None)
        env.pop('EFFIGY_PROBE_NONCE', None)
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
            if any((root / 'processes').iterdir()):
                # Unknown identity never permits deleting a live owner's files.
                # Preserve the exact ledger and fixture paths for diagnosis.
                return result or 2
        for record in (root / 'directories').iterdir():
            registration = json.loads(record.read_text())
            fixture = Path(registration['path'])
            if not fixture.exists():
                record.unlink()
                continue
            info = fixture.lstat()
            if (info.st_dev, info.st_ino, info.st_uid) != (
                    registration['device'], registration['inode'], os.getuid()):
                print(f'fixture identity changed; refusing cleanup: {fixture}', file=sys.stderr)
                leaked = True
                continue
            print(f'leaked test fixture: {fixture}', file=sys.stderr)
            leaked = True
            try:
                (fixture / '.effigy/state/kernel.db').lstat()
                has_kernel_state = True
            except FileNotFoundError:
                has_kernel_state = False
            except OSError as error:
                print(f'kernel fixture state unavailable: {fixture}: {error}', file=sys.stderr)
                continue
            if has_kernel_state:
                helper = env.get('EFFIGY_TEST_PROCESS_HELPER')
                if not helper or subprocess.run([helper, 'wait-root', str(fixture)]).returncode:
                    print(f'unresolved kernel fixture writers: {fixture}', file=sys.stderr)
                    continue
            unseal(fixture)
            shutil.rmtree(fixture)
            record.unlink()
        if any((root / 'directories').iterdir()):
            return result or 2
        new_entries = set((checkout / 'crates').rglob('*')) - before
        if new_entries:
            print(f'new entries under crates/: {sorted(map(str, new_entries))}', file=sys.stderr)
            leaked = True
        if any((root / 'fixtures').iterdir()):
            print('unregistered files left under suite fixture root', file=sys.stderr)
            leaked = True
            unseal(root / 'fixtures')
        return result if result else int(leaked)
    finally:
        processes = root / 'processes'
        directories = root / 'directories'
        if ((processes.exists() and any(processes.iterdir())) or
                (directories.exists() and any(directories.iterdir()))):
            print(f'unresolved fixture ownership; retained ledger: {root}', file=sys.stderr)
        else:
            unseal(root)
            shutil.rmtree(root)


if __name__ == '__main__':
    sys.exit(main())

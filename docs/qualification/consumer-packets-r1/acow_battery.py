#!/usr/bin/env python3
"""Test-only harness for the historical Acow boundary battery (effigy#757).

The historical packet `acow-ordinary-compose-5f466914/` is preserved byte for
byte. Its `boundaries.py` battery runs unchanged, through its own `main()`,
which first verifies the committed packet and helper hashes. The one thing
this harness changes is the disposable fixture scratch allocation.

`common.SCRATCH` is `/Volumes/Builds/scratch/effigy/540`, a volume that is not
mounted on every checkout. The battery only uses it for fixture roots and
receipt files. Here it is repointed, in memory and for this process only, to a
fresh 0700 directory under the run's TMPDIR. The override is restored on exit,
including on error. Verification functions, expected results, hashes,
authority tokens, path-shape strings and native-route defaults are untouched.

Refuses without TMPDIR. Never falls back to /tmp or the internal drive.
"""
import contextlib
import os
import sys
import tempfile
from pathlib import Path

HISTORICAL = Path(__file__).resolve().parents[1] / 'acow-ordinary-compose-5f466914'
HISTORICAL_SCRATCH = Path('/Volumes/Builds/scratch/effigy/540')


def tmp_root_from_env(environ):
    """The run TMPDIR, resolved, or a refusal. No other root is ever chosen."""
    value = environ.get('TMPDIR', '')
    if not value or not os.path.isabs(value):
        raise RuntimeError('TMPDIR must name an absolute run scratch directory')
    if os.path.islink(value) or not os.path.isdir(value):
        raise RuntimeError('TMPDIR is not a real directory: ' + value)
    return os.path.realpath(value)


def fresh_scratch(tmp_root):
    """A new private directory under TMPDIR. Refuses anything not under it."""
    fresh = Path(tempfile.mkdtemp(prefix='acow-540-boundaries-', dir=tmp_root))
    os.chmod(fresh, 0o700)
    if not str(fresh.resolve()).startswith(tmp_root + os.sep):
        raise RuntimeError('fresh scratch escaped TMPDIR: ' + str(fresh))
    return fresh


@contextlib.contextmanager
def relocated_scratch(modules, tmp_root):
    """Repoint each module's SCRATCH to a fresh directory; restore on exit.

    Only modules whose SCRATCH is the same original object are repointed. On
    exit the originals are put back, and the fresh directory is removed only if
    it is empty. Any evidence it holds stays for the run's records.
    """
    originals = {}
    for module in modules:
        if hasattr(module, 'SCRATCH'):
            originals[module] = module.SCRATCH
    if not originals:
        raise RuntimeError('no SCRATCH binding found in the historical packet')
    fresh = fresh_scratch(tmp_root)
    try:
        for module in originals:
            module.SCRATCH = fresh
        yield fresh
    finally:
        for module, original in originals.items():
            module.SCRATCH = original
        try:
            fresh.rmdir()
        except OSError:
            pass


def run_battery(environ=os.environ, out=sys.stdout):
    tmp_root = tmp_root_from_env(environ)
    sys.path.insert(0, str(HISTORICAL))
    import common  # noqa: E402  (the unchanged historical packet)
    import primary  # noqa: E402
    import warmup  # noqa: E402
    import boundaries  # noqa: E402
    modules = (common, primary, warmup, boundaries)
    with relocated_scratch(modules, tmp_root) as fresh:
        print('acow boundary scratch relocated under TMPDIR: %s' % fresh, file=out)
        status = boundaries.main()
    restored = common.SCRATCH == HISTORICAL_SCRATCH
    print('acow boundary scratch restored to historical value: %s' % restored, file=out)
    return status if restored else 1


def main():
    try:
        return run_battery()
    except RuntimeError as exc:
        print('refused: %s' % exc, file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())

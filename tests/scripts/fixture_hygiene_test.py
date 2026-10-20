"""Suite cleanup retains fixtures when directory or kernel ownership is unknown."""
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import fixture_hygiene


class SuiteOwnership(unittest.TestCase):
    def test_unregistered_nested_crate_file_fails_the_suite(self):
        with tempfile.TemporaryDirectory(prefix='effigy-wrapper-crates-') as owner:
            checkout = Path(owner) / 'checkout'
            script = checkout / 'tests/scripts/fixture_hygiene.py'
            script.parent.mkdir(parents=True)
            shutil.copyfile(fixture_hygiene.__file__, script)
            crate = checkout / 'crates/existing'
            crate.mkdir(parents=True)
            baseline = crate / 'preserve'
            baseline.write_text('pre-existing')
            leaked = crate / 'leaked'
            result = subprocess.run(
                [sys.executable, str(script), '--', sys.executable, '-c',
                 'from pathlib import Path; import sys; Path(sys.argv[1]).write_text("leak")',
                 str(leaked)], env=dict(os.environ, TMPDIR=owner, TMP=owner, TEMP=owner),
                capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 1, result)
            self.assertIn('new entries under crates/', result.stderr)
            self.assertIn(str(leaked), result.stderr)
            self.assertEqual(baseline.read_text(), 'pre-existing')
            self.assertEqual(leaked.read_text(), 'leak')

    def test_unknown_ownership_is_not_deleted_by_final_cleanup(self):
        child = r"""
import json, os
from pathlib import Path
root = Path(os.environ['EFFIGY_TEST_FIXTURE_LEDGER'])
fixture = Path(os.environ['EFFIGY_TEST_FIXTURE_ROOT']) / 'replaced'
fixture.mkdir()
(fixture / 'preserve').write_text('preserved')
mode = os.environ['FIXTURE_CASE']
if mode == 'kernel':
    state = fixture / '.effigy/state'
    state.mkdir(parents=True, mode=0o700)
    (state / 'kernel.db').write_text('deliberately corrupt private fixture database')
    (state / 'kernel.db').chmod(0o600)
info = fixture.stat()
(root / 'directories' / 'replaced').write_text(json.dumps(dict(
    path=str(fixture), device=info.st_dev, inode=info.st_ino + (mode == 'directory'))))
print(root)
"""
        for mode, message in [('directory', 'refusing cleanup'),
                              ('kernel', 'unresolved kernel fixture writers')]:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(
                    prefix='effigy-wrapper-proof-') as owner:
                try:
                    env = dict(os.environ, TMPDIR=owner, TMP=owner, TEMP=owner,
                               FIXTURE_CASE=mode)
                    result = subprocess.run(
                        [sys.executable, str(Path(fixture_hygiene.__file__)), '--',
                         sys.executable, '-c', child], env=env,
                        capture_output=True, text=True, timeout=45)
                    self.assertEqual(result.returncode, 2, result)
                    root = Path(result.stdout.strip())
                    self.assertEqual(root.parent, Path(owner).resolve())
                    self.assertIn(message, result.stderr)
                    if mode == 'kernel':
                        self.assertIn('fixture runs:', result.stderr)
                    self.assertIn('retained ledger', result.stderr)
                    self.assertEqual((root / 'fixtures/replaced/preserve').read_text(), 'preserved')
                finally:
                    # Only our exact fresh proof directory, even if the child failed.
                    fixture_hygiene.unseal(Path(owner))


if __name__ == '__main__':
    unittest.main()

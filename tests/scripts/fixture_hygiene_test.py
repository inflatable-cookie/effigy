"""Suite cleanup must retain a directory whose registered identity changed."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import fixture_hygiene


class SuiteOwnership(unittest.TestCase):
    def test_unknown_directory_is_not_deleted_by_final_cleanup(self):
        child = r"""
import json, os
from pathlib import Path
root = Path(os.environ['EFFIGY_TEST_FIXTURE_LEDGER'])
fixture = Path(os.environ['EFFIGY_TEST_FIXTURE_ROOT']) / 'replaced'
fixture.mkdir()
(fixture / 'preserve').write_text('preserved')
info = fixture.stat()
(root / 'directories' / 'replaced').write_text(json.dumps(dict(
    path=str(fixture), device=info.st_dev, inode=info.st_ino + 1)))
print(root)
"""
        with tempfile.TemporaryDirectory(prefix='effigy-wrapper-proof-') as owner:
            try:
                env = dict(os.environ, TMPDIR=owner, TMP=owner, TEMP=owner)
                result = subprocess.run(
                    [sys.executable, str(Path(fixture_hygiene.__file__)), '--',
                     sys.executable, '-c', child], env=env,
                    capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 2, result)
                root = Path(result.stdout.strip())
                self.assertEqual(root.parent, Path(owner).resolve())
                self.assertIn('refusing cleanup', result.stderr)
                self.assertIn('retained ledger', result.stderr)
                self.assertEqual((root / 'fixtures/replaced/preserve').read_text(), 'preserved')
            finally:
                # Only our exact fresh proof directory, even if the child failed.
                fixture_hygiene.unseal(Path(owner))


if __name__ == '__main__':
    unittest.main()

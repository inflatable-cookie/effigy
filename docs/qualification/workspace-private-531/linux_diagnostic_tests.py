"""Local fixture tests for diagnostic inventory; no VM commands."""
import tempfile
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_diagnostic as diagnostic
from preparation_stage import Refusal


class InventoryTests(unittest.TestCase):
    def test_all_extents_and_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            for name in ('a.hds', 'b.hds'):
                (root / name).write_bytes(b'a')
            xml = b'<Disk><StorageData><Storage><Image><File>a.hds</File></Image><Image><File>b.hds</File></Image></Storage></StorageData></Disk>'
            before = diagnostic.inventory(root, xml)
            (root / 'b.hds').write_bytes(b'abcdef')
            after = diagnostic.inventory(root, xml)
            self.assertEqual(diagnostic.extent_growth(before, after)['logical'], 5)
            (root / 'b.hds').unlink()
            (root / 'b.hds').mkdir()
            with self.assertRaises(Refusal):
                diagnostic.inventory(root, xml)

    def test_independent_retention(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'source').write_bytes(b'original')
            facts, errors = diagnostic.retain_originals(root, {'missing': root / 'missing', 'config': root / 'source'}, 'before')
            self.assertEqual((root / 'before-config').read_bytes(), b'original')
            self.assertEqual(set(facts), {'config'})
            self.assertEqual(len(errors), 1)


if __name__ == '__main__':
    unittest.main()

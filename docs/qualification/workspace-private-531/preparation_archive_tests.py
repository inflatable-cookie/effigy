"""Injected, local-only checks; no guest or network actions."""
import hashlib
import io
import os
from pathlib import Path
import tarfile
import tempfile
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import preparation_archive as subject


class ArchiveChecks(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='archive-controls-'))
        self.archive = self.root / 'input.tar'
        self.manifest = {'files': [{'path': 'dir/file', 'kind': 'file',
                                    'bytes': 4, 'mode': '0o600',
                                    'sha256': hashlib.sha256(b'data').hexdigest()}]}

    def make_archive(self, entries):
        with tarfile.open(self.archive, 'w') as tar:
            for name, content in entries:
                info = tarfile.TarInfo(name)
                info.mode = 0o600
                info.size = len(content)
                tar.addfile(info, io.BytesIO(content))
        return hashlib.sha256(self.archive.read_bytes()).hexdigest(), self.archive.stat().st_size

    def verify(self, entries):
        h, size = self.make_archive(entries)
        subject.verify_archive(self.archive, h, size, self.manifest)

    def test_valid_archive_and_tree(self):
        self.verify([('dir/file', b'data')])
        directory = self.root / 'tree' / 'dir'
        directory.mkdir(parents=True)
        (directory / 'file').write_bytes(b'data')
        os.chmod(directory / 'file', 0o600)
        subject.verify_tree(directory.parent, self.manifest)
        (directory / 'file').write_bytes(b'evil')
        with self.assertRaises(subject.Refusal):
            subject.verify_tree(directory.parent, self.manifest)

    def test_escape_collision_and_hash_mismatch_refuse(self):
        for entries in [[('../escape', b'data')], [('dir/file', b'data'), ('dir/file', b'data')]]:
            with self.subTest(entries=entries), self.assertRaises(subject.Refusal):
                self.verify(entries)
        _, size = self.make_archive([('dir/file', b'data')])
        with self.assertRaises(subject.Refusal):
            subject.verify_archive(self.archive, '0' * 64, size, self.manifest)

    def test_symlink_refused(self):
        directory = self.root / 'tree'
        directory.mkdir()
        (directory / 'dir').symlink_to(self.root)
        with self.assertRaises(subject.Refusal):
            subject.verify_tree(directory, self.manifest)


if __name__ == '__main__':
    unittest.main()

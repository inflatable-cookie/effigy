"""Pure archive and extracted-tree checks for the private Linux preparation.

These checks do not extract files or authorize transport. Callers must bind their
inputs and filesystem identity again at placement; a successful check is not a
qualification receipt.
"""
import hashlib
import os
from pathlib import Path, PurePosixPath
import stat
import tarfile


class Refusal(ValueError):
    pass


def digest(stream):
    h = hashlib.sha256()
    for block in iter(lambda: stream.read(1024 * 1024), b''):
        h.update(block)
    return h.hexdigest()


def path_name(name):
    path = PurePosixPath(name)
    if (not name or name.startswith('/') or '\\' in name or '\x00' in name
            or any(part in ('', '.', '..') for part in name.split('/'))
            or str(path) != name):
        raise Refusal('unsafe archive path: ' + repr(name))
    return name


def expected_files(manifest):
    files = {}
    for item in manifest['files']:
        name = path_name(item['path'])
        if name in files or item['kind'] != 'file':
            raise Refusal('duplicate or unsupported manifest entry: ' + name)
        files[name] = (item['bytes'], int(item['mode'], 8), item['sha256'])
    return files


def verify_archive(archive, expected_hash, expected_size, manifest):
    files = expected_files(manifest)
    with open(archive, 'rb') as source:
        if os.fstat(source.fileno()).st_size != expected_size or digest(source) != expected_hash:
            raise Refusal('archive size or hash mismatch')
        source.seek(0)
        with tarfile.open(fileobj=source, mode='r:') as tar:
            seen, actual = set(), {}
            for member in tar:
                name = path_name(member.name)
                if name in seen:
                    raise Refusal('archive path collision: ' + name)
                seen.add(name)
                parents = list(PurePosixPath(name).parents)[:-1]
                if any(str(parent) in actual for parent in parents):
                    raise Refusal('archive parent is a file: ' + name)
                if member.isdir():
                    continue
                if any(existing.startswith(name + '/') for existing in seen if existing != name):
                    raise Refusal('archive file shadows a path: ' + name)
                if not member.isfile():
                    raise Refusal('unsupported archive entry: ' + name)
                stream = tar.extractfile(member)
                if stream is None:
                    raise Refusal('unreadable archive entry: ' + name)
                actual[name] = (member.size, member.mode, digest(stream))
            if actual != files:
                raise Refusal('archive differs from manifest')


def verify_tree(root, manifest):
    expected = expected_files(manifest)
    root = Path(root)
    if not stat.S_ISDIR(root.lstat().st_mode):
        raise Refusal('tree root is not a directory')
    actual = {}
    for directory, dirs, names in os.walk(root, followlinks=False):
        for name in dirs + names:
            path = Path(directory) / name
            rel = path.relative_to(root).as_posix()
            path_name(rel)
            info = path.lstat()
            if stat.S_ISDIR(info.st_mode):
                continue
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                raise Refusal('unsupported tree entry: ' + rel)
            with path.open('rb') as source:
                actual[rel] = (info.st_size, stat.S_IMODE(info.st_mode), digest(source))
    if actual != expected:
        raise Refusal('tree differs from manifest')

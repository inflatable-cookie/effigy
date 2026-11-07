"""Bounded offline admission of the pinned nextest archive; no download."""
import hashlib
import io
import os
from pathlib import Path
import tarfile

PIN = '0ad2815fd91a7ecec3a7e25c749b584f66729ac688fd26b2fd494f7ff94b7fd0'


class Refusal(ValueError):
    pass


def _admit(data, expected_hash):
    if len(data) > 20 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != expected_hash:
        raise Refusal('nextest archive hash or size mismatch')
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
        members = tar.getmembers()
        if len(members) != 1 or members[0].name != 'cargo-nextest' or not members[0].isfile() or not 20 <= members[0].size <= 100 * 1024 * 1024:
            raise Refusal('unexpected nextest archive content')
        payload = tar.extractfile(members[0]).read()
    if len(payload) < 20 or payload[:6] != b'\x7fELF\x02\x01' or int.from_bytes(payload[18:20], 'little') != 183:
        raise Refusal('nextest is not little-endian ELF64 aarch64')
    return payload


def install_prepositioned(archive, directory, uid):
    """Caller must separately admit the stage, filesystem and source bytes."""
    directory = Path(directory)
    info = directory.lstat()
    if directory.is_symlink() or not directory.is_dir() or directory.resolve() != directory or info.st_uid != uid or info.st_mode & 0o777 != 0o700:
        raise Refusal('nextest directory must be canonical and private')
    with open(archive, 'rb') as source:
        data = source.read(20 * 1024 * 1024 + 1)
    payload = _admit(data, PIN)
    fd = os.open(directory / 'cargo-nextest', os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o500)
    with os.fdopen(fd, 'wb') as output:
        output.write(payload)
        output.flush()
        os.fsync(output.fileno())
    return {'archive_sha256': PIN, 'binary_sha256': hashlib.sha256(payload).hexdigest(),
            'archive_bytes': len(data), 'binary_bytes': len(payload), 'version': '0.9.145'}

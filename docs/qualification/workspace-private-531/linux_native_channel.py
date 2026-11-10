"""Fixed receive/read SSH entry for the owned native Linux preparation path.

The requested SSH command is an opaque root-provisioned operation ID. It is
never executed as a command. This file is installed root-owned and uses only
stdlib; the account cannot provision requests or choose a shell/argv.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import time

BASE = Path('/Volumes/Stash/scratch/effigy/linux-native-829')
REQUESTS = Path('/var/lib/effigy-829/ssh-requests')
ENTRY = '/usr/local/libexec/effigy829-native-channel'
SCHEMA = 'effigy.linux-native-channel.v1'
HEX = re.compile(r'[0-9a-f]{64}')
MAX_BYTES = 1024 ** 3


def require(condition, message):
    if not condition:
        raise ValueError(message)


def request_path(identifier):
    require(isinstance(identifier, str) and HEX.fullmatch(identifier), 'one opaque operation ID required')
    return REQUESTS / (identifier + '.json')


def parent_fd(path):
    require(isinstance(path, str) and str(Path(path)) == path and '..' not in Path(path).parts
            and Path(path).is_relative_to(BASE) and Path(path) != BASE, 'operation path is outside the owned tree')
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in Path(path).parent.parts[1:]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
        return fd
    except BaseException:
        os.close(fd)
        raise


def read_request(identifier):
    path = request_path(identifier)
    require(REQUESTS.resolve(strict=True) == REQUESTS and REQUESTS.stat().st_uid == 0
            and stat.S_IMODE(REQUESTS.stat().st_mode) == 0o755, 'request directory custody differs')
    for parent in REQUESTS.parents:
        info = parent.lstat()
        require(info.st_uid == 0 and stat.S_ISDIR(info.st_mode) and not info.st_mode & 0o022,
                'request ancestor custody differs')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        require(info.st_mode == 0o100444 and info.st_uid == 0 and info.st_nlink == 1
                and info.st_size <= 8192, 'request file custody differs')
        data = os.read(fd, 8193)
        require(len(data) == info.st_size, 'request read was incomplete')
    finally:
        os.close(fd)
    value = json.loads(data)
    require(set(value) == {'schema', 'operation', 'path', 'sha256', 'limit_bytes', 'size_bytes',
                          'parent_device', 'parent_inode', 'boot_id', 'expires_at'}, 'request schema differs')
    require(value['schema'] == SCHEMA and value['operation'] in ('receive', 'read')
            and type(value['limit_bytes']) is int and 0 <= value['limit_bytes'] <= MAX_BYTES
            and type(value['size_bytes']) is int and 0 <= value['size_bytes'] <= value['limit_bytes']
            and isinstance(value['sha256'], str) and HEX.fullmatch(value['sha256']), 'request bounds differ')
    require(value['boot_id'] == Path('/proc/sys/kernel/random/boot_id').read_text().strip()
            and type(value['expires_at']) is int and time.time() < value['expires_at'] <= time.time() + 86400,
            'request expired or belongs to another boot')
    return value


def provision(identifier, encoded):
    require(os.geteuid() == 0, 'only the host authority may provision an operation')
    request_path(identifier)
    data = base64.b64decode(encoded, validate=True)
    require(len(data) <= 4096, 'operation request exceeds its cap')
    value = json.loads(data)
    require(set(value) == {'operation', 'path', 'sha256', 'size_bytes', 'limit_bytes'}, 'provision fields differ')
    require(value['operation'] in ('receive', 'read') and type(value['limit_bytes']) is int
            and 0 <= value['limit_bytes'] <= MAX_BYTES, 'unsupported operation')
    require(BASE.stat().st_dev != os.stat('/').st_dev, 'preparation image is not mounted')
    fd = parent_fd(value['path'])
    try:
        info = os.fstat(fd)
        require(info.st_dev in (BASE.stat().st_dev, (BASE / 'qualification').stat().st_dev)
                and info.st_uid == 1001, 'operation parent is not on an owned bounded filesystem')
        if value['operation'] == 'read':
            source = os.open(Path(value['path']).name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
            try:
                original = os.fstat(source)
                require(stat.S_ISREG(original.st_mode) and original.st_size <= value['limit_bytes'],
                        'read artifact exceeds its cap')
                digest = hashlib.sha256()
                while True:
                    block = os.read(source, 1048576)
                    if not block:
                        break
                    digest.update(block)
                require(tuple(getattr(os.fstat(source), key) for key in ('st_dev','st_ino','st_size','st_mtime_ns','st_ctime_ns'))
                        == tuple(getattr(original, key) for key in ('st_dev','st_ino','st_size','st_mtime_ns','st_ctime_ns')),
                        'read artifact changed during provision')
                value.update(sha256=digest.hexdigest(), size_bytes=original.st_size)
            finally:
                os.close(source)
        require(type(value['size_bytes']) is int and 0 <= value['size_bytes'] <= value['limit_bytes']
                and isinstance(value['sha256'], str) and HEX.fullmatch(value['sha256']), 'artifact identity is incomplete')
        value.update(schema=SCHEMA, parent_device=info.st_dev, parent_inode=info.st_ino,
                     boot_id=Path('/proc/sys/kernel/random/boot_id').read_text().strip(), expires_at=int(time.time()) + 3600)
    finally:
        os.close(fd)
    REQUESTS.mkdir(mode=0o755, exist_ok=True)
    require(REQUESTS.lstat().st_uid == 0 and REQUESTS.resolve() == REQUESTS, 'request directory changed')
    os.chmod(REQUESTS, 0o755)
    path = request_path(identifier)
    if path.exists():
        prior = read_request(identifier)
        require(all(prior[key] == value[key] for key in value if key != 'expires_at'), 'operation ID collision')
        return prior
    raw = json.dumps(value, sort_keys=True).encode()
    # A root-controlled directory and exclusive creation prevent replacement by
    # the account. Failed/incomplete provisioning never permits a dispatcher read.
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        with os.fdopen(fd, 'wb', closefd=False) as stream:
            stream.write(raw)
            stream.flush()
            os.fsync(fd)
        os.fchmod(fd, 0o444)
    finally:
        os.close(fd)
    return read_request(identifier)


def directory(path):
    require(os.geteuid() == 0, 'only the host authority creates directories')
    fd = parent_fd(path)
    try:
        info = os.fstat(fd)
        require(BASE.stat().st_dev != os.stat('/').st_dev
                and info.st_dev in (BASE.stat().st_dev, (BASE / 'qualification').stat().st_dev),
                'directory parent is not physically bounded')
        os.mkdir(Path(path).name, 0o700, dir_fd=fd)
        child = os.open(Path(path).name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
        try:
            os.fchown(child, 1001, 1001)
            os.fchmod(child, 0o700)
            os.fsync(child)
            made = os.fstat(child)
        finally:
            os.close(child)
        os.fsync(fd)
        return dict(path=path, device=made.st_dev, inode=made.st_ino, uid=made.st_uid)
    finally:
        os.close(fd)


def dispatch(identifier):
    require(os.geteuid() == 1001, 'dedicated non-root SSH account required')
    value = read_request(identifier)
    parent = parent_fd(value['path'])
    try:
        info = os.fstat(parent)
        require((info.st_dev, info.st_ino) == (value['parent_device'], value['parent_inode']), 'operation parent changed')
        flags = (os.O_WRONLY | os.O_CREAT | os.O_EXCL if value['operation'] == 'receive' else os.O_RDONLY)
        fd = os.open(Path(value['path']).name, flags | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        try:
            initial = os.fstat(fd)
            require(stat.S_ISREG(initial.st_mode) and initial.st_dev == info.st_dev, 'operation artifact differs')
            digest, count = hashlib.sha256(), 0
            while True:
                block = (sys.stdin.buffer.read(min(1048576, value['limit_bytes'] + 1 - count))
                         if value['operation'] == 'receive' else os.read(fd, 1048576))
                if not block:
                    break
                count += len(block)
                require(count <= value['limit_bytes'], 'operation exceeded its byte cap')
                digest.update(block)
                if value['operation'] == 'receive':
                    offset = 0
                    while offset < len(block):
                        written = os.write(fd, block[offset:])
                        require(written > 0, 'operation write was incomplete')
                        offset += written
                else:
                    sys.stdout.buffer.write(block)
            require(count == value['size_bytes'] and digest.hexdigest() == value['sha256'], 'operation bytes differ')
            if value['operation'] == 'receive':
                os.fsync(fd)
            final = os.fstat(fd)
            require((initial.st_dev, initial.st_ino) == (final.st_dev, final.st_ino)
                    and final.st_size == value['size_bytes'], 'operation inode changed')
        finally:
            os.close(fd)
    finally:
        os.close(parent)


if __name__ == '__main__':
    try:
        if len(sys.argv) == 4 and sys.argv[1] == '--provision':
            print(json.dumps(provision(sys.argv[2], sys.argv[3]), sort_keys=True))
        elif len(sys.argv) == 3 and sys.argv[1] == '--directory':
            print(json.dumps(directory(sys.argv[2]), sort_keys=True))
        elif sys.argv[1:] == ['--dispatch']:
            dispatch(os.environ.get('SSH_ORIGINAL_COMMAND'))
        else:
            raise ValueError('unsupported channel entry')
    except (ValueError, OSError, KeyError, TypeError) as error:
        print('native-channel refused: ' + str(error), file=sys.stderr)
        sys.exit(3)

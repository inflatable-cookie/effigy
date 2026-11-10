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
import subprocess
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
    fields = {'schema', 'operation', 'path', 'sha256', 'limit_bytes', 'size_bytes',
              'parent_device', 'parent_inode', 'boot_id', 'expires_at'}
    if value.get('operation') == 'run':
        fields |= {'unit', 'invocation_id', 'deadline_seconds', 'outputs'}
    require(set(value) == fields, 'request schema differs')
    require(value['schema'] == SCHEMA and value['operation'] in ('receive', 'read', 'run')
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
    fields = {'operation', 'path', 'sha256', 'size_bytes', 'limit_bytes'}
    if value.get('operation') == 'run':
        fields |= {'unit', 'deadline_seconds', 'stdout_path', 'stderr_path'}
    require(set(value) == fields, 'provision fields differ')
    require(value['operation'] in ('receive', 'read', 'run') and type(value['limit_bytes']) is int
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
        if value['operation'] == 'run':
            require(re.fullmatch(r'effigy-829-[0-9a-f]{32}\.service', value['unit'])
                    and type(value['deadline_seconds']) is int and 0 < value['deadline_seconds'] <= 2520,
                    'exact bounded session unit required')
            state = unit_state(value['unit'])
            require(int(state['MainPID']) > 0 and state['ActiveState'] == 'active'
                    and re.fullmatch(r'[0-9a-f]{32}', state['InvocationID']), 'session unit is not waiting live')
            value['invocation_id'] = state['InvocationID']
            value['outputs'] = {}
            for name in ('stdout', 'stderr'):
                output = value.pop(name + '_path')
                parent = parent_fd(output)
                try:
                    source = os.open(Path(output).name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=parent)
                    try:
                        item = os.fstat(source)
                        require(stat.S_ISREG(item.st_mode) and item.st_dev == info.st_dev,
                                'session output is outside its bounded gate filesystem')
                        value['outputs'][name] = dict(path=output, device=item.st_dev, inode=item.st_ino)
                    finally:
                        os.close(source)
                finally:
                    os.close(parent)
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


def unit_state(unit):
    raw = subprocess.check_output(['/usr/bin/systemctl', 'show', '--all', unit,
        '--property=MainPID,ActiveState,InvocationID,ExecMainStatus,ExecMainExitTimestampMonotonic'], text=True, timeout=10)
    return dict(line.split('=',1) for line in raw.splitlines())


def wait_unit(value):
    """Return only the bound main's waited result; cgroup closure is separate."""
    end = time.monotonic() + value['deadline_seconds']
    while True:
        state = unit_state(value['unit'])
        require(state['InvocationID'] == value['invocation_id'], 'session unit generation changed')
        if state['MainPID'] == '0' and int(state['ExecMainExitTimestampMonotonic'] or '0') > 0:
            break
        require(time.monotonic() < end, 'session main did not return within its bound')
        time.sleep(.2)
    for name in ('stdout','stderr'):
        item = value['outputs'][name]
        parent = parent_fd(item['path'])
        try:
            source = os.open(Path(item['path']).name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=parent)
            try:
                info = os.fstat(source)
                require((info.st_dev, info.st_ino) == (item['device'], item['inode'])
                        and info.st_size <= 8 * 1024 ** 2, 'session output identity or cap differs')
                data = bytearray()
                while len(data) <= 8 * 1024 ** 2:
                    block = os.read(source, min(1048576, 8 * 1024 ** 2 + 1 - len(data)))
                    if not block: break
                    data.extend(block)
                require(len(data) == info.st_size and os.fstat(source).st_mtime_ns == info.st_mtime_ns,
                        'session output changed after its main returned')
                getattr(sys, name).buffer.write(data)
            finally:
                os.close(source)
        finally:
            os.close(parent)
    return int(state['ExecMainStatus'])


def dispatch(identifier):
    require(os.geteuid() == 1001, 'dedicated non-root SSH account required')
    value = read_request(identifier)
    parent = parent_fd(value['path'])
    try:
        info = os.fstat(parent)
        require((info.st_dev, info.st_ino) == (value['parent_device'], value['parent_inode']), 'operation parent changed')
        flags = (os.O_RDONLY if value['operation'] == 'read' else os.O_WRONLY | os.O_CREAT | os.O_EXCL)
        final_name = Path(value['path']).name
        name = final_name
        if value['operation'] != 'read':
            try:
                os.stat(final_name,dir_fd=parent,follow_symlinks=False)
            except FileNotFoundError:
                pass
            else:
                raise ValueError('operation destination already exists')
            name = final_name + '.pending-' + identifier
        fd = os.open(name, flags | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        try:
            initial = os.fstat(fd)
            require(stat.S_ISREG(initial.st_mode) and initial.st_dev == info.st_dev, 'operation artifact differs')
            digest, count = hashlib.sha256(), 0
            while True:
                block = (sys.stdin.buffer.read(min(1048576, value['limit_bytes'] + 1 - count))
                         if value['operation'] != 'read' else os.read(fd, 1048576))
                if not block:
                    break
                count += len(block)
                require(count <= value['limit_bytes'], 'operation exceeded its byte cap')
                digest.update(block)
                if value['operation'] != 'read':
                    offset = 0
                    while offset < len(block):
                        written = os.write(fd, block[offset:])
                        require(written > 0, 'operation write was incomplete')
                        offset += written
                else:
                    sys.stdout.buffer.write(block)
            require(count == value['size_bytes'] and digest.hexdigest() == value['sha256'], 'operation bytes differ')
            if value['operation'] != 'read':
                os.fsync(fd)
                # Publish only complete hash-checked bytes. Consumers waiting on
                # a gate or acknowledgement never observe an empty/partial file.
                # The pending name remains as retained evidence of this receive.
                os.link(name,final_name,src_dir_fd=parent,dst_dir_fd=parent,follow_symlinks=False)
                os.fsync(parent)
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
            identifier = os.environ.get('SSH_ORIGINAL_COMMAND')
            dispatch(identifier)
            request = read_request(identifier)
            if request['operation'] == 'run':
                sys.exit(wait_unit(request))
        else:
            raise ValueError('unsupported channel entry')
    except (ValueError, OSError, KeyError, TypeError) as error:
        print('native-channel refused: ' + str(error), file=sys.stderr)
        sys.exit(3)

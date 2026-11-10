"""Reviewed full-session packet primitives. No engine action at import time."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess

BACKENDS = ('colima', 'desktop', 'linux')
# Canonical removable scratch for every future invocation; SSH_CONTROL follows it.
# The consumed 531 session ran against the retired /Volumes/Builds mount, so its
# receipts name that root as history. They are never re-read as this binding.
SCRATCH = Path('/Volumes/Stash/scratch/effigy')
SSH_CONTROL = SCRATCH / '531-linux-control'
PACKET = Path(__file__).resolve().parent
HEX = re.compile(r'[0-9a-f]{64}')


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path, *, dir_fd=None):
    h = hashlib.sha256()
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=dir_fd)
    with os.fdopen(fd, 'rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def load(path):
    return json.loads(Path(path).read_bytes())


def artifact(item):
    path = Path(item['path'])
    require(path.is_absolute() and not path.is_symlink() and path.is_file(), 'artifact unavailable')
    require(HEX.fullmatch(item['sha256']) and digest(path) == item['sha256'], 'artifact hash mismatch')
    return path


def exclusive(path, value, *, dir_fd=None):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=dir_fd)
    with os.fdopen(fd, 'w', encoding='utf-8') as out:
        json.dump(value, out, sort_keys=True)
        out.write('\n')
        out.flush()
        os.fsync(out.fileno())


def exclusive_bytes(path, data, *, dir_fd=None):
    # Exclusive fsynced retention of exact original bytes (never a re-encoding).
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=dir_fd)
    with os.fdopen(fd, 'wb') as out:
        out.write(data)
        out.flush()
        os.fsync(out.fileno())


def run(argv, *, cwd=None, env=None, timeout=30, check=True, data=None):
    # Diagnostics are fixed: subprocess output can include a guest credential.
    result = subprocess.run([str(x) for x in argv], cwd=cwd, env=env, input=data,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)
    require(not check or result.returncode == 0, 'required command failed: ' + Path(argv[0]).name)
    return result


def source_head(root):
    head = run(['git', 'rev-parse', 'HEAD'], cwd=root).stdout.decode().strip()
    require(not run(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=root).stdout,
            'proof requires unchanged tracked source and index')
    return head


def qualification_path(binding):
    require(isinstance(binding, dict) and set(binding) == {'path', 'device', 'inode', 'uid'},
            'qualification root must pin path, device, inode and owner')
    raw = binding['path']
    require(isinstance(raw, str), 'qualification root path required')
    path = Path(raw)
    require(path.is_absolute() and str(path) == raw and '..' not in path.parts
            and path.is_relative_to(SCRATCH) and path != SCRATCH,
            'qualification root must be an exact absolute child of removable scratch')
    require(all(type(binding[key]) is int and binding[key] >= 0 for key in ('device', 'inode', 'uid')),
            'actual qualification root identity required')
    return path


def verify_qualification_root(binding, fd):
    path = qualification_path(binding)
    try:
        require(SCRATCH.resolve(strict=True) == SCRATCH and path.resolve(strict=True) == path,
                'qualification root aliases are prohibited')
        held, visible = os.fstat(fd), path.lstat()
        expected = (binding['device'], binding['inode'], binding['uid'])
        require(all((value.st_dev, value.st_ino, value.st_uid) == expected
                    and stat.S_ISDIR(value.st_mode) and stat.S_IMODE(value.st_mode) == 0o700
                    for value in (held, visible)) and held.st_uid == os.geteuid(),
                'qualification root ownership or identity changed')
    except OSError as error:
        raise RuntimeError('qualification root unavailable') from error


def open_qualification_root(binding):
    path = qualification_path(binding)
    try:
        fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    except OSError as error:
        raise RuntimeError('qualification root unavailable') from error
    try:
        verify_qualification_root(binding, fd)
        return path, fd
    except BaseException:
        os.close(fd)
        raise


def ssh_prefix(cell):
    return isolated_ssh_prefix(cell['ssh_alias'], cell['ssh_config'], cell['ssh_known_hosts'],
                               SSH_CONTROL, qualification_path(cell['qualification_root']))


def isolated_ssh_prefix(alias, config_item, known_item, control_root, excluded_root):
    """One private SSH policy for sealed packets and separately authorized exploration."""
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]*', alias), 'exact SSH alias required')
    config = artifact(config_item)
    info = config.lstat()
    parent = control_root.lstat()
    require(config.is_relative_to(control_root) and config.resolve(strict=True) == config
            and not any(p.is_symlink() for p in config.parents)
            and stat.S_ISDIR(parent.st_mode) and stat.S_IMODE(parent.st_mode) == 0o700
            and parent.st_uid == os.geteuid()
            and info.st_uid == os.geteuid() and stat.S_IMODE(info.st_mode) == 0o600,
            'private canonical SSH config required')
    # No Include/Match/command hooks or wildcard hosts in this test-only config.
    values = {}
    for line in config.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        parts = line.split(None, 1)
        require(len(parts) == 2 and parts[0].lower() not in values, 'simple isolated SSH config required')
        values[parts[0].lower()] = parts[1]
    fixed = dict(host=alias, batchmode='yes', stricthostkeychecking='yes', identitiesonly='yes',
                 identityagent='none', globalknownhostsfile='/dev/null', updatehostkeys='no',
                 forwardagent='no', clearallforwardings='yes', requesttty='no',
                 permitlocalcommand='no', canonicalizehostname='no', proxycommand='none',
                 proxyjump='none', remotecommand='none', localcommand='none', connecttimeout='10')
    require(set(values) == set(fixed) | {'hostname', 'user', 'identityfile', 'userknownhostsfile'}
            and all(values.get(k) == v for k, v in fixed.items()), 'isolated SSH policy differs')
    import ipaddress
    try:
        ipaddress.ip_address(values['hostname'])
    except ValueError as error:
        raise RuntimeError('observed SSH endpoint required') from error
    require(values['user'] == 'effigy531', 'dedicated SSH user required')
    for key in ('identityfile', 'userknownhostsfile'):
        path = Path(values[key])
        require(path.is_absolute() and str(path) == values[key] and '..' not in path.parts
                and path.is_relative_to(control_root) and re.fullmatch(r'[A-Za-z0-9_./-]+', values[key])
                and path.resolve(strict=False) == path and not path.is_relative_to(
                    excluded_root), 'private unshared SSH path required')
    known = artifact(known_item)
    known_info = known.lstat()
    require(str(known) == values['userknownhostsfile'] and known_info.st_uid == os.geteuid()
            and stat.S_IMODE(known_info.st_mode) == 0o600, 'pinned private known-hosts required')
    return ['/usr/bin/ssh', '-F', str(config), '-o', 'BatchMode=yes', alias]


def validate_inputs(value, head):
    require(value['schema'] == 'effigy.workspace-private-proof.v1', 'unknown input schema')
    require(value['source_head'] == head, 'candidate is not the immutable proof head')
    require(tuple(cell['backend'] for cell in value['backends']) == BACKENDS,
            'all three native backends are required, in order')
    for cell in value['backends']:
        root = qualification_path(cell.get('qualification_root'))
        require(not root.is_relative_to(SSH_CONTROL) and not SSH_CONTROL.is_relative_to(root),
                'qualification root must exclude SSH control')
        require(cell['source_head'] == head, 'backend candidate source differs')
        require(cell['old_source_head'] != head and re.fullmatch(r'[0-9a-f]{40}', cell['old_source_head']),
                'genuine old source identity required')
        require(cell['old_source_receipt']['source_head'] == cell['old_source_head'], 'old receipt source mismatch')
        require(cell['candidate_receipt']['source_head'] == head, 'candidate build receipt source mismatch')
        require(cell['candidate_receipt']['features'] == ['workspace-private-qualification'],
                'native candidate must exclude external-I/O doubles and fault injection')
        require(len({cell['candidate']['sha256'], cell['production']['sha256'], cell['old_binary']['sha256']}) == 3,
                'distinct qualification, ordinary and genuine old binaries required')
        require(cell['candidate_receipt']['sha256'] == cell['candidate']['sha256'], 'candidate receipt mismatch')
        require(cell['old_source_receipt']['sha256'] == cell['old_binary']['sha256'], 'old receipt mismatch')
        require(cell['production_receipt']['source_head'] == head and cell['production_receipt']['features'] == []
                and cell['production_receipt']['sha256'] == cell['production']['sha256'], 'ordinary production build receipt mismatch')
        require(not cell.get('environment'), 'ambient environment overrides prohibited')
        require(cell['runtime_image'].startswith('sha256:') and HEX.fullmatch(cell['runtime_image'][7:]),
                'resident image must be pinned by ID')
        identity = cell.get('guest_install_identity')
        require(isinstance(identity, dict) and set(identity) == {'path', 'sha256'}
                and isinstance(identity['path'], str) and Path(identity['path']).is_absolute()
                and isinstance(identity['sha256'], str) and HEX.fullmatch(identity['sha256']),
                'pinned producer host-identity sidecar required')
        require(cell['budget_seconds'] > 0 and cell['budget_seconds'] <= 1800,
                'packet cannot extend the existing 1800 second test window')
        require(cell['driver'] in ('local', 'ssh'), 'unknown native driver')
        require((cell['backend'] == 'linux') == (cell['driver'] == 'ssh'),
                'Linux must execute natively in its separately approved Linux host')
        if cell['driver'] == 'ssh':
            import linux_capacity_contract as capacity
            capacity.verify_preparation(cell.get('linux_capacity'), head)
            pins = cell['linux_capacity']['plan']['phases']['activation']['images']
            native_root = Path(pins['preparation']['mountpoint'])
            workspace = Path(pins['workspace']['mountpoint'])
            require(Path(cell['source_root']).is_relative_to(native_root)
                    and Path(cell['qualification_root']['path']).is_relative_to(workspace)
                    and cell['native_engine_root'] == pins['engine']['mountpoint']
                    and cell['native_engine_capacity_bytes'] == pins['engine']['image_bytes']
                    and Path(cell['docker_config']).is_relative_to(Path(cell['native_engine_root'])),
                    'Linux inputs escape their prepared physical roles')
            require(all(isinstance(cell.get(key), dict) and set(cell[key]) == {'path', 'sha256'}
                        and HEX.fullmatch(cell[key]['sha256']) for key in ('ssh_config', 'ssh_known_hosts')),
                    'pinned isolated SSH config and host keys required')
        require(cell['engine_kind'] in ('colima', 'docker'), 'unknown engine kind')
        require((cell['backend'] == 'colima') == (cell['engine_kind'] == 'colima'), 'backend engine mismatch')
        require(not any(key.startswith(('EFFIGY_TEST', 'EFFIGY_WORKSPACE_ACTIVATION_FAKE'))
                        for key in cell.get('environment', {})), 'native doubles are prohibited')
    return value


def validate_go(go, inputs_hash, cell, head, *, effects='workspace-private-531-full-session-r1'):
    require(go['schema'] == 'effigy.workspace-private-native-go.v1', 'unknown GO schema')
    require(go['operator'] == 'Tom' and go['decision_id'] and go['kind'] == 'native', 'scoped native GO required')
    require(go['source_head'] == head and go['inputs_sha256'] == inputs_hash, 'GO candidate/input binding mismatch')
    require(go['backend'] == cell['backend'], 'GO belongs to another backend')
    require(go['budget_seconds'] == cell['budget_seconds'], 'GO budget mismatch')
    require(go['engine_identity'] == cell['engine_identity'], 'GO engine identity mismatch')
    require(go['effects'] == effects, 'GO does not cover this packet')
    # Expiry is checked on each machine immediately before effects.
    import datetime
    expires = datetime.datetime.fromisoformat(go['expires_at'].replace('Z', '+00:00'))
    require(expires.tzinfo is not None and expires > datetime.datetime.now(datetime.timezone.utc), 'GO expired')

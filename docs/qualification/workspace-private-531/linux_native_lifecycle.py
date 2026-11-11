"""Owned output disposal and capacity admission for unsealed native smokes only."""
import json
import io
import hashlib
import os
import re
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tarfile

BASE = Path('/Volumes/Stash/scratch/effigy/linux-native-829')
GIB = 1024 ** 3
CACHE_LIMIT = 8 * GIB
ENGINE_TMP = Path('/var/lib/effigy-829/rootful/client/tmp')
ACTIVE = None

def require(ok, message):
    if not ok:
        raise RuntimeError('guest-capacity: ' + message)

def allocated(path):
    size = int(subprocess.check_output(['du', '-sx', '-B1', '--', str(path)]).split()[0])
    return size - (path.stat().st_blocks * 512 if path == BASE / 'build-cache' else 0)

def references(paths):
    """Never evict an entry used by a resident process, including mapped binaries."""
    found = []
    for process in Path('/proc').glob('[0-9]*'):
        try:
            names = [str((process / 'cwd').resolve())]
            for fd in (process / 'fd').iterdir():
                try:
                    names.append(os.readlink(fd))
                except FileNotFoundError:
                    pass
            names += [line.split(maxsplit=5)[-1] for line in (process / 'maps').read_text().splitlines()]
            names += (process / 'environ').read_bytes().decode(errors='replace').replace('\0', '\n').splitlines()
            if any(str(path) in name for path in paths for name in names):
                found.append(int(process.name))
        except (FileNotFoundError, ProcessLookupError):
            continue
    return found

def no_mounts(path):
    require(not any(Path(line.split()[4]).is_relative_to(path) for line in
                    Path('/proc/self/mountinfo').read_text().splitlines()), 'output contains a mount')

def inspect(preparation_only=False):
    mounts = (BASE,) if preparation_only else (BASE, BASE / 'qualification', Path('/var/lib/effigy-829/rootful'))
    rows = {}
    for path in mounts:
        info = path.stat()
        require(path.resolve() == path and info.st_dev != os.stat('/').st_dev,
                'bounded mount absent: ' + str(path))
        fs = os.statvfs(path)
        rows[str(path)] = dict(device=info.st_dev, available=fs.f_bavail * fs.f_frsize,
                               free=fs.f_bfree * fs.f_frsize)
    require(len({row['device'] for row in rows.values()}) == len(mounts), 'bounded mounts alias')
    cache = BASE / 'build-cache'
    return dict(mounts=rows, cache_bytes=allocated(cache), cache_limit_bytes=CACHE_LIMIT,
                memory=Path('/proc/meminfo').read_text(), swap=Path('/proc/swaps').read_text())

def evict_cache(limit=CACHE_LIMIT):
    cache = BASE / 'build-cache'
    require(cache.resolve() == cache and cache.stat().st_uid == 1001, 'cache custody differs')
    removed = []
    entries = sorted(cache.iterdir(), key=lambda p: (p.lstat().st_mtime_ns, p.name))
    for entry in entries:
        if allocated(cache) <= limit:
            break
        require(not entry.is_symlink() and entry.stat().st_uid == 1001, 'cache entry custody differs')
        if references([entry]):
            continue
        no_mounts(entry)
        size = allocated(entry)
        print(json.dumps(dict(cache_eviction_intent=str(entry), bytes=size,
                              mtime_ns=entry.stat().st_mtime_ns)), flush=True)
        if entry.is_dir():
            shutil.rmtree(entry)
        else:
            entry.unlink()
        removed.append(dict(path=str(entry), bytes=size))
    require(allocated(cache) <= limit, 'referenced cache exceeds its bound')
    return dict(evicted=removed, cache_bytes=allocated(cache), limit_bytes=limit)

def preflight(preparation_only=False):
    # Empty at window boundaries so cache growth cannot disguise output leaks.
    eviction = evict_cache(0)
    observation = inspect(preparation_only)
    needed = {str(BASE): 12 * GIB, str(BASE / 'qualification'): 640 * 1024 ** 2,
              '/var/lib/effigy-829/rootful': GIB}
    missing = {path: dict(available=observation['mounts'][path]['available'], required=size)
               for path, size in needed.items() if path in observation['mounts']
               and observation['mounts'][path]['available'] < size}
    return dict(observation=observation, eviction=eviction, required=needed, missing=missing)

def checked(path):
    path = Path(path)
    allowed = (BASE / 'builds', BASE / 'artifacts', BASE / 'metadata', BASE / 'qualification', BASE / 'proof-claims')
    require(path.is_absolute() and '..' not in path.parts and path != BASE
            and (any(path.is_relative_to(p) and path != p for p in allowed)
                 or path.parent == BASE and path.name.startswith(('driver-native-session-', 'run-tmp-'))
                 or path.parent == BASE / 'artifact-tmp' and path.name.startswith('runtime-')
                 or path.parent == ENGINE_TMP and path.name.startswith('enospc-')),
            'cleanup path is outside per-run outputs: ' + str(path))
    require(path.parent.resolve(strict=True) == path.parent, 'cleanup ancestor aliases')
    return path

def output_identity(record):
    path = checked(record['path'])
    if not path.exists() and not path.is_symlink():
        return None
    info = path.lstat()
    require(not stat.S_ISLNK(info.st_mode) and info.st_uid == record.get('uid', 1001), 'output custody differs')
    bounded = BASE / 'qualification' if path.is_relative_to(BASE / 'qualification') else (
        ENGINE_TMP if path.is_relative_to(ENGINE_TMP) else BASE)
    require(info.st_dev == bounded.stat().st_dev != os.stat('/').st_dev, 'output is on a foreign mount')
    if 'inode' in record:
        require((info.st_dev, info.st_ino) == (record['device'], record['inode']), 'output identity changed')
    no_mounts(path)
    return path

def dispose(records, archive=None):
    """Export before removal. Refuse references, changed identities and mounts."""
    roots = []
    for record in records:
        path = output_identity(record)
        if path is None:
            continue
        if any(path.is_relative_to(parent) for parent in roots):
            continue
        roots.append(path)
    require(not references(roots), 'outputs still have resident references')
    if archive is None:
        for path in roots:
            shutil.rmtree(path) if path.is_dir() else path.unlink()
        return [str(path) for path in roots]
    # Keep scripts, metadata, streams and failure fixtures. Build source/binary
    # bytes are already bound by the host bundle and build/asset originals.
    fd = os.open(checked(archive), os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    skipped = []
    with os.fdopen(fd, 'wb') as stream, tarfile.open(fileobj=stream, mode='w') as output:
        for path in roots:
            def evidence(member):
                relative = Path(member.name).relative_to(path.relative_to(BASE))
                if path.parent in (BASE / 'builds', BASE / 'artifacts') and relative.parts and relative.parts[0] in ('source', 'driver'):
                    return None
                if member.size > 8 * 1024 ** 2 and (path.parent in (BASE / 'builds', BASE / 'artifacts') or path.name.startswith('run-tmp-')):
                    info = (BASE / member.name).lstat()
                    skipped.append(dict(path=member.name, device=info.st_dev, inode=info.st_ino, bytes=info.st_size, blocks=info.st_blocks))
                    return None
                require(not member.isdev() and not member.isfifo(), 'unexpected output inode')
                return member
            output.add(path, arcname=str(path.relative_to(BASE)), recursive=True, filter=evidence)
        data = json.dumps(skipped).encode()
        entry = tarfile.TarInfo('rebuildable-outputs.json'); entry.size = len(data)
        output.addfile(entry, io.BytesIO(data))
        output.close()
        stream.flush()
        os.fsync(stream.fileno())
    return [str(path) for path in roots]

class Scope:
    def __init__(self, attempt):
        self.attempt = attempt
        self.records = []
        self.units = []
        self.helper = None
        self.gnu_export = None
        import uuid
        self.temporary = BASE / ('run-tmp-' + uuid.uuid4().hex)

    def record(self, path, **identity):
        import common
        path = str(path)
        if any(path == r['path'] or Path(path).is_relative_to(r['path']) for r in self.records):
            return
        record = dict(path=path, **identity)
        common.exclusive(self.attempt.root / ('owned-output-%04d.json' % len(self.records)), record)
        self.records.append(record)

    def operation(self, action, **args):
        import linux_native_run as native
        if self.helper is None:
            self.helper = native.stage_admin_script(self.attempt, Path(__file__).read_bytes())
        request = native.stage_admin_script(self.attempt, json.dumps(args).encode())
        output, errors, _, _ = native.admin_program(self.attempt,
            ['/usr/bin/python3', '-I', '-B', self.helper, action, request],
            seconds=180, memory='256M', file_limit='1G')
        require(not errors, 'lifecycle operation reported stderr')
        result = json.loads(output.splitlines()[-1])
        import common, uuid
        common.exclusive(self.attempt.root / (action + '-' + uuid.uuid4().hex + '.json'), result)
        return result

    def finish(self, ssh, guest_artifact=None):
        import linux_native_run as native
        import common
        for unit in self.units:
            native.stop_owned_unit(self.attempt, unit, BASE / 'qualification')
        if guest_artifact is not None:
            self.gnu_export = export_guest_artifact(self.attempt, ssh, guest_artifact)
        archive = str(BASE / 'metadata' / (self.attempt.root.name + '-outputs.tar'))
        self.operation('export', records=self.records, archive=archive)
        data = native.channel_read(self.attempt, ssh, archive, limit=GIB)
        common.exclusive_bytes(self.attempt.root / 'outputs.tar', data)
        self.operation('remove', records=self.records + [dict(path=archive)])
        self.operation('evict', limit=0)
        return self.operation('inspect')


def export_guest_artifact(attempt, ssh, bindings):
    """Copy the pinned guest GNU artifact family into host evidence before cleanup."""
    import common
    import linux_native_run as native
    evidence_root = attempt.root
    source_item = bindings['guest_artifact']
    require(isinstance(source_item, dict) and isinstance(source_item.get('path'), str),
            'GNU artifact binding is missing its guest path')
    source = Path(source_item['path'])
    require(source.is_absolute() and '..' not in source.parts and str(source) == source_item['path']
            and source.is_relative_to(BASE / 'builds') and source.name == 'effigy-aarch64-unknown-linux-gnu',
            'GNU artifact path is outside the owned guest build tree')
    destination = Path(evidence_root) / 'gnu-artifact'
    destination.mkdir(mode=0o700)
    exported = {}
    for key, suffix in (('guest_artifact', None), ('guest_producer_receipt', '.receipt'),
                        ('guest_active_version', '.active-version'),
                        ('guest_install_identity', '.install-identity')):
        item = bindings[key]
        require(isinstance(item, dict) and isinstance(item.get('path'), str)
                and isinstance(item.get('sha256'), str) and re.fullmatch(r'[0-9a-f]{64}', item['sha256']),
                'GNU producer binding is malformed: ' + key)
        original = Path(item['path'])
        require(original.is_absolute() and '..' not in original.parts and str(original) == item['path']
                and original.is_relative_to(BASE / 'builds'), 'GNU producer path is outside the owned guest build tree')
        if suffix is not None:
            require(original == source.with_suffix(suffix), 'GNU producer sidecar path differs')
        data = native.channel_read(attempt, ssh, str(original), limit=GIB)
        digest = hashlib.sha256(data).hexdigest()
        require(digest == item['sha256'], 'pinned GNU artifact input changed: ' + key)
        target = destination / original.name
        common.exclusive_bytes(target, data)
        exported[key] = {'path': str(target.relative_to(evidence_root)), 'sha256': digest}
    return {'path': str(destination.relative_to(evidence_root)), 'files': exported}


if __name__ == '__main__':
    action, args = sys.argv[1], json.loads(Path(sys.argv[2]).read_bytes())
    result = {'inspect': inspect, 'preflight': preflight, 'evict': evict_cache,
              'export': dispose, 'remove': dispose}[action](**args)
    if action == 'export':
        os.chown(args['archive'], 1001, 1001)
    print(json.dumps(result))

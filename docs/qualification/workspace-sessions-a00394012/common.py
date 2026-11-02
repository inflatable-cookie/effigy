"""Workspace-sessions r1 qualification shared core: identity, GO, engine and cleanup predicates.

Task effigy#530 packet helpers. Modules run from the committed qualification
directory of the reviewed task head; scratch-only evidence and frozen artifacts
(candidate binaries, baseline inventories, GO receipts, execution receipts)
stay under the scratch root and are pinned by hash in each backend manifest.
No module here pulls, starts a VM, mutates shared engine state or retries a
failed execution; every step is read-only except the exact synthetic effects
each packet names, which run only under that packet's GO.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess

BASE = Path('/Volumes/Builds/scratch/effigy/530')
# The committed proof head must be this exact directory in the task head.
DIR = Path(__file__).resolve().parent
BACKENDS = ('colima', 'desktop', 'linux')
CANDIDATE_COMMIT = 'a00394012ae8dafe52981c36d459ec0491ec0552'
CANDIDATE_CHECKOUT = BASE / 'candidate-src-a00394012'
HOST_BINARY = CANDIDATE_CHECKOUT / 'target/debug/effigy'
HOST_BINARY_SHA256 = '58092a16bdab2aa15371ab352c0ab6f60f1baccbda9cd7b9b5e16f021054efb1'
HELPERS = ('launch.py', 'common.py', 'prepare.py', 'preflight.py', 'runtime.py')
FIXTURE_PARENT = Path('/Users/tom/.effigy-scratch')

SCRUBbed_PATH = '/opt/homebrew/bin:/usr/bin:/bin'


def scrubbed_env(with_home=False, extra=None):
    env = {'PATH': SCRUBbed_PATH, 'LANG': 'C'}
    if with_home:
        # Preserve the real HOME solely for Colima's existing profile/SSH
        # state and the container CLI's context files; never forward arbitrary
        # ambient variables or change its value.
        if os.environ.get('HOME') != '/Users/tom':
            raise RuntimeError('unexpected real HOME')
        env['HOME'] = os.environ['HOME']
        for key in ('USER', 'LOGNAME'):
            if key in os.environ:
                env[key] = os.environ[key]
    if extra:
        env.update(extra)
    return env


def run(args, *, with_home=True, timeout=60, check=True, extra_env=None):
    try:
        r = subprocess.run(
            args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            timeout=timeout, env=scrubbed_env(with_home=with_home, extra=extra_env))
    except (OSError, subprocess.TimeoutExpired):
        raise RuntimeError('command unavailable or timed out: ' + Path(args[0]).name) from None
    if check and r.returncode:
        raise RuntimeError('required check failed: ' + Path(args[0]).name +
                           ' status ' + str(r.returncode) + ': ' + (r.stderr or r.stdout)[-400:])
    return r


def digest(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for data in iter(lambda: f.read(65536), b''):
            h.update(data)
    return h.hexdigest()


def exact_ids(output, kind):
    rows = [line for line in output.splitlines() if line != '']
    if any(not re.fullmatch(r'[0-9a-f]{64}', line) for line in rows) or len(rows) != len(set(rows)):
        raise RuntimeError(kind + ' inventory contains malformed or duplicate ID')
    return sorted(rows)


def simple_names(output, kind):
    rows = [line for line in output.splitlines() if line != '']
    if any(not re.fullmatch(r'[A-Za-z0-9_.:-]+', line) for line in rows) or len(rows) != len(set(rows)):
        raise RuntimeError(kind + ' inventory contains malformed or duplicate names')
    return sorted(rows)


def manifest_path(backend):
    return DIR / backend / 'manifest-r1.json'


def packet_path(backend):
    return DIR / backend / 'packet-r1.md'


def go_path(backend):
    return BASE / ('530-' + backend + '-r1-go.txt')


def load_manifest(backend):
    if backend not in BACKENDS:
        raise RuntimeError('unknown backend')
    return json.loads(manifest_path(backend).read_text())


def verify_bundle(backend, packet_sha):
    """Packet SHA against the copied GO, manifest SHA against the packet, and
    every pinned artifact hash against the manifest; candidate source clean at
    the pinned commit."""
    if not re.fullmatch(r'[0-9a-f]{64}', packet_sha):
        raise RuntimeError('packet SHA malformed')
    packet = packet_path(backend)
    if digest(packet) != packet_sha:
        raise RuntimeError('packet SHA mismatch')
    text = packet.read_text()
    mh = re.search(r'Artifacts manifest SHA-256: `([0-9a-f]{64})`', text)
    if not mh:
        raise RuntimeError('packet lacks artifact manifest hash')
    manifest_file = manifest_path(backend)
    if digest(manifest_file) != mh.group(1):
        raise RuntimeError('artifact manifest SHA mismatch')
    manifest = json.loads(manifest_file.read_text())
    if set(manifest.get('helpers', {})) != set(HELPERS):
        raise RuntimeError('artifact manifest helper set mismatch')
    for name, want in manifest['helpers'].items():
        path = DIR / name
        if path.is_symlink() or digest(path) != want:
            raise RuntimeError('helper SHA mismatch: ' + name)
    candidate = manifest['candidate']
    if candidate['commit'] != CANDIDATE_COMMIT or candidate['checkout'] != CANDIDATE_CHECKOUT.name:
        raise RuntimeError('candidate identity differs from the packet subject')
    if candidate['host_binary_sha256'] != HOST_BINARY_SHA256:
        raise RuntimeError('host binary pin differs from the packet subject')
    if HOST_BINARY.is_symlink() or digest(HOST_BINARY) != HOST_BINARY_SHA256:
        raise RuntimeError('host candidate binary hash mismatch')
    git_extra = {'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': '/dev/null',
                 'GIT_TERMINAL_PROMPT': '0'}
    head = run(['/usr/bin/git', '-C', str(CANDIDATE_CHECKOUT), 'rev-parse', 'HEAD'],
               with_home=False, extra_env=git_extra, timeout=30).stdout.strip()
    dirty = run(['/usr/bin/git', '-C', str(CANDIDATE_CHECKOUT), 'status', '--porcelain=v1'],
                with_home=False, extra_env=git_extra, timeout=30).stdout.strip()
    # Content verification of the frozen candidate, not remote equality: exact
    # commit, clean detached scratch source, and the hash pins above. Main may
    # have moved past the candidate; that movement is disclosed in the README
    # and does not invalidate the intentionally frozen subject. No remote is
    # consulted at proof time.
    if head != CANDIDATE_COMMIT or dirty:
        raise RuntimeError('candidate source head changed or checkout dirty')
    baseline = manifest.get('baseline', {})
    if baseline.get('pinned'):
        for key in ('containers', 'networks', 'volumes'):
            rel = baseline[key]
            path = BASE / rel
            if path.is_symlink() or digest(path) != baseline['sha256'][key]:
                raise RuntimeError('pinned baseline hash mismatch: ' + key)
    return {'packet_sha256': packet_sha,
            'artifact_manifest_sha256': mh.group(1),
            'candidate_commit': CANDIDATE_COMMIT,
            'host_binary_sha256': HOST_BINARY_SHA256,
            'helpers': dict(manifest['helpers'])}


def verify_go(backend, packet_sha):
    p = go_path(backend)
    if p.is_symlink():
        raise RuntimeError('exact operator-copied GO receipt missing')
    fd = os.open(p, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_CLOEXEC', 0))
    try:
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode) or st.st_uid != os.getuid() or st.st_nlink != 1 \
                or st.st_mode & 0o077 or st.st_size > 8192:
            raise RuntimeError('GO receipt mode/owner/type invalid')
        data = os.read(fd, 8193)
    finally:
        os.close(fd)
    text = data.decode('utf-8')
    if packet_sha not in text or 'GO' not in text or 'Supervisor' not in text:
        raise RuntimeError('GO receipt does not name supervisor GO and exact packet SHA')
    alias = None
    if backend == 'linux':
        match = re.search(r'^vm-alias ([A-Za-z0-9_.-]+)$', text, re.MULTILINE)
        if not match:
            raise RuntimeError('Linux GO receipt lacks its vm-alias line')
        alias = match.group(1)
    return {'receipt_sha256': hashlib.sha256(data).hexdigest(), 'vm_alias': alias}


def receipt_exclusive(path, data):
    """Write a 0600 O_EXCL receipt; any existing file means a prior execution: no retry."""
    if path.is_symlink() or os.path.lexists(path):
        raise RuntimeError('prior receipt exists; no retry: ' + path.name)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(fd, 'wb') as out:
        out.write(data if isinstance(data, bytes) else (json.dumps(data, indent=2, sort_keys=True) + '\n').encode())
        out.flush()
        os.fsync(out.fileno())
    return digest(path)


def require_absent(path, what):
    if os.path.lexists(path):
        raise RuntimeError(what + ' already exists; do not adopt or delete it: ' + str(path))


def meta(path, rel):
    s = os.lstat(path)
    if s.st_uid != os.getuid():
        raise RuntimeError('foreign-owned fixture entry: ' + rel)
    row = {'rel': rel, 'dev': s.st_dev, 'ino': s.st_ino, 'mode': stat.S_IMODE(s.st_mode), 'uid': s.st_uid}
    if stat.S_ISDIR(s.st_mode):
        row['kind'] = 'dir'
    elif stat.S_ISREG(s.st_mode):
        if s.st_nlink != 1:
            raise RuntimeError('hard-linked fixture file: ' + rel)
        fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_CLOEXEC', 0))
        try:
            fst = os.fstat(fd)
            if (fst.st_dev, fst.st_ino) != (s.st_dev, s.st_ino) or not stat.S_ISREG(fst.st_mode):
                raise RuntimeError('fixture file replaced: ' + rel)
            h = hashlib.sha256()
            while True:
                chunk = os.read(fd, 65536)
                if not chunk:
                    break
                h.update(chunk)
            after = os.fstat(fd)
            if (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns) != \
                    (fst.st_dev, fst.st_ino, fst.st_size, fst.st_mtime_ns):
                raise RuntimeError('fixture file changed while reading: ' + rel)
            row.update(kind='file', size=fst.st_size, sha256=h.hexdigest())
        finally:
            os.close(fd)
    else:
        raise RuntimeError('unexpected fixture entry type: ' + rel)
    return row


def snapshot(root):
    root = Path(root)
    rows = [meta(root, '.')]
    for here, dirs, files in os.walk(root, topdown=True, followlinks=False):
        for name in sorted(dirs + files):
            p = Path(here) / name
            rel = p.relative_to(root).as_posix()
            rows.append(meta(p, rel))
    return sorted(rows, key=lambda r: r['rel'])


def host_mountpoints():
    host = run(['/sbin/mount'], with_home=False).stdout
    mounts = []
    for line in host.splitlines():
        if ' on ' not in line or ' (' not in line:
            raise RuntimeError('host mount output ambiguous')
        mounts.append(line.split(' on ', 1)[1].rsplit(' (', 1)[0])
    return mounts


def guest_mountpoints(backend, manifest, vm_alias=None):
    """Read-only guest mountinfo for the fixture-ancestor mountpoint check.
    Colima reads it through the profile SSH. Docker Desktop has no documented
    host->VM SSH route and the Linux packet's fixture lives on the host, so
    for those backends only host mounts are checked and the packets disclose
    that boundary."""
    if backend == 'colima':
        out = run([manifest['engine']['binary'], 'ssh', '-p', manifest['engine']['profile'],
                   '--', 'cat', '/proc/self/mountinfo'], timeout=30).stdout
    else:
        return []
    mounts = []
    for line in out.splitlines():
        fields = line.split()
        if len(fields) < 10 or '-' not in fields:
            raise RuntimeError('guest mountinfo ambiguous')
        value = re.sub(r'\\([0-7]{3})', lambda m: chr(int(m.group(1), 8)), fields[4])
        if '\\' in value:
            raise RuntimeError('guest mountpoint escape ambiguous')
        mounts.append(value)
    return mounts


def validate_deletable(backend, manifest, root, expected):
    root = Path(root)
    if snapshot(root) != expected:
        raise RuntimeError('fixture tree differs from exact manifest; no deletion')
    current = Path('/')
    for part in root.parts[1:]:
        current = current / part
        if stat.S_ISLNK(os.lstat(current).st_mode):
            raise RuntimeError('symlink in fixture ancestry')
    mounts = host_mountpoints() + guest_mountpoints(backend, manifest)
    if any(m == str(root) or m.startswith(str(root) + '/') for m in mounts):
        raise RuntimeError('fixture contains a mountpoint')


def safe_delete(backend, manifest, root, expected):
    root = Path(root)
    validate_deletable(backend, manifest, root, expected)
    byrel = {r['rel']: r for r in expected}
    flags = os.O_RDONLY | os.O_DIRECTORY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_CLOEXEC', 0)
    pfd = os.open(root.parent, flags)
    rootfd = None
    try:
        before = os.stat(root.name, dir_fd=pfd, follow_symlinks=False)
        rr = byrel['.']
        if (before.st_dev, before.st_ino) != (rr['dev'], rr['ino']):
            raise RuntimeError('fixture root changed')
        rootfd = os.open(root.name, flags, dir_fd=pfd)
        if (os.fstat(rootfd).st_dev, os.fstat(rootfd).st_ino) != (rr['dev'], rr['ino']):
            raise RuntimeError('fixture root identity mismatch')

        def parent_fd(rel):
            pieces = rel.split('/')
            fd = os.dup(rootfd)
            try:
                for i, part in enumerate(pieces[:-1]):
                    nfd = os.open(part, flags, dir_fd=fd)
                    os.close(fd)
                    fd = nfd
                    row = byrel['/'.join(pieces[:i + 1])]
                    st = os.fstat(fd)
                    if (st.st_dev, st.st_ino) != (row['dev'], row['ino']):
                        raise RuntimeError('fixture parent changed')
                return fd, pieces[-1]
            except BaseException:
                os.close(fd)
                raise
        for row in sorted((r for r in expected if r['kind'] == 'file'),
                          key=lambda r: r['rel'], reverse=True):
            fd, name = parent_fd(row['rel'])
            try:
                st = os.stat(name, dir_fd=fd, follow_symlinks=False)
                if not stat.S_ISREG(st.st_mode) or st.st_nlink != 1 or \
                        (st.st_dev, st.st_ino) != (row['dev'], row['ino']):
                    raise RuntimeError('fixture file changed')
                os.unlink(name, dir_fd=fd)
            finally:
                os.close(fd)
        for row in sorted((r for r in expected if r['kind'] == 'dir' and r['rel'] != '.'),
                          key=lambda r: (r['rel'].count('/'), r['rel']), reverse=True):
            fd, name = parent_fd(row['rel'])
            try:
                st = os.stat(name, dir_fd=fd, follow_symlinks=False)
                if not stat.S_ISDIR(st.st_mode) or (st.st_dev, st.st_ino) != (row['dev'], row['ino']):
                    raise RuntimeError('fixture dir changed')
                os.rmdir(name, dir_fd=fd)
            finally:
                os.close(fd)
        after = os.stat(root.name, dir_fd=pfd, follow_symlinks=False)
        if (after.st_dev, after.st_ino) != (rr['dev'], rr['ino']):
            raise RuntimeError('fixture root replaced')
        os.rmdir(root.name, dir_fd=pfd)
    finally:
        if rootfd is not None:
            os.close(rootfd)
        os.close(pfd)


class Engine:
    """Read-only engine surface per backend kind. Effects are issued only by
    runtime.py under GO, using the same command construction."""

    def __init__(self, backend, manifest, vm_alias=None):
        self.backend = backend
        self.kind = manifest['engine']['kind']
        self.manifest = manifest
        if self.kind == 'colima-nerdctl':
            self.cmd_prefix = [manifest['engine']['binary'], 'nerdctl', '-p',
                               manifest['engine']['profile'], '--']
        elif self.kind == 'docker-context':
            self.cmd_prefix = [manifest['engine']['binary'], '--context',
                               manifest['engine']['context']]
        elif self.kind == 'ssh-docker':
            if not vm_alias:
                raise RuntimeError('ssh-docker needs the GO receipt vm-alias')
            self.vm_alias = vm_alias
            self.cmd_prefix = ['/usr/bin/ssh', '-o', 'BatchMode=yes', vm_alias, 'docker']
        else:
            raise RuntimeError('unknown engine kind: ' + self.kind)

    def args(self, *engine_args):
        return self.cmd_prefix + list(engine_args)

    def containers(self):
        return exact_ids(run(self.args('ps', '-a', '--no-trunc', '--format', '{{.ID}}'),
                             timeout=60).stdout, 'container')

    def network_names(self):
        return simple_names(run(self.args('network', 'ls', '--format', '{{.Name}}'),
                                timeout=60).stdout, 'network')

    def volume_names(self):
        return simple_names(run(self.args('volume', 'ls', '--format', '{{.Name}}'),
                                timeout=60).stdout, 'volume')

    def inventory(self):
        containers = self.containers()
        networks = self.network_names()
        volumes = self.volume_names()
        return {'containers': containers, 'networks': networks, 'volumes': volumes,
                'counts': {'containers': len(containers), 'networks': len(networks),
                           'volumes': len(volumes)}}

    def image_ids(self):
        rows = []
        for line in run(self.args('images', '--no-trunc', '--format', '{{.ID}} {{.Repository}}:{{.Tag}}'),
                        timeout=90).stdout.splitlines():
            if not line.strip():
                continue
            ident, ref = line.split(' ', 1)
            if not re.fullmatch(r'sha256:[0-9a-f]{64}', ident):
                raise RuntimeError('image list row malformed')
            rows.append({'id': ident, 'reference': ref})
        return rows

    def require_image(self, reference, image_id):
        """image_id None (Desktop/Linux before the first window) resolves by
        exact reference and returns the row so prepare can pin it; a pinned ID
        must match exactly or the packet stops."""
        found = None
        for row in self.image_ids():
            if row['reference'] == reference:
                if found is not None:
                    raise RuntimeError('duplicate resident image reference: ' + reference)
                found = row
        if found is None:
            raise RuntimeError('required image is not resident; a pull needs its own explicit GO: ' + reference)
        if image_id is not None:
            if found['id'] != image_id:
                raise RuntimeError('resident image identity drifted: ' + reference)
        elif found['reference'] != reference:
            raise RuntimeError('resident image reference mismatch: ' + reference)
        return found

"""Shared fail-closed machinery for the Acow ordinary-Compose consumer
qualification packet (Queue task effigy#540).

This is a preparation-only packet: nothing in this module creates a worktree,
starts a container, installs anything or touches consumer data. The two armed
routes (`warmup.py`, `primary.py`) re-verify every identity below under their
own operator-copied GO receipts before any effect, and refuse closed on any
missing or mismatched input. Evidence is written 0600 under
/Volumes/Builds/scratch/effigy/540/ only.

Every verifier here is written so the committed boundary tests
(`boundaries.py`, the installed-Effigy selector `test:qualification:acow`)
can drive it against disposable fixture metadata and fake backends without
any native call.
"""
import hashlib
import json
import os
import re
import stat
import subprocess
import tempfile
from pathlib import Path

DIR = Path(__file__).resolve().parent
SCRATCH = Path('/Volumes/Builds/scratch/effigy/540')
PACKET = DIR / 'packet-r1.md'
MANIFEST = DIR / 'manifest-r1.json'

# Frozen ordinary candidate (identical subject to the reviewed #537 Colima
# packet; queue-held brief for effigy#540).
CANDIDATE_COMMIT = '5f4669141d8dabd91b7fbfef28d2bce21bc42ba2'
CANDIDATE_BINARY_SHA256 = 'cb564fa64f15d080eb2b047454b55103e3e5857de2252de8be05a31a95eb00b1'
CANDIDATE_VERSION_MARKER = '5f46691'

# Pinned consumer inputs (read-only; the packet never edits Acow or the
# bundle repository). Full commit hash of Acow main as pinned by the brief.
ACOW_PRIMARY = Path('/Users/tom/Dev/projects/acowtancy')
ACOW_PIN_COMMIT = 'b7f004d74a47f702f4dc3ef5c96390f0bdbf3c0b'
ACOW_PIN_TREE = '88df708155e60deb8c8179cffd3c307a3baf5e01'
BUNDLE_SOURCE = 'git@github.com:inflatable-cookie/underlay-effigy-bundle.git'
BUNDLE_LOCK_COMMIT = '61dc5aa8405698ea757f0ff5c481cb6d7cb92314'
ATTACH_TASK = 'infra/tasks.toml:16'  # "worktree:container:attach" line at the pin

# Tracked nonsecret input files pinned by blob hash at ACOW_PIN_COMMIT
# (verified with `git rev-parse <commit>:<path>`; no working-tree reads).
PINNED_INPUT_BLOBS = {
    'infra/tasks.toml': 'ee9c7f26b516db2fbe03baabb01a5b24944bd42b',
    'infra/workspace.toml': 'df59e9722612053be479bdf77e62cee1ac82a5ab',
    'effigy.toml': '80ccc592c5a3794b9e0038657120f4ad8a5bdb0d',
    'scripts/tasks/worktree-container-attach.ts': 'b9ed0416a26064edc22efd5433e2d7dba8884259',
    'state/schema/202608221002__baseline_auth.sql': '65ab9b783d31eaf438be2577be201929a669aa68',
    'state/schema/202608221003__baseline_learning.sql': '1f41b794afe1543a5064d4c7e85d3dcdbf9d9249',
    'state/schema/202608221010__baseline_engagement.sql': 'ed887393a015b8b1fd8f93a8393a9031742d0afb',
    'state/schema/202608221011__baseline_billing.sql': 'f0eae506b32555d71289807be6f9bf845c48c8b0',
}

# Qualified schema/table names for the primary-data counts observation,
# resolved from the pinned tracked sources (never invented). Citation is
# (file at the pin, CREATE TABLE line).
EXPECTED_PRIMARY_COUNTS = {
    'auth.users': (57280, 'state/schema/202608221002__baseline_auth.sql', 5),
    'auth.legacy_user_link': (57279, 'state/schema/202608221002__baseline_auth.sql', 33),
    'billing.orders': (45540, 'state/schema/202608221011__baseline_billing.sql', 357),
    'engagement.feedback': (2372, 'state/schema/202608221010__baseline_engagement.sql', 5),
    'engagement.complaint': (62, 'state/schema/202608221010__baseline_engagement.sql', 26),
    'learning.pathway': (5, 'state/schema/202608221003__baseline_learning.sql', 5),
    'learning.module': (65, 'state/schema/202608221003__baseline_learning.sql', 40),
    'learning.outcome': (6689, 'state/schema/202608221003__baseline_learning.sql', 150),
}
PRIMARY_DATABASE = 'acowtancy'
PRIMARY_PROJECT = 'acowtancy-dev'
PRIMARY_POSTGRES_VOLUME = 'acowtancy-dev-postgres-data'

# Protected shared state (never pruned, recreated, repaired, reset, inspected
# for secret values, or re-mounted by this packet).
SHARED_MARIADB_CONTAINER = 'c47a64b6fc1fe688e3ce503a27704fb978580c22bd4d8c2f7083e509ceb0e6d9'
SHARED_MARIADB_VOLUME = 'effigy-shared-mariadb-0721de3e-mysql-data'
NEVER_TOUCH_VOLUMES = (
    'acowtancy-dev-postgres-data',
    'acowtancy-dev-silo-data',
    'acowtancy-dev-dbgate-data',
    SHARED_MARIADB_VOLUME,
    'effigy-shared-mariadb-0721de3e-legacy_mysql-data',
)

# Engine identities of record (read-only preflight; same reviewed identities
# as the #537 ordinary-Compose Colima packet).
COLIMA_BIN = '/opt/homebrew/bin/colima'
COLIMA_PROFILE = 'effigy'
COLIMA_VERSION = '0.10.3'
NERDCTL_VERSION = '2.3.1'

# Queue QA path shape: /Volumes/Builds/queue/workspaces/<64-hex>/ns-<uuid>.
QUEUE_SHAPE_PREFIX = '/Volumes/Builds/queue/workspaces/'  # 33 incl trailing slash
QUEUE_SHAPE_UUID = '13f69c1b-9a5e-455e-8004-af30ece9685f'  # this task's UUID
QUEUE_SHAPE_PARENT_LEN = len(QUEUE_SHAPE_PREFIX) - 1  # 32, no trailing slash
QUEUE_SHAPE_TOTAL_LEN = len(QUEUE_SHAPE_PREFIX) + 64 + len('/ns-') + 36  # 137
WARMUP_PARENT = '/Volumes/Builds/scratch/effigy/w'  # exactly QUEUE_SHAPE_PARENT_LEN chars

HEX64 = re.compile(r'[0-9a-f]{64}')
HEX40 = re.compile(r'[0-9a-f]{40}')
UUID_RE = re.compile(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}')

# Exact, case-sensitive token contracts for the operator-copied GO receipts.
# These are machine checks, not a substitute for the authorizations named in
# packet-r1.md; a differently worded receipt fails closed.
GO_COMMON_TOKENS = ('GO', 'queue unpaused', 'qa fence null')
GO_WARMUP_TOKENS = GO_COMMON_TOKENS + (
    'Supervisor', 'Tom', 'colima idle', CANDIDATE_BINARY_SHA256,
)
GO_PRIMARY_TOKENS = GO_COMMON_TOKENS + ('Tom', 'primary-data')
RESERVATION_RE = re.compile(r'reservation ([A-Za-z0-9._:-]{4,128})')
RECEIPT539_RE = re.compile(r'539 receipt ([0-9a-f]{64})')
VERSION_LINE_RE = re.compile(r'^version (.+)$', re.MULTILINE)
BINARY_LINE_RE = re.compile(r'^binary ([^\s]+)$', re.MULTILINE)

MOUNTS_LABEL = 'nerdctl/mounts'
MOUNTS_LABEL_BUDGET = 4096  # record length only; refuse at >= budget
WORKTREE_MARKER = '.effigy-540-acow-warmup-owner.json'
ATTACH_RECORD_REL = '.effigy/runtime/bootstrap-fresh-session.json'
COPIED_VAULT_REL = '.effigy/secrets/local.vault'


class Refused(Exception):
    """A fail-closed refusal with the exact reason recorded in evidence."""


def digest(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(65536), b''):
            h.update(chunk)
    return h.hexdigest()


def scrubbed_env(with_home=False):
    env = {'PATH': '/opt/homebrew/bin:/usr/bin:/bin', 'LANG': 'C'}
    if with_home:
        # Preserve the real HOME solely for Colima's existing profile/SSH
        # state; never forward arbitrary ambient variables or change it.
        if os.environ.get('HOME') != '/Users/tom':
            raise Refused('unexpected real HOME')
        env['HOME'] = os.environ['HOME']
        for key in ('USER', 'LOGNAME'):
            if key in os.environ:
                env[key] = os.environ[key]
    return env


def read_exclusive_file(path, max_bytes=8192):
    """Read an operator-supplied receipt with strict file hygiene, or refuse.

    Regular file, single link, owned by this uid, mode 0600, size capped,
    never a symlink (O_NOFOLLOW).
    """
    p = Path(path)
    if not os.path.lexists(p):
        raise Refused('receipt file missing: %s' % p)
    fd = os.open(p, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_CLOEXEC', 0))
    try:
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode) or st.st_uid != os.getuid() or st.st_nlink != 1 \
                or st.st_mode & 0o077 or st.st_size > max_bytes:
            raise Refused('receipt file mode/owner/type invalid: %s' % p)
        data = os.read(fd, max_bytes + 1)
        if len(data) > max_bytes:
            raise Refused('receipt file exceeds %d bytes: %s' % (max_bytes, p))
    finally:
        os.close(fd)
    return data


def verify_packet_bundle():
    """Packet/manifest self-integrity; refuses on any drift or ambiguity."""
    if not PACKET.is_file() or PACKET.is_symlink():
        raise Refused('committed packet missing')
    packet_sha = digest(PACKET)
    text = PACKET.read_text()
    match = re.search(r'Artifacts manifest SHA-256: `([0-9a-f]{64})`', text)
    if not match:
        raise Refused('packet lacks the artifacts manifest hash line')
    if not MANIFEST.is_file() or MANIFEST.is_symlink():
        raise Refused('committed manifest missing')
    if digest(MANIFEST) != match.group(1):
        raise Refused('artifact manifest SHA mismatch')
    obj = json.loads(MANIFEST.read_text())
    helpers = obj.get('helpers')
    if not isinstance(helpers, dict) or not helpers:
        raise Refused('manifest lacks the helpers map')
    for name, want in sorted(helpers.items()):
        path = DIR / name
        if path.is_symlink() or not path.is_file() or digest(path) != want:
            raise Refused('committed helper hash mismatch: ' + name)
    return {'packet_sha256': packet_sha, 'manifest_sha256': match.group(1), 'manifest': obj}


def verify_go_receipt(path, packet_sha, required_tokens, route, scratch_dir=SCRATCH):
    """Verify one operator-copied GO receipt, or refuse closed.

    Exact checks: hygiene (read_exclusive_file), `packet <sha>` equality, the
    route token set, an Acow `reservation <id>`, and per-route structured
    lines (`version `, `binary `, `539 receipt `). Returns a record for the
    invocation receipt. `scratch_dir` pins the exact parent directory; tests
    pass a disposable fixture root, routes use the evidence root.
    """
    if Path(path) != Path(scratch_dir) / Path(path).name or Path(path).is_symlink():
        raise Refused('GO receipt is not at its exact pinned scratch path')
    data = read_exclusive_file(path)
    text = data.decode('utf-8')
    match = re.search(r'packet ([0-9a-f]{64})', text)
    if not match or match.group(1) != packet_sha:
        raise Refused('GO receipt does not name this packet SHA: %s' % path)
    for token in required_tokens:
        if token not in text:
            raise Refused('GO receipt lacks required token %r: %s' % (token, path))
    reservation = RESERVATION_RE.search(text)
    if not reservation:
        raise Refused('GO receipt lacks the Acow reservation line: %s' % path)
    record = {
        'route': route,
        'path': str(path),
        'sha256': hashlib.sha256(data).hexdigest(),
        'packet_sha256': packet_sha,
        'tokens': list(required_tokens),
        'reservation_id': reservation.group(1),
    }
    if route == 'warmup':
        r539 = RECEIPT539_RE.search(text)
        if not r539:
            raise Refused('GO receipt lacks the 539 receipt line: %s' % path)
        record['receipt539_sha256'] = r539.group(1)
        version = VERSION_LINE_RE.search(text)
        if not version:
            raise Refused('GO receipt lacks the installed version line: %s' % path)
        record['installed_version_line'] = version.group(1)
        binary = BINARY_LINE_RE.search(text)
        if not binary:
            raise Refused('GO receipt lacks the installed binary line: %s' % path)
        record['installed_binary_path'] = binary.group(1)
    return record


def verify_539_receipt(path, quoted_sha):
    """Verify the green task-effigy#539 prerequisite receipt, or refuse.

    Machine contract recorded in packet-r1.md: JSON object with task
    "effigy#539", status "green", a 40-hex head and a non-empty selectors
    list. Its file SHA-256 must equal the SHA the GO receipt quotes.
    """
    data = read_exclusive_file(path, max_bytes=65536)
    if hashlib.sha256(data).hexdigest() != quoted_sha:
        raise Refused('539 receipt hash differs from the GO-quoted SHA')
    try:
        obj = json.loads(data.decode('utf-8'))
    except (ValueError, UnicodeDecodeError):
        raise Refused('539 receipt is not valid JSON') from None
    if not isinstance(obj, dict):
        raise Refused('539 receipt is not a JSON object')
    if obj.get('task') != 'effigy#539':
        raise Refused('539 receipt does not name task effigy#539')
    if obj.get('status') != 'green':
        raise Refused('539 receipt is not green')
    head = obj.get('head')
    if not isinstance(head, str) or not HEX40.fullmatch(head):
        raise Refused('539 receipt head is not a 40-hex commit')
    selectors = obj.get('selectors')
    if not isinstance(selectors, list) or not selectors or \
            not all(isinstance(s, str) and s for s in selectors):
        raise Refused('539 receipt selectors missing or malformed')
    return {'path': str(path), 'task': obj['task'], 'status': obj['status'],
            'head': head, 'selectors': selectors, 'sha256': quoted_sha}


def verify_installed_identity(which_fn, version_fn, binary_path, hash_fn,
                              go_version_line):
    """Verify the exact installed candidate identity, or refuse.

    `which_fn()` resolves the PATH effigy; `version_fn(path)` returns its
    first `--version` output line; `hash_fn(path)` hashes the GO-named
    binary. All three must agree with the frozen candidate and the GO
    receipt; a shim, drift or marker mismatch refuses before any effect.
    """
    resolved = which_fn()
    if not resolved:
        raise Refused('no effigy on PATH for the installed identity check')
    live_version = version_fn(resolved)
    if live_version != go_version_line:
        raise Refused('installed version differs from the GO-quoted version line')
    if CANDIDATE_VERSION_MARKER not in live_version:
        raise Refused('installed version does not carry the candidate source marker')
    p = Path(binary_path)
    if p.is_symlink() or not p.is_file():
        raise Refused('GO-named installed binary is not a regular file')
    actual = hash_fn(p)
    if actual != CANDIDATE_BINARY_SHA256:
        raise Refused('installed binary hash differs from the frozen candidate')
    return {'resolved_path': str(resolved), 'version_line': live_version,
            'binary_path': str(p), 'binary_sha256': actual}


def queue_shape_components(parent, packet_sha):
    """Derive the exact Queue-QA-shaped disposable worktree path, or refuse.

    Shape and length must match /Volumes/Builds/queue/workspaces/<64-hex>/
    ns-<uuid> exactly (137 characters): a short path does not qualify the
    nerdctl mounts-label size the warm-up exists to observe. The 64-hex part
    is a real exact-owned SHA-256 over packet, parent and purpose; the UUID
    part is this task's UUID. No directory is created here.
    """
    parent = str(parent)
    if len(parent) != QUEUE_SHAPE_PARENT_LEN:
        raise Refused('fixture parent length %d does not match the Queue QA shape (%d)'
                      % (len(parent), QUEUE_SHAPE_PARENT_LEN))
    if not parent.startswith('/Volumes/Builds/scratch/effigy/'):
        raise Refused('fixture parent is not the exact-owned scratch root: ' + parent)
    if not UUID_RE.fullmatch(QUEUE_SHAPE_UUID):
        raise Refused('task UUID is not canonical')
    part = hashlib.sha256(
        ('effigy#540 acow warm-up|%s|%s' % (parent, packet_sha)).encode()
    ).hexdigest()
    path = '%s/%s/ns-%s' % (parent, part, QUEUE_SHAPE_UUID)
    if len(path) != QUEUE_SHAPE_TOTAL_LEN:
        raise Refused('derived fixture path length %d does not match the Queue QA shape (%d)'
                      % (len(path), QUEUE_SHAPE_TOTAL_LEN))
    return {'path': path, 'hex': part, 'uuid': QUEUE_SHAPE_UUID,
            'length': len(path), 'parent': parent}


def ensure_owned_dir(path, must_be_empty=False):
    """Directory owned by this uid, private, optionally empty; else refuse."""
    p = Path(path)
    if p.is_symlink() or not p.is_dir():
        raise Refused('fixture parent is a symlink or not a real directory: ' + str(p))
    st = os.lstat(p)
    if st.st_uid != os.getuid() or st.st_mode & 0o077:
        raise Refused('fixture parent is not private or not owned: ' + str(p))
    if must_be_empty and any(p.iterdir()):
        raise Refused('fixture parent is not empty: ' + str(p))
    return p


def session_identity(worktree_path):
    """Replicate the pinned attach script's session derivation (task #795).

    session id = `wt-<sanitized-basename>-<sha256(canonical path)[:8]>`;
    compose project = `acowtancy-dev-<session>`. Used to verify the record
    the attach task wrote, never to talk to the attach task.
    """
    canonical = str(Path(worktree_path).resolve())
    d = hashlib.sha256(canonical.encode()).hexdigest()[:8]
    parts = [x for x in canonical.split('/') if x]
    basename = parts[-1] if parts else 'repo'
    sanitized = re.sub(r'[^a-z0-9-]', '-', basename.lower())
    sanitized = re.sub(r'^-+|-+$', '', sanitized) or 'repo'
    session = 'wt-%s-%s' % (sanitized, d)
    return {'canonical': canonical, 'session_id': session,
            'project': 'acowtancy-dev-' + session}


def validate_attach_record(record_path, ident, worktree_path):
    """Parse and verify the fresh-session record the attach task wrote."""
    p = Path(record_path)
    if p.is_symlink() or not p.is_file():
        raise Refused('fresh-session record missing after attach: ' + str(p))
    try:
        obj = json.loads(p.read_text())
    except ValueError:
        raise Refused('fresh-session record is not valid JSON') from None
    if obj.get('session_id') != ident['session_id']:
        raise Refused('fresh-session record names a different session id')
    if obj.get('root_repo') != str(worktree_path):
        raise Refused('fresh-session record names a different root repo')
    if obj.get('repos') != [str(worktree_path)] or obj.get('active') is not True:
        raise Refused('fresh-session record repos/active mismatch')
    return obj


def write_owner_marker(worktree_path, payload):
    """Write the exact-ownership marker (0600, exclusive) or refuse."""
    p = Path(worktree_path) / WORKTREE_MARKER
    fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(fd, 'w') as f:
        json.dump(payload, f, indent=2, sort_keys=True)
        f.write('\n')
    return p


def read_owner_marker(worktree_path, expected):
    """Ownership check for teardown: exact marker match or refuse."""
    p = Path(worktree_path) / WORKTREE_MARKER
    if p.is_symlink() or not p.is_file():
        raise Refused('ownership marker missing at teardown: ' + str(p))
    try:
        obj = json.loads(p.read_text())
    except ValueError:
        raise Refused('ownership marker is not valid JSON') from None
    if obj != expected:
        raise Refused('ownership marker mismatch at teardown: foreign or drifted worktree')
    return obj


def exact_ps_ids(output):
    """All nonempty rows must be canonical, distinct full container IDs."""
    rows = [line for line in output.splitlines() if line != '']
    if any(not HEX64.fullmatch(line) for line in rows) or len(rows) != len(set(rows)):
        raise Refused('container inventory contains malformed or duplicate IDs')
    return sorted(rows)


def parse_volume_names(output):
    names = [line for line in output.splitlines() if line != '']
    if any(not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]*', n) for n in names) \
            or len(names) != len(set(names)):
        raise Refused('volume inventory contains malformed or duplicate names')
    return sorted(names)


def parse_network_rows(output):
    listed = {}
    for line in output.splitlines():
        cols = line.split('\t')
        if len(cols) != 2 or not re.fullmatch(r'[A-Za-z0-9_.-]+', cols[0]) or cols[0] in listed:
            raise Refused('network inventory row ambiguous')
        listed[cols[0]] = cols[1]
    return listed


def capture_inventory(runner):
    """Read-only engine inventory: containers, volumes, networks.

    `runner(args)` returns (returncode, stdout, stderr) for
    `colima nerdctl -p effigy -- <args>`; any nonzero or ambiguous output
    refuses. Identities only — no inspect payloads, no mount contents.
    """
    rc, out, _ = runner(['ps', '-a', '--no-trunc', '--format', '{{.ID}}'])
    if rc:
        raise Refused('container inventory read failed')
    containers = exact_ps_ids(out)
    rc, out, _ = runner(['volume', 'ls', '--format', '{{.Name}}'])
    if rc:
        raise Refused('volume inventory read failed')
    volumes = parse_volume_names(out)
    rc, out, _ = runner(['network', 'ls', '--format', '{{.Name}}\t{{.ID}}'])
    if rc:
        raise Refused('network inventory read failed')
    networks = parse_network_rows(out)
    return {'containers': containers, 'volumes': volumes, 'networks': networks}


def require_mariadb_unchanged(runner):
    """Shared MariaDB container present and running; else refuse."""
    rc, out, _ = runner(['inspect', '--format', '{{.ID}} {{.State.Status}}',
                         SHARED_MARIADB_CONTAINER])
    if rc:
        raise Refused('shared MariaDB container not inspectable: refused to touch anything')
    rows = out.strip().splitlines()
    if len(rows) != 1:
        raise Refused('shared MariaDB inspect ambiguous')
    ident, status = rows[0].split(' ')
    if ident != SHARED_MARIADB_CONTAINER or status != 'running':
        raise Refused('shared MariaDB identity or state changed: refused')
    return {'container': ident, 'status': status}


def volume_mount_identity(runner, volume):
    """Identity (Mountpoint) of one named volume; refuses if absent/odd."""
    rc, out, _ = runner(['volume', 'inspect', '--format', '{{.Name}} {{.Mountpoint}}', volume])
    if rc:
        raise Refused('protected volume not inspectable: ' + volume)
    rows = out.strip().splitlines()
    if len(rows) != 1:
        raise Refused('protected volume inspect ambiguous: ' + volume)
    name, mount = rows[0].split(' ', 1)
    if name != volume or not mount.startswith('/var/lib/nerdctl/') \
            or '..' in Path(mount).parts:
        raise Refused('protected volume identity unusable: ' + volume)
    return mount


def assert_baseline_subset(live, baseline):
    """Every baseline identity still present and unchanged; else refuse."""
    if not set(baseline['containers']) <= set(live['containers']):
        raise Refused('baseline container identity removed or replaced')
    if not set(baseline['volumes']) <= set(live['volumes']):
        raise Refused('baseline volume identity removed or replaced')
    for name, ident in baseline['networks'].items():
        if live['networks'].get(name) != ident:
            raise Refused('baseline network identity removed or replaced: ' + name)


def assert_no_scoped_containers(runner, project):
    rc, out, _ = runner(['ps', '-a', '--no-trunc', '--filter',
                         'label=com.docker.compose.project=' + project,
                         '--format', '{{.ID}}'])
    if rc:
        raise Refused('scoped container read failed')
    remaining = exact_ps_ids(out)
    if remaining:
        raise Refused('scoped containers remain after down: %s' % ','.join(remaining))
    return remaining


def scoped_container_map(runner, project):
    """IDs of one scoped compose project keyed by service label."""
    rc, out, _ = runner(['ps', '-a', '--no-trunc', '--filter',
                         'label=com.docker.compose.project=' + project,
                         '--format', '{{.ID}}'])
    if rc:
        raise Refused('scoped container read failed')
    ids = exact_ps_ids(out)
    services = {}
    for cid in ids:
        rc, out, _ = runner(['inspect', '--format',
                             '{{index .Labels "com.docker.compose.service"}}', cid])
        if rc:
            raise Refused('scoped container inspect failed: ' + cid)
        service = out.strip()
        if not re.fullmatch(r'[a-z][a-z0-9-]*', service) or service in services:
            raise Refused('scoped service label ambiguous: ' + cid)
        services[service] = cid
    return services


def container_state(runner, cid):
    rc, out, _ = runner(['inspect', '--format', '{{.State.Status}}', cid])
    if rc:
        raise Refused('container state read failed: ' + cid)
    status = out.strip()
    if status not in ('running', 'exited', 'created', 'paused', 'unknown'):
        raise Refused('container state outside the closed vocabulary: ' + cid)
    return status


def label_length_only(runner, cid, label):
    """Serialized length of one label value; the value itself is never kept."""
    rc, out, _ = runner(['inspect', '--format', '{{index .Labels "%s"}}' % label, cid])
    if rc:
        raise Refused('label read failed: ' + label)
    if out.endswith('\n'):
        out = out[:-1]
    return len(out.encode('utf-8'))


def git_args():
    env = {'PATH': '/usr/bin:/bin', 'LANG': 'C', 'GIT_CONFIG_NOSYSTEM': '1',
           'GIT_CONFIG_GLOBAL': '/dev/null', 'GIT_TERMINAL_PROMPT': '0'}
    return env


def run_quiet(args, env=None, timeout=30, cwd=None):
    try:
        r = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                           text=True, timeout=timeout, env=env, cwd=cwd)
    except (OSError, subprocess.TimeoutExpired):
        raise Refused('required read-only command unavailable or timed out: '
                      + Path(args[0]).name) from None
    return r


def assert_packet_dir(path):
    """The packet-directory identity both routes and the tests require."""
    p = Path(path).resolve()
    if (p.name, p.parent.name, p.parent.parent.name) != \
            ('acow-ordinary-compose-5f466914', 'qualification', 'docs'):
        raise Refused('route must run from its committed packet directory')
    return p


def assert_no_warmup_scope(networks):
    """Busy fence: no warm-up worktree scope may already exist."""
    for name in networks:
        if name.startswith('acowtancy-dev-wt-'):
            raise Refused('a warm-up worktree scope is already active: ' + name)


def judge_teardown_inventory(live, baseline, project, protected_identity, mount_of=None):
    """Post-window inventory judgment; refuses anything not exactly owned.

    Baseline identities must all survive unchanged; the only allowed
    additions are volumes named after this window's scoped project (they are
    preserved, never deleted); the scoped network must be gone; protected
    volume identities must be unchanged (via `mount_of`, when given).
    Returns the preserved additions.
    """
    assert_baseline_subset(live, baseline)
    owned_prefix = project + '-'
    added_volumes = [v for v in live['volumes'] if v not in set(baseline['volumes'])]
    foreign = [v for v in added_volumes if not v.startswith(owned_prefix)]
    if foreign:
        raise Refused('unrecognized new volumes; not guessing: %s' % ','.join(foreign))
    removed = [c for c in baseline['containers'] if c not in set(live['containers'])]
    if removed:
        raise Refused('baseline containers disappeared during the window')
    for name, ident in baseline['networks'].items():
        if name == project + '_default':
            continue  # owned by this window; down must have removed it
        if live['networks'].get(name) != ident:
            raise Refused('baseline network changed during the window: ' + name)
    if live['networks'].get(project + '_default') is not None:
        raise Refused('scoped network still present after down')
    for name, mount in (protected_identity or {}).items():
        if mount is None:
            continue
        if not isinstance(mount, str):
            raise Refused('protected volume identity record malformed: ' + name)
        if mount_of is not None and mount_of(name) != mount:
            raise Refused('protected volume identity changed: ' + name)
    return sorted(added_volumes)


def verify_consumer_pins(git_fn, primary_path=ACOW_PRIMARY):
    """Pinned Acow main + bundle inputs, all read-only, or refuse.

    `git_fn(args)` runs /usr/bin/git with the scrubbed environment against
    the primary checkout. The pin commit's tree and every pinned input blob
    are content identity, so the check stays valid when consumer main moves
    on; the primary checkout must still be clean and contain the pin.
    """
    if (Path(primary_path) / '.git').is_file():
        raise Refused('pinned Acow path is a linked worktree, not the primary checkout')
    rc, out, _ = git_fn(['rev-parse', ACOW_PIN_COMMIT + '^{commit}'])
    if rc or out.strip() != ACOW_PIN_COMMIT:
        raise Refused('pinned Acow commit object missing or drifted')
    rc, out, _ = git_fn(['rev-parse', ACOW_PIN_COMMIT + '^{tree}'])
    if rc or out.strip() != ACOW_PIN_TREE:
        raise Refused('pinned Acow tree drifted')
    blobs = {}
    for rel, want in sorted(PINNED_INPUT_BLOBS.items()):
        rc, out, _ = git_fn(['rev-parse', '%s:%s' % (ACOW_PIN_COMMIT, rel)])
        if rc or out.strip() != want:
            raise Refused('pinned input blob drifted: ' + rel)
        blobs[rel] = want
    rc, out, _ = git_fn(['status', '--porcelain=v1'])
    if rc:
        raise Refused('primary checkout status unreadable')
    if out.strip():
        raise Refused('primary checkout is dirty')
    rc, _, _ = git_fn(['merge-base', '--is-ancestor', ACOW_PIN_COMMIT, 'HEAD'])
    if rc != 0:
        raise Refused('primary checkout head no longer contains the pinned commit')
    return {'pin_commit': ACOW_PIN_COMMIT, 'pin_tree': ACOW_PIN_TREE,
            'pinned_input_blobs': blobs, 'primary_clean': True}


def verify_bundle_lock(inspect_fn):
    """Bundle lock must still name the pinned revision, or refuse.

    `inspect_fn()` returns the parsed `effigy bundle inspect --json` result
    (read-only; inspection never fetches or writes). A moved lock needs the
    explicit reviewed re-pin the brief requires for changed inputs.
    """
    result = inspect_fn()
    source = (result or {}).get('source') or {}
    if source.get('source_type') != 'git' or source.get('source_path') != BUNDLE_SOURCE:
        raise Refused('bundle source identity changed')
    if source.get('version_hint') != BUNDLE_LOCK_COMMIT:
        raise Refused('bundle lock moved off the pinned revision; re-pin required')
    if not isinstance(source.get('stale'), bool):
        raise Refused('bundle inspect stale flag missing')
    return {'source': source.get('source_path'), 'lock': source.get('version_hint'),
            'stale': source.get('stale')}


def expect_absent(path):
    if os.path.lexists(path):
        raise Refused('exact fixture path already exists; do not adopt or delete it: '
                      + str(path))


def receipt_writer(name):
    """Exclusive 0600 receipt file under the scratch root; no retries."""
    SCRATCH.mkdir(parents=True, exist_ok=True, mode=0o700)
    path = SCRATCH / name
    if path.is_symlink() or os.path.lexists(path):
        raise Refused('evidence file already exists; no retry: ' + str(path))
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    return path, fd


def write_receipt(name, payload):
    path, fd = receipt_writer(name)
    with os.fdopen(fd, 'w') as f:
        json.dump(payload, f, indent=2, sort_keys=True)
        f.write('\n')
    return path


class ConsoleLog:
    """Exclusive 0600 console log for one authorized step's child output."""

    def __init__(self, name):
        self.path, self.fd = receipt_writer(name)

    def write(self, data):
        if isinstance(data, str):
            data = data.encode('utf-8', 'replace')
        os.write(self.fd, data)

    def close(self):
        os.close(self.fd)


def boundaries_root():
    """Fresh disposable fixture root for the committed boundary tests."""
    root = Path(tempfile.mkdtemp(prefix='boundaries-', dir=SCRATCH))
    os.chmod(root, 0o700)
    return root

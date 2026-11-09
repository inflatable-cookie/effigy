"""Fixed Nucleus proof-only Colima Recipe E producer entry.

External precondition: Nucleus must launch the pinned selector after native GO.
This process cannot authenticate launch provenance against the same user. A
request, this file, or caller environment is not that authorisation.

Three roles are bound separately:

- preparation K: the checkout that holds this entry and its program closure;
- candidate C: the clean canonical checkout whose source, config and producer
  the maintained producer compiles; the producer runs with cwd at C;
- host binary: C's own `.local-install/bin/effigy` with its active-version
  sidecar and a source receipt that names C's head and tree.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import preparation_stage as stage
import preparation_resources as resources

REPO_ROOT = HERE.parents[2]
MODULE_ROLE = HERE.relative_to(REPO_ROOT).as_posix()
GIT = '/usr/bin/git'
SCRATCH = '/Volumes/Stash/scratch/effigy'
# The literal supported Homebrew alias. Its realpath must match the allowlisted
# Cellar target below; the pinned canonical target is dispatched, never the alias.
COLIMA = '/opt/homebrew/bin/colima'
COLIMA_CANONICAL = re.compile(r'/opt/homebrew/Cellar/colima/[^/]+/bin/colima')
STASH_SOURCE_MOUNT = '/Volumes/Stash/scratch/effigy/release-source'
CANDIDATE_H_HEAD = '4f6a5ed3078bc6e354c8a7738ea5663c8ffefa02'
TARGET = 'aarch64-unknown-linux-gnu'
PROFILE = 'effigy-release'
SELECTOR = 'workspace:linux:artifact'
DURATION = 1800
RESERVE = 120
ARTIFACT_CAP = 200 * 1024 * 1024
HOST_WRITE_CAP = 5 * 1024 * 1024 * 1024
RECEIPT_CAP = 64 * 1024
SIDECAR_CAP = 4096
# The build's own cargo tree: a fixed private path on the builder's writable
# layer. It is not a profile mount, not a host bind and not shared with another
# checkout, so intermediates never reach the shared mount the source lives in.
# What it costs is VM-side storage, which the record discloses instead of
# counting as a host write.
BUILD_TREE = '/var/tmp/effigy-artifact-build'
BUILD_TREE_CAP = 25 * 1024 * 1024 * 1024
BUILD_TREE_HEADROOM = BUILD_TREE_CAP + 5 * 1024 * 1024 * 1024
# Declared pre-write cap on one staging of the source closure.
STAGED_SOURCE_CAP = 512 * 1024 * 1024
# Per-file caps on the three transferred artifact roles, and their aggregate.
EXPORT_CAP = ARTIFACT_CAP + RECEIPT_CAP + SIDECAR_CAP
BOUNDARY_SCHEMA = 'effigy.builder-write-boundary.v1'
HOST_BINARY_CAP = 512 * 1024 * 1024
HOST_BINARY_ROLE = '.local-install/bin/effigy'
HOST_RECEIPT_KEYS = ('source_head', 'source_tree', 'binary_sha256', 'active_version',
                     'build_kind', 'features', 'dirty', 'completed_at')
HOST_RECEIPT_BUILD_KIND = 'install-local'
OBSERVE_LIMIT = 15
OBSERVE_CAP = 65536
PRODUCER_OUTPUT_CAP = 1024 * 1024
SCHEMA = 'linux-recipe-e-window-v2'
OUTCOME_SCHEMA = 'linux-recipe-e-observation-v2'
SOURCES = ('linux_recipe_e_proof.py', 'preparation_stage.py', 'preparation_resources.py')
CONFIG_ROLES = ('config/tasks.toml', 'config/containers.toml')
PRODUCER_ROLE = 'scripts/build-linux-workspace-artifact.rhai'
REQUIRED_POLICY = {
    'profile': PROFILE,
    'workspace_source': 'staged',
    'checkout_scoped_identity': True,
    'project_name': 'effigy-artifact-builder-v1',
}
# Fixture runs label simulation. Production stays False. Fixture receipts cannot
# stand in as production predecessor evidence.
TEST_SEAM = {'simulation': False}
# Kit §6.3 ≤5 GiB is a Stage2-wide host-write premise (E9 archive, E14 artifact,
# evidence). The compiler has no writable host bind: `/workspace` is read-only
# and SYS_ADMIN is dropped so remount cannot restore writability. Compile output
# stays in the owned builder; the host transfers the three capped roles while
# receiving. Staging/render, transfer, evidence and destination publish are
# finite before they write. The cargo tree is VM-side disclosed storage, not a
# host-bind counter.
HOST_WRITE = {
    'status': 'enforced',
    'cap_bytes': HOST_WRITE_CAP,
    'reason': (
        'SO-host-write: kit §6.3 host writes ≤5 GiB (E9 archive, E14 artifact, evidence) is a '
        'Stage2-wide premise. The compiler has no writable host bind: `/workspace` is read-only '
        'with SYS_ADMIN dropped so remount cannot restore writability. The three artifact roles '
        'leave the builder only through a host-capped transfer that enforces each role\'s cap '
        'while receiving. Staging/render, transfer, evidence and destination publish are finite '
        'before they write. The cargo tree is VM-side disclosed storage, not a host-bind counter'
    ),
}
# Each E14 host writer, its path and the metric it can truthfully report. The
# cargo tree is VM-side disclosed storage, never counted as a host write.
HOST_WRITE_WRITERS = (
    {'writer': 'staging/render/source', 'path': 'STASH_SOURCE_MOUNT (host, checkout-scoped)',
     'metric': 'cumulative bytes written (content-addressed closure, each file once)',
     'bound': 'STAGED_SOURCE_CAP 512MiB refused before the first staged write'},
    {'writer': 'cargo tree', 'path': BUILD_TREE + ' (builder writable layer, VM-side)',
     'metric': 'VM-side retained bytes; not a host bind and not a cumulative write count',
     'bound': 'BUILD_TREE_CAP 25GiB retained, BUILD_TREE_HEADROOM free floor, VM filesystem ENOSPC'},
    {'writer': 'host-capped three-role transfer', 'path': '.effigy/linux-release/artifacts (host destination; compiler never writes it)',
     'metric': 'cumulative bytes received: three declared names, each capped while receiving',
     'bound': 'ARTIFACT_CAP 200MiB + RECEIPT_CAP 64KiB + SIDECAR_CAP 4KiB, enforced while receiving'},
    {'writer': 'evidence', 'path': 'linux-recipe-e-<activation>/ (host)',
     'metric': 'cumulative bytes via capped stream writes',
     'bound': '2*PRODUCER_OUTPUT_CAP + 4*OBSERVE_CAP per occurrence (producer once, two Colima inventories)'},
)
# E14 reserves the staged closure, one host-capped transfer of the three roles,
# and the capped evidence streams: producer stdout/stderr once, Colima inventory
# before and after the producer. Intent, process and receipt JSON are small and
# unreserved. The transfer is charged once: export sizes are not added again.
E14_RESERVATION = (STAGED_SOURCE_CAP + EXPORT_CAP
                   + 2 * PRODUCER_OUTPUT_CAP + 4 * OBSERVE_CAP)
# Restriction-only switch read by the producer's build helper: with `1` it reuses
# only an already running, already configured profile and refuses every start,
# create, restart, settings rewrite and unit start (verify_runtime_backend_no_change).
# The host binary is C's own build, so C's source supplies this route.
NO_CHANGE_ENV = 'EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING'


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def private_root(path):
    if not isinstance(path, str) or not path.startswith(SCRATCH + '/') or os.path.realpath(path) != path:
        raise stage.Refusal('root must be canonical under Stash scratch')
    info = os.lstat(path)
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
        raise stage.Refusal('root must be an owned private directory')
    return [info.st_dev, info.st_ino, info.st_uid, stat.S_IMODE(info.st_mode)]


def open_regular(path, label):
    try:
        fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
    except OSError as exc:
        raise stage.Refusal(label + ' unavailable') from exc
    source = os.fdopen(fd, 'rb')
    try:
        info = os.fstat(source.fileno())
    except BaseException:
        source.close()
        raise
    if not stat.S_ISREG(info.st_mode):
        source.close()
        raise stage.Refusal(label + ' is not a regular file')
    return source, info.st_size


def read_small(path, cap, label):
    source, size = open_regular(path, label)
    with source:
        if size > cap:
            raise stage.Refusal(label + ' exceeds its cap')
        raw = source.read(cap + 1)
    if len(raw) > cap:
        raise stage.Refusal(label + ' changed while read')
    return raw


def stream_digest(path, cap, label):
    source, size = open_regular(path, label)
    with source:
        if size == 0:
            raise stage.Refusal(label + ' is empty')
        if size > cap:
            raise stage.Refusal(label + ' exceeds its cap')
        digest = hashlib.sha256()
        total = 0
        for block in iter(lambda: source.read(1024 * 1024), b''):
            total += len(block)
            if total > size:
                raise stage.Refusal(label + ' changed while read')
            digest.update(block)
    if total != size:
        raise stage.Refusal(label + ' changed while read')
    return digest.hexdigest(), size


def git_value(root, args):
    return subprocess.check_output([GIT, '-C', str(root), *args], timeout=10).decode().strip()


def digest(path, role):
    try:
        return sha(path)
    except OSError as exc:
        raise stage.Refusal('input unavailable: ' + role) from exc


def preparation_identity():
    """Program closure of this entry (preparation K), keyed by checkout role."""
    return {MODULE_ROLE + '/' + name: digest(HERE / name, name) for name in SOURCES}


def candidate_identity(root, head):
    """Clean canonical candidate C: head, tree, config and producer inputs."""
    root = Path(root)
    if not root.is_absolute() or str(root) != os.path.realpath(root):
        raise stage.Refusal('candidate root must be canonical')
    if not re.fullmatch(r'[0-9a-f]{40}', head) or head == CANDIDATE_H_HEAD:
        raise stage.Refusal('candidate must differ from candidate H')
    if git_value(root, ['rev-parse', '--show-toplevel']) != str(root):
        raise stage.Refusal('candidate root is not its own checkout')
    actual = git_value(root, ['rev-parse', 'HEAD'])
    if actual != head:
        raise stage.Refusal('candidate head differs from the pinned candidate')
    tree = git_value(root, ['rev-parse', 'HEAD^{tree}'])
    if subprocess.check_output([GIT, '-C', str(root), 'status', '--porcelain'], timeout=10):
        raise stage.Refusal('candidate checkout is not clean')
    inputs = {}
    for role in CONFIG_ROLES + (PRODUCER_ROLE,):
        inputs[role] = digest(root / role, role)
    return {'root': str(root), 'head': head, 'tree': tree, 'inputs': inputs}


def policy_identity(root):
    text = (Path(root) / 'config/containers.toml').read_text()
    marker = '[containers."linux-release"]'
    start = text.find(marker)
    if start < 0:
        raise stage.Refusal('linux-release policy missing')
    rest = text[start + len(marker):]
    nxt = rest.find('\n[')
    body = rest if nxt < 0 else rest[:nxt]
    fields = {}
    for line in body.splitlines():
        line = line.split('#', 1)[0].strip()
        if not line or '=' not in line:
            continue
        key, value = line.split('=', 1)
        fields[key.strip()] = _toml_value(value)
    identity = {
        'profile': fields.get('profile'),
        'workspace_source': fields.get('workspace_source'),
        'checkout_scoped_identity': fields.get('checkout_scoped_identity') is True,
        'project_name': fields.get('project_name'),
    }
    if identity['profile'] != PROFILE:
        raise stage.Refusal('linux-release profile is not effigy-release')
    if identity['workspace_source'] != 'staged' or fields.get('checkout_scoped_identity') is not True:
        raise stage.Refusal('linux-release policy is not staged checkout-scoped')
    if identity['project_name'] != REQUIRED_POLICY['project_name']:
        raise stage.Refusal('linux-release project identity differs')
    return identity


def _toml_value(raw):
    raw = raw.strip()
    if raw in ('true', 'false'):
        return raw == 'true'
    if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in '"\'':
        return raw[1:-1]
    return raw


def stash_mount():
    path = os.path.realpath(STASH_SOURCE_MOUNT)
    if path != STASH_SOURCE_MOUNT:
        raise stage.Refusal('Stash source mount is not canonical')
    try:
        info = os.lstat(path)
    except OSError as exc:
        raise stage.Refusal('Stash source mount missing') from exc
    if not stat.S_ISDIR(info.st_mode):
        raise stage.Refusal('Stash source mount is not a directory')
    return STASH_SOURCE_MOUNT


def colima_binding():
    """Literal allowlisted alias, its canonical Cellar target and the target's bytes."""
    literal = COLIMA
    if not os.path.isabs(literal) or not os.path.islink(literal):
        raise stage.Refusal('Colima entry is not the supported Homebrew alias')
    canonical_path = os.path.realpath(literal)
    if not COLIMA_CANONICAL.fullmatch(canonical_path) or not os.path.isfile(canonical_path):
        raise stage.Refusal('Colima alias does not resolve to the allowlisted target')
    return {'literal': literal, 'canonical': canonical_path, 'sha256': digest(canonical_path, 'colima')}


def host_binding(root, head, tree):
    """C's own install output with its sidecar and a source receipt naming C."""
    binary = Path(root) / HOST_BINARY_ROLE
    if str(binary) != os.path.realpath(binary):
        raise stage.Refusal('host binary path is not canonical')
    sidecar = Path(str(binary) + '.active-version')
    receipt = Path(str(binary) + '.source-receipt')
    binary_sha, size = stream_digest(binary, HOST_BINARY_CAP, 'host binary')
    version = read_small(sidecar, SIDECAR_CAP, 'host active-version sidecar').decode('utf-8', 'strict').strip()
    if not version:
        raise stage.Refusal('host active-version sidecar empty')
    raw = read_small(receipt, RECEIPT_CAP, 'host source receipt')
    fields = parse_fields(raw, HOST_RECEIPT_KEYS, 'host source receipt')
    if fields['binary_sha256'] != binary_sha:
        raise stage.Refusal('host source receipt does not name these binary bytes')
    if fields['source_head'] != head or fields['source_tree'] != tree:
        raise stage.Refusal('host source receipt does not name candidate C source')
    if fields['active_version'] != version:
        raise stage.Refusal('host source receipt active version differs from the sidecar')
    if fields['build_kind'] != HOST_RECEIPT_BUILD_KIND or fields['dirty'] != 'false' or fields['features'] != '':
        raise stage.Refusal('host source receipt is not a clean default-feature install build')
    return {
        'binary': str(binary), 'sha256': binary_sha, 'size': size,
        'sidecar': str(sidecar), 'active_version': version,
        'receipt': str(receipt), 'receipt_sha256': hashlib.sha256(raw).hexdigest(),
        'source_head': head, 'source_tree': tree,
    }


def machine_identity(colima, interpreter):
    paths = [colima, interpreter, GIT]
    if len(set(paths)) != len(paths):
        raise stage.Refusal('machine input alias')
    return {path: digest(path, path) for path in paths}


def colima_observe_environment():
    env = {'PATH': '/opt/homebrew/bin:/usr/bin:/bin', 'LANG': 'C'}
    if 'HOME' in os.environ:
        env['HOME'] = os.environ['HOME']
    return env


def producer_environment():
    env = {key: os.environ[key] for key in ('PATH', 'HOME', 'TMPDIR', 'TMP', 'TEMP') if key in os.environ}
    env['LANG'] = 'C'
    env['EFFIGY_LINUX_RELEASE_TRIPLE'] = TARGET
    env[NO_CHANGE_ENV] = '1'
    return env


def budget_identity(e9_reserved, e9_actual):
    """Planner-pinned Stage2 values; headroom covers only the E14 writers in E14_RESERVATION."""
    headroom = stage2_headroom(e9_reserved, e9_actual, E14_RESERVATION)
    return {
        'cap_bytes': HOST_WRITE_CAP, 'e9_reserved_bytes': e9_reserved,
        'e9_actual_bytes': e9_actual, 'e14_reserved_bytes': E14_RESERVATION,
        'headroom_bytes': headroom,
        'scope': 'bounded E14 writers only: artifact, receipt, sidecar and evidence streams',
    }


def plan(root, activation_id, head, candidate_root, candidate_head, e9_reserved, e9_actual):
    """Read-only prospective bindings; this is not activation authority."""
    if not re.fullmatch(r'[A-Za-z0-9_-]{4,64}', activation_id):
        raise stage.Refusal('invalid activation id')
    if not re.fullmatch(r'[0-9a-f]{40}', head) or head == CANDIDATE_H_HEAD:
        raise stage.Refusal('preparation head must differ from candidate H')
    identity = private_root(root)
    preparation = preparation_identity()
    candidate = candidate_identity(candidate_root, candidate_head)
    policy = policy_identity(candidate['root'])
    mount = stash_mount()
    program = colima_binding()
    interpreter = os.path.realpath(sys.executable)
    if os.path.realpath(GIT) != GIT:
        raise stage.Refusal('git is not at its canonical path')
    host = host_binding(candidate['root'], candidate['head'], candidate['tree'])
    budget = budget_identity(e9_reserved, e9_actual)
    return {
        'schema': SCHEMA, 'head': head, 'candidate_h': CANDIDATE_H_HEAD,
        'root': root, 'root_identity': identity, 'activation_id': activation_id,
        'duration_seconds': DURATION, 'target': TARGET, 'profile': PROFILE,
        'stash_source_mount': mount, 'selector': SELECTOR, 'policy': policy,
        'preparation': preparation, 'candidate': candidate, 'host': host, 'budget': budget,
        'colima': program, 'program': program['canonical'], 'effigy': host['binary'],
        'machine': machine_identity(program['canonical'], interpreter),
        'interpreter': interpreter,
    }


def parse_unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise stage.Refusal('duplicate JSON key')
        value[key] = item
    return value


def parse_fields(raw, required, label):
    fields = {}
    try:
        text = raw.decode('utf-8', 'strict')
    except UnicodeDecodeError as exc:
        raise stage.Refusal(label + ' is not UTF-8') from exc
    for line in text.splitlines():
        key, separator, value = line.partition('=')
        if not separator or not key or key in fields:
            raise stage.Refusal(label + ' has an unknown or duplicate field')
        fields[key] = value
    for key in required:
        if key not in fields or (fields[key] == '' and key != 'features'):
            raise stage.Refusal(label + ' is missing ' + key)
    return fields


def read_request(path, expected):
    if os.path.realpath(path) != path or not path.startswith(SCRATCH + '/'):
        raise stage.Refusal('request must be canonical under Stash scratch')
    info = os.lstat(path)
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_nlink != 1 or info.st_mode & 0o222:
        raise stage.Refusal('request must be owned and read-only')
    raw = Path(path).read_bytes()
    if len(raw) > 65536 or hashlib.sha256(raw).hexdigest() != expected:
        raise stage.Refusal('request hash or size mismatch')
    return json.loads(raw, object_pairs_hook=parse_unique)


def artifact_paths(candidate_root):
    directory = Path(candidate_root) / '.effigy/linux-release/artifacts'
    name = 'effigy-' + TARGET
    return directory, directory / name, directory / (name + '.receipt'), directory / (name + '.active-version')


def select_release_profile(raw):
    rows = []
    for line in raw.splitlines():
        if not line.strip():
            continue
        item = json.loads(line, object_pairs_hook=parse_unique)
        if item.get('name') == PROFILE:
            rows.append(item)
    if len(rows) != 1:
        raise stage.Refusal('effigy-release profile observation is incomplete')
    row = rows[0]
    if row.get('status') != 'Running':
        raise stage.Refusal('effigy-release profile is not Running')
    arch = row.get('arch')
    if arch not in ('aarch64', 'arm64'):
        raise stage.Refusal('effigy-release architecture is not aarch64')
    # The profile's declared disk is the ceiling the build tree's VM-side writes
    # are bounded by. It is read from the engine's own inventory, never assumed
    # and never raised here.
    disk = row.get('disk')
    if not isinstance(disk, int) or isinstance(disk, bool) or disk <= 0:
        raise stage.Refusal('effigy-release profile does not report its declared disk')
    if disk < BUILD_TREE_HEADROOM:
        raise stage.Refusal('effigy-release profile disk is below the declared build-tree floor')
    return {'name': PROFILE, 'status': row.get('status'), 'runtime': row.get('runtime'),
            'arch': arch, 'disk_bytes': disk}


def parse_receipt(raw):
    fields = parse_fields(raw, ('binary', 'release_triple', 'sha256', 'build_kind', 'completed_at'),
                          'receipt')
    return fields


def check_elf(data):
    if len(data) < 64:
        raise stage.Refusal('artifact is not an ELF64 image')
    if data[0:4] != b'\x7fELF':
        raise stage.Refusal('artifact ELF magic differs')
    if data[4] != 2:
        raise stage.Refusal('artifact is not ELFCLASS64')
    if data[5] != 1:
        raise stage.Refusal('artifact is not ELFDATA2LSB')
    e_type = int.from_bytes(data[16:18], 'little')
    e_machine = int.from_bytes(data[18:20], 'little')
    if e_type not in (2, 3):
        raise stage.Refusal('artifact e_type is not ET_EXEC or ET_DYN')
    if e_machine != 183:
        raise stage.Refusal('artifact e_machine is not EM_AARCH64')
    return {'ei_class': 2, 'ei_data': 1, 'e_type': e_type, 'e_machine': 183}


def check_artifact(binary, receipt_path, sidecar, expected_version):
    source, size = open_regular(binary, 'artifact')
    with source:
        if size == 0:
            raise stage.Refusal('artifact is empty')
        if size > ARTIFACT_CAP:
            raise stage.Refusal('artifact exceeds 200MiB')
        header = source.read(64)
        source.seek(0)
        digest_value = hashlib.sha256()
        total = 0
        for block in iter(lambda: source.read(1024 * 1024), b''):
            total += len(block)
            if total > size:
                raise stage.Refusal('artifact changed while read')
            digest_value.update(block)
    if total != size:
        raise stage.Refusal('artifact changed while read')
    digest_value = digest_value.hexdigest()
    raw_receipt = read_small(receipt_path, RECEIPT_CAP, 'artifact receipt')
    receipt = parse_receipt(raw_receipt)
    if receipt['sha256'] != digest_value:
        raise stage.Refusal('receipt SHA mismatch')
    if receipt['release_triple'] != TARGET:
        raise stage.Refusal('receipt target differs')
    if receipt['build_kind'] != 'workspace-artifact':
        raise stage.Refusal('receipt build_kind differs')
    version = read_small(sidecar, SIDECAR_CAP, 'artifact active-version sidecar').decode('utf-8', 'strict').strip()
    if version != expected_version:
        raise stage.Refusal('active-version sidecar differs from host')
    elf = check_elf(header)
    return {
        'sha256': digest_value, 'size': size, 'elf': elf, 'receipt': receipt,
        'receipt_sha256': hashlib.sha256(raw_receipt).hexdigest(),
        'active_version': version, 'binary': str(binary),
        'receipt_path': str(receipt_path), 'sidecar': str(sidecar),
    }


def runtime_inputs(request):
    """Hash-bound inputs for the stage primitive: K program, C config/producer, host bytes."""
    runtime = {}
    for role, expected in request['preparation'].items():
        resolved = _resolve(REPO_ROOT, role)
        if sha(resolved) != expected:
            raise stage.Refusal('repository input differs: ' + role)
        runtime[str(resolved)] = expected
    candidate = request['candidate']
    for role, expected in candidate['inputs'].items():
        resolved = _resolve(Path(candidate['root']), role)
        if sha(resolved) != expected:
            raise stage.Refusal('repository input differs: ' + role)
        runtime[str(resolved)] = expected
    host = request['host']
    runtime[host['binary']] = host['sha256']
    runtime[host['sidecar']] = hashlib.sha256(read_small(host['sidecar'], SIDECAR_CAP, 'host active-version sidecar')).hexdigest()
    runtime[host['receipt']] = host['receipt_sha256']
    return runtime


def _resolve(root, role):
    relative = Path(role)
    if relative.is_absolute() or any(part in ('', '.', '..') for part in relative.parts):
        raise stage.Refusal('invalid repository role')
    try:
        resolved = (Path(root) / relative).resolve(strict=True)
    except OSError as exc:
        raise stage.Refusal('repository input missing: ' + role) from exc
    if resolved != Path(root).resolve(strict=True) / relative or not resolved.is_file():
        raise stage.Refusal('repository input differs: ' + role)
    return resolved


def bindings(request):
    """Recompute every binding with the request's own pinned roles, heads and budget."""
    return plan(request['root'], request['activation_id'], request['head'],
                request['candidate']['root'], request['candidate']['head'],
                request['budget']['e9_reserved_bytes'], request['budget']['e9_actual_bytes'])


def bound_inputs(request):
    current = bindings(request)
    if current != request:
        drifted = sorted(key for key in set(current) | set(request) if current.get(key) != request.get(key))
        raise stage.Refusal('request bindings drifted: ' + ', '.join(drifted))
    actual = git_value(HERE, ['rev-parse', 'HEAD'])
    if subprocess.check_output([GIT, '-C', str(HERE), 'status', '--porcelain'], timeout=10):
        raise stage.Refusal('preparation checkout is not clean')
    if actual != request['head']:
        raise stage.Refusal('preparation head drifted')
    runtime = runtime_inputs(request)
    if set(runtime) & set(request['machine']):
        raise stage.Refusal('input alias between checkout and machine')
    return actual, runtime


def stage2_headroom(e9_reserved, e9_actual, e14_reserved, cap=HOST_WRITE_CAP):
    """Stage2 host-write headroom: E9 is committed at its reservation, plus E14's reservation.

    E9 actual above its reservation is an overshoot, not a smaller claim. Missing,
    boolean, negative or non-integer telemetry refuses. Zero reservations are valid.
    """
    values = (e9_reserved, e9_actual, e14_reserved)
    if any(not isinstance(v, int) or isinstance(v, bool) or v < 0 for v in values):
        raise stage.Refusal('stage2 budget telemetry is missing or invalid')
    if e9_actual > e9_reserved:
        raise stage.Refusal('E9 actual host writes exceed their reservation')
    committed = e9_reserved + e14_reserved
    if committed > cap:
        raise stage.Refusal('stage2 host-write budget exceeded')
    return cap - committed


def execute(request, request_path, request_hash):
    launched_at = time.time()
    deadline = time.monotonic() + DURATION
    if HOST_WRITE['status'] != 'enforced':
        raise stage.Refusal(HOST_WRITE['reason'])
    actual, runtime = bound_inputs(request)
    if time.monotonic() >= deadline - RESERVE:
        raise stage.Refusal('blocked producer clock')
    if not 0 < PRODUCER_OUTPUT_CAP <= 64 * 1024 * 1024 or not 0 < OBSERVE_CAP <= 64 * 1024 * 1024:
        raise stage.Refusal('zero output cap')
    root = Path(request['root'])
    candidate_root = request['candidate']['root']
    claim = root / ('linux-recipe-e-' + request['activation_id'] + '.claim')
    destination = root / ('linux-recipe-e-' + request['activation_id'])
    if os.path.lexists(claim) or os.path.lexists(destination):
        raise stage.Refusal('duplicate claim')
    stage.exclusive(claim, canonical({'request_sha256': request_hash, 'launched_at': launched_at,
                                      'duration_seconds': DURATION}) + b'\n')
    fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)
    destination.mkdir(mode=0o700)
    operational_end = deadline - RESERVE
    inputs = dict(runtime)
    inputs.update(request['machine'])
    inputs[request_path] = request_hash
    outcome = {
        'schema': OUTCOME_SCHEMA, 'request_sha256': request_hash, 'head': actual,
        'candidate_h': CANDIDATE_H_HEAD, 'launched_at': launched_at,
        'target': TARGET, 'profile': PROFILE, 'selector': SELECTOR,
        'policy': request['policy'], 'preparation': request['preparation'],
        'candidate': {key: value for key, value in request['candidate'].items() if key != 'inputs'},
        'candidate_inputs': request['candidate']['inputs'], 'host': request['host'],
        'budget': request['budget'], 'colima': request['colima'], 'machine_identity': request['machine'],
        'preparation_root': str(REPO_ROOT), 'candidate_root': candidate_root,
        'runtime_inputs': runtime, 'simulation': bool(TEST_SEAM['simulation']),
        'usable_production_predecessor': False,
        'toolchain': {
            'status': 'missing',
            'reason': 'SO-4: owned-builder exec is not a supported E14 interface; cargo -V/rustc -Vv not collected',
        },
        'remote_workload_closure': 'unknown',
        'producer_closure': 'unknown',
        'profile_stop_attempted': False,
        'host_write': dict(HOST_WRITE),
        'host_write_writers': list(HOST_WRITE_WRITERS),
        'findings': [],
    }
    serial = 0

    def remaining(end):
        return end - time.monotonic()

    def call(argv, limit, cap, end, environment, cwd):
        nonlocal serial
        if private_root(str(root)) != request['root_identity']:
            raise stage.Refusal('evidence root identity changed')
        budget = min(limit, remaining(end))
        if not 0 < budget <= stage.STAGE_TIMEOUT_CAP:
            raise stage.Refusal('blocked producer clock')
        serial += 1
        step = destination / ('step-%03d' % serial)
        step.mkdir(mode=0o700)
        result = stage.run_stage(step, argv, budget, cap, inputs, cwd=cwd,
                                 environment=environment, deadline=end,
                                 cleanup_deadline=deadline)
        return result, step

    def good(result):
        return result.get('disposition') == 'exited' and result.get('waited_exit') == 0

    try:
        artifacts_dir, binary, receipt_path, sidecar = artifact_paths(candidate_root)
        existing = [path for path in (binary, receipt_path, sidecar) if os.path.lexists(path)]
        if existing:
            raise stage.Refusal('stale artifact/receipt')
        colima_env = colima_observe_environment()
        colima_argv = [request['program'], 'list', '--json']
        if any(token in ('-p', '--profile', 'start', 'stop', 'delete', 'restart') for token in colima_argv[1:]):
            raise stage.Refusal('Colima observation argv is not inventory-only')
        outcome['colima_argv'] = colima_argv
        outcome['colima_environment'] = colima_env
        result, step = call(colima_argv, OBSERVE_LIMIT, OBSERVE_CAP, operational_end, colima_env, str(REPO_ROOT))
        outcome['colima_observation'] = result
        if not good(result):
            raise stage.Refusal('Colima inventory observation failed')
        outcome['colima_profile'] = select_release_profile((step / 'stdout').read_bytes())
        producer_env = producer_environment()
        if 'EFFIGY_ACTIVE_VERSION' in producer_env:
            raise stage.Refusal('EFFIGY_ACTIVE_VERSION must not authenticate the candidate')
        producer_argv = [request['effigy'], SELECTOR]
        outcome['producer_argv'] = producer_argv
        outcome['producer_environment'] = producer_env
        result, step = call(producer_argv, DURATION, PRODUCER_OUTPUT_CAP, operational_end, producer_env, candidate_root)
        outcome['producer'] = result
        outcome['producer_closure'] = result.get('disposition', 'unknown')
        if result.get('disposition') in ('unknown', 'timeout') or result.get('waited_exit') is None:
            outcome['remote_workload_closure'] = 'unknown'
            raise stage.Refusal('producer closure unknown')
        if not good(result):
            outcome['remote_workload_closure'] = 'unknown'
            raise stage.Refusal('producer nonzero exit')
        # The producer's own record of its writers. A build that does not publish
        # one — an older implementation that ignores the restriction switch among
        # them — is refused here, before its bytes are read or retained.
        boundary = verify_boundary(read_boundary(read_small(step / 'stdout', PRODUCER_OUTPUT_CAP,
                                                            'producer stdout')), request)
        outcome['write_boundary'] = boundary
        if boundary['build_tree']['reset_before']:
            outcome['findings'].append('builder cargo tree was reset above its cap before the build')
        result, step = call(colima_argv, OBSERVE_LIMIT, OBSERVE_CAP, operational_end, colima_env, str(REPO_ROOT))
        outcome['colima_observation_after'] = result
        if not good(result):
            raise stage.Refusal('Colima inventory observation failed after the producer')
        if select_release_profile((step / 'stdout').read_bytes()) != outcome['colima_profile']:
            raise stage.Refusal('effigy-release profile changed during the producer')
        actual, runtime = bound_inputs(request)
        outcome['head'] = actual
        outcome['runtime_inputs'] = runtime
        checked = check_artifact(binary, receipt_path, sidecar, request['host']['active_version'])
        # Bind the record to the bytes before anything is retained: an export
        # whose own record names other bytes is refused with the originals left
        # exactly where the producer wrote them.
        boundary_record = host_write_boundary(boundary, request['budget'], checked['size'])
        retained = destination / 'retained'
        retained.mkdir(mode=0o700)
        copies = {}
        caps = {'binary': ARTIFACT_CAP, 'receipt': RECEIPT_CAP, 'active-version': SIDECAR_CAP}
        for label, source in (('binary', binary), ('receipt', receipt_path), ('active-version', sidecar)):
            handle, _ = open_regular(source, label)
            with handle:
                copies[label] = resources.bounded_copy(handle, retained / Path(source).name, caps[label])
        if copies['binary']['sha256'] != checked['sha256'] or copies['receipt']['sha256'] != checked['receipt_sha256']:
            raise stage.Refusal('producer originals changed while retained')
        actual, runtime = bound_inputs(request)
        outcome['head'] = actual
        outcome['runtime_inputs'] = runtime
        outcome['artifact'] = checked
        outcome['host_write'] = boundary_record
        outcome['original_hashes'] = {label: row['sha256'] for label, row in copies.items()}
        outcome['producer_workload'] = 'completed'
        outcome['remote_workload_closure'] = 'unknown'
        outcome['checker'] = {
            'artifact_cap_bytes': ARTIFACT_CAP,
            'target': TARGET,
            'elf': 'ELF64 little-endian AArch64 ET_EXEC or ET_DYN',
            'rewrites_originals': False,
        }
    except Exception as exc:
        outcome['findings'].append(type(exc).__name__ + ': ' + str(exc))
        outcome['remote_workload_closure'] = 'unknown'
    finally:
        outcome['finished_at'] = time.time()
        outcome['profile_stop_attempted'] = False
        stage.record(destination / 'outcome.json', outcome)
    return outcome


def boundary_fields(value, keys, label):
    """One record's exact field set. A missing, extra or retyped field refuses."""
    if not isinstance(value, dict) or set(value) != set(keys):
        raise stage.Refusal('write-boundary record field set differs at ' + label)
    return value


def boundary_count(value, label, maximum=HOST_WRITE_CAP):
    """A non-negative integer the container itself reported."""
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise stage.Refusal('write-boundary record ' + label + ' is not a non-negative integer')
    if value > maximum:
        raise stage.Refusal('write-boundary record ' + label + ' exceeds its declared bound')
    return value


def read_boundary(raw):
    """The one record a successful producer publishes about its own writers."""
    try:
        text = raw.decode('utf-8', 'strict')
    except UnicodeDecodeError as exc:
        raise stage.Refusal('producer stdout is not UTF-8') from exc
    records = []
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith('{') or BOUNDARY_SCHEMA not in line:
            continue
        value = json.loads(line, object_pairs_hook=parse_unique)
        if isinstance(value, dict) and value.get('schema') == BOUNDARY_SCHEMA:
            records.append(value)
    if len(records) != 1:
        raise stage.Refusal(
            'the producer published ' + str(len(records))
            + ' write-boundary records; the source that ran must publish exactly one')
    return records[0]


def verify_boundary(value, request):
    """Bind the producer's record to this entry's declared boundary."""
    boundary_fields(value, ('schema', 'scope', 'target', 'no_change', 'source_bind', 'staged',
                            'export', 'transfer', 'build_tree'), 'the record')
    if value['target'] != TARGET:
        raise stage.Refusal('write-boundary record names another target')
    if not isinstance(value['scope'], str) or 'finite-before-write' not in value['scope']:
        raise stage.Refusal('write-boundary record does not declare finite-before-write scope')
    no_change = boundary_fields(value['no_change'], ('switch', 'value', 'restricted'), 'no_change')
    if no_change['switch'] != NO_CHANGE_ENV or no_change['value'] != '1' or no_change['restricted'] is not True:
        raise stage.Refusal(
            'the producer did not run under the restriction-only switch; an implementation that '
            'ignores it cannot publish this record and is never assumed safe')
    source = boundary_fields(
        value['source_bind'],
        ('target', 'read_only', 'nested_mounts', 'census', 'source_files', 'build_output_entries_before'),
        'source_bind')
    if source['target'] != '/workspace' or source['nested_mounts'] != 0:
        raise stage.Refusal('the build retains a mount below its source bind')
    if source['read_only'] is not True:
        raise stage.Refusal('the source bind is not read-only; compiler writes are not contained')
    # The bound source starts as the closure and nothing else: an entry already
    # there when the build starts is output from another writer, not this build's
    # starting state, so nothing it observes afterwards is attributable.
    if boundary_count(source['build_output_entries_before'], 'source_bind.build_output_entries_before', 4096) != 0:
        raise stage.Refusal('the bound source already held build output this build did not write')
    staged = boundary_fields(value['staged'], ('mode', 'files', 'bytes', 'cap_bytes', 'changed'), 'staged')
    if staged['mode'] != 'staged':
        raise stage.Refusal('the candidate policy is not the staged source this entry binds')
    if staged['cap_bytes'] != STAGED_SOURCE_CAP or staged['bytes'] > STAGED_SOURCE_CAP:
        raise stage.Refusal('the staged source closure exceeds its declared cap')
    if source['census'] is not True or boundary_count(source['source_files'], 'source_bind.source_files') == 0:
        raise stage.Refusal('the build did not census the staged source it compiled')
    if staged['files'] != source['source_files']:
        raise stage.Refusal('the staged closure census and the staged record disagree')
    export = boundary_fields(value['export'], ('directory', 'entries', 'unexpected', 'binary_bytes',
                                               'receipt_bytes', 'sidecar_bytes', 'caps', 'released'), 'export')
    if export['directory'] != BUILD_TREE + '/target/' + TARGET + '/release':
        raise stage.Refusal('the artifact export is not the declared container-local directory')
    caps = boundary_fields(export['caps'], ('binary', 'receipt', 'sidecar'), 'export.caps')
    if caps != {'binary': ARTIFACT_CAP, 'receipt': RECEIPT_CAP, 'sidecar': SIDECAR_CAP}:
        raise stage.Refusal('the artifact export caps differ from this entry\'s declaration')
    if export['unexpected'] != 0:
        raise stage.Refusal('the artifact export holds entries that are not artifact-shaped files')
    if boundary_count(export['entries'], 'export.entries', 64) < 3:
        raise stage.Refusal('the artifact export does not hold the three declared names')
    if boundary_count(export['binary_bytes'], 'export.binary_bytes', ARTIFACT_CAP) == 0:
        raise stage.Refusal('the artifact export published an empty binary')
    boundary_count(export['receipt_bytes'], 'export.receipt_bytes', RECEIPT_CAP)
    boundary_count(export['sidecar_bytes'], 'export.sidecar_bytes', SIDECAR_CAP)
    if export['released'] is not True:
        raise stage.Refusal('the build did not release its staged export')
    transfer = boundary_fields(value['transfer'], ('mode', 'bytes'), 'transfer')
    if transfer['mode'] != 'host-capped':
        raise stage.Refusal('the transfer is not the host-capped three-role path')
    boundary_count(transfer['bytes'], 'transfer.bytes', EXPORT_CAP)
    tree = boundary_fields(value['build_tree'], ('path', 'cap_bytes', 'headroom_floor_bytes',
                                                 'retained_bytes', 'reset_before', 'free_bytes_before',
                                                 'free_bytes_after', 'writers_closed', 'quiescent',
                                                 'storage'), 'build_tree')
    if tree['path'] != BUILD_TREE:
        raise stage.Refusal('the build tree is not the declared container-local path')
    if tree['cap_bytes'] != BUILD_TREE_CAP or tree['headroom_floor_bytes'] != BUILD_TREE_HEADROOM:
        raise stage.Refusal('the build tree caps differ from this entry\'s declaration')
    boundary_count(tree['retained_bytes'], 'build_tree.retained_bytes', BUILD_TREE_CAP)
    for key in ('free_bytes_before', 'free_bytes_after'):
        if boundary_count(tree[key], 'build_tree.' + key, 1 << 62) < BUILD_TREE_HEADROOM:
            raise stage.Refusal('the builder filesystem lost the declared headroom at ' + key)
    if tree['writers_closed'] is not True or tree['quiescent'] is not True:
        raise stage.Refusal(
            'the build writer\'s closure is not positively proven; client exit alone is not closure')
    if not isinstance(tree['storage'], str) or 'VM disk' not in tree['storage']:
        raise stage.Refusal('the build tree does not disclose its VM backing-disk storage')
    if not isinstance(tree['reset_before'], bool):
        raise stage.Refusal('the build tree reset flag is not a boolean')
    return value


def host_write_boundary(value, budget, artifact_size):
    """Charge the finite-before-write host surfaces under the Stage2 premise.

    Staging, the host-capped three-role transfer and evidence are charged.
    Export sizes inside the builder are not added again on top of the transfer.
    The cargo tree is disclosed as VM-side storage, never as a host-bind count.
    """
    export = value['export']
    observed = (value['staged']['bytes'] + value['transfer']['bytes']
                + 2 * PRODUCER_OUTPUT_CAP + 4 * OBSERVE_CAP)
    if observed > E14_RESERVATION:
        raise stage.Refusal('E14 host writes exceed their reservation')
    if export['binary_bytes'] != artifact_size:
        raise stage.Refusal('the export the producer recorded is not the artifact this entry checked')
    committed = budget['e9_reserved_bytes'] + observed
    if committed > HOST_WRITE_CAP:
        raise stage.Refusal('stage2 host-write budget exceeded')
    tree = value['build_tree']
    return {
        'status': HOST_WRITE['status'],
        'guarantee': 'finite-before-write',
        'scope': HOST_WRITE['reason'],
        'cap_bytes': HOST_WRITE_CAP,
        'reason': HOST_WRITE['reason'],
        'e9_reserved_bytes': budget['e9_reserved_bytes'],
        'e9_actual_bytes': budget['e9_actual_bytes'],
        'e14_observed_bytes': observed,
        'e14_reserved_bytes': E14_RESERVATION,
        'committed_bytes': committed,
        'headroom_bytes': HOST_WRITE_CAP - committed,
        'observed': {
            'staged_source_bytes': value['staged']['bytes'],
            'staged_source_cap_bytes': STAGED_SOURCE_CAP,
            'export_binary_bytes': export['binary_bytes'],
            'export_receipt_bytes': export['receipt_bytes'],
            'export_sidecar_bytes': export['sidecar_bytes'],
            'export_cap_bytes': EXPORT_CAP,
            'transfer_bytes': value['transfer']['bytes'],
            'evidence_cap_bytes': 2 * PRODUCER_OUTPUT_CAP + 4 * OBSERVE_CAP,
            'originals_released_staged_export': export['released'],
        },
        'vm_backing_disk': {
            'writer': 'cargo tree',
            'path': tree['path'],
            'retained_bytes': tree['retained_bytes'],
            'cap_bytes': tree['cap_bytes'],
            'headroom_floor_bytes': tree['headroom_floor_bytes'],
            'free_bytes_before': tree['free_bytes_before'],
            'free_bytes_after': tree['free_bytes_after'],
            'writers_closed': tree['writers_closed'],
            'quiescent': tree['quiescent'],
            'disclosure': tree['storage'],
            'counted_as_host_write': False,
        },
        'no_change': value['no_change'],
        'writers': list(HOST_WRITE_WRITERS),
    }


def stage2_argument(raw):
    if not re.fullmatch(r'[0-9]{1,20}', raw):
        raise stage.Refusal('stage2 budget argument must be a non-negative integer')
    return int(raw)


def main(argv):
    if len(argv) == 9 and argv[1] == 'plan':
        budget = [stage2_argument(argv[7]), stage2_argument(argv[8])]
        print(json.dumps(plan(argv[2], argv[3], argv[4], argv[5], argv[6], *budget), sort_keys=True, indent=2))
        return 0
    if len(argv) != 3:
        raise stage.Refusal('proof entry requires a request and SHA-256')
    request = read_request(argv[1], argv[2])
    result = execute(request, argv[1], argv[2])
    print(json.dumps(result, sort_keys=True))
    return 0 if not result['findings'] else 1


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except (stage.Refusal, OSError, ValueError, KeyError) as exc:
        print('refused: ' + str(exc), file=sys.stderr)
        sys.exit(2)

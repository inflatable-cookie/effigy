"""Fixed Nucleus proof-only Colima Recipe E producer entry.

External precondition: Nucleus must launch the pinned selector after native GO.
This process cannot authenticate launch provenance against the same user. A
request, this file, or caller environment is not that authorisation.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import preparation_stage as stage

REPO_ROOT = HERE.parents[2]
MODULE_ROLE = HERE.relative_to(REPO_ROOT).as_posix()
GIT = '/usr/bin/git'
SCRATCH = '/Volumes/Stash/scratch/effigy'
COLIMA = '/opt/homebrew/bin/colima'
STASH_SOURCE_MOUNT = '/Volumes/Stash/scratch/effigy/release-source'
CANDIDATE_H_HEAD = '4f6a5ed3078bc6e354c8a7738ea5663c8ffefa02'
TARGET = 'aarch64-unknown-linux-gnu'
PROFILE = 'effigy-release'
SELECTOR = 'workspace:linux:artifact'
DURATION = 1800
RESERVE = 120
ARTIFACT_CAP = 200 * 1024 * 1024
HOST_WRITE_CAP = 5 * 1024 * 1024 * 1024
OBSERVE_LIMIT = 15
OBSERVE_CAP = 65536
PRODUCER_OUTPUT_CAP = 1024 * 1024
SCHEMA = 'linux-recipe-e-window-v1'
OUTCOME_SCHEMA = 'linux-recipe-e-observation-v1'
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
# evidence). workspace:linux:artifact has no host-write telemetry; run_stage
# resource_monitor is sampling-only; sampling is not a hard quota. Named
# unresolved — do not claim the bound is met.
HOST_WRITE = {
    'status': 'missing',
    'cap_bytes': HOST_WRITE_CAP,
    'reason': (
        'SO-host-write: kit §6.3 host writes ≤5 GiB (E9 archive, E14 artifact, evidence) '
        'is a Stage2-wide premise; workspace:linux:artifact has no host-write telemetry; '
        'run_stage resource_monitor is sampling-only and sampling is not a hard quota'
    ),
}


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


def digest(path, role):
    try:
        return sha(path)
    except OSError as exc:
        raise stage.Refusal('input unavailable: ' + role) from exc


def repository_identity():
    roles = {MODULE_ROLE + '/' + name: digest(HERE / name, name) for name in SOURCES}
    for role in CONFIG_ROLES:
        roles[role] = digest(REPO_ROOT / role, role)
    roles[PRODUCER_ROLE] = digest(REPO_ROOT / PRODUCER_ROLE, PRODUCER_ROLE)
    return roles


def runtime_inputs(repository):
    root = REPO_ROOT.resolve(strict=True)
    runtime = {}
    for role, expected in repository.items():
        relative = Path(role)
        if relative.is_absolute() or any(part in ('', '.', '..') for part in relative.parts):
            raise stage.Refusal('invalid repository role')
        try:
            resolved = (REPO_ROOT / relative).resolve(strict=True)
        except OSError as exc:
            raise stage.Refusal('repository input missing: ' + role) from exc
        if resolved != root / relative or not resolved.is_file() or sha(resolved) != expected:
            raise stage.Refusal('repository input differs: ' + role)
        runtime[str(resolved)] = expected
    return runtime


def _toml_value(raw):
    raw = raw.strip()
    if raw in ('true', 'false'):
        return raw == 'true'
    if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in '"\'':
        return raw[1:-1]
    return raw


def policy_identity():
    text = (REPO_ROOT / 'config/containers.toml').read_text()
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


def locate_effigy():
    found = shutil.which('effigy')
    if not found:
        raise stage.Refusal('installed Effigy unavailable')
    path = os.path.realpath(found)
    if not os.path.isfile(path):
        raise stage.Refusal('installed Effigy unavailable')
    return path


def sidecar_path(binary):
    return str(binary) + '.active-version'


def host_active_version(binary):
    path = sidecar_path(binary)
    if not os.path.isfile(path) or os.path.islink(path):
        raise stage.Refusal('host active-version sidecar missing')
    value = Path(path).read_text().strip()
    if not value:
        raise stage.Refusal('host active-version sidecar empty')
    return value, path


def machine_identity(colima, interpreter, effigy, sidecar):
    paths = [colima, interpreter, GIT, effigy, sidecar]
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
    return env


def plan(root, activation_id, head):
    """Read-only prospective bindings; this is not activation authority."""
    if not re.fullmatch(r'[A-Za-z0-9_-]{4,64}', activation_id):
        raise stage.Refusal('invalid activation id')
    if not re.fullmatch(r'[0-9a-f]{40}', head) or head == CANDIDATE_H_HEAD:
        raise stage.Refusal('preparation head must differ from candidate H')
    identity = private_root(root)
    repository = repository_identity()
    policy = policy_identity()
    mount = stash_mount()
    colima = COLIMA
    if not os.path.isabs(colima) or os.path.realpath(colima) != colima or not os.path.isfile(colima):
        raise stage.Refusal('Colima CLI is not at its canonical path')
    interpreter = os.path.realpath(sys.executable)
    if os.path.realpath(GIT) != GIT:
        raise stage.Refusal('git is not at its canonical path')
    effigy = locate_effigy()
    version, sidecar = host_active_version(effigy)
    return {
        'schema': SCHEMA, 'head': head, 'candidate_h': CANDIDATE_H_HEAD,
        'root': root, 'root_identity': identity, 'activation_id': activation_id,
        'duration_seconds': DURATION, 'target': TARGET, 'profile': PROFILE,
        'stash_source_mount': mount, 'selector': SELECTOR, 'policy': policy,
        'host_active_version': version, 'repository': repository,
        'machine': machine_identity(colima, interpreter, effigy, sidecar),
        'program': colima, 'effigy': effigy, 'interpreter': interpreter,
    }


def parse_unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise stage.Refusal('duplicate JSON key')
        value[key] = item
    return value


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


def artifact_paths():
    directory = REPO_ROOT / '.effigy/linux-release/artifacts'
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
    arch = row.get('arch')
    if arch not in ('aarch64', 'arm64'):
        raise stage.Refusal('effigy-release architecture is not aarch64')
    return {'name': PROFILE, 'status': row.get('status'), 'runtime': row.get('runtime'), 'arch': arch}


def parse_receipt(raw):
    fields = {}
    try:
        text = raw.decode('utf-8', 'strict')
    except UnicodeDecodeError as exc:
        raise stage.Refusal('receipt is not UTF-8') from exc
    for line in text.splitlines():
        key, separator, value = line.partition('=')
        if not separator or not key or key in fields:
            raise stage.Refusal('receipt has an unknown or duplicate field')
        fields[key] = value
    for key in ('binary', 'copied_binary', 'release_triple', 'sha256', 'build_kind', 'completed_at'):
        if key not in fields or not fields[key]:
            raise stage.Refusal('receipt is missing ' + key)
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
    data = Path(binary).read_bytes()
    size = len(data)
    if size == 0:
        raise stage.Refusal('artifact is empty')
    if size > ARTIFACT_CAP:
        raise stage.Refusal('artifact exceeds 200MiB')
    digest = hashlib.sha256(data).hexdigest()
    receipt = parse_receipt(Path(receipt_path).read_bytes())
    if receipt['sha256'] != digest:
        raise stage.Refusal('receipt SHA mismatch')
    if receipt['release_triple'] != TARGET:
        raise stage.Refusal('receipt target differs')
    if receipt['build_kind'] != 'workspace-artifact':
        raise stage.Refusal('receipt build_kind differs')
    version = Path(sidecar).read_text().strip()
    if version != expected_version:
        raise stage.Refusal('active-version sidecar differs from host')
    elf = check_elf(data)
    return {
        'sha256': digest, 'size': size, 'elf': elf, 'receipt': receipt,
        'active_version': version, 'binary': str(binary),
        'receipt_path': str(receipt_path), 'sidecar': str(sidecar),
    }


def bound_inputs(request):
    if request != plan(request['root'], request['activation_id'], request['head']):
        raise stage.Refusal('request bindings drifted')
    actual = subprocess.check_output([GIT, '-C', str(HERE), 'rev-parse', 'HEAD'], timeout=10).decode().strip()
    clean = subprocess.check_output([GIT, '-C', str(HERE), 'status', '--porcelain'], timeout=10)
    if clean:
        raise stage.Refusal('preparation checkout is not clean')
    if actual != request['head']:
        raise stage.Refusal('preparation head drifted')
    runtime = runtime_inputs(request['repository'])
    if set(runtime) & set(request['machine']):
        raise stage.Refusal('input alias between checkout and machine')
    return actual, runtime


def execute(request, request_path, request_hash):
    launched_at = time.time()
    deadline = time.monotonic() + DURATION
    actual, runtime = bound_inputs(request)
    if time.monotonic() >= deadline - RESERVE:
        raise stage.Refusal('blocked producer clock')
    if not 0 < PRODUCER_OUTPUT_CAP <= 64 * 1024 * 1024 or not 0 < OBSERVE_CAP <= 64 * 1024 * 1024:
        raise stage.Refusal('zero output cap')
    root = Path(request['root'])
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
        'policy': request['policy'], 'host_active_version': request['host_active_version'],
        'repository_identity': request['repository'], 'machine_identity': request['machine'],
        'checkout_root': str(REPO_ROOT), 'runtime_inputs': runtime,
        'simulation': bool(TEST_SEAM['simulation']),
        'usable_production_predecessor': False,
        'toolchain': {
            'status': 'missing',
            'reason': 'SO-4: owned-builder exec is not a supported E14 interface; cargo -V/rustc -Vv not collected',
        },
        'remote_workload_closure': 'unknown',
        'producer_closure': 'unknown',
        'profile_stop_attempted': False,
        'host_write': dict(HOST_WRITE),
        'findings': [],
    }
    serial = 0

    def remaining(end):
        return end - time.monotonic()

    def call(argv, limit, cap, end, environment):
        nonlocal serial
        if private_root(str(root)) != request['root_identity']:
            raise stage.Refusal('evidence root identity changed')
        budget = min(limit, remaining(end))
        if not 0 < budget <= stage.STAGE_TIMEOUT_CAP:
            raise stage.Refusal('blocked producer clock')
        serial += 1
        step = destination / ('step-%03d' % serial)
        step.mkdir(mode=0o700)
        result = stage.run_stage(step, argv, budget, cap, inputs, cwd=str(REPO_ROOT),
                                 environment=environment, deadline=end,
                                 cleanup_deadline=deadline)
        return result, step

    def good(result):
        return result.get('disposition') == 'exited' and result.get('waited_exit') == 0

    try:
        artifacts_dir, binary, receipt_path, sidecar = artifact_paths()
        existing = [path for path in (binary, receipt_path, sidecar) if os.path.lexists(path)]
        if existing:
            raise stage.Refusal('stale artifact/receipt')
        colima_env = colima_observe_environment()
        colima_argv = [request['program'], 'list', '--json']
        if any(token in ('-p', '--profile', 'start', 'stop', 'delete', 'restart') for token in colima_argv[1:]):
            raise stage.Refusal('Colima observation argv is not inventory-only')
        outcome['colima_argv'] = colima_argv
        outcome['colima_environment'] = colima_env
        result, step = call(colima_argv, OBSERVE_LIMIT, OBSERVE_CAP, operational_end, colima_env)
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
        result, step = call(producer_argv, DURATION, PRODUCER_OUTPUT_CAP, operational_end, producer_env)
        outcome['producer'] = result
        outcome['producer_closure'] = result.get('disposition', 'unknown')
        if result.get('disposition') in ('unknown', 'timeout') or result.get('waited_exit') is None:
            outcome['remote_workload_closure'] = 'unknown'
            raise stage.Refusal('producer closure unknown')
        if not good(result):
            outcome['remote_workload_closure'] = 'unknown'
            raise stage.Refusal('producer nonzero exit')
        actual, runtime = bound_inputs(request)
        outcome['head'] = actual
        outcome['runtime_inputs'] = runtime
        checked = check_artifact(binary, receipt_path, sidecar, request['host_active_version'])
        retained = destination / 'retained'
        retained.mkdir(mode=0o700)
        originals = {}
        for label, source in (('binary', binary), ('receipt', receipt_path), ('active-version', sidecar)):
            originals[label] = sha(source)
            stage.exclusive(retained / Path(source).name, Path(source).read_bytes())
        if sha(binary) != originals['binary'] or sha(receipt_path) != originals['receipt']:
            raise stage.Refusal('producer originals changed while retained')
        actual, runtime = bound_inputs(request)
        outcome['head'] = actual
        outcome['runtime_inputs'] = runtime
        outcome['artifact'] = checked
        outcome['original_hashes'] = originals
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


def main(argv):
    if len(argv) == 5 and argv[1] == 'plan':
        print(json.dumps(plan(argv[2], argv[3], argv[4]), sort_keys=True, indent=2))
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

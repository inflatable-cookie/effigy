"""Fixed Nucleus proof-only Linux diagnostic entry.

External precondition: Nucleus must launch the pinned selector after native GO.
This process cannot authenticate launch provenance against the same user. A
request, this file, or caller environment is not that authorisation.
"""
import contextlib
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
import linux_diagnostic as diagnostic
import linux_diagnostic_probe as guest_probe
import linux_disk_association as disk_association
import linux_disk_inventory as disk_inventory
import linux_engine_inputs as engine_inputs
import linux_native_support as setup
import preparation_stage as stage
import preparation_requests

REPO_ROOT = HERE.parents[2]
MODULE_ROLE = HERE.relative_to(REPO_ROOT).as_posix()
MANIFEST_ROLE = 'config/tasks.toml'
GIT = '/usr/bin/git'
SCRATCH = '/Volumes/Stash/scratch/effigy'
# /usr/local/bin/prlctl is a symlink to the vendor parallels_wrapper, which selects its tool from
# basename($0) and exits 9 for its own name. The fixed route launches the bundle CLI directly with
# the same argv[0] that the wrapper execs; no wrapper bootstrap (inittool2, Parallels Service) runs.
PRLCTL_ALIAS = '/usr/local/bin/prlctl'
PRLCTL = '/Applications/Parallels Desktop.app/Contents/MacOS/prlctl'
PRLCTL_SHA256 = '2f61a1c60db2d51591fb8479a248a982a81658f136f1501a94733b7a81731c57'
SNAPSHOT_SHA = 'd21610ba279b4faf2d7a62c87ef8f1050943fc8b6c072a04c71ce4d0c953e3ad'
PROBE_KEYS = frozenset('uid boot_id system_state systemctl systemd_run systemd_version cgroup2 ssh_host_ed25519_fp sshd_expiry_time_mentions effigy531_passwd_exit effigy531_group_exit effigy531_home docker_group root_source root_fstype root_uuid root_parent root_serial root_model root_size ssh_listen_ipv4'.split())
BOOT_ID = re.compile(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
# Only the exact echo sentinel proves a guest session; failed empty-output probes may retry
# inside the existing readiness window. See linux-setup.md for the vendor contract's limits.
TRANSPORT_SENTINEL = 'linux-diagnostic-transport-ready'
TRANSPORT_WAIT = 20
# A transport attempt starts only when this much of the one shared readiness window remains, so an
# exhausted window is a named exhaustion instead of a client killed by the shared deadline.
TRANSPORT_FLOOR = 5
DURATION = 600
RESERVE = 120
# Cleanup is bounded inside the same operational deadline: one window to observe this bound VM's
# own eligible running state before the one graceful stop, one window to observe its terminal
# state afterwards, and a final reserve kept for these originals. No window adds budget.
PAUSE = 5
STATE_WAIT = 120
RUN_WAIT = 180
READY_WAIT = 120
STOP_OBSERVE = 60
FINAL_RESERVE = 15
SOURCES = ('linux_diagnostic_proof.py', 'linux_diagnostic.py', 'linux_diagnostic_probe.py',
           'linux_setup.py', 'linux_supervisor.py', 'linux_transfer.py', 'linux_receipts.py',
           'linux_capacity_contract.py', 'linux_loop_bound.py',
           'preparation_stage.py', 'preparation_resources.py', 'preparation_observations.py',
           'preparation_requests.py', 'linux_disk_identity.py', 'linux_disk_association.py',
           'linux_disk_inventory.py', 'linux_engine_inputs.py', 'linux-setup-probe.sh')


# Match the largest existing per-step output cap; selector retention adds no unbounded output.
SELECTOR_STREAM_CAP = 262144


class SelectorStream:
    """Write each emitted UTF-8 byte durably before forwarding it to the caller.

    Retention stops at the cap and records overflow; it never reconstructs output from receipts.
    The final stdout JSON intentionally omits its own stream digest. outcome.json holds that
    digest after stdout has finished, avoiding a self-referential hash.
    """
    def __init__(self, path, original, cap=SELECTOR_STREAM_CAP):
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0)
        self.output = os.fdopen(os.open(path, flags, 0o600), 'wb')
        self.path = path
        self.original = original
        self.cap = cap
        self.count = 0
        self.exceeded = False
        self.digest = hashlib.sha256()

    def write(self, text):
        raw = text.encode('utf-8')
        kept = raw[:max(0, self.cap - self.count)].decode('utf-8', 'ignore').encode('utf-8')
        self.exceeded = self.exceeded or len(kept) != len(raw)
        self.output.write(kept)
        self.digest.update(kept)
        self.count += len(kept)
        self.flush()
        # The retained stream covers exactly the bytes exposed to the executor, including when
        # its own tool interface truncates them. A broken output consumer cannot lose originals.
        try:
            self.original.write(kept.decode('utf-8'))
            self.original.flush()
        except (OSError, ValueError):
            pass
        return len(text)

    def flush(self):
        self.output.flush()
        os.fsync(self.output.fileno())

    def close(self):
        self.flush()
        self.output.close()

    def metadata(self):
        return {'path': self.path.name, 'bytes': self.count,
                'sha256': self.digest.hexdigest(), 'cap_bytes': self.cap,
                'complete': not self.exceeded}


class SelectorStreams:
    def __init__(self):
        self.stack = contextlib.ExitStack()
        self.streams = {}
        self.destination = None

    def start(self, destination):
        self.destination = destination
        try:
            for name in ('stdout', 'stderr'):
                stream = SelectorStream(destination / ('selector-' + name), getattr(sys, name))
                self.streams[name] = stream
                self.stack.callback(stream.close)
                redirect = contextlib.redirect_stdout if name == 'stdout' else contextlib.redirect_stderr
                self.stack.enter_context(redirect(stream))
                stream.flush()
            fd = os.open(destination, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(fd)
            finally:
                os.close(fd)
        except BaseException:
            self.stack.close()
            raise

    def close(self):
        self.stack.close()

    def metadata(self):
        return {name: stream.metadata() for name, stream in self.streams.items()}


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
    """Checkout-owned inputs by repository-relative role and exact SHA-256. No checkout path is stored."""
    names = sorted(set(SOURCES) | set(preparation_requests.REQUIRED_MODULE_NAMES))
    roles = {MODULE_ROLE + '/' + name: digest(HERE / name, name) for name in names}
    roles[MANIFEST_ROLE] = digest(REPO_ROOT / MANIFEST_ROLE, MANIFEST_ROLE)
    if roles[MODULE_ROLE + '/linux-setup-probe.sh'] != setup.PROBE_SHA256:
        raise stage.Refusal('reviewed probe differs')
    return roles


def runtime_inputs(repository):
    """Resolve the reviewed roles against this checkout. Aliases, escapes, missing and drifted inputs refuse."""
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


def machine_identity(program, interpreter, effigy):
    """Machine tools keep their canonical paths; the same canonical path must hash the same."""
    paths = [program, interpreter, GIT, effigy]
    if len(set(paths)) != len(paths):
        raise stage.Refusal('machine input alias')
    return {path: digest(path, path) for path in paths}


def plan(root, activation_id, head):
    """Read-only prospective bindings; this is not activation authority.

    Checkout-owned inputs are identified by repository-relative role and content hash, so the
    same reviewed commit validates at any clean detached path. execute resolves the roles in
    its own checkout and passes the actual paths and hashes to every launch.
    """
    if not re.fullmatch(r'[0-9a-f]{64}', SNAPSHOT_SHA):
        raise stage.Refusal('built-in snapshot digest is malformed')
    if not re.fullmatch(r'[A-Za-z0-9_-]{8,64}', activation_id):
        raise stage.Refusal('invalid activation id')
    if not re.fullmatch(r'[0-9a-f]{40}', head) or head == setup.CANDIDATE_H_HEAD:
        raise stage.Refusal('preparation head must differ from candidate H')
    identity = private_root(root)
    repository = repository_identity()
    program = PRLCTL
    if not os.path.isabs(program) or os.path.realpath(program) != program or not os.path.isfile(program):
        raise stage.Refusal('Parallels CLI is not at its canonical bundle path')
    if digest(program, 'Parallels CLI') != PRLCTL_SHA256:
        raise stage.Refusal('Parallels CLI differs from the reviewed build')
    interpreter = os.path.realpath(sys.executable)
    if os.path.realpath(GIT) != GIT:
        raise stage.Refusal('git is not at its canonical path')
    effigy = shutil.which('effigy')
    if not effigy:
        raise stage.Refusal('installed Effigy unavailable')
    effigy = os.path.realpath(effigy)
    return {'schema': 'linux-diagnostic-window-v4', 'head': head,
            'candidate_h': setup.CANDIDATE_H_HEAD, 'root': root, 'root_identity': identity,
            'activation_id': activation_id, 'duration_seconds': DURATION,
            'observation_routes_sha256': guest_probe.routes_digest(),
            'engine_routes_sha256': engine_inputs.routes_digest(),
            'observation_budget_seconds': guest_probe.BUDGET_SECONDS,
            'repository': repository, 'machine': machine_identity(program, interpreter, effigy),
            'program': program, 'interpreter': interpreter,
            'snapshot_sha256': SNAPSHOT_SHA, 'effigy': effigy}


def parse_unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise stage.Refusal('duplicate JSON key')
        value[key] = item
    return value


def association_transport(dispatch, pin=None):
    """The stage's supervised route for the one host inventory read.

    dispatch(argv, timeout, cap) is the occurrence's own recorded-step dispatch: it runs the fixed
    helper through stage.run_stage with the same inputs, environment, resource monitor and stage
    deadlines as every other fixed prlctl call in the same occurrence, and returns (result dict,
    stdout bytes, stderr bytes). The adapter re-checks the fixed argv, stdin, timeout and cap so
    the seam cannot widen, and returns the exact shape the reader's dispatch contract expects."""
    pin = disk_association.PINS[disk_association.MECHANISM] if pin is None else pin

    def transport(argv, stdin_path, timeout, cap):
        if stdin_path is not None:
            raise stage.Refusal('the host inventory read takes no stdin')
        if (list(argv) != pin['argv'] or timeout != disk_inventory.INVENTORY_TIMEOUT
                or cap != disk_inventory.MAX_BYTES):
            raise stage.Refusal('the host inventory read argv or bounds differ from the fixed route')
        result, stdout, stderr = dispatch(list(argv), timeout, cap)
        return {'disposition': result.get('disposition'), 'waited_exit': result.get('waited_exit'),
                'stdout': stdout, 'stderr': stderr}

    return transport


def observe_association(boot_id, read_boot_id, config, descriptor, extents, writer_bound,
                        transport=None, deadline=None):
    """The production disk-association handoff for one already observed boot.

    No caller constant is a reading here. The pinned reader is the supported CLI configuration read
    (prlctl list --info --json through the fixed helper), dispatched in production through the
    stage's supervised route (association_transport over the occurrence's own recorded-step
    dispatch, resource monitor and deadlines); its one bounded read is bracketed by two fresh
    guest boot originals of this same VM and boot; the selected VM UUID and home come from the
    reader's own output; the registered config, descriptor and extent inventory are the originals
    this occurrence retained; and the guest block observations are the writer-bound reads of this
    same boot. The complete record then goes through production setup.verify_disk_association. A
    decided refusal is raised, never softened into a status.

    A dispatch that never reached the helper's own exact refusal is transport, not a finding:
    the same shared bounded policy as the read-only probe routes (a bounded pause between
    attempts inside the one occurrence window) retries it, and an exhausted window returns
    status transport-unavailable with no facts and no finding. Without a deadline there is no
    window to share, so one attempt runs and a transport failure raises as before.
    """
    if not disk_association.reader_executable():
        # Defensive seam: the accessor returns 'none' on the supported CLI route. If a limitation
        # ever returns, this records it instead of spending an occurrence on a known refusal.
        return {'status': 'not attempted: ' + disk_association.reader_limitation(),
                'facts': None, 'result': None}
    rows = writer_bound.get('routes') or []
    if (guest_probe.status_of(rows, 'block-sysfs') != guest_probe.OBSERVED
            or guest_probe.status_of(rows, 'block-lsblk') != guest_probe.OBSERVED):
        return {'status': 'not attempted: the guest block observations did not complete for this boot',
                'facts': None, 'result': None}
    attempts = []
    while True:
        try:
            facts = disk_association.association_facts(
                boot_id=boot_id, read_boot_id=read_boot_id, config=config, descriptor=descriptor,
                extents=extents, block=guest_probe.facts_of(rows, 'block-sysfs'),
                lsblk=guest_probe.facts_of(rows, 'block-lsblk'),
                root=(writer_bound.get('facts') or {}).get('root') or {}, transport=transport)
        except disk_inventory.Refused as exc:
            if exc.phase not in disk_inventory.TRANSPORT_PHASES:
                raise
            attempts.append({'phase': exc.phase, 'reason': exc.reason})
            if deadline is None:
                raise
            if deadline - time.monotonic() < PAUSE + TRANSPORT_FLOOR:
                return {'status': 'transport-unavailable', 'facts': None, 'result': None,
                        'attempts': attempts}
            time.sleep(min(PAUSE, max(0, deadline - time.monotonic())))
            continue
        return {'status': 'verified', 'facts': facts,
                'result': setup.verify_disk_association(facts), 'attempts': attempts}


def stored_association(handoff):
    """The outcome keys one handoff contributes.

    `disk_association` holds the complete fact document (raw originals included), which is exactly
    what historical evidence readers re-verify; the reduced verify result is kept beside it for reading
    and is never the stored record. A handoff that was not attempted stores nothing.
    """
    if not isinstance(handoff, dict) or handoff.get('facts') is None:
        return {'disk_association': None, 'disk_association_result': None}
    return {'disk_association': disk_association.stored_facts(handoff['facts']),
            'disk_association_result': handoff.get('result')}


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


def check_vm(raw, state):
    data = json.loads(raw, object_pairs_hook=parse_unique)
    if not isinstance(data, list) or len(data) != 1:
        raise stage.Refusal('VM observation is incomplete')
    vm = data[0]
    if (vm.get('ID') != setup.VM_UUID or vm.get('State') != state
            or vm.get('Home') != setup.PVM_HOME + '/'
            or vm.get('Home path') != setup.PVM_HOME + '/config.pvs'):
        raise stage.Refusal('VM UUID, home or state differs')
    hardware = vm.get('Hardware', {})
    disks = [v for k, v in hardware.items() if re.fullmatch(r'hdd[0-9]+', k)]
    if len(disks) != 1 or disks[0].get('image') != setup.PVM_HOME + '/harddisk1.hdd' or disks[0].get('enabled') is not True:
        raise stage.Refusal('configured internal disk differs')
    return vm


def check_snapshots(raw, expected=None):
    data = json.loads(raw, object_pairs_hook=parse_unique)
    if hashlib.sha256(canonical(data)).hexdigest() != (SNAPSHOT_SHA if expected is None else expected):
        raise stage.Refusal('snapshot inventory differs from retained original')
    return data


def compare_originals(outcome, before, post):
    """Separate retained content hashes from metadata identities.

    A byte-identical replacement is an identity event, never a content-change
    claim; a changed config is a diagnostic observation and not setup
    compatibility. Descriptor bytes are the extent-inventory binding, so a
    descriptor content change stays fatal.
    """
    for name in ('config', 'descriptor'):
        old, new = before.get(name), post.get(name)
        if old is None or new is None:
            continue
        if (old['device'], old['inode']) != (new['device'], new['inode']):
            outcome['identity_events'].append({
                'object': name, 'source': 'retained-originals',
                'content_identical': old['sha256'] == new['sha256'],
                'previous': {key: old[key] for key in ('device', 'inode', 'uid', 'size')},
                'current': {key: new[key] for key in ('device', 'inode', 'uid', 'size')}})
        if old['sha256'] == new['sha256']:
            continue
        if name == 'descriptor':
            outcome['findings'].append('descriptor content changed during observation')
        else:
            outcome['metadata_differences'].append(
                'config bytes changed during observation: diagnostic observation, not setup compatibility')


def check_probe(raw):
    try:
        lines = raw.decode('utf-8', 'strict').splitlines()
    except UnicodeDecodeError as exc:
        raise stage.Refusal('probe output is not UTF-8') from exc
    fields = {}
    for line in lines:
        key, separator, value = line.partition('=')
        if not separator or key not in PROBE_KEYS or key in fields:
            raise stage.Refusal('probe output has an unknown or duplicate field')
        fields[key] = value
    if set(fields) != PROBE_KEYS or not raw.endswith(b'\n'):
        raise stage.Refusal('probe output is partial')
    if fields['system_state'] != 'running':
        raise stage.Refusal('probe did not observe systemd running')
    if not BOOT_ID.fullmatch(fields['boot_id']):
        raise stage.Refusal('probe boot identity is missing or malformed')
    return fields


def check_boot_id(raw):
    try:
        value = raw.decode('ascii', 'strict')
    except UnicodeDecodeError as exc:
        raise stage.Refusal('boot identity is not ASCII') from exc
    if not value.endswith('\n') or not BOOT_ID.fullmatch(value[:-1]):
        raise stage.Refusal('boot identity is missing or malformed')
    return value[:-1]


def classify_transport(result, stdout, stderr):
    """Classify the fixed echo probe without depending on Parallels' error wording.

    The prlctl(8) exec/DIAGNOSTICS contract requires Tools and reports failures nonzero,
    but does not distinguish session-open from guest failures. Empty stdout with a waited
    failure is only not-ready-yet, never proof of a transient fault or guest readiness.
    """
    if (result.get('disposition') == 'exited' and result.get('waited_exit') == 0
            and not stderr and stdout == (TRANSPORT_SENTINEL + '\n').encode()):
        return 'ready'
    if guest_probe.transport_unavailable(result, stdout, stderr):
        return 'unavailable'
    raise stage.Refusal(
        'guest transport probe is neither the exact sentinel nor a bounded empty-output session '
        'refusal: disposition=%s waited_exit=%s stdout_bytes=%d stderr_bytes=%d'
        % (result.get('disposition'), result.get('waited_exit'), len(stdout), len(stderr)))


def execute(request, request_path, request_hash, selector_streams=None, *, exploratory_snapshot=None):
    launched_at = time.time()
    deadline = time.monotonic() + DURATION
    if request != plan(request['root'], request['activation_id'], request['head']):
        raise stage.Refusal('request bindings drifted')
    actual = subprocess.check_output([GIT, '-C', str(HERE), 'rev-parse', 'HEAD'], timeout=10).decode().strip()
    clean = subprocess.check_output([GIT, '-C', str(HERE), 'status', '--porcelain'], timeout=10)
    if clean and exploratory_snapshot is None:
        raise stage.Refusal('preparation checkout is not clean')
    if actual != request['head']:
        raise stage.Refusal('preparation head drifted')
    runtime = runtime_inputs(request['repository'])
    if set(runtime) & set(request['machine']):
        raise stage.Refusal('input alias between checkout and machine')
    inputs = dict(runtime)
    inputs.update(request['machine'])
    if time.monotonic() >= deadline - RESERVE - 120:
        raise stage.Refusal('insufficient time to start and reserve graceful stop')
    root = Path(request['root'])
    claim = root / ('linux-diagnostic-' + request['activation_id'] + '.claim')
    destination = root / ('linux-diagnostic-' + request['activation_id'])
    if os.path.lexists(destination):
        raise stage.Refusal('evidence destination already exists')
    stage.exclusive(claim, canonical({'request_sha256': request_hash, 'launched_at': launched_at,
                                      'duration_seconds': DURATION}) + b'\n')
    fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)
    destination.mkdir(mode=0o700)
    if selector_streams is not None:
        selector_streams.start(destination)
    operational_end = deadline - RESERVE
    start_end = operational_end - 120
    outcome = {'schema': 'linux-diagnostic-observation-v7',
               'exploratory': exploratory_snapshot is not None,
               'qualification_credit': False,
               'snapshot_sha256': exploratory_snapshot or SNAPSHOT_SHA, 'request_sha256': request_hash,
               'head': actual, 'candidate_h': setup.CANDIDATE_H_HEAD, 'launched_at': launched_at,
               'writable_top': 'unknown', 'guest_association': 'unknown',
               'disk_association': None, 'disk_association_result': None,
               'readiness': 'not evaluated', 'readiness_observation': None,
               'transport': {'sentinel': TRANSPORT_SENTINEL, 'status': 'not attempted',
                             'attempts': []},
               'boot_binding': {'status': 'not established', 'boot_id': None,
                                'route_window_start': None, 'route_window_end': None},
               'placement': 'not evaluated',
               'hard_quota': 'not established', 'vm_terminal': 'unknown',
               'launcher_closure_is_vm_closure': False, 'start_count': 0, 'stop_count': 0,
               'stop_eligibility': 'unknown', 'findings': [],
               'metadata_accounting': 'sampled conservative charges; not a hard quota',
               'ancillary_ownership': 'observed (stat only); no trusted input or ownership claim',
               'metadata_differences': [], 'identity_events': [], 'metadata_events': [],
               'observation_failures': [],
               'guest_writer_bound': {'schema': 'linux-writer-bound-observation-v1',
                                      'status': 'not attempted',
                                      'reason': 'the boot-complete probe or the writer-bound dispatch did not complete',
                                      'routes_sha256': guest_probe.routes_digest(), 'routes': [],
                                      'facts': {}, 'limits': dict(guest_probe.LIMITS),
                                      'complete': False, 'findings': []},
               'guest_engine_inputs': {'schema': engine_inputs.SCHEMA,
                                       'status': 'not attempted',
                                       'reason': 'the boot-complete probe or the engine-input dispatch did not complete',
                                       'routes_sha256': engine_inputs.routes_digest(), 'routes': [],
                                       'facts': {}, 'limits': dict(engine_inputs.LIMITS),
                                       'complete': False, 'findings': []},
               'repository_identity': request['repository'], 'machine_identity': request['machine'],
               'checkout_root': str(REPO_ROOT), 'runtime_inputs': runtime}
    serial = 0
    start_launched = False
    monitor = None
    inputs[request_path] = request_hash

    class StopMonitor:
        def check(self):
            try:
                return monitor.check()
            except Exception as exc:
                finding = 'stop accounting: ' + str(exc)
                if finding not in outcome['findings']:
                    outcome['findings'].append(finding)
                return {'status': 'unknown'}

    stop_monitor = StopMonitor()

    def call(args, limit, cap, end, track=True):
        nonlocal serial, start_launched
        setup_route = None
        delivered_route = None
        if args[:2] == ['exec', setup.VM_UUID] and args[2:4] == ['/bin/sh', '-c']:
            setup_route = {'id': 'setup-probe', 'script': (HERE / 'linux-setup-probe.sh').read_text()}
            if args[4:] == [setup_route['script']]:
                args = args[:2] + guest_probe.route_argv(setup_route)
                delivered_route = setup_route
            else:
                setup_route = None
                for route in (*guest_probe.ROUTES, *engine_inputs.ROUTES):
                    if args[2:] == guest_probe.route_argv(route):
                        delivered_route = route
                        break
                if delivered_route is None:
                    raise stage.Refusal('guest script is not a fixed delivered program')
        if private_root(str(root)) != request['root_identity']:
            raise stage.Refusal('evidence root identity changed')
        serial += 1
        step = destination / ('step-%03d' % serial)
        step.mkdir(mode=0o700)
        if delivered_route is not None:
            stage.record(step / 'guest-program-intent.json', {
                'route': delivered_route['id'], 'interpreter': '/bin/sh',
                'script_sha256': hashlib.sha256(guest_probe.delivered_program(delivered_route).encode()).hexdigest(),
                'source_script_sha256': hashlib.sha256(delivered_route['script'].encode()).hexdigest(),
                'delivered_command_bytes': len(' '.join(args[2:]).encode()),
                'delivered_command_cap_bytes': guest_probe.MAX_COMMAND_BYTES})
            fd = os.open(step, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(fd)
            finally:
                os.close(fd)
        try:
            result = stage.run_stage(step, [request['program']] + args, limit, cap, inputs,
                                     guest=bool(args and args[0] == 'exec'),
                                     cwd=str(HERE), environment={'PATH': '/usr/bin:/bin', 'LANG': 'C'},
                                     resource_monitor=monitor if track else (stop_monitor if monitor else None), deadline=end,
                                     cleanup_deadline=operational_end)
        except Exception:
            # The primitive retains a receipt even if it raises after spawning. A
            # recorded process is evidence that this exact start was launched.
            if args == ['start', setup.VM_UUID]:
                try:
                    receipt = json.loads((step / 'receipt.json').read_bytes())
                    if receipt.get('process'):
                        start_launched = True
                        outcome['start'] = receipt
                except (OSError, ValueError, AttributeError):
                    pass
            raise
        if args == ['start', setup.VM_UUID]:
            start_launched = bool(result.get('process'))
            outcome['start'] = result
        result['stderr_text'] = (step / 'stderr').read_bytes().decode('utf-8', 'replace')
        raw = (step / 'stdout').read_bytes()
        if setup_route is not None and result.get('waited_exit') == 0:
            try:
                raw = guest_probe.unwrap_program(setup_route, result, raw)
            except guest_probe.Bad as exc:
                raise stage.Refusal('setup probe program incomplete: ' + str(exc)) from exc
        return result, raw

    def read_only_route(argv, timeout, cap, end):
        # Only these fixed observation callers enter here. Start/stop/setup never do.
        # The caller derives this shared absolute window once. A route timeout bounds each
        # attempt, not a fresh retry window; all later routes spend the same remaining time.
        route_end = end
        remaining = cap
        attempts = []
        while True:
            result, raw = call(['exec', setup.VM_UUID] + argv,
                               min(timeout, max(0, route_end - time.monotonic())),
                               remaining, route_end)
            streams = result_streams()
            stderr = streams['stderr']
            attempts.append({'step': 'step-%03d' % serial,
                             'disposition': result.get('disposition'),
                             'waited_exit': result.get('waited_exit'),
                             'stdout': {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()},
                             'stderr': {'bytes': len(stderr), 'sha256': hashlib.sha256(stderr).hexdigest(),
                                        'text': stderr[:2048].decode('utf-8', 'replace')}})
            remaining -= len(raw) + len(stderr)
            # Retry only the empty nonzero read shape, within the shared deadline.
            try:
                retry = classify_transport(result, raw, stderr) == 'unavailable'
            except stage.Refusal:
                retry = False
            if (not retry or remaining < 1
                    or route_end - time.monotonic() < PAUSE + TRANSPORT_FLOOR):
                break
            if private_root(str(root)) != request['root_identity']:
                raise stage.Refusal('evidence root identity changed')
            pause(route_end)
            # Accounting during the pause can spend the remaining window too. Preserve the
            # last waited refusal instead of trying to launch after the shared end.
            if route_end - time.monotonic() < TRANSPORT_FLOOR:
                break
        result['attempts'] = attempts
        if retry:
            result['status'] = guest_probe.TRANSPORT_UNAVAILABLE
            result['stderr_classes'] = sorted({row['stderr']['text'] for row in attempts})
        return result, raw

    def pause(end, track=True):
        until = min(end, time.monotonic() + PAUSE)
        while time.monotonic() < until:
            if monitor is not None:
                (monitor if track else stop_monitor).check()
            time.sleep(min(0.25, PAUSE, max(0, until - time.monotonic())))

    def result_streams():
        """The stdout/stderr originals the last `call` retained, read back as evidence."""
        step = destination / ('step-%03d' % serial)
        return {name: (step / name).read_bytes() for name in ('stdout', 'stderr')}

    def good(result):
        return result.get('disposition') == 'exited' and result.get('waited_exit') == 0

    home = Path(setup.PVM_HOME)
    disk = home / setup.DISK_DIR
    files = {'config': home / setup.CONFIG_NAME, 'descriptor': disk / setup.DESCRIPTOR_NAME}
    try:
        if home.resolve() != home or disk.resolve() != disk:
            raise stage.Refusal('package or disk directory is not canonical')
        outcome['package_identity'] = setup._owned_dir(home)
        outcome['disk_identity'] = setup._owned_dir(disk)
        before, errors = diagnostic.retain_originals(destination, files, 'before')
        outcome['before'] = before
        if errors:
            raise stage.Refusal('original retention failed: ' + '; '.join(errors))
        descriptor = (destination / 'before-descriptor').read_bytes()
        extents = diagnostic.inventory(disk, descriptor)
        outcome['before_extents'] = extents
        monitor = diagnostic.ExtentMonitor(home, disk, descriptor, extents,
                                           bound=[files['config']],
                                           ancillary_ownership=diagnostic.OBSERVED)
        outcome['before_metadata'] = monitor.package_initial
        result, raw = call(['list', '--info', setup.VM_UUID, '--json'], 10, 65536, start_end)
        if not good(result):
            raise stage.Refusal('pre-observation failed')
        check_vm(raw, 'stopped')
        result, raw = call(['snapshot-list', setup.VM_UUID, '--json'], 10, 262144, start_end)
        if not good(result):
            raise stage.Refusal('pre-snapshot observation failed')
        check_snapshots(raw, exploratory_snapshot or SNAPSHOT_SHA)
        outcome['start_count'] = 1
        result, _ = call(['start', setup.VM_UUID], 145, 16384, start_end)
        if not good(result):
            raise stage.Refusal('start result is not a positively owned successful start')
        running = False
        run_end = min(start_end, time.monotonic() + RUN_WAIT)
        while time.monotonic() < run_end:
            result, raw = call(['list', '--info', setup.VM_UUID, '--json'], 10, 65536, start_end)
            if good(result):
                try:
                    check_vm(raw, 'running')
                    running = True
                    break
                except stage.Refusal:
                    pass
            pause(run_end)
        if not running:
            raise stage.Refusal('running observation unknown')
        # One absolute readiness window covers guest transport, systemd's wait and the first
        # boot-id read; no retry or per-call deadline can extend the 600s occurrence.
        ready_end = min(start_end, time.monotonic() + READY_WAIT)
        while ready_end - time.monotonic() >= TRANSPORT_FLOOR:
            result, raw = call(['exec', setup.VM_UUID, '/bin/echo', TRANSPORT_SENTINEL],
                               min(TRANSPORT_WAIT, ready_end - time.monotonic()), 2048, ready_end)
            streams = result_streams()
            attempt = {'step': 'step-%03d' % serial, 'disposition': result.get('disposition'),
                       'waited_exit': result.get('waited_exit'),
                       'elapsed_seconds': result.get('elapsed_seconds'),
                       'stdout': {'bytes': len(raw),
                                  'sha256': hashlib.sha256(raw).hexdigest()},
                       'stderr': {'bytes': len(streams['stderr']),
                                  'sha256': hashlib.sha256(streams['stderr']).hexdigest(),
                                  'text': streams['stderr'][:2048].decode('utf-8', 'replace')}}
            try:
                kind = classify_transport(result, raw, streams['stderr'])
            except stage.Refusal:
                outcome['transport']['attempts'].append(dict(attempt, classification='refused'))
                outcome['transport']['status'] = 'refused'
                raise
            outcome['transport']['attempts'].append(dict(attempt, classification=kind))
            if kind == 'ready':
                outcome['transport']['status'] = 'ready'
                break
            pause(ready_end)
        if outcome['transport']['status'] != 'ready':
            outcome['transport']['status'] = 'exhausted'
            outcome['transport']['stderr_classes'] = sorted({
                row['stderr']['text'] for row in outcome['transport']['attempts']})
            raise stage.Refusal(
                'guest transport readiness unknown: %d attempt(s) in the one shared readiness '
                'deadline without the exact echo sentinel; stderr classes: %s'
                % (len(outcome['transport']['attempts']),
                   repr(outcome['transport']['stderr_classes'])))
        result, raw = read_only_route(
            ['/usr/bin/systemctl', 'is-system-running', '--wait'], READY_WAIT, 1024, ready_end)
        outcome['readiness_observation'] = result
        try:
            outcome['readiness'] = setup.stage1_accept_readiness(
                dict(result, stdout=raw))
        except setup.Refusal as exc:
            raise stage.Refusal('systemd boot readiness refused: ' + str(exc)) from exc
        if raw != b'running\n' or not good(result):
            raise stage.Refusal('systemd boot readiness output is not exact running/exit 0')
        boot_result, boot_raw = read_only_route(
            ['/bin/cat', '/proc/sys/kernel/random/boot_id'], 20, 2048, ready_end)
        if not good(boot_result):
            raise stage.Refusal('boot identity after readiness was not positively observed')
        boot_id = check_boot_id(boot_raw)
        outcome['boot_binding']['boot_id'] = boot_id
        probe = (HERE / 'linux-setup-probe.sh').read_text()
        if hashlib.sha256(probe.encode()).hexdigest() != setup.PROBE_SHA256:
            raise stage.Refusal('probe drift')
        result, raw = read_only_route(['/bin/sh', '-c', probe], 75, 262144, start_end)
        outcome['probe'] = result
        if not good(result):
            raise stage.Refusal('probe incomplete')
        outcome['probe_fields'] = check_probe(raw)
        if outcome['probe_fields']['boot_id'] != boot_id:
            raise stage.Refusal('probe boot identity differs from readiness')
        # The writer-bound capability reads share this boot-complete occurrence and its
        # deadlines. They are observations only: no field here feeds writable_top, readiness,
        # placement, hard_quota or any setup gate. The one disk-association handoff below reads
        # the guest block observations of this same boot and nothing else from them.
        def guard():
            if private_root(str(root)) != request['root_identity']:
                raise stage.Refusal('evidence root identity changed')

        def retain_observation(name, data):
            path = destination / ('observation-' + name + '.raw')
            stage.exclusive(path, data)
            return path.name

        def read_guest_boot():
            """One fresh guest boot original; a partial or failed read is not an identity."""
            result, raw = read_only_route(
                ['/bin/cat', '/proc/sys/kernel/random/boot_id'], 20, 2048, start_end)
            if not good(result):
                raise stage.Refusal('guest boot identity read did not complete')
            return check_boot_id(raw)

        before_routes = read_guest_boot()
        if before_routes != boot_id:
            raise stage.Refusal('boot identity before writer routes differs from readiness')
        outcome['boot_binding']['route_window_start'] = boot_id
        writer_bound = guest_probe.observe(
            read_only_route,
            retain_observation, guard, start_end)
        # The engine-input routes share this boot-complete occurrence, its one boot bracket and
        # its per-route limits: artifact identity pins and packaged launch metadata for the
        # accepted 783 §3.4 route. They observe nothing running, execute no engine binary and
        # contact no daemon; they feed no gate here, and no process ancestry exists in them.
        engine_bound = engine_inputs.observe(
            read_only_route,
            retain_observation, guard, start_end)
        # The disk-association handoff: the supported CLI configuration read, dispatched through
        # this occurrence's own supervised route (recorded step, inputs, environment, resource
        # monitor and stage deadlines, exactly like every other fixed prlctl call here), and
        # bracketed by two fresh guest boot originals of this same boot. The writer-bound guest
        # block reads must have completed for this same boot, and the complete fact document goes
        # through production setup.verify_disk_association; that document is what the stored
        # `disk_association` key holds, so historical readers re-verify the same facts and the
        # retained raw originals rather than a reduced summary.
        def association_dispatch(argv, timeout, cap):
            nonlocal serial
            if private_root(str(root)) != request['root_identity']:
                raise stage.Refusal('evidence root identity changed')
            serial += 1
            step = destination / ('step-%03d' % serial)
            step.mkdir(mode=0o700)
            result = stage.run_stage(step, argv, timeout, cap, inputs, cwd=str(HERE),
                                     environment={'PATH': '/usr/bin:/bin', 'LANG': 'C'},
                                     resource_monitor=monitor, deadline=start_end,
                                     cleanup_deadline=operational_end)
            return result, (step / 'stdout').read_bytes(), (step / 'stderr').read_bytes()

        try:
            handoff = observe_association(
                boot_id, read_guest_boot,
                (destination / 'before-config').read_bytes(),
                (destination / 'before-descriptor').read_bytes(),
                outcome.get('before_extents') or {}, writer_bound,
                transport=association_transport(association_dispatch),
                deadline=start_end)
            outcome['guest_association'] = handoff['status']
            # A transport-unavailable handoff carries no facts and no finding: the
            # inventory route never reached a decided refusal inside the window.
            if handoff.get('attempts'):
                outcome['disk_association_attempts'] = handoff['attempts']
            outcome.update(stored_association(handoff))
        except (stage.Refusal, disk_association.Refusal, disk_inventory.Refused, OSError) as exc:
            outcome['guest_association'] = 'refused: ' + str(exc)
            outcome['findings'].append('disk association: ' + str(exc))
        after_routes = read_guest_boot()
        if after_routes != boot_id:
            writer_bound['complete'] = False
            writer_bound['findings'].append('boot identity changed during writer route window')
            engine_bound['complete'] = False
            engine_bound['findings'].append('boot identity changed during the engine-input route window')
            outcome['findings'].append('writer route window boot identity differs from readiness')
        else:
            outcome['boot_binding']['route_window_end'] = boot_id
            outcome['boot_binding']['status'] = 'same boot observed'
        writer_bound['status'] = 'observed' if writer_bound['complete'] else 'partial'
        engine_bound['status'] = 'observed' if engine_bound['complete'] else 'partial'
        outcome['guest_writer_bound'] = writer_bound
        outcome['guest_engine_inputs'] = engine_bound
        outcome['findings'].extend(writer_bound['findings'])
        outcome['findings'].extend(engine_bound['findings'])
    except Exception as exc:
        observation = getattr(exc, 'observation', None)
        if observation is not None and not any(
                tuple(row.get(key) for key in ('phase', 'operation', 'path', 'errno'))
                == tuple(observation.get(key) for key in ('phase', 'operation', 'path', 'errno'))
                for row in (monitor.observation_failures if monitor else [])):
            # A refusal that names an object keeps that object's observed identity even when no
            # live sample recorded it: the fact that explains the refusal is evidence.
            outcome['observation_failures'].append(dict(observation, attempt=0))
        outcome['findings'].append(type(exc).__name__ + ': ' + str(exc))
    finally:
        if start_launched:
            # A stop sent while Parallels still reports this VM starting is refused and
            # closes nothing (the actual 712 failure). Observe this same bound VM's own
            # state first, inside the existing operational deadline, and send the one
            # graceful stop only when eligible running state is positively observed.
            eligible = False
            state_end = min(operational_end - STOP_OBSERVE - FINAL_RESERVE,
                            time.monotonic() + STATE_WAIT)
            budget = state_end > time.monotonic()
            try:
                while time.monotonic() < state_end:
                    result, raw = call(['list', '--info', setup.VM_UUID, '--json'], 10, 65536, state_end, track=False)
                    if good(result):
                        try:
                            check_vm(raw, 'running')
                            eligible = True
                            break
                        except stage.Refusal:
                            try:
                                check_vm(raw, 'stopped')
                                outcome['vm_terminal'] = 'stopped'
                                outcome['stop'] = {'command': 'not sent', 'observed': 'stopped'}
                                break
                            except stage.Refusal:
                                pass
                    pause(state_end, track=False)
            except Exception as exc:
                outcome['findings'].append('pre-stop state observation: ' + str(exc))
            outcome['stop_eligibility'] = 'running' if eligible else (
                'stopped' if outcome['vm_terminal'] == 'stopped' else 'unknown')
            if eligible:
                try:
                    outcome['stop_count'] = 1
                    result, _ = call(['stop', setup.VM_UUID], 120, 16384, operational_end, track=False)
                    outcome['stop'] = result
                    if not good(result):
                        outcome['findings'].append('graceful stop did not complete successfully')
                except Exception as exc:
                    outcome['findings'].append('stop observation: ' + str(exc))
                # Waited command facts stay separate from eventual state: a later stopped
                # observation establishes terminal state only, never a closed stop client
                # or a repaired observation.
                terminal_end = min(operational_end - FINAL_RESERVE, time.monotonic() + STOP_OBSERVE)
                try:
                    while time.monotonic() < terminal_end:
                        result, raw = call(['list', '--info', setup.VM_UUID, '--json'], 10, 65536, terminal_end, track=False)
                        if good(result):
                            try:
                                check_vm(raw, 'stopped')
                                outcome['vm_terminal'] = 'stopped'
                                break
                            except stage.Refusal:
                                pass
                        pause(terminal_end, track=False)
                except Exception as exc:
                    outcome['findings'].append('terminal state observation: ' + str(exc))
            elif outcome['vm_terminal'] == 'stopped':
                # Observed, not claimed as a stop: this VM reached stopped without this entry
                # executing a graceful stop, so the observation is not a complete
                # boot/probe/graceful-stop handoff and must not read as successful.
                outcome['findings'].append(
                    'bound VM was already stopped; no graceful stop command was executed')
            elif budget:
                outcome['findings'].append(
                    'eligible running state not observed in the cleanup window; graceful stop not attempted')
            else:
                outcome['findings'].append(
                    'no cleanup budget remained for an eligible running observation; graceful stop not attempted')
            if outcome['vm_terminal'] != 'stopped':
                outcome['findings'].append('terminal VM state unknown')
            try:
                result, raw = call(['snapshot-list', setup.VM_UUID, '--json'], 10, 262144, operational_end, track=False)
                if not good(result):
                    raise stage.Refusal('post-snapshot observation failed')
                check_snapshots(raw, exploratory_snapshot or SNAPSHOT_SHA)
            except Exception as exc:
                outcome['findings'].append('post snapshots: ' + str(exc))
        elif outcome['start_count']:
            outcome['findings'].append('start launch not confirmed; terminal VM state unknown and stop not attempted')
        post, errors = diagnostic.retain_originals(destination, files, 'post')
        outcome['post'] = post
        outcome['findings'].extend(errors)
        try:
            if 'descriptor' in post:
                outcome['post_extents'] = diagnostic.inventory(disk, (destination / 'post-descriptor').read_bytes())
        except Exception as exc:
            outcome['findings'].append('post inventory: ' + str(exc))
        compare_originals(outcome, outcome.get('before', {}), post)
        try:
            if outcome.get('package_identity') != setup._owned_dir(home) or outcome.get('disk_identity') != setup._owned_dir(disk):
                outcome['findings'].append('package or disk identity changed')
        except Exception as exc:
            outcome['findings'].append('package identity: ' + str(exc))
        if monitor:
            try:
                outcome['final_growth'] = monitor.check()
                outcome['post_extents'] = monitor.last
                outcome['post_metadata'] = dict(monitor.ancillary.held)
            except Exception as exc:
                outcome['findings'].append('final accounting: ' + str(exc))
            outcome['metadata_events'] = monitor.ancillary.events
            outcome['identity_events'].extend(monitor.identity_events)
            # Every failed live attempt stays here with its phase, operation, path and errno.
            if monitor.observation_failures:
                outcome['observation_failures'] = monitor.observation_failures
        outcome['finished_at'] = time.time()
        outcome['probe_sha256'] = setup.PROBE_SHA256
        if selector_streams is None:
            stage.record(destination / 'outcome.json', outcome)
    return outcome


def main(argv):
    if len(argv) == 5 and argv[1] == 'plan':
        print(json.dumps(plan(argv[2], argv[3], argv[4]), sort_keys=True, indent=2))
        return 0
    if len(argv) != 3:
        raise stage.Refusal('proof entry requires a request and SHA-256')
    request = read_request(argv[1], argv[2])
    streams = SelectorStreams()
    result = None
    try:
        result = execute(request, argv[1], argv[2], selector_streams=streams)
        print(json.dumps(result, sort_keys=True))
    except (stage.Refusal, OSError, ValueError, KeyError) as exc:
        if streams.destination is None:
            raise
        print('refused: ' + str(exc), file=sys.stderr)
        return 2
    finally:
        streams.close()
        if result is not None:
            result['selector_streams'] = streams.metadata()
            if any(not item['complete'] for item in result['selector_streams'].values()):
                result['findings'].append('selector output exceeded its retained stream cap')
            stage.record(streams.destination / 'outcome.json', result)
    return 0 if not result['findings'] and result['vm_terminal'] == 'stopped' else 1


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except (stage.Refusal, OSError, ValueError, KeyError) as exc:
        print('refused: ' + str(exc), file=sys.stderr)
        sys.exit(2)

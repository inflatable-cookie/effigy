"""Fixed Nucleus proof-only Linux diagnostic entry.

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
import linux_diagnostic as diagnostic
import linux_setup as setup
import preparation_stage as stage

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
SOURCES = ('linux_diagnostic_proof.py', 'linux_diagnostic.py', 'linux_setup.py',
           'linux_supervisor.py', 'linux_transfer.py', 'linux_receipts.py',
           'preparation_stage.py', 'preparation_resources.py', 'preparation_observations.py',
           'preparation_requests.py', 'linux_disk_identity.py', 'linux-setup-probe.sh')


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
    names = sorted(set(SOURCES) | set(setup.REQUIRED_MODULE_NAMES))
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
    return {'schema': 'linux-diagnostic-window-v2', 'head': head,
            'candidate_h': setup.CANDIDATE_H_HEAD, 'root': root, 'root_identity': identity,
            'activation_id': activation_id, 'duration_seconds': DURATION,
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


def check_snapshots(raw):
    data = json.loads(raw, object_pairs_hook=parse_unique)
    if hashlib.sha256(canonical(data)).hexdigest() != SNAPSHOT_SHA:
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
    return fields


def execute(request, request_path, request_hash):
    launched_at = time.time()
    deadline = time.monotonic() + DURATION
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
    operational_end = deadline - RESERVE
    start_end = operational_end - 120
    outcome = {'schema': 'linux-diagnostic-observation-v3', 'request_sha256': request_hash,
               'head': actual, 'candidate_h': setup.CANDIDATE_H_HEAD, 'launched_at': launched_at,
               'writable_top': 'unknown', 'guest_association': 'unknown',
               'readiness': 'not evaluated', 'placement': 'not evaluated',
               'hard_quota': 'not established', 'vm_terminal': 'unknown',
               'launcher_closure_is_vm_closure': False, 'start_count': 0, 'stop_count': 0,
               'stop_eligibility': 'unknown', 'findings': [],
               'metadata_accounting': 'sampled conservative charges; not a hard quota',
               'metadata_differences': [], 'identity_events': [], 'metadata_events': [],
               'observation_failures': [],
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
        if private_root(str(root)) != request['root_identity']:
            raise stage.Refusal('evidence root identity changed')
        serial += 1
        step = destination / ('step-%03d' % serial)
        step.mkdir(mode=0o700)
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
        return result, (step / 'stdout').read_bytes()

    def pause(end, track=True):
        until = min(end, time.monotonic() + PAUSE)
        while time.monotonic() < until:
            if monitor is not None:
                (monitor if track else stop_monitor).check()
            time.sleep(min(0.25, PAUSE, max(0, until - time.monotonic())))

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
        monitor = diagnostic.ExtentMonitor(home, disk, descriptor, extents)
        outcome['before_metadata'] = monitor.package_initial
        result, raw = call(['list', '--info', setup.VM_UUID, '--json'], 10, 65536, start_end)
        if not good(result):
            raise stage.Refusal('pre-observation failed')
        check_vm(raw, 'stopped')
        result, raw = call(['snapshot-list', setup.VM_UUID, '--json'], 10, 262144, start_end)
        if not good(result):
            raise stage.Refusal('pre-snapshot observation failed')
        check_snapshots(raw)
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
        ready = False
        ready_end = min(start_end, time.monotonic() + READY_WAIT)
        while time.monotonic() < ready_end:
            result, raw = call(['exec', setup.VM_UUID, '/bin/echo', 'ready'], 20, 1024, start_end)
            if good(result) and raw == b'ready\n':
                ready = True
                break
            pause(ready_end)
        if not ready:
            raise stage.Refusal('bootstrap transport readiness unknown')
        probe = (HERE / 'linux-setup-probe.sh').read_text()
        if hashlib.sha256(probe.encode()).hexdigest() != setup.PROBE_SHA256:
            raise stage.Refusal('probe drift')
        result, raw = call(['exec', setup.VM_UUID, '/bin/sh', '-c', probe], 75, 262144, start_end)
        outcome['probe'] = result
        if not good(result):
            raise stage.Refusal('probe incomplete')
        outcome['probe_fields'] = check_probe(raw)
    except Exception as exc:
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
                check_snapshots(raw)
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
            outcome['observation_failures'] = monitor.observation_failures
        outcome['finished_at'] = time.time()
        outcome['probe_sha256'] = setup.PROBE_SHA256
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
    return 0 if not result['findings'] and result['vm_terminal'] == 'stopped' else 1


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except (stage.Refusal, OSError, ValueError, KeyError) as exc:
        print('refused: ' + str(exc), file=sys.stderr)
        sys.exit(2)

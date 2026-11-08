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

SCRATCH = '/Volumes/Stash/scratch/effigy'
PRLCTL = '/usr/local/bin/prlctl'
SNAPSHOT_SHA = 'd21610ba279b4faf2d7a62c87ef8f1050943fc9c072a04c71ce4d0c953e3ad'
PROBE_KEYS = frozenset('uid boot_id system_state systemctl systemd_run systemd_version cgroup2 ssh_host_ed25519_fp sshd_expiry_time_mentions effigy531_passwd_exit effigy531_group_exit effigy531_home docker_group root_source root_fstype root_uuid root_parent root_serial root_model root_size ssh_listen_ipv4'.split())
DURATION = 600
RESERVE = 120
SOURCES = ('linux_diagnostic_proof.py', 'linux_diagnostic.py', 'linux_setup.py',
           'linux_supervisor.py', 'linux_transfer.py', 'linux_receipts.py',
           'preparation_stage.py', 'preparation_resources.py', 'preparation_observations.py',
           'preparation_requests.py', 'linux-setup-probe.sh')


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


def plan(root, activation_id, head):
    """Read-only prospective bindings; this is not activation authority."""
    if not re.fullmatch(r'[A-Za-z0-9_-]{8,64}', activation_id):
        raise stage.Refusal('invalid activation id')
    if not re.fullmatch(r'[0-9a-f]{40}', head) or head == setup.CANDIDATE_H_HEAD:
        raise stage.Refusal('preparation head must differ from candidate H')
    identity = private_root(root)
    names = set(SOURCES) | set(setup.REQUIRED_MODULE_NAMES)
    inputs = {str(HERE / name): sha(HERE / name) for name in sorted(names)}
    if inputs[str(HERE / 'linux-setup-probe.sh')] != setup.PROBE_SHA256:
        raise stage.Refusal('reviewed probe differs')
    program = os.path.realpath(PRLCTL)
    if not os.path.isabs(program) or not os.path.isfile(program):
        raise stage.Refusal('Parallels program unavailable')
    inputs[program] = sha(program)
    interpreter = os.path.realpath(sys.executable)
    inputs[interpreter] = sha(interpreter)
    inputs['/usr/bin/git'] = sha('/usr/bin/git')
    effigy = shutil.which('effigy')
    if not effigy:
        raise stage.Refusal('installed Effigy unavailable')
    effigy = os.path.realpath(effigy)
    inputs[effigy] = sha(effigy)
    manifest = HERE.parents[2] / 'config' / 'tasks.toml'
    inputs[str(manifest)] = sha(manifest)
    return {'schema': 'linux-diagnostic-window-v1', 'head': head,
            'candidate_h': setup.CANDIDATE_H_HEAD, 'root': root, 'root_identity': identity,
            'activation_id': activation_id, 'duration_seconds': DURATION,
            'inputs': inputs, 'program': program, 'interpreter': interpreter,
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
    git = '/usr/bin/git'
    actual = subprocess.check_output([git, '-C', str(HERE), 'rev-parse', 'HEAD'], timeout=10).decode().strip()
    clean = subprocess.check_output([git, '-C', str(HERE), 'status', '--porcelain'], timeout=10)
    if clean:
        raise stage.Refusal('preparation checkout is not clean')
    if actual != request['head']:
        raise stage.Refusal('preparation head drifted')
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
    outcome = {'schema': 'linux-diagnostic-observation-v1', 'request_sha256': request_hash,
               'head': actual, 'candidate_h': setup.CANDIDATE_H_HEAD, 'launched_at': launched_at,
               'writable_top': 'unknown', 'guest_association': 'unknown',
               'readiness': 'not evaluated', 'placement': 'not evaluated',
               'hard_quota': 'not established', 'vm_terminal': 'unknown',
               'launcher_closure_is_vm_closure': False, 'start_count': 0, 'findings': []}
    serial = 0
    monitor = None
    inputs = dict(request['inputs'])
    inputs[request_path] = request_hash

    def call(args, limit, cap, end, track=True):
        nonlocal serial
        if private_root(str(root)) != request['root_identity']:
            raise stage.Refusal('evidence root identity changed')
        serial += 1
        step = destination / ('step-%03d' % serial)
        step.mkdir(mode=0o700)
        result = stage.run_stage(step, [request['program']] + args, limit, cap, inputs,
                                 guest=bool(args and args[0] == 'exec'),
                                 cwd=str(HERE), environment={'PATH': '/usr/bin:/bin', 'LANG': 'C'},
                                 resource_monitor=monitor if track else None, deadline=end,
                                 cleanup_deadline=operational_end)
        return result, (step / 'stdout').read_bytes()

    def pause(end, track=True):
        until = min(end, time.monotonic() + 5)
        while time.monotonic() < until:
            if track and monitor is not None:
                monitor.check()
            time.sleep(min(0.25, max(0, until - time.monotonic())))

    def good(result):
        return result.get('disposition') == 'exited' and result.get('waited_exit') == 0

    owned_start = False
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
        outcome['start'] = result
        owned_start = result.get('waited_exit') == 0
        if not good(result):
            raise stage.Refusal('start result is not a positively owned successful start')
        owned_start = True
        running = False
        while time.monotonic() < start_end:
            result, raw = call(['list', '--info', setup.VM_UUID, '--json'], 10, 65536, start_end)
            if good(result):
                try:
                    check_vm(raw, 'running')
                    running = True
                    break
                except stage.Refusal:
                    pass
            pause(start_end)
        if not running:
            raise stage.Refusal('running observation unknown')
        ready = False
        while time.monotonic() < start_end:
            result, raw = call(['exec', setup.VM_UUID, '/bin/echo', 'ready'], 20, 1024, start_end)
            if good(result) and raw == b'ready\n':
                ready = True
                break
            pause(start_end)
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
        if owned_start:
            try:
                result, _ = call(['stop', setup.VM_UUID], 120, 16384, operational_end, track=False)
                outcome['stop'] = result
                if not good(result):
                    outcome['findings'].append('graceful stop did not complete successfully')
                if good(result):
                    while time.monotonic() < operational_end:
                        result, raw = call(['list', '--info', setup.VM_UUID, '--json'], 10, 65536, operational_end, track=False)
                        if good(result):
                            try:
                                check_vm(raw, 'stopped')
                                outcome['vm_terminal'] = 'stopped'
                                break
                            except stage.Refusal:
                                pass
                        pause(operational_end, track=False)
            except Exception as exc:
                outcome['findings'].append('stop observation: ' + str(exc))
            if outcome['vm_terminal'] != 'stopped':
                outcome['findings'].append('terminal VM state unknown')
            try:
                result, raw = call(['snapshot-list', setup.VM_UUID, '--json'], 10, 262144, operational_end, track=False)
                if not good(result):
                    raise stage.Refusal('post-snapshot observation failed')
                check_snapshots(raw)
            except Exception as exc:
                outcome['findings'].append('post snapshots: ' + str(exc))
        post, errors = diagnostic.retain_originals(destination, files, 'post')
        outcome['post'] = post
        outcome['findings'].extend(errors)
        try:
            if 'descriptor' in post:
                outcome['post_extents'] = diagnostic.inventory(disk, (destination / 'post-descriptor').read_bytes())
        except Exception as exc:
            outcome['findings'].append('post inventory: ' + str(exc))
        for name in ('config', 'descriptor'):
            if name in outcome.get('before', {}) and name in post and outcome['before'][name] != post[name]:
                outcome['findings'].append(name + ' changed')
        try:
            if outcome.get('package_identity') != setup._owned_dir(home) or outcome.get('disk_identity') != setup._owned_dir(disk):
                outcome['findings'].append('package or disk identity changed')
        except Exception as exc:
            outcome['findings'].append('package identity: ' + str(exc))
        try:
            if monitor:
                outcome['final_growth'] = monitor.check()
                outcome['post_extents'] = monitor.last
        except Exception as exc:
            outcome['findings'].append('final accounting: ' + str(exc))
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

"""Run a fresh unsealed Linux session under an owned, bounded guest service.

The asset seed describes real prebuilt native artifacts. Every attempt creates
new inputs, a new fixture and a new service; earlier failed attempts stay intact.
"""
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common
import linux_diagnostic_proof as diagnostic
import linux_native_build as build
import linux_native_session as session
import linux_setup as setup

FILES = ('common.py', 'runtime.py', 'probe.py', 'linux_native_session.py', 'linux_native_window.py', 'linux_native_storage.py', 'linux_loop_bound.py')


def submit_unit(attempt, argv, unit):
    observe = 'systemctl show ' + unit + '.service -p LoadState'
    common.require(attempt.admin(observe, read_only=True).strip() == b'LoadState=not-found',
                   'fresh unit already exists')
    for trial in range(4):
        try:
            attempt.admin(shlex.join(argv))
            return
        except RuntimeError:
            state = attempt.admin(observe, read_only=True).strip()
            if state == b'LoadState=loaded':
                return
            common.require(state == b'LoadState=not-found' and trial < 3,
                           'unit submission remains unknown or unavailable')
            # This is the sole repeatable mutation: systemd atomically owns
            # the unique name. RemainAfterExit + CollectMode=inactive retain
            # both successful and failed units. A late duplicate submission
            # therefore refuses rather than executing a second payload.
            time.sleep(5)


def stop_owned_unit(attempt, target, output_root):
    """Latch one caller-owned unit stop; independently require empty custody."""
    import re
    common.require(re.fullmatch(r'effigy-829-[a-z0-9-]+', target), 'invalid owned unit identifier')
    unit = 'effigy-829-quiesce-' + uuid.uuid4().hex
    streams = {name: str(Path(output_root) / (unit + '.' + name)) for name in ('stdout', 'stderr')}
    argv = ['systemd-run', '--unit=' + unit, '--no-block', '--service-type=exec',
            '--property=RemainAfterExit=yes', '--property=CollectMode=inactive',
            '--property=RuntimeMaxSec=45', '--property=TimeoutStopSec=5',
            '--property=MemoryMax=64M', '--property=MemorySwapMax=0', '--property=TasksMax=16',
            '--property=LimitFSIZE=1M', '--property=LimitCORE=0',
            '--property=StandardOutput=append:' + streams['stdout'],
            '--property=StandardError=append:' + streams['stderr'],
            '--', '/usr/bin/systemctl', 'stop', target + '.service']
    submit_unit(attempt, argv, unit)
    deadline = time.monotonic() + 65
    while True:
        raw = attempt.admin('systemctl show --all ' + unit + '.service '
                            '--property=MainPID,SubState,Result,ExecMainStatus', read_only=True)
        state = dict(line.split('=', 1) for line in raw.decode().splitlines())
        if state.get('MainPID') == '0' and state.get('SubState') in ('exited', 'failed', 'dead'):
            break
        common.require(time.monotonic() < deadline, 'latched unit stop has no terminal observation')
        time.sleep(.2)
    common.exclusive(attempt.root / (unit + '.terminal.json'), state)
    for name, path in streams.items():
        common.exclusive_bytes(attempt.root / (unit + '.' + name),
                               attempt.admin('cat ' + shlex.quote(path), read_only=True))
    common.require(state['Result'] == 'success' and state['ExecMainStatus'] == '0',
                   'latched stop failed; retained streams and unit identity')
    raw = attempt.admin('systemctl show --all ' + target + '.service '
                        '--property=ActiveState,MainPID,ControlGroup', read_only=True)
    state = dict(line.split('=', 1) for line in raw.decode().splitlines())
    code = ('import json; from pathlib import Path; p=Path('
            + repr('/sys/fs/cgroup/system.slice/' + target + '.service') + '); '
            'print(json.dumps([int(n) for f in p.rglob("cgroup.procs") for n in f.read_text().split()]))')
    pids = json.loads(attempt.admin('python3 -I -B -c ' + shlex.quote(code), read_only=True))
    common.exclusive(attempt.root / (unit + '.owner-terminal.json'), {'target': target, 'state': state, 'pids': pids})
    common.require(state.get('ActiveState') in ('inactive', 'failed') and state.get('MainPID') == '0'
                   and state.get('ControlGroup') == '' and pids == [], 'owned unit did not close')
    # Keep the completed zero-process latch loaded. Its name cannot be replayed
    # after an ambiguous acknowledgement; no process remains to clean up.
    return state, pids


class Attempt:
    def __init__(self, root, *, label='native-session'):
        diagnostic.private_root(str(root))
        self.root = root / (label + '-' + uuid.uuid4().hex)
        self.root.mkdir(mode=0o700)
        self.count = 0

    def command(self, argv, *, data=None, stdin=None, timeout=30, read_only=False):
        for trial in range(12 if read_only else 1):
            self.count += 1
            stem = self.root / ('command-%04d' % self.count)
            intent = {'argv': [str(arg) for arg in argv], 'timeout_seconds': timeout,
                      'started_at': time.time(), 'read_only': read_only,
                      'stdin_file': str(stdin.name) if stdin is not None else None,
                      'input_sha256': hashlib.sha256(data).hexdigest() if data is not None else None}
            common.exclusive(stem.with_suffix('.intent.json'), intent)
            with stem.with_suffix('.stdout').open('xb') as out, stem.with_suffix('.stderr').open('xb') as err:
                child = subprocess.Popen(argv, stdin=subprocess.PIPE if data is not None else (stdin if stdin is not None else subprocess.DEVNULL),
                                         stdout=out, stderr=err)
                common.exclusive(stem.with_suffix('.pid.json'), {'local_pid': child.pid})
                try:
                    child.communicate(data, timeout=timeout)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.communicate()
                    common.exclusive(stem.with_suffix('.terminal.json'), {
                        'local_exit': child.returncode, 'timeout': True, 'remote_closure': 'unknown'})
                    raise RuntimeError('native client timed out: ' + str(stem))
            common.exclusive(stem.with_suffix('.terminal.json'), {
                'local_exit': child.returncode, 'elapsed_seconds': time.time() - intent['started_at']})
            stdout = stem.with_suffix('.stdout').read_bytes()
            if child.returncode == 0:
                return stdout
            if not read_only or stdout or trial == 11:
                raise RuntimeError('native command failed: ' + str(stem))
            time.sleep(5)

    def admin(self, script, *, read_only=False):
        # Parallels re-parses guest arguments. Retain one explicitly quoted
        # command, and refuse oversized transport before any guest effect.
        argv = [diagnostic.PRLCTL, 'exec', setup.VM_UUID, '/bin/sh', '-c', shlex.quote(script)]
        common.require(len(' '.join(argv[3:]).encode()) <= 2891, 'Parallels command exceeds observed safe cap')
        return self.command(argv, timeout=45, read_only=read_only)


def execute(root, seed, mode):
    common.require(mode in ('acquire', 'preflight', 'full'), 'explicit native session mode required')
    attempt = Attempt(root)
    raw = attempt.command([diagnostic.PRLCTL, 'list', '--info', setup.VM_UUID, '--json'], read_only=True)
    diagnostic.check_vm(raw, 'running')
    control = root / 'control'
    diagnostic.private_root(str(control))
    ssh = common.isolated_ssh_prefix('effigy-531-vault', build.file_input(control / 'ssh_config'),
                                    build.file_input(control / 'ssh_known_hosts'), control, attempt.root)
    key = attempt.command([diagnostic.PRLCTL, 'exec', setup.VM_UUID, '/bin/cat',
                           '/etc/ssh/ssh_host_ed25519_key.pub'], read_only=True).decode().split()
    known = (control / 'ssh_known_hosts').read_text().split()
    common.require(len(key) >= 2 and len(known) >= 3 and key[:2] == known[1:3], 'fixed VM host key changed')
    value = json.loads(seed.read_bytes())
    common.require(value.get('schema') == 'effigy.linux-native-session-exploration.v1'
                   and value.get('task_id') == session.TASK and value.get('exploratory') is True
                   and value.get('qualification_credit') is False, 'unsealed task-owned asset seed required')
    value['cell']['native_writer_gate'] = uuid.uuid4().hex if mode == 'full' else None
    common.exclusive(attempt.root / 'seed.json', value)
    driver = str(session.BASE / ('driver-' + attempt.root.name))
    attempt.command(ssh + ['umask 077; mkdir ' + shlex.quote(driver)])
    files = {}
    for name in FILES:
        data = (Path(__file__).parent / name).read_bytes()
        files[name] = hashlib.sha256(data).hexdigest()
        attempt.command(ssh + ['umask 077; set -C; cat > ' + shlex.quote(driver + '/' + name)], data=data)
    # The remote account binds actual inode/device/uid, never caller-provided
    # guesses. Existing artifacts and baseline remain pinned by the seed.
    script = ('import json,os,sys,tempfile,hashlib; from pathlib import Path; '
              'v=json.load(sys.stdin); q=Path(tempfile.mkdtemp(prefix="' + mode + '-",dir='
              + repr(str(session.BASE / 'qualification')) + ')); i=q.stat(); '
              'v["cell"]["qualification_root"]={"path":str(q),"device":i.st_dev,"inode":i.st_ino,"uid":i.st_uid}; '
              'p=Path(' + repr(str(session.BASE / 'metadata' / (attempt.root.name + '.json'))) + '); '
              'd=json.dumps(v,sort_keys=True,indent=2).encode(); '
              'f=os.open(p,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o400); os.write(f,d); os.fsync(f); os.close(f); '
              'print(json.dumps({"path":str(p),"sha256":hashlib.sha256(d).hexdigest(),"fixture":str(q)}))')
    binding = json.loads(attempt.command(ssh + ['/usr/bin/python3 -I -B -c ' + shlex.quote(script)],
                                         data=json.dumps(value).encode()))
    unit = 'effigy-829-' + uuid.uuid4().hex
    # The controller's retained stream is a writer too: keep its pre-opened
    # output descriptor on the same physically capped workspace filesystem.
    log = str(session.BASE / 'qualification' / (unit + '.log'))
    writable = str(session.BASE if mode in ('acquire', 'preflight') else Path(binding['fixture']))
    argv = ['/usr/bin/systemd-run', '--unit=' + unit, '--no-block', '--service-type=exec',
            '--uid=effigy531', '--gid=effigy531', '--property=ExitType=cgroup',
            '--property=RemainAfterExit=yes', '--property=KillMode=control-group',
            '--property=CollectMode=inactive',
            '--property=RuntimeMaxSec=1850', '--property=TimeoutStopSec=15',
            '--property=PrivateUsers=identity', '--property=NoNewPrivileges=yes',
            '--property=CapabilityBoundingSet=', '--property=ProtectSystem=strict',
            '--property=ProtectHome=yes', '--property=BindReadOnlyPaths=/dev /sys',
            '--property=DevicePolicy=closed',
            '--property=ProtectKernelTunables=yes', '--property=ProtectKernelModules=yes',
            '--property=ProtectKernelLogs=yes', '--property=ReadWritePaths=' + writable,
            '--property=MemoryMax=3G', '--property=MemorySwapMax=0', '--property=LimitCORE=0',
            '--property=StandardOutput=append:' + log, '--property=StandardError=append:' + log,
            '--setenv=PATH=' + str(session.BASE / 'tools/bin') + ':/usr/bin:/bin',
            '--setenv=HOME=/home/effigy531', '--', '/usr/bin/python3', '-I', '-B',
            driver + '/linux_native_session.py', binding['path'], binding['sha256'], mode]
    binding.update(unit=unit, log=log, driver=driver, files=files, unit_argv=argv,
                   exploratory=True, qualification_credit=False, mode=mode)
    common.exclusive(attempt.root / 'binding.json', binding)
    submit_unit(attempt, argv, unit)
    deadline = time.monotonic() + 1900
    properties = 'LoadState,ActiveState,SubState,Result,ExecMainStatus,MainPID,ControlGroup,InvocationID,ExecMainExitTimestampMonotonic'
    terminal = None
    observed_gates = set()
    observation_failure = None
    while time.monotonic() < deadline:
        raw = attempt.admin('systemctl show --all ' + unit + '.service --property=' + properties, read_only=True)
        status = dict(line.split('=', 1) for line in raw.decode().splitlines())
        if (status.get('MainPID') == '0' and status.get('ControlGroup')
                and int(status.get('ExecMainExitTimestampMonotonic', '0')) > 0):
            # The assertions have returned. Dispose only this attempt's exact
            # service, including any intentionally retained fixture helpers.
            # Preserve the pre-stop state; this is cleanup, not proof that a
            # running session settled naturally. Its journal checks ran first.
            common.exclusive(attempt.root / 'post-main-cgroup.json', status)
            stop_owned_unit(attempt, unit, session.BASE / 'qualification')
            continue
        if (status.get('MainPID') == '0' and status.get('ControlGroup') == ''
                and status.get('ActiveState') in ('active', 'failed', 'inactive')
                and status.get('SubState') != 'running'):
            terminal = status
            break
        if mode == 'full' and status.get('MainPID') != '0':
            # Each committed up pauses at its unique gate. Observe the exact
            # live owning cohort before allowing the native assertions onward.
            code = ('import json; from pathlib import Path; p=Path(' + repr(binding['fixture'])
                    + '); print(json.dumps(sorted(str(f) for f in p.glob("writer-gate-*.request.json"))))')
            gates = json.loads(attempt.admin('python3 -I -B -c ' + shlex.quote(code), read_only=True))
            for gate in gates:
                if gate in observed_gates:
                    continue
                import linux_native_smoke as smoke
                try:
                    original = smoke.root_collect(root, attempt, ssh, unit)
                    common.require(original['writer_closure']['complete'], 'live native writer closure refused')
                except Exception as error:
                    observation_failure = str(error)
                    common.exclusive(attempt.root / 'writer-observation-failed.json', {'error': observation_failure})
                    stop_owned_unit(attempt, unit, session.BASE / 'qualification')
                    break
                acknowledgement = gate.removesuffix('.request.json') + '.observed.json'
                data = json.dumps({'nonce': value['cell']['native_writer_gate'], 'complete': True}).encode()
                attempt.command(ssh + ['set -C; cat > ' + shlex.quote(acknowledgement)], data=data)
                observed_gates.add(gate)
        time.sleep(1)
    if terminal is None:
        stop_owned_unit(attempt, unit, session.BASE / 'qualification')
        raise RuntimeError('owned native service exceeded deadline; stopped exact unit, no success claim')
    common.exclusive(attempt.root / 'terminal.json', terminal)
    log_data = attempt.command(ssh + ['cat ' + shlex.quote(log)], read_only=True)
    (attempt.root / 'guest.log').write_bytes(log_data)
    # Retain the entire exact fixture, including failed command streams. No
    # deletion or rebaseline is performed by the exploratory runner.
    archive = attempt.command(ssh + ['tar -C ' + shlex.quote(binding['fixture']) + ' -cf - .'],
                              timeout=180, read_only=True)
    (attempt.root / 'fixture.tar').write_bytes(archive)
    passed = (observation_failure is None and terminal.get('Result') == 'success'
              and terminal.get('ExecMainStatus') == '0')
    result = dict(binding, passed=passed, terminal=terminal, observation_failure=observation_failure)
    common.exclusive(attempt.root / 'outcome.json', result)
    print(json.dumps({'evidence': str(attempt.root), 'passed': passed, 'qualification_credit': False}))
    return 0 if passed else 1


if __name__ == '__main__':
    if len(sys.argv) != 4:
        raise SystemExit('usage: qualification:linux:native-session -- EVIDENCE_ROOT ASSET_SEED acquire|preflight|full')
    sys.exit(execute(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]))

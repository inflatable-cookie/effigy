"""Unsealed native exploration. Never supplies a qualification receipt or proof credit.

Requires the caller's explicit authority over the fixed VM. The sealed entry still
uses its reviewed snapshot pin; this entry binds the current inventory for one run.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import uuid
import shlex
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_diagnostic_proof as diagnostic
import linux_setup as setup
import common
import linux_native_assets as assets
import linux_native_build as build
import linux_native_run as native_run


def diagnostic_run(root):
    root = str(root)
    diagnostic.private_root(root)
    snapshots = subprocess.run(
        [diagnostic.PRLCTL, 'snapshot-list', setup.VM_UUID, '--json'],
        capture_output=True, timeout=10, check=True)
    pin = hashlib.sha256(diagnostic.canonical(json.loads(
        snapshots.stdout, object_pairs_hook=diagnostic.parse_unique))).hexdigest()
    head = subprocess.check_output([diagnostic.GIT, 'rev-parse', 'HEAD'], timeout=10).decode().strip()
    request = diagnostic.plan(root, 'explore-' + uuid.uuid4().hex, head)
    raw = diagnostic.canonical(request)
    request_path = Path(root) / (request['activation_id'] + '.request.json')
    diagnostic.stage.exclusive(request_path, raw)
    request_path.chmod(0o400)
    result = diagnostic.execute(request, str(request_path), hashlib.sha256(raw).hexdigest(),
                                exploratory_snapshot=pin)
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if not result['findings'] and result['vm_terminal'] == 'stopped' else 1


def ssh_prefix(root, attempt):
    control = root / 'control'
    return common.isolated_ssh_prefix('effigy-531-vault', build.file_input(control / 'ssh_config'),
                                     build.file_input(control / 'ssh_known_hosts'), control, attempt.root)


def root_operation(root, attempt, ssh, entry, files, arguments=(), *, runtime_seconds=45, file_limit='8M'):
    remote = build.BASE + '/metadata/smoke-' + uuid.uuid4().hex
    attempt.command(ssh + ['umask 077; mkdir ' + shlex.quote(remote)])
    pins = {}
    for name in files:
        data = (Path(__file__).parent / name).read_bytes()
        pins[name] = hashlib.sha256(data).hexdigest()
        attempt.command(ssh + ['umask 077; set -C; cat > ' + shlex.quote(remote + '/' + name)], data=data)
    # Copy verified bytes into a fresh root-controlled directory before import.
    # The mutable account directory is never on root Python's import path.
    code = ('import os,hashlib,tempfile,runpy; from pathlib import Path; '
            'p=Path("/var/lib/effigy-829/helpers"); p.mkdir(mode=0o700,exist_ok=True); '
            'assert p.resolve()==p and p.stat().st_uid==0 and p.stat().st_mode&0o777==0o700; '
            'd=Path(tempfile.mkdtemp(prefix="smoke-",dir=p)); '
            'pins=' + repr(pins) + '; '
            'data={n:(Path(' + repr(remote) + ')/n).read_bytes() for n in pins}; '
            'assert all(hashlib.sha256(b).hexdigest()==pins[n] for n,b in data.items()); '
            '[(d/n).write_bytes(b) for n,b in data.items()]; '
            'import sys; sys.argv=[' + repr(entry) + ']+ ' + repr(list(arguments)) + '; '
            'runpy.run_path(str(d/' + repr(entry) + '),run_name="__main__")')
    payload = code.encode()
    bootstrap = remote + '/root-bootstrap.py'
    attempt.command(ssh + ['umask 077; set -C; cat > ' + shlex.quote(bootstrap)], data=payload)
    unit = 'effigy-829-resume-' + uuid.uuid4().hex
    stdout, stderr = ('/var/lib/effigy-829/' + unit + suffix for suffix in ('.stdout.json', '.stderr.log'))
    # Authenticate the bootstrap bytes in memory. Its account-owned directory
    # is never imported by root. One retained unit name is the execution latch,
    # so an ambiguous Parallels acknowledgement cannot replay mount/start work.
    entry = ('import hashlib; from pathlib import Path; p=' + repr(bootstrap)
             + '; b=Path(p).read_bytes(); assert hashlib.sha256(b).hexdigest()=='
             + repr(hashlib.sha256(payload).hexdigest())
             + '; exec(compile(b,p,"exec"),{"__name__":"__main__"})')
    argv = ['/usr/bin/systemd-run', '--unit=' + unit, '--no-block', '--service-type=exec',
            '--property=RemainAfterExit=yes', '--property=CollectMode=inactive',
            '--property=ExitType=cgroup', '--property=KillMode=control-group',
            '--property=RuntimeMaxSec=' + str(runtime_seconds), '--property=TimeoutStopSec=5',
            '--property=MemoryMax=256M', '--property=MemorySwapMax=0', '--property=TasksMax=64',
            '--property=LimitFSIZE=' + file_limit, '--property=UMask=0077', '--property=LimitCORE=0',
            '--property=StandardOutput=append:' + stdout, '--property=StandardError=append:' + stderr,
            '--', '/usr/bin/python3', '-I', '-B', '-c', entry]
    native_run.submit_unit(attempt, argv, unit)
    deadline = time.monotonic() + runtime_seconds + 20
    while True:
        raw = attempt.admin('systemctl show --all ' + unit + '.service '
                            '--property=MainPID,SubState,Result,ExecMainStatus,ControlGroup', read_only=True)
        state = dict(line.split('=', 1) for line in raw.decode().splitlines())
        if state.get('MainPID') == '0' and state.get('SubState') in ('exited', 'failed', 'dead'):
            break
        common.require(time.monotonic() < deadline, 'root resume unit did not finish; retain exact owner')
        time.sleep(.2)
    common.exclusive(attempt.root / (unit + '.terminal.json'), state)
    output = attempt.admin('cat ' + shlex.quote(stdout), read_only=True)
    errors = attempt.admin('cat ' + shlex.quote(stderr), read_only=True)
    common.exclusive_bytes(attempt.root / (unit + '.stdout'), output)
    common.exclusive_bytes(attempt.root / (unit + '.stderr'), errors)
    common.require(state['Result'] == 'success' and state['ExecMainStatus'] == '0',
                   'root helper failed; retained unit and streams')
    # This exact, completed helper is ours. Its daemon is in a distinct named
    # unit and remains available for the next native step.
    common.require(state.get('ControlGroup') == '', 'completed root helper retains a cgroup')
    result = json.loads(output)
    common.exclusive(attempt.root / ('resume-' + uuid.uuid4().hex + '.json'), result)
    return result


def root_resume(root, attempt, ssh):
    files = ('linux_loop_bound.py', 'linux_native_storage.py', 'linux_native_writers.py', 'linux_native_resume.py')
    result = root_operation(root, attempt, ssh, 'linux_native_resume.py', files)
    common.require(result['writer_closure']['complete'],
                   'native writer closure refused; inspect the retained actors and findings')
    return result


def root_collect(root, attempt, ssh, controller=None):
    files = ('linux_loop_bound.py', 'linux_native_storage.py', 'linux_native_writers.py',
             'linux_native_resume.py', 'linux_native_collect.py')
    arguments = () if controller is None else (controller,)
    return root_operation(root, attempt, ssh, 'linux_native_collect.py', files, arguments)



def initialize_images(root, seed):
    """Explicit unsealed activation; preserve every byte and the owned rollback."""
    attempt = native_run.Attempt(root, label='native-image-initialization')
    raw = attempt.command([diagnostic.PRLCTL, 'list', '--info', setup.VM_UUID, '--json'], read_only=True)
    diagnostic.check_vm(raw, 'running')
    snapshots = json.loads(attempt.command([diagnostic.PRLCTL, 'snapshot-list', setup.VM_UUID, '--json'], read_only=True),
                           object_pairs_hook=diagnostic.parse_unique)
    # Parallels' observed JSON indexes snapshots by braced UUID; the ID is not
    # a field inside an entry. Bind that exact owned entry, without recursion
    # through unrelated snapshots or accepting an ID mentioned as a parent.
    owned = snapshots.get('{d1602e5a-964a-4375-9eec-df6ed4350305}') if isinstance(snapshots, dict) else None
    common.require(isinstance(owned, dict) and owned.get('name') == 'effigy-explore-2026-11-09',
                   'task-owned pre-change snapshot is absent or differs')
    common.exclusive(attempt.root / 'snapshot-original.json', snapshots)
    value = common.load(seed)
    common.require(value.get('schema') == 'effigy.linux-native-session-exploration.v1'
                   and value.get('exploratory') is True and value.get('qualification_credit') is False,
                   'unsealed owned asset seed required')
    cell = value['cell']
    common.require(cell['engine_identity'] == 'effigy-829-rootful' and cell['baseline']['containers'] == [],
                   'only the idle dedicated owned engine may be initialized')
    ssh = ssh_prefix(root, attempt)
    actual = attempt.command(ssh + ['sha256sum -- ' + shlex.quote(cell['engine_binary']['path'])], read_only=True)
    common.require(actual.decode().split()[0] == cell['engine_binary']['sha256'], 'engine executable changed')
    engine = shlex.join([cell['engine_binary']['path'], '--config', cell['docker_config'],
                         '--context', cell['engine_identity'], 'ps', '-a', '--no-trunc', '--format', '{{.ID}}'])
    common.require(attempt.command(ssh + [engine], read_only=True).strip() == b'',
                   'dedicated engine still has containers; settle their exact owners first')
    stop_engine(attempt)
    files = ('linux_loop_bound.py', 'linux_native_storage.py', 'linux_native_writers.py',
             'linux_native_resume.py', 'linux_native_initialize.py')
    result = root_operation(root, attempt, ssh, 'linux_native_initialize.py', files,
                            runtime_seconds=300, file_limit='2G')
    common.exclusive(attempt.root / 'initialized.json', result)
    print(json.dumps({'evidence': str(attempt.root), 'initialized': True, 'exploratory': True,
                      'qualification_credit': False}))
    return 0


def verify_engine_stopped(state, pids):
    common.require(state == {'ActiveState': 'inactive', 'MainPID': '0', 'ControlGroup': ''}
                   and pids == [], 'engine did not close; retain the exact owner')


def stop_engine(attempt):
    state, pids = native_run.stop_owned_unit(attempt, 'effigy-829-rootful',
                                              '/var/lib/effigy-829/rootful')
    verify_engine_stopped(state, pids)


def capacity_probe(attempt, ssh, root, size, *, image=None):
    payload = (Path(__file__).parent / 'linux_native_capacity.py').read_bytes()
    argv = '/usr/bin/python3 -I -B - ' + shlex.quote(root) + ' ' + str(size)
    if image:
        argv = ('env DOCKER_CONFIG=/var/lib/effigy-829/rootful/client/config /usr/bin/docker '
                '--context effigy-829-rootful run --rm -i --network none --user 1001:1001 '
                '--volume ' + shlex.quote(root + ':' + root) + ' ' + shlex.quote(image) + ' ' + argv)
    result = json.loads(attempt.command(ssh + [argv], data=payload, timeout=120))
    common.require(result['errno'] == 28 and result['retained_empty_inode']
                   and (not image or result['seccomp'] == '2'), 'physical ENOSPC control differs')
    common.exclusive(attempt.root / ('capacity-' + uuid.uuid4().hex + '.json'), result)


def full_chain(root, seed):
    diagnostic.private_root(str(root))
    value = common.load(seed)
    common.require(value.get('schema') == 'effigy.linux-native-session-exploration.v1'
                   and value.get('qualification_credit') is False, 'exploratory native asset seed required')
    attempt = native_run.Attempt(root)
    ssh = ssh_prefix(root, attempt)
    # Completed smoke runs leave this VM stopped. An interrupted run is kept
    # for diagnosis and must be reconciled before beginning another window.
    raw = attempt.command([diagnostic.PRLCTL, 'list', '--info', setup.VM_UUID, '--json'], read_only=True)
    diagnostic.check_vm(raw, 'stopped')
    common.require(diagnostic_run(root) == 0, 'native diagnostic has findings')
    attempt.command([diagnostic.PRLCTL, 'start', setup.VM_UUID], timeout=90)
    attempt.admin('cat /proc/sys/kernel/random/boot_id', read_only=True)
    key = attempt.command([diagnostic.PRLCTL, 'exec', setup.VM_UUID, '/bin/cat',
                           '/etc/ssh/ssh_host_ed25519_key.pub'], read_only=True).decode().split()
    known = (root / 'control/ssh_known_hosts').read_text().split()
    common.require(len(key) >= 2 and key[:2] == known[1:3], 'fixed VM SSH identity differs')
    root_resume(root, attempt, ssh)
    stop_engine(attempt)
    capacity_probe(attempt, ssh, '/var/lib/effigy-829/rootful/client/tmp', 2147483648)
    root_resume(root, attempt, ssh)
    capacity_probe(attempt, ssh, build.BASE + '/qualification', 1073741824,
                   image=value['cell']['runtime_image'])
    binding = build.build(root, 'HEAD')
    value['cell']['source_head'] = binding['source_pin']
    prepared = assets.prepare(root, binding, value)
    common.require(native_run.execute(root, prepared, 'acquire') == 0, 'native acquisition failed')
    common.require(native_run.execute(root, prepared, 'full') == 0, 'native full session failed')
    final = root_resume(root, attempt, ssh)
    stop_engine(attempt)
    attempt.command([diagnostic.PRLCTL, 'stop', setup.VM_UUID], timeout=90)
    raw = attempt.command([diagnostic.PRLCTL, 'list', '--info', setup.VM_UUID, '--json'], read_only=True)
    diagnostic.check_vm(raw, 'stopped')
    result = {'exploratory': True, 'qualification_credit': False, 'source_head': binding['source_pin'],
              'assets': str(prepared), 'final_physical_bounds': final, 'vm_terminal': 'stopped',
              'chain': ['diagnostic', 'resume', 'engine-enospc', 'workspace-enospc',
                        'native-build', 'gnu-artifact', 'ordinary-acquisition', 'full-session']}
    common.exclusive(attempt.root / 'native-smoke-complete.json', result)
    print(json.dumps(result, indent=2))
    return 0


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('evidence_root', type=Path)
    parser.add_argument('asset_seed', type=Path, nargs='?')
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--diagnostic-only', action='store_true')
    mode.add_argument('--initialize-images', action='store_true')
    args = parser.parse_args(argv[1:])
    root = args.evidence_root.resolve(strict=True)
    if args.diagnostic_only:
        common.require(args.asset_seed is None, 'diagnostic-only does not accept a session seed')
        return diagnostic_run(root)
    if args.initialize_images:
        common.require(args.asset_seed is not None, 'image initialization requires the prepared owned seed')
        return initialize_images(root, args.asset_seed)
    common.require(args.asset_seed is not None, 'full native smoke requires the prepared asset seed; see linux-setup.md')
    return full_chain(root, args.asset_seed)


if __name__ == '__main__':
    sys.exit(main(sys.argv))

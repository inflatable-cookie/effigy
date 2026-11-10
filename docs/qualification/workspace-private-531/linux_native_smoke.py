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
import linux_native_support as setup
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


def root_operation(root, attempt, ssh, entry, files, arguments=(), *, runtime_seconds=45, file_limit='8M', expected=None, memory_limit='256M'):
    import base64
    data = {name: base64.b64encode((Path(__file__).parent / name).read_bytes()).decode() for name in files}
    pins = {name: common.digest(Path(__file__).parent / name) for name in files}
    code = ('import os,hashlib,tempfile,runpy,base64,sys; from pathlib import Path; '
            'p=Path("/var/lib/effigy-829/helpers"); '
            'assert p.resolve()==p and p.stat().st_uid==0 and p.stat().st_mode&0o777==0o700; '
            'd=Path(tempfile.mkdtemp(prefix="modules-",dir=p)); '
            'data=' + repr(data) + '; pins=' + repr(pins) + '; '
            'decoded={n:base64.b64decode(b) for n,b in data.items()}; '
            'assert all(hashlib.sha256(b).hexdigest()==pins[n] for n,b in decoded.items()); '
            '[(d/n).write_bytes(b) for n,b in decoded.items()]; '
            'sys.argv=[' + repr(entry) + ']+' + repr(list(arguments)) + '; '
            'runpy.run_path(str(d/' + repr(entry) + '),run_name="__main__")')
    unit = expected['observer_unit'].removesuffix('.service') if expected else None
    output, errors, state, unit = native_run.admin_operation(
        attempt, code.encode(), seconds=runtime_seconds, memory=memory_limit, file_limit=file_limit, unit=unit)
    result = json.loads(output)
    if expected is not None:
        import linux_capacity_contract as capacity
        witness = {'schema': capacity.SCHEMA, 'mechanism': capacity.MECHANISM,
                   'origin': expected['origin'], 'unit': dict(state, name=unit + '.service'),
                   'stdout': output.decode('utf-8'), 'stderr': errors.decode('utf-8'),
                   'stdout_sha256': hashlib.sha256(output).hexdigest(),
                   'stderr_sha256': hashlib.sha256(errors).hexdigest(), 'exit': 0}
        common.exclusive(attempt.root / (unit + '.original.json'), witness)
        capacity.verify_observation(witness, expected)
    common.exclusive(attempt.root / ('resume-' + uuid.uuid4().hex + '.json'), result)
    return result


def root_resume(root, attempt, ssh):
    native_run.bootstrap_preparation(attempt)
    files = ('linux_loop_bound.py', 'linux_native_storage.py', 'linux_native_writers.py', 'linux_native_resume.py')
    result = root_operation(root, attempt, ssh, 'linux_native_resume.py', files)
    common.require(result['writer_closure']['complete'],
                   'native writer closure refused; inspect the retained actors and findings')
    return result


COLLECTOR_FILES = ('linux_loop_bound.py', 'linux_native_storage.py', 'linux_native_writers.py',
                   'linux_native_resume.py', 'linux_native_collect.py')


def collector_identity(attempt, *, allow_install):
    """Deploy once during preparation; proof execution only verifies these bytes."""
    import base64
    pins = {name: common.digest(Path(__file__).parent/name) for name in COLLECTOR_FILES}
    identity = hashlib.sha256(json.dumps(pins,sort_keys=True,separators=(',', ':')).encode()).hexdigest()
    directory = '/var/lib/effigy-829/helpers/observer-' + identity
    inspect = ('import json,hashlib,stat; from pathlib import Path; p=Path(' + repr(directory) + '); '
               'pins=' + repr(pins) + '; exists=p.exists(); '
               'assert p.parent.resolve()==p.parent and p.parent.stat().st_uid==0 and p.parent.stat().st_mode&0o777==0o700; '
               'exec(' + repr('if exists:\n assert p.resolve()==p and p.stat().st_uid==0 and p.stat().st_mode&0o777==0o700\n assert {f.name for f in p.iterdir()}==set(pins)\n for name,digest in pins.items():\n  f=p/name; i=f.lstat()\n  assert i.st_uid==0 and i.st_mode==0o100400 and i.st_nlink==1\n  assert hashlib.sha256(f.read_bytes()).hexdigest()==digest') + '); '
               'print(json.dumps({"exists":exists}))')
    # Keep one bounded VM command; the source map has only five fixed names.
    present = json.loads(attempt.admin('python3 -I -B -c '+shlex.quote(inspect),read_only=True))['exists']
    if not present:
        common.require(allow_install, 'prepared observer is absent; full execution cannot install it')
        modules = {name:base64.b64encode((Path(__file__).parent/name).read_bytes()).decode() for name in COLLECTOR_FILES}
        code = ('import os,base64,hashlib; from pathlib import Path; p=Path(' + repr(directory) + '); '
                'data=' + repr(modules) + '; pins=' + repr(pins) + '; '
                'p.mkdir(mode=0o700); '
                'exec(' + repr('for name,encoded in data.items():\n b=base64.b64decode(encoded)\n assert hashlib.sha256(b).hexdigest()==pins[name]\n f=os.open(p/name,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o400)\n with os.fdopen(f,"wb") as stream:\n  stream.write(b); stream.flush(); os.fsync(stream.fileno())') + '); '
                'print("observer-prepared")')
        output,errors,_,_ = native_run.admin_operation(attempt,code.encode())
        common.require(output.strip()==b'observer-prepared' and not errors,'observer preparation differs')
        common.require(json.loads(attempt.admin('python3 -I -B -c '+shlex.quote(inspect),read_only=True))['exists'],
                       'prepared observer readback differs')
    return directory,pins


def root_collect(root, attempt, ssh, controller=None, expected=None):
    proof = expected is not None and expected['origin']['evidence_kind']=='proof'
    directory,pins = collector_identity(attempt,allow_install=not proof)
    arguments = [] if controller is None else [controller]
    if expected is not None:
        common.require(expected['origin']['programs']==pins,'frozen root observer source closure differs')
        arguments = [controller or '-',json.dumps(expected['origin'],sort_keys=True)]
    output,errors,state,unit = native_run.admin_program(attempt,
        ['/usr/bin/python3','-I','-B',directory+'/linux_native_collect.py',*arguments],
        unit=expected['observer_unit'].removesuffix('.service') if expected else None)
    result = json.loads(output)
    if expected is not None:
        import linux_capacity_contract as capacity
        witness = dict(schema=capacity.SCHEMA,mechanism=capacity.MECHANISM,origin=expected['origin'],
            unit=dict(state,name=unit+'.service'),stdout=output.decode(),stderr=errors.decode(),
            stdout_sha256=hashlib.sha256(output).hexdigest(),stderr_sha256=hashlib.sha256(errors).hexdigest(),exit=0)
        common.exclusive(attempt.root/(unit+'.original.json'),witness)
        capacity.verify_observation(witness,expected)
    common.exclusive(attempt.root/('observation-'+uuid.uuid4().hex+'.json'),result)
    return result


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
    state_raw = attempt.admin('systemctl show --all effigy-829-rootful.service '
                              '--property=ActiveState,MainPID,ControlGroup', read_only=True)
    state = dict(line.split('=', 1) for line in state_raw.decode().splitlines())
    if state.get('ActiveState') == 'active':
        engine = shlex.join([cell['engine_binary']['path'], '--config', cell['docker_config'],
                             '--context', cell['engine_identity'], 'ps', '-a', '--no-trunc', '--format', '{{.ID}}'])
        common.require(attempt.command(ssh + [engine], read_only=True).strip() == b'',
                       'dedicated engine still has containers; settle their exact owners first')
    else:
        verify_engine_stopped(state, [])
        common.require(attempt.admin('test ! -d /sys/fs/cgroup/system.slice/effigy-829-rootful.service '
                                     '|| test -z "$(find /sys/fs/cgroup/system.slice/effigy-829-rootful.service -name cgroup.procs -exec cat {} +)"', read_only=True) == b'',
                       'inactive engine cgroup observation failed')
    stop_engine(attempt)
    files = ('linux_loop_bound.py', 'linux_native_storage.py', 'linux_native_writers.py',
             'linux_native_resume.py', 'linux_native_initialize.py')
    result = root_operation(root, attempt, ssh, 'linux_native_initialize.py', files, (),
                            runtime_seconds=300, file_limit='3G', memory_limit='256M')
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
    import base64
    payload = (Path(__file__).parent / 'linux_native_capacity.py').read_bytes()
    argv = ['/usr/bin/setpriv', '--reuid=1001', '--regid=1001', '--clear-groups',
            '--bounding-set=-all', '--no-new-privs', '/usr/bin/python3', '-I', '-B', '-', root, str(size)]
    if image:
        argv = ['/usr/bin/setpriv', '--reuid=1001', '--regid=1001', '--clear-groups',
                '--bounding-set=-all', '--no-new-privs', '/usr/bin/env',
                'DOCKER_CONFIG=/var/lib/effigy-829/rootful/client/config', '/usr/bin/docker',
                '--context', 'effigy-829-rootful', 'run', '--rm', '-i', '--network', 'none',
                '--user', '1001:1001', '--volume', root + ':' + root, image,
                '/usr/bin/python3', '-I', '-B', '-', root, str(size)]
    code = ('import subprocess,base64; subprocess.run(' + repr(argv)
            + ',input=base64.b64decode(' + repr(base64.b64encode(payload).decode()) + '),check=True)')
    output, errors, _, _ = native_run.admin_operation(attempt, code.encode(), seconds=180,
                                                    memory='128M', file_limit=str(size))
    common.require(not errors, 'capacity control reported stderr')
    result = json.loads(output)
    common.require(result['errno'] == 28 and result['retained_empty_inode']
                   and (not image or result['seccomp'] == '2'), 'physical ENOSPC control differs')
    path=attempt.root/('capacity-'+uuid.uuid4().hex+'.json')
    common.exclusive(path,result)
    return build.file_input(path)



def stage1_observation(attempt):
    """Adopt actual account/tools/forced SSH state; never install a substitute."""
    code = r'''import hashlib,json,os,pwd,stat,subprocess
from pathlib import Path
account=pwd.getpwnam('effigy531')
assert account.pw_uid==account.pw_gid==1001
assert os.getgrouplist('effigy531',1001)==[1001]
shadow=[line.split(':')[1] for line in Path('/etc/shadow').read_text().splitlines() if line.startswith('effigy531:')]
assert len(shadow)==1 and shadow[0].startswith(('!','*'))
policy=Path('/etc/ssh/sshd_config.d/90-effigy829.conf')
entry=Path('/usr/local/libexec/effigy829-native-channel')
for path in (policy,entry):
 info=path.lstat()
 assert path.resolve()==path and stat.S_ISREG(info.st_mode) and info.st_uid==0 and not info.st_mode&0o022
text=policy.read_text()
assert 'AuthorizedKeysFile /etc/ssh/effigy829-authorized-keys' in text
assert 'DisableForwarding yes' in text and 'PermitTTY no' in text
programs={}
for name in ('/usr/bin/python3','/usr/bin/docker','/usr/bin/git','/usr/bin/setpriv','/usr/bin/systemd-run','/usr/sbin/sshd'):
 path=Path(name).resolve(strict=True);info=path.stat()
 assert info.st_uid==0 and not info.st_mode&0o022
 programs[name]=dict(path=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest())
subprocess.run(['/usr/sbin/sshd','-t'],check=True)
print(json.dumps(dict(uid=account.pw_uid,gid=account.pw_gid,groups=[1001],password_locked=True,
 ssh_policy_sha256=hashlib.sha256(policy.read_bytes()).hexdigest(),
 forced_entry_sha256=hashlib.sha256(entry.read_bytes()).hexdigest(),programs=programs)))
'''
    output, errors, _, _ = native_run.admin_operation(attempt, code.encode())
    common.require(not errors, 'Stage 1 adoption reported stderr')
    result=json.loads(output)
    common.exclusive(attempt.root/'stage1-adoption.json',result)
    return result


def full_chain(root, seed):
    diagnostic.private_root(str(root))
    common.require(not common.run([diagnostic.GIT,'status','--porcelain']).stdout,
                   'full native smoke requires a clean committed candidate')
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
    native_run.authorize_channel(root, attempt)
    ssh = ssh_prefix(root, attempt)
    stage1_observation(attempt)
    stop_engine(attempt)
    capacity_probe(attempt, ssh, '/var/lib/effigy-829/rootful/client/tmp', 3221225472)
    root_resume(root, attempt, ssh)
    capacity_probe(attempt, ssh, build.BASE + '/qualification', 1073741824,
                   image=value['cell']['runtime_image'])
    capacity_probe(attempt,ssh,build.BASE+'/artifact-tmp',42949672960)
    binding = build.build(root, 'HEAD')
    value['cell']['source_head'] = binding['source_pin']
    prepared = assets.prepare(root, binding, value)
    import linux_native_contract as contract
    current = common.load(prepared)
    packet = contract.prepare(root, attempt, ssh, current['cell'])
    current['cell']['linux_capacity_preparation'] = build.file_input(packet)
    prepared = attempt.root / 'assets-with-capacity.json'
    common.exclusive(prepared, current)
    common.require(native_run.execute(root, prepared, 'acquire') == 0, 'native acquisition failed')
    common.require(native_run.execute(root, prepared, 'full') == 0, 'native full session failed')
    final = root_resume(root, attempt, ssh)
    stop_engine(attempt)
    attempt.command([diagnostic.PRLCTL, 'stop', setup.VM_UUID], timeout=90)
    raw = attempt.command([diagnostic.PRLCTL, 'list', '--info', setup.VM_UUID, '--json'], read_only=True)
    diagnostic.check_vm(raw, 'stopped')
    result = {'exploratory': True, 'qualification_credit': False, 'source_head': binding['source_pin'],
              'assets': str(prepared), 'final_physical_bounds': final, 'vm_terminal': 'stopped',
              'chain': ['diagnostic', 'resume', 'engine-enospc', 'workspace-enospc', 'preparation-enospc',
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

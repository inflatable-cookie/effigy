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


def bootstrap_preparation(attempt):
    """Mount only the adopted preparation image before account-side staging."""
    code = """import os,struct,fcntl,subprocess
from pathlib import Path
image=Path('/var/lib/effigy-829/preparation.ext4')
base=Path('/Volumes/Stash/scratch/effigy/linux-native-829')
i=image.lstat()
assert image.resolve()==image and i.st_ino==2641184 and i.st_size==42949672960
assert i.st_uid==i.st_gid==0 and i.st_mode==0o100600 and i.st_nlink==1
assert base.resolve()==base
rows=[line.split(' - ',1)[0].split() for line in Path('/proc/self/mountinfo').read_text().splitlines()]
found=[row for row in rows if row[4]==str(base)]
if not found:
 assert base.stat().st_dev==os.stat('/').st_dev and not list(base.iterdir())
 assert not subprocess.check_output(['losetup','-j',str(image)]).strip()
 subprocess.run(['mount','-o','loop,nodev,nosuid',str(image),str(base)],check=True)
rows=[line.split(' - ',1)[0].split() for line in Path('/proc/self/mountinfo').read_text().splitlines()]
found=[row for row in rows if row[4]==str(base)]
assert len(found)==1
node=Path('/sys/dev/block')/found[0][2]
assert (node/'loop/backing_file').read_text().strip().lstrip('/')==str(image).lstrip('/')
fd=os.open('/dev/'+node.resolve().name,os.O_RDONLY)
try:
 raw=bytearray(232);fcntl.ioctl(fd,0x4c05,raw,True)
 device,inode,rdevice,offset,size=struct.unpack_from('<QQQQQ',raw)
 assert (device,inode,rdevice,offset,size)==(i.st_dev,i.st_ino,0,0,0)
finally: os.close(fd)
(node/'queue/discard_max_bytes').write_text('0')
assert (node/'queue/discard_max_bytes').read_text().strip()=='0'
assert base.stat().st_uid==1001 and base.stat().st_mode&0o777==0o700
print('owned-preparation-mounted-discard-disabled')
"""
    output, errors, _, _ = admin_operation(attempt, code.encode(), seconds=30, memory='64M', file_limit='1M')
    common.require(output.strip() == b'owned-preparation-mounted-discard-disabled' and not errors,
                   'preparation bootstrap differs')


def admin_operation(attempt, payload, *, seconds=45, memory='256M', file_limit='8M', unit=None):
    """One retained root operation; no transport exit substitutes for cgroup closure."""
    script = stage_admin_script(attempt, payload)
    unit = unit or ('effigy-829-resume-' + uuid.uuid4().hex)
    import re
    common.require(re.fullmatch(r'effigy-829-resume-[0-9a-f]{32}', unit), 'fresh observer unit required')
    output = '/var/lib/effigy-829/helpers/' + unit
    argv = ['systemd-run', '--unit=' + unit, '--slice=system.slice', '--no-block', '--service-type=exec',
            '--property=RemainAfterExit=yes', '--property=CollectMode=inactive',
            '--property=ExitType=cgroup', '--property=KillMode=control-group',
            '--property=RuntimeMaxSec=' + str(seconds), '--property=TimeoutStopSec=5',
            '--property=MemoryMax=' + memory, '--property=MemorySwapMax=0', '--property=TasksMax=64',
            '--property=LimitFSIZE=' + file_limit, '--property=LimitCORE=0', '--property=UMask=0077',
            '--property=StandardOutput=append:' + output + '.stdout',
            '--property=StandardError=append:' + output + '.stderr',
            '--', '/usr/bin/python3', '-I', '-B', script]
    submit_unit(attempt, argv, unit)
    deadline = time.monotonic() + seconds + 20
    while True:
        raw = attempt.admin('systemctl show --all ' + unit + '.service '
                            '--property=MainPID,ControlGroup,SubState,Result,ExecMainStatus,InvocationID,ExecMainExitTimestampMonotonic', read_only=True)
        state = dict(line.split('=', 1) for line in raw.decode().splitlines())
        if (state['MainPID'] == '0' and state['ControlGroup'] == ''
                and (int(state['ExecMainExitTimestampMonotonic']) > 0 or state['SubState'] == 'failed')):
            break
        common.require(time.monotonic() < deadline, 'root operation custody remains unknown')
        time.sleep(.2)
    code = ('import json; from pathlib import Path; p=Path('
            + repr('/sys/fs/cgroup/system.slice/' + unit + '.service') + '); '
            'print(json.dumps([int(n) for f in p.rglob("cgroup.procs") for n in f.read_text().split()]))')
    state['kernel_pids'] = json.loads(attempt.admin('python3 -I -B -c ' + shlex.quote(code), read_only=True))
    common.require(state['kernel_pids'] == [], 'root operation retained kernel cgroup members')
    common.exclusive(attempt.root / (unit + '.terminal.json'), state)
    streams = {name: attempt.admin('head -c 8388609 -- ' + output + '.' + name, read_only=True)
               for name in ('stdout', 'stderr')}
    for name, data in streams.items():
        common.exclusive_bytes(attempt.root / (unit + '.' + name), data)
        common.require(len(data) <= 8388608, 'root operation stream exceeded its retained cap')
    common.require(state['Result'] == 'success' and state['ExecMainStatus'] == '0',
                   'root operation failed; retained exact unit and streams')
    return streams['stdout'], streams['stderr'], state, unit


def stage_admin_script(attempt, data):
    """Stage root-owned source with idempotent, hash-checked fixed-offset chunks."""
    import base64
    common.require(len(data) <= 1024 * 1024, 'root script exceeds its source cap')
    path = '/var/lib/effigy-829/helpers/native-' + uuid.uuid4().hex + '.py'
    for offset in range(0, len(data), 768):
        block = data[offset:offset + 768]
        code = ('import os,base64,hashlib; from pathlib import Path; '
                'p=Path(' + repr(path) + '); '
                'assert p.parent.resolve()==p.parent and p.parent.stat().st_uid==0 and p.parent.stat().st_mode&0o777==0o700; '
                'b=base64.b64decode(' + repr(base64.b64encode(block).decode()) + '); '
                'f=os.open(p,os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW,0o600); i=os.fstat(f); '
                'assert i.st_uid==0 and i.st_mode==0o100600 and i.st_nlink==1; '
                'assert i.st_size in (' + str(offset) + ',' + str(offset + len(block)) + '); '
                'assert hashlib.sha256(os.pread(f,' + str(offset) + ',0)).hexdigest()=='
                + repr(hashlib.sha256(data[:offset]).hexdigest()) + '; '
                'assert os.pwrite(f,b,' + str(offset) + ')==len(b); os.fsync(f); os.close(f)')
        for trial in range(4):
            try:
                attempt.admin('python3 -I -B -c ' + shlex.quote(code))
                break
            except RuntimeError:
                # Rewriting the SAME bytes at the SAME offset is idempotent.
                # Each invocation verifies the complete prior prefix and size;
                # it cannot append a duplicate or change an existing prefix.
                if trial == 3:
                    raise
    digest = hashlib.sha256(data).hexdigest()
    code = ('import os,hashlib; from pathlib import Path; p=Path(' + repr(path) + '); '
            'assert hashlib.sha256(p.read_bytes()).hexdigest()==' + repr(digest)
            + '; os.chmod(p,0o400); print(' + repr(digest) + ')')
    # A chmod to the one fixed final mode is idempotent, and no payload executes
    # until the complete immutable root-owned source passes this readback.
    for trial in range(4):
        try:
            actual = attempt.admin('python3 -I -B -c ' + shlex.quote(code))
            common.require(actual.decode().strip() == digest, 'root source readback differs')
            break
        except RuntimeError:
            if trial == 3:
                raise
    common.exclusive(attempt.root / (Path(path).name + '.source.json'),
                     {'path': path, 'sha256': digest, 'bytes': len(data)})
    return path


def install_channel(attempt):
    source = Path(__file__).with_name('linux_native_channel.py').read_bytes()
    staged = stage_admin_script(attempt, source)
    target = '/usr/local/libexec/effigy829-native-channel'
    # Reinstalling the same checked bytes at this task-owned guest entry is
    # idempotent. The source is immutable and root controlled before install.
    for trial in range(4):
        try:
            attempt.admin('install -m 0555 ' + shlex.quote(staged) + ' ' + target)
            break
        except RuntimeError:
            if trial == 3:
                raise
    actual = attempt.admin('sha256sum -- ' + target, read_only=True).decode().split()[0]
    common.require(actual == hashlib.sha256(source).hexdigest(), 'installed fixed SSH entry differs')


def authorize_channel(root, attempt):
    """Install the one fixed guest entry and an exclusive, expiring task key."""
    import datetime
    install_channel(attempt)
    control = root / 'control'
    key = control / 'native-channel-key'
    if not key.exists():
        attempt.command(['/usr/bin/ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-C',
                         'effigy829-native-transfer', '-f', str(key)])
    common.require(key.lstat().st_mode == 0o100600 and key.lstat().st_nlink == 1,
                   'private transfer key custody differs')
    public = attempt.command(['/usr/bin/ssh-keygen', '-y', '-f', str(key)]).decode().strip()
    fields = public.split()
    common.require(len(fields) in (2, 3) and fields[0] == 'ssh-ed25519'
                   and (len(fields) == 2 or fields[2] == 'effigy829-native-transfer'), 'unexpected transfer key')
    public = ' '.join(fields[:2])
    expiry = (datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(hours=12)).strftime('%Y%m%d%H%M%SZ')
    line = ('restrict,expiry-time="' + expiry + '",command="/usr/bin/python3 -I -B '
            '/usr/local/libexec/effigy829-native-channel --dispatch" ' + public + '\n')
    policy = ('Match User effigy531\n'
              '    AuthorizedKeysFile /etc/ssh/effigy829-authorized-keys\n'
              '    AuthenticationMethods publickey\n'
              '    PasswordAuthentication no\n'
              '    KbdInteractiveAuthentication no\n'
              '    PermitTTY no\n'
              '    DisableForwarding yes\n')
    code = ('import os,subprocess; from pathlib import Path; '
            'files=' + repr({'/etc/ssh/effigy829-authorized-keys':line,
                            '/etc/ssh/sshd_config.d/90-effigy829.conf':policy}) + '; '
            'exec(' + repr('for name,text in files.items():\n p=Path(name)\n assert p.parent.resolve()==p.parent and p.parent.stat().st_uid==0\n if p.exists():\n  i=p.lstat(); assert i.st_uid==0 and i.st_mode==0o100644 and i.st_nlink==1\n f=os.open(p,os.O_WRONLY|os.O_CREAT|os.O_TRUNC|os.O_NOFOLLOW,0o644)\n os.write(f,text.encode()); os.fchmod(f,0o644); os.fsync(f); os.close(f)') + '); '
            'subprocess.run(["/usr/sbin/sshd","-t"],check=True); '
            'subprocess.run(["systemctl","reload","ssh.service"],check=True); '
            'print(subprocess.check_output(["/usr/sbin/sshd","-T","-C","user=effigy531,host=localhost,addr=127.0.0.1"],text=True))')
    output, errors, _, _ = admin_operation(attempt, code.encode(), seconds=30, file_limit='1M')
    effective = dict(line.split(' ',1) for line in output.decode().strip().splitlines())
    common.require(not errors and effective['authorizedkeysfile'] == '/etc/ssh/effigy829-authorized-keys'
                   and effective['authenticationmethods'] == 'publickey' and effective['permittty'] == 'no'
                   and effective['disableforwarding'] == 'yes', 'effective SSH restrictions differ')
    config = control / 'ssh_config'
    text = config.read_text()
    rows = text.splitlines()
    common.require(sum(row.startswith('IdentityFile ') for row in rows) == 1, 'one private identity required')
    config.write_text('\n'.join('IdentityFile ' + str(key) if row.startswith('IdentityFile ') else row for row in rows) + '\n')
    common.exclusive(attempt.root / 'channel-authority.json', dict(entry_sha256=common.digest(Path(__file__).with_name('linux_native_channel.py')),
                     public_key=public, expires_at=expiry, effective=effective))
    return expiry


def channel_request(attempt, operation, path, *, data=None, size=None, digest=None, limit=1024 ** 3):
    import base64
    identifier = uuid.uuid4().hex + uuid.uuid4().hex
    value = {'operation': operation, 'path': str(path), 'sha256': hashlib.sha256(data).hexdigest() if data is not None else (digest or '0' * 64),
             'size_bytes': len(data) if data is not None else (size or 0), 'limit_bytes': limit}
    encoded = base64.b64encode(json.dumps(value, separators=(',', ':')).encode()).decode()
    command = ('/usr/bin/python3 -I -B /usr/local/libexec/effigy829-native-channel --provision '
               + identifier + ' ' + shlex.quote(encoded))
    for trial in range(4):
        try:
            result = json.loads(attempt.admin(command))
            break
        except (RuntimeError, ValueError):
            if trial == 3:
                raise
            time.sleep(.2)
    common.exclusive(attempt.root / ('channel-' + identifier + '.json'), result)
    return identifier, result


def channel_receive(attempt, ssh, path, data, *, timeout=180):
    identifier, expected = channel_request(attempt, 'receive', path, data=data, limit=len(data))
    attempt.command(ssh + [identifier], data=data, timeout=timeout)
    return expected


def channel_receive_file(attempt, ssh, path, source, *, timeout=180):
    info = source.stat()
    identifier, expected = channel_request(attempt, 'receive', path, size=info.st_size,
                                           digest=common.digest(source), limit=info.st_size)
    with source.open('rb') as stream:
        attempt.command(ssh + [identifier], stdin=stream, timeout=timeout)
    common.require(all(getattr(source.stat(), key) == getattr(info, key) for key in
                       ('st_dev','st_ino','st_size','st_mtime_ns','st_ctime_ns')), 'transfer source changed')
    return expected


def channel_read(attempt, ssh, path, *, limit=1024 ** 3, timeout=180):
    identifier, expected = channel_request(attempt, 'read', path, limit=limit)
    data = attempt.command(ssh + [identifier], timeout=timeout, read_only=True)
    common.require(len(data) == expected['size_bytes'] and hashlib.sha256(data).hexdigest() == expected['sha256'],
                   'fixed SSH read differs from its independently provisioned root identity')
    return data


def channel_directory(attempt, path):
    code = ('import runpy,json; m=runpy.run_path("/usr/local/libexec/effigy829-native-channel"); '
            'print(json.dumps(m["directory"](' + repr(str(path)) + '),sort_keys=True))')
    output, errors, _, _ = admin_operation(attempt, code.encode(), seconds=30, memory='64M', file_limit='16K')
    common.require(not errors, 'directory operation reported an error')
    return json.loads(output)


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
    channel_directory(attempt, driver)
    files = {}
    for name in FILES:
        data = (Path(__file__).parent / name).read_bytes()
        files[name] = hashlib.sha256(data).hexdigest()
        channel_receive(attempt, ssh, driver + '/' + name, data)
    fixture = str(session.BASE / 'qualification' / (mode + '-' + uuid.uuid4().hex))
    made = channel_directory(attempt, fixture)
    value['cell']['qualification_root'] = dict(path=fixture, device=made['device'], inode=made['inode'], uid=made['uid'])
    metadata = str(session.BASE / 'metadata' / (attempt.root.name + '.json'))
    encoded = json.dumps(value, sort_keys=True, indent=2).encode()
    channel_receive(attempt, ssh, metadata, encoded)
    binding = dict(path=metadata, sha256=hashlib.sha256(encoded).hexdigest(), fixture=fixture)
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
                channel_receive(attempt, ssh, acknowledgement, data)
                observed_gates.add(gate)
        time.sleep(1)
    if terminal is None:
        stop_owned_unit(attempt, unit, session.BASE / 'qualification')
        raise RuntimeError('owned native service exceeded deadline; stopped exact unit, no success claim')
    common.exclusive(attempt.root / 'terminal.json', terminal)
    log_data = channel_read(attempt, ssh, log, limit=8 * 1024 ** 2)
    (attempt.root / 'guest.log').write_bytes(log_data)
    # Retain the entire exact fixture, including failed command streams. No
    # deletion or rebaseline is performed by the exploratory runner.
    archive_path = str(session.BASE / 'metadata' / (attempt.root.name + '.tar'))
    code = ('import os,subprocess; p=' + repr(archive_path) + '; '
            'f=os.open(p,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600); '
            'subprocess.run(["tar","-C",' + repr(binding['fixture']) + ',"-cf","-","."],stdout=f,check=True); '
            'os.fsync(f); os.fchown(f,1001,1001); os.close(f)')
    admin_operation(attempt, code.encode(), seconds=120, file_limit='1G')
    archive = channel_read(attempt, ssh, archive_path)
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

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
import linux_native_support as setup

FILES = ('common.py', 'runtime.py', 'probe.py', 'linux_native_session.py', 'linux_native_window.py', 'linux_native_storage.py', 'linux_loop_bound.py', 'linux_capacity_contract.py')


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
    return admin_program(attempt, ['/usr/bin/python3', '-I', '-B', script], seconds=seconds,
                         memory=memory, file_limit=file_limit, unit=unit)


def admin_program(attempt, command, *, seconds=45, memory='256M', file_limit='8M', unit=None):
    """Run a prepared fixed program and retain its independently closed unit."""
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
            '--', *command]
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
        attempt.admin('python3 -I -B -c ' + shlex.quote(code), repeatable=True)
    digest = hashlib.sha256(data).hexdigest()
    code = ('import os,hashlib; from pathlib import Path; p=Path(' + repr(path) + '); '
            'assert hashlib.sha256(p.read_bytes()).hexdigest()==' + repr(digest)
            + '; os.chmod(p,0o400); print(' + repr(digest) + ')')
    # A chmod to the one fixed final mode is idempotent, and no payload executes
    # until the complete immutable root-owned source passes this readback.
    actual = attempt.admin('python3 -I -B -c ' + shlex.quote(code), repeatable=True)
    common.require(actual.decode().strip() == digest, 'root source readback differs')
    common.exclusive(attempt.root / (Path(path).name + '.source.json'),
                     {'path': path, 'sha256': digest, 'bytes': len(data)})
    return path


def install_channel(attempt):
    source = Path(__file__).with_name('linux_native_channel.py').read_bytes()
    staged = stage_admin_script(attempt, source)
    target = '/usr/local/libexec/effigy829-native-channel'
    # Reinstalling the same checked bytes at this task-owned guest entry is
    # idempotent. The source is immutable and root controlled before install.
    attempt.admin('install -m 0555 ' + shlex.quote(staged) + ' ' + target, repeatable=True)
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


def channel_request(attempt, operation, path, *, data=None, size=None, digest=None, limit=1024 ** 3, run=None):
    import base64
    identifier = uuid.uuid4().hex + uuid.uuid4().hex
    value = {'operation': operation, 'path': str(path), 'sha256': hashlib.sha256(data).hexdigest() if data is not None else (digest or '0' * 64),
             'size_bytes': len(data) if data is not None else (size or 0), 'limit_bytes': limit}
    if run is not None:
        common.require(operation == 'run', 'run details on another operation')
        value.update(run)
    encoded = base64.b64encode(json.dumps(value, separators=(',', ':')).encode()).decode()
    command = ('/usr/bin/python3 -I -B /usr/local/libexec/effigy829-native-channel --provision '
               + identifier + ' ' + shlex.quote(encoded))
    result = json.loads(attempt.admin(command, repeatable=True))
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
    """Retry one retained name only after positive absence, inside one window."""
    deadline = time.monotonic() + 120
    code = ('import json,subprocess; from pathlib import Path; '
            's=dict(line.split("=",1) for line in subprocess.check_output('
            + repr(['systemctl', 'show', '--all', unit + '.service',
                    '--property=LoadState,ActiveState,MainPID,ControlGroup'])
            + ',text=True,timeout=10).splitlines()); '
            'p=Path(' + repr('/sys/fs/cgroup/system.slice/' + unit + '.service') + '); '
            's["kernel_pids"]=[int(n) for f in p.rglob("cgroup.procs") for n in f.read_text().split()]; '
            'print(json.dumps(s))')
    absent = dict(LoadState='not-found', ActiveState='inactive', MainPID='0', ControlGroup='', kernel_pids=[])
    def observe():
        try:
            return json.loads(attempt.admin('python3 -I -B -c ' + shlex.quote(code),
                                            read_only=True, deadline=deadline))
        except ValueError as error:
            raise RuntimeError('unit submission custody is unknown') from error
    common.require(observe() == absent, 'fresh unit is not positively absent')
    while True:
        try:
            attempt.admin(shlex.join(argv), deadline=deadline)
            return
        except TransportUnavailable:
            state = observe()
            if isinstance(state, dict) and state.get('LoadState') == 'loaded':
                return
            if state != absent:
                raise TransportUnavailable('transport-unavailable: unit custody unknown; refusing resubmission')
            if time.monotonic() + 5 >= deadline:
                raise TransportUnavailable('transport-unavailable: unit remained absent through submission window')
            # A late duplicate refuses the SAME atomic systemd name. Existing
            # callers retain successful/failed units; no payload is run twice.
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


def gated_python(request, expected):
    """Short immutable bootstrap; the bound JSON carries argv, gate and file map."""
    return ('import os,time,hashlib,json; from pathlib import Path; '
            'b=Path('+repr(request)+').read_bytes(); '
            'assert hashlib.sha256(b).hexdigest()=='+repr(expected)+'; q=json.loads(b); '
            'p=Path(q["release"]); end=time.monotonic()+90; '
            'exec('+repr('while not p.exists():\n assert time.monotonic()<end\n time.sleep(.1)')+'); '
            'assert p.read_bytes()==bytes.fromhex(q["token_hex"]); '
            'assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest for path,digest in q["files"].items()); '
            'os.execv("/usr/bin/python3",["/usr/bin/python3","-I","-B"]+q["main"])')


class SSHRun:
    """An owned SSH release/wait process, separate from remote cgroup custody."""
    def __init__(self, attempt, ssh, identifier, token, *, timeout):
        self.stem = attempt.root / ('ssh-run-' + uuid.uuid4().hex)
        self.started, self.timeout = time.monotonic(), timeout
        self.streams = {name: self.stem.with_suffix('.' + name).open('xb') for name in ('stdout','stderr')}
        argv = ssh + [identifier]
        common.exclusive(self.stem.with_suffix('.intent.json'),
                         {'argv':argv,'input_sha256':hashlib.sha256(token).hexdigest(),'timeout_seconds':timeout})
        self.child = subprocess.Popen(argv,stdin=subprocess.PIPE,stdout=self.streams['stdout'],stderr=self.streams['stderr'])
        common.exclusive(self.stem.with_suffix('.pid.json'), {'local_pid':self.child.pid})
        try:
            self.child.stdin.write(token)
            self.child.stdin.close()
        except BrokenPipeError:
            # Retain the actual transport result below; it supplies no remote
            # settlement or permission to replay the opaque operation.
            pass
        self.result = None

    def finish(self):
        if self.result is not None:
            return self.result
        unknown = False
        try:
            self.child.wait(timeout=min(10,max(.1,self.timeout-(time.monotonic()-self.started))))
        except subprocess.TimeoutExpired:
            self.child.kill()
            self.child.wait()
            unknown = True
        finally:
            for stream in self.streams.values(): stream.close()
        result = dict(local_exit=self.child.returncode,waited=True,transport_timeout=unknown,
                      elapsed_seconds=time.monotonic()-self.started)
        common.exclusive(self.stem.with_suffix('.terminal.json'),result)
        self.result = result
        return result


class TransportUnavailable(RuntimeError):
    pass


class Attempt:
    def __init__(self, root, *, label='native-session'):
        diagnostic.private_root(str(root))
        self.root = root / (label + '-' + uuid.uuid4().hex)
        self.root.mkdir(mode=0o700)
        self.count = 0

    def command(self, argv, *, data=None, stdin=None, timeout=30, read_only=False, repeatable=False, deadline=None):
        deadline = deadline if deadline is not None else time.monotonic() + (120 if read_only or repeatable else timeout)
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TransportUnavailable('transport-unavailable: native command window exhausted')
            timeout = min(timeout, remaining)
            self.count += 1
            stem = self.root / ('command-%04d' % self.count)
            intent = {'argv': [str(arg) for arg in argv], 'timeout_seconds': timeout,
                      'started_at': time.time(), 'read_only': read_only, 'repeatable': repeatable,
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
                        'local_exit': child.returncode, 'timeout': True, 'remote_closure': 'unknown',
                        'status': 'transport-unavailable'})
                    raise TransportUnavailable('transport-unavailable: native client timed out; remote closure unknown: ' + str(stem))
            stdout = stem.with_suffix('.stdout').read_bytes()
            unavailable = (not stdout and child.returncode == 255 and
                           (argv[0] == diagnostic.PRLCTL or Path(argv[0]).name == 'ssh'))
            common.exclusive(stem.with_suffix('.terminal.json'), {
                'local_exit': child.returncode, 'elapsed_seconds': time.time() - intent['started_at'],
                'status': 'transport-unavailable' if unavailable else 'exited'})
            if child.returncode == 0:
                return stdout
            if not unavailable:
                raise RuntimeError('native command failed: ' + str(stem))
            if not (read_only or repeatable) or time.monotonic() + 5 >= deadline:
                raise TransportUnavailable('transport-unavailable: ' + str(stem))
            time.sleep(5)

    def admin(self, script, *, read_only=False, repeatable=False, deadline=None):
        # Parallels re-parses guest arguments. Retain one explicitly quoted
        # command, and refuse oversized transport before any guest effect.
        argv = [diagnostic.PRLCTL, 'exec', setup.VM_UUID, '/bin/sh', '-c', shlex.quote(script)]
        common.require(len(' '.join(argv[3:]).encode()) <= 2891, 'Parallels command exceeds observed safe cap')
        return self.command(argv, timeout=45, read_only=read_only, repeatable=repeatable, deadline=deadline)


class CheckedSSH:
    def __init__(self, verify):
        self.verify=verify

    def __add__(self, arguments):
        return self.verify()+arguments


def execute_proof(root, inputs, inputs_hash, go_index, go_hash):
    """Existing launch authority, one Linux cell, and the same bounded runner.

    Only the Nucleus source-immutable proof entry may invoke this route. This
    function grants no activation; it rechecks the frozen inputs and scoped GO.
    """
    common.require(common.digest(inputs)==inputs_hash and common.digest(go_index)==go_hash,
                   'proof authority bytes changed')
    head=common.source_head(Path(__file__).resolve().parents[3])
    value=common.validate_inputs(common.load(inputs),head)
    cell=next(cell for cell in value['backends'] if cell['backend']=='linux')
    go=common.load(common.artifact(common.load(go_index)['linux']))
    common.validate_go(go,inputs_hash,cell,head)
    preparation=common.load(common.artifact(cell['linux_preparation']))
    common.require(preparation['schema']=='effigy.linux-physical-preparation-complete.v1'
                   and preparation['source_head']==head and preparation['qualification_credit'] is False,
                   'Linux preparation is not the frozen candidate')
    for name,item in preparation['cell'].items():
        common.require(cell.get(name)==item,'Linux full inputs change a prepared field: '+name)
    common.require(cell['native_writer_gate']==cell['linux_capacity']['plan']['phases']['live']['origin']['nonce'],
                   'proof writer gate is outside its frozen physical plan')
    return execute(root,inputs,'full',proof=dict(cell=cell,inputs_sha256=inputs_hash,go_sha256=go_hash,go_index=str(go_index),
                   deadline=time.monotonic()+cell['budget_seconds']))


def execute(root, seed, mode, *, proof=None):
    common.require(mode in ('acquire', 'preflight', 'full'), 'explicit native session mode required')
    attempt = Attempt(root)
    raw = attempt.command([diagnostic.PRLCTL, 'list', '--info', setup.VM_UUID, '--json'], read_only=True)
    diagnostic.check_vm(raw, 'running')
    if proof is None:
        control = root / 'control'
        diagnostic.private_root(str(control))
        config,known_item = build.file_input(control/'ssh_config'),build.file_input(control/'ssh_known_hosts')
        ssh = CheckedSSH(lambda:common.isolated_ssh_prefix('effigy-531-vault',config,known_item,control,attempt.root))
        value = json.loads(seed.read_bytes())
        common.require(value.get('schema') == 'effigy.linux-native-session-exploration.v1'
                       and value.get('task_id') == session.TASK and value.get('exploratory') is True
                       and value.get('qualification_credit') is False, 'unsealed task-owned asset seed required')
        value['cell']['native_writer_gate'] = uuid.uuid4().hex if mode == 'full' else None
    else:
        common.require(mode == 'full', 'proof execution cannot select a preparation mode')
        value = {'cell':proof['cell']}
        ssh = CheckedSSH(lambda:common.ssh_prefix(value['cell']))
        known_item = value['cell']['ssh_known_hosts']
    key = attempt.command([diagnostic.PRLCTL, 'exec', setup.VM_UUID, '/bin/cat',
                           '/etc/ssh/ssh_host_ed25519_key.pub'], read_only=True).decode().split()
    known = common.artifact(known_item).read_text().split()
    common.require(len(key) >= 2 and len(known) >= 3 and key[:2] == known[1:3], 'fixed VM host key changed')
    common.exclusive(attempt.root / 'seed.json', value)
    files = {name:common.digest(Path(__file__).parent/name) for name in FILES}
    if proof is None:
        driver = str(session.BASE / ('driver-' + attempt.root.name))
        channel_directory(attempt, driver)
        for name in FILES:
            channel_receive(attempt,ssh,driver+'/'+name,(Path(__file__).parent/name).read_bytes())
        fixture = str(session.BASE / 'qualification' / (mode + '-' + uuid.uuid4().hex))
        made = channel_directory(attempt, fixture)
        value['cell']['qualification_root'] = dict(path=fixture, device=made['device'], inode=made['inode'], uid=made['uid'])
        metadata = str(session.BASE / 'metadata' / (attempt.root.name + '.json'))
        encoded = json.dumps(value, sort_keys=True, indent=2).encode()
        channel_receive(attempt, ssh, metadata, encoded)
        binding = dict(path=metadata, sha256=hashlib.sha256(encoded).hexdigest(), fixture=fixture)
        main = [driver+'/linux_native_session.py',binding['path'],binding['sha256'],mode]
        physical = value['cell'].get('linux_capacity_preparation') if mode == 'full' else None
    else:
        remote = value['cell']['remote']
        driver = remote['packet']
        common.require(Path(driver).is_relative_to(session.BASE), 'prepared driver is outside the native base')
        for name,digest in files.items():
            data = channel_read(attempt,ssh,driver+'/'+name,limit=1024**2)
            common.require(hashlib.sha256(data).hexdigest()==digest,'prepared proof driver differs')
        transfers=[('inputs',seed.read_bytes()),('go_bindings',Path(proof['go_index']).read_bytes()),
                   ('selected_go',common.artifact(common.load(proof['go_index'])['linux']).read_bytes())]
        for role,data in transfers:
            common.require(Path(remote[role]).is_relative_to(session.BASE/'metadata'),
                           'proof authority transfer is outside its bounded metadata root')
            channel_receive(attempt,ssh,remote[role],data)
        binding = dict(path=remote['inputs'],sha256=proof['inputs_sha256'],fixture=value['cell']['qualification_root']['path'])
        main = [driver+'/runtime.py','--inputs',remote['inputs'],'--sha256',proof['inputs_sha256'],
                '--go-bindings',remote['go_bindings'],'--go-sha256',proof['go_sha256'],'--backend','linux',
                '--selected-go-path',remote['selected_go']]
        physical = value['cell']['linux_capacity']
    evidence_kind = 'exploration' if proof is None else 'proof'
    capacity_packet, capacity_originals = None, {}
    if physical:
        import linux_native_contract as contract
        import linux_capacity_contract as capacity
        capacity_packet = common.load(common.artifact(physical)) if proof is None else physical
        capacity.verify_preparation(capacity_packet, value['cell']['source_head'], evidence_kind=evidence_kind)
        groups = capacity_packet['plan']['phases']['live']['cgroups']
        unit = next(group for group in groups if group != capacity.ENGINE_GROUP).rsplit('/',1)[-1].removesuffix('.service')
        common.require(__import__('re').fullmatch(r'effigy-829-[0-9a-f]{32}', unit), 'frozen controller identity differs')
        capacity_originals['before'] = contract.observe(root, attempt, ssh, capacity_packet['plan'], 'before')
    else:
        unit = 'effigy-829-' + uuid.uuid4().hex
    # The controller's retained stream is a writer too: keep its pre-opened
    # output descriptor on the same physically capped workspace filesystem.
    log = str(session.BASE / 'qualification' / (unit + '.log'))
    writable = str(session.BASE if mode in ('acquire', 'preflight') else Path(binding['fixture']))
    release = str(session.BASE / 'qualification' / ('ssh-release-' + unit))
    token = uuid.uuid4().hex.encode()
    launch_request=dict(release=release,token_hex=token.hex(),main=main,
                        files={driver+'/'+name:digest for name,digest in files.items()})
    launch_path=str(session.BASE/'metadata'/(attempt.root.name+'.run.json'))
    launch_bytes=json.dumps(launch_request,sort_keys=True).encode()
    channel_receive(attempt,ssh,launch_path,launch_bytes)
    script=gated_python(launch_path,hashlib.sha256(launch_bytes).hexdigest())
    if mode=='full':
        writable += ' ' + value['cell']['linux_claim_root']
    remaining = 1850 if proof is None else int(proof['deadline']-time.monotonic())
    common.require(remaining > 120, 'insufficient proof time for the bounded session and closure')
    argv = ['/usr/bin/systemd-run', '--unit=' + unit, '--no-block', '--service-type=exec',
            '--uid=effigy531', '--gid=effigy531', '--property=ExitType=cgroup',
            '--property=RemainAfterExit=yes', '--property=KillMode=control-group',
            '--property=CollectMode=inactive',
            '--property=RuntimeMaxSec='+str(remaining), '--property=TimeoutStopSec=15',
            '--property=PrivateUsers=identity', '--property=NoNewPrivileges=yes',
            '--property=CapabilityBoundingSet=', '--property=ProtectSystem=strict',
            '--property=ProtectHome=yes', '--property=BindReadOnlyPaths=/dev /sys',
            '--property=DevicePolicy=closed',
            '--property=ProtectKernelTunables=yes', '--property=ProtectKernelModules=yes',
            '--property=ProtectKernelLogs=yes', '--property=ReadWritePaths=' + writable,
            '--property=MemoryMax=3G', '--property=MemorySwapMax=0', '--property=LimitCORE=0',
            '--property=StandardOutput=append:' + log + '.stdout', '--property=StandardError=append:' + log + '.stderr',
            '--setenv=PATH=' + str(session.BASE / 'tools/bin') + ':/usr/bin:/bin',
            '--setenv=HOME=/home/effigy531', '--', '/usr/bin/python3', '-I', '-B',
            '-c', script]
    binding.update(unit=unit, log=log, driver=driver, files=files, unit_argv=argv,
                   exploratory=proof is None, qualification_credit=False, mode=mode)
    common.exclusive(attempt.root / 'binding.json', binding)
    submit_unit(attempt, argv, unit)
    relay = None
    try:
        identifier, _ = channel_request(attempt, 'run', release, data=token, limit=len(token),
            run=dict(unit=unit+'.service',deadline_seconds=remaining,stdout_path=log+'.stdout',stderr_path=log+'.stderr'))
        relay = SSHRun(attempt,ssh,identifier,token,timeout=remaining+20)
        deadline = time.monotonic()+1900 if proof is None else proof['deadline']
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
                relay.finish()
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
                        if capacity_packet is not None:
                            cohorts = capacity_originals.setdefault('live',[])
                            cohorts.append(contract.observe(root, attempt, ssh, capacity_packet['plan'], 'live',len(cohorts)))
                            original = json.loads(cohorts[-1]['stdout'])
                        else:
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
            relay.finish()
            raise RuntimeError('owned native service exceeded deadline; stopped exact unit, no success claim')
        transport = relay.finish()
        common.exclusive(attempt.root / 'terminal.json', terminal)
        log_data = b''
        for name in ('stdout','stderr'):
            data = channel_read(attempt, ssh, log+'.'+name, limit=8 * 1024 ** 2)
            common.exclusive_bytes(attempt.root / ('guest.'+name),data)
            log_data += data
        (attempt.root / 'guest.log').write_bytes(log_data)
        passed = (not transport['transport_timeout'] and transport['local_exit'] == 0
                  and observation_failure is None and terminal.get('Result') == 'success'
                  and terminal.get('ExecMainStatus') == '0')
        if passed:
            returned=json.loads((attempt.root/'guest.stdout').read_bytes())
            archive_item=returned['linux_archive']
            expected_archive=str(Path(value['cell']['linux_claim_root'])/(binding['sha256']+'.fixture.tar'))
            common.require(archive_item['path']==expected_archive and type(archive_item['bytes']) is int
                           and 0<archive_item['bytes']<=1024**3,'native archive escapes its admitted role')
            archive=channel_read(attempt,ssh,expected_archive,limit=1024**3)
            common.require(len(archive)==archive_item['bytes'] and hashlib.sha256(archive).hexdigest()==archive_item['sha256'],
                           'native original archive differs')
            common.exclusive_bytes(attempt.root/'fixture.tar',archive)
        elif proof is None:
            archive_path = str(session.BASE / 'metadata' / (attempt.root.name + '.tar'))
            code = ('import os,subprocess; p=' + repr(archive_path) + '; '
                    'f=os.open(p,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600); '
                    'subprocess.run(["tar","-C",' + repr(binding['fixture']) + ',"-cf","-","."],stdout=f,check=True); '
                    'os.fsync(f); os.fchown(f,1001,1001); os.close(f)')
            admin_operation(attempt, code.encode(), seconds=120, file_limit='1G')
            archive = channel_read(attempt, ssh, archive_path)
            (attempt.root / 'fixture.tar').write_bytes(archive)
        if passed and capacity_packet is not None:
            capacity_originals['after'] = contract.observe(root, attempt, ssh, capacity_packet['plan'], 'after')
            capacity.verify_full(capacity_packet, capacity_originals, value['cell']['source_head'], evidence_kind=evidence_kind)
            common.exclusive(attempt.root / 'capacity-originals.json', capacity_originals)
        result = dict(binding, passed=passed, terminal=terminal, observation_failure=observation_failure)
        common.exclusive(attempt.root / 'outcome.json', result)
        if proof is not None:
            common.require(passed,'native Linux proof failed; exact unit, streams and originals retained')
            returned=json.loads((attempt.root/'guest.stdout').read_bytes())
            expected=str(Path(binding['fixture'])/'complete.json')
            common.require(returned.get('receipt')==expected,'proof returned another receipt')
            receipt=channel_read(attempt,ssh,expected,limit=1024**2)
            common.require(hashlib.sha256(receipt).hexdigest()==returned['receipt_sha256'],'completed receipt changed after its main returned')
            return dict(receipt=expected,receipt_bytes=receipt,receipt_sha256=returned['receipt_sha256'],linux_allocation=returned['linux_allocation'],
                        linux_capacity_originals=build.file_input(attempt.root/'capacity-originals.json'),
                        stdout=(attempt.root/'guest.stdout').read_bytes(),stderr=(attempt.root/'guest.stderr').read_bytes())
        print(json.dumps({'evidence': str(attempt.root), 'passed': passed, 'qualification_credit': False}))
        return 0 if passed else 1
    except BaseException:
        stop_owned_unit(attempt, unit, session.BASE / 'qualification')
        if relay is not None:
            relay.finish()
        raise


if __name__ == '__main__':
    if len(sys.argv) != 4:
        raise SystemExit('usage: qualification:linux:native-session -- EVIDENCE_ROOT ASSET_SEED acquire|preflight|full')
    sys.exit(execute(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]))

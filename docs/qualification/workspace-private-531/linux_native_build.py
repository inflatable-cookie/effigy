"""Unsealed guest-local bootstrap through the documented missing-Oxide selector route.

Only the fixed task-829 VM is addressed. No host Cargo command, install, engine or
proof-only entry is used. Source and command originals stay in an owned evidence root.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import stat
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common
import linux_diagnostic_proof as diagnostic
import linux_native_support as setup

BASE = '/Volumes/Stash/scratch/effigy/linux-native-829'
SOURCE_REF = 'refs/remotes/origin/main'


def file_input(path):
    info = path.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid()
            or info.st_nlink != 1 or path.resolve(strict=True) != path):
        raise RuntimeError('unsafe native build input: ' + str(path))
    return {'path': str(path), 'sha256': common.digest(path)}


def guest_script(pin, remote, bundle_sha, source_ref=SOURCE_REF, temporary=BASE + '/build-tmp'):
    if not re.fullmatch('[0-9a-f]{40}', pin) or not re.fullmatch('[0-9a-f]{64}', bundle_sha):
        raise ValueError('exact source and bundle digests required')
    return '\n'.join([
        'set -eu', 'umask 077',
        'base=' + shlex.quote(BASE), 'remote=' + shlex.quote(remote),
        'python3 -I -B -c ' + shlex.quote(
            'import os,stat; from pathlib import Path; p=Path(' + repr(temporary) + '); '
            'p.mkdir(mode=0o700,exist_ok=True); i=p.lstat(); '
            'assert p.resolve()==p and stat.S_ISDIR(i.st_mode) and i.st_uid==os.geteuid() '
            'and stat.S_IMODE(i.st_mode)==0o700'),
        'export TMPDIR=' + shlex.quote(temporary) + ' TMP=' + shlex.quote(temporary) + ' TEMP=' + shlex.quote(temporary),
        'printf "guest-build-pid=%s\\n" "$$"',
        "printf '%s  %s\\n' " + shlex.quote(bundle_sha) + ' "$remote/source.bundle" | sha256sum -c -',
        'export RUSTUP_HOME="$base/toolchain/rustup" CARGO_HOME="$base/toolchain/cargo"',
        'export PATH="$base/tools/bin:$CARGO_HOME/bin:/usr/bin:/bin"',
        'if command -v oxide >/dev/null 2>&1; then echo "guest now has Oxide; rebind route" >&2; exit 1; fi',
        'git init -q "$remote/source"', 'cd "$remote/source"',
        'git fetch --no-tags ../source.bundle ' + shlex.quote(source_ref),
        'git checkout --detach ' + pin,
        'test "$(git rev-parse HEAD)" = ' + pin,
        'test -z "$(git status --porcelain)"',
        'rustc -vV', 'cargo -vV',
        'export EFFIGY_OXIDE_ESCAPE=cargo',
        'export CARGO_TARGET_DIR="$base/build-cache"',
        'export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_INCREMENTAL=false CARGO_BUILD_JOBS=4',
        # The host Effigy selector owns this bootstrap. Once built, native Effigy
        # dispatches subsequent selectors with the same recorded guest-only need.
        'sh scripts/oxide-selector.sh build --locked --bin effigy',
        'mkdir -p target/debug',
        'install -m 700 "$CARGO_TARGET_DIR/debug/effigy" target/debug/effigy',
        './target/debug/effigy --version', 'sha256sum target/debug/effigy',
    ]) + '\n'


def run_script(attempt, root, ssh, remote, payload, *, seconds=1800):
    """Execute one hash-bound native preparation script under exact unit custody."""
    import uuid
    import time
    import linux_native_run as native
    import linux_native_smoke as smoke
    if not 0 < seconds <= 2400 or len(payload) > 1024 * 1024:
        raise ValueError('bounded preparation script required')
    smoke.collector_identity(attempt,allow_install=True)
    script = remote + '/bounded-script-' + uuid.uuid4().hex
    native.channel_receive(attempt, ssh, script, payload)
    expected = __import__('hashlib').sha256(payload).hexdigest()
    unit = 'effigy-829-' + uuid.uuid4().hex
    gate = remote + '/' + unit + '.go'
    if native.lifecycle.ACTIVE:
        native.lifecycle.ACTIVE.record(gate)
    streams = {name: remote + '/' + unit + '.' + name for name in ('stdout', 'stderr')}
    code = ('import os,time,hashlib,fcntl; from pathlib import Path; '
            'b=Path(' + repr(script) + ').read_bytes(); '
            'assert hashlib.sha256(b).hexdigest()==' + repr(expected) + '; '
            'f=os.memfd_create("effigy-preparation",os.MFD_ALLOW_SEALING); '
            'assert os.write(f,b)==len(b); '
            'fcntl.fcntl(f,fcntl.F_ADD_SEALS,fcntl.F_SEAL_WRITE|fcntl.F_SEAL_GROW|fcntl.F_SEAL_SHRINK|fcntl.F_SEAL_SEAL); '
            'os.set_inheritable(f,True); '
            'p=Path(' + repr(gate) + '); end=time.monotonic()+90; '
            'exec(' + repr('while not p.exists():\n assert time.monotonic()<end\n time.sleep(.1)') + '); '
            'assert p.read_text()==' + repr(expected) + '; '
            'os.execv("/bin/sh",["/bin/sh","/proc/self/fd/"+str(f)])')
    argv = ['systemd-run', '--unit=' + unit, '--no-block', '--service-type=exec',
            '--uid=effigy531', '--gid=effigy531', '--property=ExitType=cgroup',
            '--property=RemainAfterExit=yes', '--property=CollectMode=inactive',
            '--property=KillMode=control-group', '--property=RuntimeMaxSec=' + str(seconds + 100),
            '--property=TimeoutStopSec=15', '--property=MemoryMax=3G', '--property=MemorySwapMax=0',
            '--property=TasksMax=1024', '--property=PrivateUsers=identity',
            '--property=NoNewPrivileges=yes', '--property=CapabilityBoundingSet=',
            '--property=SupplementaryGroups=', '--property=DevicePolicy=closed',
            '--property=ProtectSystem=strict', '--property=ProtectHome=yes',
            '--property=BindReadOnlyPaths=/dev /sys', '--property=LimitCORE=0',
            '--property=ProtectKernelTunables=yes', '--property=ProtectKernelModules=yes',
            '--property=ProtectKernelLogs=yes',
            '--property=ReadWritePaths=' + BASE + ' /var/lib/effigy-829/rootful/client',
            '--property=StandardOutput=append:' + streams['stdout'],
            '--property=StandardError=append:' + streams['stderr'],
            '--setenv=HOME=' + BASE + '/toolchain/home', '--setenv=PATH=/usr/bin:/bin',
            '--', '/usr/bin/python3', '-I', '-B', '-c', code]
    common.exclusive(attempt.root / (unit + '.request.json'),
                     {'unit_argv': argv, 'script_sha256': expected, 'timeout_seconds': seconds})
    native.submit_unit(attempt, argv, unit)
    try:
        deadline = time.monotonic() + 30
        while True:
            raw = attempt.admin('systemctl show --all ' + unit + '.service '
                                '--property=ActiveState,MainPID,ControlGroup', read_only=True)
            state = dict(line.split('=', 1) for line in raw.decode().splitlines())
            if state['ActiveState'] == 'active' and int(state['MainPID']) > 0:
                break
            common.require(time.monotonic() < deadline, 'bounded preparation gate did not start')
            time.sleep(.2)
        observed = smoke.root_collect(root, attempt, ssh, unit)
        common.require(observed['writer_closure']['complete'], 'preparation writer bound refused')
        identifier, _ = native.channel_request(attempt, 'run', gate, data=expected.encode(), limit=len(expected),
            run=dict(unit=unit + '.service', deadline_seconds=seconds + 90,
                     stdout_path=streams['stdout'], stderr_path=streams['stderr']))
        attempt.command(ssh + [identifier], data=expected.encode(), timeout=seconds + 120)
        deadline = time.monotonic() + seconds + 120
        while True:
            raw = attempt.admin('systemctl show --all ' + unit + '.service '
                '--property=ActiveState,SubState,Result,ExecMainStatus,MainPID,ControlGroup,InvocationID', read_only=True)
            state = dict(line.split('=', 1) for line in raw.decode().splitlines())
            if state['MainPID'] == '0' and state['ControlGroup'] == '':
                break
            common.require(time.monotonic() < deadline, 'bounded preparation did not close')
            time.sleep(2)
        common.exclusive(attempt.root / (unit + '.terminal.json'), state)
        output = {}
        for name, path in streams.items():
            data = attempt.admin('head -c 8388609 -- ' + shlex.quote(path), read_only=True)
            common.exclusive_bytes(attempt.root / (unit + '.' + name), data)
            common.require(len(data) <= 8388608, 'preparation output exceeds its retained stream cap')
            output[name] = data
        common.require(state['Result'] == 'success' and state['ExecMainStatus'] == '0',
                       'bounded preparation failed; exact terminal and streams retained')
        return output['stdout']
    except BaseException:
        native.stop_owned_unit(attempt, unit, BASE + '/qualification')
        raise


def build(root, source_ref=SOURCE_REF):
    if source_ref != 'HEAD' and not re.fullmatch(r'refs/[A-Za-z0-9_./-]+', source_ref):
        raise ValueError('HEAD or an explicit source ref required')
    # One retained transport implementation for bootstrap and session routes.
    # Import here because the session module also uses our input/path helpers.
    import linux_native_run as native
    from linux_native_run import Attempt
    attempt = Attempt(root, label='native-build')
    run_root = attempt.root
    run = attempt.command

    raw = run([diagnostic.PRLCTL, 'list', '--info', setup.VM_UUID, '--json'], read_only=True)
    diagnostic.check_vm(raw, 'running')
    pin = run([diagnostic.GIT, 'rev-parse', '--verify', source_ref + '^{commit}']).decode().strip()
    if not re.fullmatch('[0-9a-f]{40}', pin):
        raise RuntimeError('current main source pin missing')
    control = root / 'control'
    diagnostic.private_root(str(control))
    config, known = file_input(control / 'ssh_config'), file_input(control / 'ssh_known_hosts')
    ssh = common.isolated_ssh_prefix('effigy-531-vault', config, known, control, run_root)
    # Authenticate the exact fixed VM's public SSH host key again, rather than
    # trusting endpoint spelling or an unauthenticated network key scan.
    key = run([diagnostic.PRLCTL, 'exec', setup.VM_UUID, '/bin/cat', '/etc/ssh/ssh_host_ed25519_key.pub'], read_only=True).decode().split()
    pinned = (control / 'ssh_known_hosts').read_text().split()
    if len(key) < 2 or len(pinned) < 3 or key[:2] != pinned[1:3]:
        raise RuntimeError('SSH host key differs from the fixed VM')
    bundle = run_root / 'source.bundle'
    run([diagnostic.GIT, 'bundle', 'create', str(bundle), source_ref], timeout=120)
    bundle.chmod(0o400)
    binding = {'schema': 'effigy.linux-native-build-exploration.v1', 'exploratory': True,
               'qualification_credit': False, 'source_pin': pin, 'source_ref': source_ref, 'bundle': file_input(bundle),
               'ssh_config': config, 'ssh_known_hosts': known,
               'route': 'guest-local scripts/oxide-selector.sh with EFFIGY_OXIDE_ESCAPE=cargo',
               'need': 'guest has no Oxide; planner answer ee843c6e 2026-11-09'}
    (run_root / 'binding.json').write_text(json.dumps(binding, indent=2))
    remote = BASE + '/builds/' + run_root.name
    native.channel_directory(attempt, remote)
    native.channel_receive_file(attempt, ssh, remote + '/source.bundle', bundle)
    temporary = str(native.lifecycle.ACTIVE.temporary) if native.lifecycle.ACTIVE else BASE + '/build-tmp'
    if native.lifecycle.ACTIVE:
        binding['guest_tmp'] = temporary
    script = guest_script(pin, remote, binding['bundle']['sha256'], source_ref, temporary).encode()
    (run_root / 'build.sh').write_bytes(script)
    output = run_script(attempt, root, ssh, remote, script, seconds=1800)
    binding['guest_source'] = remote + '/source'
    binding['guest_binary'] = remote + '/source/target/debug/effigy'
    match = re.search(rb'(?m)^([0-9a-f]{64})  target/debug/effigy$', output)
    if not match:
        raise RuntimeError('native build did not return the actual binary digest')
    binding['guest_binary_sha256'] = match[1].decode()
    binding['complete'] = True
    (run_root / 'outcome.json').write_text(json.dumps(binding, indent=2))
    print(json.dumps(binding, indent=2))
    return binding


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('evidence_root', type=Path)
    parser.add_argument('--source-ref', default=SOURCE_REF)
    args = parser.parse_args()
    build(args.evidence_root, args.source_ref)

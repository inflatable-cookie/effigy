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
import linux_setup as setup

BASE = '/Volumes/Stash/scratch/effigy/linux-native-829'
SOURCE_REF = 'refs/remotes/origin/main'


def file_input(path):
    info = path.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid()
            or info.st_nlink != 1 or path.resolve(strict=True) != path):
        raise RuntimeError('unsafe native build input: ' + str(path))
    return {'path': str(path), 'sha256': common.digest(path)}


def guest_script(pin, remote, bundle_sha, source_ref=SOURCE_REF):
    if not re.fullmatch('[0-9a-f]{40}', pin) or not re.fullmatch('[0-9a-f]{64}', bundle_sha):
        raise ValueError('exact source and bundle digests required')
    return '\n'.join([
        'set -eu', 'umask 077',
        'base=' + shlex.quote(BASE), 'remote=' + shlex.quote(remote),
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


def build(root, source_ref=SOURCE_REF):
    if source_ref != 'HEAD' and not re.fullmatch(r'refs/[A-Za-z0-9_./-]+', source_ref):
        raise ValueError('HEAD or an explicit source ref required')
    # One retained transport implementation for bootstrap and session routes.
    # Import here because the session module also uses our input/path helpers.
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
    with bundle.open('rb') as source:
        run(ssh + ['mkdir -p ' + shlex.quote(remote) + '; cat > ' + shlex.quote(remote + '/source.bundle')],
            stdin=source, timeout=180)
    script = guest_script(pin, remote, binding['bundle']['sha256'], source_ref).encode()
    (run_root / 'build.sh').write_bytes(script)
    output = run(ssh + ['timeout --signal=TERM --kill-after=20s 1800 /bin/sh -s'], data=script, timeout=1860)
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

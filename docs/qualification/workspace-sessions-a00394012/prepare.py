#!/usr/bin/env python3
"""Prepare exact workspace-sessions r1 fixtures and the guest binary. RUN ONLY under the packet GO."""
import argparse
import hashlib
import json
import os
import re
os.umask(0o077)
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (BASE, CANDIDATE_CHECKOUT, FIXTURE_PARENT, digest, verify_bundle, verify_go,
                    Engine, load_manifest, receipt_exclusive, require_absent, run, snapshot)

parser = argparse.ArgumentParser()
parser.add_argument('--backend', required=True, choices=('colima', 'desktop', 'linux'))
parser.add_argument('--packet-sha', required=True)
parser.add_argument('--go-receipt', required=True)
args = parser.parse_args()
backend = args.backend
manifest = load_manifest(backend)

bundle = verify_bundle(backend, args.packet_sha)
go = verify_go(backend, args.packet_sha)
if args.go_receipt != str(Path('/Volumes/Builds/scratch/effigy/530') / ('530-' + backend + '-r1-go.txt')):
    raise RuntimeError('GO receipt path differs from the packet route')

FIXTURE = Path(manifest['fixtures']['root'])
BUILD_SRC = FIXTURE / 'build-src'
TOOLCHAIN = manifest['images']['toolchain']
RUNTIME_IMAGE = manifest['images']['runtime']
BUILD_CONTAINER = manifest['synthetic']['build_container']
IDENT_A = manifest['synthetic']['ident_a']
IDENT_B = manifest['synthetic']['ident_b']

FIXTURE_BYTES = {
    'effigy.toml': b"""[containers]
default = "web"
[containers.web]
primary_service = "app"
profile = "pinned-profile"
[containers.web.services.app]
catalog = "web-app"
[systems]
default = "dev"
[systems.dev]
default_workspace = "app"
[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
user = "1000:1000"
home = "/home/operator"
""",
    'infra/dev/catalog/web-app/service.toml': b"""[service]
name = "web-app"
description = "Workspace-sessions r1 qualification refusal fixture service"
""",
    'infra/dev/catalog/web-app/compose.fragment.yml': b"""services:
  app:
    image: example:test
""",
}
FIXTURE_SHA = {name: hashlib.sha256(data).hexdigest() for name, data in FIXTURE_BYTES.items()}

for step in ('prepare', 'preflight', 'runtime'):
    require_absent(BASE / ('530-' + backend + '-' + step + '-receipt.json'), 'prior receipt')
require_absent(BASE / ('530-' + backend + '-baseline-captured.json'), 'prior baseline')
require_absent(BASE / ('530-' + backend + '-guest-binary'), 'prior guest binary')

engine = Engine(backend, manifest, vm_alias=go.get('vm_alias'))
vm_alias = go.get('vm_alias')
environment = {'engine_kind': engine.kind, 'inventory': engine.inventory()}
if backend == 'colima':
    version = run([manifest['engine']['binary'], 'version'], timeout=30).stdout
    if manifest['engine']['colima_version'] not in version:
        raise RuntimeError('Colima version differs from the pinned environment')
    nerdctl_version = run(engine.args('version'), timeout=30).stdout
    if manifest['engine']['nerdctl_version'] not in nerdctl_version:
        raise RuntimeError('nerdctl version differs from the pinned environment')
    environment['colima_version'] = manifest['engine']['colima_version']
    environment['nerdctl_version'] = manifest['engine']['nerdctl_version']
elif backend == 'desktop':
    version = run(engine.args('version', '--format', '{{.Server.Version}}'), timeout=60).stdout.strip()
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', version):
        raise RuntimeError('Docker Desktop engine version unavailable')
    environment['engine_version'] = version
else:
    uname = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', vm_alias,
                 'uname -a'], timeout=30).stdout.strip()
    if 'aarch64' not in uname and 'x86_64' not in uname:
        raise RuntimeError('Linux VM architecture unavailable')
    version = run(engine.args('version', '--format', '{{.Server.Version}}'), timeout=60).stdout.strip()
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', version):
        raise RuntimeError('Linux Docker Engine version unavailable')
    environment['vm_uname'] = uname
    environment['engine_version'] = version

baseline = environment['inventory']
pinned = manifest.get('baseline', {})
if pinned.get('pinned'):
    for key, rel in (('containers', pinned['containers']), ('networks', pinned['networks']),
                     ('volumes', pinned['volumes'])):
        want = sorted(line for line in (BASE / rel).read_text().splitlines() if line)
        if sorted(baseline[key]) != want:
            raise RuntimeError('engine ' + key + ' drifted from the pinned baseline; stop and re-baseline with the owners')
    if baseline['counts'] != pinned['counts']:
        raise RuntimeError('engine inventory counts drifted from the pinned baseline')
else:
    receipt_exclusive(BASE / ('530-' + backend + '-baseline-captured.json'),
                      {'captured_at_prepare': True, 'inventory': baseline})
environment['baseline'] = baseline

# Fixture checkout with exact bytes; refuse any pre-existing path.
if FIXTURE_PARENT.is_symlink():
    raise RuntimeError('fixture parent is a symlink')
require_absent(FIXTURE, 'fixture root')
FIXTURE.mkdir(mode=0o700)
(FIXTURE / '.git').mkdir(mode=0o700)
for name, data in FIXTURE_BYTES.items():
    path = FIXTURE / name
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(fd, 'wb') as out:
        out.write(data)

# Guest binary: built only under this GO, from the frozen source, recorded by hash.
guest_binary = BASE / ('530-' + backend + '-guest-binary')
if backend == 'linux':
    tool = {}
    for name, argv in (('cargo', ['cargo', '--version']), ('rustc', ['rustc', '--version'])):
        out = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', vm_alias,
                   ' '.join(argv)], timeout=30).stdout.strip()
        if not out.startswith(name):
            raise RuntimeError('Linux VM lacks the native ' + name + ' the packet requires')
        tool[name] = out
    oxide = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', vm_alias,
                 'oxide --version'], timeout=30, check=False)
    tool['oxide'] = (oxide.stdout + oxide.stderr).strip() if oxide.returncode == 0 else 'absent'
    run(['/usr/bin/rsync', '-a', '--exclude', '.git', '--exclude', 'target',
         str(CANDIDATE_CHECKOUT) + '/', vm_alias + ':' + manifest['vm_build_dir'] + '/'],
        timeout=600)
    build = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', vm_alias,
                 'cd ' + manifest['vm_build_dir'] + ' && CARGO_TARGET_DIR=' + manifest['vm_build_dir'] + '/target cargo build --release --bin effigy'],
                timeout=7200)
    run(['/usr/bin/scp', '-q', '-o', 'BatchMode=yes',
         vm_alias + ':' + manifest['vm_build_dir'] + '/target/release/effigy',
         str(guest_binary)], timeout=600)
    guest_sha = digest(guest_binary)
    environment['guest_toolchain'] = tool
    environment['guest_build'] = {'route': 'native in-VM plain Cargo (recorded guest escape)',
                                  'toolchain': tool, 'build_exit': build.returncode}
else:
    toolchain = engine.require_image(TOOLCHAIN['reference'], TOOLCHAIN['id'])
    runtime_image = engine.require_image(RUNTIME_IMAGE['reference'], RUNTIME_IMAGE['id'])
    run(['/usr/bin/rsync', '-a', '--exclude', '.git', '--exclude', 'target',
         str(CANDIDATE_CHECKOUT) + '/', str(BUILD_SRC) + '/'], timeout=600)
    work_mount = str(BUILD_SRC)
    create = engine.args('run', '-d', '--name', BUILD_CONTAINER, '--pull', 'never',
                         '-v', work_mount + ':/work', '-w', '/work',
                         TOOLCHAIN['reference'], 'sleep', '10800')
    created = run(create, timeout=120).stdout.strip()
    if not re.fullmatch(r'[0-9a-f]{64}', created):
        raise RuntimeError('build container identity malformed')
    try:
        run(engine.args('exec', BUILD_CONTAINER, 'sh', '-c',
                        'RUSTUP_TOOLCHAIN=$(ls /usr/local/rustup/toolchains | head -1) '
                        'CARGO_TARGET_DIR=/work/target cargo build --release --bin effigy'),
            timeout=7200)
        run(engine.args('cp', BUILD_CONTAINER + ':/work/target/release/effigy', str(guest_binary)),
            timeout=300)
    finally:
        run(engine.args('rm', '-f', BUILD_CONTAINER), timeout=120, check=False)
    guest_sha = digest(guest_binary)
    if created in engine.containers():
        raise RuntimeError('build container survived cleanup')
    environment['guest_build'] = {'route': 'disposable container, resident toolchain image, no pull',
                                  'toolchain_image': toolchain, 'runtime_image': runtime_image,
                                  'container': BUILD_CONTAINER, 'source_mount': work_mount}
if not guest_sha or len(guest_sha) != 64:
    raise RuntimeError('guest binary digest unavailable')

after = engine.inventory()
if after != baseline:
    raise RuntimeError('engine inventory changed across preparation; refuse to continue')

# The exact-manifest deletion at runtime covers the whole tree including any
# build-src remains; the refusal cells re-snapshot and compare live instead of
# using this manifest.
fixture_manifest = snapshot(FIXTURE)

report = {'backend': backend, 'bundle': bundle, 'go': go, 'environment': environment,
          'fixture': {'root': str(FIXTURE), 'tree': fixture_manifest,
                      'bytes_sha256': FIXTURE_SHA},
          'guest_binary_sha256': guest_sha,
          'inventory_after_prepare': after,
          'baseline_pinned': bool(pinned.get('pinned'))}
receipt = receipt_exclusive(BASE / ('530-' + backend + '-prepare-receipt.json'), report)
print(json.dumps({'prepared': backend, 'guest_binary_sha256': guest_sha,
                  'receipt_sha256': receipt, 'inventory': after['counts']}, indent=2, sort_keys=True))

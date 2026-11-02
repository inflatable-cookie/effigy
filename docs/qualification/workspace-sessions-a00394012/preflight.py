#!/usr/bin/env python3
"""Complete read-only predicate sweep. RUN ONLY after prepare under the packet GO."""
import argparse
import hashlib
import json
import os
import re
os.umask(0o077)
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (BASE, HOST_BINARY, digest, verify_bundle, verify_go, Engine, load_manifest,
                    receipt_exclusive, run, snapshot)

parser = argparse.ArgumentParser()
parser.add_argument('--backend', required=True, choices=('colima', 'desktop', 'linux'))
parser.add_argument('--packet-sha', required=True)
parser.add_argument('--go-receipt', required=True)
args = parser.parse_args()
backend = args.backend
manifest = load_manifest(backend)

bundle = verify_bundle(backend, args.packet_sha)
go = verify_go(backend, args.packet_sha)
engine = Engine(backend, manifest, vm_alias=go.get('vm_alias'))
vm_alias = go.get('vm_alias')

preparation = json.loads((BASE / ('530-' + backend + '-prepare-receipt.json')).read_text())
if preparation['bundle'] != bundle or preparation['go'] != go:
    raise RuntimeError('preparation bundle/GO differs from the current packet execution')
prepared = preparation['environment']['inventory']
current = engine.inventory()
if current != prepared:
    raise RuntimeError('engine inventory drifted between prepare and preflight')

FIXTURE = Path(manifest['fixtures']['root'])
if snapshot(FIXTURE) != preparation['fixture']['tree']:
    raise RuntimeError('fixture changed between prepare and preflight')
guest_binary = BASE / ('530-' + backend + '-guest-binary')
if guest_binary.is_symlink() or digest(guest_binary) != preparation['guest_binary_sha256']:
    raise RuntimeError('guest binary hash differs from the prepared record')

if backend == 'linux':
    uname = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', vm_alias, 'uname -a'],
                timeout=30).stdout.strip()
    if uname != preparation['environment']['vm_uname']:
        raise RuntimeError('Linux VM identity drifted across the window')
    for name in ('cargo', 'rustc'):
        out = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', vm_alias,
                   name + ' --version'], timeout=30).stdout.strip()
        if out != preparation['environment']['guest_toolchain'][name]:
            raise RuntimeError('guest ' + name + ' drifted across the window')
else:
    build_row = preparation['environment'].get('guest_build', {}).get('toolchain_image')
    runtime_row = preparation['environment'].get('guest_build', {}).get('runtime_image')
    engine.require_image(manifest['images']['toolchain']['reference'],
                         manifest['images']['toolchain']['id'] or (build_row or {}).get('id'))
    engine.require_image(manifest['images']['runtime']['reference'],
                         manifest['images']['runtime']['id'] or (runtime_row or {}).get('id'))

# Read-only refusal cells: exact texts, fixture zero-write, no engine effect.
REFUSALS = [
    (['container', 'up', '--system', 'dev'],
     'workspace private preparation output is unavailable; no preparation or workspace apply was attempted'),
    (['container', 'up', '--system', 'dev', '--attach'],
     'workspace activation requires detached persistent ownership; held/attached handoff is unavailable'),
]
cells = []
tree_before = snapshot(FIXTURE)
for argv, want in REFUSALS:
    r = run([str(HOST_BINARY), *argv], cwd=str(FIXTURE), timeout=120, check=False)
    if r.returncode == 0 or want not in (r.stderr or ''):
        raise RuntimeError('refusal cell mismatch for ' + ' '.join(argv) +
                           ': exit ' + str(r.returncode))
    cells.append({'argv': argv, 'exit': r.returncode, 'refusal': want,
                  'stderr_sha256': hashlib.sha256(r.stderr.encode()).hexdigest()})
tree_after = snapshot(FIXTURE)
if tree_after != tree_before:
    raise RuntimeError('refusal cells wrote into the fixture')

host_probe = run([str(HOST_BINARY), '__guest-identity'], timeout=30, check=False)
if host_probe.returncode == 0 or 'guest-local identity is unproven' not in (host_probe.stderr or ''):
    raise RuntimeError('host identity probe did not refuse as required')
cells.append({'argv': ['__guest-identity'], 'exit': host_probe.returncode,
              'refusal': 'guest-local identity is unproven',
              'stderr_sha256': hashlib.sha256(host_probe.stderr.encode()).hexdigest()})

for name in (manifest['synthetic']['ident_a'], manifest['synthetic']['ident_b'],
             manifest['synthetic']['build_container']):
    if name in current['containers']:
        raise RuntimeError('synthetic container name already present: ' + name)

report = {'backend': backend, 'bundle': bundle, 'go': go,
          'inventory': current, 'fixture_tree': tree_after,
          'refusal_cells': cells,
          'host_identity_refusal_sha256': cells[-1]['stderr_sha256']}
receipt = receipt_exclusive(BASE / ('530-' + backend + '-preflight-receipt.json'), report)
print(json.dumps({'preflight': backend, 'cells': len(cells), 'receipt_sha256': receipt,
                  'inventory': current['counts']}, indent=2, sort_keys=True))

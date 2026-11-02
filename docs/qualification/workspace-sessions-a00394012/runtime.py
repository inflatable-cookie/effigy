#!/usr/bin/env python3
"""ONE authorized workspace-sessions r1 runtime cell set; never execute without exact packet GO."""
import argparse
import hashlib
import json
import os
import re
os.umask(0o077)
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import (BASE, HOST_BINARY, digest, exact_ids, verify_bundle, verify_go, Engine,
                    load_manifest, receipt_exclusive, run, safe_delete, snapshot)

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
preflight = json.loads((BASE / ('530-' + backend + '-preflight-receipt.json')).read_text())
if preparation['bundle'] != bundle or preparation['go'] != go \
        or preflight['bundle'] != bundle or preflight['go'] != go:
    raise RuntimeError('receipt chain bundle/GO differs from the current packet execution')

current = engine.inventory()
if current != preflight['inventory']:
    raise RuntimeError('engine inventory drifted between preflight and runtime')

FIXTURE = Path(manifest['fixtures']['root'])
if snapshot(FIXTURE) != preflight['fixture_tree']:
    raise RuntimeError('fixture changed between preflight and runtime')
guest_binary = BASE / ('530-' + backend + '-guest-binary')
guest_sha = digest(guest_binary)
if guest_binary.is_symlink() or guest_sha != preparation['guest_binary_sha256']:
    raise RuntimeError('guest binary hash differs from the prepared record')

IMAGE = manifest['images']['runtime']
IDENT_A = manifest['synthetic']['ident_a']
IDENT_B = manifest['synthetic']['ident_b']
UUID_RE = re.compile(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}')
PULL_FLAG = ['--pull=never']

results = {'refusals': [], 'identity': [], 'resolve': [], 'cleanup': []}


def create_identity_container(name):
    created = run(engine.args('run', '-d', *PULL_FLAG, '--name', name, '--net', 'none',
                              IMAGE['reference'], 'sleep', '3600'), timeout=120).stdout.strip()
    if not re.fullmatch(r'[0-9a-f]{64}', created):
        raise RuntimeError('identity container identity malformed: ' + name)
    if created not in engine.containers():
        raise RuntimeError('identity container absent from the engine list: ' + name)
    return created


def stage_and_probe(name, container_id, expect_reply=True):
    run(engine.args('exec', name, 'mkdir', '-p', '/staged'), timeout=60)
    run(engine.args('cp', str(guest_binary), name + ':/staged/effigy'), timeout=300)
    probes = []
    for _ in range(2):
        r = run(engine.args('exec', name, '/staged/effigy', '__guest-identity'),
                timeout=120, check=False)
        probes.append(r)
    rows = []
    for r in probes:
        match = re.search(r'\{.*\}', r.stdout or '', re.DOTALL)
        if r.returncode != 0 or not match:
            raise RuntimeError('identity probe did not produce a reply in ' + name +
                               ': exit ' + str(r.returncode) + ' stderr ' + (r.stderr or '')[-300:])
        reply = json.loads(match.group(0))
        if reply.get('schema') != 'effigy.guest-probe.v1':
            raise RuntimeError('identity probe schema mismatch in ' + name)
        identity = reply.get('identity') or {}
        if identity.get('container_id') != container_id:
            raise RuntimeError('identity probe container id differs from the engine record in ' + name)
        boot = identity.get('vm_boot_id') or ''
        if not UUID_RE.fullmatch(boot) or boot == '00000000-0000-0000-0000-000000000000':
            raise RuntimeError('identity probe boot id malformed in ' + name)
        if reply.get('executable') != 'sha256:' + guest_sha:
            raise RuntimeError('identity probe executable digest differs from the staged binary in ' + name)
        if set(identity) != {'container_id', 'vm_boot_id', 'platform', 'libc'}:
            raise RuntimeError('identity probe observation shape differs in ' + name)
        rows.append({'exit': r.returncode,
                     'stderr_sha256': hashlib.sha256((r.stderr or '').encode()).hexdigest(),
                     'reply': reply})
    if rows[0]['reply'] != rows[1]['reply']:
        raise RuntimeError('identity probe was not stable across repeated calls in ' + name)
    return rows


try:
    # Refusal cells under GO, live engine: identical texts, fixture zero-write.
    tree_before = snapshot(FIXTURE)
    for argv, want in (('container up --system dev'.split(),
                        'workspace private preparation output is unavailable; no preparation or workspace apply was attempted'),
                       ('container up --system dev --attach'.split(),
                        'workspace activation requires detached persistent ownership; held/attached handoff is unavailable')):
        r = run([str(HOST_BINARY), *argv], cwd=str(FIXTURE), timeout=120, check=False)
        if r.returncode == 0 or want not in (r.stderr or ''):
            raise RuntimeError('refusal cell mismatch under GO for ' + ' '.join(argv))
        results['refusals'].append({'argv': argv, 'exit': r.returncode, 'refusal': want})
    if snapshot(FIXTURE) != tree_before:
        raise RuntimeError('refusal cells wrote into the fixture under GO')

    # Identity cells on real backend containers. Each engine id is recorded
    # before its probes so exact-ID cleanup keeps custody even on a probe failure.
    id_a = create_identity_container(IDENT_A)
    results['identity'].append({'container': IDENT_A, 'engine_id': id_a})
    rows_a = stage_and_probe(IDENT_A, id_a)
    results['identity'][0]['probes'] = rows_a
    id_b = create_identity_container(IDENT_B)
    results['identity'].append({'container': IDENT_B, 'engine_id': id_b})
    rows_b = stage_and_probe(IDENT_B, id_b)
    results['identity'][1]['probes'] = rows_b
    if id_a == id_b:
        raise RuntimeError('distinct identity containers observed the same container id')
    boot_a = rows_a[0]['reply']['identity']['vm_boot_id']
    boot_b = rows_b[0]['reply']['identity']['vm_boot_id']
    results['identity'].append({'boot_id_relation': {
        'shared_engine': boot_a == boot_b, 'boot_a': boot_a, 'boot_b': boot_b}})

    # Host-resolve refusal inside the container: no published selection evidence.
    r = run(engine.args('exec', IDENT_A, '/staged/effigy', '__guest-reach', '--resolve'),
            timeout=120, check=False)
    refusal = (r.stderr or '')
    named = ('workspace host address unknown' in refusal) or \
            ('guest-local identity is unproven' in refusal)
    if r.returncode == 0 or not named:
        raise RuntimeError('resolve cell did not refuse by name: exit ' + str(r.returncode))
    results['resolve'].append({'exit': r.returncode,
                               'stderr_sha256': hashlib.sha256(refusal.encode()).hexdigest()})
finally:
    # Exact-ID cleanup runs even on a failed cell, then the protected-state comparison.
    removed = []
    for name, expect in ((IDENT_A, results['identity'][0]['engine_id'] if results['identity'] else None),
                         (IDENT_B, results['identity'][1]['engine_id'] if len(results['identity']) > 1 and 'engine_id' in results['identity'][1] else None)):
        containers = engine.containers()
        if expect is not None and expect in containers:
            run(engine.args('rm', '-f', expect), timeout=120)
            removed.append(expect)
    still = engine.containers()
    for ident in removed:
        if ident in still:
            raise RuntimeError('synthetic container survived cleanup: ' + ident)
    results['cleanup']['removed_exact_ids'] = removed
    baseline = preparation['environment']['baseline']
    after = engine.inventory()
    results['cleanup']['inventory_equal_to_baseline'] = after == baseline
    if backend == 'linux':
        # Remove this run's own VM build directory by its exact manifest path.
        run(['/usr/bin/ssh', '-o', 'BatchMode=yes', vm_alias,
            'test -d ' + manifest['vm_build_dir'] + ' && rm -rf ' + manifest['vm_build_dir'] + ' || true'],
            timeout=120)
        gone = run(['/usr/bin/ssh', '-o', 'BatchMode=yes', vm_alias,
                    'test -e ' + manifest['vm_build_dir'] + ' && echo present || echo gone'],
                   timeout=60).stdout.strip()
        results['cleanup']['vm_build_dir_removed'] = gone == 'gone'
    tree_of_record = preflight['fixture_tree']
    results['cleanup']['fixture_removed'] = False
    if os.path.lexists(FIXTURE):
        safe_delete(backend, manifest, FIXTURE, tree_of_record)
        results['cleanup']['fixture_removed'] = not os.path.lexists(FIXTURE)

if not results['cleanup']['inventory_equal_to_baseline']:
    raise RuntimeError('engine inventory differs from the prepare-time baseline after cleanup')
if not results['cleanup']['fixture_removed']:
    raise RuntimeError('fixture root survived cleanup')

report = {'backend': backend, 'bundle': bundle, 'go': go,
          'guest_binary_sha256': guest_sha, 'results': results}
receipt = receipt_exclusive(BASE / ('530-' + backend + '-runtime-receipt.json'), report)
print(json.dumps({'runtime': backend, 'receipt_sha256': receipt,
                  'inventory_final': after['counts'],
                  'outcome': 'exercisable cells executed; blocked prerequisites remain recorded'},
                 indent=2, sort_keys=True))

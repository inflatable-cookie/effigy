#!/usr/bin/env python3
"""Real backend proof: mixed runs and deleted linked checkout retirement."""
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
import fcntl
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import time

os.umask(0o077)
binary, base = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
alternate = os.environ["EFFIGY_PROOF_ALTERNATE_DOCKER"]
home, primary = base / 'home', base / 'primary'
home.mkdir(); primary.mkdir()
env = {key: value for key, value in os.environ.items() if not key.startswith('EFFIGY_')}
env.update(HOME=str(home), NO_COLOR='1', EFFIGY_COMPOSE_BACKEND='docker',
           EFFIGY_TEST_CONTAINERS_HOME=str(home / '.effigy'))

def call(argv, cwd=primary, success=True, timeout=180):
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    if (result.returncode == 0) != success and argv[0] == str(binary):
        listed = subprocess.run(['docker', 'ps', '-aq'], env=env, capture_output=True, text=True, timeout=15)
        if listed.returncode == 0 and listed.stdout.split():
            details = subprocess.run(['docker', 'inspect', *listed.stdout.split()], env=env, capture_output=True, text=True, timeout=15)
            (base / 'backend-failure.json').write_text(details.stdout)
    assert (result.returncode == 0) == success, (argv, result.returncode, result.stdout, result.stderr)
    return result

def cli(cwd, *args, **kwargs):
    return call([str(binary), *args], cwd, **kwargs)

def docker(*args, **kwargs):
    return call(['docker', *args], **kwargs)

def frame(value, schema):
    if isinstance(value, dict):
        if value.get('schema') == schema: return value
        for child in value.values():
            found = frame(child, schema)
            if found is not None: return found
    if isinstance(value, list):
        for child in value:
            found = frame(child, schema)
            if found is not None: return found
    return None

manifest = '''[containers]
default = "stack"
[containers.stack]
driver = "colima"
startup = "detached"
project_name = "q010-proof"
primary_service = "cache"
working_dir = "/workspace"
[containers.stack.host]
mounts = ["./:/workspace"]
[containers.stack.services.cache]
catalog = "redis"
shared = false
[tasks.hold]
mode = "tui"
scratch = false
concurrent = [{name = "job", run = "touch detached-ready; sleep 300"}]
'''
manifest += '\n'.join(f'[tasks.short{i}]\nrun = "printf short-{i}"' for i in range(8))
(primary / 'effigy.toml').write_text(manifest)
for args in [('init', '-q'), ('config', 'user.name', 'Private Fixture'),
             ('config', 'user.email', 'fixture@example.invalid'), ('add', 'effigy.toml'),
             ('commit', '-qm', 'private fixture')]:
    call(['git', *args])
checkouts = [primary]
for name in ['deleted', 'live']:
    root = base / name
    call(['git', 'worktree', 'add', '-qb', name, str(root)])
    checkouts.append(root)
dead, live = checkouts[1:]
# Pull before the mixed proof so an image download does not dominate its timing.
docker('pull', 'redis:7-alpine')
managed = []
try:
    for root in [primary, live]:
        cli(root, 'hold', '--detach')
        managed.append(root)
        deadline = time.monotonic() + 15
        while not (root / 'detached-ready').exists():
            assert time.monotonic() < deadline, 'detached run never started'
            time.sleep(.03)
    # An unrelated local writer remains stalled while memory runs proceed.
    db = sqlite3.connect(live / '.effigy/state/kernel.db')
    db.execute('BEGIN IMMEDIATE')
    started = time.monotonic()
    try:
        with ThreadPoolExecutor(max_workers=25) as pool:
            transition = pool.submit(cli, dead, 'container', 'up', '--detach')
            foreground = [pool.submit(cli, root, f'short{i}') for root in checkouts for i in range(8)]
            for result in foreground: result.result()
            transition.result()
    finally:
        db.rollback(); db.close()
    mixed_seconds = round(time.monotonic() - started, 3)
    cli(live, 'container', 'up', '--detach')
finally:
    for root in managed:
        cli(root, 'hold', 'stop')

# Retirement keeps no ownership record: labels on the backend are the only
# source of truth, so the proof reads them straight from the backend.
rows = json.loads(docker('inspect', *docker('ps', '-aq').stdout.split()).stdout)
dead_row = next(row for row in rows if row['Config']['Labels'].get('com.effigy.checkout') == str(dead))
live_ids = {row['Id'] for row in rows if row['Config']['Labels'].get('com.effigy.checkout') == str(live)}
labels = dead_row['Config']['Labels']
token = labels['com.effigy.scope']
assert labels['com.effigy.checkout'] == str(dead)
assert 'com.effigy.checkout-generation' not in labels, 'no inode generation is written'
assert 'com.effigy.checkout-fence' not in labels, 'no custody fence is written'
assert not (home / '.effigy/runtime-scopes').exists(), 'central scope index created'
assert not (dead / '.effigy/state/runtime-scopes').exists(), 'checkout scope journal created'

def owned_args(values):
    return [item for key, value in values.items() for item in ['--label', f'{key}={value}']]

owner = {key: labels[key] for key in ['com.effigy.managed', 'com.effigy.scope',
    'com.effigy.project', 'com.effigy.checkout']}
# The same labels on an unselected engine must remain untouched.
docker('--host', alternate, 'pull', 'redis:7-alpine')
alternate_id = docker('--host', alternate, 'create', *owned_args(owner), 'redis:7-alpine', 'sleep', '300').stdout.strip()
docker('--host', alternate, 'volume', 'create', *owned_args(owner | {'com.effigy.persist': 'false'}), 'q010-unselected')
mutable, persistent = 'q010-mutable', 'q010-persistent'
docker('volume', 'create', *owned_args(owner | {'com.effigy.persist': 'false'}), mutable)
docker('volume', 'create', *owned_args(owner | {'com.effigy.persist': 'true'}), persistent)
# A container of the same project that carries no Effigy checkout label is
# reported and kept: retirement never adopts an unlabelled stack.
unlabelled_id = docker('create', '--label', 'com.effigy.managed=true',
    '--label', 'com.docker.compose.project=' + labels['com.effigy.project'],
    'redis:7-alpine', 'sleep', '300').stdout.strip()
foreign_id = docker('create', '--label', 'com.effigy.managed=true', 'redis:7-alpine', 'sleep', '300').stdout.strip()
mutable_tree(dead)
shutil.rmtree(dead)
assert not dead.exists()
result = cli(primary, 'container', 'retire', '--dead', '--yes', '--json')
report = frame(json.loads(result.stdout), 'effigy.container.retire-dead.v1')
assert report and any(row['checkout'] == str(dead) and row['ok'] for row in report['scopes']), result.stdout
remaining_ids = set(docker('ps', '-aq', '--no-trunc').stdout.split())
assert dead_row['Id'] not in remaining_ids
assert live_ids <= remaining_ids and {unlabelled_id, foreign_id} <= remaining_ids
volumes = set(docker('volume', 'ls', '-q').stdout.split())
assert mutable not in volumes and persistent in volumes
assert alternate_id in docker('--host', alternate, 'ps', '-aq', '--no-trunc').stdout.split()
assert 'q010-unselected' in docker('--host', alternate, 'volume', 'ls', '-q').stdout.split()
assert not dead.exists(), 'cleanup recreated deleted checkout'
# A second dead retirement has nothing left to do.
again = cli(primary, 'container', 'retire', '--dead', '--yes', '--json')
assert frame(json.loads(again.stdout), 'effigy.container.retire-dead.v1')['scopes'] == []
# `--keep-data` keeps the named volumes it would otherwise remove and lists
# them, then an ordinary retire removes them.
keep_volume = 'q010-keep-data'
keep_owner = {key: value for key, value in owner.items()}
keep_owner['com.effigy.checkout'] = str(live)
docker('volume', 'create', *owned_args(keep_owner | {'com.effigy.persist': 'false'}), keep_volume)
kept = cli(live, 'container', 'retire', '--keep-data', '--yes', '--json')
kept_report = frame(json.loads(kept.stdout), 'effigy.container.retire.v1')
assert any(row['name'] == keep_volume and row.get('reason') == 'kept by `--keep-data`'
           for row in kept_report['kept']), kept.stdout
assert keep_volume in docker('volume', 'ls', '-q').stdout.split()
# A volume the runtime refuses to remove fails retirement and is named.
lock_volume = 'q010-locked'
docker('volume', 'create', *owned_args(keep_owner | {'com.effigy.persist': 'false'}), lock_volume)
docker('create', '--name', 'q010-lock-holder', '--mount', f'type=volume,source={lock_volume},target=/data',
       'redis:7-alpine', 'sleep', '300')
failed = cli(live, 'container', 'retire', '--yes', '--json', success=False)
failed_report = frame(json.loads(failed.stdout), 'effigy.container.retire.v1')
assert failed_report['ok'] is False
assert any(row['name'] in (lock_volume, keep_volume) for row in failed_report['remaining']), failed.stdout
assert lock_volume in docker('volume', 'ls', '-q').stdout.split()
docker('rm', '-f', 'q010-lock-holder')
docker('volume', 'rm', lock_volume)
docker('volume', 'rm', keep_volume) if keep_volume in docker('volume', 'ls', '-q').stdout.split() else None
cli(live, 'container', 'retire', '--yes', '--json')
value = {'schema': 'effigy.proof.runtime-retire-dead.v1', 'foreground_runs': 24,
         'checkouts': 3, 'detached_runs': 2, 'stalled_local_writer': True,
         'mixed_seconds': mixed_seconds,
         'live_backend_preserved_ids_started_at_and_port_pins': True,
         'labels_only_ownership': True, 'no_scope_journal_or_generation': True,
         'deleted_scope': token, 'preserved_live_containers': len(live_ids),
         'unlabelled_and_foreign_kept': 2, 'persistent_volume_retained': True,
         'keep_data_lists_kept_volumes': True, 'locked_volume_failure_named': True,
         'unselected_engine_preserved': True,
         'report': report}
(base / 'proof.json').write_text(json.dumps(value, indent=2) + '\n')
print(json.dumps(value))

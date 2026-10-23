#!/usr/bin/env python3
"""Real backend proof: mixed runs and deleted linked checkout retirement."""
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
import fcntl
import hashlib
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

# Model a stopped current-main launcher by combining these private journals
# into its central schema. The actual backend keeps running throughout cutover.
state = home / '.effigy/state'
source = state / 'kernel.db'
def read_db(path):
    return closing(sqlite3.connect(f'file:{path}?mode=ro&immutable=1', uri=True))
def mutable_tree(root):
    for directory, _, _ in os.walk(root, followlinks=False):
        os.chmod(directory, 0o700)
before_backend = json.loads(docker('inspect', *docker('ps', '-aq').stdout.split()).stdout)
before_identity = {row['Id']: row['State']['StartedAt'] for row in before_backend}
with read_db(state / 'machine.db') as machine:
    before_endpoints = machine.execute('SELECT state FROM resources').fetchall()
    with closing(sqlite3.connect(source)) as target:
        machine.backup(target)
with closing(sqlite3.connect(source)) as target:
    target.execute('PRAGMA foreign_keys=OFF')
    tables = ['checkouts', 'resources', 'runs', 'events', 'attachments', 'claims',
              'requests', 'objects', 'object_refs', 'logs']
    for root in checkouts:
        local = root / '.effigy/state'
        with read_db(local / 'kernel.db') as journal:
            for table in tables:
                rows = journal.execute(f'SELECT * FROM {table}').fetchall()
                if rows:
                    placeholders = ','.join('?' for _ in rows[0])
                    target.executemany(f'INSERT INTO {table} VALUES ({placeholders})', rows)
        objects = local / 'objects'
        if objects.exists():
            shutil.copytree(objects, state / 'objects', dirs_exist_ok=True, symlinks=True)
            mutable_tree(state / 'objects')
        mutable_tree(local)
        shutil.rmtree(local)
    assert not target.execute('PRAGMA foreign_key_check').fetchall()
    target.commit()
    assert target.execute('PRAGMA journal_mode=DELETE').fetchone() == ('delete',)
assert source.read_bytes()[18:20] == bytes([1, 1]), 'fixture backup is not checkpointed'
assert not Path(str(source) + '-wal').exists()
Path(str(source) + '-shm').unlink(missing_ok=True)
# Only private proof files are replaced; preserve the exact retirement gates.
for name in ['machine.db', 'machine.db-wal', 'machine.db-shm', 'machine-format.json']:
    (state / name).unlink(missing_ok=True)
os.chmod(source, 0o400)
source_digest = hashlib.sha256(source.read_bytes()).hexdigest()
cli(primary, 'run', 'migrate', '--plan', '--json')
started = time.monotonic()
migrated = cli(primary, 'run', 'migrate', '--legacy-launchers-stopped', '--json')
(base / 'migration.json').write_text(migrated.stdout)
migration_seconds = round(time.monotonic() - started, 3)
assert hashlib.sha256(source.read_bytes()).hexdigest() == source_digest
with read_db(state / 'machine.db') as machine:
    assert machine.execute('SELECT state FROM resources').fetchall() == before_endpoints
# Primary held only completed history, which the importer deliberately drops.
assert all((root / '.effigy/state/kernel.db').exists() for root in [dead, live])
assert not (primary / '.effigy/state/kernel.db').exists()
cli(live, 'container', 'up', '--detach')
after_backend = json.loads(docker('inspect', *before_identity).stdout)
assert {row['Id']: row['State']['StartedAt'] for row in after_backend} == before_identity
assert all(row['State']['Running'] for row in after_backend)
with read_db(state / 'machine.db') as machine:
    assert machine.execute('SELECT state FROM resources').fetchall() == before_endpoints

rows = json.loads(docker('inspect', *docker('ps', '-aq').stdout.split()).stdout)
dead_row = next(row for row in rows if row['Config']['Labels'].get('com.effigy.checkout') == str(dead))
live_ids = {row['Id'] for row in rows if row['Config']['Labels'].get('com.effigy.checkout') == str(live)}
labels = dead_row['Config']['Labels']
token = labels['com.effigy.scope']
assert labels['com.effigy.checkout-generation'] and labels['com.effigy.checkout-fence']
assert not (home / '.effigy/runtime-scopes').exists(), 'central scope index created'

def owned_args(values):
    return [item for key, value in values.items() for item in ['--label', f'{key}={value}']]

owner = {key: labels[key] for key in ['com.effigy.managed', 'com.effigy.scope',
    'com.effigy.project', 'com.effigy.checkout', 'com.effigy.checkout-generation', 'com.effigy.checkout-fence']}
# The same scope on an unselected engine must remain untouched.
docker('--host', alternate, 'pull', 'redis:7-alpine')
alternate_id = docker('--host', alternate, 'create', *owned_args(owner), 'redis:7-alpine', 'sleep', '300').stdout.strip()
docker('--host', alternate, 'volume', 'create', *owned_args(owner | {'com.effigy.persist': 'false'}), 'q010-unselected')
mutable, persistent = 'q010-mutable', 'q010-persistent'
docker('volume', 'create', *owned_args(owner | {'com.effigy.persist': 'false'}), mutable)
docker('volume', 'create', *owned_args(owner | {'com.effigy.persist': 'true'}), persistent)
unknown = owner | {'com.effigy.scope': 'f' * 32, 'com.effigy.project': 'unknown-wt-' + 'f' * 12}
unknown.pop('com.effigy.checkout-fence')
unknown_id = docker('create', *owned_args(unknown), 'redis:7-alpine', 'sleep', '300').stdout.strip()
foreign_id = docker('create', '--label', 'com.effigy.managed=true', 'redis:7-alpine', 'sleep', '300').stdout.strip()
# Only this fixture's own linked worktree is removed, including its journal.
mutable_tree(dead)
shutil.rmtree(dead)
assert not dead.exists()
fence = home / '.effigy/gates' / f'checkout-{token}.retirement'
assert fence.exists(), list(home.rglob('*retirement'))
with fence.open('r+b') as lease:
    fcntl.flock(lease, fcntl.LOCK_SH)
    blocked = cli(primary, 'container', 'retire', '--dead', '--yes', '--json', success=False)
    assert docker('inspect', dead_row['Id']).returncode == 0
    assert mutable in docker('volume', 'ls', '-q').stdout.split()
result = cli(primary, 'container', 'retire', '--dead', '--yes', '--json')
report = frame(json.loads(result.stdout), 'effigy.container.retire-dead.v1')
assert report and any(row['token'] == token and row['ok'] for row in report['scopes']), result.stdout
remaining_ids = set(docker('ps', '-aq', '--no-trunc').stdout.split())
assert dead_row['Id'] not in remaining_ids
assert live_ids <= remaining_ids and {unknown_id, foreign_id} <= remaining_ids
volumes = set(docker('volume', 'ls', '-q').stdout.split())
assert mutable not in volumes and persistent in volumes
assert alternate_id in docker('--host', alternate, 'ps', '-aq', '--no-trunc').stdout.split()
assert 'q010-unselected' in docker('--host', alternate, 'volume', 'ls', '-q').stdout.split()
assert not dead.exists(), 'cleanup recreated deleted checkout'
assert hashlib.sha256(source.read_bytes()).hexdigest() == source_digest
cli(primary, 'container', 'retire', '--dead', '--yes', '--json')
value = {'schema': 'effigy.proof.runtime-retire-dead.v1', 'foreground_runs': 24,
         'checkouts': 3, 'detached_runs': 2, 'stalled_local_writer': True,
         'mixed_seconds': mixed_seconds, 'migration_seconds': migration_seconds,
         'live_backend_migration_preserved_ids_started_at_and_port_pins': True, 'lease_refusal_before_deletion': True,
         'deleted_scope': token, 'preserved_live_containers': len(live_ids),
         'preserved_unknown_and_foreign': 2, 'persistent_volume_retained': True, 'unselected_engine_preserved': True,
         'report': report}
(base / 'proof.json').write_text(json.dumps(value, indent=2) + '\n')
print(json.dumps(value))

"""Unsealed native exploration. Never supplies a qualification receipt or proof credit.

Requires the caller's explicit authority over the fixed VM. The sealed entry still
uses its reviewed snapshot pin; this entry binds the current inventory for one run.
A successful run deletes its own pre-run snapshot by exact ID (never a referenced
one, never on failure) and records the deletion in the completion record.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
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
import linux_native_lifecycle as lifecycle
import os


# The run-owned pre-run snapshot name. The trailing hex is fresh per run, so the
# name can never equal a referenced snapshot name; deletion additionally requires
# the exact created identity, a live-list match and a veto against every UUID the
# qualification sources reference.
SNAPSHOT_NAME_PREFIX = 'effigy-native-before-'
SNAPSHOT_NAME_RE = re.compile(r'^effigy-native-before-[0-9a-f]{32}$')
SNAPSHOT_ID_RE = re.compile(r'^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$')
# Every UUID string referenced anywhere under docs/qualification/** at this commit
# (plus the two code pins, which are in that set), frozen. Over-inclusion is safe:
# a fresh uuid4 pre-run identity can never collide with this set, and deletion
# still requires the exact created name and a live-list match. A control re-scans
# the tree and refuses when the sources name a UUID this literal does not cover.
REFERENCED_SNAPSHOTS = frozenset({
    '00000000-0000-0000-0000-000000000000',
    '00000000-1111-4222-8333-444444444444',
    '01234567-89ab-cdef-0123-456789abcdef',
    '06266c0c-08a4-4592-9dd3-e4240db42c77',
    '06fa2bbb-55f5-4f02-84c9-069e3a179946',
    '07fc2413-69da-4209-8585-e0818a104f03',
    '0f1ad39c-b49d-4f1d-a18c-4d9919526ecb',
    '0f1e2d3c-4b5a-4968-8777-665544332211',
    '0f22c5f2-ebc9-4823-a690-18d9ee6d9929',
    '11111111-1111-1111-1111-111111111111',
    '11111111-2222-3333-4444-555555555555',
    '1164ddbc-df14-4d0e-8098-02b5882aa5b9',
    '12345678-1234-1234-1234-123456789abc',
    '12563bc2-e1fe-421d-94b5-6bb1df991eb5',
    '13f69c1b-9a5e-455e-8004-af30ece9685f',
    '16cee685-93d5-4783-bb1f-2734644a0630',
    '18ad4b3d-6b39-4d23-a9bd-51e9ca4f3ed4',
    '18f16a9f-a2cd-421e-8390-8f8d6d2bd829',
    '1910c8d2-7261-4059-b8e5-d6a2447d57b9',
    '1b522ff5-5c56-4255-b4f4-f297e69b45c7',
    '1b8278fc-1a9f-491b-bba0-63aeeab38788',
    '1d154272-c88b-4a6d-89b7-fc42e6697653',
    '1ee33c03-8f8f-4cea-a825-0070e8df251b',
    '22222222-2222-2222-2222-222222222222',
    '22d5e876-e652-494c-93d1-a1a483e66ece',
    '27df9002-8ec3-4a25-b2de-a6fc1b61a032',
    '2c5eb87b-7996-4fdc-a26a-aa70a412c0a3',
    '33333333-3333-3333-3333-333333333333',
    '33bb9447-5caf-406c-90a7-7c48bbccff61',
    '365bc2f0-d601-49bf-9a3d-33997b5a333f',
    '4102d7ce-2edf-487c-9895-fd7a62d24837',
    '433c9e5b-c7b9-469f-9633-24b4addcd3b5',
    '44444444-4444-4444-4444-444444444444',
    '44b5d35e-b053-4aae-9140-266a00bbef9a',
    '4b3cb82c-d590-4023-9bac-7c13319e0f66',
    '4d475daf-8aa1-497a-9a38-355f21fcd867',
    '4dd38cd0-d7a0-42c7-8d08-e4e7f5cdfa44',
    '5049f515-1de4-4fe9-a7f2-439293f7d8f9',
    '55555555-5555-5555-5555-555555555555',
    '56fee8f8-1a38-42d6-acf9-d446ecff279c',
    '5e9a2e28-1cea-48ef-a328-6a9f3b8c3b03',
    '5fbaabe3-6958-40ff-92a7-860e329aab41',
    '631ec2a1-3980-4429-83dc-7df9715f7093',
    '63c338e2-1c50-435e-9783-e78e9d8558e6',
    '64b1fe6f-9f7e-44b0-bded-254967c26615',
    '6715adf6-dc88-41ff-9cfa-cc483888358d',
    '68e7e6e3-d9ce-4935-9e62-18e4f6f16f81',
    '6c707041-05ac-0011-0007-000000000001',
    '6de5909b-25da-444a-9610-731ee1247b11',
    '704718e1-2314-44c8-9087-d78ed36b0f4e',
    '769d7766-ad2d-4fba-9418-1f1a00000001',
    '7d3e6e0d-607a-46fa-8852-6b35db5ab62c',
    '7d95a88a-18df-4760-8e46-6476b6b08d70',
    '8335884c-3de4-4cf1-a560-792bd38437bb',
    '8ccbdfe4-4fd8-42b4-8402-11b694b6e8f9',
    '8d8707ea-dd3d-48ab-b408-47f66e5475dd',
    '8f3e1a2b-1111-2222-3333-444455556666',
    '90ff1853-563e-423e-81e5-68c1b8828b85',
    '9248502a-2a45-462f-a62c-3e582361a7a8',
    '929e3707-21ba-4809-983b-3cbba80da87b',
    '9447f202-8d43-4ec2-80c9-ce527b34bd9d',
    '9565acda-e540-4879-a568-fd907af1bf2f',
    '98ba9681-b85f-4a7f-a15e-9ab9cdbf5350',
    '98d0cde3-67bd-4bf9-b943-2e002eb78e08',
    '99999999-8888-7777-6666-555555555555',
    '99999999-9999-9999-9999-999999999999',
    '9ea86682-b930-4da6-9901-73681bd7ba85',
    '9f8a67a6-2fec-46ae-998f-141b27a82bab',
    'a4b2b55c-875e-40a3-82f2-3dc3351d8339',
    'a648d634-82cf-4b3c-8903-a009eb427c12',
    'a6b0e831-afa1-4179-aad9-91e297e58014',
    'aaaaaaaa-0000-0000-0000-000000000002',
    'aaaaaaaa-0000-4c11-8e22-333333333333',
    'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
    'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
    'b77d1fbf-e60b-41f5-a32d-fb285d3b7d71',
    'ba08b2af-40d2-4da5-8f67-f5e48e02a4d6',
    'bbbbbbbb-0000-4d22-8f33-444444444444',
    'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
    'bc84e330-2ac6-4009-acb9-3b79332c3384',
    'c1a80a32-6ab0-485a-af7a-398dd7b67a72',
    'cb0e6df4-3a97-4c4e-bd81-34857e1a9b9c',
    'cccccccc-0000-4e33-8044-555555555555',
    'd1602e5a-964a-4375-9eec-df6ed4350305',
    'd1e2f3a4-0000-4b55-9c66-7d8e9f0a1b2c',
    'd1e2f3a4-0000-4b55-9c66-7d8e9f0a1b2d',
    'd45b3846-b86e-4f49-afc0-f448c4ec800e',
    'd488be7b-682f-41ac-b79b-57bbf495a01d',
    'd6b887ef-8b40-44ac-a8ba-6b1ffd17df64',
    'da34736c-c2a0-4d19-bf5e-909794feb584',
    'dddddddd-0000-4f44-8155-666666666666',
    'e16cb22b-cf85-468a-8cc2-af1719d50754',
    'ed11a591-6e6a-4e88-8101-b925091e12ad',
    'edb0e66f-c0a9-4139-8e34-0de8adba2211',
    'ef9cc944-30a9-4df6-b4e5-0d8b56ee9ecd',
    'f43ee466-ee0e-4680-8115-571b367ecd0d',
    'f8b78288-7c50-4d6e-bbf1-fac45bc9f273',
    'fd847a76-380a-43cf-91ef-f7153707f0d9',
})


def snapshot_list_ids(attempt):
    """The live snapshot inventory as {unbraced lowercase ID: entry}.

    Parallels indexes the JSON object by braced UUID; the ID is never a field
    inside an entry, so only the exact owned key binds, without recursion through
    unrelated snapshots or accepting an ID mentioned as a parent.
    """
    raw = attempt.command([diagnostic.PRLCTL, 'snapshot-list', setup.VM_UUID, '--json'],
                          read_only=True)
    snapshots = json.loads(raw, object_pairs_hook=diagnostic.parse_unique)
    common.require(isinstance(snapshots, dict), 'snapshot inventory is not the indexed object')
    entries = {}
    for key, entry in snapshots.items():
        common.require(isinstance(key, str) and key.startswith('{') and key.endswith('}')
                       and SNAPSHOT_ID_RE.fullmatch(key[1:-1]) and isinstance(entry, dict),
                       'snapshot inventory carries a non-snapshot key')
        entries[key[1:-1].lower()] = entry
    return entries


def resolve_created_snapshot(before, after, name):
    """The one snapshot this run created: the single listed ID absent before, whose
    live entry carries exactly the created name, uniquely. Anything else refuses: a
    missing, renamed, duplicated or foreign entry is never this run's snapshot."""
    common.require(isinstance(name, str) and SNAPSHOT_NAME_RE.fullmatch(name),
                   'pre-run snapshot name is outside the run-owned prefix')
    new = [key for key in after if key not in before]
    common.require(len(new) == 1, 'pre-run snapshot creation is not exactly one new listed identity')
    created = new[0]
    common.require(after[created].get('name') == name,
                   'created snapshot entry does not carry the created name')
    common.require(sum(1 for entry in after.values() if entry.get('name') == name) == 1,
                   'created snapshot name is not unique in the live inventory')
    return created


def check_deletable(snapshot_id, snapshot_name, listed, referenced=REFERENCED_SNAPSHOTS):
    """The exact deletable snapshot identity, or refusal.

    Only the run's own pre-run snapshot may be deleted, and only by exact ID: the
    name must be the created run-owned name, the ID must be the live listed identity
    under exactly that name (unique), and the ID must not be referenced in
    docs/qualification/** or by code. A referenced ID refuses even under a
    run-prefixed name, so a live-list confusion can never spend a reviewed snapshot.
    """
    common.require(isinstance(snapshot_name, str) and SNAPSHOT_NAME_RE.fullmatch(snapshot_name),
                   'snapshot name is not the run-owned pre-run name')
    common.require(isinstance(snapshot_id, str) and SNAPSHOT_ID_RE.fullmatch(snapshot_id),
                   'snapshot identity is malformed')
    snapshot_id = snapshot_id.lower()
    common.require(snapshot_id not in referenced,
                   'snapshot is referenced in docs/qualification/** or by code; never delete')
    common.require(snapshot_id in listed and listed[snapshot_id].get('name') == snapshot_name,
                   'snapshot identity is not the live listed entry of the created name')
    common.require(sum(1 for entry in listed.values() if entry.get('name') == snapshot_name) == 1,
                   'snapshot name is not unique in the live inventory')
    return '{' + snapshot_id + '}'


def take_prerun_snapshot(attempt):
    """Take the run-owned pre-run snapshot and bind its exact created identity."""
    before = snapshot_list_ids(attempt)
    name = SNAPSHOT_NAME_PREFIX + uuid.uuid4().hex
    attempt.command([diagnostic.PRLCTL, 'snapshot', setup.VM_UUID, '--name', name], timeout=120)
    created = resolve_created_snapshot(before, snapshot_list_ids(attempt), name)
    return {'id': created, 'name': name}


def delete_owned_snapshot(attempt, snapshot_id, snapshot_name):
    """Delete only the run's own pre-run snapshot by exact ID and verify it is gone.

    Called only after the run succeeded and its guest outputs were cleaned up. Every
    guard in check_deletable runs again against the live inventory immediately before
    the delete; after the delete the inventory is re-read and the ID must be absent.
    Returns the deletion record for the completion record."""
    braced = check_deletable(snapshot_id, snapshot_name, snapshot_list_ids(attempt))
    argv = [diagnostic.PRLCTL, 'snapshot-delete', setup.VM_UUID, '--id', braced]
    attempt.command(argv, timeout=120)
    common.require(snapshot_id.lower() not in snapshot_list_ids(attempt),
                   'deleted pre-run snapshot is still listed')
    return {'deleted_id': snapshot_id.lower(), 'deleted_name': snapshot_name,
            'delete_argv': argv, 'verified_absent': True}


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


def root_resume(root, attempt, ssh):
    native_run.bootstrap_preparation(attempt)
    directory, _ = collector_identity(attempt, allow_install=True)
    output, errors, _, _ = native_run.admin_program(attempt,
        ['/usr/bin/python3', '-I', '-B', directory + '/linux_native_resume.py'])
    common.require(not errors, 'native resume reported stderr')
    result = json.loads(output)
    common.require(result['writer_closure']['complete'],
                   'native writer closure refused; inspect the retained actors and findings')
    return result


COLLECTOR_FILES = ('linux_loop_bound.py', 'linux_native_storage.py', 'linux_native_writers.py',
                   'linux_native_resume.py', 'linux_native_collect.py')


def collector_identity(attempt, *, allow_install, files=COLLECTOR_FILES):
    """Deploy once during preparation; proof execution only verifies these bytes."""
    import base64
    pins = {name: common.digest(Path(__file__).parent/name) for name in files}
    identity = hashlib.sha256(json.dumps(pins,sort_keys=True,separators=(',', ':')).encode()).hexdigest()
    directory = '/var/lib/effigy-829/helpers/observer-' + identity
    inspect = ('import json,hashlib,stat; from pathlib import Path; p=Path(' + repr(directory) + '); '
               'pins=' + repr(pins) + '; exists=p.exists(); '
               'assert p.parent.resolve()==p.parent and p.parent.stat().st_uid==0 and p.parent.stat().st_mode&0o777==0o700; '
               'exec(' + repr('if exists:\n assert p.resolve()==p and p.stat().st_uid==0 and p.stat().st_mode&0o777==0o700\n assert {f.name for f in p.iterdir()}==set(pins)\n for name,digest in pins.items():\n  f=p/name; i=f.lstat()\n  assert i.st_uid==0 and i.st_mode==0o100400 and i.st_nlink==1\n  assert hashlib.sha256(f.read_bytes()).hexdigest()==digest') + '); '
               'print(json.dumps({"exists":exists}))')
    # Keep one bounded VM command for the explicit, hash-bound source map.
    present = json.loads(attempt.admin('python3 -I -B -c '+shlex.quote(inspect),read_only=True))['exists']
    if not present:
        common.require(allow_install, 'prepared observer is absent; full execution cannot install it')
        modules = {name:base64.b64encode((Path(__file__).parent/name).read_bytes()).decode() for name in files}
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
    directory, _ = collector_identity(attempt, allow_install=True, files=files)
    output, errors, _, _ = native_run.admin_program(attempt,
        ['/usr/bin/python3', '-I', '-B', directory + '/linux_native_initialize.py'],
        seconds=300, file_limit='3G', memory='256M')
    common.require(not errors, 'image initialization reported stderr')
    result = json.loads(output)
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
    output, errors, _, _ = native_run.admin_operation(attempt, code.encode(), seconds=600 if lifecycle.ACTIVE else 180,
                                                    memory='128M', file_limit=str(size))
    common.require(not errors, 'capacity control reported stderr')
    result = json.loads(output)
    common.require(result['errno'] == 28 and result['retained_empty_inode']
                   and (not image or result['seccomp'] == '2'), 'physical ENOSPC control differs')
    path=attempt.root/('capacity-'+uuid.uuid4().hex+'.json')
    common.exclusive(path,result)
    if lifecycle.ACTIVE and not Path(result['path']).is_relative_to(lifecycle.ACTIVE.temporary):
        lifecycle.ACTIVE.operation('remove', records=[dict(path=str(Path(result['path']).parent),
            device=result['device'], inode=result['directory_inode'])])
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
    raw = attempt.command([diagnostic.PRLCTL, 'list', '--info', setup.VM_UUID, '--json'], read_only=True)
    diagnostic.check_vm(raw, 'stopped')
    processes = subprocess.check_output(['ps', '-axo', 'pid=,comm=,args='], text=True)
    common.require(not any('python' in line.split()[1] and 'linux_native_smoke.py' in line
                           and int(line.split()[0]) != os.getpid() for line in processes.splitlines()
                           if line.split()),
                   'another linux_native_smoke.py is running')
    # Heavy native runs serialize after the closing Colima session: overlapping
    # runs on Stash caused today's guest disk resets.
    common.require(not any('colima_session_smoke' in line or 'macos_colima_session_smoke.py' in line
                           for line in processes.splitlines()),
                   'a Colima session smoke is running; native runs serialize after it')
    pre_run_snapshot = take_prerun_snapshot(attempt)
    common.require(diagnostic_run(root) == 0, 'native diagnostic has findings')
    attempt.command([diagnostic.PRLCTL, 'start', setup.VM_UUID], timeout=90)
    scope = lifecycle.Scope(attempt)
    lifecycle.ACTIVE = scope
    baseline, after, binding, prepared, final, guest_artifact = None, None, None, None, None, None
    try:
        attempt.admin('cat /proc/sys/kernel/random/boot_id', read_only=True)
        key = attempt.command([diagnostic.PRLCTL, 'exec', setup.VM_UUID, '/bin/cat',
                               '/etc/ssh/ssh_host_ed25519_key.pub'], read_only=True).decode().split()
        known = (root / 'control/ssh_known_hosts').read_text().split()
        common.require(len(key) >= 2 and key[:2] == known[1:3], 'fixed VM SSH identity differs')
        native_run.bootstrap_preparation(attempt)
        # Admit before the census, ENOSPC controls or any compilation. Retained
        # output can exhaust the census observer's cgroup as well as the disk.
        admission = scope.operation('preflight', preparation_only=True)
        common.require(not admission['missing'], 'guest-capacity: ' + json.dumps(admission['missing']))
        root_resume(root, attempt, ssh)
        admission = scope.operation('preflight')
        common.require(not admission['missing'], 'guest-capacity: ' + json.dumps(admission['missing']))
        baseline = admission['observation']
        native_run.authorize_channel(root, attempt)
        ssh = ssh_prefix(root, attempt)
        native_run.channel_directory(attempt, scope.temporary)
        stage1_observation(attempt)
        stop_engine(attempt)
        capacity_probe(attempt, ssh, '/var/lib/effigy-829/rootful/client/tmp', 3221225472)
        root_resume(root, attempt, ssh)
        capacity_probe(attempt, ssh, build.BASE + '/qualification', 1073741824,
                       image=value['cell']['runtime_image'])
        capacity_probe(attempt, ssh, str(scope.temporary), 42949672960)
        binding = build.build(root, 'HEAD')
        scope.operation('evict')
        value['cell']['source_head'] = binding['source_pin']
        prepared = assets.prepare(root, binding, value)
        scope.operation('evict')
        import linux_native_contract as contract
        current = common.load(prepared)
        packet = contract.prepare(root, attempt, ssh, current['cell'])
        current['cell']['linux_capacity_preparation'] = build.file_input(packet)
        prepared = attempt.root / 'assets-with-capacity.json'
        common.exclusive(prepared, current)
        guest_artifact = current['cell']
        common.require(native_run.execute(root, prepared, 'acquire') == 0, 'native acquisition failed')
        common.require(native_run.execute(root, prepared, 'full') == 0, 'native full session failed')
        final = root_resume(root, attempt, ssh)
    finally:
        try:
            stop_engine(attempt)
            if scope.records:
                after = scope.finish(ssh, guest_artifact=guest_artifact)
        finally:
            lifecycle.ACTIVE = None
            attempt.command([diagnostic.PRLCTL, 'stop', setup.VM_UUID], timeout=90)
            raw = attempt.command([diagnostic.PRLCTL, 'list', '--info', setup.VM_UUID, '--json'], read_only=True)
            diagnostic.check_vm(raw, 'stopped')
    tolerance = 128 * 1024 ** 2
    deltas = {path: row['free'] - after['mounts'][path]['free'] for path, row in baseline['mounts'].items()}
    # This point is reached only when the try block above raised nothing and the finally
    # block stopped the engine, finished the scope (guest outputs cleaned up and exported)
    # and stopped the VM. The final success validations run before the snapshot deletion:
    # a failed GNU export or capacity check keeps the pre-run snapshot for diagnosis.
    # Only with every success criterion validated is the run's own snapshot deleted by
    # exact ID, and the deletion is recorded in the completion record.
    result = {'exploratory': True, 'qualification_credit': False, 'source_head': binding['source_pin'],
              'assets': str(prepared), 'final_physical_bounds': final, 'vm_terminal': 'stopped',
              'capacity_before': baseline, 'capacity_after': after, 'consumed_bytes': deltas,
              'gnu_artifact_export': scope.gnu_export,
              'space_tolerance_bytes': tolerance,
              'pre_run_snapshot': pre_run_snapshot,
              'chain': ['diagnostic', 'resume', 'engine-enospc', 'workspace-enospc', 'preparation-enospc',
                        'native-build', 'gnu-artifact', 'ordinary-acquisition', 'full-session']}
    common.require(scope.gnu_export is not None,
                   'completed native smoke is missing its exported GNU artifact family')
    common.require(all(abs(delta) <= tolerance for delta in deltas.values()),
                   'guest-capacity: run did not restore free space within tolerance')
    snapshot_deletion = delete_owned_snapshot(attempt, pre_run_snapshot['id'], pre_run_snapshot['name'])
    result['pre_run_snapshot_deletion'] = snapshot_deletion
    common.require(snapshot_deletion['verified_absent'],
                   'pre-run snapshot deletion is not verified')
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

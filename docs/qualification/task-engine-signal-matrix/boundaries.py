#!/usr/bin/env python3
"""Committed refusal-boundary battery for the task-engine signal packet.

Queue tasks effigy#579, effigy#582 and effigy#585. Installed-Effigy selector
`test:qualification:task-engine-signals` runs only this file. Fake backends,
labeled doubles, and owned local fake-executable HostCli fixtures under this
run's TMPDIR (or /Volumes/Builds/scratch/effigy/579/) — no VM, engine,
container, image, install, or native task. Passing these controls is not
native closure.
"""
import hashlib
import json
import os
import signal as os_signal
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402
import execute  # noqa: E402
import launch  # noqa: E402
from common import (  # noqa: E402
    DIR, HELPER_NAME, HEX40, HEX64, NONCE_HEX_LEN, SCRATCH, SIGNAL_CELLS,
    STATUS_SCHEMA, UNARMED, Refused, apply_overlay, bundle_digest,
    construct_plan, decode_output, generate_nonce, helper_kill_tail,
    load_manifest, observe_snapshot, parse_identity, parse_inspect,
    parse_snapshot, parse_status, production_helper, require_armed,
    verify_go_receipt, verify_packet_bundle,
)
from execute import HostCliDriver, LabeledDouble, LostResponse, execute_pass  # noqa: E402

PASS = []
FAIL = []


def check(name, fn):
    try:
        fn()
        PASS.append(name)
        print('ok   %s' % name)
    except Refused as e:
        FAIL.append('%s: refused unexpectedly: %s' % (name, e))
        print('FAIL %s: unexpected refusal: %s' % (name, e))
    except AssertionError as e:
        FAIL.append('%s: %s' % (name, e))
        print('FAIL %s: %s' % (name, e))
    except Exception as e:  # noqa: BLE001 — the battery reports any failure
        FAIL.append('%s: %s: %s' % (name, type(e).__name__, e))
        print('FAIL %s: %s: %s' % (name, type(e).__name__, e))


def expect_refused(fn, fragment=None):
    try:
        fn()
    except Refused as e:
        if fragment is not None and fragment not in str(e):
            raise AssertionError('refusal text %r lacks %r' % (str(e), fragment)) from None
        return
    raise AssertionError('expected a fail-closed refusal, got none')


def owned_tmp():
    base = os.environ.get('TMPDIR') or os.environ.get('TMP') or str(SCRATCH)
    Path(base).mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix='tes-579-', dir=base))


def tmpfile(root, name, data, mode=0o600):
    p = root / name
    fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), mode)
    with os.fdopen(fd, 'wb') as f:
        f.write(data if isinstance(data, bytes) else data.encode())
    os.chmod(p, mode)
    return p


def go_text(bundle_sha, scratch, **over):
    engine = over.get('engine', 'colima-nerdctl')
    full_id = over.get('full_id', 'a' * 64)
    context = over.get('context', 'qual-579')
    identity = over.get('engine_id', '%s:%s@%s' % (engine, full_id, context))
    lines = [
        'packet ' + over.get('packet', bundle_sha),
        'GO',
        'Tom',
        'task-engine-signals',
        'engine ' + engine,
        'engine-id ' + identity,
        'context ' + context,
        'image-digest ' + over.get('image_digest', 'sha256:' + 'b' * 64),
        'binary ' + over.get('binary', str(scratch / 'candidate' / 'effigy')),
        'binary-sha256 ' + over.get('binary_sha256', 'c' * 64),
        'source ' + over.get('source', 'd' * 40),
        'window ' + over.get('window', '2026-11-03T00:00:00Z'),
        'inventory ' + over.get('inventory', str(scratch / 'inventory-pre.json')),
    ]
    extra = over.get('extra')
    if extra:
        lines.append(extra)
    return '\n'.join(lines) + '\n'


def armed_overlay(scratch, binary_sha=None, source=None, binary_path=None):
    return {
        'armed': True,
        'candidate': {
            'source_commit': source or ('d' * 40),
            'binary_sha256': binary_sha or ('c' * 64),
            'binary_path': binary_path or str(scratch / 'candidate' / 'effigy'),
        },
        'engine': {
            'kind': 'colima-nerdctl',
            'identity': 'colima-nerdctl:%s@qual-579' % ('a' * 64),
            'context': 'qual-579',
            'image_digest': 'sha256:' + 'b' * 64,
        },
    }


def armed_fixture(scratch):
    cand = scratch / 'candidate'
    cand.mkdir(parents=True, exist_ok=True)
    path = cand / 'effigy'
    data = b'labeled-double-candidate\n'
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o700)
    os.write(fd, data)
    os.close(fd)
    sha = hashlib.sha256(data).hexdigest()
    overlay = armed_overlay(scratch, binary_sha=sha, binary_path=str(path))
    return overlay


def write_matching_go(tmp, overlay, **over):
    candidate = overlay['candidate']
    engine = overlay['engine']
    _, rest = engine['identity'].split(':', 1)
    full_id, context = rest.split('@', 1)
    tmpfile(tmp, 'go.txt', go_text(
        BUNDLE, tmp,
        engine=engine['kind'],
        full_id=full_id,
        context=context,
        engine_id=engine['identity'],
        image_digest=engine['image_digest'],
        binary=candidate['binary_path'],
        binary_sha256=candidate['binary_sha256'],
        source=candidate['source_commit'],
        **over,
    ))


FULL_ID = 'a' * 64
IMAGE_DIGEST = 'sha256:' + 'b' * 64
BOOT = 'boot-x'
NS = 'pid:[1]'


def inspect_json(full_id=FULL_ID, image=IMAGE_DIGEST, running=True):
    return json.dumps([{
        'Id': full_id,
        'Created': '2026-01-01T00:00:00Z',
        'State': {'Running': running},
        'Image': image,
    }]).encode()


def status_json(state='not-recorded'):
    return json.dumps({
        'schema': STATUS_SCHEMA,
        'schema_version': 3,
        'task': {'selector': 'hold'},
        'state': state,
        'current': None,
        'latest': None,
        'history': [],
        'warnings': [],
        'routing': {},
    }).encode()


def snap_owned():
    return {
        'kind': 'verified', 'code': 0,
        'stdout': (
            'boot %s\nns %s\np 10 S 1 10 10 100 y\np 11 S 10 10 10 101 y\n' % (BOOT, NS)
        ).encode(),
        'stderr': b'',
    }


def snap_empty():
    return {
        'kind': 'verified', 'code': 0,
        'stdout': ('boot %s\nns %s\n' % (BOOT, NS)).encode(),
        'stderr': b'',
    }


def snap_foreign():
    return {
        'kind': 'verified', 'code': 0,
        'stdout': ('boot %s\nns %s\np 99 S 1 99 99 200 n\n' % (BOOT, NS)).encode(),
        'stderr': b'',
    }


class Script:
    def __init__(self, mode='happy'):
        self.mode = mode
        self.pids = 4242

    def __call__(self, call):
        op = call['op']
        phase = call.get('phase')
        if self.mode == 'lost-snap' and op == 'helper-snap' and phase == 'registered':
            return {'lost': True, 'reason': 'response lost'}
        if self.mode == 'missing-closure' and phase == 'after-signal':
            return snap_owned()
        if self.mode == 'cleanup-partial' and phase == 'post':
            return snap_owned()
        if self.mode == 'cleanup-partial' and op == 'helper-kill':
            return {'kind': 'failed', 'partial': True, 'code': 1, 'stdout': b'partial\n', 'stderr': b''}
        if self.mode == 'missing-image' and op == 'inspect':
            return {'kind': 'failed', 'code': 1, 'stdout': b'', 'stderr': b'no such image'}
        if self.mode == 'unparsable-inspect' and op == 'inspect':
            return {'kind': 'verified', 'code': 0, 'stdout': b'{}\n', 'stderr': b''}
        if self.mode == 'wrong-image' and op == 'inspect':
            return {
                'kind': 'verified', 'code': 0,
                'stdout': inspect_json(image='sha256:' + 'c' * 64),
                'stderr': b'',
            }
        if self.mode == 'failed-pre-snap' and op == 'helper-snap' and phase == 'pre':
            return {'kind': 'failed', 'code': 1, 'stdout': b'', 'stderr': b'exec failed'}
        if self.mode == 'unparsable-snap' and op == 'helper-snap' and phase == 'registered':
            return {'kind': 'verified', 'code': 0, 'stdout': b'garbage\n', 'stderr': b''}
        if self.mode == 'moved-generation' and phase == 'after-signal':
            return {
                'kind': 'verified', 'code': 0,
                'stdout': b'boot other\nns pid:[9]\n',
                'stderr': b'',
            }
        if self.mode == 'bad-status' and op == 'status':
            return {
                'kind': 'verified', 'code': 0, 'gate': 'not-recorded',
                'stdout': b'not-recorded\n', 'stderr': b'',
            }
        if op == 'inspect':
            return {'kind': 'verified', 'code': 0, 'stdout': inspect_json(), 'stderr': b''}
        if op == 'helper-snap':
            if phase == 'registered':
                return snap_owned()
            if phase == 'after-signal':
                return snap_empty()
            if phase == 'foreign':
                return snap_foreign()
            return snap_empty()
        if op == 'workload':
            self.pids += 1
            env = call.get('env') or {}
            nonce = env.get('EFFIGY_CUSTODY_NONCE') or call.get('nonce')
            if not nonce or len(nonce) != NONCE_HEX_LEN:
                raise AssertionError('workload missing per-cell nonce')
            return {
                'kind': 'running', 'code': None, 'pid': self.pids, 'start': '100',
                'stdout': b'', 'stderr': b'',
            }
        if op == 'signal-owner':
            if not call.get('program'):
                raise AssertionError('signal-owner program is missing')
            if not isinstance(call.get('pid'), int):
                raise AssertionError('signal-owner pid is missing')
            return {'kind': 'verified', 'code': 0, 'stdout': b'', 'stderr': b''}
        if op == 'wait-owner':
            return {'kind': 'verified', 'code': 0, 'stdout': b'', 'stderr': b''}
        if op == 'status':
            return {
                'kind': 'verified', 'code': 0,
                'stdout': status_json(), 'stderr': b'',
            }
        if op == 'helper-kill':
            return {'kind': 'verified', 'code': 0, 'stdout': b'sent\n', 'stderr': b''}
        raise AssertionError('unexpected call %r' % {k: call[k] for k in ('op', 'phase', 'cell') if k in call})


def run_execute(tmp, overlay, mode='happy'):
    if not os.path.lexists(tmp / 'go.txt'):
        write_matching_go(tmp, overlay)
    go = verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp)
    working = apply_overlay(load_manifest(), overlay)
    plan = construct_plan(working)
    double = LabeledDouble(Script(mode))
    result = execute_pass(working, go, plan, tmp, transport=double, allow_native=False)
    return result, double


BUNDLE = None


def t_packet_bundle():
    bundle = verify_packet_bundle()
    assert HEX64.fullmatch(bundle['bundle_sha256'])
    assert bundle['manifest']['schema'] == common.SCHEMA
    assert bundle['manifest']['armed'] is False
    assert bundle['manifest']['status'] == 'prepared-not-executed'
    assert bundle['manifest']['qualification'] is False
    assert bundle['bundle_sha256'] == bundle_digest()


def t_helpers_are_plain_files():
    obj = load_manifest()
    for name in obj['helpers']:
        path = DIR / name
        assert path.is_file() and not path.is_symlink(), name


def t_helpers_import_no_subprocess():
    for name in ('common.py', 'launch.py'):
        text = (DIR / name).read_text()
        for needle in ('subprocess', 'os.system', 'os.popen', 'os.execv', 'os.spawn'):
            assert needle not in text, '%s names %s' % (name, needle)
    host = (DIR / 'execute.py').read_text()
    assert 'subprocess.run' in host
    assert 'subprocess.Popen' in host
    assert 'start_new_session' in host
    assert 'HostCliDriver' in host
    assert 'EFFIGY_QUAL_TRANSPORT' not in host
    assert 'TEST_REMOTE' not in host


def t_committed_unarmed():
    m = load_manifest()
    assert m['candidate']['source_commit'] == UNARMED
    assert m['candidate']['binary_sha256'] == UNARMED
    assert m['engine']['kind'] == UNARMED
    assert m['engine']['identity'] == UNARMED
    assert m['engine']['image_digest'] == UNARMED
    expect_refused(lambda: require_armed(m), 'unarmed')


def t_go_missing():
    tmp = owned_tmp()
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'missing')


def t_go_wrong_path():
    tmp = owned_tmp()
    nested = tmp / 'nested'
    nested.mkdir()
    tmpfile(nested, 'go.txt', go_text(BUNDLE, tmp))
    expect_refused(lambda: verify_go_receipt(nested / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'pinned scratch path')


def t_go_wrong_packet():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp, packet='9' * 64))
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'packet SHA')


def t_go_mode():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp), mode=0o644)
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'mode/owner/type')


def t_go_symlink():
    tmp = owned_tmp()
    target = tmpfile(tmp, 'target.txt', go_text(BUNDLE, tmp))
    link = tmp / 'go.txt'
    os.symlink(target, link)
    expect_refused(lambda: verify_go_receipt(link, BUNDLE, scratch_dir=tmp),
                   'pinned scratch path')


def t_go_missing_token():
    tmp = owned_tmp()
    text = go_text(BUNDLE, tmp).replace('Tom\n', '')
    tmpfile(tmp, 'go.txt', text)
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'Tom')


def t_go_fake_engine():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp, engine='fake',
                                   engine_id='fake:%s@qual-579' % ('a' * 64)))
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'fake engine')


def t_go_test_remote():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp, engine='TEST_REMOTE',
                                   engine_id='TEST_REMOTE:%s@qual-579' % ('a' * 64)))
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'fake engine')


def t_go_operator_profile():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(
        BUNDLE, tmp, context='effigy',
        engine_id='colima-nerdctl:%s@effigy' % ('a' * 64)))
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'forbidden')


def t_go_short_id():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(
        BUNDLE, tmp, full_id='abc',
        engine_id='colima-nerdctl:abc@qual-579'))
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   '64 hex')


def t_go_missing_image():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp, image_digest='UNARMED'))
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'preparation prerequisite')


def t_go_inventory_outside_scratch():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp, inventory='/etc/hosts'))
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'owned scratch')


def t_go_forbidden_pkill():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp, extra='pkill -f hold'))
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'forbidden')


def t_go_531_reuse():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp, extra='reuse effigy#531 GO'))
    expect_refused(lambda: verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp),
                   'forbidden')


def t_go_well_formed_still_unarmed():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp))
    record = verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp)
    assert record['engine'] == 'colima-nerdctl'
    expect_refused(lambda: require_armed(load_manifest()), 'unarmed')


def t_launch_execute_unarmed_with_go():
    tmp = owned_tmp()
    tmpfile(tmp, 'go.txt', go_text(BUNDLE, tmp))
    expect_refused(lambda: launch.main(['--execute'], scratch_dir=tmp), 'unarmed')


def t_launch_missing_go_refused():
    tmp = owned_tmp()
    expect_refused(lambda: launch.main(['--execute'], scratch_dir=tmp), 'missing')


def t_construct_plan_shape():
    tmp = owned_tmp()
    plan = construct_plan(load_manifest(), armed_overlay(tmp))
    assert plan['spawn'] is False
    assert plan['qualification'] is False
    assert plan['execute'] is False
    assert plan['adapter'] == 'ComposeRemote<HostCli>'
    assert plan['call_path'] == 'src/runner/task_engine/remote.rs::run_remote'
    assert plan['identity_format'] == 'engine:full-id@context'
    assert plan['deadlines_s'] == {'ack': 60, 'call': 20, 'grace': 3, 'signal_exit': 10}
    assert plan['scratch'] == str(SCRATCH)
    assert plan['fixture_task']['run_in'] == 'container'
    assert plan['fixture_task']['discard'] is False
    assert plan['argv'] == [str(tmp / 'candidate' / 'effigy'), 'hold']
    ids = [c['id'] for c in plan['cells']]
    assert ids == ['container-int', 'container-term', 'container-hup',
                   'container-kill', 'container-foreign-unknown']
    assert [c['signal'] for c in plan['cells'][:4]] == ['INT', 'TERM', 'HUP', 'KILL']
    engine, full_id, context = parse_identity(plan['identity'])
    assert engine == 'colima-nerdctl'
    assert HEX64.fullmatch(full_id)
    assert context == 'qual-579'


def t_construct_refuses_unarmed():
    expect_refused(lambda: construct_plan(load_manifest()), 'unarmed')


def t_construct_refuses_fake_overlay():
    tmp = owned_tmp()
    over = armed_overlay(tmp)
    over['engine']['kind'] = 'fake'
    over['engine']['identity'] = 'fake:%s@qual-579' % ('a' * 64)
    expect_refused(lambda: construct_plan(load_manifest(), over), 'fake engine')


def t_construct_refuses_operator_profile():
    tmp = owned_tmp()
    over = armed_overlay(tmp)
    over['engine']['context'] = 'effigy'
    over['engine']['identity'] = 'colima-nerdctl:%s@effigy' % ('a' * 64)
    expect_refused(lambda: construct_plan(load_manifest(), over), 'forbidden')


def t_construct_refuses_discard():
    tmp = owned_tmp()
    m = apply_overlay(load_manifest(), armed_overlay(tmp))
    m['fixture_task']['discard'] = True
    expect_refused(lambda: construct_plan(m), 'ordinary')


def t_construct_refuses_private_apply():
    tmp = owned_tmp()
    m = apply_overlay(load_manifest(), armed_overlay(tmp))
    m['fixture_task']['workspace_private_apply'] = True
    expect_refused(lambda: construct_plan(m), 'ordinary')


def t_construct_refuses_wrong_deadlines():
    tmp = owned_tmp()
    m = apply_overlay(load_manifest(), armed_overlay(tmp))
    m['deadlines_s']['ack'] = 600
    expect_refused(lambda: construct_plan(m), 'deadlines')


def t_construct_refuses_owner_exit_inference():
    tmp = owned_tmp()
    m = apply_overlay(load_manifest(), armed_overlay(tmp))
    m['closure']['infer_from_owner_exit'] = True
    expect_refused(lambda: construct_plan(m), 'forbidden proxy')


def t_construct_refuses_empty_list_inference():
    tmp = owned_tmp()
    m = apply_overlay(load_manifest(), armed_overlay(tmp))
    m['closure']['infer_from_empty_list'] = True
    expect_refused(lambda: construct_plan(m), 'forbidden proxy')


def t_construct_refuses_fake_transport_inference():
    tmp = owned_tmp()
    m = apply_overlay(load_manifest(), armed_overlay(tmp))
    m['closure']['infer_from_fake_transport'] = True
    expect_refused(lambda: construct_plan(m), 'forbidden proxy')


def t_docker_kind_shape():
    tmp = owned_tmp()
    over = armed_overlay(tmp)
    over['engine']['kind'] = 'docker-compose'
    over['engine']['identity'] = 'docker-compose:%s@qual-579' % ('a' * 64)
    plan = construct_plan(load_manifest(), over)
    assert plan['engine_kind'] == 'docker-compose'
    assert plan['spawn'] is False


def t_matrix_missing_cells_are_real_container():
    matrix = json.loads((DIR / 'matrix.json').read_text())
    missing = [c['id'] for c in matrix['cells'] if c['credit'] == 'missing']
    assert missing == [
        'container-int', 'container-term', 'container-hup',
        'container-kill', 'container-foreign-unknown',
    ]
    shared = [c['id'] for c in matrix['cells'] if c['credit'] == 'shared-path']
    assert shared == []
    host = [c for c in matrix['cells'] if c['credit'] == 'host-source']
    assert [c['id'] for c in host] == ['sequence-term', 'sequence-hup']
    for cell in host:
        assert 'linux-custody.sh' in (cell.get('obligation') or '')
        assert 'shared-path' in (cell.get('receipt') or '')
    accepted_plain = [c['id'] for c in matrix['cells']
                      if c['kind'] == 'plain' and c['credit'] == 'accepted']
    assert 'plain-int' in accepted_plain and 'plain-kill' in accepted_plain
    fake = [c for c in matrix['cells'] if c['id'] == 'container-source-fake-engine'][0]
    assert fake['credit'] == 'accepted-source'
    assert matrix['legacy_shadow_oracle'] == 'retired'
    assert matrix['qualification'] is False
    assert matrix['status'] == 'prepared-not-executed'


def t_matrix_routing_no_census():
    matrix = json.loads((DIR / 'matrix.json').read_text())
    assert 'no new giant census' in matrix['routing']['census']
    ids = [c['id'] for c in matrix['routing']['cells']]
    assert ids == [
        'canonical-taskid', 'one-resolver',
        'consumer-expected-routes', 'legacy-shadow-oracle',
    ]


def t_signal_cell_ids_match_plan():
    assert [c[0] for c in SIGNAL_CELLS] == [
        'container-int', 'container-term', 'container-hup', 'container-kill',
    ]


def t_identity_rejects_non_hex():
    expect_refused(lambda: parse_identity('colima-nerdctl:xyz@qual-579'), '64 hex')


def t_identity_rejects_fake():
    expect_refused(lambda: parse_identity('fake:%s@qual-579' % ('a' * 64)),
                   'production Compose')


def t_execute_py_is_a_helper():
    obj = load_manifest()
    assert 'execute.py' in obj['helpers']
    assert (DIR / 'execute.py').is_file() and not (DIR / 'execute.py').is_symlink()


def t_default_transport_is_hostcli():
    driver = execute.default_transport()
    assert driver.__class__.__name__ == 'HostCliDriver'
    assert driver.label == 'HostCli'


def t_execute_armed_double_records_hostcli_argv():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, double = run_execute(tmp, overlay)
    assert result['kind'] == 'labeled-double'
    assert result['qualification'] is False
    assert result['credit'] == 'source-control-flow'
    assert result['retry'] is False
    helper = production_helper()
    full_id = 'a' * 64
    inspect = next(c for c in double.calls if c['op'] == 'inspect')
    assert inspect['program'] == 'colima'
    assert inspect['argv'][:4] == ['nerdctl', '--profile', 'qual-579', '--']
    assert inspect['argv'][-2:] == ['inspect', full_id]
    snap = next(c for c in double.calls if c['op'] == 'helper-snap' and c.get('phase') == 'registered')
    assert snap['program'] == 'colima'
    assert snap['argv'][4:9] == ['exec', '-i', '-u', '0', full_id]
    assert snap['argv'][9:11] == ['sh', '-c']
    assert snap['argv'][11] == helper
    assert snap['argv'][12] == HELPER_NAME
    assert snap['argv'][13] == 'snap'
    workloads = [c for c in double.calls if c['op'] == 'workload']
    assert len(workloads) == 4
    nonces = []
    for workload in workloads:
        assert workload['program'] == overlay['candidate']['binary_path']
        assert workload['argv'] == ['hold']
        assert workload['program'] not in workload['argv']
        assert workload.get('async') is True
        nonce = (workload.get('env') or {}).get('EFFIGY_CUSTODY_NONCE')
        assert nonce and HEX64.fullmatch(nonce)
        nonces.append(nonce)
    assert len(set(nonces)) == 4
    registered = [c for c in double.calls if c['op'] == 'helper-snap' and c.get('phase') == 'registered']
    assert len(registered) == 4
    for workload, snap_call in zip(workloads, registered):
        nonce = (workload.get('env') or {}).get('EFFIGY_CUSTODY_NONCE')
        assert snap_call.get('input') == (nonce + '\n').encode()
        assert snap_call.get('nonce') == nonce
    owners = [c for c in double.calls if c['op'] == 'signal-owner']
    assert [c['argv'][0] for c in owners] == ['INT', 'TERM', 'HUP', 'KILL']
    for owner in owners:
        assert owner['program'] == overlay['candidate']['binary_path']
        assert isinstance(owner.get('pid'), int) and owner['pid'] > 0
    status_calls = [c for c in double.calls if c['op'] == 'status']
    assert status_calls
    for status in status_calls:
        assert status['program'] == overlay['candidate']['binary_path']
        assert status['argv'] == ['tasks', 'status', 'hold', '--json']
        assert status['argv'][0] != status['program']
    assert not any(c['op'] == 'helper-kill' for c in double.calls)
    ids = [c['id'] for c in result['cells']]
    assert ids[:4] == ['container-int', 'container-term', 'container-hup', 'container-kill']
    assert ids[-1] == 'container-foreign-unknown'
    assert result['cells'][-1]['refused_pids'] == ['99']
    for cell in result['cells'][:4]:
        assert HEX64.fullmatch(cell['nonce'])
        assert cell['generation'] == {'boot': BOOT, 'ns': NS}
        assert cell['owner']['pid']
    blob = json.dumps(
        [{'program': c.get('program'), 'argv': c.get('argv')} for c in double.calls]
    ).lower()
    for fragment in ('pkill', 'killall', 'colima start', 'docker pull'):
        assert fragment not in blob
    invocation = json.loads((tmp / 'invocation-receipt.json').read_text())
    assert invocation['qualification'] is False
    assert invocation['transport'] == 'labeled-double'


def t_execute_one_shot_refuses_resend():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay)
    assert result['kind'] == 'labeled-double'
    expect_refused(lambda: run_execute(tmp, overlay)[0], 'already attempted')


def t_execute_lost_response_is_unknown_not_retry():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay, mode='lost-snap')
    assert result['kind'] == 'unknown'
    assert result['qualification'] is False
    assert result['retry'] is False
    assert 'lost' in result['reason']
    error = json.loads((tmp / 'error-receipt.json').read_text())
    assert error['retry'] is False
    expect_refused(lambda: run_execute(tmp, overlay)[0], 'already attempted')


def t_execute_missing_closure_is_unknown():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay, mode='missing-closure')
    assert result['kind'] == 'unknown'
    assert 'parent exit is not closure' in result['reason']
    assert result['retry'] is False


def t_execute_cleanup_partial_is_failed():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, double = run_execute(tmp, overlay, mode='cleanup-partial')
    assert result['kind'] == 'failed'
    assert 'cleanup partial' in result['reason']
    cleanup = json.loads((tmp / 'cleanup-receipt.json').read_text())
    assert cleanup['failed']
    assert cleanup['retry'] is False
    killed = next(c for c in double.calls if c['op'] == 'helper-kill')
    assert killed['argv'][-7:] == ['kill', BOOT, NS, 'TERM', 'member', '10', '100']
    assert 'boot' not in killed['argv'][-6:-4]
    assert killed['argv'][-6] != 'boot'
    assert killed['argv'][-5] != 'ns'


def t_execute_failure_before_admission_leaves_no_invocation():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    write_matching_go(tmp, overlay, packet='9' * 64)
    expect_refused(
        lambda: launch.main(['--execute'], scratch_dir=tmp, overlay=overlay,
                            transport=LabeledDouble(Script())),
        'packet SHA',
    )
    assert not (tmp / 'invocation-receipt.json').exists()


def t_execute_failure_after_admission_keeps_invocation():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay, mode='missing-image')
    assert result['kind'] == 'failed'
    assert 'preparation prerequisite' in result['reason']
    assert (tmp / 'invocation-receipt.json').is_file()
    expect_refused(lambda: run_execute(tmp, overlay)[0], 'already attempted')


def t_execute_go_mismatch_refuses_before_effects():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    tmpfile(tmp, 'go.txt', go_text(
        BUNDLE, tmp,
        engine='docker-compose',
        engine_id='docker-compose:%s@qual-579' % ('a' * 64),
        context='qual-579',
        image_digest=overlay['engine']['image_digest'],
        binary=overlay['candidate']['binary_path'],
        binary_sha256=overlay['candidate']['binary_sha256'],
        source=overlay['candidate']['source_commit'],
    ))
    expect_refused(
        lambda: launch.main(['--execute'], scratch_dir=tmp, overlay=overlay,
                            transport=LabeledDouble(Script())),
        'does not match',
    )
    assert not (tmp / 'invocation-receipt.json').exists()


def t_execute_native_refused_without_allow_flag():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    write_matching_go(tmp, overlay)
    go = verify_go_receipt(tmp / 'go.txt', BUNDLE, scratch_dir=tmp)
    working = apply_overlay(load_manifest(), overlay)
    plan = construct_plan(working)
    expect_refused(
        lambda: execute_pass(working, go, plan, tmp, transport=None, allow_native=False),
        'HostCli',
    )
    assert not (tmp / 'invocation-receipt.json').exists()


def t_launch_execute_with_double_is_control_flow():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    write_matching_go(tmp, overlay)
    code = launch.main(
        ['--execute'], scratch_dir=tmp, overlay=overlay,
        transport=LabeledDouble(Script()), allow_native=False,
    )
    assert code == 0
    invocation = json.loads((tmp / 'invocation-receipt.json').read_text())
    assert invocation['transport'] == 'labeled-double'
    assert invocation['qualification'] is False


def t_plan_is_never_execution():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    write_matching_go(tmp, overlay)
    code = launch.main([], scratch_dir=tmp, overlay=overlay)
    assert code == 0
    assert not (tmp / 'invocation-receipt.json').exists()


def t_constant_nonce_is_not_forwarded_placeholder():
    """PR713 defect: execute used nonce='n'*32 and never put it on the workload."""
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    _result, double = run_execute(tmp, overlay)
    workloads = [c for c in double.calls if c['op'] == 'workload']
    nonces = [(c.get('env') or {}).get('EFFIGY_CUSTODY_NONCE') for c in workloads]
    assert all(n and n != 'n' * 32 and HEX64.fullmatch(n) for n in nonces)
    assert len(set(nonces)) == 4


def t_signal_owner_program_is_not_none():
    """PR713 defect: signal-owner program=None TypeError on HostCli."""
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    _result, double = run_execute(tmp, overlay)
    owners = [c for c in double.calls if c['op'] == 'signal-owner']
    assert owners
    for owner in owners:
        assert owner.get('program') == overlay['candidate']['binary_path']
        assert owner.get('program') is not None


def t_helper_kill_uses_observed_generation_not_literals():
    """PR713 defect: helper-kill used literal boot/ns rather than the snap."""
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    _result, double = run_execute(tmp, overlay, mode='cleanup-partial')
    killed = next(c for c in double.calls if c['op'] == 'helper-kill')
    mode_args = killed['argv'][killed['argv'].index(HELPER_NAME) + 1:]
    assert mode_args == ['kill', BOOT, NS, 'TERM', 'member', '10', '100']
    expect_refused(
        lambda: helper_kill_tail(FULL_ID, 'boot', 'ns', 'TERM', 'member', '10', '100'),
        'observed generation',
    )


def t_status_argv_does_not_duplicate_binary():
    """PR713 defect: status argv repeated the binary and trusted a modeled gate."""
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    _result, double = run_execute(tmp, overlay)
    status = next(c for c in double.calls if c['op'] == 'status')
    assert status['argv'] == ['tasks', 'status', 'hold', '--json']
    assert status['program'] not in status['argv']


def t_failed_snap_is_failed_observe_not_empty_members():
    """PR713 defect: a failed snap was recorded as members=[] (observed-empty)."""
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay, mode='failed-pre-snap')
    assert result['kind'] == 'unknown'
    assert 'failed-observe' in result['reason']
    inventory = json.loads((tmp / 'inventory-pre.json').read_text())
    assert inventory['observe'] == 'failed'
    assert 'members' not in inventory
    empty = observe_snapshot(snap_empty(), nonce_supplied=False)
    assert empty['observe'] == 'verified'
    assert empty['members'] == []


def t_unparsable_inspect_is_failed_not_exit0():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay, mode='unparsable-inspect')
    assert result['kind'] == 'failed'
    assert 'inspect did not verify' in result['reason']
    assert (tmp / 'invocation-receipt.json').is_file()


def t_wrong_image_inspect_is_failed():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay, mode='wrong-image')
    assert result['kind'] == 'failed'
    assert 'inspect did not verify' in result['reason']


def t_unparsable_registration_snap_is_failed_observe():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay, mode='unparsable-snap')
    assert result['kind'] == 'unknown'
    assert 'failed-observe' in result['reason']


def t_moved_generation_is_unknown():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay, mode='moved-generation')
    assert result['kind'] == 'unknown'
    assert 'generation moved' in result['reason']
    assert result['retry'] is False


def t_modeled_gate_substring_is_not_status():
    tmp = owned_tmp()
    overlay = armed_fixture(tmp)
    result, _double = run_execute(tmp, overlay, mode='bad-status')
    assert result['kind'] == 'unknown'
    assert 'foreground gate was not released' in result['reason']


def t_parse_inspect_and_snapshot_and_status():
    parsed = parse_inspect(inspect_json(), FULL_ID, IMAGE_DIGEST)
    assert parsed['id'] == FULL_ID
    assert parsed['image'] == IMAGE_DIGEST
    assert parse_inspect(b'[]', FULL_ID, IMAGE_DIGEST) is None
    assert parse_inspect(inspect_json(running=False), FULL_ID, IMAGE_DIGEST) is None
    snap = parse_snapshot(snap_owned()['stdout'])
    assert snap['boot'] == BOOT
    assert snap['ns'] == NS
    assert parse_snapshot(b'boot x\nns y\n') is None
    status = parse_status(status_json())
    assert status['state'] == 'not-recorded'
    assert parse_status(b'not-recorded\n') is None


def t_nonce_width_matches_kernel_digest():
    nonce = generate_nonce()
    assert HEX64.fullmatch(nonce)
    assert nonce != generate_nonce()


def write_exec(path, body):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o700)
    os.write(fd, body if isinstance(body, bytes) else body.encode())
    os.close(fd)
    os.chmod(path, 0o700)
    return path


def reap_driver(driver):
    for pid, rec in list(getattr(driver, 'owners', {}).items()):
        proc = rec.get('proc')
        if proc is None:
            continue
        if proc.poll() is None:
            try:
                os.kill(pid, os_signal.SIGTERM)
            except OSError:
                pass
            try:
                proc.wait(timeout=1)
            except Exception:
                try:
                    os.kill(pid, os_signal.SIGKILL)
                except OSError:
                    pass
                try:
                    proc.wait(timeout=1)
                except Exception:
                    pass


FAKE_COLIMA = r'''#!/usr/bin/env python3
import json, os, sys, pathlib
log = pathlib.Path(os.environ['TES_FAKE_LOG'])
args = sys.argv[1:]
stdin = sys.stdin.read()
with log.open('a') as fh:
    fh.write(json.dumps({'argv': args, 'stdin': stdin}) + '\n')
tail = args[args.index('--') + 1:] if '--' in args else args
if 'inspect' in tail:
    print(os.environ['TES_INSPECT'])
    raise SystemExit(0)
if 'snap' in tail:
    nonce = stdin.split('\n', 1)[0]
    print('boot %s' % os.environ.get('TES_BOOT', 'boot-x'))
    print('ns %s' % os.environ.get('TES_NS', 'pid:[1]'))
    if nonce:
        print('p 10 S 1 10 10 100 y')
    raise SystemExit(0)
if 'kill' in tail:
    helper = 'effigy-custody'
    mode = tail[tail.index(helper) + 1:] if helper in tail else tail
    boot = os.environ.get('TES_BOOT', 'boot-x')
    ns = os.environ.get('TES_NS', 'pid:[1]')
    if len(mode) < 3 or mode[1] != boot or mode[2] != ns:
        print('moved')
        raise SystemExit(0)
    print('sent')
    raise SystemExit(0)
raise SystemExit(2)
'''

FAKE_CANDIDATE = r'''#!/usr/bin/env python3
import json, os, signal, sys, time, pathlib
record = pathlib.Path(os.environ['TES_OWNER_RECORD'])
log = pathlib.Path(os.environ['TES_FAKE_LOG'])
with log.open('a') as fh:
    fh.write(json.dumps({
        'argv': sys.argv,
        'nonce': os.environ.get('EFFIGY_CUSTODY_NONCE'),
        'pid': os.getpid(),
    }) + '\n')
if len(sys.argv) >= 2 and sys.argv[1] == 'tasks':
    print(json.dumps({
        'schema': 'effigy.tasks-status.v3',
        'schema_version': 3,
        'task': {'selector': 'hold'},
        'state': 'not-recorded',
        'current': None,
        'latest': None,
        'history': [],
        'warnings': [],
        'routing': {},
    }))
    raise SystemExit(0)
got = []
def handler(signum, _frame):
    got.append(signum)
    record.write_text(json.dumps({
        'pid': os.getpid(),
        'nonce': os.environ.get('EFFIGY_CUSTODY_NONCE'),
        'signal': signum,
        'argv': sys.argv,
    }))
    raise SystemExit(128 + signum)
for item in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
    signal.signal(item, handler)
record.write_text(json.dumps({
    'pid': os.getpid(),
    'nonce': os.environ.get('EFFIGY_CUSTODY_NONCE'),
    'argv': sys.argv,
    'running': True,
}))
if os.environ.get('TES_IGNORE_TERM') == '1':
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    time.sleep(30)
    raise SystemExit(0)
while True:
    time.sleep(0.05)
'''


def hostcli_env(tmp, inspect=None, ignore_term=False):
    bin_dir = tmp / 'bin'
    bin_dir.mkdir(parents=True, exist_ok=True)
    write_exec(bin_dir / 'colima', FAKE_COLIMA)
    candidate = tmp / 'candidate' / 'effigy'
    candidate.parent.mkdir(parents=True, exist_ok=True)
    write_exec(candidate, FAKE_CANDIDATE)
    env = os.environ.copy()
    env['PATH'] = str(bin_dir) + os.pathsep + env.get('PATH', '')
    env['TES_FAKE_LOG'] = str(tmp / 'fake.log')
    env['TES_OWNER_RECORD'] = str(tmp / 'owner.json')
    env['TES_INSPECT'] = (inspect or inspect_json()).decode()
    env['TES_BOOT'] = BOOT
    env['TES_NS'] = NS
    env['TES_FULL_ID'] = FULL_ID
    env['TES_IMAGE'] = IMAGE_DIGEST
    if ignore_term:
        env['TES_IGNORE_TERM'] = '1'
    Path(env['TES_FAKE_LOG']).write_text('')
    return env, candidate


def t_hostcli_none_program_is_refused_not_typeerror():
    """PR713 defect: HostCli TypeError when program is None."""
    driver = HostCliDriver()
    try:
        driver.call({'op': 'signal-owner', 'program': None, 'argv': ['TERM']})
    except Refused as exc:
        assert 'program' in str(exc)
        return
    except TypeError as exc:
        raise AssertionError('HostCli TypeError on missing program: %s' % exc) from exc
    raise AssertionError('expected a fail-closed refusal, got none')


def t_hostcli_fixture_inspect_argv_stdin_and_parse():
    tmp = owned_tmp()
    env, _candidate = hostcli_env(tmp)
    driver = HostCliDriver(env=env)
    try:
        result = driver.call({
            'op': 'inspect',
            'program': 'colima',
            'argv': ['nerdctl', '--profile', 'qual-579', '--', 'inspect', FULL_ID],
            'deadline_s': 5,
        })
        assert result['kind'] == 'verified'
        parsed = parse_inspect(result['stdout'], FULL_ID, IMAGE_DIGEST)
        assert parsed['id'] == FULL_ID
        rows = [json.loads(line) for line in Path(env['TES_FAKE_LOG']).read_text().splitlines() if line]
        assert rows[0]['argv'][:4] == ['nerdctl', '--profile', 'qual-579', '--']
        assert rows[0]['argv'][-2:] == ['inspect', FULL_ID]
    finally:
        reap_driver(driver)


def t_hostcli_fixture_snap_forwards_nonce_and_generation():
    tmp = owned_tmp()
    env, _candidate = hostcli_env(tmp)
    driver = HostCliDriver(env=env)
    nonce = generate_nonce()
    try:
        result = driver.call({
            'op': 'helper-snap',
            'program': 'colima',
            'argv': [
                'nerdctl', '--profile', 'qual-579', '--',
                'exec', '-i', '-u', '0', FULL_ID, 'sh', '-c', 'true', HELPER_NAME, 'snap',
            ],
            'input': (nonce + '\n').encode(),
            'deadline_s': 5,
        })
        parsed = parse_snapshot(result['stdout'])
        assert parsed['boot'] == BOOT
        assert parsed['ns'] == NS
        assert parsed['members'][0]['nonce'] == 'y'
        rows = [json.loads(line) for line in Path(env['TES_FAKE_LOG']).read_text().splitlines() if line]
        assert rows[-1]['stdin'].split('\n', 1)[0] == nonce
        killed = driver.call({
            'op': 'helper-kill',
            'program': 'colima',
            'argv': [
                'nerdctl', '--profile', 'qual-579', '--',
                'exec', '-i', '-u', '0', FULL_ID, 'sh', '-c', 'true', HELPER_NAME,
                'kill', parsed['boot'], parsed['ns'], 'TERM', 'member', '10', '100',
            ],
            'input': (nonce + '\n').encode(),
            'deadline_s': 5,
        })
        assert decode_output(killed['stdout']).strip() == 'sent'
        moved = driver.call({
            'op': 'helper-kill',
            'program': 'colima',
            'argv': [
                'nerdctl', '--profile', 'qual-579', '--',
                'exec', '-i', '-u', '0', FULL_ID, 'sh', '-c', 'true', HELPER_NAME,
                'kill', 'boot', 'ns', 'TERM', 'member', '10', '100',
            ],
            'input': (nonce + '\n').encode(),
            'deadline_s': 5,
        })
        assert decode_output(moved['stdout']).strip() == 'moved'
    finally:
        reap_driver(driver)


def t_hostcli_fixture_async_owner_signal_wait():
    tmp = owned_tmp()
    env, candidate = hostcli_env(tmp)
    driver = HostCliDriver(env=env)
    nonce = generate_nonce()
    try:
        started = driver.start({
            'op': 'workload',
            'program': str(candidate),
            'argv': ['hold'],
            'env': {'EFFIGY_CUSTODY_NONCE': nonce},
            'deadline_s': 5,
        })
        assert started['kind'] == 'running'
        pid = started['pid']
        assert pid > 0
        assert driver.owners[pid]['proc'].poll() is None
        deadline = time.time() + 5
        while time.time() < deadline and not Path(env['TES_OWNER_RECORD']).exists():
            time.sleep(0.05)
        owner = json.loads(Path(env['TES_OWNER_RECORD']).read_text())
        assert owner['nonce'] == nonce
        assert owner['pid'] == pid
        assert owner['argv'] == [str(candidate), 'hold']
        driver.signal({
            'op': 'signal-owner',
            'program': str(candidate),
            'argv': ['TERM'],
            'pid': pid,
            'start': started.get('start'),
            'deadline_s': 3,
        })
        waited = driver.wait({
            'op': 'wait-owner',
            'pid': pid,
            'deadline_s': 5,
        })
        assert waited['code'] == 128 + int(os_signal.SIGTERM)
        owner = json.loads(Path(env['TES_OWNER_RECORD']).read_text())
        assert owner['signal'] == int(os_signal.SIGTERM)
        status = driver.call({
            'op': 'status',
            'program': str(candidate),
            'argv': ['tasks', 'status', 'hold', '--json'],
            'deadline_s': 5,
        })
        parsed = parse_status(status['stdout'])
        assert parsed['schema'] == STATUS_SCHEMA
        assert parsed['state'] == 'not-recorded'
    finally:
        reap_driver(driver)


def t_hostcli_fixture_wait_timeout_is_lost_not_kill():
    tmp = owned_tmp()
    env, candidate = hostcli_env(tmp, ignore_term=True)
    driver = HostCliDriver(env=env)
    try:
        started = driver.start({
            'op': 'workload',
            'program': str(candidate),
            'argv': ['hold'],
            'env': {'EFFIGY_CUSTODY_NONCE': generate_nonce()},
            'deadline_s': 5,
        })
        pid = started['pid']
        try:
            driver.wait({'op': 'wait-owner', 'pid': pid, 'deadline_s': 1})
        except LostResponse as exc:
            assert 'wait exceeded' in str(exc)
        else:
            raise AssertionError('expected LostResponse on owner wait timeout')
        assert driver.owners[pid]['proc'].poll() is None
    finally:
        reap_driver(driver)


def t_hostcli_fixture_foreign_pid_is_refused():
    tmp = owned_tmp()
    env, candidate = hostcli_env(tmp)
    driver = HostCliDriver(env=env)
    try:
        started = driver.start({
            'op': 'workload',
            'program': str(candidate),
            'argv': ['hold'],
            'env': {'EFFIGY_CUSTODY_NONCE': generate_nonce()},
        })
        expect_refused(
            lambda: driver.signal({
                'op': 'signal-owner',
                'program': str(candidate),
                'argv': ['TERM'],
                'pid': started['pid'] + 1,
            }),
            'live recorded owner',
        )
    finally:
        reap_driver(driver)


def t_hostcli_owner_missing_start_identity_is_refused():
    tmp = owned_tmp()
    env, candidate = hostcli_env(tmp)
    driver = HostCliDriver(env=env)
    try:
        started = driver.start({'op': 'workload', 'program': str(candidate), 'argv': ['hold']})
        expect_refused(
            lambda: driver.signal({
                'op': 'signal-owner', 'program': str(candidate), 'argv': ['TERM'],
                'pid': started['pid'], 'start': None,
            }),
            'start identity',
        )
        original = execute.process_start_identity
        try:
            for observed in (None, 'moved'):
                execute.process_start_identity = lambda _pid: observed
                expect_refused(
                    lambda: driver.signal({
                        'op': 'signal-owner', 'program': str(candidate), 'argv': ['TERM'],
                        'pid': started['pid'], 'start': started['start'],
                    }),
                    'start identity',
                )
        finally:
            execute.process_start_identity = original
        assert driver.owners[started['pid']]['proc'].poll() is None
    finally:
        reap_driver(driver)


def t_hostcli_unparsable_inspect_stdout_is_not_verified():
    tmp = owned_tmp()
    env, _candidate = hostcli_env(tmp, inspect=b'[]')
    driver = HostCliDriver(env=env)
    try:
        result = driver.call({
            'op': 'inspect',
            'program': 'colima',
            'argv': ['nerdctl', '--profile', 'qual-579', '--', 'inspect', FULL_ID],
            'deadline_s': 5,
        })
        assert result['kind'] == 'verified'
        assert result['code'] == 0
        assert parse_inspect(result['stdout'], FULL_ID, IMAGE_DIGEST) is None
    finally:
        reap_driver(driver)


CASES = [
    t_hostcli_owner_missing_start_identity_is_refused,
    t_packet_bundle,
    t_helpers_are_plain_files,
    t_helpers_import_no_subprocess,
    t_committed_unarmed,
    t_go_missing,
    t_go_wrong_path,
    t_go_wrong_packet,
    t_go_mode,
    t_go_symlink,
    t_go_missing_token,
    t_go_fake_engine,
    t_go_test_remote,
    t_go_operator_profile,
    t_go_short_id,
    t_go_missing_image,
    t_go_inventory_outside_scratch,
    t_go_forbidden_pkill,
    t_go_531_reuse,
    t_go_well_formed_still_unarmed,
    t_launch_execute_unarmed_with_go,
    t_launch_missing_go_refused,
    t_construct_plan_shape,
    t_construct_refuses_unarmed,
    t_construct_refuses_fake_overlay,
    t_construct_refuses_operator_profile,
    t_construct_refuses_discard,
    t_construct_refuses_private_apply,
    t_construct_refuses_wrong_deadlines,
    t_construct_refuses_owner_exit_inference,
    t_construct_refuses_empty_list_inference,
    t_construct_refuses_fake_transport_inference,
    t_docker_kind_shape,
    t_matrix_missing_cells_are_real_container,
    t_matrix_routing_no_census,
    t_signal_cell_ids_match_plan,
    t_identity_rejects_non_hex,
    t_identity_rejects_fake,
    t_execute_py_is_a_helper,
    t_default_transport_is_hostcli,
    t_execute_armed_double_records_hostcli_argv,
    t_execute_one_shot_refuses_resend,
    t_execute_lost_response_is_unknown_not_retry,
    t_execute_missing_closure_is_unknown,
    t_execute_cleanup_partial_is_failed,
    t_execute_failure_before_admission_leaves_no_invocation,
    t_execute_failure_after_admission_keeps_invocation,
    t_execute_go_mismatch_refuses_before_effects,
    t_execute_native_refused_without_allow_flag,
    t_launch_execute_with_double_is_control_flow,
    t_plan_is_never_execution,
    t_constant_nonce_is_not_forwarded_placeholder,
    t_signal_owner_program_is_not_none,
    t_helper_kill_uses_observed_generation_not_literals,
    t_status_argv_does_not_duplicate_binary,
    t_failed_snap_is_failed_observe_not_empty_members,
    t_unparsable_inspect_is_failed_not_exit0,
    t_wrong_image_inspect_is_failed,
    t_unparsable_registration_snap_is_failed_observe,
    t_moved_generation_is_unknown,
    t_modeled_gate_substring_is_not_status,
    t_parse_inspect_and_snapshot_and_status,
    t_nonce_width_matches_kernel_digest,
    t_hostcli_none_program_is_refused_not_typeerror,
    t_hostcli_fixture_inspect_argv_stdin_and_parse,
    t_hostcli_fixture_snap_forwards_nonce_and_generation,
    t_hostcli_fixture_async_owner_signal_wait,
    t_hostcli_fixture_wait_timeout_is_lost_not_kill,
    t_hostcli_fixture_foreign_pid_is_refused,
    t_hostcli_unparsable_inspect_stdout_is_not_verified,
]


def main():
    global BUNDLE
    BUNDLE = bundle_digest()
    for fn in CASES:
        check(fn.__name__, fn)
    print('%d passed, %d failed' % (len(PASS), len(FAIL)))
    if FAIL:
        for item in FAIL:
            print(item)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

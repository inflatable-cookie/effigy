#!/usr/bin/env python3
"""Committed refusal-boundary battery for the task-engine signal packet.

Queue task effigy#579. Installed-Effigy selector
`test:qualification:task-engine-signals` runs only this file. Fake backends
and disposable fixture metadata under this run's TMPDIR (or
/Volumes/Builds/scratch/effigy/579/) — no VM, engine, container, image,
install, or native task. Passing these controls is not native closure.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402
import launch  # noqa: E402
from common import (  # noqa: E402
    DIR, HEX40, HEX64, SCRATCH, SIGNAL_CELLS, UNARMED, Refused,
    apply_overlay, bundle_digest, construct_plan, load_manifest,
    parse_identity, require_armed, verify_go_receipt, verify_packet_bundle,
)

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


def armed_overlay(scratch):
    return {
        'armed': True,
        'candidate': {
            'source_commit': 'd' * 40,
            'binary_sha256': 'c' * 64,
            'binary_path': str(scratch / 'candidate' / 'effigy'),
        },
        'engine': {
            'kind': 'colima-nerdctl',
            'identity': 'colima-nerdctl:%s@qual-579' % ('a' * 64),
            'context': 'qual-579',
            'image_digest': 'sha256:' + 'b' * 64,
        },
    }


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


def t_launch_execute_refused():
    expect_refused(lambda: launch.main(['--execute']), 'execute')


def t_launch_missing_go_refused():
    expect_refused(lambda: launch.main([]), 'missing')


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
    assert shared == ['sequence-term', 'sequence-hup']
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


CASES = [
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
    t_launch_execute_refused,
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

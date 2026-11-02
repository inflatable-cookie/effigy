#!/usr/bin/env python3
"""Committed refusal-boundary battery for the Acow ordinary-Compose packet.

Queue task effigy#540. This is the only file the installed-Effigy selector
`test:qualification:acow` executes. It runs the packet's fail-closed
boundaries against fake backends and disposable fixture metadata under
/Volumes/Builds/scratch/effigy/540/ — no real runtime call, no consumer
stack, no vault read, no warm-up, no build/install, no native probe ever
happens here. Passing these controls is not consumer qualification; the
packet's proof routes stay separately GO-armed (packet-r1.md).
"""
import hashlib
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402
import primary  # noqa: E402
import warmup  # noqa: E402
from common import (  # noqa: E402
    BUNDLE_LOCK_COMMIT, CANDIDATE_BINARY_SHA256, CANDIDATE_VERSION_MARKER,
    EXPECTED_PRIMARY_COUNTS, GO_PRIMARY_TOKENS, GO_WARMUP_TOKENS,
    QUEUE_SHAPE_TOTAL_LEN, Refused, assert_no_scoped_containers,
    assert_no_warmup_scope, assert_packet_dir, capture_inventory,
    container_state, ensure_owned_dir, exact_ps_ids, judge_teardown_inventory,
    label_length_only, parse_network_rows, parse_volume_names,
    queue_shape_components, read_owner_marker, require_mariadb_unchanged,
    scoped_container_map, session_identity, validate_attach_record,
    verify_539_receipt, verify_bundle_lock, verify_consumer_pins,
    verify_go_receipt, verify_installed_identity, verify_packet_bundle,
    volume_mount_identity, write_owner_marker,
)
from primary import compare_counts, observation_plan  # noqa: E402

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


def tmpfile(root, name, data, mode=0o600):
    p = root / name
    fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), mode)
    with os.fdopen(fd, 'wb') as f:
        f.write(data if isinstance(data, bytes) else data.encode())
    os.chmod(p, mode)
    return p


def sha(data):
    return hashlib.sha256(data if isinstance(data, bytes) else data.encode()).hexdigest()


PACKET_SHA = None
FAKE_RESERVATION = 'acow-reservation-2026-1103-a1'


def warmup_go_text(packet_sha, **over):
    parts = [
        'packet ' + over.get('packet', packet_sha),
        'GO',
        'Supervisor',
        'Tom',
        'colima idle',
        'reservation ' + FAKE_RESERVATION,
        'queue unpaused',
        'qa fence null',
        '539 receipt ' + over.get('r539', 'b' * 64),
        'version ' + over.get('version', 'effigy v0.14.1+local.5f46691'),
        'binary ' + over.get('binary', '/Volumes/Builds/scratch/effigy/540/fake-install/effigy'),
        CANDIDATE_BINARY_SHA256,
    ]
    return '\n'.join(parts) + '\n'


def primary_go_text(packet_sha, **over):
    return '\n'.join([
        'packet ' + over.get('packet', packet_sha),
        'GO',
        'Tom',
        'primary-data',
        'reservation ' + FAKE_RESERVATION,
        'queue unpaused',
        'qa fence null',
    ]) + '\n'


# --- packet self-integrity -------------------------------------------------

def t_packet_bundle(tmp):
    bundle = verify_packet_bundle()
    assert len(bundle['packet_sha256']) == 64
    assert bundle['manifest']['schema'] == 'effigy.qualification.acow-ordinary-compose.r1'


def t_manifest_helpers_match_disk(tmp):
    obj = json.loads((Path(common.DIR) / 'manifest-r1.json').read_text())
    for name, want in obj['helpers'].items():
        got = hashlib.sha256((Path(common.DIR) / name).read_bytes()).hexdigest()
        assert got == want, 'manifest helper hash drift: ' + name
    assert 'boundaries.py' in obj['helpers']


# --- GO receipts -----------------------------------------------------------

def t_go_missing(tmp):
    expect_refused(lambda: verify_go_receipt(tmp / 'nope.txt', PACKET_SHA,
                                             GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'missing')


def t_go_wrong_path(tmp):
    nested = tmp / 'nested'
    nested.mkdir()
    p = tmpfile(nested, 'go.txt', warmup_go_text(PACKET_SHA))
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'pinned scratch path')


def t_go_wrong_packet(tmp):
    p = tmpfile(tmp, 'go.txt', warmup_go_text(PACKET_SHA, packet='9' * 64))
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'packet SHA')


def t_go_mode(tmp):
    p = tmpfile(tmp, 'go.txt', warmup_go_text(PACKET_SHA), mode=0o644)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'mode/owner/type')


def t_go_symlink(tmp):
    target = tmpfile(tmp, 'target.txt', warmup_go_text(PACKET_SHA))
    link = tmp / 'go.txt'
    os.symlink(target, link)
    expect_refused(lambda: verify_go_receipt(link, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'pinned scratch path')


def t_go_missing_tokens(tmp):
    for i, token in enumerate(('GO', 'Supervisor', 'Tom', 'colima idle',
                               'queue unpaused', 'qa fence null',
                               CANDIDATE_BINARY_SHA256)):
        text = warmup_go_text(PACKET_SHA).replace(token, 'REDACTED-TOKEN')
        p = tmpfile(tmp, 'go-token-%d.txt' % i, text)
        expect_refused(lambda p=p, text=text: verify_go_receipt(
            p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup', scratch_dir=tmp),
            token)


def t_go_missing_reservation(tmp):
    text = warmup_go_text(PACKET_SHA).replace('reservation ' + FAKE_RESERVATION, 'reservation')
    p = tmpfile(tmp, 'go.txt', text)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'reservation')


def t_go_missing_539_line(tmp):
    text = warmup_go_text(PACKET_SHA).replace('539 receipt ' + 'b' * 64, '')
    p = tmpfile(tmp, 'go.txt', text)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   '539 receipt')


def t_go_missing_version_line(tmp):
    text = warmup_go_text(PACKET_SHA).replace(
        'version effigy v0.14.1+local.5f46691', 'installed build present')
    p = tmpfile(tmp, 'go.txt', text)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'version')


def t_go_missing_binary_line(tmp):
    text = warmup_go_text(PACKET_SHA).replace(
        'binary /Volumes/Builds/scratch/effigy/540/fake-install/effigy',
        'binary-at /Volumes/Builds/scratch/effigy/540/fake-install/effigy')
    p = tmpfile(tmp, 'go.txt', text)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'binary')


def t_go_warmup_valid(tmp):
    p = tmpfile(tmp, 'go.txt', warmup_go_text(PACKET_SHA))
    rec = verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup', scratch_dir=tmp)
    assert rec['reservation_id'] == FAKE_RESERVATION
    assert rec['receipt539_sha256'] == 'b' * 64


def t_route_separation(tmp):
    # A valid warm-up GO verified as the primary route refuses: the routes are
    # separately armed; a warm-up receipt never arms counts observation.
    p = tmpfile(tmp, 'go.txt', warmup_go_text(PACKET_SHA))
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_PRIMARY_TOKENS, 'primary',
                                             scratch_dir=tmp),
                   'primary-data')
    # A valid primary GO verified as the warm-up route refuses: it carries no
    # Supervisor/539/version/binary provenance.
    p2 = tmpfile(tmp, 'go2.txt', primary_go_text(PACKET_SHA))
    expect_refused(lambda: verify_go_receipt(p2, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'Supervisor')


def t_primary_go_valid(tmp):
    p = tmpfile(tmp, 'go.txt', primary_go_text(PACKET_SHA))
    rec = verify_go_receipt(p, PACKET_SHA, GO_PRIMARY_TOKENS, 'primary', scratch_dir=tmp)
    assert rec['route'] == 'primary' and rec['reservation_id'] == FAKE_RESERVATION


# --- 539 prerequisite receipt ----------------------------------------------

def r539_valid():
    return json.dumps({'task': 'effigy#539', 'status': 'green',
                       'head': 'a' * 40,
                       'selectors': ['proof:some:selector']}) + '\n'


def t_539_missing(tmp):
    expect_refused(lambda: verify_539_receipt(tmp / 'none.json', sha(r539_valid())))


def t_539_hash_drift(tmp):
    p = tmpfile(tmp, '539-green-receipt.json', r539_valid())
    expect_refused(lambda: verify_539_receipt(p, 'c' * 64), 'differs')


def t_539_not_json(tmp):
    p = tmpfile(tmp, '539-green-receipt.json', 'not json at all')
    expect_refused(lambda: verify_539_receipt(p, sha('not json at all')), 'JSON')


def t_539_wrong_task(tmp):
    text = r539_valid().replace('effigy#539', 'effigy#540')
    p = tmpfile(tmp, '539-green-receipt.json', text)
    expect_refused(lambda: verify_539_receipt(p, sha(text)), 'effigy#539')


def t_539_not_green(tmp):
    text = r539_valid().replace('"green"', '"yellow"')
    p = tmpfile(tmp, '539-green-receipt.json', text)
    expect_refused(lambda: verify_539_receipt(p, sha(text)), 'green')


def t_539_bad_head(tmp):
    text = r539_valid().replace('a' * 40, 'zz')
    p = tmpfile(tmp, '539-green-receipt.json', text)
    expect_refused(lambda: verify_539_receipt(p, sha(text)), '40-hex')


def t_539_no_selectors(tmp):
    text = r539_valid().replace('"proof:some:selector"', '""')
    p = tmpfile(tmp, '539-green-receipt.json', text)
    expect_refused(lambda: verify_539_receipt(p, sha(text)), 'selectors')


def t_539_valid(tmp):
    p = tmpfile(tmp, '539-green-receipt.json', r539_valid())
    rec = verify_539_receipt(p, sha(r539_valid()))
    assert rec['task'] == 'effigy#539' and rec['status'] == 'green'


# --- installed identity ----------------------------------------------------

class FakeBin:
    def __init__(self, root, payload):
        self.path = root / 'fake-effigy'
        fd = os.open(self.path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        os.write(fd, payload)
        os.close(fd)

    def __str__(self):
        return str(self.path)


def t_identity_which_none(tmp):
    expect_refused(lambda: verify_installed_identity(
        lambda: None, lambda p: 'x', '/nonexistent', lambda p: 'h', 'v'),
        'PATH')


def t_identity_version_mismatch(tmp):
    binp = FakeBin(tmp, b'fake-payload')
    expect_refused(lambda: verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v1.0+local.other',
        str(binp), lambda p: CANDIDATE_BINARY_SHA256,
        'effigy v0.14.1+local.5f46691'),
        'version')


def t_identity_marker_missing(tmp):
    binp = FakeBin(tmp, b'fake-payload')
    expect_refused(lambda: verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v9.9.9+local.abcdef0',
        str(binp), lambda p: CANDIDATE_BINARY_SHA256,
        'effigy v9.9.9+local.abcdef0'),
        'marker')


def t_identity_hash_mismatch(tmp):
    binp = FakeBin(tmp, b'not-the-candidate')
    expect_refused(lambda: verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v0.14.1+local.5f46691',
        str(binp), lambda p: 'd' * 64,
        'effigy v0.14.1+local.5f46691'),
        'binary hash')


def t_identity_symlink_binary(tmp):
    real = FakeBin(tmp, b'x')
    link = tmp / 'link-effigy'
    os.symlink(real.path, link)
    expect_refused(lambda: verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v0.14.1+local.5f46691',
        str(link), lambda p: CANDIDATE_BINARY_SHA256,
        'effigy v0.14.1+local.5f46691'),
        'regular file')


def t_identity_valid(tmp):
    binp = FakeBin(tmp, b'candidate-payload')
    rec = verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v0.14.1+local.5f46691',
        str(binp), lambda p: CANDIDATE_BINARY_SHA256,
        'effigy v0.14.1+local.5f46691')
    assert rec['binary_sha256'] == CANDIDATE_BINARY_SHA256
    assert CANDIDATE_VERSION_MARKER in rec['version_line']


# --- fixture path shape ----------------------------------------------------

def t_shape_length(tmp):
    shape = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    real = '/Volumes/Builds/queue/workspaces/%s/ns-%s' % (
        '0a95477a55a47e8a77ef32eee19e29ecdb90c123737506dca62d66413a2c7b74',
        common.QUEUE_SHAPE_UUID)
    assert shape['length'] == len(real) == QUEUE_SHAPE_TOTAL_LEN == 137
    assert shape['path'].endswith('/ns-' + common.QUEUE_SHAPE_UUID)


def t_shape_deterministic(tmp):
    a = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    b = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    assert a['path'] == b['path'] and len(a['hex']) == 64


def t_shape_bad_parent_length(tmp):
    expect_refused(lambda: queue_shape_components(str(tmp), PACKET_SHA),
                   'Queue QA shape')


def t_shape_foreign_prefix(tmp):
    expect_refused(lambda: queue_shape_components('/tmp/' + 'x' * 27, PACKET_SHA),
                   'scratch root')


# --- inventory parsing and judgment ----------------------------------------

def t_ps_ids(tmp):
    good = 'a' * 64 + '\n' + 'b' * 64 + '\n'
    assert exact_ps_ids(good) == sorted(['a' * 64, 'b' * 64])
    expect_refused(lambda: exact_ps_ids('zz\n'), 'malformed')
    expect_refused(lambda: exact_ps_ids('a' * 64 + '\n' + 'a' * 64 + '\n'), 'duplicate')


def t_volumes(tmp):
    assert parse_volume_names('vol-a\nvol.b-1\n') == ['vol-a', 'vol.b-1']
    expect_refused(lambda: parse_volume_names('bad name\n'), 'malformed')
    expect_refused(lambda: parse_volume_names('v\nv\n'), 'duplicate')


def t_networks(tmp):
    listed = parse_network_rows('net1\taaaaaaaaaaaa\nnet2\tbbbbbbbbbbbb\n')
    assert listed['net1'] == 'aaaaaaaaaaaa'
    expect_refused(lambda: parse_network_rows('onlyname\n'), 'ambiguous')
    expect_refused(lambda: parse_network_rows('n\ta\nn\tb\n'), 'ambiguous')


def fake_inventory(containers=(), volumes=(), networks=None):
    return {'containers': sorted(containers), 'volumes': sorted(volumes),
            'networks': networks if networks is not None else {'host': '', 'none': ''}}


def t_capture_inventory(tmp):
    calls = []

    def runner(args):
        calls.append(tuple(args))
        if args[0] == 'ps':
            return 0, 'b' * 64 + '\n' + 'a' * 64 + '\n', ''
        if args[0] == 'volume':
            return 0, 'vol-b\nvol-a\n', ''
        return 0, 'net1\t1234567890ab\nhost\t\nnone\t\n', ''

    inv = capture_inventory(runner)
    assert inv['containers'] == sorted(['a' * 64, 'b' * 64])
    assert inv['volumes'] == ['vol-a', 'vol-b']
    assert len(calls) == 3

    def bad_runner(args):
        return 1, '', 'boom'

    expect_refused(lambda: capture_inventory(bad_runner), 'read failed')


def t_mariadb_guard(tmp):
    def ok_runner(args):
        return 0, common.SHARED_MARIADB_CONTAINER + ' running\n', ''

    rec = require_mariadb_unchanged(ok_runner)
    assert rec['container'] == common.SHARED_MARIADB_CONTAINER

    def gone(args):
        return 1, '', 'no such'

    expect_refused(lambda: require_mariadb_unchanged(gone), 'MariaDB')

    def exited(args):
        return 0, common.SHARED_MARIADB_CONTAINER + ' exited\n', ''

    expect_refused(lambda: require_mariadb_unchanged(exited), 'identity or state')

    def two(args):
        return 0, common.SHARED_MARIADB_CONTAINER + ' running\nother running\n', ''

    expect_refused(lambda: require_mariadb_unchanged(two), 'ambiguous')


def t_volume_identity(tmp):
    def ok_runner(args):
        assert args[0] == 'volume' and args[1] == 'inspect'
        return 0, 'effigy-shared-mariadb-0721de3e-mysql-data /var/lib/nerdctl/volumes/1\n', ''

    mount = volume_mount_identity(ok_runner, 'effigy-shared-mariadb-0721de3e-mysql-data')
    assert mount.startswith('/var/lib/nerdctl/')

    def missing(args):
        return 1, '', 'none'

    expect_refused(lambda: volume_mount_identity(missing, 'v'), 'not inspectable')

    def odd(args):
        return 0, 'v /etc/passwd\n', ''

    expect_refused(lambda: volume_mount_identity(odd, 'v'), 'identity unusable')


def t_no_warmup_scope(tmp):
    assert_no_warmup_scope(['bridge', 'host'])
    expect_refused(lambda: assert_no_warmup_scope(
        ['bridge', 'acowtancy-dev-wt-ns-x_default']), 'already active')


def t_no_scoped_containers(tmp):
    def clean(args):
        return 0, '', ''

    assert assert_no_scoped_containers(clean, 'acowtancy-dev-wt-x') == []

    def leftover(args):
        return 0, 'c' * 64 + '\n', ''

    expect_refused(lambda: assert_no_scoped_containers(leftover, 'p'), 'remain')


def t_judge_teardown(tmp):
    base = fake_inventory(containers=['a' * 64], volumes=['keep-vol'],
                          networks={'bridge': 'x1', 'host': '', 'none': ''})
    scoped = 'acowtancy-dev-wt-ns-x'
    live_ok = fake_inventory(containers=['a' * 64],
                             volumes=['keep-vol', scoped + '-postgres-data',
                                      scoped + '-workspace-data'],
                             networks={'bridge': 'x1', 'host': '', 'none': ''})
    protected = {common.SHARED_MARIADB_VOLUME: '/var/lib/nerdctl/volumes/9'}
    preserved = judge_teardown_inventory(live_ok, base, scoped, protected)
    assert preserved == sorted([scoped + '-postgres-data', scoped + '-workspace-data'])
    # Foreign new volume refuses.
    live_foreign = fake_inventory(containers=['a' * 64], volumes=['keep-vol', 'other-vol'],
                                  networks=base['networks'])
    expect_refused(lambda: judge_teardown_inventory(live_foreign, base, scoped, protected),
                   'not guessing')
    # Baseline container disappearance refuses.
    live_lost = fake_inventory(containers=[], volumes=['keep-vol'], networks=base['networks'])
    expect_refused(lambda: judge_teardown_inventory(live_lost, base, scoped, protected),
                   'removed or replaced')
    # Lingering scoped network refuses.
    live_net = fake_inventory(containers=['a' * 64], volumes=['keep-vol'],
                              networks={'bridge': 'x1', 'host': '', 'none': '',
                                        scoped + '_default': 'n1'})
    expect_refused(lambda: judge_teardown_inventory(live_net, base, scoped, protected),
                   'still present')
    # Baseline volume removal refuses.
    live_vol = fake_inventory(containers=['a' * 64], volumes=[], networks=base['networks'])
    expect_refused(lambda: judge_teardown_inventory(live_vol, base, scoped, protected),
                   'volume identity removed')
    # Protected identity drift refuses through the mount_of callback.
    expect_refused(lambda: judge_teardown_inventory(
        live_ok, base, scoped, protected,
        mount_of=lambda name: '/var/lib/nerdctl/volumes/CHANGED'),
        'protected volume identity changed')


def t_scoped_map(tmp):
    ws, pg = 'd' * 64, 'e' * 64
    calls = {'inspect': 0}

    def runner(args):
        if args[0] == 'ps':
            return 0, ws + '\n' + pg + '\n', ''
        calls['inspect'] += 1
        return 0, 'workspace\n' if calls['inspect'] == 1 else 'postgres\n', ''

    services = scoped_container_map(runner, 'acowtancy-dev-wt-ns-x')
    assert services == {'workspace': ws, 'postgres': pg}

    def dup(args):
        if args[0] == 'ps':
            return 0, ws + '\n' + pg + '\n', ''
        return 0, 'workspace\n', ''

    expect_refused(lambda: scoped_container_map(dup, 'p'), 'ambiguous')


def t_container_state(tmp):
    assert container_state(lambda a: (0, 'running\n', ''), 'f' * 64) == 'running'
    expect_refused(lambda: container_state(lambda a: (0, 'zombie\n', ''), 'f' * 64),
                   'closed vocabulary')
    expect_refused(lambda: container_state(lambda a: (1, '', 'x'), 'f' * 64), 'read failed')


def t_label_length_only(tmp):
    label = 'x' * 100
    assert label_length_only(lambda a: (0, label + '\n', ''), 'g' * 64,
                             common.MOUNTS_LABEL) == 100
    multi = 'é' * 10
    assert label_length_only(lambda a: (0, multi, ''), 'g' * 64,
                             common.MOUNTS_LABEL) == 20  # utf-8 bytes
    expect_refused(lambda: label_length_only(lambda a: (1, '', ''), 'g' * 64,
                                             common.MOUNTS_LABEL), 'read failed')


# --- session identity and records ------------------------------------------

def t_session_identity(tmp):
    shape = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    ident = session_identity(shape['path'])
    canonical = str(Path(shape['path']))
    want = 'wt-ns-13f69c1b-9a5e-455e-8004-af30ece9685f-' + \
        hashlib.sha256(canonical.encode()).hexdigest()[:8]
    assert ident['session_id'] == want
    assert ident['project'] == 'acowtancy-dev-' + want


def t_attach_record(tmp):
    shape = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    ident = session_identity(shape['path'])
    good = json.dumps({'session_id': ident['session_id'], 'root_repo': shape['path'],
                       'repos': [shape['path']], 'active': True})
    p = tmpfile(tmp, 'record.json', good)
    assert validate_attach_record(p, ident, shape['path'])['active'] is True
    bad = json.dumps({'session_id': 'wt-other', 'root_repo': shape['path'],
                      'repos': [shape['path']], 'active': True})
    p2 = tmpfile(tmp, 'record2.json', bad)
    expect_refused(lambda: validate_attach_record(p2, ident, shape['path']), 'session id')
    p3 = tmpfile(tmp, 'record3.json', '{nope')
    expect_refused(lambda: validate_attach_record(p3, ident, shape['path']), 'JSON')
    expect_refused(lambda: validate_attach_record(tmp / 'missing.json', ident,
                                                  shape['path']), 'missing')


def t_owner_marker(tmp):
    wt = tmp / 'wt'
    wt.mkdir(mode=0o700)
    payload = {'task': 'effigy#540', 'route': 'warmup', 'k': 'v'}
    write_owner_marker(wt, payload)
    assert read_owner_marker(wt, payload) == payload
    expect_refused(lambda: read_owner_marker(wt, {'task': 'other'}), 'mismatch')
    wt2 = tmp / 'wt2'
    wt2.mkdir(mode=0o700)
    expect_refused(lambda: read_owner_marker(wt2, payload), 'missing')


# --- authorized command surface --------------------------------------------

def t_authorized_commands(tmp):
    assert ('container', 'up', '--detach') in warmup.AUTHORIZED_COMMANDS
    for cmd in warmup.AUTHORIZED_COMMANDS:
        assert '--system' not in cmd, '--system must never be authorized: %r' % (cmd,)
    assert ('container', 'up', '--system') not in warmup.AUTHORIZED_COMMANDS
    assert ('worktree:container:attach',) in warmup.AUTHORIZED_COMMANDS
    assert ('bundle', 'sync') in warmup.AUTHORIZED_COMMANDS
    assert ('container', 'down') in warmup.AUTHORIZED_COMMANDS
    assert warmup.PREFLIGHT_INSPECT[:2] == ('bundle', 'inspect')
    assert '--repo' in warmup.PREFLIGHT_INSPECT
    assert '--json' in warmup.PREFLIGHT_INSPECT


def t_runner_refuses_outside_allowlist(tmp):
    class Log:
        def write(self, data):
            pass

        def flush(self):
            pass

    run = warmup.make_effigy_runner(tmp, Log(), '/nonexistent-effigy')
    expect_refused(lambda: run(['container', 'up', '--system']), 'authorized sequence')
    expect_refused(lambda: run(['exec', 'bash']), 'authorized sequence')
    # The allowlist refusal happens before any process spawn: a nonexistent
    # effigy path never gets executed for a refused command.


# --- primary route observation guards --------------------------------------

def t_observation_plan(tmp):
    plans = primary.observation_plan('1234' * 16)
    assert set(plans) == set(EXPECTED_PRIMARY_COUNTS)
    for table, args in plans.items():
        assert args[0] == 'exec' and args[1] == '1234' * 16 and args[2] == 'psql'
        assert tuple(args[3:11]) == primary.PSQL_ALLOWLIST
        assert args[11] == 'SELECT count(*) FROM %s' % table
        assert len(args) == 12
        assert 'password' not in ' '.join(args).lower()


def t_observe_counts_and_console(tmp):
    from types import SimpleNamespace
    original_run = primary.run_quiet
    original_writer = common.receipt_writer
    calls = []
    def fake_run(args, **kwargs):
        calls.append(args)
        return SimpleNamespace(returncode=0, stderr='', stdout='5\n')
    def fake_writer(name):
        path = tmp / name
        return path, os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    primary.run_quiet = fake_run
    common.receipt_writer = fake_writer
    try:
        log = common.ConsoleLog('console')
        try:
            observed = primary.observe_counts(None, 'a' * 64, log)
            log.flush()
        finally:
            log.close()
        assert observed == {table: 5 for table in EXPECTED_PRIMARY_COUNTS}
        assert len(calls) == len(EXPECTED_PRIMARY_COUNTS)
        assert '[exit 0]' in (tmp / 'console').read_text()
    finally:
        primary.run_quiet = original_run
        common.receipt_writer = original_writer


def t_compare_counts(tmp):
    observed = {t: EXPECTED_PRIMARY_COUNTS[t][0] for t in EXPECTED_PRIMARY_COUNTS}
    assert compare_counts(observed) == {}
    observed['learning.pathway'] = 6
    mm = compare_counts(observed)
    assert mm == {'learning.pathway': {'expected': 5, 'observed': 6}}


def t_pick_postgres(tmp):
    pg = '9' * 64
    other = '8' * 64

    def runner(args):
        if args[0] == 'ps':
            return 0, pg + '\n' + other + '\n', ''
        return 0, 'postgres\n' if args[-1] == pg else 'dbgate\n', ''

    assert primary.pick_primary_postgres(runner) == pg

    def none_runner(args):
        if args[0] == 'ps':
            return 0, other + '\n', ''
        return 0, 'dbgate\n', ''

    expect_refused(lambda: primary.pick_primary_postgres(none_runner), 'not up')

    def two_runner(args):
        if args[0] == 'ps':
            return 0, pg + '\n' + other + '\n', ''
        return 0, 'postgres\n', ''

    expect_refused(lambda: primary.pick_primary_postgres(two_runner), 'ambiguous')


def t_dir_guard(tmp):
    assert_packet_dir(Path(common.DIR))
    expect_refused(lambda: assert_packet_dir(tmp), 'packet directory')


# --- bundle lock ------------------------------------------------------------

def t_bundle_lock(tmp):
    def ok():
        return {'source': {'source_type': 'git',
                           'source_path': common.BUNDLE_SOURCE,
                           'version_hint': BUNDLE_LOCK_COMMIT, 'stale': True}}

    rec = verify_bundle_lock(ok)
    assert rec['lock'] == BUNDLE_LOCK_COMMIT

    def moved():
        return {'source': {'source_type': 'git', 'source_path': common.BUNDLE_SOURCE,
                           'version_hint': 'f' * 40, 'stale': True}}

    expect_refused(lambda: verify_bundle_lock(moved), 're-pin')

    def other_url():
        return {'source': {'source_type': 'git', 'source_path': 'git@example.com:x.git',
                           'version_hint': BUNDLE_LOCK_COMMIT, 'stale': False}}

    expect_refused(lambda: verify_bundle_lock(other_url), 'source identity')

    def no_stale():
        return {'source': {'source_type': 'git', 'source_path': common.BUNDLE_SOURCE,
                           'version_hint': BUNDLE_LOCK_COMMIT}}

    expect_refused(lambda: verify_bundle_lock(no_stale), 'stale flag')


# --- consumer pins (fake git backend) ---------------------------------------

def git_fake(responses):
    def git_fn(args):
        key = ' '.join(args)
        if key not in responses:
            raise AssertionError('unexpected git call: ' + key)
        rc, out = responses[key]
        return rc, out, ''
    return git_fn


def pins_responses(tree=None, blobs=None, status='', ancestor=0):
    tree = tree or common.ACOW_PIN_TREE
    responses = {
        'rev-parse ' + common.ACOW_PIN_COMMIT + '^{' + 'commit}':
            (0, common.ACOW_PIN_COMMIT + '\n'),
        'rev-parse ' + common.ACOW_PIN_COMMIT + '^{' + 'tree}': (0, tree + '\n'),
        'status --porcelain=v1': (0, status),
        'merge-base --is-ancestor %s HEAD' % common.ACOW_PIN_COMMIT: (ancestor, ''),
    }
    for rel, blob in (blobs or common.PINNED_INPUT_BLOBS).items():
        responses['rev-parse %s:%s' % (common.ACOW_PIN_COMMIT, rel)] = (0, blob + '\n')
    return responses


def t_pins_valid(tmp):
    rec = verify_consumer_pins(git_fake(pins_responses()),
                               primary_path=tmp / 'fake-primary')
    assert rec['pin_tree'] == common.ACOW_PIN_TREE


def t_pins_tree_drift(tmp):
    r = pins_responses(tree='e' * 40)
    expect_refused(lambda: verify_consumer_pins(git_fake(r), primary_path=tmp / 'p'),
                   'tree drifted')


def t_pins_dirty(tmp):
    r = pins_responses(status=' M apps/x\n')
    expect_refused(lambda: verify_consumer_pins(git_fake(r), primary_path=tmp / 'p'),
                   'dirty')


def t_pins_not_ancestor(tmp):
    r = pins_responses(ancestor=1)
    expect_refused(lambda: verify_consumer_pins(git_fake(r), primary_path=tmp / 'p'),
                   'no longer contains')


def t_pins_blob_drift(tmp):
    blobs = dict(common.PINNED_INPUT_BLOBS)
    first = sorted(blobs)[0]
    blobs[first] = 'd' * 40
    r = pins_responses(blobs=blobs)
    expect_refused(lambda: verify_consumer_pins(git_fake(r), primary_path=tmp / 'p'),
                   'blob drifted')


def t_pins_linked_worktree(tmp):
    fake_primary = tmp / 'linked'
    fake_primary.mkdir()
    (fake_primary / '.git').write_text('gitdir: /elsewhere\n')
    expect_refused(lambda: verify_consumer_pins(git_fake(pins_responses()),
                                                primary_path=fake_primary),
                   'linked worktree')


# --- owned-dir hygiene ------------------------------------------------------

def t_owned_dir(tmp):
    d = tmp / 'ok'
    d.mkdir(mode=0o700)
    assert ensure_owned_dir(d).is_dir()
    loose = tmp / 'loose'
    loose.mkdir(mode=0o755)
    expect_refused(lambda: ensure_owned_dir(loose), 'private')
    filey = tmpfile(tmp, 'afile', 'x')
    expect_refused(lambda: ensure_owned_dir(filey), 'real directory')
    link = tmp / 'linkdir'
    os.symlink(d, link)
    expect_refused(lambda: ensure_owned_dir(link), 'symlink')


def main():
    global PACKET_SHA
    PACKET_SHA = verify_packet_bundle()['packet_sha256']
    common.SCRATCH.mkdir(parents=True, exist_ok=True, mode=0o700)
    root = common.boundaries_root()
    cases = [(name, fn) for name, fn in sorted(globals().items())
             if name.startswith('t_') and callable(fn)]
    try:
        for name, fn in cases:
            sub = root / name
            sub.mkdir(mode=0o700)
            check(name[len('t_'):], lambda fn=fn, s=sub: fn(s))
    finally:
        for path in sorted(root.rglob('*'), reverse=True):
            try:
                if path.is_symlink() or path.is_file():
                    path.unlink()
                elif path.is_dir():
                    path.rmdir()
            except OSError:
                pass
        try:
            root.rmdir()
        except OSError:
            pass
    print('\n%d passed, %d failed' % (len(PASS), len(FAIL)))
    if FAIL:
        for row in FAIL:
            print('FAILED: ' + row)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())

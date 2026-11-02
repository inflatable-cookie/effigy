#!/usr/bin/env python3
"""Fake-only proof that the r18 ordinary-Compose Colima qualification launcher
enforces its complete entry sweep (task effigy#542).

No real Colima, docker, nerdctl, VM, network, volume or scratch qualification
state is touched: every native call goes through the single `common._spawn`
seam, which these tests replace with a deterministic bounded fake, and every
machine-specific constant (scratch BASE, recovery evidence directory, fixture
roots, candidate worktree, GO receipt) is repointed at a disposable temp
fixture. The tests prove: the exact matching baseline passes; drifted,
same-count-replaced, malformed, duplicate, truncated, unsupported and timed-out
inventories refuse; manifest/hash integrity and old/new GO mismatch refuse;
mount and symlink fixture-ancestor safety before creation and the shared-parse
refusals at deletion; launcher/prepare/runtime ordering before any create or
up; zero fixture writes and read-only backend calls; and the failing-first gap
— the pre-fix guards extracted from git history accept drifted identities and
mounted or symlinked fixture ancestors that the r18 sweep refuses before any
creation. Read-only checks against the real machine pins run only when the
pinned evidence is present on this host; they are skipped elsewhere.

Backend argv shapes used by the committed helpers: colima's own subcommands,
`colima nerdctl -p effigy -- <verb> ...` and `colima ssh -p effigy -- <guest...>`,
plus `/sbin/mount`, `/proc/self/mountinfo` over SSH, and the two git reads.
"""
import argparse
import contextlib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import types

REPO = Path(__file__).resolve().parents[2]
PACKET_DIR = REPO / 'docs/qualification/ordinary-compose-colima-5f466914'
FIXTURE_BASELINE = REPO / 'tests/fixtures/qualification-entry-sweep/baseline-identities.json'
HELPER_FILES = ('launch.py', 'common.py', 'prepare.py', 'preflight.py', 'runtime.py', 'source_estimate.py')
OLD_REVISION_PACKET_SHA = 'a9c83d12e693ba318fc45cb12a55b09da68eacdd27369c066eb1abe9baa0e7c9'
TEST_HEAD = 'a' * 40
FAKE_PACKET_SHA = 'b' * 64

sys.path.insert(0, str(PACKET_DIR))
import common  # noqa: E402  (the committed packet module under test)

TESTS = []
FAILURES = []


def test(fn):
    TESTS.append(fn)
    return fn


class Skipped(Exception):
    pass


# --------------------------------------------------------------- fake backend

class FakeCompleted:
    def __init__(self, returncode=0, stdout='', stderr=''):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


class FakeRunner:
    """Deterministic bounded host/guest runner: first matching rule wins."""

    def __init__(self, rules):
        self.rules = list(rules)
        self.calls = []

    def run(self, args, env, timeout):
        self.calls.append(list(args))
        for matcher, handler in self.rules:
            if not matcher(args):
                continue
            if callable(handler):
                handler = handler(args)
            if handler == 'timeout':
                raise subprocess.TimeoutExpired(tuple(args), timeout)
            return FakeCompleted(handler[0], handler[1]) if len(handler) == 2 else FakeCompleted(*handler)
        raise AssertionError('fake runner has no rule for: ' + ' '.join(args[:9]))


def mountinfo_text(extra_points):
    rows = ['36 35 0:34 / / rw,relatime master:1 - apfs /dev/disk1s1 rw',
            '42 1 0:42 / /Users/tom rw - virtiofs effigy-share rw']
    rows += [f'{90 + i} 42 0:{90 + i} / {point} rw - tmpfs tmpfs rw' for i, point in enumerate(extra_points)]
    return '\n'.join(rows) + '\n'


def host_mount_text(extra_points):
    rows = ['/dev/disk1s1s1 on / (apfs, local, journaled)']
    rows += [f'tmpfs on {point} (local, nodev)' for point in extra_points]
    return '\n'.join(rows) + '\n'


def nerd_rules(networks, volumes, containers):
    """Read-only backend rules serving the given exact identity maps."""
    container_ids = sorted(containers)
    is_colima = lambda a: a[:1] == ['/opt/homebrew/bin/colima']
    is_nerd = lambda a: is_colima(a) and a[1:5] == ['nerdctl', '-p', 'effigy', '--']
    is_ssh = lambda a: is_colima(a) and a[1:5] == ['ssh', '-p', 'effigy', '--']
    rules = [
        (lambda a: is_colima(a) and a[1:] == ['version'], (0, 'colima version 0.10.3\n')),
        (lambda a: is_colima(a) and a[1:4] == ['status', '-p', 'effigy'],
         (0, 'INFO[0000] effigy is running\nruntime: containerd\n', '')),
        (lambda a: is_nerd(a) and a[5:] == ['version'], (0, 'nerdctl version v2.3.1\n')),
        (lambda a: is_nerd(a) and a[5:7] == ['compose', 'version'], (0, 'Compose version v2.3.1\n')),
        (lambda a: is_nerd(a) and a[5:7] == ['image', 'inspect'],
         (0, 'sha256:6cf065f724d522cbb3e1898559cd88f015db8a1a4210d2d048a0853b69b57ba9 arm64\n')),
        (lambda a: is_ssh(a) and a[5] == 'test', (0, '')),
        (lambda a: is_nerd(a) and a[5:7] == ['ps', '-a'] and not any(x.startswith('--filter') for x in a),
         (0, '\n'.join(container_ids) + ('\n' if container_ids else ''))),
        (lambda a: is_nerd(a) and a[5:7] == ['ps', '-a'], (0, '')),
        (lambda a: is_nerd(a) and a[5:7] == ['network', 'ls'] and '\t' in a[-1],
         lambda a: (0, ''.join(name + '\t' + (ident[:12] if len(ident) == 64 and name not in ('host', 'none') else '') + '\n' for name, ident in sorted(networks.items())))),
        (lambda a: is_nerd(a) and a[5:7] == ['network', 'ls'],
         lambda a: (0, ''.join(name + '\n' for name in sorted(networks)))),
        (lambda a: is_nerd(a) and a[5:7] == ['network', 'inspect'],
         lambda a: (0, json.dumps([{'Name': n, 'Id': i} for n, i in sorted(networks.items()) if n not in ('host', 'none')]))),
        (lambda a: is_nerd(a) and a[5:7] == ['volume', 'ls'],
         (0, ''.join(name + '\n' for name in sorted(volumes)))),
        (lambda a: is_nerd(a) and a[5:7] == ['volume', 'inspect'],
         lambda a: (0, json.dumps([{'Name': n, 'Mountpoint': spec[0]} for n, spec in sorted(volumes.items())]))),
    ]
    stat_by_path = {spec[0]: spec[1] for spec in volumes.values()}

    def serve_stat(args):
        split = len(args) - args[::-1].index('--')
        return (0, ''.join(stat_by_path[path] + '\n' for path in args[split:]))

    rules.append(((lambda a: is_ssh(a) and a[5:8] == ['sudo', '-n', 'stat']), serve_stat))
    return rules


def mountinfo_rule(guest_mount_points):
    return ((lambda a: a[:1] == ['/opt/homebrew/bin/colima'] and a[1:5] == ['ssh', '-p', 'effigy', '--']
             and a[5:] == ['cat', '/proc/self/mountinfo']),
            (lambda a: (0, mountinfo_text(guest_mount_points))))


def read_only_rules(networks, volumes, containers, head, *, host_mount_points=(), guest_mount_points=()):
    rules = nerd_rules(networks, volumes, containers)
    rules.append(((lambda a: a[:1] == ['/sbin/mount']), (lambda a: (0, host_mount_text(host_mount_points)))))
    rules.append(mountinfo_rule(guest_mount_points))
    rules.append(((lambda a: a[:1] == ['/usr/bin/git'] and 'rev-parse' in a), (0, head + '\n')))
    rules.append(((lambda a: a[:1] == ['/usr/bin/git']), (0, '')))
    return rules


def assert_read_only_calls(runner):
    for args in runner.calls:
        if args[:1] == ['/sbin/mount']:
            continue
        if args[:1] == ['/usr/bin/git']:
            assert 'rev-parse' in args or 'status' in args, args
            continue
        assert args[:1] == ['/opt/homebrew/bin/colima'], args
        rest = args[1:]
        if rest[:4] == ['nerdctl', '-p', 'effigy', '--']:
            assert len(rest) > 4, args
            verb = rest[4]
            assert verb in ('version', 'ps', 'image', 'network', 'volume', 'compose'), args
            if verb == 'ps':
                assert rest[5] == '-a' and '--no-trunc' in rest, args
            if verb == 'compose':
                assert rest[5] == 'version', args
        elif rest[:4] == ['ssh', '-p', 'effigy', '--']:
            assert len(rest) > 4, args
            assert rest[4] in ('test', 'cat', 'sudo'), args
            if rest[4] == 'sudo':
                assert rest[5:7] == ['-n', 'stat'], args
        else:
            assert rest[0] in ('version', 'status'), args


# ----------------------------------------------------------------- utilities

NO_ENV = {'PATH': '/usr/bin:/bin', 'LANG': 'C'}


@contextlib.contextmanager
def patched(module, **overrides):
    saved = {name: getattr(module, name, None) for name in overrides}
    for name, value in overrides.items():
        setattr(module, name, value)
    try:
        yield
    finally:
        for name, value in saved.items():
            setattr(module, name, value)


def install_common_fakes(rules):
    runner = FakeRunner(rules)

    def fake_spawn(args, *, env=None, timeout=30):
        return runner.run(args, env, timeout)

    return runner, patched(common, _spawn=fake_spawn, scrubbed_env=lambda with_home=False: dict(NO_ENV))


def write_go(base, packet_sha, *, mode=0o600, text=None):
    path = Path(base) / 'r18-go.txt'
    body = text if text is not None else f'packet {packet_sha}\nSupervisor GO for the r18 ordinary-Compose Colima packet.\n'
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, 'w') as out:
        out.write(body)
    os.chmod(path, mode)
    return path


def real_packet_sha():
    return hashlib.sha256((common.DIR / 'colima-qualification-5f466914-r18.md').read_bytes()).hexdigest()


def load_baseline_maps(key=None):
    maps = json.loads(FIXTURE_BASELINE.read_text())
    return maps if key is None else maps[key]


def expect_refusal(fn, *args, **kwargs):
    try:
        fn(*args, **kwargs)
    except Exception:
        return
    raise AssertionError('expected refusal, but the check passed')


def no_prior_evidence(base):
    return not any((Path(base) / name).exists() for name in common.PRIOR_EVIDENCE)


def tree_state(root):
    state = {}
    for base, dirs, files in os.walk(root):
        for name in dirs + files:
            path = Path(base) / name
            try:
                st = os.lstat(path)
                state[str(path)] = (st.st_mode, st.st_size, st.st_mtime_ns)
            except OSError:
                pass
    return state


def temp_bundle(tmp, networks, volumes, containers):
    """Self-consistent packet/manifest/helper/artifact fixture mirroring the
    committed r18 structure with machine-specific pins repointed."""
    src = common.DIR
    bundle = tmp / 'packet-dir'
    scratch = tmp / 'scratch'
    (bundle / 'candidate-src-5f466914/target/debug').mkdir(parents=True)
    scratch.mkdir()
    for name in HELPER_FILES:
        (bundle / name).write_bytes((src / name).read_bytes())
    binary_dir = scratch / 'candidate-src-5f466914/target/debug'
    binary_dir.mkdir(parents=True)
    binary = binary_dir / 'effigy'
    binary.write_bytes(b'fake candidate binary\n')
    (scratch / 'r3-baseline-container-ids.txt').write_text('\n'.join(sorted(containers)) + '\n')
    manifest = {name: common.digest(bundle / name) for name in HELPER_FILES}
    manifest['candidate-src-5f466914/target/debug/effigy'] = common.digest(binary)
    manifest['r3-baseline-container-ids.txt'] = common.digest(scratch / 'r3-baseline-container-ids.txt')
    manifest_path = bundle / 'r18-artifacts.json'
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
    packet_text = re.sub(r'Artifacts manifest SHA-256: `[0-9a-f]{64}`',
                         'Artifacts manifest SHA-256: `' + common.digest(manifest_path) + '`',
                         (src / 'colima-qualification-5f466914-r18.md').read_text())
    packet_path = bundle / 'colima-qualification-5f466914-r18.md'
    packet_path.write_text(packet_text)
    return {'dir': bundle, 'base': scratch, 'packet': packet_path,
            'packet_sha': common.digest(packet_path), 'manifest': manifest_path,
            'go': write_go(scratch, common.digest(packet_path)), 'binary': binary,
            'worktree': scratch / 'candidate-src-5f466914'}


def patch_bundle_constants(bundle):
    return patched(common, PACKET=bundle['packet'], DIR=bundle['dir'], BASE=bundle['base'],
                   WORKTREE=bundle['worktree'], HEAD=TEST_HEAD)


RECOVERY_SEQ = [0]


def temp_recovery(tmp, networks, volumes, *, counts=None, mutate=None):
    RECOVERY_SEQ[0] += 1
    recovery = tmp / f'recovery-{RECOVERY_SEQ[0]}'
    recovery.mkdir()
    inv = {'networks': json.loads(json.dumps(networks)), 'volumes': json.loads(json.dumps(volumes))}
    if mutate:
        mutate(inv)
    inventory = recovery / 'post-start-readonly-inventory.json'
    inventory.write_text(json.dumps(inv, indent=2))
    receipt = {
        'network_digest': common.canonical_digest(inv['networks']),
        'volume_digest': common.canonical_digest(inv['volumes']),
        'inventory_counts': counts or {'containers': 15, 'networks': len(inv['networks']), 'volumes': len(inv['volumes'])},
        'historical_identity_digest_equality': {'network': True, 'volume': True},
        'artifact_sha256': {'post-start-readonly-inventory.json': common.digest(inventory)},
    }
    receipt_path = recovery / 'recovery-receipt.json'
    receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
    return recovery, common.digest(receipt_path), common.digest(inventory)


def recovery_patches(recovery_dir, receipt_sha, inventory_sha):
    return patched(common, RECOVERY_DIR=recovery_dir, RECOVERY_RECEIPT_SHA256=receipt_sha,
                   RECOVERY_INVENTORY_SHA256=inventory_sha)


def container_ids_or_skip():
    ids = load_baseline_maps('containers')
    if len(ids) != 15:
        raise Skipped('container baseline fixture incomplete')
    return ids


# --------------------------------------------------------------------- tests

@test
def test_packet_pins_match_recovered_evidence(tmp):
    networks, volumes = load_baseline_maps('networks'), load_baseline_maps('volumes')
    assert len(networks) == 36 and len(volumes) == 255
    assert common.canonical_digest(networks) == common.NETWORK_BASELINE_SHA256
    assert common.canonical_digest(volumes) == common.VOLUME_BASELINE_SHA256
    assert common.EXPECTED_NETWORK_COUNT == 36 and common.EXPECTED_VOLUME_COUNT == 255
    for name, ident in common.PROTECTED_VOLUMES.items():
        assert volumes[name] == ident, name
    packet_text = (common.DIR / 'colima-qualification-5f466914-r18.md').read_text()
    for pin in (common.NETWORK_BASELINE_SHA256, common.VOLUME_BASELINE_SHA256,
                common.RECOVERY_RECEIPT_SHA256, common.RECOVERY_INVENTORY_SHA256,
                common.MARIADB_CONTAINER_ID):
        assert pin in packet_text, pin


@test
def test_committed_manifest_matches_helpers_and_packet(tmp):
    manifest = json.loads((common.DIR / 'r18-artifacts.json').read_text())
    assert set(manifest) == set(HELPER_FILES) | {'candidate-src-5f466914/target/debug/effigy', 'r3-baseline-container-ids.txt'}
    for name in HELPER_FILES:
        assert manifest[name] == common.digest(common.DIR / name), name
    packet_text = (common.DIR / 'colima-qualification-5f466914-r18.md').read_text()
    pinned = re.search(r'Artifacts manifest SHA-256: `([0-9a-f]{64})`', packet_text)
    assert pinned and pinned.group(1) == common.digest(common.DIR / 'r18-artifacts.json')
    assert common.digest(common.DIR / 'colima-qualification-5f466914-r18.md') == real_packet_sha()


@test
def test_bundle_positive_and_refusals(tmp):
    networks, volumes = load_baseline_maps('networks'), load_baseline_maps('volumes')
    bundle = temp_bundle(tmp, networks, volumes, ['c' * 64])
    runner, patches = install_common_fakes(read_only_rules(networks, volumes, [], TEST_HEAD))
    with patches, patch_bundle_constants(bundle):
        bundle_info = common.verify_bundle(bundle['packet_sha'])
        assert bundle_info['source_head'] == TEST_HEAD
        assert bundle_info['artifact_manifest_sha256'] == common.digest(bundle['manifest'])
        expect_refusal(common.verify_bundle, 'd' * 64)  # wrong packet sha
        saved = bundle['manifest'].read_bytes()
        bundle['manifest'].write_bytes(saved.replace(b'{\n', b' {\n', 1))
        expect_refusal(common.verify_bundle, bundle['packet_sha'])  # tampered manifest
        bundle['manifest'].write_bytes(saved)
        helper = bundle['dir'] / 'preflight.py'
        saved = helper.read_bytes()
        helper.write_bytes(saved + b'# tampered\n')
        expect_refusal(common.verify_bundle, bundle['packet_sha'])  # tampered helper
        helper.unlink()
        helper.symlink_to(bundle['dir'] / 'common.py')
        expect_refusal(common.verify_bundle, bundle['packet_sha'])  # symlinked helper
        helper.unlink()
        helper.write_bytes(saved)
        artifact = bundle['base'] / 'r3-baseline-container-ids.txt'
        saved = artifact.read_bytes()
        artifact.unlink()
        expect_refusal(common.verify_bundle, bundle['packet_sha'])  # missing artifact
        artifact.write_bytes(saved)
        runner.rules[-2] = ((lambda a: a[:1] == ['/usr/bin/git'] and 'rev-parse' in a), (0, 'f' * 40 + '\n'))
        expect_refusal(common.verify_bundle, bundle['packet_sha'])  # source head drift
        assert no_prior_evidence(bundle['base'])
    assert_read_only_calls(runner)


@test
def test_go_receipt_accepts_only_exact_new_packet(tmp):
    base = tmp / 'scratch'
    base.mkdir()
    with patched(common, BASE=base):
        go = write_go(base, FAKE_PACKET_SHA)
        assert re.fullmatch(r'[0-9a-f]{64}', common.verify_go(go, FAKE_PACKET_SHA))
        expect_refusal(common.verify_go, go, OLD_REVISION_PACKET_SHA)  # old r17 GO hash
        expect_refusal(common.verify_go, base / 'absent-go.txt', FAKE_PACKET_SHA)  # missing GO
        expect_refusal(common.verify_go, tmp / 'elsewhere-go.txt', FAKE_PACKET_SHA)  # wrong exact path
        wrong = write_go(base, FAKE_PACKET_SHA, text='packet ' + 'e' * 64 + '\nSupervisor GO\n')
        expect_refusal(common.verify_go, wrong, FAKE_PACKET_SHA)
        expect_refusal(common.verify_go, write_go(base, FAKE_PACKET_SHA, mode=0o644), FAKE_PACKET_SHA)
        linked = base / 'linked-go.txt'
        linked.symlink_to(go)
        expect_refusal(common.verify_go, linked, FAKE_PACKET_SHA)
        hard = base / 'hard-go.txt'
        os.link(go, hard)
        expect_refusal(common.verify_go, hard, FAKE_PACKET_SHA)
        tokenless = write_go(base, FAKE_PACKET_SHA, text='packet ' + FAKE_PACKET_SHA + '\nplain approval\n')
        expect_refusal(common.verify_go, tokenless, FAKE_PACKET_SHA)
        huge = write_go(base, FAKE_PACKET_SHA, text='packet ' + FAKE_PACKET_SHA + ' Supervisor ' + 'x' * 9000 + '\n')
        expect_refusal(common.verify_go, huge, FAKE_PACKET_SHA)


@test
def test_historical_baseline_reconstruction(tmp):
    networks, volumes = load_baseline_maps('networks'), load_baseline_maps('volumes')
    recovery, receipt_sha, inventory_sha = temp_recovery(tmp, networks, volumes)
    with recovery_patches(recovery, receipt_sha, inventory_sha):
        result = common.verify_historical_baseline()
        assert result['network_identity_sha256'] == common.NETWORK_BASELINE_SHA256
        assert result['volume_identity_sha256'] == common.VOLUME_BASELINE_SHA256
        assert result['network_count'] == 36 and result['volume_count'] == 255
        with patched(common, RECOVERY_RECEIPT_SHA256='0' * 64):
            expect_refusal(common.verify_historical_baseline)  # receipt hash mismatch
    broken = tmp / 'recovery-broken'
    broken.mkdir()
    (broken / 'recovery-receipt.json').write_text('{}\n')
    (broken / 'post-start-readonly-inventory.json').write_text('{"networks": {"host": "bui')
    with recovery_patches(broken, common.digest(broken / 'recovery-receipt.json'),
                          common.digest(broken / 'post-start-readonly-inventory.json')):
        expect_refusal(common.verify_historical_baseline)  # truncated inventory

    def bump_inode(inv):
        name = sorted(inv['volumes'])[0]
        path, value = inv['volumes'][name]
        inv['volumes'][name] = [path, re.sub(r'^([0-9]+):([0-9]+)', r'\1:9999999', value)]

    mutated, mr, mi = temp_recovery(tmp, networks, volumes, mutate=bump_inode)
    with recovery_patches(mutated, mr, mi):
        expect_refusal(common.verify_historical_baseline)  # same-count replacement

    def drop(inv):
        inv['volumes'].pop(sorted(inv['volumes'])[0])

    short, sr, si = temp_recovery(tmp, networks, volumes, mutate=drop,
                                  counts={'containers': 15, 'networks': 36, 'volumes': 254})
    with recovery_patches(short, sr, si):
        expect_refusal(common.verify_historical_baseline)  # count mismatch
    noprotect, nr, ni = temp_recovery(tmp, networks, volumes, mutate=drop,
                                      counts={'containers': 15, 'networks': 36, 'volumes': 254})
    with recovery_patches(noprotect, nr, ni):
        expect_refusal(common.verify_historical_baseline)  # missing protected identity


def resource_sweep_case(base, rules):
    runner, patches = install_common_fakes(rules)
    with patches, patched(common, BASE=base):
        result = common.verify_resource_baseline()
    return runner, result


@test
def test_resource_baseline_positive_and_refusals(tmp):
    networks, volumes = load_baseline_maps('networks'), load_baseline_maps('volumes')
    containers = container_ids_or_skip()
    base = tmp / 'scratch'
    base.mkdir()
    (base / 'r3-baseline-container-ids.txt').write_text('\n'.join(sorted(containers)) + '\n')

    def mutate_networks(fn):
        changed = json.loads(json.dumps(networks))
        fn(changed)
        return changed

    def mutate_volumes(fn):
        changed = json.loads(json.dumps(volumes))
        fn(changed)
        return changed

    runner, result = resource_sweep_case(base, read_only_rules(networks, volumes, containers, TEST_HEAD))
    assert result['network_identity_sha256'] == common.NETWORK_BASELINE_SHA256
    assert result['volume_identity_sha256'] == common.VOLUME_BASELINE_SHA256
    assert result['network_count'] == 36 and result['volume_count'] == 255
    assert_read_only_calls(runner)

    # same-count network identity replacement refuses (name swap and ID swap)
    name_swap = mutate_networks(lambda m: m.__setitem__(
        'impersonated-net', m.pop(sorted(k for k in m if k not in ('host', 'none'))[0])))

    def id_swap(m):
        name = sorted(k for k in m if k not in ('host', 'none'))[0]
        m[name] = ('f' * 12) + m[name][12:]

    for changed in (name_swap, mutate_networks(id_swap)):
        expect_refusal(resource_sweep_case, base, read_only_rules(changed, volumes, containers, TEST_HEAD))

    # same-count volume identity replacement refuses (stat and backing path)
    def bump_inode(v):
        name = sorted(v)[0]
        path, value = v[name]
        v[name] = [path, re.sub(r'^([0-9]+):([0-9]+)', r'\1:9999999', value)]

    def rebind(v):
        name = sorted(v)[0]
        path, value = v[name]
        v[name] = [path.replace('/_data', '/_data2'), value]

    for changed in (mutate_volumes(bump_inode), mutate_volumes(rebind)):
        expect_refusal(resource_sweep_case, base, read_only_rules(networks, changed, containers, TEST_HEAD))
    # count drift refuses
    expect_refusal(resource_sweep_case, base, read_only_rules(
        mutate_networks(lambda m: m.pop(sorted(k for k in m if k not in ('host', 'none'))[0])),
        volumes, containers, TEST_HEAD))
    expect_refusal(resource_sweep_case, base, read_only_rules(
        networks, mutate_volumes(lambda v: v.pop(sorted(v)[0])), containers, TEST_HEAD))
    # malformed network list row refuses
    malformed = read_only_rules(networks, volumes, containers, TEST_HEAD)
    malformed.insert(0, ((lambda a: a[5:7] == ['network', 'ls']),
                         (0, 'host\t\nnone\t\nbad!!name\tabc123def456\n')))
    expect_refusal(resource_sweep_case, base, malformed)
    # duplicate network list row refuses
    duplicate = read_only_rules(networks, volumes, containers, TEST_HEAD)
    duplicate.insert(0, ((lambda a: a[5:7] == ['network', 'ls']),
                         (0, 'host\t\nnone\t\nhost\t\n' + ''.join(
                             n + '\t' + networks[n] + '\n' for n in sorted(networks) if n not in ('host', 'none')))))
    expect_refusal(resource_sweep_case, base, duplicate)
    # truncated network inspect refuses
    truncated = read_only_rules(networks, volumes, containers, TEST_HEAD)
    truncated.insert(0, ((lambda a: a[5:7] == ['network', 'inspect']),
                         lambda a: (0, json.dumps([{'Name': n, 'Id': networks[n]} for n in sorted(networks) if n not in ('host', 'none')][:-1]))))
    expect_refusal(resource_sweep_case, base, truncated)
    # duplicate and truncated volume inspect refuse
    dup_volume = read_only_rules(networks, volumes, containers, TEST_HEAD)
    dup_volume.insert(0, ((lambda a: a[5:7] == ['volume', 'inspect']),
                          lambda a: (0, json.dumps([{'Name': n, 'Mountpoint': volumes[n][0]} for n in sorted(volumes)]
                                                   + [{'Name': sorted(volumes)[0], 'Mountpoint': volumes[sorted(volumes)[0]][0]}]))))
    expect_refusal(resource_sweep_case, base, dup_volume)
    short_volume = read_only_rules(networks, volumes, containers, TEST_HEAD)
    short_volume.insert(0, ((lambda a: a[5:7] == ['volume', 'inspect']),
                            lambda a: (0, json.dumps([{'Name': n, 'Mountpoint': volumes[n][0]} for n in sorted(volumes)][:-1]))))
    expect_refusal(resource_sweep_case, base, short_volume)
    # unsupported stat output refuses
    bad_stat = read_only_rules(networks, volumes, containers, TEST_HEAD)
    bad_stat.insert(0, ((lambda a: a[5:8] == ['sudo', '-n', 'stat']), (0, 'not-a-stat-line\n')))
    expect_refusal(resource_sweep_case, base, bad_stat)
    # truncated stat output refuses
    short_stat = read_only_rules(networks, volumes, containers, TEST_HEAD)
    short_stat.insert(0, ((lambda a: a[5:8] == ['sudo', '-n', 'stat']), (0, '')))
    expect_refusal(resource_sweep_case, base, short_stat)
    # subprocess timeout refuses
    timing_out = read_only_rules(networks, volumes, containers, TEST_HEAD)
    timing_out.insert(0, ((lambda a: a[5:7] == ['network', 'inspect']), 'timeout'))
    expect_refusal(resource_sweep_case, base, timing_out)
    # failing backend command refuses
    failing = read_only_rules(networks, volumes, containers, TEST_HEAD)
    failing.insert(0, ((lambda a: a[5:7] == ['volume', 'ls']), (7, '', 'boom')))
    expect_refusal(resource_sweep_case, base, failing)
    # protected MariaDB container missing from the baseline file refuses
    (base / 'r3-baseline-container-ids.txt').write_text(
        '\n'.join(cid for cid in sorted(containers) if cid != common.MARIADB_CONTAINER_ID) + '\n')
    expect_refusal(resource_sweep_case, base, read_only_rules(networks, volumes, containers, TEST_HEAD))


def ancestry_case(tmp, rules, parent=None, roots=None):
    parent = parent or tmp / 'fixture-parent'
    roots = roots or [parent / 'root-a', parent / 'root-b']
    runner, patches = install_common_fakes(rules)
    with patches, patched(common, BASE=tmp / 'scratch', FIXTURE_PARENT=parent, FIXTURE_ROOTS=list(roots)):
        result = common.verify_fixture_ancestry()
    return runner, result


@test
def test_fixture_ancestry_before_creation(tmp):
    networks, volumes = load_baseline_maps('networks'), load_baseline_maps('volumes')
    containers = container_ids_or_skip()
    (tmp / 'scratch').mkdir()
    parent = tmp / 'fixture-parent'
    roots = [parent / 'root-a', parent / 'root-b']
    # acceptable: shared mounts strictly above the fixture parent
    rules = read_only_rules(networks, volumes, containers, TEST_HEAD, guest_mount_points=('/Users/tom', '/Users'))
    runner, result = ancestry_case(tmp, rules, parent, roots)
    assert result == {'fixture_ancestors_safe': True, 'mount_tables_parsed': True}
    assert_read_only_calls(runner)
    # a mount at the fixture parent refuses before any path exists
    expect_refusal(ancestry_case, tmp, read_only_rules(
        networks, volumes, containers, TEST_HEAD, guest_mount_points=(str(parent),)), parent, roots)
    # a mount at or inside an exact fixture root refuses
    expect_refusal(ancestry_case, tmp, read_only_rules(
        networks, volumes, containers, TEST_HEAD, host_mount_points=(str(roots[0]) + '/deep',)), parent, roots)
    expect_refusal(ancestry_case, tmp, read_only_rules(
        networks, volumes, containers, TEST_HEAD, guest_mount_points=(str(roots[1]),)), parent, roots)
    # symlink in the ancestry refuses
    (tmp / 'linked-target' / 'parent').mkdir(parents=True)
    link = tmp / 'uplink'
    link.symlink_to(tmp / 'linked-target', target_is_directory=True)
    linked_parent = link / 'parent'
    expect_refusal(ancestry_case, tmp, read_only_rules(networks, volumes, containers, TEST_HEAD),
                   linked_parent, [linked_parent / 'r'])
    # symlinked fixture parent refuses
    parent.symlink_to(tmp / 'elsewhere', target_is_directory=True)
    expect_refusal(ancestry_case, tmp, read_only_rules(networks, volumes, containers, TEST_HEAD), parent, roots)
    parent.unlink()
    parent.mkdir()
    # non-directory ancestor refuses
    (tmp / 'blocker').write_text('not a directory\n')
    expect_refusal(ancestry_case, tmp, read_only_rules(networks, volumes, containers, TEST_HEAD),
                   tmp / 'blocker' / 'sub', [tmp / 'blocker' / 'sub' / 'r'])
    # foreign-owned ancestor refuses
    proxy = types.SimpleNamespace(**{attr: getattr(os, attr) for attr in dir(os) if not attr.startswith('_')})
    proxy.getuid = lambda: os.getuid() + 424242
    runner, patches = install_common_fakes(read_only_rules(networks, volumes, containers, TEST_HEAD))
    with patches, patched(common, BASE=tmp / 'scratch', FIXTURE_PARENT=parent, FIXTURE_ROOTS=roots, os=proxy):
        expect_refusal(common.verify_fixture_ancestry)
    # ambiguous mount tables and escaped mountpoints refuse
    ambiguous_host = read_only_rules(networks, volumes, containers, TEST_HEAD)
    ambiguous_host.insert(0, ((lambda a: a[:1] == ['/sbin/mount']), (0, 'this line is not a mount row\n')))
    expect_refusal(ancestry_case, tmp, ambiguous_host, parent, roots)
    ambiguous_guest = read_only_rules(networks, volumes, containers, TEST_HEAD)
    ambiguous_guest.insert(0, ((lambda a: a[:1] == ['/opt/homebrew/bin/colima']), (0, 'short row\n')))
    expect_refusal(ancestry_case, tmp, ambiguous_guest, parent, roots)
    # a residual-backslash mountpoint (not octal-escaped) refuses
    escaped = read_only_rules(networks, volumes, containers, TEST_HEAD)
    escaped.insert(0, ((lambda a: a[:1] == ['/opt/homebrew/bin/colima']),
                       (0, '42 1 0:42 / /Users/\\tom rw - virtiofs effigy rw\n')))
    expect_refusal(ancestry_case, tmp, escaped, parent, roots)


@test
def test_roots_and_prior_evidence_absence(tmp):
    base = tmp / 'scratch'
    base.mkdir()
    with patched(common, BASE=base, FIXTURE_PARENT=tmp / 'parent',
                 FIXTURE_ROOTS=[tmp / 'parent/root-a', tmp / 'parent/root-b']):
        assert common.verify_fixture_roots_absent() == {'fixture_roots_absent': True}
        assert common.verify_prior_artifacts_absent() == {'prior_artifacts_absent': True}
        (tmp / 'parent').mkdir()
        (tmp / 'parent/root-a').mkdir()
        expect_refusal(common.verify_fixture_roots_absent)
        (tmp / 'parent/root-a').rmdir()
        (base / 'r18-preparation-receipt.json').write_text('{}\n')
        expect_refusal(common.verify_prior_artifacts_absent)
        (base / 'r18-preparation-receipt.json').unlink()
        assert common.verify_prior_artifacts_absent() == {'prior_artifacts_absent': True}


def full_sweep_env(tmp, networks, volumes, containers, *, backend_volumes=None):
    bundle = temp_bundle(tmp, networks, volumes, containers)
    recovery, receipt_sha, inventory_sha = temp_recovery(tmp, networks, volumes)
    parent = tmp / 'fixture-parent'
    roots = [parent / 'root-a', parent / 'root-b']
    runner, patches = install_common_fakes(
        read_only_rules(networks, backend_volumes or volumes, containers, TEST_HEAD))
    stack = contextlib.ExitStack()
    stack.enter_context(patches)
    stack.enter_context(patch_bundle_constants(bundle))
    stack.enter_context(recovery_patches(recovery, receipt_sha, inventory_sha))
    stack.enter_context(patched(common, FIXTURE_PARENT=parent, FIXTURE_ROOTS=roots))
    return runner, stack, bundle


@test
def test_entry_sweep_positive_passes_with_zero_writes(tmp):
    networks, volumes = load_baseline_maps('networks'), load_baseline_maps('volumes')
    containers = container_ids_or_skip()
    runner, stack, bundle = full_sweep_env(tmp, networks, volumes, containers)
    with stack:
        before = tree_state(tmp)
        bundle_info, go_sha, sweep = common.entry_sweep(bundle['packet_sha'], bundle['go'])
        assert tree_state(tmp) == before, 'the entry sweep must not write anything'
        assert sweep['fixture_roots_absent'] == {'fixture_roots_absent': True}
        assert sweep['fixture_ancestors_safe'] == {'fixture_ancestors_safe': True, 'mount_tables_parsed': True}
        assert sweep['prior_artifacts_absent'] == {'prior_artifacts_absent': True}
        assert sweep['resource_baseline']['network_identity_sha256'] == common.NETWORK_BASELINE_SHA256
        assert sweep['resource_baseline']['volume_identity_sha256'] == common.VOLUME_BASELINE_SHA256
        assert re.fullmatch(r'[0-9a-f]{64}', sweep['historical_baseline']['receipt_sha256'])
        assert sweep['environment']['synthetic_projects_absent'] is True
        assert no_prior_evidence(bundle['base'])
    assert_read_only_calls(runner)


@test
def test_entry_sweep_refuses_drift_with_zero_writes(tmp):
    networks, volumes = load_baseline_maps('networks'), load_baseline_maps('volumes')
    containers = container_ids_or_skip()
    drifted = json.loads(json.dumps(volumes))
    name = sorted(drifted)[0]
    path, value = drifted[name]
    drifted[name] = [path, re.sub(r'^([0-9]+):([0-9]+)', r'\1:31337', value)]
    runner, stack, bundle = full_sweep_env(tmp, networks, volumes, containers, backend_volumes=drifted)
    with stack:
        before = tree_state(tmp)
        expect_refusal(common.entry_sweep, bundle['packet_sha'], bundle['go'])
        assert tree_state(tmp) == before, 'a refusing sweep must not write anything'
        assert no_prior_evidence(bundle['base'])
        assert not (tmp / 'fixture-parent').exists(), 'no fixture parent may be created by a refusing sweep'
    assert_read_only_calls(runner)


@test
def test_launcher_orders_sweep_before_stages(tmp):
    import launch
    containers = container_ids_or_skip()
    bundle = temp_bundle(tmp, load_baseline_maps('networks'), load_baseline_maps('volumes'), containers)
    real_sha = bundle['packet_sha']
    stage_calls = []

    def fake_run(cmd, stdout=None, stderr=None):
        stage_calls.append(list(cmd))
        return FakeCompleted(0, '', '')

    shim = types.SimpleNamespace(run=fake_run, Popen=subprocess.Popen, STDOUT=subprocess.STDOUT)
    sweeps = []

    def fake_entry_sweep(packet_sha, go_path):
        sweeps.append(packet_sha)
        return {'fake': 'bundle'}, 'g' * 64, {'environment': {}, 'resource_baseline': {}}

    launch_base = tmp / 'launch-scratch'
    launch_base.mkdir()
    with patched(common, entry_sweep=fake_entry_sweep), \
         patched(launch, BASE=launch_base, subprocess=shim, load_bundle=lambda: (real_sha, real_sha)):
        launch.main()
    assert sweeps == [real_sha], 'the entry sweep must run first'
    assert [Path(cmd[3]).name for cmd in stage_calls] == ['prepare.py', 'preflight.py', 'runtime.py']
    receipt = json.loads((launch_base / 'r18-invocation-receipt.json').read_text())
    assert receipt['entry_sweep'] == {'environment': {}, 'resource_baseline': {}}
    assert receipt['computed_packet_sha'] == real_sha
    for step in ('prepare', 'preflight', 'runtime'):
        assert (launch_base / f'r18-{step}-console.log').is_file()
    # a refusing sweep stops the launcher before any stage
    stage_calls.clear()
    sweeps.clear()

    def refusing_entry_sweep(packet_sha, go_path):
        sweeps.append(packet_sha)
        raise RuntimeError('live network identities differ from the pinned baseline')

    with patched(common, entry_sweep=refusing_entry_sweep), \
         patched(launch, BASE=launch_base, subprocess=shim, load_bundle=lambda: (real_sha, real_sha)):
        expect_refusal(launch.main)
    assert sweeps == [real_sha], 'exactly the refusing sweep must have run'
    assert stage_calls == [], 'no stage may spawn after a refused sweep'
    assert no_prior_evidence(launch_base)


@test
def test_launcher_refuses_old_packet_go_before_anything(tmp):
    import launch
    launch_base = tmp / 'launch-scratch'
    launch_base.mkdir()
    write_go(launch_base, OLD_REVISION_PACKET_SHA)
    stage_calls = []

    def fake_run(cmd, stdout=None, stderr=None):
        stage_calls.append(list(cmd))
        return FakeCompleted(0, '', '')

    shim = types.SimpleNamespace(run=fake_run, Popen=subprocess.Popen, STDOUT=subprocess.STDOUT)
    with patched(launch, BASE=launch_base, subprocess=shim):
        expect_refusal(launch.main)
    assert stage_calls == []
    assert no_prior_evidence(launch_base)


@test
def test_stage_ordering_is_static_and_helpers_compile(tmp):
    prepare_text = (common.DIR / 'prepare.py').read_text()
    runtime_text = (common.DIR / 'runtime.py').read_text()
    assert prepare_text.index('entry_sweep(') < prepare_text.index('os.mkdir(')
    assert prepare_text.index('entry_sweep(') < prepare_text.index('put(dst/')
    assert runtime_text.index('canonical_digest(net_before)!=NETWORK_BASELINE_SHA256') < runtime_text.index('subprocess.Popen')
    for name in HELPER_FILES:
        compile((common.DIR / name).read_text(), str(common.DIR / name), 'exec')


@test
def test_prepare_sweep_refuses_before_any_write(tmp):
    networks, volumes = load_baseline_maps('networks'), load_baseline_maps('volumes')
    containers = container_ids_or_skip()
    bundle = temp_bundle(tmp, networks, volumes, containers)
    recovery, receipt_sha, inventory_sha = temp_recovery(tmp, networks, volumes)
    drifted = json.loads(json.dumps(networks))
    name = sorted(k for k in drifted if k not in ('host', 'none'))[0]
    drifted[name] = ('9' * 12) + drifted[name][12:]
    runner, patches = install_common_fakes(read_only_rules(drifted, volumes, containers, TEST_HEAD))
    spec = importlib.util.spec_from_file_location('effigy_qual_prepare', common.DIR / 'prepare.py')
    prepare = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prepare)
    with patches, patch_bundle_constants(bundle), \
         recovery_patches(recovery, receipt_sha, inventory_sha), \
         patched(prepare, BASE=bundle['base'], BINARY=bundle['binary'], PARENT=tmp / 'fixture-parent'):
        before = tree_state(tmp)
        argv = [str(common.DIR / 'prepare.py'), '--packet-sha', bundle['packet_sha'], '--go-receipt', str(bundle['go'])]
        with patched(sys, argv=argv):
            expect_refusal(prepare.main)
        assert tree_state(tmp) == before, 'prepare must refuse with zero writes on drifted identities'
        assert not (tmp / 'fixture-parent').exists()
    assert_read_only_calls(runner)


@test
def test_scrubbed_env_home_guard(tmp):
    real = common.scrubbed_env
    home = os.environ.get('HOME')
    try:
        os.environ['HOME'] = '/Users/tom'
        env = real(True)
        assert env['HOME'] == '/Users/tom'
        assert env['PATH'] == '/opt/homebrew/bin:/usr/bin:/bin'
        os.environ['HOME'] = str(tmp)
        expect_refusal(real, True)
    finally:
        if home is None:
            os.environ.pop('HOME', None)
        else:
            os.environ['HOME'] = home


@test
def test_deletion_mount_parsing_matches_entry(tmp):
    runner, patches = install_common_fakes([
        ((lambda a: a[:1] == ['/sbin/mount']), (0, 'broken host mount line\n')),
    ])
    with patches, patched(common, FIXTURE_PARENT=tmp / 'p', FIXTURE_ROOTS=[tmp / 'p' / 'r']):
        root = tmp / 'root'
        root.mkdir()
        expect_refusal(common.validate_deletable, root, common.snapshot(root))
    assert_read_only_calls(runner)


@test
def test_failing_first_pre_fix_guards_accept_what_r18_refuses(tmp):
    """Failing-first: the pre-#542 guards (extracted from git history) accept
    drifted network identities, never observe volume identity, and do not see a
    mount at the fixture parent or a symlinked fixture ancestor — all of which
    let the old prepare flow reach fixture creation. The r18 entry sweep
    refuses each one before any creation, with zero writes."""
    networks, volumes = load_baseline_maps('networks'), load_baseline_maps('volumes')
    containers = container_ids_or_skip()
    old_source = pre_fix_common_source()
    assert 'entry_sweep' not in old_source

    def old_environment_case(rules):
        old_dir = tmp / 'old'
        old_dir.mkdir(exist_ok=True)
        module = {'__name__': 'pre_fix_common', '__file__': str(old_dir / 'common.py')}
        exec(compile(old_source, str(old_dir / 'common.py'), 'exec'), module)
        scratch = tmp / 'old-scratch'
        scratch.mkdir(exist_ok=True)
        (scratch / 'r3-baseline-container-ids.txt').write_text('\n'.join(sorted(containers)) + '\n')
        module['BASE'] = scratch
        module['scrubbed_env'] = lambda with_home=False: dict(NO_ENV)
        runner = FakeRunner(rules)

        def fake_run(args, stdout=None, stderr=None, text=True, timeout=None, env=None, check=False):
            return runner.run(list(args), env, timeout)

        module['subprocess'] = types.SimpleNamespace(run=fake_run, TimeoutExpired=subprocess.TimeoutExpired,
                                                     PIPE=subprocess.PIPE, DEVNULL=subprocess.DEVNULL)
        return runner, module

    # control: exact matching identities pass the old guard
    runner, module = old_environment_case(read_only_rules(networks, volumes, containers, TEST_HEAD))
    assert module['verify_environment']()['synthetic_projects_absent'] is True
    # drifted network identity (same count, replaced name): old guard still passes
    drifted = json.loads(json.dumps(networks))
    drifted['impersonated-net'] = drifted.pop(sorted(k for k in drifted if k not in ('host', 'none'))[0])
    runner, module = old_environment_case(read_only_rules(drifted, volumes, containers, TEST_HEAD))
    assert module['verify_environment']()['synthetic_projects_absent'] is True
    # volume identity drift is never observed, and a mount at the fixture
    # parent is invisible to the old entry guard
    runner, module = old_environment_case(read_only_rules(
        networks, volumes, containers, TEST_HEAD, guest_mount_points=(str(tmp / 'fixture-parent'),)))
    assert 'resource_baseline' not in json.dumps(module['verify_environment']())
    # the old pre-creation prepare guards (parent is_symlink, root lexists)
    # miss a symlinked ancestor: the path resolves through the link as a real
    # directory and neither guard refuses
    old_dir = tmp / 'old2'
    old_dir.mkdir()
    module2 = {'__name__': 'pre_fix_common2', '__file__': str(old_dir / 'common.py')}
    exec(compile(old_source, str(old_dir / 'common.py'), 'exec'), module2)
    (tmp / 'linked-target' / 'parent').mkdir(parents=True)
    link = tmp / 'uplink'
    link.symlink_to(tmp / 'linked-target', target_is_directory=True)
    linked_parent = link / 'parent'
    assert not module2['Path'](str(linked_parent)).is_symlink()
    assert not module2['os'].path.lexists(str(linked_parent / 'root-a'))

    # the r18 sweep refuses every one of these before any creation
    recovery, receipt_sha, inventory_sha = temp_recovery(tmp, networks, volumes)
    bundle = temp_bundle(tmp, networks, volumes, containers)
    parent = tmp / 'fixture-parent'
    parent.mkdir()
    roots = [parent / 'root-a', parent / 'root-b']

    def sweep_case(rules):
        runner, patches = install_common_fakes(rules)
        with patches, patch_bundle_constants(bundle), \
             recovery_patches(recovery, receipt_sha, inventory_sha), \
             patched(common, FIXTURE_PARENT=parent, FIXTURE_ROOTS=roots):
            return common.entry_sweep(bundle['packet_sha'], bundle['go'])

    expect_refusal(sweep_case, read_only_rules(drifted, volumes, containers, TEST_HEAD))
    expect_refusal(sweep_case, read_only_rules(networks, volumes, containers, TEST_HEAD,
                                               guest_mount_points=(str(parent),)))
    runner, patches = install_common_fakes(read_only_rules(networks, volumes, containers, TEST_HEAD))
    with patches, patch_bundle_constants(bundle), \
         recovery_patches(recovery, receipt_sha, inventory_sha), \
         patched(common, FIXTURE_PARENT=linked_parent,
                 FIXTURE_ROOTS=[linked_parent / 'root-a', linked_parent / 'root-b']):
        expect_refusal(common.entry_sweep, bundle['packet_sha'], bundle['go'])
    # zero writes across every refusal above
    assert not any(root.exists() or root.is_symlink() for root in roots)
    assert no_prior_evidence(bundle['base'])


def pre_fix_common_source():
    rel = 'docs/qualification/ordinary-compose-colima-5f466914/common.py'
    git = '/usr/bin/git'
    env = {'PATH': '/usr/bin:/bin', 'LANG': 'C', 'GIT_CONFIG_NOSYSTEM': '1',
           'GIT_CONFIG_GLOBAL': '/dev/null', 'GIT_TERMINAL_PROMPT': '0'}
    log = subprocess.run([git, '-C', str(REPO), 'log', '--format=%H', '-n', '80', 'main', '--', rel],
                         capture_output=True, text=True, env=env)
    if log.returncode:
        raise Skipped('git history unavailable for the pre-fix module')
    for sha in log.stdout.split():
        shown = subprocess.run([git, '-C', str(REPO), 'show', f'{sha}:{rel}'],
                               capture_output=True, text=True, env=env).stdout
        if shown and 'entry_sweep' not in shown:
            return shown
    raise Skipped('no pre-entry-sweep revision of common.py found on main')


@test
def test_real_machine_pins_when_present(tmp):
    """Guarded read-only check against the real pinned evidence and bundle;
    skipped automatically where those pins are absent."""
    binary = Path('/Volumes/Builds/scratch/effigy/524/candidate-src-5f466914/target/debug/effigy')
    baseline = Path('/Volumes/Builds/scratch/effigy/524/r3-baseline-container-ids.txt')
    recovery = Path('/Volumes/Builds/scratch/effigy/colima-recovery-20261102/recovery-receipt.json')
    if not (binary.is_file() and baseline.is_file() and recovery.is_file()):
        raise Skipped('pinned scratch artifacts or recovery evidence absent on this host')
    manifest = json.loads((common.DIR / 'r18-artifacts.json').read_text())
    assert common.digest(binary) == manifest['candidate-src-5f466914/target/debug/effigy']
    assert common.digest(baseline) == manifest['r3-baseline-container-ids.txt']
    assert common.digest(recovery) == common.RECOVERY_RECEIPT_SHA256
    result = common.verify_historical_baseline()
    assert result['network_count'] == 36 and result['volume_count'] == 255


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('rest', nargs='*', help='reserved; unused')
    parser.parse_args(argv)
    results = {}
    for fn in TESTS:
        label = fn.__name__
        try:
            with tempfile.TemporaryDirectory(prefix='qes-fixture-') as scratch:
                fn(Path(scratch))
            results[label] = 'pass'
            print(f'pass {label}', flush=True)
        except Skipped as reason:
            results[label] = f'skipped ({reason})'
            print(f'skip {label}: {reason}', flush=True)
        except Exception as error:  # the harness records the failure and exits nonzero
            results[label] = f'FAIL: {type(error).__name__}: {error}'
            FAILURES.append(label)
            print(f'FAIL {label}: {type(error).__name__}: {error}', flush=True)
    receipt = {'proof': 'qualification-entry-sweep-fakes', 'tests': results,
               'failed': FAILURES, 'passed': not FAILURES}
    print(json.dumps(receipt, indent=2), flush=True)
    return 1 if FAILURES else 0


if __name__ == '__main__':
    raise SystemExit(main(sys.argv[1:]))

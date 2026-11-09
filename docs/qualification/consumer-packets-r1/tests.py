#!/usr/bin/env python3
"""Fail-closed controls for the r1 consumer packets (task effigy#757).

Source and fixture only. Every route is driven through its real entry
boundary (`main` or `run`) with injected fixture transports. The one real
child runner is exercised only against harmless `sys.executable -I -c` fixture
children, which the runner's own process-group cleanup owns. No test touches a
real engine, VM, consumer checkout, row, secret or network.

Fixtures are fresh directories under TMPDIR (`tempfile.mkdtemp`). Nothing is
written outside them.
"""
import contextlib
import copy
import io
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import types
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import acow  # noqa: E402
import acow_battery  # noqa: E402
import cbs  # noqa: E402
import consumer_core as core  # noqa: E402

FAILURES = []
PASSED = []
C = 'c' * 40
H = 'd' * 40
M = 'e' * 40
SHA = 'sha256:' + 'f' * 64
NS = 10 ** 9
WINDOW_START = 5_000
WINDOW_DEADLINE = WINDOW_START + 600 * NS
NOW = WINDOW_START + 1
ROOT = Path(os.path.realpath(tempfile.mkdtemp(prefix='consumer-packets-tests-')))


def check(name):
    def wrap(fn):
        try:
            fn()
            PASSED.append(name)
        except AssertionError as exc:
            FAILURES.append('%s: %s' % (name, exc))
        except Exception as exc:  # a test that crashes is a failure, not a pass
            FAILURES.append('%s: unexpected %s: %s' % (name, type(exc).__name__, exc))
        return fn
    return wrap


def refuses(code, fn):
    try:
        fn()
    except core.Refused as exc:
        assert exc.code == code, 'expected %s got %s (%s)' % (code, exc.code, exc.detail)
        return
    raise AssertionError('expected refusal %s, nothing refused' % code)


def fill(node, key=''):
    """Replace every pending pin with a concrete fixture value (test-only)."""
    if isinstance(node, dict):
        if node.get('kind') == 'container' and 'identity' in node:
            node['identity'] = 'e' * 64
        for name in list(node):
            node[name] = fill(node[name], name)
        return node
    if isinstance(node, list):
        return [fill(item, key) for item in node]
    if not (isinstance(node, str) and (node == 'pending' or node.startswith('pending:'))):
        return node
    values = {
        'source_commit': C, 'merged_commit': M, 'source_immutable_head': H,
        'head': H, 'resolved_revision': H, 'receipt_sha256': SHA,
        'binary_sha256': SHA, 'content_digest': SHA, 'version_marker': 'fixture-C',
        'status_contract': 'fixture-verified', 'protected_baseline': 'fixture-baseline',
        'identity': 'fixture-volume-identity', 'name': 'fixture-name',
        'count_tail': ['mysql', '--batch', '--skip-column-names', '-uroot'],
        'baseline': 3,
    }
    if key == 'install_identity':
        return {'path': '/fixture/bin/effigy', 'version': 'fixture-C', 'sha256': SHA}
    return values.get(key, SHA)


def fixture_manifest():
    manifest = copy.deepcopy(core.load_manifest())
    for key in ('candidate', 'predecessor', 'helpers', 'provenance'):
        manifest[key] = fill(manifest[key], key)
    manifest['candidate']['install_identity'] = {
        'path': '/fixture/bin/effigy', 'version': 'fixture-C', 'sha256': SHA}
    manifest['helpers']['modules'] = {name: SHA for name in manifest['helpers']['modules']}
    for route in manifest['routes'].values():
        for key in list(route):
            route[key] = fill(route[key], key)
    manifest['routes']['acow']['counts'] = {
        table: {'baseline': 3, 'delta': 0, 'citation': 'fixture'}
        for table in manifest['routes']['acow']['counts']}
    cbs_tables = manifest['routes']['cbs']['counts']['tables']
    for db in cbs_tables:
        for table in cbs_tables[db]:
            cbs_tables[db][table] = {'baseline': 2, 'delta': 0, 'citation': 'fixture'}
    return manifest


def request_for(manifest, route, prior_intent):
    pinned = manifest['routes'][route]
    return copy.deepcopy({
        'schema': core.REQUEST_SCHEMA,
        'route': route,
        'candidate': {
            'source_commit': manifest['candidate']['source_commit'],
            'binary_sha256': manifest['candidate']['binary_sha256'],
            'install_identity': manifest['candidate']['install_identity'],
        },
        'helpers': {'head': manifest['helpers']['head'],
                    'modules': manifest['helpers']['modules']},
        'predecessor': {'receipt_sha256': manifest['predecessor']['receipt_sha256'],
                        'same_candidate_commit': manifest['candidate']['source_commit']},
        'consumer': {
            'selected_head': '1' * 40,
            'relevant_inputs': pinned['consumer']['relevant_inputs'],
            'includes': pinned['consumer']['includes'],
            'bundle': {'resolved_revision': pinned['consumer']['bundle']['resolved_revision'],
                       'content_digest': pinned['consumer']['bundle']['content_digest']},
        },
        'provenance': {'merged_commit': M, 'source_immutable_head': H},
        'window': {'id': 'fixture-window-' + route, 'token_sha256': SHA,
                   'start_monotonic_ns': WINDOW_START,
                   'deadline_monotonic_ns': WINDOW_DEADLINE},
        'budgets': dict(pinned['budgets']),
        'resources': [dict(role=role, **res) for role, res in sorted(pinned['resources'].items())],
        'prior_intent': prior_intent,
    })


def authenticate_ok(provenance):
    return {'authenticated': True, 'merged_commit': M, 'source_immutable_head': H}


class FakeEngine:
    """Deterministic engine: running state, counts, and scripted failures."""

    def __init__(self, running, counts, children, fail=None):
        self.running = dict(running)
        self.counts = dict(counts)
        self.children = Path(children)
        self.fail = fail or {}
        self.calls = []

    def __call__(self, argv, label):
        self.calls.append((label, tuple(argv[1:])))
        if label in self.fail:
            raise core.Refused(self.fail[label], label)
        out = self._output(argv)
        path = self.children / (label + '.stdout')
        self.children.mkdir(parents=True, exist_ok=True)
        path.write_text(out)
        return {'label': label, 'exit': 0,
                'streams': {'stdout': {'path': str(path)}}}

    def _output(self, argv):
        tail = list(argv[1:])
        if 'exec' in tail:
            name = tail[-1].split(' FROM ', 1)[1].replace('`', '')
            return str(self.counts[name])
        env = tail[tail.index('--system') + 1] if '--system' in tail else tail[1]
        if 'status' in tail:
            return json.dumps({'running': self.running[env]})
        if 'up' in tail:
            self.running[env] = True
        if 'down' in tail:
            self.running[env] = False
        return ''


def acow_counts():
    manifest = fixture_manifest()
    return {table: 3 for table in acow.expected_counts(manifest)}


def cbs_counts():
    manifest = fixture_manifest()
    return {key: 2 for key in cbs.expected_counts(manifest)}


def evidence(name):
    path = ROOT / name / 'evidence'
    path.mkdir(parents=True)
    return str(path)


def run_route(module, route, request, name, engine, authenticate=authenticate_ok, manifest=None):
    manifest = manifest or fixture_manifest()
    req_path = ROOT / name / 'request.json'
    req_path.parent.mkdir(parents=True, exist_ok=True)
    req_path.write_text(json.dumps(request))
    ev = evidence(name)
    return module.run(str(req_path), ev, manifest=manifest, transport=engine,
                      authenticate=authenticate, now_ns=NOW,
                      roots=(str(ROOT),)), ev


def mutate(manifest, route, request, field, value=None, delete=False):
    req = copy.deepcopy(request)
    if delete:
        del req[field]
    else:
        req[field] = value
    return req


def no_child(*args, **kwargs):
    raise AssertionError('a child process was started')


# ---------------------------------------------------------------- committed state


@check('fixture manifest is fully concrete, so only the pins under test refuse')
def _():
    assert core.pending_pins(fixture_manifest()) == []


@check('committed manifest is pending everywhere')
def _():
    manifest = core.load_manifest()
    for route in ('acow', 'cbs'):
        refuses('candidate-unfrozen', lambda: core.require_frozen(manifest, route))
    assert core.pending_pins({'x': 'pending: later'}) == ['x']


@check('unarmed CLI refuses ordinary invocation before any child')
def _():
    original = subprocess.Popen
    subprocess.Popen = no_child
    try:
        for module in (acow, cbs):
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                assert module.main([]) == 2
            assert json.loads(out.getvalue())['code'] == 'unarmed'
    finally:
        subprocess.Popen = original


@check('committed proof entry refuses a request at candidate-unfrozen, no evidence written')
def _():
    request_path = ROOT / 'committed' / 'request.json'
    request_path.parent.mkdir(parents=True, exist_ok=True)
    manifest = core.load_manifest()
    request = request_for(fixture_manifest(), 'acow', {'stack': False, 'hybrid': True})
    request_path.write_text(json.dumps(request))
    ev = ROOT / 'committed' / 'evidence'
    original = subprocess.Popen
    subprocess.Popen = no_child
    try:
        refuses('candidate-unfrozen', lambda: acow.run(
            str(request_path), str(ev), manifest=manifest, roots=(str(ROOT),), now_ns=NOW))
    finally:
        subprocess.Popen = original
    assert not ev.exists()


@check('committed Acow pins reconcile with the reviewed 5f466914 packet')
def _():
    hist = core._HIST
    pinned = core.load_manifest()['routes']['acow']
    assert pinned['consumer']['relevant_inputs'] == hist.RELEVANT_INPUT_BLOBS
    assert pinned['consumer']['includes'] == hist.MANIFEST_INCLUDES
    for table, (value, _, _) in hist.EXPECTED_PRIMARY_COUNTS.items():
        assert pinned['counts'][table]['baseline'] == value, table


@check('CBS counts and resources are pending, not borrowed from Acow')
def _():
    pins = core.load_manifest()['routes']['cbs']
    assert pins['counts']['tables']['cbs']['user_client']['baseline'] == 'pending'
    assert 'cbs' in pins['counts']['tables'] and 'cbs-mortcalc' in pins['counts']['tables']


# ---------------------------------------------------------------- argv and shapes


@check('static plans are fixed allowlisted argv, dev:test undeclared as armed')
def _():
    binary = '/fixture/bin/effigy'
    for step_label, argv in acow.plan({'stack': False, 'hybrid': True}, binary):
        core.check_argv(argv, acow.ALLOWED_TAILS, binary)
    for step_label, argv in cbs.plan({'dev': False}, binary):
        core.check_argv(argv, cbs.ALLOWED_TAILS, binary)
    assert all('dev:test' not in ' '.join(argv) for _, argv in acow.plan({}, binary))


@check('argv outside the allowlist, relative binaries and wrong binaries refuse')
def _():
    refuses('argv-not-allowlisted', lambda: core.check_argv(
        ['/b/effigy', 'container', 'stack', 'shell', '--command', 'id'], acow.ALLOWED_TAILS))
    refuses('argv-not-pinned', lambda: core.check_argv(
        ['effigy', 'container', 'stack', 'status', '--json'], acow.ALLOWED_TAILS))
    refuses('argv-not-pinned', lambda: core.check_argv(
        ['/other/effigy', 'container', 'stack', 'status', '--json'], acow.ALLOWED_TAILS, '/b/effigy'))


@check('counts plan refuses tables and databases outside the named set')
def _():
    engine = fixture_manifest()['routes']['acow']['engine']
    refuses('counts-plan', lambda: acow.count_argv(engine, 'e' * 64, 'auth.users; drop'))
    cengine = fixture_manifest()['routes']['cbs']['counts']['engine']
    refuses('counts-plan', lambda: cbs.count_argv(cengine, 'e' * 64, 'mysql', 'user_client'))


@check('status must name a boolean running state')
def _():
    refuses('status-unparsed', lambda: acow.parse_running('not json'))
    refuses('status-unparsed', lambda: acow.parse_running('{"running": "yes"}'))
    assert acow.parse_running('{"running": true}') is True


# ---------------------------------------------------------------- request gates


def _gate(mutator, code, route='acow'):
    manifest = fixture_manifest()
    request = request_for(manifest, route, {'stack': False, 'hybrid': True} if route == 'acow' else {'dev': False})
    mutator(request)
    refuses(code, lambda: core.validate_request(request, manifest, route, authenticate_ok, NOW))


@check('stale or wrong C refuses')
def _():
    _gate(lambda r: r['candidate'].update(source_commit='a' * 40), 'candidate-mismatch')
    _gate(lambda r: r['candidate'].update(binary_sha256=SHA.replace('f', '0')), 'candidate-mismatch')


@check('stale positive624 and failed predecessor refuse')
def _():
    _gate(lambda r: r['predecessor'].update(receipt_sha256='sha256:' + '1' * 64), 'predecessor-mismatch')
    _gate(lambda r: r['predecessor'].update(same_candidate_commit='a' * 40), 'predecessor-mismatch')


@check('input, include and bundle drift refuse')
def _():
    _gate(lambda r: r['consumer']['relevant_inputs'].update({'effigy.toml': '0' * 40}), 'input-drift')
    _gate(lambda r: r['consumer'].update(includes=['infra/workspace.toml']), 'input-drift')
    _gate(lambda r: r['consumer']['bundle'].update(content_digest=SHA.replace('f', '1')), 'bundle-drift')
    _gate(lambda r: r['consumer']['bundle'].update(resolved_revision='0' * 40), 'bundle-drift')


@check('helper hash drift and a non-commit selected head refuse')
def _():
    _gate(lambda r: r['helpers']['modules'].update({'acow.py': 'sha256:' + '0' * 64}), 'helper-mismatch')
    _gate(lambda r: r['consumer'].update(selected_head='main'), 'consumer-mismatch')


@check('missing, consumed, expired and oversized windows refuse')
def _():
    _gate(lambda r: r['window'].update(token_sha256='not-a-digest'), 'window-missing')
    _gate(lambda r: r['window'].update(deadline_monotonic_ns=WINDOW_START + 10_000 * NS), 'window-bounds')
    _gate(lambda r: r['window'].update(deadline_monotonic_ns=WINDOW_START - 1), 'window-bounds')


@check('an already expired window and a consumed window refuse')
def _():
    manifest = fixture_manifest()
    manifest['consumed_windows'] = ['fixture-window-acow']
    request = request_for(manifest, 'acow', {'stack': False, 'hybrid': True})
    refuses('window-consumed', lambda: core.validate_request(
        request, manifest, 'acow', authenticate_ok, NOW))
    manifest['consumed_windows'] = []
    refuses('window-expired', lambda: core.validate_request(
        request, manifest, 'acow', authenticate_ok, WINDOW_DEADLINE + 1))


@check('foreign or replaced resources and over-budget requests refuse')
def _():
    _gate(lambda r: r['resources'][0].update(identity='f' * 64), 'resource-mismatch')
    _gate(lambda r: r['resources'].pop(), 'resource-mismatch')
    _gate(lambda r: r['budgets'].update(stdout_cap_bytes=10 ** 9), 'budget-exceeded')


@check('provenance absent, unauthenticated, or for another commit refuses')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'stack': False, 'hybrid': True})
    refuses('nucleus-provenance-unavailable', lambda: core.validate_request(
        request, manifest, 'acow', None, NOW))
    refuses('nucleus-provenance-unavailable', lambda: core.validate_request(
        request, manifest, 'acow', lambda p: {'authenticated': False}, NOW))
    refuses('provenance-mismatch', lambda: core.validate_request(
        request, manifest, 'acow',
        lambda p: {'authenticated': True, 'merged_commit': 'a' * 40, 'source_immutable_head': H}, NOW))


@check('request shape, duplicate keys and oversized files refuse')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'stack': False, 'hybrid': True})
    request['extra'] = 1
    refuses('request-shape', lambda: core.validate_request_shape(request, 'acow'))
    dup = ROOT / 'dup' / 'request.json'
    dup.parent.mkdir(parents=True, exist_ok=True)
    dup.write_text('{"schema": "a", "schema": "b"}')
    refuses('duplicate-key', lambda: core.load_request(str(dup)))
    big = ROOT / 'big' / 'request.json'
    big.parent.mkdir(parents=True, exist_ok=True)
    big.write_text('x' * (core.MAX_REQUEST_BYTES + 1))
    refuses('file-too-large', lambda: core.load_request(str(big)))


@check('evidence roots refuse symlinks, relative paths and outside directories')
def _():
    link = ROOT / 'linkroot'
    if not link.exists():
        os.symlink(str(ROOT / 'committed'), str(link))
    refuses('evidence-root', lambda: core.within_root(str(link / 'x'), (str(ROOT),)))
    refuses('evidence-root', lambda: core.within_root('relative/dir', (str(ROOT),)))
    refuses('evidence-root', lambda: core.within_root('/var/elsewhere', (str(ROOT),)))


# ---------------------------------------------------------------- child runner


@check('child runner keeps stdout and stderr separate with the waited exit')
def _():
    out = ROOT / 'runner-pass'
    rec = core.run_child(
        [sys.executable, '-I', '-c',
         'import sys; sys.stdout.write("out-bytes"); sys.stderr.write("err-bytes")'],
        timeout_s=30, stdout_cap=1024, stderr_cap=1024, out_dir=out, label='pass')
    assert rec['exit'] == 0
    assert Path(rec['streams']['stdout']['path']).read_bytes() == b'out-bytes'
    assert Path(rec['streams']['stderr']['path']).read_bytes() == b'err-bytes'


@check('over-cap output refuses and is recorded as truncated')
def _():
    out = ROOT / 'runner-overcap'
    refuses('over-cap-stream', lambda: core.run_child(
        [sys.executable, '-I', '-c', 'import sys; sys.stdout.write("x" * 5000)'],
        timeout_s=30, stdout_cap=100, stderr_cap=100, out_dir=out, label='overcap'))
    record = json.loads((out / 'overcap.record.json').read_text())
    assert record['streams']['stdout']['truncated'] is True
    assert record['streams']['stdout']['bytes'] == 100


@check('a child past its deadline is killed by its own group and refuses')
def _():
    out = ROOT / 'runner-deadline'
    started = time.monotonic()
    refuses('deadline-exceeded', lambda: core.run_child(
        [sys.executable, '-I', '-c', 'import time; time.sleep(60)'],
        timeout_s=1, stdout_cap=100, stderr_cap=100, out_dir=out, label='deadline'))
    assert time.monotonic() - started < 30
    record = json.loads((out / 'deadline.record.json').read_text())
    assert record['exit'] < 0


@check('a failed exit refuses as child-failed and keeps its record')
def _():
    out = ROOT / 'runner-fail'
    refuses('child-failed', lambda: core.run_child(
        [sys.executable, '-I', '-c', 'import sys; sys.exit(3)'],
        timeout_s=30, stdout_cap=100, stderr_cap=100, out_dir=out, label='fail'))
    assert json.loads((out / 'fail.record.json').read_text())['exit'] == 3


@check('a relative argv[0] never reaches the runner')
def _():
    refuses('argv-not-pinned', lambda: core.check_argv(['python3', '-c', 'pass'], frozenset()))


# ---------------------------------------------------------------- fixture positives


@check('fixture-positive Acow: prior-stopped stack is started, prior-running hybrid stays')
def _():
    manifest = fixture_manifest()
    children = ROOT / 'acow-positive' / 'children'
    engine = FakeEngine({'stack': False, 'hybrid': True}, acow_counts(), children)
    request = request_for(manifest, 'acow', {'stack': False, 'hybrid': True})
    result, ev = run_route(acow, 'acow', request, 'acow-positive', engine)
    assert result['status'] == 'passed'
    assert result['started'] == ['stack']
    labels = [label for label, _ in engine.calls]
    assert labels.index('hybrid-1-up') < labels.index('container-up') < labels.index('hybrid-2-up')
    assert 'rollback-stack' in labels and 'rollback-hybrid' not in labels
    assert engine.running == {'stack': False, 'hybrid': True}
    occurrence = json.loads((Path(ev) / 'occurrence.json').read_text())
    assert occurrence['status'] == 'passed' and occurrence['spent'] is False


@check('fixture-positive CBS: detached app-shell, forward, counts, scoped rollback')
def _():
    manifest = fixture_manifest()
    children = ROOT / 'cbs-positive' / 'children'
    engine = FakeEngine({'dev': False}, cbs_counts(), children)
    request = request_for(manifest, 'cbs', {'dev': False})
    result, _ = run_route(cbs, 'cbs', request, 'cbs-positive', engine)
    assert result['status'] == 'passed' and result['started'] == ['dev']
    assert engine.running == {'dev': False}
    assert engine.calls[-1][0] == 'rollback-dev'


@check('prior intent that drifts before any bring-up refuses')
def _():
    manifest = fixture_manifest()
    children = ROOT / 'acow-drift' / 'children'
    engine = FakeEngine({'stack': True, 'hybrid': True}, acow_counts(), children)
    request = request_for(manifest, 'acow', {'stack': False, 'hybrid': True})
    try:
        run_route(acow, 'acow', request, 'acow-drift', engine)
    except core.Refused as exc:
        assert exc.code == 'prior-intent'
    else:
        raise AssertionError('drifted prior intent did not refuse')
    assert not any(label.endswith('-up') for label, _ in engine.calls)


@check('unexpected counts refuse after the armed bring-up and roll back')
def _():
    manifest = fixture_manifest()
    children = ROOT / 'acow-counts' / 'children'
    counts = acow_counts()
    counts['auth.users'] = 99
    engine = FakeEngine({'stack': False, 'hybrid': False}, counts, children)
    request = request_for(manifest, 'acow', {'stack': False, 'hybrid': False})
    try:
        run_route(acow, 'acow', request, 'acow-counts', engine)
    except core.Refused as exc:
        assert exc.code == 'unexpected-counts'
    else:
        raise AssertionError('unexpected counts did not refuse')
    assert engine.running == {'stack': False, 'hybrid': False}
    occurrence = json.loads((ROOT / 'acow-counts' / 'evidence' / 'occurrence.json').read_text())
    assert occurrence['status'] == 'unexpected-counts' and occurrence['spent'] is True


@check('a failed rollback refuses as a spent occurrence with its evidence retained')
def _():
    manifest = fixture_manifest()
    children = ROOT / 'cbs-rollback' / 'children'
    engine = FakeEngine({'dev': False}, cbs_counts(), children,
                        fail={'rollback-dev': 'child-failed'})
    request = request_for(manifest, 'cbs', {'dev': False})
    try:
        run_route(cbs, 'cbs', request, 'cbs-rollback', engine)
    except core.Refused as exc:
        assert exc.code == 'child-failed'
    else:
        raise AssertionError('interrupted rollback did not refuse')
    occurrence = json.loads((ROOT / 'cbs-rollback' / 'evidence' / 'occurrence.json').read_text())
    assert occurrence['spent'] is True


@check('a failed bring-up refuses and leaves the prior-running environment running')
def _():
    manifest = fixture_manifest()
    children = ROOT / 'acow-up-fail' / 'children'
    engine = FakeEngine({'stack': False, 'hybrid': True}, acow_counts(), children,
                        fail={'container-up': 'child-failed'})
    request = request_for(manifest, 'acow', {'stack': False, 'hybrid': True})
    try:
        run_route(acow, 'acow', request, 'acow-up-fail', engine)
    except core.Refused as exc:
        assert exc.code == 'child-failed'
    else:
        raise AssertionError('failed bring-up did not refuse')
    assert engine.running['hybrid'] is True


@check('Acow and CBS static plans never name dev:test, seed, deploy or sync')
def _():
    forbidden = re.compile(r'dev:test|seed|deploy|sync|schema|down --global|prune')
    for argv in [a for _, a in acow.plan({'stack': False}, '/b/effigy')] + \
            [a for _, a in cbs.plan({'dev': False}, '/b/effigy')]:
        assert not forbidden.search(' '.join(argv)), argv


# ---------------------------------------------------------------- acow battery harness


class _FakeModule:
    def __init__(self, scratch):
        self.SCRATCH = scratch


@check('acow battery harness refuses without a usable TMPDIR and never falls back')
def _():
    builds = Path('/Volumes/Builds/scratch/effigy/540')
    before = builds.exists()
    refuses_env = {'PATH': os.environ.get('PATH', '')}
    result = subprocess.run(
        [sys.executable, '-I', '-B', str(Path(acow_battery.__file__))],
        env=refuses_env, capture_output=True, text=True, timeout=120)
    assert result.returncode == 2, result.returncode
    assert 'TMPDIR' in result.stderr
    assert builds.exists() == before, 'refused run touched the Builds scratch'
    for bad in ('', 'relative/dir'):
        try:
            acow_battery.tmp_root_from_env({'TMPDIR': bad})
        except RuntimeError:
            continue
        raise AssertionError('accepted TMPDIR %r' % bad)
    link = ROOT / 'battery-link'
    if not link.exists():
        os.symlink(str(ROOT), str(link))
    try:
        acow_battery.tmp_root_from_env({'TMPDIR': str(link)})
    except RuntimeError:
        pass
    else:
        raise AssertionError('accepted a symlinked TMPDIR')


@check('acow battery harness repoints every bound SCRATCH and restores it on exit')
def _():
    original = Path('/Volumes/Builds/scratch/effigy/540')
    modules = (_FakeModule(original), _FakeModule(original), _FakeModule(original),
               _FakeModule(None))
    modules[3].SCRATCH = original
    root = Path(os.path.realpath(ROOT / 'battery-tmp'))
    root.mkdir(parents=True, exist_ok=True)
    with acow_battery.relocated_scratch(modules, str(root)) as fresh:
        for module in modules:
            assert module.SCRATCH == fresh
        assert fresh.parent == root
        assert oct(fresh.stat().st_mode & 0o777) == '0o700'
        assert fresh.exists()
    for module in modules:
        assert module.SCRATCH == original
    assert not fresh.exists(), 'empty fresh scratch was not removed'


@check('acow battery harness restores SCRATCH after an error and keeps evidence')
def _():
    original = Path('/Volumes/Builds/scratch/effigy/540')
    module = _FakeModule(original)
    root = Path(os.path.realpath(ROOT / 'battery-error'))
    root.mkdir(parents=True, exist_ok=True)
    kept = None
    try:
        with acow_battery.relocated_scratch((module,), str(root)) as fresh:
            kept = fresh
            (fresh / 'receipt.json').write_text('evidence')
            raise RuntimeError('boom')
    except RuntimeError:
        pass
    assert module.SCRATCH == original
    assert (kept / 'receipt.json').read_text() == 'evidence'


@check('acow battery harness refuses modules that bind no SCRATCH')
def _():
    try:
        with acow_battery.relocated_scratch((types.SimpleNamespace(),), str(ROOT)):
            pass
    except RuntimeError:
        return
    raise AssertionError('accepted a packet with no SCRATCH binding')


def main():
    for line in PASSED:
        print('ok   ' + line)
    for line in FAILURES:
        print('FAIL ' + line)
    print('%d passed, %d failed' % (len(PASSED), len(FAILURES)))
    return 1 if FAILURES else 0


if __name__ == '__main__':
    sys.exit(main())

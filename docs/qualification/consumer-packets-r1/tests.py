#!/usr/bin/env python3
"""Fail-closed controls for the r1 consumer packets (task effigy#757, armed by effigy#760).

Three kinds of control, none of them touching a real engine, VM, consumer
checkout, row, secret or network.

1. Gate controls. Each route runs through `arm` with an injected fixture engine
   transport and an injected fixture git reader. A refusal must come from the
   named gate, before the first effect where the gate reads no child.
2. Supervision controls. The real `Supervisor`, which drives the shipped
   `preparation_stage.run_stage`, runs harmless `sys.executable -I -c` children
   for stream capture, caps, deadlines, waited exits and escaping descendants.
3. End-to-end controls. A fixture effigy/Colima binary, a real git checkout at
   the reviewed closure and the real supervisor run each route through
   `proof_entry.run`. The request is bound to the packet's real HEAD and real
   module bytes. This is the reached-the-supervisor proof. It is fixture
   evidence only, not consumer, native or engine credit.

Fixtures are fresh directories under TMPDIR (`tempfile.mkdtemp`). Nothing is
written outside them. Fixture children are stopped by their recorded PIDs.
"""
import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import types
from pathlib import Path

DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(DIR))

import acow  # noqa: E402
import acow_acquisition as acq  # noqa: E402
import acow_battery  # noqa: E402
import cbs  # noqa: E402
import consumer_core as core  # noqa: E402
import proof_entry  # noqa: E402

FAILURES = []
PASSED = []
NS = 10 ** 9
START = 5_000
NOW = START + 1
CAND = 'c' * 40
HEAD = '1' * 40
HEAD2 = '2' * 40
H = 'd' * 40
M = 'e' * 40
SHA = 'sha256:' + 'f' * 64
SHA2 = 'sha256:' + '9' * 64
TOKEN = 'sha256:' + 'a' * 64
WINDOW_ID = {'acow': 'fixture-window-acow', 'cbs': 'fixture-window-cbs'}
PG_ID = 'e' * 64
DB_ID = 'd' * 64
CBS_VOLUME = 'fixture-cbs-volume'
ROOT = Path(os.path.realpath(tempfile.mkdtemp(prefix='consumer-packets-tests-')))
for _route in ('acow', 'cbs'):
    (ROOT / 'checkouts' / _route).mkdir(parents=True, exist_ok=True)
(ROOT / 'bin').mkdir(parents=True, exist_ok=True)
FIXTURE_BIN = ROOT / 'bin' / 'effigy'
FIXTURE_BIN.write_bytes(b'fixture candidate C, harmless bytes\n')
FIXTURE_BIN_SHA = core.sha256_file(FIXTURE_BIN)


def check(name):
    def wrap(fn):
        try:
            fn()
            PASSED.append(name)
        except AssertionError as exc:
            FAILURES.append('%s: %s' % (name, exc))
        except Exception as exc:  # a crashing control is a failure, not a pass
            FAILURES.append('%s: unexpected %s: %s' % (name, type(exc).__name__, exc))
        return fn
    return wrap


def expect_code(code, fn):
    try:
        fn()
    except core.Refused as exc:
        assert exc.code == code, 'expected %s got %s (%s)' % (code, exc.code, exc.detail)
        return
    raise AssertionError('expected refusal %s, nothing refused' % code)


def checkout_of(route):
    return os.path.realpath(ROOT / 'checkouts' / route)


def fill(node, key=''):
    """Replace every pending pin with a concrete fixture value (test-only)."""
    if isinstance(node, dict):
        for name in list(node):
            node[name] = fill(node[name], name)
        return node
    if isinstance(node, list):
        if key == 'count_tail':
            return ['mysql', '--batch', '--skip-column-names', '-uroot']
        return [fill(item, key) for item in node]
    if not (isinstance(node, str) and (node == 'pending' or node.startswith('pending:'))):
        return node
    values = {
        'resolved_revision': H, 'content_digest': SHA, 'status_contract': 'fixture-verified',
        'baseline': 2, 'project': 'fixture-cbs-project',
        'approved_revision_prefix': 'd' * 7,
    }
    return values.get(key, 'fixture')


def fixture_manifest():
    manifest = copy.deepcopy(core.load_manifest())
    for route in manifest['routes'].values():
        for key in list(route):
            route[key] = fill(route[key], key)
    acow_pins = manifest['routes']['acow']
    acow_pins['checkout'] = checkout_of('acow')
    acow_pins['counts'] = {table: {'baseline': 3, 'delta': 0, 'citation': 'fixture'}
                           for table in acow_pins['counts']}
    acow_pins['consumer']['bundle']['approved_revision_prefix'] = 'd' * 7
    manifest['routes']['cbs']['checkout'] = checkout_of('cbs')
    return manifest


def request_for(manifest, route, prior_intent, merged=M, helper_head=H):
    pins = manifest['routes'][route]
    if route == 'acow':
        resources = [
            {'role': 'postgres-container', 'kind': 'container', 'identity': PG_ID},
            {'role': 'postgres-volume', 'kind': 'volume',
             'identity': pins['identity']['volume']},
        ]
    else:
        resources = [
            {'role': 'database-container', 'kind': 'container', 'identity': DB_ID},
            {'role': 'engine-volume', 'kind': 'volume', 'identity': CBS_VOLUME},
        ]
    return {
        'schema': core.REQUEST_SCHEMA,
        'route': route,
        'candidate': {'source_commit': CAND, 'binary_sha256': FIXTURE_BIN_SHA,
                      'install_identity': {'path': str(FIXTURE_BIN), 'version': 'fixture-C',
                                           'sha256': FIXTURE_BIN_SHA}},
        'helpers': {'head': helper_head, 'modules': core.module_hashes()},
        'predecessor': {'receipt_sha256': SHA2, 'same_candidate_commit': CAND},
        'consumer': {
            'selected_head': HEAD,
            'relevant_inputs': copy.deepcopy(pins['consumer']['relevant_inputs']),
            'includes': copy.deepcopy(pins['consumer']['includes']),
            'bundle': {'resolved_revision': H,
                       'content_digest': pins['consumer']['bundle']['content_digest']},
        },
        'provenance': {'merged_commit': merged, 'source_immutable_head': helper_head},
        'window': {'id': WINDOW_ID[route], 'token_sha256': TOKEN,
                   'start_monotonic_ns': START, 'deadline_monotonic_ns': START + 600 * NS},
        'budgets': copy.deepcopy(pins['budgets']),
        'resources': resources,
        'prior_intent': prior_intent,
    }


def fake_git(manifest, route, head=HEAD, status='', blobs=None, calls=None, helper=M):
    """Object ids and status only. The packet checkout reads as the merged commit."""
    closure = dict(manifest['routes'][route]['consumer']['relevant_inputs'])
    closure.update(blobs or {})
    log = calls if calls is not None else []

    def git(args, cwd):
        log.append((list(args), str(cwd)))
        if str(cwd) == str(core.DIR):
            if list(args) == ['rev-parse', 'HEAD']:
                return helper + '\n'
            raise AssertionError('unexpected packet-checkout read %r' % (args,))
        if list(args) == ['rev-parse', 'HEAD']:
            return head + '\n'
        if list(args) == ['status', '--porcelain']:
            return status
        if len(args) == 2 and args[0] == 'rev-parse' and args[1].startswith('HEAD:'):
            path = args[1][len('HEAD:'):]
            if path not in closure:
                raise core.Refused('input-drift', 'no blob at ' + path)
            return closure[path] + '\n'
        raise AssertionError('unexpected git read %r' % (args,))
    return git


def acow_counts():
    return {table: 3 for table in acow.expected_counts(fixture_manifest()['routes']['acow'])}


def cbs_counts():
    return {key: 2 for key in cbs.expected_counts(fixture_manifest()['routes']['cbs'])}


class FakeEngine:
    """A deterministic engine transport. Asserts every call runs from the pinned checkout."""

    def __init__(self, route, children, running, counts=None, containers=None,
                 volumes=None, fail=None, on_up=None, fail_after=None):
        manifest = fixture_manifest()
        identity = manifest['routes'][route]['identity']
        self.route = route
        self.children = Path(children)
        self.running = dict(running)
        self.counts = dict(counts if counts is not None
                           else (acow_counts() if route == 'acow' else cbs_counts()))
        if containers is None:
            if route == 'acow':
                containers = [
                    {'id': PG_ID, 'project': identity['project'], 'service': identity['service'],
                     'mounts': [identity['volume']]},
                    {'id': identity['shared_container'], 'project': None, 'service': None,
                     'mounts': []},
                ]
            else:
                containers = [{'id': DB_ID, 'project': identity['project'],
                               'service': identity['service'], 'mounts': [CBS_VOLUME]}]
        self.containers = containers
        if volumes is None:
            volumes = ([identity['volume'], identity['shared_volume']]
                       + identity.get('never_touch_volumes', [])
                       if route == 'acow' else [CBS_VOLUME])
        self.volumes = set(volumes)
        self.fail = fail or {}
        self.fail_after = fail_after or {}
        self.on_up = on_up
        self.calls = []
        self.cwd = checkout_of(route)

    def __call__(self, argv, label, cwd):
        short = label.split('-', 1)[1] if label[:3].isdigit() and label[3:4] == '-' else label
        self.calls.append((short, list(argv[1:])))
        if str(cwd) != self.cwd:
            raise AssertionError('child not run from the pinned checkout: %r' % (cwd,))
        if short in self.fail:
            raise core.Refused(self.fail[short], short)
        text = self._output(argv, label)
        if short in self.fail_after:
            raise core.Refused(self.fail_after[short], short)
        self.children.mkdir(parents=True, exist_ok=True)
        path = self.children / (label + '.stdout')
        path.write_text(text)
        return {'label': label, 'exit': 0, 'cwd': cwd,
                'streams': {'stdout': {'path': str(path)}}}

    def _output(self, argv, label):
        if argv[1] == 'container':
            return self._effigy(argv[1:])
        rest = argv[argv.index('--') + 1:]
        verb = rest[0]
        if verb == 'ps':
            filters = [rest[i + 1] for i, item in enumerate(rest) if item == '--filter']
            rows = []
            for box in self.containers:
                if not filters or all(self._match(box, f) for f in filters):
                    rows.append(box['id'])
            return '\n'.join(sorted(rows)) + ('\n' if rows else '')
        if verb == 'volume':
            return '\n'.join(sorted(self.volumes)) + '\n'
        if verb == 'inspect':
            box = next(b for b in self.containers if b['id'] == rest[-1])
            return ''.join('volume %s\n' % name for name in box['mounts'])
        if verb == 'exec':
            name = rest[-1].split(' FROM ', 1)[1].replace('`', '')
            return str(self.counts[name]) + '\n'
        raise AssertionError('unexpected engine verb ' + verb)

    @staticmethod
    def _match(box, flt):
        if flt.startswith('label=com.docker.compose.project='):
            return box['project'] == flt.split('=', 2)[2]
        if flt.startswith('label=com.docker.compose.service='):
            return box['service'] == flt.split('=', 2)[2]
        return False

    def _effigy(self, tail):
        env = tail[tail.index('--system') + 1] if '--system' in tail else tail[1]
        if 'status' in tail:
            return json.dumps({'running': self.running[env]}) + '\n'
        if 'up' in tail:
            self.running[env] = True
            if self.on_up:
                self.on_up(self)
        if 'down' in tail:
            self.running[env] = False
        return ''

    def labels(self):
        return [label for label, _ in self.calls]

    def ups(self):
        return [label for label in self.labels() if label.endswith('-up')]


def forbidden_transport(argv, label, cwd):
    raise AssertionError('a child ran on a path that must refuse first: ' + label)


def run_entry(module, route, request, name, *, engine=None, git=None, manifest=None,
              now=NOW, sha=None):
    """Drive `run` with fixture transport and fixture git. `sha` overrides the bound digest."""
    folder = ROOT / name
    folder.mkdir(parents=True, exist_ok=True)
    manifest = manifest if manifest is not None else fixture_manifest()
    raw = json.dumps(request, sort_keys=True).encode()
    request_path = folder / 'request.json'
    request_path.write_bytes(raw)
    digest = core.sha256_bytes(raw)
    evidence = folder / 'evidence'
    result = module.run(str(request_path), digest if sha is None else sha, str(evidence),
                        transport=engine or forbidden_transport, manifest=manifest,
                        git=git or fake_git(manifest, route), now_ns=now,
                        roots=(str(ROOT),))
    return result, evidence


def refusal_run(module, route, request, name, code, **kwargs):
    """The refusal must come from the named gate.

    A refusal before the evidence claim leaves no evidence. A refusal after it
    spends the occurrence and records the same code, so a retry cannot reuse it.
    """
    expect_code(code, lambda: run_entry(module, route, request, name, **kwargs))
    evidence = ROOT / name / 'evidence'
    if evidence.exists():
        occurrence = json.loads((evidence / 'occurrence.json').read_text())
        assert occurrence['status'] == code and occurrence['spent'] is True, occurrence


def mutated(route, prior_intent, edit):
    request = request_for(fixture_manifest(), route, prior_intent)
    edit(request)
    return request


def load_historical():
    """Test-only: the reviewed Acow r1 readers, loaded for reconciliation only."""
    path = DIR.parent / 'acow-ordinary-compose-5f466914' / 'common.py'
    spec = importlib.util.spec_from_file_location('acow_r1_common_reconcile', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def no_child(*args, **kwargs):
    raise AssertionError('a child process was started')


def stop_recorded(pid):
    """Stop one fixture process by the PID this test recorded. Never a pattern."""
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def wait_gone(pid, seconds=3.0):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return True
        time.sleep(0.05)
    return False


# ---------------------------------------------------------------- committed state


@check('committed routes refuse for pending route pins before any request is read')
def _():
    manifest = core.load_manifest()
    for route in ('acow', 'cbs'):
        expect_code('route-pins-pending', lambda: core.require_route_frozen(manifest, route))


@check('fixture manifest is fully concrete, so only the control under test refuses')
def _():
    assert core.pending_pins(fixture_manifest()) == []


@check('unarmed CLIs refuse every ordinary invocation before any child or read')
def _():
    original = subprocess.Popen
    subprocess.Popen = no_child
    try:
        for module in (acow, cbs):
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                assert module.main([]) == 2
            assert json.loads(out.getvalue())['code'] == 'unarmed'
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            assert proof_entry.main([]) == 2
        assert json.loads(out.getvalue())['code'] == 'unarmed'
        expect_code('unarmed', lambda: proof_entry.run(['acow', '--request', 'x']))
    finally:
        subprocess.Popen = original


@check('committed proof entry refuses a well-formed request at route-pins-pending')
def _():
    folder = ROOT / 'committed'
    folder.mkdir(parents=True, exist_ok=True)
    raw = json.dumps(request_for(fixture_manifest(), 'acow',
                                 {'hybrid': True, 'stack': False})).encode()
    request_path = folder / 'request.json'
    request_path.write_bytes(raw)
    original = subprocess.Popen
    subprocess.Popen = no_child
    out = io.StringIO()
    try:
        with contextlib.redirect_stdout(out):
            assert proof_entry.main(['acow', '--request', str(request_path),
                                     '--sha256', core.sha256_bytes(raw),
                                     '--evidence-dir', str(folder / 'evidence')]) == 2
    finally:
        subprocess.Popen = original
    assert json.loads(out.getvalue())['code'] == 'route-pins-pending'
    assert not (folder / 'evidence').exists()


@check('committed Acow pins reconcile with the reviewed 5f466914 packet')
def _():
    hist = load_historical()
    pins = core.load_manifest()['routes']['acow']
    assert pins['consumer']['relevant_inputs'] == hist.RELEVANT_INPUT_BLOBS
    assert pins['consumer']['includes'] == hist.MANIFEST_INCLUDES
    for table, (value, _, _) in hist.EXPECTED_PRIMARY_COUNTS.items():
        assert pins['counts'][table]['baseline'] == value, table
    assert pins['identity']['project'] == hist.PRIMARY_PROJECT
    assert pins['identity']['volume'] == hist.PRIMARY_POSTGRES_VOLUME
    assert pins['identity']['shared_container'] == hist.SHARED_MARIADB_CONTAINER
    assert pins['identity']['shared_volume'] == hist.SHARED_MARIADB_VOLUME
    assert set(pins['identity']['never_touch_volumes']) == set(hist.NEVER_TOUCH_VOLUMES)


@check('CBS counts, identity and bundle digest are pending, not borrowed from Acow')
def _():
    pins = core.load_manifest()['routes']['cbs']
    assert pins['counts']['cbs']['user_client']['baseline'] == 'pending'
    assert pins['identity']['project'].startswith('pending')
    assert pins['consumer']['bundle']['content_digest'].startswith('pending')


# ---------------------------------------------------------------- request digest and bindings


@check('request bytes must match the SHA-256 argument before anything is read or run')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    refusal_run(acow, 'acow', request, 'digest-other', 'request-sha-mismatch', manifest=manifest,
                sha='sha256:' + '0' * 64, engine=forbidden_transport)
    refusal_run(acow, 'acow', request, 'digest-bare', 'request-sha-mismatch', manifest=manifest,
                sha='f' * 64, engine=forbidden_transport)
    expect_code('request-sha-mismatch', lambda: core.load_request(str(ROOT / 'digest-other' / 'request.json'), 'x'))


@check('a helper head that differs from the source head, or stale module bytes, refuses before claim')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['helpers'].update(head='a' * 40)),
                'helper-head', 'helper-mismatch', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['helpers'].update(
        modules={name: SHA for name in core.ENTRY_MODULES})), 'helper-modules',
        'helper-mismatch', engine=forbidden_transport)


@check('the packet checkout must be the request merged commit, observed after the claim')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', request_for(fixture_manifest(), 'acow', base, merged='b' * 40),
                'merged-substituted', 'provenance-mismatch', engine=forbidden_transport)
    refusal_run(acow, 'acow', request_for(fixture_manifest(), 'acow', base, merged='not-a-commit'),
                'merged-malformed', 'provenance-mismatch', engine=forbidden_transport)


@check('an installed candidate whose bytes differ from the request refuses before claim')
def _():
    base = {'hybrid': True, 'stack': False}
    wrong = 'sha256:' + '2' * 64
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['candidate'].update(
        binary_sha256=wrong, install_identity=dict(r['candidate']['install_identity'], sha256=wrong))),
        'candidate-bytes', 'candidate-mismatch', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['candidate']['install_identity'].update(
        sha256=wrong)), 'candidate-install-split', 'candidate-mismatch', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['candidate']['install_identity'].update(
        path='relative/effigy')), 'candidate-relative', 'candidate-mismatch', engine=forbidden_transport)


@check('stale positive624, a receipt for another candidate, and malformed receipts refuse before claim')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['predecessor'].update(
        same_candidate_commit='a' * 40)), 'other-c-receipt', 'predecessor-mismatch',
        engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['predecessor'].update(
        receipt_sha256='not-a-digest')), 'malformed-receipt', 'predecessor-mismatch',
        engine=forbidden_transport)


@check('window bounds, expiry, one-use and token digest refuse before claim')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['window'].update(
        deadline_monotonic_ns=START + 10_000 * NS)), 'window-span', 'window-bounds',
        engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['window'].update(
        token_sha256='not-a-digest')), 'window-token', 'window-missing', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['window'].update(
        start_monotonic_ns='soon')), 'window-type', 'window-missing', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: None), 'window-expired',
                'window-expired', now=START + 600 * NS + 1, engine=forbidden_transport)
    manifest = fixture_manifest()
    manifest['consumed_windows'] = [WINDOW_ID['acow']]
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: None), 'window-consumed',
                'window-consumed', manifest=manifest, engine=forbidden_transport)


@check('budgets above the route caps, and unequal stream caps, refuse before claim')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['budgets'].update(
        stdout_cap_bytes=10 ** 9, stderr_cap_bytes=10 ** 9)), 'budget', 'budget-exceeded',
        engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['budgets'].update(
        stderr_cap_bytes=1024)), 'budget-split', 'budget-exceeded', engine=forbidden_transport)


@check('resource roles and kinds must be the pinned set')
def _():
    refusal_run(acow, 'acow', mutated('acow', {'hybrid': True, 'stack': False},
                                      lambda r: r['resources'].pop()), 'resource-set',
                'resource-mismatch', engine=forbidden_transport)


@check('a symlinked consumer checkout refuses before claim')
def _():
    manifest = fixture_manifest()
    link = ROOT / 'checkouts' / 'acow-link'
    if not link.is_symlink():
        os.symlink(checkout_of('acow'), str(link))
    manifest['routes']['acow']['checkout'] = str(link)
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    refusal_run(acow, 'acow', request, 'checkout-link', 'checkout-mismatch',
                manifest=manifest, engine=forbidden_transport)


# ---------------------------------------------------------------- consumer closure


@check('include, closure and bundle drift refuse after claim, with the occurrence spent')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['consumer'].update(
        includes=['infra/workspace.toml'])), 'includes', 'input-drift', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['consumer']['relevant_inputs']
                                      .update({'effigy.toml': '0' * 40})), 'closure',
                'input-drift', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['consumer']['bundle'].update(
        content_digest=SHA2)), 'bundle', 'bundle-drift', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['consumer']['bundle'].update(
        resolved_revision='7' * 40)), 'bundle-revision', 'bundle-drift', engine=forbidden_transport)


@check('the checkout must be at the selected head, clean, with every blob at that head')
def _():
    base = {'hybrid': True, 'stack': False}
    manifest = fixture_manifest()
    refusal_run(acow, 'acow', request_for(manifest, 'acow', base), 'other-head',
                'consumer-mismatch', manifest=manifest, engine=forbidden_transport,
                git=fake_git(manifest, 'acow', head='2' * 40))
    refusal_run(acow, 'acow', request_for(manifest, 'acow', base), 'dirty',
                'consumer-dirty', manifest=manifest, engine=forbidden_transport,
                git=fake_git(manifest, 'acow', status=' M effigy.toml\n'))
    refusal_run(acow, 'acow', request_for(manifest, 'acow', base), 'blob-drift',
                'input-drift', manifest=manifest, engine=forbidden_transport,
                git=fake_git(manifest, 'acow', blobs={'effigy.toml': '5' * 40}))


@check('git reads are an allowlist of object ids and status, never content')
def _():
    expect_code('git-not-allowlisted', lambda: core.check_git_args(['show', 'HEAD:effigy.toml']))
    expect_code('git-not-allowlisted', lambda: core.check_git_args(['cat-file', '-p', 'HEAD']))
    expect_code('git-not-allowlisted', lambda: core.check_git_args(['rev-parse', 'HEAD~1']))
    core.check_git_args(['rev-parse', 'HEAD:infra/workspace.toml'])


# ---------------------------------------------------------------- engine identity and protection


@check('a foreign or replaced container refuses before any lifecycle effect')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    engine = FakeEngine('acow', ROOT / 'foreign' / 'children', {'hybrid': False, 'stack': False},
                        containers=[{'id': 'a' * 64, 'project': 'acowtancy-dev',
                                     'service': 'postgres', 'mounts': ['acowtancy-dev-postgres-data']}])
    refusal_run(acow, 'acow', request, 'foreign', 'resource-mismatch', manifest=manifest,
                engine=engine)
    assert not engine.ups(), engine.labels()


@check('a container that does not mount the pinned volume refuses before effect')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    engine = FakeEngine('acow', ROOT / 'nomount' / 'children', {'hybrid': False, 'stack': False},
                        containers=[{'id': PG_ID, 'project': 'acowtancy-dev',
                                     'service': 'postgres', 'mounts': ['other-volume']},
                                    {'id': manifest['routes']['acow']['identity']['shared_container'],
                                     'project': None, 'service': None, 'mounts': []}])
    refusal_run(acow, 'acow', request, 'nomount', 'resource-mismatch', manifest=manifest,
                engine=engine)
    assert not engine.ups()


@check('a request volume that is not the pinned volume refuses before effect')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    request['resources'][1]['identity'] = 'some-other-volume'
    engine = FakeEngine('acow', ROOT / 'vol-id' / 'children', {'hybrid': False, 'stack': False})
    refusal_run(acow, 'acow', request, 'vol-id', 'resource-mismatch', manifest=manifest,
                engine=engine)
    assert not engine.ups()


@check('shared MariaDB absent before effect refuses; a lost named volume after refuses')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    identity = manifest['routes']['acow']['identity']
    absent = FakeEngine('acow', ROOT / 'no-shared' / 'children', {'hybrid': False, 'stack': False},
                        volumes=[identity['volume']])
    refusal_run(acow, 'acow', request, 'no-shared', 'protected-missing', manifest=manifest,
                engine=absent)
    assert not absent.ups()

    def drop_silo(engine):
        engine.volumes.discard('acowtancy-dev-silo-data')

    lost = FakeEngine('acow', ROOT / 'lost' / 'children', {'hybrid': False, 'stack': False},
                      on_up=drop_silo)
    lost_request = request_for(manifest, 'acow', {'hybrid': False, 'stack': False})
    refusal_run(acow, 'acow', lost_request, 'lost', 'protected-identity-changed',
                manifest=manifest, engine=lost)


# ---------------------------------------------------------------- prior intent and rollback


@check('prior intent must name exactly the environments the route starts')
def _():
    manifest = fixture_manifest()
    partial = request_for(manifest, 'acow', {'stack': False})
    engine = FakeEngine('acow', ROOT / 'partial' / 'children', {'hybrid': True, 'stack': False})
    refusal_run(acow, 'acow', partial, 'partial-intent', 'prior-intent', manifest=manifest,
                engine=engine)
    assert not engine.labels(), engine.labels()


@check('prior running state that drifts from the request refuses before any bring-up')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    engine = FakeEngine('acow', ROOT / 'drift' / 'children', {'hybrid': True, 'stack': True})
    refusal_run(acow, 'acow', request, 'drift', 'prior-intent', manifest=manifest, engine=engine)
    assert not engine.ups()


@check('CBS prior intent must name exactly the dev system')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'cbs', {'dev': False, 'hybrid': True})
    engine = FakeEngine('cbs', ROOT / 'cbs-intent' / 'children', {'dev': False})
    refusal_run(cbs, 'cbs', request, 'cbs-intent', 'prior-intent', manifest=manifest,
                engine=engine)
    assert not engine.ups()


@check('unexpected counts refuse and roll back the environments this window started')
def _():
    manifest = fixture_manifest()
    counts = acow_counts()
    counts['auth.users'] = 99
    engine = FakeEngine('acow', ROOT / 'counts' / 'children',
                        {'hybrid': False, 'stack': False}, counts=counts)
    request = request_for(manifest, 'acow', {'hybrid': False, 'stack': False})
    refusal_run(acow, 'acow', request, 'counts', 'unexpected-counts', manifest=manifest,
                engine=engine)
    assert engine.running == {'hybrid': False, 'stack': False}, engine.running
    occurrence = json.loads((ROOT / 'counts' / 'evidence' / 'occurrence.json').read_text())
    assert occurrence['status'] == 'unexpected-counts' and occurrence['spent'] is True


@check('a failed bring-up refuses and leaves the prior-running environment running')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    engine = FakeEngine('acow', ROOT / 'up-fail' / 'children',
                        {'hybrid': True, 'stack': False}, fail={'hybrid-1-up': 'child-failed'})
    refusal_run(acow, 'acow', request, 'up-fail', 'child-failed', manifest=manifest,
                engine=engine)
    assert engine.running['hybrid'] is True


@check('an interrupted rollback refuses as a spent occurrence with evidence kept')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'cbs', {'dev': False})
    engine = FakeEngine('cbs', ROOT / 'rb' / 'children', {'dev': False},
                        fail={'rollback-dev': 'child-failed'})
    refusal_run(cbs, 'cbs', request, 'rb', 'child-failed', manifest=manifest, engine=engine)
    occurrence = json.loads((ROOT / 'rb' / 'evidence' / 'occurrence.json').read_text())
    assert occurrence['spent'] is True
    assert (ROOT / 'rb' / 'evidence' / 'binding.json').exists()


@check('acow: a failed up that partly started stack is rolled back, not left running')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    engine = FakeEngine('acow', ROOT / 'acow-up-partial' / 'children',
                        {'hybrid': True, 'stack': False},
                        fail_after={'container-up': 'child-failed'})
    refusal_run(acow, 'acow', request, 'acow-up-partial', 'child-failed', manifest=manifest,
                engine=engine)
    assert engine.running == {'hybrid': True, 'stack': False}, engine.running
    assert 'rollback-stack' in engine.labels() and 'rollback-hybrid' not in engine.labels()


@check('acow: a stack status read that fails after its up is rolled back, not left running')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    engine = FakeEngine('acow', ROOT / 'acow-status-fail' / 'children',
                        {'hybrid': True, 'stack': False},
                        fail_after={'container-status': 'child-failed'})
    refusal_run(acow, 'acow', request, 'acow-status-fail', 'child-failed', manifest=manifest,
                engine=engine)
    assert engine.running == {'hybrid': True, 'stack': False}, engine.running
    assert 'rollback-stack' in engine.labels()


@check('acow: a failed second hybrid up rolls back every environment the window started')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': False, 'stack': False})
    engine = FakeEngine('acow', ROOT / 'acow-h2-fail' / 'children',
                        {'hybrid': False, 'stack': False},
                        fail_after={'hybrid-2-up': 'child-failed'})
    refusal_run(acow, 'acow', request, 'acow-h2-fail', 'child-failed', manifest=manifest,
                engine=engine)
    assert engine.running == {'hybrid': False, 'stack': False}, engine.running
    assert {'rollback-hybrid', 'rollback-stack'} <= set(engine.labels()), engine.labels()


@check('cbs: a failed app-shell up that partly started the system is rolled back')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'cbs', {'dev': False})
    engine = FakeEngine('cbs', ROOT / 'cbs-up-partial' / 'children', {'dev': False},
                        fail_after={'app-shell-up': 'child-failed'})
    refusal_run(cbs, 'cbs', request, 'cbs-up-partial', 'child-failed', manifest=manifest,
                engine=engine)
    assert engine.running == {'dev': False}, engine.running
    assert 'rollback-dev' in engine.labels()


@check('cbs: a failed app-shell status read after its up is rolled back')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'cbs', {'dev': False})
    engine = FakeEngine('cbs', ROOT / 'cbs-status-fail' / 'children', {'dev': False},
                        fail_after={'app-shell-status': 'child-failed'})
    refusal_run(cbs, 'cbs', request, 'cbs-status-fail', 'child-failed', manifest=manifest,
                engine=engine)
    assert engine.running == {'dev': False}, engine.running
    assert 'rollback-dev' in engine.labels()


@check('a second run into an existing evidence directory refuses and keeps the first record')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'cbs', {'dev': False})
    first = FakeEngine('cbs', ROOT / 'twice' / 'children', {'dev': False})
    run_entry(cbs, 'cbs', request, 'twice', engine=first, manifest=manifest)
    before = (ROOT / 'twice' / 'evidence' / 'occurrence.json').read_text()
    second = FakeEngine('cbs', ROOT / 'twice-2' / 'children', {'dev': False})
    raw = (ROOT / 'twice' / 'request.json').read_bytes()
    expect_code('evidence-exists', lambda: cbs.run(
        str(ROOT / 'twice' / 'request.json'), core.sha256_bytes(raw),
        str(ROOT / 'twice' / 'evidence'), transport=second, manifest=manifest,
        git=fake_git(manifest, 'cbs'), now_ns=NOW, roots=(str(ROOT),)))
    assert (ROOT / 'twice' / 'evidence' / 'occurrence.json').read_text() == before


# ---------------------------------------------------------------- allowlist and cwd


@check('the session refuses any tail or engine verb outside its allowlist')
def _():
    session = core.Session(forbidden_transport, '/fixture/bin/effigy', str(ROOT),
                           acow.TAILS, {'colima_binary': '/opt/homebrew/bin/colima',
                                        'profile': 'effigy'})
    expect_code('argv-not-allowlisted', lambda: session.effigy(
        ('container', 'stack', 'shell', '--command', 'id'), 'x'))
    expect_code('argv-not-allowlisted', lambda: session.engine_call(
        core.engine_argv({'colima_binary': '/opt/homebrew/bin/colima', 'profile': 'effigy'},
                         'run', 'x'), 'x'))
    expect_code('argv-not-pinned', lambda: core.Session(forbidden_transport, 'effigy',
                                                        str(ROOT), acow.TAILS, {}))


@check('static route tails never name dev:test, seed, deploy, sync, schema or prune')
def _():
    forbidden = re.compile(r'dev:test|seed|deploy|sync|schema|prune|--global')
    for tail in list(acow.TAILS) + list(cbs.TAILS):
        assert not forbidden.search(' '.join(tail)), tail


# ---------------------------------------------------------------- supervision, real children


def supervisor(name, *, cap=262144, reserve=core.RESERVE_S, seconds=60, step=900):
    budgets = {'step_timeout_s': step, 'stdout_cap_bytes': cap, 'stderr_cap_bytes': cap}
    end = time.monotonic_ns() + seconds * NS
    return core.Supervisor(end, budgets, ROOT / 'sup' / name, reserve_s=reserve)


def py(code):
    return [sys.executable, '-I', '-c', code]


@check('the pinned checkout reaches a supervised child as its working directory')
def _():
    sup = supervisor('cwd')
    (ROOT / 'sup').mkdir(exist_ok=True)
    rec = sup.child(py('import os; print(os.getcwd())'), 'cwd-probe', checkout_of('acow'))
    assert Path(rec['streams']['stdout']['path']).read_text().strip() == checkout_of('acow')


@check('the supervisor keeps stdout and stderr separate, with hashes and a positive waited exit')
def _():
    (ROOT / 'sup').mkdir(exist_ok=True)
    sup = supervisor('split')
    rec = sup.child(py('import sys; sys.stdout.write("out-bytes"); sys.stderr.write("err-bytes")'),
                    'split', str(ROOT))
    assert rec['disposition'] == 'exited' and rec['waited_exit'] == 0, rec
    assert Path(rec['streams']['stdout']['path']).read_bytes() == b'out-bytes'
    assert Path(rec['streams']['stderr']['path']).read_bytes() == b'err-bytes'
    for name in ('stdout', 'stderr'):
        stream = rec['streams'][name]
        assert stream['sha256'] == hashlib.sha256(Path(stream['path']).read_bytes()).hexdigest(), name


@check('over-cap output refuses with the capped bytes and the receipt kept')
def _():
    (ROOT / 'sup').mkdir(exist_ok=True)
    sup = supervisor('overcap', cap=100)
    expect_code('over-cap-stream', lambda: sup.child(py('import sys; sys.stdout.write("x" * 5000)'),
                                                     'overcap', str(ROOT)))
    receipt = json.loads((ROOT / 'sup' / 'overcap' / 'overcap' / 'receipt.json').read_text())
    assert receipt['disposition'] == 'output-cap'
    assert receipt['streams']['stdout']['bytes'] == 100, receipt['streams']


@check('a zero stream cap refuses before any child starts, and leaves no intent')
def _():
    (ROOT / 'sup').mkdir(exist_ok=True)
    sup = supervisor('zerocap', cap=0)
    expect_code('supervision-refused', lambda: sup.child(py('print("never")'), 'zero', str(ROOT)))
    assert not (ROOT / 'sup' / 'zerocap' / 'zero' / 'intent.json').exists()
    assert not (ROOT / 'sup' / 'zerocap' / 'zero' / 'stdout').exists()


@check('a blocked child is stopped at the window deadline and its exit is waited')
def _():
    (ROOT / 'sup').mkdir(exist_ok=True)
    sup = supervisor('blocked', reserve=1, seconds=4)
    started = time.monotonic()
    expect_code('deadline-exceeded', lambda: sup.child(py('import time; time.sleep(60)'),
                                                       'blocked', str(ROOT)))
    assert time.monotonic() - started < 15
    receipt = json.loads((ROOT / 'sup' / 'blocked' / 'blocked' / 'receipt.json').read_text())
    assert receipt['disposition'] == 'timeout' and receipt['waited_exit'] is not None, receipt


@check('a child that exits nonzero refuses as child-failed with its exit recorded')
def _():
    (ROOT / 'sup').mkdir(exist_ok=True)
    sup = supervisor('failed')
    expect_code('child-failed', lambda: sup.child(py('import sys; sys.exit(3)'), 'fail', str(ROOT)))
    receipt = json.loads((ROOT / 'sup' / 'failed' / 'fail' / 'receipt.json').read_text())
    assert receipt['waited_exit'] == 3


@check('the supervisor samples resources while a child is blocked, not only at exit')
def _():
    (ROOT / 'sup').mkdir(exist_ok=True)
    sup = supervisor('sample')
    rec = sup.child(py('import time; time.sleep(1)'), 'sample', str(ROOT))
    usage = rec.get('resource_usage') or {}
    assert usage.get('window_ms', 0) >= 500, usage


@check('an escaping descendant holding the inherited pipe refuses, and is stopped by its recorded PID')
def _():
    (ROOT / 'sup').mkdir(exist_ok=True)
    sup = supervisor('escape', reserve=1, seconds=4)
    code = ('import subprocess, sys\n'
            'g = subprocess.Popen([sys.executable, "-I", "-c", "import time; time.sleep(60)"], '
            'start_new_session=True)\n'
            'print(g.pid, flush=True)\n')
    pid = None
    try:
        expect_code('deadline-exceeded', lambda: sup.child(py(code), 'escape', str(ROOT)))
        receipt = json.loads((ROOT / 'sup' / 'escape' / 'escape' / 'receipt.json').read_text())
        assert receipt['disposition'] == 'timeout', receipt
        pid = int((ROOT / 'sup' / 'escape' / 'escape' / 'stdout').read_text().split()[0])
    finally:
        if pid is not None:
            stop_recorded(pid)
            assert wait_gone(pid), 'fixture descendant survived its recorded stop'


@check('a same-group descendant holding the pipe is stopped with the group and the window refuses')
def _():
    (ROOT / 'sup').mkdir(exist_ok=True)
    sup = supervisor('group', reserve=1, seconds=4)
    code = ('import subprocess, sys\n'
            'g = subprocess.Popen([sys.executable, "-I", "-c", "import time; time.sleep(60)"])\n'
            'print(g.pid, flush=True)\n')
    pid = None
    try:
        expect_code('deadline-exceeded', lambda: sup.child(py(code), 'group', str(ROOT)))
        receipt = json.loads((ROOT / 'sup' / 'group' / 'group' / 'receipt.json').read_text())
        assert receipt['disposition'] == 'timeout', receipt
        pid = int((ROOT / 'sup' / 'group' / 'group' / 'stdout').read_text().split()[0])
    finally:
        if pid is not None:
            stop_recorded(pid)
            assert wait_gone(pid), 'fixture descendant survived its recorded stop'


@check('every non-waited-zero disposition maps to a refusal code, and only exited passes')
def _():
    def rec(disposition, exit_code=0):
        return {'label': 'x', 'disposition': disposition, 'waited_exit': exit_code}
    core.disposition_refusal(rec('exited', 0))
    for disposition, code in (('failed', 'child-failed'), ('timeout', 'deadline-exceeded'),
                              ('output-cap', 'over-cap-stream'),
                              ('resource-refusal', 'resource-refusal'),
                              ('unknown', 'child-unreaped'), ('local-error', 'child-unreaped'),
                              ('context-output-refused', 'child-unreaped')):
        expect_code(code, lambda d=disposition: core.disposition_refusal(rec(d, None)))
    expect_code('child-unreaped', lambda: core.disposition_refusal(rec('exited', None)))


# ---------------------------------------------------------------- end to end, real supervisor


FIXTURE_ENGINE = '''#!PYTHON -I
"""Harmless fixture engine. Serves the effigy and colima surfaces from state.json."""
import json
import sys
from pathlib import Path

STATE = Path(__file__).with_name('state.json')
state = json.loads(STATE.read_text())
argv = sys.argv[1:]


def save():
    STATE.write_text(json.dumps(state, sort_keys=True))


def match(box, flt):
    if flt.startswith('label=com.docker.compose.project='):
        return box['project'] == flt.split('=', 2)[2]
    if flt.startswith('label=com.docker.compose.service='):
        return box['service'] == flt.split('=', 2)[2]
    return False


if argv[:1] == ['container']:
    env = argv[argv.index('--system') + 1] if '--system' in argv else argv[1]
    if 'status' in argv:
        print(json.dumps({'running': state['running'][env]}))
    elif 'up' in argv:
        state['running'][env] = True
        save()
    elif 'down' in argv:
        state['running'][env] = False
        save()
    sys.exit(0)
rest = argv[argv.index('--') + 1:]
verb = rest[0]
if verb == 'ps':
    filters = [rest[i + 1] for i, item in enumerate(rest) if item == '--filter']
    rows = [b['id'] for b in state['containers'] if all(match(b, f) for f in filters)]
    print('\\n'.join(sorted(rows)))
elif verb == 'volume':
    print('\\n'.join(sorted(state['volumes'])))
elif verb == 'inspect':
    box = next(b for b in state['containers'] if b['id'] == rest[-1])
    print('\\n'.join('volume ' + name for name in box['mounts']))
elif verb == 'exec':
    table = rest[-1].split(' FROM ', 1)[1].replace('`', '').strip()
    print(state['counts'][table])
else:
    sys.exit(2)
'''


def e2e_fixture(route, name, manifest, prior_intent):
    """A real git checkout at the reviewed closure, and the fixture engine bound to state."""
    base = ROOT / name
    checkout = base / 'checkout'
    checkout.mkdir(parents=True, exist_ok=True)
    env = {'GIT_CONFIG_NOSYSTEM': '1', 'HOME': str(base), 'PATH': os.environ.get('PATH', '')}
    run_git = lambda *args: subprocess.run(  # noqa: E731
        ['/usr/bin/git', '-C', str(checkout), *args], capture_output=True, text=True,
        check=True, env=env).stdout
    run_git('init', '-q')
    for path in manifest['routes'][route]['consumer']['relevant_inputs']:
        target = checkout / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text('fixture closure input ' + path + '\n')
    run_git('add', '-A')
    run_git('-c', 'user.name=fixture', '-c', 'user.email=fixture@example.invalid',
            '-c', 'commit.gpgsign=false', 'commit', '-q', '-m', 'fixture closure')
    head = run_git('rev-parse', 'HEAD').strip()
    blobs = {path: run_git('rev-parse', 'HEAD:' + path).strip()
             for path in manifest['routes'][route]['consumer']['relevant_inputs']}
    pins = manifest['routes'][route]
    pins['checkout'] = str(checkout)
    pins['consumer']['relevant_inputs'] = blobs
    script = base / 'bin' / 'effigy'
    script.parent.mkdir(parents=True, exist_ok=True)
    script.write_text(FIXTURE_ENGINE.replace('PYTHON', sys.executable))
    script.chmod(0o755)
    pins['engine']['colima_binary'] = str(script)
    counts = (acow if route == 'acow' else cbs).expected_counts(manifest['routes'][route])
    if route == 'acow':
        identity = pins['identity']
        containers = [
            {'id': PG_ID, 'project': identity['project'], 'service': identity['service'],
             'mounts': [identity['volume']]},
            {'id': identity['shared_container'], 'project': None, 'service': None, 'mounts': []},
        ]
        volumes = sorted(set([identity['volume'], identity['shared_volume']]
                             + identity['never_touch_volumes']))
    else:
        identity = pins['identity']
        containers = [{'id': DB_ID, 'project': identity['project'], 'service': identity['service'],
                       'mounts': [CBS_VOLUME]}]
        volumes = [CBS_VOLUME]
    state = {'running': dict(prior_intent), 'counts': counts,
             'containers': containers, 'volumes': volumes}
    (base / 'bin' / 'state.json').write_text(json.dumps(state, sort_keys=True))
    return checkout, head, script


def e2e_request(route, manifest, prior_intent, head, script):
    request = request_for(manifest, route, prior_intent,
                          merged=packet_head(), helper_head=packet_head())
    request['consumer']['selected_head'] = head
    request['consumer']['relevant_inputs'] = copy.deepcopy(
        manifest['routes'][route]['consumer']['relevant_inputs'])
    now = time.monotonic_ns()
    request['window'].update(start_monotonic_ns=now, deadline_monotonic_ns=now + 600 * NS)
    digest = core.sha256_file(script)
    request['candidate'].update(binary_sha256=digest)
    request['candidate']['install_identity'].update(path=str(script), sha256=digest)
    return request


def packet_head():
    """The real HEAD of the packet's own repository: the merged commit a launch would pin."""
    return subprocess.run(['/usr/bin/git', 'rev-parse', 'HEAD'], cwd=str(DIR),
                          capture_output=True, text=True, check=True).stdout.strip()


def e2e_run(route, name, prior_intent):
    manifest = fixture_manifest()
    checkout, head, script = e2e_fixture(route, name, manifest, prior_intent)
    request = e2e_request(route, manifest, prior_intent, head, script)
    folder = ROOT / name
    folder.mkdir(parents=True, exist_ok=True)
    raw = json.dumps(request, sort_keys=True).encode()
    request_path = folder / 'request.json'
    request_path.write_bytes(raw)
    evidence = folder / 'evidence'
    result = proof_entry.run([route, '--request', str(request_path), '--sha256',
                              core.sha256_bytes(raw), '--evidence-dir', str(evidence)],
                             manifest=manifest, roots=(str(ROOT),))
    state = json.loads((script.parent / 'state.json').read_text())
    return result, evidence, state


@check('end to end Acow: a fully bound occurrence reaches the real supervisor and settles')
def _():
    result, evidence, state = e2e_run('acow', 'e2e-acow', {'hybrid': False, 'stack': False})
    assert result['status'] == 'passed' and result['started'] == ['hybrid', 'stack'], result
    assert state['running'] == {'hybrid': False, 'stack': False}, state
    occurrence = json.loads((evidence / 'occurrence.json').read_text())
    assert occurrence['status'] == 'passed' and occurrence['spent'] is False
    children = sorted(p.name for p in (evidence / 'children').iterdir())
    assert any(name.startswith('git-') for name in children), children
    assert any(name.endswith('-hybrid-1-up') for name in children), children
    for child in children:
        receipt = json.loads((evidence / 'children' / child / 'receipt.json').read_text())
        assert receipt['disposition'] == 'exited' and receipt['waited_exit'] == 0, (child, receipt)
    closure = json.loads((evidence / 'closure.json').read_text())
    assert len(closure['reads']) >= 2 and all('stdout' in read for read in closure['reads'])


@check('end to end CBS: detached app-shell, forward, counts and scoped rollback reach the supervisor')
def _():
    result, evidence, state = e2e_run('cbs', 'e2e-cbs', {'dev': False})
    assert result['status'] == 'passed' and result['started'] == ['dev'], result
    assert state['running'] == {'dev': False}, state
    assert (evidence / 'binding.json').exists() and (evidence / 'closure.json').exists()


@check('end to end: the packet only runs through the fixed proof entry, with the request digest bound')
def _():
    manifest = fixture_manifest()
    checkout, head, script = e2e_fixture('cbs', 'e2e-digest', manifest, {'dev': False})
    request = e2e_request('cbs', manifest, {'dev': False}, head, script)
    folder = ROOT / 'e2e-digest'
    folder.mkdir(parents=True, exist_ok=True)
    raw = json.dumps(request, sort_keys=True).encode()
    request_path = folder / 'request.json'
    request_path.write_bytes(raw)
    expect_code('request-sha-mismatch', lambda: proof_entry.run(
        ['cbs', '--request', str(request_path), '--sha256', 'sha256:' + '0' * 64,
         '--evidence-dir', str(folder / 'evidence')], manifest=manifest, roots=(str(ROOT),)))
    assert not (folder / 'evidence').exists()


# ---------------------------------------------------------------- evidence and request files


@check('evidence roots refuse symlinks, relative paths and outside directories')
def _():
    link = ROOT / 'linkroot'
    if not link.exists():
        os.symlink(str(ROOT / 'committed'), str(link))
    expect_code('evidence-root', lambda: core.within_root(str(link / 'x'), (str(ROOT),)))
    expect_code('evidence-root', lambda: core.within_root('relative/dir', (str(ROOT),)))
    expect_code('evidence-root', lambda: core.within_root('/elsewhere/dir', (str(ROOT),)))


@check('malformed request files refuse: duplicate keys, extra keys, oversized')
def _():
    dup = ROOT / 'dup' / 'request.json'
    dup.parent.mkdir(parents=True, exist_ok=True)
    dup.write_text('{"schema": "a", "schema": "b"}')
    expect_code('duplicate-key', lambda: core.load_request(str(dup), core.sha256_file(dup)))
    big = ROOT / 'big' / 'request.json'
    big.parent.mkdir(parents=True, exist_ok=True)
    big.write_text('x' * (core.MAX_REQUEST_BYTES + 1))
    expect_code('file-too-large', lambda: core.load_request(str(big), core.sha256_file(big)))
    request = request_for(fixture_manifest(), 'acow', {'hybrid': True, 'stack': False})
    request['extra'] = 1
    expect_code('request-shape', lambda: core.validate_request_shape(request, 'acow'))


# ---------------------------------------------------------------- acow battery harness


class _FakeModule:
    def __init__(self, scratch):
        self.SCRATCH = scratch


@check('acow battery harness refuses without a usable TMPDIR and never falls back')
def _():
    builds = Path('/Volumes/Builds/scratch/effigy/540')
    before = builds.exists()
    result = subprocess.run(
        [sys.executable, '-I', '-B', str(Path(acow_battery.__file__))],
        env={'PATH': os.environ.get('PATH', '')}, capture_output=True, text=True, timeout=120)
    assert result.returncode == 2, result.returncode
    assert 'TMPDIR' in result.stderr
    assert builds.exists() == before, 'refused run touched the Builds scratch'
    for bad in ('', 'relative/dir'):
        expect_code_plain = False
        try:
            acow_battery.tmp_root_from_env({'TMPDIR': bad})
        except RuntimeError:
            expect_code_plain = True
        assert expect_code_plain, 'accepted TMPDIR %r' % bad
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
               _FakeModule(original))
    root = Path(os.path.realpath(ROOT / 'battery-tmp'))
    root.mkdir(parents=True, exist_ok=True)
    with acow_battery.relocated_scratch(modules, str(root)) as fresh:
        for module in modules:
            assert module.SCRATCH == fresh
        assert fresh.parent == root
        assert oct(fresh.stat().st_mode & 0o777) == '0o700'
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


# ------------------------------------------------- ordinary acquisition (effigy#785, #790)

ACQ_NAMES = {'legacy': 'acquisition-legacy', 'modern': 'acquisition-modern'}
LEG_PG = 'a1' * 32
LEG_SVC = 'legacy-db'
LEG_VOL = 'legacy-pg-volume'
# Each pass runs its own pinned ordinary environment. The fixture pins legacy to
# `dev` and modern to `stack` to prove the route maps each consumer to its own
# pinned name; the fixture claims nothing about the real legacy composition.
ACQ_ENV = {'legacy': 'dev', 'modern': 'stack'}
ACQ_VERSION = 'v0.15.0+local.ccccccc'
ACQ_PAYLOAD = b'fixture candidate bytes for acquisition\n'
ARTIFACT_BYTES = b'fixture linux artifact bytes\n'
for _name in ACQ_NAMES.values():
    (ROOT / 'checkouts' / _name).mkdir(parents=True, exist_ok=True)


def uuid_like():
    return hashlib.sha256(os.urandom(16)).hexdigest()[:16]


def producer_script(base=None):
    """A producer checkout instance: the pinned install at its provenance shape."""
    if base is None:
        base = ROOT / 'producers' / uuid_like()
    script = base / '.local-install' / 'bin' / 'effigy'
    script.parent.mkdir(parents=True, exist_ok=True)
    script.write_bytes(ACQ_PAYLOAD)
    script.with_suffix('.active-version').write_text(ACQ_VERSION + '\n')
    for rel in ('effigy.toml', 'config/tasks.toml', 'config/containers.toml'):
        target = base / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text('fixture producer checkout shape: ' + rel + '\n')
    return script


def producer_identity(script):
    """The install identity production computes for this anchor (§8.9)."""
    stat = Path(script).stat()
    return 'local:%s:%d:%d' % (ACQ_VERSION, stat.st_size, stat.st_mtime_ns)


def acquisition_manifest():
    manifest = fixture_manifest()
    route = manifest['routes'][acq.ROUTE]
    route['checkout'] = checkout_of(ACQ_NAMES['legacy'])
    route['environments'] = dict(ACQ_ENV)
    route['identity'].update(service=LEG_SVC, volume=LEG_VOL,
                             never_touch_volumes=[LEG_VOL, route['identity']['shared_volume']])
    route['consumer']['bundle']['approved_revision_prefix'] = 'd' * 7
    route['modern']['checkout'] = checkout_of(ACQ_NAMES['modern'])
    route['modern']['consumer']['bundle']['approved_revision_prefix'] = 'd' * 7
    return manifest


def consumer_block(pins, head):
    return {'selected_head': head,
            'relevant_inputs': copy.deepcopy(pins['relevant_inputs']),
            'includes': copy.deepcopy(pins['includes']),
            'bundle': {'resolved_revision': H, 'content_digest': pins['bundle']['content_digest']}}


def acquisition_request(manifest, prior, legacy_head=HEAD, modern_head=HEAD2, merged=M,
                        helper_head=H, script=None, version=ACQ_VERSION, identity=None,
                        artifact_sha256=None, producer_checkout=None):
    route = manifest['routes'][acq.ROUTE]
    if script is None:
        # Each control gets its own producer instance, so no control inherits
        # another control's producer output or anchor identity.
        script = producer_script()
    binary = str(script)
    digest = core.sha256_file(binary)
    return {
        'schema': core.REQUEST_SCHEMA,
        'route': acq.ROUTE,
        'candidate': {'source_commit': CAND, 'binary_sha256': digest,
                      'install_identity': {'path': binary, 'version': version, 'sha256': digest}},
        'helpers': {'head': helper_head, 'modules': core.module_hashes()},
        'predecessor': {'receipt_sha256': SHA2, 'same_candidate_commit': CAND},
        'consumer': consumer_block(route['consumer'], legacy_head),
        'modern_consumer': consumer_block(route['modern']['consumer'], modern_head),
        'producer': {'checkout': producer_checkout if producer_checkout is not None
                     else str(Path(binary).parent.parent.parent),
                     'artifact_sha256': artifact_sha256,
                     'install_identity': identity if identity is not None
                     else producer_identity(binary)},
        'provenance': {'merged_commit': merged, 'source_immutable_head': helper_head},
        'window': {'id': 'fixture-window-acquisition', 'token_sha256': TOKEN,
                   'start_monotonic_ns': START, 'deadline_monotonic_ns': START + 600 * NS},
        'budgets': copy.deepcopy(route['budgets']),
        'resources': [
            {'role': 'legacy-postgres-container', 'kind': 'container', 'identity': LEG_PG},
            {'role': 'legacy-postgres-volume', 'kind': 'volume', 'identity': LEG_VOL},
            {'role': 'postgres-container', 'kind': 'container', 'identity': PG_ID},
            {'role': 'postgres-volume', 'kind': 'volume',
             'identity': route['modern']['identity']['volume']},
        ],
        'prior_intent': copy.deepcopy(prior),
    }


ACQ_INTENT = {'legacy': {'dev': False}, 'modern': {'stack': False}}


def acquisition_git(manifest, calls=None, legacy_head=HEAD, modern_head=HEAD2, modern_status='',
                    modern_closure=None):
    route = manifest['routes'][acq.ROUTE]
    closures = {
        checkout_of(ACQ_NAMES['legacy']): (legacy_head, '', route['consumer']['relevant_inputs']),
        checkout_of(ACQ_NAMES['modern']): (modern_head, modern_status,
                                           modern_closure or route['modern']['consumer']['relevant_inputs']),
    }
    log = calls if calls is not None else []

    def git(args, cwd):
        log.append((list(args), str(cwd)))
        if str(cwd) == str(core.DIR):
            if list(args) == ['rev-parse', 'HEAD']:
                return M + '\n'
            raise AssertionError('unexpected packet-checkout read %r' % (args,))
        head, status, closure = closures[str(cwd)]
        if list(args) == ['rev-parse', 'HEAD']:
            return head + '\n'
        if list(args) == ['status', '--porcelain']:
            return status
        if len(args) == 2 and args[0] == 'rev-parse' and args[1].startswith('HEAD:'):
            path = args[1][len('HEAD:'):]
            if path not in closure:
                raise core.Refused('input-drift', 'no blob at ' + path)
            return closure[path] + '\n'
        raise AssertionError('unexpected git read %r' % (args,))
    return git


def write_artifact(producer, request, *, active_version=None, install_identity=None,
                   receipt_triple=acq.TARGET, receipt_sha256=None, payload=ARTIFACT_BYTES):
    """What the ordinary producer leaves beside its output when it rebuilds.

    The receipt is production-shaped: `write_workspace_artifact_receipt` strips
    the `sha256:` prefix, so the recorded digest is bare hex.
    """
    path = Path(producer) / '.effigy' / 'linux-release' / 'artifacts' / acq.ARTIFACT_NAME
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    digest = receipt_sha256 if receipt_sha256 is not None else core.sha256_file(path)
    hex_digest = str(digest).split(':', 1)[1] if ':' in str(digest) else str(digest)
    (Path(str(path) + acq.RECEIPT_SUFFIX)
     .write_text('release_triple=%s\nsha256=%s\nbuild_kind=fixture\n' % (receipt_triple, hex_digest)))
    version = active_version if active_version is not None \
        else request['candidate']['install_identity']['version']
    identity = install_identity if install_identity is not None \
        else request['producer']['install_identity']
    Path(str(path) + acq.ACTIVE_VERSION_SUFFIX).write_text(version + '\n')
    Path(str(path) + acq.INSTALL_IDENTITY_SUFFIX).write_text(identity + '\n')
    return path


def stale_artifact(producer, request):
    """Output that predates the pinned identities: present, receipted, not current."""
    path = write_artifact(producer, request, active_version='v0.0.0+local.0000000',
                          install_identity='local:v0.0.0:1:1')
    stamp = time.time_ns() - 10 ** 12
    os.utime(path, ns=(stamp, stamp))
    return path


def reset_checkouts(request):
    """Each control starts with no producer output anywhere."""
    for base in (request['producer']['checkout'], checkout_of(ACQ_NAMES['legacy']),
                 checkout_of(ACQ_NAMES['modern'])):
        effigy = Path(base) / '.effigy'
        if effigy.is_symlink() or effigy.is_file():
            os.unlink(effigy)
        elif effigy.is_dir():
            shutil.rmtree(effigy)


class AcquisitionEngine:
    """Harmless transport: per-pass stacks, and a production-shaped producer step.

    The producer step mirrors `ensure_local_linux_workspace_effigy_artifact`: an
    absent or not-current artifact is rebuilt once with fresh bytes, sidecars and
    receipt; a current one is reused untouched. Stamp overrides let a control
    make the producer leave stale or wrong-identity output. Stack state and the
    artifact are per fixture root; both consumer checkouts stay output-free.
    """

    def __init__(self, manifest, request, *, running=None, produce=None, fail_up=None,
                 stamp=None, on_up=None):
        reset_checkouts(request)
        route = manifest['routes'][acq.ROUTE]
        self.request = request
        self.producer = Path(request['producer']['checkout'])
        self.names = {checkout_of(name): key for key, name in ACQ_NAMES.items()}
        self.env = acq.environments(route)
        self.running = {'legacy': False, 'modern': False}
        self.running.update(running or {})
        self.produce = set(produce if produce is not None else ['legacy', 'modern'])
        self.fail_up = dict(fail_up or {})
        self.stamp = dict(stamp or {})
        self.on_up = on_up
        self.children = ROOT / 'acq-children' / uuid_like()
        self.containers = [
            {'id': LEG_PG, 'project': route['identity']['project'], 'service': LEG_SVC,
             'mounts': [LEG_VOL]},
            {'id': PG_ID, 'project': route['modern']['identity']['project'],
             'service': route['modern']['identity']['service'],
             'mounts': [route['modern']['identity']['volume']]},
            {'id': route['identity']['shared_container'], 'project': None, 'service': None,
             'mounts': []},
        ]
        self.volumes = {LEG_VOL, route['identity']['shared_volume']} | set(
            route['modern']['identity']['never_touch_volumes']) | {
            route['modern']['identity']['volume'], route['modern']['identity']['shared_volume']}
        self.calls = []

    def __call__(self, argv, label, cwd):
        name = self.names.get(str(cwd))
        if name is None:
            raise AssertionError('child not run from a pinned checkout: %r' % (cwd,))
        short = label.split('-', 1)[1] if label[:3].isdigit() and label[3:4] == '-' else label
        self.calls.append((name, short, list(argv[1:])))
        if argv[1] != 'container':
            return self._record(short, self._engine_output(argv))
        env = self.env[name]
        tail = tuple(argv[1:])
        if tail == ('container', env, 'up', '--detach'):
            if name in self.fail_up:
                raise core.Refused(self.fail_up[name], short)
            self.running[name] = True
            if name in self.produce:
                self._produce()
            if self.on_up:
                self.on_up(self)
            return self._record(short, '')
        if tail == ('container', env, 'status', '--json'):
            return self._record(short, json.dumps(status_report(self.running[name])) + '\n')
        if tail == ('container', env, 'down'):
            self.running[name] = False
            return self._record(short, '')
        raise AssertionError('tail outside the route: %r' % (tail,))

    def _produce(self):
        """Rebuild once when absent or not current; reuse a current artifact untouched."""
        if acq.artifact_current(acq.artifact_facts(str(self.producer)), self.request):
            return
        write_artifact(self.producer, self.request, **self.stamp)

    def _record(self, short, text):
        self.children.mkdir(parents=True, exist_ok=True)
        path = self.children / (short + '.stdout')
        path.write_text(text)
        return {'label': short, 'exit': 0, 'cwd': '', 'streams': {'stdout': {'path': str(path)}}}

    def _engine_output(self, argv):
        rest = argv[argv.index('--') + 1:]
        verb = rest[0]
        if verb == 'ps':
            filters = [rest[i + 1] for i, item in enumerate(rest) if item == '--filter']
            rows = [box['id'] for box in self.containers
                    if all(FakeEngine._match(box, f) for f in filters)]
            return '\n'.join(sorted(rows)) + ('\n' if rows else '')
        if verb == 'volume':
            return '\n'.join(sorted(self.volumes)) + '\n'
        if verb == 'inspect':
            box = next(b for b in self.containers if b['id'] == rest[-1])
            return ''.join('volume %s\n' % name for name in box['mounts'])
        raise AssertionError('unexpected engine verb ' + verb)

    def ups(self):
        return [(name, short) for name, short, tail in self.calls
                if len(tail) == 4 and tail[2] == 'up']

    def downs(self):
        return [(name, short) for name, short, tail in self.calls
                if len(tail) == 3 and tail[2] == 'down']


def status_report(running):
    """The candidate's `container status --json` shape for the acquisition route."""
    return {'schema': 'effigy.container.status.v1', 'schema_version': 1, 'ok': True,
            'runtime_running': running}


def acq_refusal(name, code, *, prior=ACQ_INTENT, engine=None, git=None, manifest=None,
                request=None, **engine_kw):
    manifest = manifest if manifest is not None else acquisition_manifest()
    request = request if request is not None else acquisition_request(manifest, prior)
    engine = engine if engine is not None else AcquisitionEngine(manifest, request, **engine_kw)
    refusal_run(acq, acq.ROUTE, request, name, code, engine=engine, manifest=manifest,
                git=git or acquisition_git(manifest))
    return engine


def acq_passed(name, *, prior=ACQ_INTENT, engine=None, manifest=None, request=None, **engine_kw):
    manifest = manifest if manifest is not None else acquisition_manifest()
    request = request if request is not None else acquisition_request(manifest, prior)
    engine = engine if engine is not None else AcquisitionEngine(manifest, request, **engine_kw)
    result, evidence = run_entry(acq, acq.ROUTE, request, name, engine=engine, manifest=manifest,
                                 git=acquisition_git(manifest))
    return result, evidence, engine


@check('ordinary acquisition: committed route refuses for pending pins before any request read')
def _():
    expect_code('route-pins-pending', lambda: acq.run(
        str(ROOT / 'no-such-request.json'), SHA, str(ROOT / 'no-such-evidence')))
    assert acq.main([]) == 2


@check('ordinary acquisition: the entry is the one fixed route and refuses unknown argv')
def _():
    expect_code('unarmed', lambda: proof_entry.run(['acow-acquisition', '--request', 'x']))
    assert proof_entry.ROUTES['acow-acquisition'] is acq


@check('ordinary acquisition: a request for another route refuses on shape, not on pins')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    request['route'] = 'acow'
    expect_code('request-route', lambda: core.validate_request_shape(request, acq.ROUTE))


@check('ordinary acquisition: modern_consumer and producer are shape keys only on this route')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    core.validate_request_shape(request, acq.ROUTE)
    plain = request_for(manifest, 'acow', {'stack': False})
    plain['modern_consumer'] = copy.deepcopy(request['modern_consumer'])
    plain['producer'] = copy.deepcopy(request['producer'])
    expect_code('request-shape', lambda: core.validate_request_shape(plain, 'acow'))
    dropped = copy.deepcopy(request)
    del dropped['producer']
    expect_code('request-shape', lambda: core.validate_request_shape(dropped, acq.ROUTE))


@check('ordinary acquisition: both passes run their own pinned ordinary forms, in order')
def _():
    result, evidence, engine = acq_passed('acq-happy')
    assert result['status'] == 'passed', result
    ups = engine.ups()
    assert ups == [('legacy', 'legacy-up'), ('modern', 'modern-up')], ups
    first_up = next(call for call in engine.calls if call[1].endswith('-up'))
    assert first_up == ('legacy', 'legacy-up',
                        ['container', 'dev', 'up', '--detach']), first_up
    assert any(tail == ['container', 'stack', 'up', '--detach']
               for _, _, tail in engine.calls), engine.calls
    assert result['passes']['legacy']['produced'] is True
    assert result['passes']['legacy']['reused'] is False
    assert result['passes']['legacy']['artifact']['before'] == {'present': False}
    assert result['passes']['legacy']['artifact']['after']['receipt_release_triple'] == acq.TARGET
    assert result['passes']['modern']['produced'] is False
    assert result['passes']['modern']['reused'] is True
    assert result['passes']['modern']['artifact']['after']['receipt_release_triple'] == acq.TARGET
    assert engine.running == {'legacy': False, 'modern': False}, 'the window stops what it started'
    assert not (Path(checkout_of(ACQ_NAMES['legacy'])) / '.effigy').exists()
    assert not (Path(checkout_of(ACQ_NAMES['modern'])) / '.effigy').exists()
    occurrence = json.loads((evidence / 'occurrence.json').read_text())
    assert occurrence['status'] == 'passed' and occurrence['spent'] is False, occurrence


@check('ordinary acquisition: a producer exit alone is not handoff success')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, produce=[])
    acq_refusal('acq-exit-only', 'handoff-missing', manifest=manifest, request=request,
                engine=engine)
    assert engine.ups() == [('legacy', 'legacy-up')], engine.ups()
    out = Path(request['producer']['checkout']) / '.effigy'
    assert not out.exists() or not list(out.rglob(acq.ARTIFACT_NAME)), \
        'a failed producer publishes nothing'


@check('ordinary acquisition: a legacy refusal skips modern and nothing retries')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, fail_up={'legacy': 'child-failed'})
    acq_refusal('acq-legacy-fails', 'child-failed', manifest=manifest, request=request,
                engine=engine)
    assert engine.ups() == [('legacy', 'legacy-up')], engine.ups()
    assert not any(name == 'modern' for name, _, _ in engine.calls), 'modern must not start'


@check('ordinary acquisition: an unknown legacy closure skips modern')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, fail_up={'legacy': 'child-unreaped'})
    acq_refusal('acq-legacy-unknown', 'child-unreaped', manifest=manifest, request=request,
                engine=engine)
    assert not any(name == 'modern' for name, _, _ in engine.calls), 'modern must not start'


@check('ordinary acquisition: a modern failure keeps the legacy pass and spends the window')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, fail_up={'modern': 'child-failed'})
    acq_refusal('acq-modern-fails', 'child-failed', manifest=manifest, request=request,
                engine=engine)
    assert engine.ups() == [('legacy', 'legacy-up'), ('modern', 'modern-up')], engine.ups()


@check('ordinary acquisition: an already-running stack is reconciled, never stopped')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, {'legacy': {'dev': True}, 'modern': {'stack': False}})
    engine = AcquisitionEngine(manifest, request, running={'legacy': True})
    result, evidence = run_entry(acq, acq.ROUTE, request, 'acq-prior-running', engine=engine,
                                 manifest=manifest, git=acquisition_git(manifest))
    assert result['status'] == 'passed', result
    assert engine.running['legacy'] is True, 'a pre-existing running stack stays running'
    assert all(name != 'legacy' for name, _ in engine.downs()), engine.downs()
    assert ('legacy', 'legacy-up') in engine.ups(), 'the ordinary up still reconciles'


@check('ordinary acquisition: prior intent drift refuses before any bring-up')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, running={'legacy': True})
    refusal_run(acq, acq.ROUTE, request, 'acq-drift', 'prior-intent', manifest=manifest,
                engine=engine, git=acquisition_git(manifest))
    assert engine.ups() == []


@check('ordinary acquisition: prior intent must name the pinned environment per pass')
def _():
    manifest = acquisition_manifest()
    engine = AcquisitionEngine(manifest, acquisition_request(manifest, ACQ_INTENT))
    acq_refusal('acq-intent-shape', 'prior-intent',
                prior={'legacy': {'stack': False}, 'modern': {'stack': False}}, engine=engine)
    assert engine.ups() == []


@check('ordinary acquisition: modern closure drift refuses before any engine or up effect')
def _():
    manifest = acquisition_manifest()
    closure = copy.deepcopy(manifest['routes'][acq.ROUTE]['modern']['consumer']['relevant_inputs'])
    key = sorted(closure)[0]
    closure[key] = 'b' * 40
    engine = AcquisitionEngine(manifest, acquisition_request(manifest, ACQ_INTENT))
    git = acquisition_git(manifest, modern_closure=closure)
    acq_refusal('acq-modern-drift', 'input-drift', manifest=manifest, engine=engine, git=git)
    assert engine.calls == [], 'closure drift must refuse before any child'


@check('ordinary acquisition: a dirty modern checkout refuses before any child')
def _():
    manifest = acquisition_manifest()
    engine = AcquisitionEngine(manifest, acquisition_request(manifest, ACQ_INTENT))
    acq_refusal('acq-modern-dirty', 'consumer-dirty', manifest=manifest, engine=engine,
                git=acquisition_git(manifest, modern_status=' M effigy.toml\n'))
    assert engine.calls == []


@check('ordinary acquisition: a foreign stack container refuses at the post-up binding')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, produce=[])
    engine.containers[0] = dict(engine.containers[0], id='b2' * 32)
    acq_refusal('acq-foreign', 'resource-mismatch', manifest=manifest, request=request,
                engine=engine)
    assert engine.ups() == [('legacy', 'legacy-up')], engine.ups()
    assert any(short == 'legacy-rollback' for _, short in engine.downs()), engine.downs()
    assert not any(name == 'modern' for name, _, _ in engine.calls), 'modern must not start'


@check('ordinary acquisition: a container that does not mount the pinned volume refuses')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, produce=[])
    engine.containers[0] = dict(engine.containers[0], mounts=['other-volume'])
    acq_refusal('acq-nomount', 'resource-mismatch', manifest=manifest, request=request,
                engine=engine)
    assert not any(name == 'modern' for name, _, _ in engine.calls)


@check('ordinary acquisition: an already-running foreign container refuses before any up')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, {'legacy': {'dev': True}, 'modern': {'stack': False}})
    engine = AcquisitionEngine(manifest, request, running={'legacy': True}, produce=[])
    engine.containers[0] = dict(engine.containers[0], id='b2' * 32)
    acq_refusal('acq-foreign-running', 'resource-mismatch', manifest=manifest, request=request,
                engine=engine)
    assert engine.ups() == [], 'the pre-up binding must refuse before the reconcile'
    assert engine.running['legacy'] is True, 'the foreign state is left alone'


@check('ordinary acquisition: a lost protected volume after a pass refuses')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)

    def drop_silo(passed_engine):
        passed_engine.volumes.discard('acowtancy-dev-silo-data')
    engine = AcquisitionEngine(manifest, request, on_up=drop_silo)
    acq_refusal('acq-lost-volume', 'protected-identity-changed', manifest=manifest,
                request=request, engine=engine)


@check('ordinary acquisition: currency binds receipt target and digest, and both sidecars')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    reset_checkouts(request)
    producer = request['producer']['checkout']
    assert acq.artifact_current({'present': False}, request) is False
    write_artifact(producer, request)
    assert acq.artifact_current(acq.artifact_facts(producer), request) is True
    good = acq.artifact_facts(producer)
    payload_hex = good['sha256'].split(':', 1)[1]
    assert good['receipt_sha256'] == payload_hex, \
        'the production receipt records bare hex, not sha256:<hex>'
    wrong_triple = dict(good, receipt_release_triple='x86_64-unknown-linux-gnu')
    wrong_digest = dict(good, receipt_sha256='0' * 64)
    prefixed = dict(good, receipt_sha256='sha256:' + payload_hex)
    upper = dict(good, receipt_sha256=payload_hex.upper())
    wrong_version = dict(good, active_version='v0.0.0+local.0000000')
    wrong_identity = dict(good, install_identity='local:v0.0.0:1:1')
    wrong_bytes = dict(good, sha256='sha256:' + '1' * 64)
    for facts in (wrong_triple, wrong_digest, prefixed, wrong_version, wrong_identity,
                  wrong_bytes):
        assert acq.artifact_current(facts, request) is False, facts
    assert acq.artifact_current(upper, request) is True, \
        'production compares recorded hex case-insensitively'
    request['producer']['artifact_sha256'] = 'sha256:' + '2' * 64
    assert acq.artifact_current(good, request) is False, 'an unpinned digest is not the pin'
    assert acq.artifact_current(
        dict(good, sha256='sha256:' + '2' * 64, receipt_sha256='2' * 64),
        request) is True
    assert acq.digest_matches('0' * 64, 'sha256:' + '0' * 64) is True
    assert acq.digest_matches('sha256:' + '0' * 64, 'sha256:' + '0' * 64) is False
    assert acq.digest_matches(None, 'sha256:' + '0' * 64) is False
    assert acq.digest_matches('', 'sha256:' + '0' * 64) is False


@check('ordinary acquisition: stale input needs a rewrite, warm reuse needs none')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    reset_checkouts(request)
    producer = request['producer']['checkout']
    expect_code('handoff-missing', lambda: acq.produced_in_window(
        {'present': False}, {'present': False}, request, 'legacy'))
    _stale_path = stale_artifact(producer, request)
    stale = acq.artifact_facts(producer)
    expect_code('handoff-missing', lambda: acq.produced_in_window(
        stale, stale, request, 'legacy'))
    touched = dict(stale, mtime_ns=stale['mtime_ns'] + 1)
    expect_code('handoff-missing', lambda: acq.produced_in_window(
        stale, touched, request, 'legacy'))
    write_artifact(producer, request)
    fresh = acq.artifact_facts(producer)
    handoff = acq.produced_in_window(stale, fresh, request, 'legacy')
    assert handoff == {'produced': True, 'reused': False}, handoff
    warm = acq.produced_in_window(fresh, dict(fresh, mtime_ns=fresh['mtime_ns']), request, 'modern')
    assert warm == {'produced': False, 'reused': True}, warm


@check('ordinary acquisition: a status report in the old running shape refuses before any up')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, produce=[])

    def old_shape(argv, label, cwd):
        if tuple(argv[1:]) == ('container', 'dev', 'status', '--json'):
            return engine._record(label.split('-', 1)[1], json.dumps({'running': False}) + '\n')
        return engine(argv, label, cwd)
    acq_refusal('acq-old-status', 'status-unparsed', manifest=manifest, request=request,
                engine=old_shape)
    assert engine.ups() == [], 'an unreadable status must refuse before the up'


@check('ordinary acquisition: a status report with another schema refuses before any up')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, produce=[])

    def other_schema(argv, label, cwd):
        if tuple(argv[1:]) == ('container', 'dev', 'status', '--json'):
            record = engine._record(label.split('-', 1)[1],
                                    json.dumps({'schema': 'effigy.container.status.v0',
                                                'runtime_running': False}) + '\n')
            record['label'] = label
            return record
        return engine(argv, label, cwd)
    acq_refusal('acq-other-schema', 'status-unparsed', manifest=manifest, request=request,
                engine=other_schema)
    assert engine.ups() == []


@check('ordinary acquisition: the route reads the pinned runtime_running contract')
def _():
    manifest = acquisition_manifest()
    contract = manifest['routes'][acq.ROUTE]['status_contract']
    assert contract['field'] == 'runtime_running'
    assert contract['schema'] == 'effigy.container.status.v1'
    assert core.parse_running(json.dumps(status_report(True)), contract['field'],
                              contract['schema']) is True
    expect_code('status-unparsed', lambda: core.parse_running(
        json.dumps({'running': True}), contract['field'], contract['schema']))


@check('ordinary acquisition: a symlinked producer artifact path refuses before any read or up')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, produce=[])
    base = Path(request['producer']['checkout']) / '.effigy'
    base.mkdir(parents=True, exist_ok=True)
    link = base / 'linux-release'
    target_dir = ROOT / 'elsewhere'
    target_dir.mkdir(parents=True, exist_ok=True)
    os.symlink(target_dir, link)
    try:
        acq_refusal('acq-symlink', 'artifact-symlink', manifest=manifest, request=request,
                    engine=engine)
        assert engine.ups() == []
    finally:
        os.unlink(link)


@check('ordinary acquisition: provenance derives the checkout the candidate derives')
def _():
    shape = ROOT / 'provenance'
    shutil.rmtree(shape, ignore_errors=True)
    for rel in ('effigy.toml', 'config/tasks.toml', 'config/containers.toml'):
        target = shape / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text('fixture\n')
    install = shape / '.local-install' / 'bin' / 'effigy'
    install.parent.mkdir(parents=True, exist_ok=True)
    install.write_bytes(b'x')
    assert acq.provenance_checkout(str(install)) == shape
    build = shape / 'target' / 'release' / 'effigy'
    build.parent.mkdir(parents=True, exist_ok=True)
    build.write_bytes(b'x')
    (shape / '.local-install').rename(ROOT / 'moved-local-install')
    assert acq.provenance_checkout(str(build)) == shape
    (shape / 'config' / 'tasks.toml').unlink()
    expect_code('producer-mismatch', lambda: acq.provenance_checkout(str(build)))
    (shape / 'config' / 'tasks.toml').write_text('fixture\n')
    plain = ROOT / 'plain-bin-effigy'
    plain.write_bytes(b'x')
    expect_code('producer-mismatch', lambda: acq.provenance_checkout(str(plain)))
    expect_code('producer-mismatch', lambda: acq.provenance_checkout(
        str(shape / 'target' / 'x86' / 'effigy')))


@check('ordinary acquisition: install receipts bind version and candidate commit')
def _():
    assert acq.local_commit_from_identity(ACQ_VERSION) == 'ccccccc'
    assert acq.local_commit_from_identity('v1+local.abc1234.dirty') == 'abc1234'
    assert acq.local_commit_from_identity('v1') is None
    assert acq.local_commit_from_identity('v1+local.zzz') is None
    assert acq.local_commit_from_identity('v1+local.ab') is None
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    acq.verify_install_receipt(request)
    wrong_version = copy.deepcopy(request)
    wrong_version['candidate']['install_identity']['version'] = 'v9.9.9+local.ccccccc'
    expect_code('candidate-mismatch', lambda: acq.verify_install_receipt(wrong_version))
    wrong_commit = copy.deepcopy(request)
    wrong_commit['candidate']['source_commit'] = 'd' * 40
    expect_code('candidate-mismatch', lambda: acq.verify_install_receipt(wrong_commit))
    no_receipt = copy.deepcopy(request)
    no_receipt['candidate']['install_identity']['path'] = str(ROOT / 'bin' / 'no-receipt-effigy')
    expect_code('candidate-mismatch', lambda: acq.verify_install_receipt(no_receipt))


@check('ordinary acquisition: a wrong producer binding refuses before claim or child')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    derived = str(Path(request['candidate']['install_identity']['path']).parent.parent.parent)
    cases = {
        'outside': dict(checkout=str(ROOT / 'not-the-producer'), artifact_sha256=None,
                        install_identity=request['producer']['install_identity']),
        'consumer': dict(checkout=checkout_of(ACQ_NAMES['legacy']), artifact_sha256=None,
                         install_identity=request['producer']['install_identity']),
        'relative': dict(checkout='relative/producer', artifact_sha256=None,
                         install_identity=request['producer']['install_identity']),
        'digest': dict(checkout=derived, artifact_sha256='sha256:short',
                       install_identity=request['producer']['install_identity']),
        'identity': dict(checkout=derived, artifact_sha256=None,
                         install_identity='bad identity!'),
    }
    for name, producer in cases.items():
        request = acquisition_request(manifest, ACQ_INTENT, producer_checkout=producer['checkout'])
        request['producer'] = dict(request['producer'], **producer)
        engine = AcquisitionEngine(manifest, request, produce=[])
        acq_refusal('acq-producer-' + name, 'producer-mismatch', manifest=manifest,
                    request=request, engine=engine)
        assert not (ROOT / ('acq-producer-' + name) / 'evidence').exists()
        assert engine.calls == [], name


@check('ordinary acquisition: a producer checkout nested inside a consumer refuses')
def _():
    """Containment, not equality: a producer under a consumer puts its output in it."""
    manifest = acquisition_manifest()
    for name, checkout in (('legacy', checkout_of(ACQ_NAMES['legacy'])),
                           ('modern', checkout_of(ACQ_NAMES['modern']))):
        nested = Path(checkout) / ('nested-producer-' + name)
        request = acquisition_request(manifest, ACQ_INTENT, script=producer_script(nested))
        assert acq.path_within(request['producer']['checkout'], checkout), 'fixture must nest'
        engine = AcquisitionEngine(manifest, request, produce=[])
        acq_refusal('acq-producer-nested-' + name, 'producer-mismatch', manifest=manifest,
                    request=request, engine=engine)
        assert not (ROOT / ('acq-producer-nested-' + name) / 'evidence').exists()
        assert engine.calls == [], name


@check('ordinary acquisition: containment is component-wise, not a string prefix')
def _():
    """A neighbouring name that shares a string prefix is not containment."""
    manifest = acquisition_manifest()
    neighbour = ROOT / 'checkouts' / 'acquisition-legacy-x'
    neighbour.mkdir(parents=True, exist_ok=True)
    request = acquisition_request(manifest, ACQ_INTENT,
                                  script=producer_script(ROOT / 'checkouts'
                                                         / 'acquisition-legacy-y'
                                                         / 'producer'))
    engine = AcquisitionEngine(manifest, request)
    result, evidence = run_entry(acq, acq.ROUTE, request, 'acq-producer-neighbour',
                                 engine=engine, manifest=manifest,
                                 git=acquisition_git(manifest))
    assert result['status'] == 'passed', result


@check('ordinary acquisition: a modern producer pin whose checkout is the modern consumer refuses')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT,
                                  producer_checkout=checkout_of(ACQ_NAMES['modern']))
    engine = AcquisitionEngine(manifest, request, produce=[])
    acq_refusal('acq-producer-modern-consumer', 'producer-mismatch', manifest=manifest,
                request=request, engine=engine)
    assert engine.calls == []


@check('ordinary acquisition: wrong-identity producer output refuses after the up')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request,
                               stamp={'active_version': 'v0.0.0+local.0000000'})
    acq_refusal('acq-wrong-identity', 'handoff-missing', manifest=manifest, request=request,
                engine=engine)
    assert engine.ups() == [('legacy', 'legacy-up')], engine.ups()
    assert not any(name == 'modern' for name, _, _ in engine.calls)


@check('ordinary acquisition: wrong-receipt producer output refuses after the up')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request, stamp={'receipt_triple': 'x86_64-unknown-linux-gnu'})
    acq_refusal('acq-wrong-receipt', 'handoff-missing', manifest=manifest, request=request,
                engine=engine)
    engine = AcquisitionEngine(manifest, request, stamp={'receipt_sha256': '0' * 64})
    acq_refusal('acq-wrong-receipt-digest', 'handoff-missing', manifest=manifest,
                request=request, engine=engine)


@check('ordinary acquisition: a current artifact is verified warm reuse, not new output')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request)
    write_artifact(request['producer']['checkout'], request)
    before = acq.artifact_facts(request['producer']['checkout'])
    result, evidence = run_entry(acq, acq.ROUTE, request, 'acq-warm', engine=engine,
                                 manifest=manifest, git=acquisition_git(manifest))
    assert result['status'] == 'passed', result
    for name in ('legacy', 'modern'):
        assert result['passes'][name]['reused'] is True, name
        assert result['passes'][name]['produced'] is False, name
        assert result['passes'][name]['artifact']['after']['mtime_ns'] \
            == before['mtime_ns'], 'warm reuse rewrites nothing'


@check('ordinary acquisition: a pinned digest accepts reuse and refuses other bytes')
def _():
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request)
    path = write_artifact(request['producer']['checkout'], request)
    digest = core.sha256_file(path)
    request['producer']['artifact_sha256'] = digest
    result, evidence = run_entry(acq, acq.ROUTE, request, 'acq-pinned-warm', engine=engine,
                                 manifest=manifest, git=acquisition_git(manifest))
    assert result['status'] == 'passed', result
    other = acquisition_request(manifest, ACQ_INTENT, artifact_sha256='sha256:' + '2' * 64)
    engine = AcquisitionEngine(manifest, other)
    write_artifact(other['producer']['checkout'], other)
    refusal_run(acq, acq.ROUTE, other, 'acq-pinned-wrong', 'handoff-missing', engine=engine,
                manifest=manifest, git=acquisition_git(manifest))


@check('ordinary acquisition: the session refuses any tail outside the pinned forms')
def _():
    manifest = acquisition_manifest()
    pins = manifest['routes'][acq.ROUTE]
    session = core.Session(forbidden_transport, str(producer_script()),
                           checkout_of(ACQ_NAMES['legacy']), acq.pass_tails(pins),
                           pins['engine'])
    expect_code('argv-not-allowlisted', lambda: session.effigy(
        ('container', 'stack', 'up', '--detach', '--repo', '/x'), 'bad'))
    expect_code('argv-not-allowlisted', lambda: session.effigy(
        ('container', 'web', 'up', '--detach'), 'bad-env'))
    expect_code('argv-not-allowlisted', lambda: session.effigy(
        ('dev',), 'bad-dev'))
    assert acq.pass_tails(pins) == frozenset({
        ('container', 'dev', 'up', '--detach'), ('container', 'dev', 'status', '--json'),
        ('container', 'dev', 'down'), ('container', 'stack', 'up', '--detach'),
        ('container', 'stack', 'status', '--json'), ('container', 'stack', 'down')})


@check('ordinary acquisition: malformed environment names refuse at the pins')
def _():
    manifest = acquisition_manifest()
    pins = manifest['routes'][acq.ROUTE]
    pins['environments'] = {'legacy': 'web/dev', 'modern': 'stack'}
    expect_code('environment-name', lambda: acq.environments(pins))
    pins['environments'] = {'legacy': 'dev'}
    expect_code('binding-pins', lambda: acq.environments(pins))
    pins['environments'] = {'legacy': 'dev', 'modern': 'stack', 'extra': 'x'}
    expect_code('binding-pins', lambda: acq.environments(pins))


@check('ordinary acquisition: the committed manifest keeps its consumed windows under a binding')
def _():
    """One use survives an external binding: the binding may burn, never reopen."""
    manifest = acquisition_manifest()
    manifest['consumed_windows'] = ['fixture-window-acquisition']
    request = acquisition_request(manifest, ACQ_INTENT)
    folder = ROOT / 'acq-oneuse-binding'
    folder.mkdir(parents=True, exist_ok=True)
    pins = {key: manifest['routes'][acq.ROUTE][key] for key in sorted(acq.PIN_KEYS)}
    doc = {'schema': core.BINDING_SCHEMA, 'route': acq.ROUTE, 'pins': pins,
           'consumed_windows': []}
    raw = json.dumps(doc, sort_keys=True).encode()
    (folder / 'binding.json').write_bytes(raw)
    req_raw = json.dumps(request, sort_keys=True).encode()
    (folder / 'request.json').write_bytes(req_raw)
    request_sha = core.sha256_bytes(req_raw)

    def run_binding():
        argv = [acq.ROUTE, '--request', str(folder / 'request.json'),
                '--sha256', request_sha,
                '--evidence-dir', str(folder / 'evidence'),
                '--binding', str(folder / 'binding.json'),
                '--binding-sha256', core.sha256_bytes((folder / 'binding.json').read_bytes())]
        return proof_entry.run(argv, manifest=manifest, roots=(str(ROOT),),
                               transport=forbidden_transport, now_ns=NOW,
                               git=acquisition_git(manifest))

    expect_code('window-consumed', run_binding)
    assert not (folder / 'evidence').exists()
    # The binding adding the same burn changes nothing.
    doc = dict(doc, consumed_windows=['fixture-window-acquisition'])
    (folder / 'binding.json').write_bytes(json.dumps(doc, sort_keys=True).encode())
    expect_code('window-consumed', run_binding)


@check('ordinary acquisition: the window is one use, bounded, and unexpired')
def _():
    manifest = acquisition_manifest()
    manifest['consumed_windows'] = ['fixture-window-acquisition']
    request = acquisition_request(manifest, ACQ_INTENT)
    engine = AcquisitionEngine(manifest, request)
    refusal_run(acq, acq.ROUTE, request, 'acq-consumed', 'window-consumed', engine=engine,
                manifest=manifest, git=acquisition_git(manifest))
    assert engine.calls == []
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    request['window']['deadline_monotonic_ns'] = START + 3601 * NS
    refusal_run(acq, acq.ROUTE, request, 'acq-span', 'window-bounds',
                engine=AcquisitionEngine(manifest, request),
                manifest=manifest, git=acquisition_git(manifest))
    manifest = acquisition_manifest()
    request = acquisition_request(manifest, ACQ_INTENT)
    refusal_run(acq, acq.ROUTE, request, 'acq-expired', 'window-expired',
                engine=AcquisitionEngine(manifest, request), manifest=manifest,
                git=acquisition_git(manifest), now=START + 600 * NS)


@check('ordinary acquisition: the packet keeps the acow and cbs routes unchanged in shape')
def _():
    for route in ('acow', 'cbs'):
        assert 'modern_consumer' not in core.ROUTE_EXTRA_KEYS.get(route, frozenset())
        assert 'producer' not in core.ROUTE_EXTRA_KEYS.get(route, frozenset())
    assert set(core.REQUEST_KEYS) == {
        'schema', 'route', 'candidate', 'helpers', 'predecessor', 'consumer',
        'provenance', 'window', 'budgets', 'resources', 'prior_intent'}
    assert 'consumer-packets-r1/acow_acquisition.py' in core.ENTRY_MODULES


@check('ordinary acquisition: a same-source detached copy computes the same module hashes')
def _():
    base = ROOT / 'portable'
    shutil.rmtree(base, ignore_errors=True)
    for name in ('consumer-packets-r1', 'workspace-private-531'):
        shutil.copytree(core.QUALIFICATION / name, base / name,
                        ignore=shutil.ignore_patterns('__pycache__'))
    code = ('import json, sys; sys.path.insert(0, sys.argv[1]); import consumer_core as c; '
            'print(json.dumps(c.module_hashes(), sort_keys=True))')
    out = subprocess.run([sys.executable, '-I', '-B', '-c', code, str(base / 'consumer-packets-r1')],
                         capture_output=True, text=True, check=True).stdout
    assert json.loads(out) == core.module_hashes(), 'a detached copy must bind the same modules'


# Fixture candidate CLI for the end-to-end acquisition controls. Its own path is
# <producer>/.local-install/bin/effigy, so the producer checkout derives exactly the
# way the candidate derives it. `container` argv is the route's ordinary form run in
# the checkout named by cwd; the producer step mirrors
# ensure_local_linux_workspace_effigy_artifact. `nerdctl` argv is a read through the
# pinned profile. No real engine, consumer or native effect exists behind it.
FIXTURE_PROD_CLI = '''#!PYTHON -I
"""Production-shaped fixture candidate: engine surface plus the maintained producer."""
import hashlib
import json
import os
import sys
from pathlib import Path

PRODUCER = Path(__file__).resolve().parent.parent.parent
STATE = PRODUCER / 'state.json'
argv = sys.argv[1:]
state = json.loads(STATE.read_text())
name = os.path.basename(os.getcwd())


def save():
    STATE.write_text(json.dumps(state, sort_keys=True))


def produce():
    """Rebuild once when absent or not current; reuse a current artifact untouched."""
    path = PRODUCER / '.effigy' / 'linux-release' / 'artifacts' / ('effigy-' + state['target'])
    if path.is_file():
        receipt = {}
        for line in Path(str(path) + '.receipt').read_text().splitlines():
            key, sep, value = line.strip().partition('=')
            if sep:
                receipt[key] = value
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        current = (receipt.get('release_triple') == state['target']
                   and receipt.get('sha256') == digest
                   and Path(str(path) + '.active-version').read_text().strip() == state['version']
                   and Path(str(path) + '.install-identity').read_text().strip()
                   == state['identity'])
        if current:
            return
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = b'fixture linux artifact bytes\\n'
    path.write_bytes(payload)
    digest = hashlib.sha256(payload).hexdigest()
    stamp = state.get('stamp') or {}
    triple = stamp.get('receipt_triple', state['target'])
    receipt_sha = stamp.get('receipt_sha256', digest)
    Path(str(path) + '.receipt').write_text(
        'release_triple=%s\\nsha256=%s\\nbuild_kind=fixture\\n' % (triple, receipt_sha))
    Path(str(path) + '.active-version').write_text(
        stamp.get('active_version', state['version']) + '\\n')
    Path(str(path) + '.install-identity').write_text(
        stamp.get('install_identity', state['identity']) + '\\n')


if argv[:1] == ['container']:
    state['log'].append([name, argv])
    env = argv[1]
    if argv == ['container', env, 'up', '--detach']:
        if name in state['fail_up']:
            save()
            sys.exit(1)
        state['running'][name] = True
        if name in state['produce']:
            produce()
        save()
        sys.exit(0)
    if argv == ['container', env, 'status', '--json']:
        print(json.dumps({'schema': 'effigy.container.status.v1',
                          'runtime_running': state['running'][name]}))
        save()
        sys.exit(0)
    if argv == ['container', env, 'down']:
        state['running'][name] = False
        save()
        sys.exit(0)
    sys.exit(2)
rest = argv[argv.index('--') + 1:]
state['log'].append([name, argv])
save()
verb = rest[0]
if verb == 'ps':
    filters = [rest[i + 1] for i, item in enumerate(rest) if item == '--filter']

    def match(box, flt):
        if flt.startswith('label=com.docker.compose.project='):
            return box['project'] == flt.split('=', 2)[2]
        if flt.startswith('label=com.docker.compose.service='):
            return box['service'] == flt.split('=', 2)[2]
        return False
    rows = [b['id'] for b in state['containers'] if all(match(b, f) for f in filters)]
    print('\\n'.join(sorted(rows)))
elif verb == 'volume':
    print('\\n'.join(sorted(state['volumes'])))
elif verb == 'inspect':
    box = next(b for b in state['containers'] if b['id'] == rest[-1])
    print('\\n'.join('volume ' + name for name in box['mounts']))
else:
    sys.exit(2)
'''


def make_main_producer(base):
    """The pinned install at its provenance shape, backed by the fixture CLI."""
    script = base / '.local-install' / 'bin' / 'effigy'
    script.parent.mkdir(parents=True, exist_ok=True)
    script.write_text(FIXTURE_PROD_CLI.replace('PYTHON', sys.executable))
    script.chmod(0o755)
    script.with_suffix('.active-version').write_text(ACQ_VERSION + '\n')
    for rel in ('effigy.toml', 'config/tasks.toml', 'config/containers.toml'):
        target = base / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text('fixture producer checkout shape: ' + rel + '\n')
    return script


def write_binding_doc(base, manifest):
    """The hash-pinned external runtime binding document for the acquisition route.

    It carries the route's functional pin set exactly; manifest-side citation
    fields are annotations and are not carried.
    """
    pins = {key: manifest['routes'][acq.ROUTE][key] for key in sorted(acq.PIN_KEYS)}
    doc = {'schema': core.BINDING_SCHEMA, 'route': acq.ROUTE, 'pins': pins,
           'consumed_windows': []}
    raw = json.dumps(doc, sort_keys=True).encode()
    path = base / 'binding.json'
    path.write_bytes(raw)
    return path, core.sha256_bytes(raw)


def acq_main_e2e(name, *, prior=ACQ_INTENT, produce=('legacy', 'modern'), fail_up=(),
                 stamp=None, artifact_sha256=None, running=None, binding_override=None):
    """A real `main` run: real argv parsing, the committed pending manifest, the
    hash-pinned binding, real supervisor children. Evidence is claimed under the
    packet's real evidence root; the test removes the fixture root it created."""
    base = ROOT / name
    manifest = acquisition_manifest()
    route = manifest['routes'][acq.ROUTE]
    env = {'GIT_CONFIG_NOSYSTEM': '1', 'HOME': str(base), 'PATH': os.environ.get('PATH', '')}
    checkouts = {'legacy': base / 'legacy', 'modern': base / 'modern'}
    producer = base / 'producer'
    script = make_main_producer(producer)
    heads = {}
    blobs = {}
    closures = {'legacy': route['consumer']['relevant_inputs'],
                'modern': route['modern']['consumer']['relevant_inputs']}
    for key, checkout in checkouts.items():
        checkout.mkdir(parents=True, exist_ok=True)
        run_git = lambda *args, c=checkout: subprocess.run(  # noqa: E731
            ['/usr/bin/git', '-C', str(c), *args], capture_output=True, text=True,
            check=True, env=env).stdout
        run_git('init', '-q')
        for path in closures[key]:
            target = checkout / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text('fixture closure input ' + path + '\n')
        run_git('add', '-A')
        run_git('-c', 'user.name=fixture', '-c', 'user.email=fixture@example.invalid',
                '-c', 'commit.gpgsign=false', 'commit', '-q', '-m', 'fixture closure')
        heads[key] = run_git('rev-parse', 'HEAD').strip()
        blobs[key] = {path: run_git('rev-parse', 'HEAD:' + path).strip() for path in closures[key]}
    route['checkout'] = str(checkouts['legacy'])
    route['modern']['checkout'] = str(checkouts['modern'])
    route['consumer']['relevant_inputs'] = blobs['legacy']
    route['modern']['consumer']['relevant_inputs'] = blobs['modern']
    request = acquisition_request(manifest, prior, script=script,
                                  artifact_sha256=artifact_sha256)
    now = time.monotonic_ns()
    request['window'].update(start_monotonic_ns=now, deadline_monotonic_ns=now + 600 * NS)
    digest = core.sha256_file(script)
    request['candidate'].update(binary_sha256=digest)
    request['candidate']['install_identity'].update(sha256=digest)
    containers = [
        {'id': LEG_PG, 'project': route['identity']['project'], 'service': LEG_SVC,
         'mounts': [LEG_VOL]},
        {'id': PG_ID, 'project': route['modern']['identity']['project'],
         'service': route['modern']['identity']['service'],
         'mounts': route['modern']['identity']['volume']},
        {'id': route['identity']['shared_container'], 'project': None, 'service': None,
         'mounts': []},
    ]
    volumes = sorted(set([LEG_VOL, route['identity']['shared_volume']]
                         + route['modern']['identity']['never_touch_volumes']
                         + route['identity']['never_touch_volumes']))
    state = {'running': dict(running) if running else {'legacy': False, 'modern': False},
             'produce': list(produce), 'fail_up': list(fail_up), 'stamp': stamp or {},
             'target': acq.TARGET, 'version': ACQ_VERSION,
             'identity': request['producer']['install_identity'],
             'containers': containers, 'volumes': volumes, 'log': []}
    (producer / 'state.json').write_text(json.dumps(state, sort_keys=True))
    binding_path, binding_sha = write_binding_doc(base, manifest)
    if binding_override is not None:
        binding_override(binding_path)
        binding_sha = core.sha256_file(binding_path)
    raw = json.dumps(request, sort_keys=True).encode()
    request_path = base / 'request.json'
    request_path.write_bytes(raw)
    # The real claim creates the evidence directory itself under the packet's
    # committed evidence root. On a workspace without that root the claim
    # refuses fail-closed; on the host it claims a fresh exclusive directory.
    evidence = Path(core.EVIDENCE_ROOTS[0]) / (name + '-' + uuid_like())
    argv = [sys.executable, '-I', '-B', str(DIR / 'proof_entry.py'), acq.ROUTE,
            '--request', str(request_path), '--sha256', core.sha256_bytes(raw),
            '--evidence-dir', str(evidence),
            '--binding', str(binding_path), '--binding-sha256', binding_sha]
    completed = subprocess.run(argv, cwd=str(DIR), env=env, capture_output=True, text=True,
                               timeout=600)
    state = json.loads((producer / 'state.json').read_text())
    return completed, evidence, evidence, state


EVIDENCE_ROOT_PRESENT = Path(core.EVIDENCE_ROOTS[0]).is_dir()


def binding_run_happy(name):
    """The full armed occurrence through the entry's binding form, no manifest kwarg.

    The committed manifest stays pending; the hash-pinned binding carries the
    route's pins. The fixture is production-shaped: the producer output sits in
    its own producer checkout outside both consumer checkouts, legacy builds
    cold, modern reuses verified, and the window stops what it started.
    """
    manifest = acquisition_manifest()
    route = manifest['routes'][acq.ROUTE]
    request = acquisition_request(manifest, ACQ_INTENT)
    reset_checkouts(request)
    folder = ROOT / name
    folder.mkdir(parents=True, exist_ok=True)
    doc = {'schema': core.BINDING_SCHEMA, 'route': acq.ROUTE,
           'pins': {key: route[key] for key in sorted(acq.PIN_KEYS)}, 'consumed_windows': []}
    raw = json.dumps(doc, sort_keys=True).encode()
    (folder / 'binding.json').write_bytes(raw)
    req_raw = json.dumps(request, sort_keys=True).encode()
    (folder / 'request.json').write_bytes(req_raw)
    engine = AcquisitionEngine(manifest, request)
    result = proof_entry.run(
        [acq.ROUTE, '--request', str(folder / 'request.json'),
         '--sha256', core.sha256_bytes(req_raw),
         '--evidence-dir', str(folder / 'evidence'),
         '--binding', str(folder / 'binding.json'),
         '--binding-sha256', core.sha256_bytes(raw)],
        roots=(str(ROOT),), transport=engine, git=acquisition_git(manifest), now_ns=NOW)
    assert result['status'] == 'passed', result
    ups = [pass_name for pass_name, _, tail in engine.calls
           if len(tail) == 4 and tail[2] == 'up']
    assert ups == ['legacy', 'modern'], ups
    first_up = next(call for call in engine.calls if call[1].endswith('-up'))
    assert first_up[2] == ['container', 'dev', 'up', '--detach'], first_up
    assert any(tail == ['container', 'stack', 'up', '--detach']
               for _, _, tail in engine.calls), 'modern runs its own pinned ordinary form'
    for pass_name in ('legacy', 'modern'):
        assert result['passes'][pass_name]['produced'] is (pass_name == 'legacy'), pass_name
        assert result['passes'][pass_name]['reused'] is (pass_name == 'modern'), pass_name
    assert not (Path(checkout_of(ACQ_NAMES['legacy'])) / '.effigy').exists()
    assert not (Path(checkout_of(ACQ_NAMES['modern'])) / '.effigy').exists()
    assert (Path(request['producer']['checkout']) / '.effigy' / 'linux-release' / 'artifacts'
            / acq.ARTIFACT_NAME).is_file()
    assert engine.running == {'legacy': False, 'modern': False}, 'the window stops what it started'
    occurrence = json.loads((folder / 'evidence' / 'occurrence.json').read_text())
    assert occurrence['status'] == 'passed' and occurrence['spent'] is False, occurrence
    binding_record = json.loads((folder / 'evidence' / 'binding.json').read_text())
    assert binding_record['binding_sha256'].startswith('sha256:'), binding_record
    return result


@check('ordinary acquisition: end to end through the real main entry, the binding arms the run')
def _():
    completed, evidence, evidence_root, state = acq_main_e2e('acq-main-e2e')
    try:
        if not EVIDENCE_ROOT_PRESENT:
            # This workspace materialises no committed evidence root, so the real
            # claim refuses fail-closed. The same armed occurrence still runs
            # below through the entry's binding form with fixture roots; nothing
            # is faked and no fake success is recorded.
            assert completed.returncode == 2, (completed.returncode, completed.stdout)
            assert json.loads(completed.stdout)['code'] == 'evidence-root'
            assert not evidence.exists()
            binding_run_happy('acq-main-e2e-runpath')
            return
        assert completed.returncode == 0, (completed.returncode, completed.stdout, completed.stderr)
        result = json.loads(completed.stdout)
        assert result['status'] == 'passed', result
        ups = [name for name, argv in state['log']
               if argv == ['container', 'dev', 'up', '--detach']]
        assert ups == ['legacy'], 'legacy runs its own pinned ordinary form'
        ups = [name for name, argv in state['log']
               if argv == ['container', 'stack', 'up', '--detach']]
        assert ups == ['modern'], 'modern runs its own pinned ordinary form'
        assert state['running'] == {'legacy': False, 'modern': False}, state['running']
        producer_out = ROOT / 'acq-main-e2e' / 'producer' / '.effigy' / 'linux-release' \
            / 'artifacts' / acq.ARTIFACT_NAME
        assert producer_out.is_file(), 'the host producer output sits in the producer checkout'
        assert not (ROOT / 'acq-main-e2e' / 'legacy' / '.effigy').exists()
        assert not (ROOT / 'acq-main-e2e' / 'modern' / '.effigy').exists()
        passes = result['passes']
        assert passes['legacy']['produced'] is True and passes['legacy']['reused'] is False
        assert passes['modern']['reused'] is True and passes['modern']['produced'] is False
        assert passes['modern']['artifact']['after']['mtime_ns'] \
            == passes['legacy']['artifact']['after']['mtime_ns'], 'reuse rewrites nothing'
        occurrence = json.loads((evidence / 'occurrence.json').read_text())
        assert occurrence['status'] == 'passed' and occurrence['spent'] is False, occurrence
        binding = json.loads((evidence / 'binding.json').read_text())
        assert binding['binding_sha256'].startswith('sha256:'), binding
        children = sorted(p.name for p in (evidence / 'children').iterdir())
        assert any(name.endswith('-legacy-up') for name in children), children
        assert any(name.endswith('-modern-up') for name in children), children
        for child in children:
            receipt = json.loads((evidence / 'children' / child / 'receipt.json').read_text())
            assert receipt['disposition'] == 'exited' and receipt['waited_exit'] == 0, \
                (child, receipt)
    finally:
        shutil.rmtree(evidence_root, ignore_errors=True)  # only exists when the root does


@check('ordinary acquisition: through the real main entry, an altered binding refuses first')
def _():
    base = ROOT / 'acq-main-badbinding'
    base.mkdir(parents=True, exist_ok=True)
    binding_path = base / 'binding.json'
    binding_path.write_bytes(b'{"schema": "x"}')
    evidence = base / 'evidence'
    argv = [sys.executable, '-I', '-B', str(DIR / 'proof_entry.py'), acq.ROUTE,
            '--request', str(base / 'missing-request.json'), '--sha256', SHA,
            '--evidence-dir', str(evidence),
            '--binding', str(binding_path), '--binding-sha256', 'sha256:' + '0' * 64]
    completed = subprocess.run(argv, cwd=str(DIR),
                               env={'PATH': os.environ.get('PATH', '')},
                               capture_output=True, text=True, timeout=300)
    assert completed.returncode == 2, (completed.returncode, completed.stdout)
    assert json.loads(completed.stdout)['code'] == 'binding-sha-mismatch'
    assert not evidence.exists(), 'a refusal before the claim leaves no evidence'


@check('ordinary acquisition: through the real main entry, wrong producer pins refuse')
def _():
    completed, evidence, evidence_root, state = acq_main_e2e(
        'acq-main-wrongstamp', stamp={'install_identity': 'local:v0:1:1'})
    try:
        if not EVIDENCE_ROOT_PRESENT:
            # No committed evidence root here: the real claim refuses before any
            # child. The wrong-identity refusal itself is proven by the
            # in-process control over the same route code above.
            assert completed.returncode == 2, (completed.returncode, completed.stdout)
            assert json.loads(completed.stdout)['code'] == 'evidence-root'
            assert not evidence.exists()
            return
        assert completed.returncode == 2, (completed.returncode, completed.stdout)
        assert json.loads(completed.stdout)['code'] == 'handoff-missing'
        occurrence = json.loads((evidence / 'occurrence.json').read_text())
        assert occurrence['status'] == 'handoff-missing' and occurrence['spent'] is True
        ups = [argv for _, argv in state['log'] if argv[2:] == ['up', '--detach']]
        assert len(ups) == 1, 'a wrong-identity output stops the window before modern'
    finally:
        shutil.rmtree(evidence_root, ignore_errors=True)  # only exists when the root does


@check('ordinary acquisition: through the real main entry, a failed legacy never starts modern')
def _():
    completed, evidence, evidence_root, state = acq_main_e2e('acq-main-legacy-fails',
                                                             fail_up=('legacy',))
    try:
        if not EVIDENCE_ROOT_PRESENT:
            # No committed evidence root here: the real claim refuses before any
            # child. The legacy-failure ordering itself is proven by the
            # in-process control over the same route code above.
            assert completed.returncode == 2, (completed.returncode, completed.stdout)
            assert json.loads(completed.stdout)['code'] == 'evidence-root'
            assert not evidence.exists()
            return
        assert completed.returncode == 2, (completed.returncode, completed.stdout)
        assert json.loads(completed.stdout)['code'] == 'child-failed'
        ups = [argv for _, argv in state['log'] if argv[2:] == ['up', '--detach']]
        assert len(ups) == 1, 'modern must not start'
        producer_out = ROOT / 'acq-main-legacy-fails' / 'producer' / '.effigy'
        assert not producer_out.exists() or not list(
            producer_out.rglob(acq.ARTIFACT_NAME)), 'a failed producer publishes nothing'
    finally:
        shutil.rmtree(evidence_root, ignore_errors=True)  # only exists when the root does


@check('ordinary acquisition: the binding refuses malformed, duplicate and unknown forms')
def _():
    folder = ROOT / 'acq-binding-shapes'
    folder.mkdir(parents=True, exist_ok=True)
    manifest = acquisition_manifest()
    pins = {key: manifest['routes'][acq.ROUTE][key] for key in sorted(acq.PIN_KEYS)}
    good = {'schema': core.BINDING_SCHEMA, 'route': acq.ROUTE, 'pins': pins,
            'consumed_windows': []}

    def run_with(raw, sha, request_sha=SHA, transport=None):
        path = folder / 'binding.json'
        path.write_bytes(raw)
        argv = [acq.ROUTE, '--request', str(folder / 'request.json'), '--sha256', request_sha,
                '--evidence-dir', str(folder / 'evidence'),
                '--binding', str(path), '--binding-sha256',
                sha if sha is not None else core.sha256_bytes(raw)]
        return proof_entry.run(argv, roots=(str(ROOT),), now_ns=NOW,
                               git=acquisition_git(manifest), transport=transport)

    expect_code('binding-schema', lambda: run_with(
        json.dumps(dict(good, schema='other.schema')).encode(), None))
    expect_code('binding-route', lambda: run_with(
        json.dumps(dict(good, route='cbs')).encode(), None))
    expect_code('binding-shape', lambda: run_with(
        json.dumps({**good, 'extra': 1}).encode(), None))
    expect_code('binding-shape', lambda: run_with(
        json.dumps({k: v for k, v in good.items() if k != 'consumed_windows'}).encode(), None))
    expect_code('binding-shape', lambda: run_with(
        json.dumps(dict(good, consumed_windows='window')).encode(), None))
    expect_code('duplicate-key', lambda: run_with(
        b'{"schema": "%s", "schema": "%s", "route": "%s", "pins": {}, "consumed_windows": []}'
        % (core.BINDING_SCHEMA.encode(), core.BINDING_SCHEMA.encode(), acq.ROUTE.encode()), None))
    pending = copy.deepcopy(good)
    pending['pins']['consumer']['bundle']['approved_revision_prefix'] = 'pending: later'
    expect_code('route-pins-pending', lambda: run_with(json.dumps(pending).encode(), None))
    expect_code('binding-sha-mismatch', lambda: run_with(
        json.dumps(good).encode(), 'sha256:' + '0' * 64))
    expect_code('binding-sha-mismatch', lambda: run_with(
        json.dumps(good).encode(), 'not-a-digest'))
    assert not (folder / 'evidence').exists(), 'refusals before the claim leave no evidence'
    request = acquisition_request(manifest, ACQ_INTENT)
    raw = json.dumps(request, sort_keys=True).encode()
    (folder / 'request.json').write_bytes(raw)
    engine = AcquisitionEngine(manifest, request)
    result = run_with(json.dumps(good).encode(), None, request_sha=core.sha256_bytes(raw),
                      transport=engine)
    assert result['status'] == 'passed', result


@check('ordinary acquisition: a binding with the wrong pin set or cross-route pins refuses')
def _():
    manifest = acquisition_manifest()
    pins = {key: manifest['routes'][acq.ROUTE][key] for key in sorted(acq.PIN_KEYS)}
    good = {'schema': core.BINDING_SCHEMA, 'route': acq.ROUTE, 'pins': pins,
            'consumed_windows': []}
    missing_env = copy.deepcopy(good)
    del missing_env['pins']['environments']
    expect_code('binding-pins', lambda: acq.validate_route_pins(missing_env['pins']))
    extra = copy.deepcopy(good)
    extra['pins']['candidate'] = {'binary_sha256': SHA}
    expect_code('binding-pins', lambda: acq.validate_route_pins(extra['pins']))
    modern_extra = copy.deepcopy(good)
    modern_extra['pins']['modern']['window'] = {}
    expect_code('binding-pins', lambda: acq.validate_route_pins(modern_extra['pins']))
    assert acq.validate_route_pins(good['pins']) is None


def main():
    for line in PASSED:
        print('ok   ' + line)
    for line in FAILURES:
        print('FAIL ' + line)
    print('%d passed, %d failed' % (len(PASSED), len(FAILURES)))
    return 1 if FAILURES else 0


if __name__ == '__main__':
    sys.exit(main())

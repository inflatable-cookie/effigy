#!/usr/bin/env python3
"""Fail-closed controls for the r1 consumer packets (task effigy#757).

Source and fixture only. Every route is driven through its real entry boundary
(`main` or `run`) with an injected fixture transport, an injected fixture git
reader and an injected authenticated-provenance callable. The one real child
runner is exercised only against harmless `sys.executable -I -c` fixture
children, which the runner's own process-group cleanup owns. No test touches a
real engine, VM, consumer checkout, row, secret or network.

Fixtures are fresh directories under TMPDIR (`tempfile.mkdtemp`). Nothing is
written outside them.
"""
import contextlib
import copy
import importlib.util
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

DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(DIR))

import acow  # noqa: E402
import acow_battery  # noqa: E402
import cbs  # noqa: E402
import consumer_core as core  # noqa: E402

FAILURES = []
PASSED = []
NS = 10 ** 9
START = 5_000
NOW = START + 1
CAND = 'c' * 40
HEAD = '1' * 40
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


def request_for(manifest, route, prior_intent):
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
        'candidate': {'source_commit': CAND, 'binary_sha256': SHA,
                      'install_identity': {'path': '/fixture/bin/effigy',
                                           'version': 'fixture-C', 'sha256': SHA}},
        'helpers': {'head': H, 'modules': core.module_hashes()},
        'predecessor': {'receipt_sha256': SHA2, 'same_candidate_commit': CAND},
        'consumer': {
            'selected_head': HEAD,
            'relevant_inputs': copy.deepcopy(pins['consumer']['relevant_inputs']),
            'includes': copy.deepcopy(pins['consumer']['includes']),
            'bundle': {'resolved_revision': H,
                       'content_digest': pins['consumer']['bundle']['content_digest']},
        },
        'provenance': {'merged_commit': M, 'source_immutable_head': H},
        'window': {'id': WINDOW_ID[route], 'token_sha256': TOKEN,
                   'start_monotonic_ns': START, 'deadline_monotonic_ns': START + 600 * NS},
        'budgets': copy.deepcopy(pins['budgets']),
        'resources': resources,
        'prior_intent': prior_intent,
    }


def record_for(request, digest, **changes):
    record = {
        'authenticated': True,
        'request_sha256': digest,
        'merged_commit': M,
        'source_immutable_head': H,
        'candidate': copy.deepcopy(request['candidate']),
        'predecessor': {'receipt_sha256': SHA2, 'same_candidate_commit': CAND,
                        'status': 'positive'},
        'window': {'id': request['window']['id'], 'token_sha256': TOKEN},
    }
    record.update(changes)
    return record


def fake_git(manifest, route, head=HEAD, status='', blobs=None, calls=None):
    """Object ids and status only. Blobs default to the reviewed closure."""
    closure = dict(manifest['routes'][route]['consumer']['relevant_inputs'])
    closure.update(blobs or {})
    log = calls if calls is not None else []

    def git(args, cwd):
        log.append((list(args), cwd))
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
    return {table: 3 for table in acow.expected_counts(fixture_manifest())}


def cbs_counts():
    return {key: 2 for key in cbs.expected_counts(fixture_manifest())}


class FakeEngine:
    """A deterministic engine. Asserts every call runs from the pinned checkout."""

    def __init__(self, route, children, running, counts=None, containers=None,
                 volumes=None, fail=None, on_up=None):
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
        self.on_up = on_up
        self.calls = []
        self.cwd = checkout_of(route)

    def __call__(self, argv, label, cwd):
        short = label.split('-', 1)[1] if label[:3].isdigit() and label[3:4] == '-' else label
        self.calls.append((short, list(argv[1:])))
        if cwd != self.cwd:
            raise AssertionError('child not run from the pinned checkout: %r' % (cwd,))
        if short in self.fail:
            raise core.Refused(self.fail[short], short)
        text = self._output(argv, label)
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


AUTO = object()


def run_entry(module, route, request, name, *, engine=None, record_changes=None,
              auth=AUTO, git=None, manifest=None, now=NOW):
    """Drive `run` with fixture authority.

    `auth=AUTO` gives a record built from the PRISTINE fixture request, so a
    mutated request always differs from its record. `auth=None` means no
    authenticated channel at all.
    """
    folder = ROOT / name
    folder.mkdir(parents=True, exist_ok=True)
    manifest = manifest if manifest is not None else fixture_manifest()
    raw = json.dumps(request, sort_keys=True).encode()
    request_path = folder / 'request.json'
    request_path.write_bytes(raw)
    digest = core.sha256_bytes(raw)
    if auth is AUTO:
        pristine = request_for(manifest, route, request['prior_intent'])
        record = record_for(pristine, digest, **(record_changes or {}))
        auth = lambda ctx: record if ctx['request_sha256'] == digest else {'authenticated': False}  # noqa: E731
    evidence = folder / 'evidence'
    result = module.run(str(request_path), str(evidence), authenticate=auth,
                        transport=engine or forbidden_transport, manifest=manifest,
                        git=git or fake_git(manifest, route), now_ns=now,
                        roots=(str(ROOT),))
    return result, evidence


def refusal_run(module, route, request, name, code, **kwargs):
    """The refusal must come from the entry boundary.

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


# ---------------------------------------------------------------- committed state


@check('committed routes refuse for pending route pins before any authority check')
def _():
    manifest = core.load_manifest()
    for route in ('acow', 'cbs'):
        expect_code('route-pins-pending', lambda: core.require_route_frozen(manifest, route))


@check('fixture manifest is fully concrete, so only the control under test refuses')
def _():
    assert core.pending_pins(fixture_manifest()) == []


@check('unarmed CLI refuses ordinary invocation before any child or read')
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


@check('committed proof entry refuses a well-formed request at route-pins-pending')
def _():
    folder = ROOT / 'committed'
    folder.mkdir(parents=True, exist_ok=True)
    request_path = folder / 'request.json'
    request_path.write_text(json.dumps(request_for(fixture_manifest(), 'acow',
                                                   {'hybrid': True, 'stack': False})))
    original = subprocess.Popen
    subprocess.Popen = no_child
    out = io.StringIO()
    try:
        with contextlib.redirect_stdout(out):
            assert acow.main(['--request', str(request_path),
                              '--evidence-dir', str(folder / 'evidence')]) == 2
    finally:
        subprocess.Popen = original
    assert json.loads(out.getvalue())['code'] == 'route-pins-pending'
    assert not (folder / 'evidence').exists()


@check('armed entry without an authenticated channel refuses before any read')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    refusal_run(acow, 'acow', request, 'no-channel', 'nucleus-provenance-unavailable',
                auth=None, manifest=manifest, engine=forbidden_transport)


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


# ---------------------------------------------------------------- provenance and authority


@check('authenticated record must name this request and nothing else')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    refusal_run(acow, 'acow', request, 'other-digest', 'provenance-mismatch',
                auth=lambda ctx: record_for(request, 'sha256:' + '0' * 64),
                engine=forbidden_transport)
    refusal_run(acow, 'acow', request, 'unauth', 'nucleus-provenance-unavailable',
                auth=lambda ctx: {'authenticated': False}, engine=forbidden_transport)
    refusal_run(acow, 'acow', request, 'wrong-merge', 'provenance-mismatch',
                record_changes={'merged_commit': 'a' * 40}, engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', {'hybrid': True, 'stack': False},
                                      lambda r: r['provenance'].update(merged_commit='b' * 40)),
                'request-merge', 'provenance-mismatch', engine=forbidden_transport)


@check('wrong or stale candidate C refuses before any child')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: None), 'stale-c',
                'candidate-mismatch', record_changes={'candidate': {
                    'source_commit': 'a' * 40, 'binary_sha256': SHA,
                    'install_identity': {'path': '/fixture/bin/effigy', 'version': 'fixture-C',
                                         'sha256': SHA}}}, engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['candidate'].update(
        binary_sha256='sha256:' + '2' * 64)), 'request-binary', 'candidate-mismatch',
        engine=forbidden_transport)


@check('stale positive624, receipt for another C, and failed predecessor refuse')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['predecessor'].update(
        receipt_sha256='sha256:' + '3' * 64)), 'stale-receipt', 'predecessor-mismatch',
        engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: None), 'other-c-receipt',
                'predecessor-mismatch', record_changes={'predecessor': {
                    'receipt_sha256': SHA2, 'same_candidate_commit': 'a' * 40,
                    'status': 'positive'}}, engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: None), 'failed-predecessor',
                'predecessor-not-positive', record_changes={'predecessor': {
                    'receipt_sha256': SHA2, 'same_candidate_commit': CAND,
                    'status': 'failed'}}, engine=forbidden_transport)


@check('helper head, on-disk module hashes and window binding must match the record')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['helpers'].update(
        head='a' * 40)), 'helper-head', 'helper-mismatch', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['helpers'].update(
        modules={name: SHA for name in core.ENTRY_MODULES})), 'helper-modules',
        'helper-mismatch', engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['window'].update(
        token_sha256='sha256:' + '4' * 64)), 'window-token', 'window-mismatch',
        engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['window'].update(
        token_sha256='not-a-digest')), 'window-missing', 'window-missing',
        record_changes={'window': {'id': WINDOW_ID['acow'], 'token_sha256': 'not-a-digest'}},
        engine=forbidden_transport)


@check('window bounds, expiry and one-use refuse before any child')
def _():
    base = {'hybrid': True, 'stack': False}
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: r['window'].update(
        deadline_monotonic_ns=START + 10_000 * NS)), 'window-span', 'window-bounds',
        engine=forbidden_transport)
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: None), 'window-expired',
                'window-expired', now=START + 600 * NS + 1, engine=forbidden_transport)
    manifest = fixture_manifest()
    manifest['consumed_windows'] = [WINDOW_ID['acow']]
    refusal_run(acow, 'acow', mutated('acow', base, lambda r: None), 'window-consumed',
                'window-consumed', manifest=manifest, engine=forbidden_transport)


@check('budgets above the route caps refuse before any child')
def _():
    refusal_run(acow, 'acow', mutated('acow', {'hybrid': True, 'stack': False},
                                      lambda r: r['budgets'].update(stdout_cap_bytes=10 ** 9)),
                'budget', 'budget-exceeded', engine=forbidden_transport)


@check('resource roles and kinds must be the pinned set')
def _():
    refusal_run(acow, 'acow', mutated('acow', {'hybrid': True, 'stack': False},
                                      lambda r: r['resources'].pop()), 'resource-set',
                'resource-mismatch', engine=forbidden_transport)


@check('a symlinked consumer checkout refuses before any read')
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


@check('include, closure and bundle drift refuse before any child')
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
    expect_code('git-not-allowlisted', lambda: core.default_git(['show', 'HEAD:effigy.toml'], str(ROOT)))
    expect_code('git-not-allowlisted', lambda: core.default_git(['cat-file', '-p', 'HEAD'], str(ROOT)))


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
                        {'hybrid': True, 'stack': False}, fail={'container-up': 'child-failed'})
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


@check('a second run into an existing evidence directory refuses and keeps the first record')
def _():
    manifest = fixture_manifest()
    request = request_for(manifest, 'cbs', {'dev': False})
    first = FakeEngine('cbs', ROOT / 'twice' / 'children', {'dev': False})
    run_entry(cbs, 'cbs', request, 'twice', engine=first, manifest=manifest)
    before = (ROOT / 'twice' / 'evidence' / 'occurrence.json').read_text()
    second = FakeEngine('cbs', ROOT / 'twice-2' / 'children', {'dev': False})
    expect_code('evidence-exists', lambda: cbs.run(
        str(ROOT / 'twice' / 'request.json'), str(ROOT / 'twice' / 'evidence'),
        authenticate=lambda ctx: record_for(request, core.sha256_bytes(
            (ROOT / 'twice' / 'request.json').read_bytes())),
        transport=second, manifest=manifest, git=fake_git(manifest, 'cbs'),
        now_ns=NOW, roots=(str(ROOT),)))
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


@check('the pinned checkout reaches the child as its working directory')
def _():
    out = ROOT / 'cwd-probe'
    rec = core.run_child([sys.executable, '-I', '-c', 'import os; print(os.getcwd())'],
                         timeout_s=30, stdout_cap=4096, stderr_cap=4096, out_dir=out,
                         label='cwd', cwd=checkout_of('acow'))
    assert Path(rec['streams']['stdout']['path']).read_text().strip() == checkout_of('acow')


# ---------------------------------------------------------------- positive fixtures


@check('fixture-positive Acow: prior-stopped stack started, prior-running hybrid stays')
def _():
    manifest = fixture_manifest()
    engine = FakeEngine('acow', ROOT / 'acow-positive' / 'children',
                        {'hybrid': True, 'stack': False})
    request = request_for(manifest, 'acow', {'hybrid': True, 'stack': False})
    result, evidence = run_entry(acow, 'acow', request, 'acow-positive', engine=engine,
                                 manifest=manifest)
    assert result['status'] == 'passed', result
    assert result['started'] == ['stack'], result
    labels = engine.labels()
    assert 'pre-hybrid' in labels and 'pre-stack' in labels, labels
    assert labels.index('hybrid-1-up') < labels.index('container-up') < labels.index('hybrid-2-up')
    assert 'rollback-stack' in labels and 'rollback-hybrid' not in labels, labels
    assert engine.running == {'hybrid': True, 'stack': False}
    occurrence = json.loads((evidence / 'occurrence.json').read_text())
    assert occurrence['status'] == 'passed' and occurrence['spent'] is False
    assert (evidence / 'binding.json').exists()


@check('fixture-positive CBS: detached app-shell, forward, counts, scoped rollback')
def _():
    manifest = fixture_manifest()
    engine = FakeEngine('cbs', ROOT / 'cbs-positive' / 'children', {'dev': False})
    request = request_for(manifest, 'cbs', {'dev': False})
    result, _ = run_entry(cbs, 'cbs', request, 'cbs-positive', engine=engine, manifest=manifest)
    assert result['status'] == 'passed' and result['started'] == ['dev'], result
    assert engine.running == {'dev': False}
    assert 'rollback-dev' in engine.labels(), engine.labels()


@check('the production runner reports over-cap streams, deadline breach and failed exits')
def _():
    out = ROOT / 'runner'
    expect_code('over-cap-stream', lambda: core.run_child(
        [sys.executable, '-I', '-c', 'import sys; sys.stdout.write("x" * 5000)'],
        timeout_s=30, stdout_cap=100, stderr_cap=100, out_dir=out, label='overcap'))
    record = json.loads((out / 'overcap.record.json').read_text())
    assert record['streams']['stdout']['truncated'] is True
    assert record['streams']['stdout']['bytes'] == 100
    started = time.monotonic()
    expect_code('deadline-exceeded', lambda: core.run_child(
        [sys.executable, '-I', '-c', 'import time; time.sleep(60)'],
        timeout_s=1, stdout_cap=100, stderr_cap=100, out_dir=out, label='deadline'))
    assert time.monotonic() - started < 30
    assert json.loads((out / 'deadline.record.json').read_text())['exit'] < 0
    expect_code('child-failed', lambda: core.run_child(
        [sys.executable, '-I', '-c', 'import sys; sys.exit(3)'],
        timeout_s=30, stdout_cap=100, stderr_cap=100, out_dir=out, label='fail'))
    assert json.loads((out / 'fail.record.json').read_text())['exit'] == 3


@check('the runner keeps stdout and stderr separate with the waited exit')
def _():
    rec = core.run_child(
        [sys.executable, '-I', '-c',
         'import sys; sys.stdout.write("out-bytes"); sys.stderr.write("err-bytes")'],
        timeout_s=30, stdout_cap=1024, stderr_cap=1024, out_dir=ROOT / 'runner-pass',
        label='pass')
    assert rec['exit'] == 0
    assert Path(rec['streams']['stdout']['path']).read_bytes() == b'out-bytes'
    assert Path(rec['streams']['stderr']['path']).read_bytes() == b'err-bytes'


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
    expect_code('duplicate-key', lambda: core.load_request(str(dup)))
    big = ROOT / 'big' / 'request.json'
    big.parent.mkdir(parents=True, exist_ok=True)
    big.write_text('x' * (core.MAX_REQUEST_BYTES + 1))
    expect_code('file-too-large', lambda: core.load_request(str(big)))
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


def main():
    for line in PASSED:
        print('ok   ' + line)
    for line in FAILURES:
        print('FAIL ' + line)
    print('%d passed, %d failed' % (len(PASSED), len(FAILURES)))
    return 1 if FAILURES else 0


if __name__ == '__main__':
    sys.exit(main())

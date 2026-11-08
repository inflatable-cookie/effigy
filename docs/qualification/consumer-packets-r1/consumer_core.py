"""Shared core for the r1 consumer proof packets (Acow and CBS). Source only.

This module holds the fail-closed machinery both route entries use: bounded
strict JSON reads, the pinned manifest, the hash-pinned prospective request,
the Nucleus provenance seam, a fixed-argv child runner with separate captured
streams and a waited exit, and the counts and rollback rules. It performs no
effect by itself. The committed manifest pins nothing yet, so every route
refuses at `candidate-unfrozen` before a transport is called.

Historical reuse is narrow: five pure readers from the Acow r1 packet
(`acow-ordinary-compose-5f466914/common.py`) are loaded by file path under a
distinct module name. That packet is never edited and its own boundaries stay
unchanged.
"""
import hashlib
import importlib.util
import json
import os
import re
import signal
import stat
import subprocess
import threading
import time
from pathlib import Path

DIR = Path(__file__).resolve().parent
MANIFEST = DIR / 'manifest-r1.json'
MANIFEST_SCHEMA = 'effigy.qualification.consumer-packets.r1'
REQUEST_SCHEMA = 'effigy.qualification.consumer-packet.request.v1'
PENDING = 'pending'
MAX_REQUEST_BYTES = 65536
MAX_MANIFEST_BYTES = 262144
KILL_GRACE_S = 5
PUMP_JOIN_S = 10
HEX40 = re.compile(r'[0-9a-f]{40}')
HEX64 = re.compile(r'[0-9a-f]{64}')
SHA256_TAG = re.compile(r'sha256:[0-9a-f]{64}')
ID_RE = re.compile(r'[A-Za-z0-9][A-Za-z0-9._:-]{2,127}')
REQUEST_KEYS = frozenset({
    'schema', 'route', 'candidate', 'helpers', 'predecessor', 'consumer',
    'provenance', 'window', 'budgets', 'resources', 'prior_intent',
})


class Refused(Exception):
    """A fail-closed refusal. `code` is stable; `detail` is for the operator."""

    def __init__(self, code, detail=''):
        super().__init__('%s: %s' % (code, detail) if detail else code)
        self.code = code
        self.detail = detail


def _historical():
    path = DIR.parent / 'acow-ordinary-compose-5f466914' / 'common.py'
    spec = importlib.util.spec_from_file_location('acow_r1_common', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_HIST = _historical()


def historical(name, *args):
    """Call one pure reader from the reviewed Acow r1 packet, refusing alike."""
    try:
        return getattr(_HIST, name)(*args)
    except _HIST.Refused as exc:
        raise Refused('historical-reader', '%s: %s' % (name, exc))


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for chunk in iter(lambda: handle.read(65536), b''):
            digest.update(chunk)
    return 'sha256:' + digest.hexdigest()


def monotonic_now():
    return time.monotonic_ns()


def read_bounded_json(path, max_bytes):
    """Regular, non-symlink, size-bounded JSON with no duplicate keys."""
    try:
        st = os.lstat(path)
    except OSError as exc:
        raise Refused('file-unreadable', '%s: %s' % (path, exc))
    if not stat.S_ISREG(st.st_mode):
        raise Refused('not-regular-file', str(path))
    if st.st_size > max_bytes:
        raise Refused('file-too-large', '%s (%d bytes)' % (path, st.st_size))
    raw = Path(path).read_bytes()

    def no_duplicates(pairs):
        obj = {}
        for key, value in pairs:
            if key in obj:
                raise Refused('duplicate-key', key)
            obj[key] = value
        return obj

    try:
        return json.loads(raw.decode('utf-8'), object_pairs_hook=no_duplicates)
    except (UnicodeDecodeError, ValueError) as exc:
        raise Refused('malformed-json', '%s: %s' % (path, exc))


def pending_pins(node, prefix=''):
    """Dotted paths of every pin still `pending`. Empty means fully frozen."""
    found = []
    if isinstance(node, dict):
        for key in sorted(node):
            found += pending_pins(node[key], prefix + key + '.')
    elif isinstance(node, list):
        for index, item in enumerate(node):
            found += pending_pins(item, '%s%d.' % (prefix, index))
    elif isinstance(node, str) and (node == PENDING or node.startswith(PENDING + ':')):
        found.append(prefix.rstrip('.'))
    return found


def load_manifest(path=MANIFEST):
    manifest = read_bounded_json(path, MAX_MANIFEST_BYTES)
    if manifest.get('schema') != MANIFEST_SCHEMA:
        raise Refused('manifest-schema', str(manifest.get('schema')))
    return manifest


def require_frozen(manifest, route):
    """Refuse while any pin the route depends on is still pending.

    Frozen means the candidate, helpers, predecessor, provenance and this
    route's own pins are all concrete. A placeholder never counts as frozen.
    """
    scope = {key: manifest[key] for key in
             ('candidate', 'helpers', 'predecessor', 'provenance')}
    scope['route'] = manifest['routes'][route]
    pending = pending_pins(scope)
    if pending:
        raise Refused('candidate-unfrozen',
                      '%d pins pending, first: %s' % (len(pending), pending[0]))


def check_exact_keys(obj, keys, what):
    if not isinstance(obj, dict) or set(obj) != set(keys):
        raise Refused('request-shape', '%s keys are not the reviewed set' % what)


def validate_request_shape(request, route):
    """Strict shape only. Pins are compared later, after the frozen check."""
    check_exact_keys(request, REQUEST_KEYS, 'request')
    if request['schema'] != REQUEST_SCHEMA:
        raise Refused('request-schema', str(request['schema']))
    if request['route'] != route:
        raise Refused('request-route', 'expected %s' % route)
    for key in ('candidate', 'helpers', 'predecessor', 'consumer',
                'provenance', 'window', 'budgets', 'prior_intent'):
        if not isinstance(request[key], dict):
            raise Refused('request-shape', key + ' is not an object')
    if not isinstance(request['resources'], list):
        raise Refused('request-shape', 'resources is not a list')
    return request


def compare_pins(request, manifest, route):
    """Exact technical bindings: candidate, helpers, predecessor, consumer."""
    cand = request['candidate']
    pinned = manifest['candidate']
    check_exact_keys(request['consumer'], {'selected_head', 'relevant_inputs',
                                           'includes', 'bundle'}, 'consumer')
    if cand.get('source_commit') != pinned['source_commit']:
        raise Refused('candidate-mismatch', 'source commit is not C')
    if cand.get('binary_sha256') != pinned['binary_sha256']:
        raise Refused('candidate-mismatch', 'binary hash is not C')
    if cand.get('install_identity') != pinned['install_identity']:
        raise Refused('candidate-mismatch', 'installed identity differs')
    helpers = request['helpers']
    if helpers.get('head') != manifest['helpers']['head']:
        raise Refused('helper-mismatch', 'helper source head is not pinned')
    if helpers.get('modules') != manifest['helpers']['modules']:
        raise Refused('helper-mismatch', 'helper module hashes differ')
    pred = request['predecessor']
    if pred.get('receipt_sha256') != manifest['predecessor']['receipt_sha256']:
        raise Refused('predecessor-mismatch', 'positive624 receipt is not pinned')
    if pred.get('same_candidate_commit') != pinned['source_commit']:
        raise Refused('predecessor-mismatch', 'receipt is not for this C')
    consumer = request['consumer']
    pins = manifest['routes'][route]['consumer']
    if not HEX40.fullmatch(str(consumer.get('selected_head', ''))):
        raise Refused('consumer-mismatch', 'selected head is not a commit')
    if consumer.get('relevant_inputs') != pins['relevant_inputs']:
        raise Refused('input-drift', 'relevant tracked input closure differs')
    if consumer.get('includes') != pins['includes']:
        raise Refused('input-drift', 'declared include set differs')
    bundle = consumer.get('bundle', {})
    if bundle.get('resolved_revision') != pins['bundle']['resolved_revision']:
        raise Refused('bundle-drift', 'resolved revision differs')
    if bundle.get('content_digest') != pins['bundle']['content_digest']:
        raise Refused('bundle-drift', 'content digest differs')


def check_window(request, manifest, route, now_ns):
    """Absolute monotonic bounds, one use, and the route's maximum span."""
    window = request['window']
    check_exact_keys(window, {'id', 'token_sha256', 'start_monotonic_ns',
                              'deadline_monotonic_ns'}, 'window')
    if not ID_RE.fullmatch(str(window['id'])):
        raise Refused('window-missing', 'window id is malformed')
    if window['id'] in manifest.get('consumed_windows', []):
        raise Refused('window-consumed', window['id'])
    if not SHA256_TAG.fullmatch(str(window['token_sha256'])):
        raise Refused('window-missing', 'one-use token digest is missing')
    start = window['start_monotonic_ns']
    deadline = window['deadline_monotonic_ns']
    if not (isinstance(start, int) and isinstance(deadline, int)):
        raise Refused('window-missing', 'monotonic bounds are not integers')
    span_ns = manifest['routes'][route]['max_window_seconds'] * 1_000_000_000
    if deadline <= start or deadline - start > span_ns:
        raise Refused('window-bounds', 'deadline outside the route span')
    if now_ns >= deadline:
        raise Refused('window-expired', 'deadline already passed')


def check_budgets(request, manifest, route):
    caps = manifest['routes'][route]['budgets']
    budgets = request['budgets']
    check_exact_keys(budgets, set(caps), 'budgets')
    for key, cap in caps.items():
        value = budgets[key]
        if not isinstance(value, int) or value < 1 or value > cap:
            raise Refused('budget-exceeded', '%s outside 1..%d' % (key, cap))


def check_resources(request, manifest, route):
    """Exact engine identity for every resource the window may touch."""
    pinned = manifest['routes'][route]['resources']
    listed = {item.get('role'): item for item in request['resources']
              if isinstance(item, dict)}
    if set(listed) != set(pinned) or len(listed) != len(request['resources']):
        raise Refused('resource-mismatch', 'resource set is not the pinned set')
    for role, want in pinned.items():
        got = {key: value for key, value in listed[role].items() if key != 'role'}
        if got != want:
            raise Refused('resource-mismatch', 'identity differs for ' + role)


def check_prior_intent(request, observed_running):
    """Running-state intent recorded before effect must match the engine."""
    intent = request['prior_intent']
    if set(intent) != set(observed_running):
        raise Refused('prior-intent', 'environment set differs from intent')
    for name, running in intent.items():
        if not isinstance(running, bool) or observed_running[name] != running:
            raise Refused('prior-intent', 'running state changed for ' + name)


def verify_provenance(request, manifest, authenticate):
    """Nucleus exact merged-commit and source-immutable proof-entry precondition.

    File and environment values are data, never authorization. `authenticate`
    is the Nucleus proof-entry channel. Source-only code has none, so the
    committed entries refuse here.
    """
    if authenticate is None:
        raise Refused('nucleus-provenance-unavailable',
                      'no authenticated proof-entry channel in this source')
    record = authenticate(request['provenance'])
    if not isinstance(record, dict) or record.get('authenticated') is not True:
        raise Refused('nucleus-provenance-unavailable', 'record not authenticated')
    pinned = manifest['provenance']
    for key in ('merged_commit', 'source_immutable_head'):
        if record.get(key) != pinned[key] or request['provenance'].get(key) != pinned[key]:
            raise Refused('provenance-mismatch', key + ' is not the pinned value')
    return record


def validate_request(request, manifest, route, authenticate, now_ns):
    """Ordered gate: shape, frozen pins, provenance, pins, window, budgets."""
    validate_request_shape(request, route)
    require_frozen(manifest, route)
    verify_provenance(request, manifest, authenticate)
    compare_pins(request, manifest, route)
    check_window(request, manifest, route, now_ns)
    check_budgets(request, manifest, route)
    check_resources(request, manifest, route)
    return request


def check_argv(argv, allowed_tails, binary_prefix=None):
    """Fixed allowlist: argv[0] absolute, the rest an exact allowed tail."""
    if not argv or not os.path.isabs(argv[0]):
        raise Refused('argv-not-pinned', 'argv[0] must be an absolute path')
    if binary_prefix is not None and argv[0] != binary_prefix:
        raise Refused('argv-not-pinned', 'binary is not the pinned install')
    if tuple(argv[1:]) not in allowed_tails:
        raise Refused('argv-not-allowlisted', ' '.join(argv[1:4]))
    return list(argv)


def _pump(pipe, buffer, cap, overflow, group):
    while True:
        chunk = pipe.read(4096)
        if not chunk:
            break
        room = cap - len(buffer)
        if len(chunk) > room:
            buffer.extend(chunk[:max(room, 0)])
            overflow.set()
            _kill_group(group)
            break
        buffer.extend(chunk)
    pipe.close()


def _kill_group(pgid):
    try:
        os.killpg(pgid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def run_child(argv, *, timeout_s, stdout_cap, stderr_cap, out_dir, label,
              env=None, cwd=None):
    """One fixed child: separate captured streams, waited exit, owned cleanup.

    The child leads its own session, so cleanup kills only its process group,
    by the PID this runner started. Over-cap output, a deadline breach or an
    unreapable child refuses. The record is written before the refusal.
    """
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            start_new_session=True)
    pgid = proc.pid
    out_buf, err_buf = bytearray(), bytearray()
    overflow = {'stdout': threading.Event(), 'stderr': threading.Event()}
    timed_out = False
    started = time.monotonic_ns()
    pumps = [
        threading.Thread(target=_pump, args=(proc.stdout, out_buf, stdout_cap,
                                             overflow['stdout'], pgid)),
        threading.Thread(target=_pump, args=(proc.stderr, err_buf, stderr_cap,
                                             overflow['stderr'], pgid)),
    ]
    for thread in pumps:
        thread.start()
    try:
        rc = proc.wait(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        timed_out = True
        _kill_group(pgid)
        try:
            rc = proc.wait(timeout=KILL_GRACE_S)
        except subprocess.TimeoutExpired:
            raise Refused('child-unreaped', 'pid %d survived SIGKILL' % pgid)
    for thread in pumps:
        thread.join(PUMP_JOIN_S)
    _kill_group(pgid)
    record = {
        'label': label,
        'argv_sha256': 'sha256:' + hashlib.sha256(
            '\0'.join(argv).encode()).hexdigest(),
        'exit': rc,
        'duration_ns': time.monotonic_ns() - started,
        'streams': {},
    }
    for name, buf in (('stdout', out_buf), ('stderr', err_buf)):
        path = out_dir / ('%s.%s' % (label, name))
        path.write_bytes(bytes(buf))
        record['streams'][name] = {
            'path': str(path), 'bytes': len(buf),
            'sha256': sha256_file(path), 'truncated': overflow[name].is_set(),
        }
    (out_dir / (label + '.record.json')).write_text(
        json.dumps(record, indent=2, sort_keys=True) + '\n')
    if any(event.is_set() for event in overflow.values()):
        raise Refused('over-cap-stream', label)
    if timed_out:
        raise Refused('deadline-exceeded', label)
    if rc != 0:
        raise Refused('child-failed', '%s exit %d' % (label, rc))
    return record


def counts_match(observed, expected):
    """Named-table cardinality only. Any missing, extra or unequal count refuses."""
    if set(observed) != set(expected):
        raise Refused('unexpected-counts', 'table set differs from the pin')
    for table, want in sorted(expected.items()):
        if observed[table] != want:
            raise Refused('unexpected-counts', '%s: %s != %s'
                          % (table, observed[table], want))


def rollback_plan(prior_running, now_running, started_by_window):
    """Stop only environments this window started, restoring prior intent.

    Anything running before the window stays running. An environment is
    stopped only when the window started it, so a foreign or pre-existing
    environment is never touched.
    """
    plan = []
    for name in sorted(started_by_window):
        if prior_running.get(name) is False and now_running.get(name) is True:
            plan.append(name)
    return plan


def write_occurrence(evidence_dir, status, detail):
    """The occurrence record. Failed or Unknown spends the window; never reset."""
    body = {'status': status, 'detail': detail,
            'spent': status != 'passed'}
    Path(evidence_dir).mkdir(parents=True, exist_ok=True)
    (Path(evidence_dir) / 'occurrence.json').write_text(
        json.dumps(body, indent=2, sort_keys=True) + '\n')
    return body


def within_root(path, roots):
    """Evidence directories must sit under an allowed root, never a symlink."""
    resolved = Path(path)
    if not resolved.is_absolute():
        raise Refused('evidence-root', 'evidence dir must be absolute')
    for ancestor in [resolved] + list(resolved.parents):
        if ancestor.is_symlink():
            raise Refused('evidence-root', 'symlink ancestor: %s' % ancestor)
    if not any(str(resolved).startswith(str(root).rstrip('/') + '/') for root in roots):
        raise Refused('evidence-root', 'outside the allowed scratch root')
    return resolved


def load_request(path):
    return read_bounded_json(path, MAX_REQUEST_BYTES)

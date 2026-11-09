"""Shared core for the r1 consumer proof packets (Acow and CBS). Source only.

Authority model. The committed manifest holds only source-visible facts: route
constants, consumer closure blobs, count baselines, engine and identity facts,
budgets, and the one-use consumed-window list. It does not pin the candidate,
the predecessor, the helper head or the window. Those arrive in a hash-pinned
request, and an authenticated Nucleus proof-entry record must name that
request's SHA-256 and confirm the same values. File and environment values are
data, never authorization. Without an authenticated record the entries refuse
before any read of the consumer checkout or any engine.

Every child runs through `Session`: a fixed argv allowlist, the pinned consumer
checkout as cwd, and one bounded transport. Routes build no other child.
Git reads go through one fixed allowlist in `default_git` and read object ids
and status only, never tracked file contents.
"""
import hashlib
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
ENTRY_MODULES = ('consumer_core.py', 'acow.py', 'cbs.py')
GIT = '/usr/bin/git'
MAX_REQUEST_BYTES = 65536
MAX_MANIFEST_BYTES = 262144
KILL_GRACE_S = 5
PUMP_JOIN_S = 10
GIT_TIMEOUT_S = 60
ENGINE_VERBS = frozenset({'ps', 'inspect', 'volume', 'exec'})
HEX40 = re.compile(r'[0-9a-f]{40}')
HEX64 = re.compile(r'[0-9a-f]{64}')
SHA256_TAG = re.compile(r'sha256:[0-9a-f]{64}')
ID_RE = re.compile(r'[A-Za-z0-9][A-Za-z0-9._:-]{2,127}')
NAME_RE = re.compile(r'[A-Za-z0-9][A-Za-z0-9_.-]*')
REQUEST_KEYS = frozenset({
    'schema', 'route', 'candidate', 'helpers', 'predecessor', 'consumer',
    'provenance', 'window', 'budgets', 'resources', 'prior_intent',
})
RECORD_KEYS = frozenset({
    'authenticated', 'request_sha256', 'merged_commit', 'source_immutable_head',
    'candidate', 'predecessor', 'window',
})


class Refused(Exception):
    """A fail-closed refusal. `code` is stable; `detail` is for the operator."""

    def __init__(self, code, detail=''):
        super().__init__('%s: %s' % (code, detail) if detail else code)
        self.code = code
        self.detail = detail


def sha256_bytes(data):
    return 'sha256:' + hashlib.sha256(data).hexdigest()


def sha256_file(path):
    return sha256_bytes(Path(path).read_bytes())


def monotonic_now():
    return time.monotonic_ns()


def module_hashes():
    """Hashes of the entry modules on disk, compared with the request and record."""
    return {name: sha256_file(DIR / name) for name in ENTRY_MODULES}


def read_bounded_bytes(path, max_bytes):
    """Regular, non-symlink, size-bounded bytes."""
    try:
        st = os.lstat(path)
    except OSError as exc:
        raise Refused('file-unreadable', '%s: %s' % (path, exc))
    if not stat.S_ISREG(st.st_mode):
        raise Refused('not-regular-file', str(path))
    if st.st_size > max_bytes:
        raise Refused('file-too-large', '%s (%d bytes)' % (path, st.st_size))
    return Path(path).read_bytes()


def parse_json_strict(raw):
    """JSON with no duplicate keys."""
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
        raise Refused('malformed-json', str(exc))


def read_bounded_json(path, max_bytes):
    return parse_json_strict(read_bounded_bytes(path, max_bytes))


def load_request(path):
    """The request object and the SHA-256 of its exact bytes."""
    raw = read_bounded_bytes(path, MAX_REQUEST_BYTES)
    return parse_json_strict(raw), sha256_bytes(raw)


def load_manifest(path=MANIFEST):
    manifest = read_bounded_json(path, MAX_MANIFEST_BYTES)
    if manifest.get('schema') != MANIFEST_SCHEMA:
        raise Refused('manifest-schema', str(manifest.get('schema')))
    return manifest


def pending_pins(node, prefix=''):
    """Dotted paths of every pin still `pending`. Empty means concrete."""
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


def require_route_frozen(manifest, route):
    """Refuse while the route's own source pins are pending."""
    pending = pending_pins(manifest['routes'][route])
    if pending:
        raise Refused('route-pins-pending',
                      '%d pins pending, first: %s' % (len(pending), pending[0]))


def check_exact_keys(obj, keys, what):
    if not isinstance(obj, dict) or set(obj) != set(keys):
        raise Refused('request-shape', '%s keys are not the reviewed set' % what)


def validate_request_shape(request, route):
    """Strict shape only. Bindings are checked against the authenticated record."""
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


def authenticate_provenance(request, digest, route, authenticate):
    """The Nucleus proof-entry precondition: an authenticated record for THIS request.

    `authenticate` is the Nucleus proof-entry channel. It receives the route
    and the request's SHA-256 and returns a record. Source-only code has no
    such channel, so the committed entries refuse here, before any read.
    """
    if authenticate is None:
        raise Refused('nucleus-provenance-unavailable',
                      'no authenticated proof-entry record reached the entry')
    record = authenticate({'route': route, 'request_sha256': digest})
    if not isinstance(record, dict) or record.get('authenticated') is not True:
        raise Refused('nucleus-provenance-unavailable', 'record not authenticated')
    check_exact_keys(record, RECORD_KEYS, 'provenance record')
    if record['request_sha256'] != digest:
        raise Refused('provenance-mismatch', 'record names another request')
    return record


def bind_record(request, record):
    """Every candidate, predecessor, helper and window binding must match the record."""
    provenance = {'merged_commit': record['merged_commit'],
                  'source_immutable_head': record['source_immutable_head']}
    if request['provenance'] != provenance:
        raise Refused('provenance-mismatch', 'merged commit or source head differs')
    if not HEX40.fullmatch(str(record['merged_commit'])):
        raise Refused('provenance-mismatch', 'merged commit is not a commit')
    if request['helpers'] != {'head': record['source_immutable_head'],
                              'modules': module_hashes()}:
        raise Refused('helper-mismatch', 'helper head or on-disk module hashes differ')
    candidate = record['candidate']
    check_exact_keys(candidate, {'source_commit', 'binary_sha256', 'install_identity'},
                     'candidate')
    if not HEX40.fullmatch(str(candidate['source_commit'])):
        raise Refused('candidate-mismatch', 'source commit is not a commit')
    if not os.path.isabs(str(candidate['install_identity'].get('path', ''))):
        raise Refused('candidate-mismatch', 'installed path is not absolute')
    if request['candidate'] != candidate:
        raise Refused('candidate-mismatch', 'request candidate is not the record candidate')
    predecessor = record['predecessor']
    check_exact_keys(predecessor, {'receipt_sha256', 'same_candidate_commit', 'status'},
                     'predecessor')
    if predecessor['status'] != 'positive':
        raise Refused('predecessor-not-positive', str(predecessor['status']))
    if predecessor['same_candidate_commit'] != candidate['source_commit']:
        raise Refused('predecessor-mismatch', 'receipt is not for this candidate')
    if request['predecessor'] != {k: predecessor[k] for k in
                                  ('receipt_sha256', 'same_candidate_commit')}:
        raise Refused('predecessor-mismatch', 'request predecessor is not the record')
    window = record['window']
    check_exact_keys(window, {'id', 'token_sha256'}, 'record window')
    if request['window'].get('id') != window['id'] \
            or request['window'].get('token_sha256') != window['token_sha256']:
        raise Refused('window-mismatch', 'request window is not the record window')


def check_window(request, manifest, now_ns, route):
    """Absolute monotonic bounds, one use, and the route's maximum span."""
    window = request['window']
    check_exact_keys(window, {'id', 'token_sha256', 'start_monotonic_ns',
                              'deadline_monotonic_ns'}, 'window')
    if not ID_RE.fullmatch(str(window['id'])):
        raise Refused('window-missing', 'window id is malformed')
    if not SHA256_TAG.fullmatch(str(window['token_sha256'])):
        raise Refused('window-missing', 'one-use token digest is missing')
    if window['id'] in manifest.get('consumed_windows', []):
        raise Refused('window-consumed', window['id'])
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
    """The resource roles and kinds the route pins. Identities are probed at the engine."""
    pinned = manifest['routes'][route]['resources']
    listed = {}
    for item in request['resources']:
        check_exact_keys(item, {'role', 'kind', 'identity'}, 'resource')
        if item['role'] in listed:
            raise Refused('resource-mismatch', 'duplicate role ' + item['role'])
        listed[item['role']] = item
    if set(listed) != set(pinned):
        raise Refused('resource-mismatch', 'resource roles are not the pinned set')
    for role, want in pinned.items():
        if listed[role]['kind'] != want['kind']:
            raise Refused('resource-mismatch', 'kind differs for ' + role)
        if not ID_RE.fullmatch(str(listed[role]['identity'])):
            raise Refused('resource-mismatch', 'identity is malformed for ' + role)


def resource_identity(request, role):
    for item in request['resources']:
        if item['role'] == role:
            return item['identity']
    raise Refused('resource-mismatch', 'missing role ' + role)


def resolve_checkout(pinned):
    """The pinned consumer checkout: a real directory, never a symlink."""
    if os.path.islink(pinned) or not os.path.isdir(pinned):
        raise Refused('checkout-mismatch', 'pinned checkout is not a real directory')
    return os.path.realpath(pinned)


def default_git(args, cwd):
    """One fixed read: object ids and status of the pinned checkout. No content."""
    if list(args) not in (['rev-parse', 'HEAD'], ['status', '--porcelain']) \
            and not (len(args) == 2 and args[0] == 'rev-parse' and args[1].startswith('HEAD:')):
        raise Refused('git-not-allowlisted', ' '.join(args))
    result = subprocess.run([GIT, *args], cwd=cwd, capture_output=True, text=True,
                            timeout=GIT_TIMEOUT_S)
    if result.returncode != 0:
        raise Refused('input-drift', 'git read failed: ' + ' '.join(args))
    return result.stdout


def recording_git(git, reads):
    """Wrap a git reader so every read is kept for the occurrence evidence."""
    def recording(args, cwd):
        stdout = git(args, cwd)
        reads.append({'args': list(args), 'cwd': cwd, 'stdout': stdout})
        return stdout
    return recording


def write_closure(evidence_dir, reads):
    """Retain every consumer read the closure check made, before any child."""
    write_exclusive(Path(evidence_dir) / 'closure.json',
                    json.dumps({'reads': reads}, indent=2, sort_keys=True) + '\n')


def verify_consumer(git, request, manifest, route, checkout):
    """Selected clean head, declared include set, and every closure blob at that head."""
    pins = manifest['routes'][route]['consumer']
    consumer = request['consumer']
    check_exact_keys(consumer, {'selected_head', 'relevant_inputs', 'includes', 'bundle'},
                     'consumer')
    if consumer['includes'] != pins['includes']:
        raise Refused('input-drift', 'declared include set differs')
    if consumer['relevant_inputs'] != pins['relevant_inputs']:
        raise Refused('input-drift', 'request closure differs from the reviewed closure')
    bundle = consumer['bundle']
    check_exact_keys(bundle, {'resolved_revision', 'content_digest'}, 'bundle')
    if bundle['content_digest'] != pins['bundle']['content_digest']:
        raise Refused('bundle-drift', 'bundle content digest differs from the approved target')
    revision = str(bundle['resolved_revision'])
    if not HEX40.fullmatch(revision) or not revision.startswith(pins['bundle']['approved_revision_prefix']):
        raise Refused('bundle-drift', 'resolved bundle revision is not the approved target')
    selected = consumer['selected_head']
    if not HEX40.fullmatch(str(selected)):
        raise Refused('consumer-mismatch', 'selected head is not a commit')
    if git(['rev-parse', 'HEAD'], checkout).strip() != selected:
        raise Refused('consumer-mismatch', 'checkout HEAD is not the selected head')
    if git(['status', '--porcelain'], checkout).strip():
        raise Refused('consumer-dirty', 'checkout has uncommitted changes')
    for path, blob in sorted(pins['relevant_inputs'].items()):
        if git(['rev-parse', 'HEAD:' + path], checkout).strip() != blob:
            raise Refused('input-drift', 'blob differs at selected head: ' + path)


def check_prior_intent(request, observed_running):
    """Running-state intent recorded before effect must match the engine."""
    intent = request['prior_intent']
    if set(intent) != set(observed_running):
        raise Refused('prior-intent', 'environment set differs from intent')
    for name, running in intent.items():
        if not isinstance(running, bool) or observed_running[name] != running:
            raise Refused('prior-intent', 'running state changed for ' + name)


def counts_match(observed, expected):
    """Named-table cardinality only. Any missing, extra or unequal count refuses."""
    if set(observed) != set(expected):
        raise Refused('unexpected-counts', 'table set differs from the pin')
    for table, want in sorted(expected.items()):
        if observed[table] != want:
            raise Refused('unexpected-counts', '%s: %s != %s'
                          % (table, observed[table], want))


def rollback_plan(prior_running, now_running, started_by_window):
    """Stop only environments this window started. Unknown state counts as running.

    An environment is stopped unless it was observed not running afterwards, so
    an environment whose status was never read cannot be left running. Anything
    running before the window is never named here.
    """
    return [name for name in sorted(started_by_window)
            if prior_running.get(name) is False and now_running.get(name) is not False]


def engine_argv(engine, *verb):
    return [engine['colima_binary'], 'nerdctl', '-p', engine['profile'], '--', *verb]


def parse_running(text):
    try:
        body = json.loads(text)
    except ValueError:
        raise Refused('status-unparsed', 'status is not JSON')
    if not isinstance(body, dict) or not isinstance(body.get('running'), bool):
        raise Refused('status-unparsed', 'running state is not a bool')
    return body['running']


def parse_ids(text):
    rows = [line for line in text.splitlines() if line != '']
    if any(not HEX64.fullmatch(row) for row in rows) or len(rows) != len(set(rows)):
        raise Refused('resource-mismatch', 'container inventory is malformed or duplicated')
    return sorted(rows)


def parse_names(text):
    rows = [line for line in text.splitlines() if line != '']
    if any(not NAME_RE.fullmatch(row) for row in rows) or len(rows) != len(set(rows)):
        raise Refused('resource-mismatch', 'volume inventory is malformed or duplicated')
    return sorted(rows)


def parse_mounts(text):
    names = []
    for line in text.splitlines():
        if line.startswith('volume '):
            names.append(line[len('volume '):])
    return sorted(names)


def stdout_text(record):
    return Path(record['streams']['stdout']['path']).read_text()


def protected_capture(session, engine):
    """Container and volume identities now. Used before effect and after rollback."""
    containers = parse_ids(stdout_text(session.engine_call(
        engine_argv(engine, 'ps', '-a', '--no-trunc', '--format', '{{.ID}}'),
        'capture-containers')))
    volumes = parse_names(stdout_text(session.engine_call(
        engine_argv(engine, 'volume', 'ls', '--format', '{{.Name}}'),
        'capture-volumes')))
    return {'containers': set(containers), 'volumes': set(volumes)}


def identity_check(session, engine, identity, request, container_role, volume_role):
    """Bind the actual container and volume the window will touch. Read-only probes."""
    container_id = resource_identity(request, container_role)
    volume_name = resource_identity(request, volume_role)
    pinned_volume = identity.get('volume')
    if pinned_volume is not None and volume_name != pinned_volume:
        raise Refused('resource-mismatch', 'volume identity is not the pinned volume')
    ids = parse_ids(stdout_text(session.engine_call(
        engine_argv(engine, 'ps', '-a', '--no-trunc', '--format', '{{.ID}}',
                    '--filter', 'label=com.docker.compose.project=' + identity['project'],
                    '--filter', 'label=com.docker.compose.service=' + identity['service']),
        'probe-' + container_role)))
    if ids != [container_id]:
        raise Refused('resource-mismatch', 'container is foreign, replaced or absent')
    mounts = parse_mounts(stdout_text(session.engine_call(
        engine_argv(engine, 'inspect', '--format',
                    '{{range .Mounts}}{{.Type}} {{.Name}}\n{{end}}', container_id),
        'probe-' + container_role + '-mounts')))
    if volume_name not in mounts:
        raise Refused('resource-mismatch', 'container does not mount the pinned volume')
    return container_id


def protected_check(baseline, after, shared_container, shared_volume, never_touch):
    """Shared identities present before, present after, and no volume lost."""
    if shared_container not in baseline['containers'] or shared_volume not in baseline['volumes']:
        raise Refused('protected-missing', 'shared MariaDB identity is not present')
    for name in never_touch:
        if name not in baseline['volumes']:
            raise Refused('protected-missing', 'protected volume absent: ' + name)
    if shared_container not in after['containers'] or not baseline['volumes'] <= after['volumes']:
        raise Refused('protected-identity-changed', 'a protected identity was removed')


def volumes_kept(baseline, after):
    """Named volumes are never removed by a window; containers may be the window's own."""
    if not baseline['volumes'] <= after['volumes']:
        raise Refused('protected-identity-changed', 'a named volume was removed')


def within_root(path, roots):
    """Evidence directories sit under an allowed root, with no symlink ancestor."""
    resolved = Path(path)
    if not resolved.is_absolute():
        raise Refused('evidence-root', 'evidence dir must be absolute')
    for ancestor in [resolved] + list(resolved.parents):
        if ancestor.is_symlink():
            raise Refused('evidence-root', 'symlink ancestor: %s' % ancestor)
    if not any(str(resolved).startswith(str(root).rstrip('/') + '/') for root in roots):
        raise Refused('evidence-root', 'outside the allowed scratch root')
    return resolved


def claim_evidence(path, roots):
    """A new private evidence directory. An existing one refuses: no adoption."""
    resolved = within_root(path, roots)
    try:
        os.mkdir(resolved, 0o700)
    except FileExistsError:
        raise Refused('evidence-exists', 'evidence directory already exists: %s' % resolved)
    return resolved


def write_exclusive(path, text):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(fd, 'w') as handle:
        handle.write(text)


def write_occurrence(evidence_dir, status, detail):
    """The occurrence record, written once. Failed or Unknown stays spent."""
    body = {'status': status, 'detail': detail, 'spent': status != 'passed'}
    write_exclusive(Path(evidence_dir) / 'occurrence.json',
                    json.dumps(body, indent=2, sort_keys=True) + '\n')
    return body


def write_binding(evidence_dir, request_sha256, record, modules):
    """Retain the bindings the occurrence ran against, before any child."""
    body = {'request_sha256': request_sha256, 'provenance': record, 'modules': modules}
    write_exclusive(Path(evidence_dir) / 'binding.json',
                    json.dumps(body, indent=2, sort_keys=True) + '\n')


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
        'argv': list(argv),
        'cwd': str(cwd) if cwd is not None else None,
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


def make_transport(budgets, children_dir):
    """The production transport: fixed argv, the pinned cwd, bounded streams."""
    def transport(argv, label, cwd):
        return run_child(argv, timeout_s=budgets['step_timeout_s'],
                         stdout_cap=budgets['stdout_cap_bytes'],
                         stderr_cap=budgets['stderr_cap_bytes'],
                         out_dir=children_dir, label=label, cwd=cwd)
    return transport


class Session:
    """The only way a route runs a child.

    `effigy` runs an allowlisted tail of the pinned install. `engine_call` runs
    a read or an allowlisted engine verb through the pinned Colima profile.
    Every call carries the pinned checkout as cwd and a sequence-numbered
    label, so each original stream stays separate and unique.
    """

    def __init__(self, transport, binary, checkout, tails, engine):
        if not os.path.isabs(binary):
            raise Refused('argv-not-pinned', 'binary must be absolute')
        self.transport = transport
        self.binary = binary
        self.checkout = checkout
        self.tails = frozenset(tails)
        self.engine = engine
        self.calls = 0

    def _label(self, label):
        self.calls += 1
        return '%03d-%s' % (self.calls, label)

    def effigy(self, tail, label):
        tail = tuple(tail)
        if tail not in self.tails:
            raise Refused('argv-not-allowlisted', ' '.join(tail[:4]))
        return self.transport([self.binary, *tail], self._label(label), self.checkout)

    def engine_call(self, argv, label):
        prefix = [self.engine['colima_binary'], 'nerdctl', '-p', self.engine['profile'], '--']
        if list(argv[:5]) != prefix or len(argv) < 6 or argv[5] not in ENGINE_VERBS:
            raise Refused('argv-not-allowlisted', 'engine verb outside the allowlist')
        return self.transport(list(argv), self._label(label), self.checkout)


def read_running(session, env, label):
    record = session.effigy(('container', env, 'status', '--json'), label)
    return parse_running(stdout_text(record))

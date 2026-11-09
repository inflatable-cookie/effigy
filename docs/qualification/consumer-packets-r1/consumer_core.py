"""Shared core for the r1 consumer proof packets (Acow and CBS). Source only.

Trust precondition. The entry runs only when Nucleus launches `proof_entry.py`
at the exact reviewed merged commit, with the request file's SHA-256 on argv,
under already-recorded same-sequence authority. This is the precondition the
accepted task effigy#739 proof entry states. No code here can authenticate that
launch against another process of the same user. A request file, an environment
value, a caller-set flag, or this file's existence is not that authority, and
nothing here reads one as authority.

Observable checks, all before effect or recorded in the occurrence. The request
bytes match the SHA-256 argument. The schema is strict. The on-disk entry modules
match the request. The packet checkout HEAD is the request's merged commit. The
installed candidate bytes match the request. The consumer checkout is at the
selected clean head with the reviewed closure blobs. The bundle revision and
digest match. The window is in bounds, unexpired and unconsumed. Budgets and
resource roles match. Engine identities, prior running state and counts are
probed through the supervised engine.

Not observable here. That positive624 exists for candidate C. The request names
that receipt; the Nucleus precondition must have checked it.

Supervision. Every child and every git read runs through `Supervisor`, which
drives the shipped one-use stage primitive `preparation_stage.run_stage` from
task effigy#531. That gives each child its own stdout and stderr files with
hashes, a per-stream cap, one absolute window deadline with a rollback reserve,
live resource sampling during blocked children, and a waited exit. Only a waited
zero exit with no cap, deadline or resource breach counts as success. Stage
receipts stay on disk for refusals too. The primitive stops only the process
group of the leader it started, by that leader's PID while the leader is still
unreaped. Nothing else is signalled. An escaped descendant that closes its
inherited pipes and leaves the group is not seen; the packet does not claim
otherwise.
"""
import hashlib
import json
import os
import re
import stat
import sys
import time
from pathlib import Path

DIR = Path(__file__).resolve().parent
QUALIFICATION = DIR.parent
MANIFEST = DIR / 'manifest-r1.json'
MANIFEST_SCHEMA = 'effigy.qualification.consumer-packets.r1'
REQUEST_SCHEMA = 'effigy.qualification.consumer-packet.request.v1'
PENDING = 'pending'
# Every module this packet executes, relative to docs/qualification. The two
# supervisor modules are the shared 531 primitive, imported and hash-bound, not copied.
ENTRY_MODULES = (
    'consumer-packets-r1/consumer_core.py',
    'consumer-packets-r1/proof_entry.py',
    'consumer-packets-r1/acow.py',
    'consumer-packets-r1/cbs.py',
    'consumer-packets-r1/acow_acquisition.py',
    'workspace-private-531/preparation_stage.py',
    'workspace-private-531/preparation_resources.py',
)
EVIDENCE_ROOTS = ('/Volumes/Builds/scratch/effigy',)
GIT = '/usr/bin/git'
MAX_REQUEST_BYTES = 65536
MAX_MANIFEST_BYTES = 262144
RESERVE_S = 30
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
# Keys a route adds to REQUEST_KEYS. Only the ordinary acquisition check has one:
# its second consumer checkout (modern) is bound beside the first (legacy).
ROUTE_EXTRA_KEYS = {'acow-acquisition': frozenset({'modern_consumer'})}
# Stage disposition to refusal code. `exited` with a waited zero is the only success.
DISPOSITION_CODES = {
    'failed': 'child-failed',
    'timeout': 'deadline-exceeded',
    'output-cap': 'over-cap-stream',
    'resource-refusal': 'resource-refusal',
    'unknown': 'child-unreaped',
    'local-error': 'child-unreaped',
}

# The 531 stage primitive and its resource monitor are imported, not copied. The
# directory is appended, so no module of this packet can be shadowed by it.
sys.path.append(str(QUALIFICATION / 'workspace-private-531'))
import preparation_resources  # noqa: E402
import preparation_stage  # noqa: E402


class Refused(Exception):
    """A fail-closed refusal. `code` is stable; `detail` is for the operator."""

    def __init__(self, code, detail=''):
        super().__init__('%s: %s' % (code, detail) if detail else code)
        self.code = code
        self.detail = detail


def sha256_bytes(data):
    return 'sha256:' + hashlib.sha256(data).hexdigest()


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(block)
    return 'sha256:' + digest.hexdigest()


def monotonic_now():
    return time.monotonic_ns()


def module_hashes():
    """Hashes of every executed module on disk, compared with the request."""
    return {rel: sha256_file(QUALIFICATION / rel) for rel in ENTRY_MODULES}


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


def load_request(path, expected_sha256):
    """The request object, after its exact bytes match the SHA-256 argument."""
    if not isinstance(expected_sha256, str) or not SHA256_TAG.fullmatch(expected_sha256):
        raise Refused('request-sha-mismatch', 'expected digest is not sha256:<64 hex>')
    raw = read_bounded_bytes(path, MAX_REQUEST_BYTES)
    if sha256_bytes(raw) != expected_sha256:
        raise Refused('request-sha-mismatch', 'request bytes differ from the expected digest')
    return parse_json_strict(raw), expected_sha256


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
    """Refuse while the route's own source pins are pending. Runs before the request is read."""
    pending = pending_pins(manifest['routes'][route])
    if pending:
        raise Refused('route-pins-pending',
                      '%d pins pending, first: %s' % (len(pending), pending[0]))


def check_exact_keys(obj, keys, what):
    if not isinstance(obj, dict) or set(obj) != set(keys):
        raise Refused('request-shape', '%s keys are not the reviewed set' % what)


def validate_request_shape(request, route):
    """Strict shape only. Values are checked by `check_bindings` and the observations."""
    extra = ROUTE_EXTRA_KEYS.get(route, frozenset())
    check_exact_keys(request, REQUEST_KEYS | extra, 'request')
    if request['schema'] != REQUEST_SCHEMA:
        raise Refused('request-schema', str(request['schema']))
    if request['route'] != route:
        raise Refused('request-route', 'expected %s' % route)
    for key in ('candidate', 'helpers', 'predecessor', 'consumer',
                'provenance', 'window', 'budgets', 'prior_intent') + tuple(sorted(extra)):
        if not isinstance(request[key], dict):
            raise Refused('request-shape', key + ' is not an object')
    if not isinstance(request['resources'], list):
        raise Refused('request-shape', 'resources is not a list')


def check_bindings(request):
    """Self-consistency of the request's bindings, and the packet's own module bytes.

    Reads no consumer checkout and no engine. The packet checkout HEAD and the
    installed binary are observed later, by `verify_helper_head` and `verify_install`.
    """
    provenance = request['provenance']
    check_exact_keys(provenance, {'merged_commit', 'source_immutable_head'}, 'provenance')
    if not HEX40.fullmatch(str(provenance['merged_commit'])) \
            or not HEX40.fullmatch(str(provenance['source_immutable_head'])):
        raise Refused('provenance-mismatch', 'merged commit or source head is not a commit')
    helpers = request['helpers']
    check_exact_keys(helpers, {'head', 'modules'}, 'helpers')
    if helpers['head'] != provenance['source_immutable_head'] \
            or helpers['modules'] != module_hashes():
        raise Refused('helper-mismatch', 'helper head or on-disk module hashes differ')
    candidate = request['candidate']
    check_exact_keys(candidate, {'source_commit', 'binary_sha256', 'install_identity'}, 'candidate')
    install = candidate['install_identity']
    check_exact_keys(install, {'path', 'version', 'sha256'}, 'install identity')
    if not HEX40.fullmatch(str(candidate['source_commit'])):
        raise Refused('candidate-mismatch', 'source commit is not a commit')
    if not SHA256_TAG.fullmatch(str(candidate['binary_sha256'])) \
            or install['sha256'] != candidate['binary_sha256']:
        raise Refused('candidate-mismatch', 'binary digest is malformed or differs from the install identity')
    if not os.path.isabs(str(install['path'])):
        raise Refused('candidate-mismatch', 'installed path is not absolute')
    predecessor = request['predecessor']
    check_exact_keys(predecessor, {'receipt_sha256', 'same_candidate_commit'}, 'predecessor')
    if not SHA256_TAG.fullmatch(str(predecessor['receipt_sha256'])):
        raise Refused('predecessor-mismatch', 'receipt digest is malformed')
    if predecessor['same_candidate_commit'] != candidate['source_commit']:
        raise Refused('predecessor-mismatch', 'receipt is not for this candidate')
    window = request['window']
    check_exact_keys(window, {'id', 'token_sha256', 'start_monotonic_ns',
                              'deadline_monotonic_ns'}, 'window')


def verify_install(request):
    """The installed candidate bytes are the bytes the request names. A file read, no child."""
    real = os.path.realpath(request['candidate']['install_identity']['path'])
    try:
        digest = sha256_file(real)
    except OSError as exc:
        raise Refused('candidate-mismatch', 'installed binary unreadable: %s' % exc)
    if digest != request['candidate']['binary_sha256']:
        raise Refused('candidate-mismatch', 'installed binary bytes differ from the request')
    return real


def check_window(request, manifest, now_ns, route):
    """Absolute monotonic bounds, one use, and the route's maximum span."""
    window = request['window']
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
    # The stage primitive applies one cap to each stream, so the two caps must be equal.
    if budgets['stdout_cap_bytes'] != budgets['stderr_cap_bytes']:
        raise Refused('budget-exceeded', 'stream caps must be equal for the stage primitive')


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


def check_git_args(args):
    """One fixed read: object ids and status of a checkout. No content."""
    if list(args) not in (['rev-parse', 'HEAD'], ['status', '--porcelain']) \
            and not (len(args) == 2 and args[0] == 'rev-parse' and args[1].startswith('HEAD:')):
        raise Refused('git-not-allowlisted', ' '.join(args))


def recording_git(git, reads):
    """Wrap a git reader so every successful read is kept for the closure evidence."""
    def recording(args, cwd):
        stdout = git(args, cwd)
        reads.append({'args': list(args), 'cwd': str(cwd), 'stdout': stdout})
        return stdout
    return recording


def verify_helper_head(git, request):
    """The code running this entry is the reviewed merged commit."""
    head = git(['rev-parse', 'HEAD'], str(DIR)).strip()
    if head != request['provenance']['merged_commit']:
        raise Refused('provenance-mismatch', 'packet checkout HEAD is not the merged commit')


def verify_consumer(git, request, manifest, route, checkout):
    """The route's own consumer block, at its pinned checkout."""
    verify_consumer_block(git, request['consumer'], manifest['routes'][route]['consumer'], checkout)


def verify_consumer_block(git, consumer, pins, checkout):
    """Selected clean head, declared include set, and every closure blob at that head."""
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


def parse_running(text, field='running', schema=None):
    """The running state of one environment, from its `status --json` report.

    `field` and `schema` are the route's pinned status contract. The default is the
    `running` field the acow and cbs routes were written against; a route that pins
    another contract passes it explicitly, and a report that does not match refuses.
    """
    try:
        body = json.loads(text)
    except ValueError:
        raise Refused('status-unparsed', 'status is not JSON')
    if not isinstance(body, dict):
        raise Refused('status-unparsed', 'status is not an object')
    if schema is not None and body.get('schema') != schema:
        raise Refused('status-unparsed', 'status schema differs from the pinned contract')
    if not isinstance(body.get(field), bool):
        raise Refused('status-unparsed', 'running state is not a bool in field ' + field)
    return body[field]


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


def write_binding(evidence_dir, request_sha256, request, modules):
    """Retain the bindings the occurrence ran against, before any child."""
    body = {
        'request_sha256': request_sha256,
        'provenance': request['provenance'],
        'candidate': request['candidate'],
        'predecessor': request['predecessor'],
        'window': {'id': request['window']['id'], 'token_sha256': request['window']['token_sha256']},
        'modules': modules,
    }
    write_exclusive(Path(evidence_dir) / 'binding.json',
                    json.dumps(body, indent=2, sort_keys=True) + '\n')


def write_closure(evidence_dir, reads):
    """Retain every consumer read the closure check made. Written even on refusal."""
    write_exclusive(Path(evidence_dir) / 'closure.json',
                    json.dumps({'reads': reads}, indent=2, sort_keys=True) + '\n')


def disposition_refusal(record):
    """Raise unless the stage recorded a waited zero exit with no breach."""
    disposition = record['disposition']
    if disposition == 'exited' and record.get('waited_exit') == 0:
        return
    code = DISPOSITION_CODES.get(disposition, 'child-unreaped')
    raise Refused(code, '%s: %s, waited exit %r' % (record['label'], disposition,
                                                    record.get('waited_exit')))


class Supervisor:
    """The one supervised path for every consumer child and git read of an occurrence.

    Each call is one `run_stage` stage in its own fresh 0700 directory under the
    evidence root, so original streams, hashes and the receipt survive refusals.
    Steps launch before `end - RESERVE_S`. Rollback may use the reserve. Both
    share the one absolute window end as their cleanup bound.
    """

    def __init__(self, deadline_ns, budgets, children_dir, reserve_s=RESERVE_S):
        self.end_s = deadline_ns / 1e9
        self.budgets = budgets
        self.children = Path(children_dir)
        self.reserve_s = reserve_s
        self.calls = 0

    def child(self, argv, label, cwd, rollback=False):
        # The stage primitive accepts a zero cap and then kills the child at its first byte.
        # The packet refuses it before any record exists, as the route budgets do.
        if self.budgets['stdout_cap_bytes'] < 1 or self.budgets['stderr_cap_bytes'] < 1:
            raise Refused('supervision-refused', '%s: stream caps must be at least one byte' % label)
        self.children.mkdir(parents=True, exist_ok=True)
        root = self.children / label
        try:
            os.mkdir(root, 0o700)
        except FileExistsError:
            raise Refused('evidence-exists', 'child label already recorded: ' + label)
        deadline = self.end_s if rollback else self.end_s - self.reserve_s
        start = time.monotonic()
        span_ms = max(1, int((self.end_s - start) * 1000) + 1)
        # Sampled every loop turn while the child runs, so a blocked child is still watched.
        monitor = preparation_resources.Monitor(
            {'window_ms': span_ms},
            lambda: {'window_ms': int((time.monotonic() - start) * 1000)})
        try:
            result = preparation_stage.run_stage(
                root, list(argv), self.budgets['step_timeout_s'],
                self.budgets['stdout_cap_bytes'], {}, cwd=str(cwd),
                resource_monitor=monitor, deadline=deadline, cleanup_deadline=self.end_s)
        except (preparation_stage.Refusal, preparation_resources.Refusal) as exc:
            raise Refused('supervision-refused', '%s: %s' % (label, exc))
        except OSError as exc:
            raise Refused('child-launch-failed', '%s: %s' % (label, exc))
        record = dict(result, label=label, argv=list(argv), cwd=str(cwd))
        record['streams'] = {name: dict(result['streams'][name], path=str(root / name))
                             for name in ('stdout', 'stderr')}
        disposition_refusal(record)
        return record

    def transport(self, argv, label, cwd):
        """Session's transport. Labels are '<seq>-<name>'; rollback names may use the reserve."""
        rest = label.split('-', 1)[1] if '-' in label else label
        return self.child(argv, label, cwd, rollback=rest.startswith('rollback'))

    def git(self, args, cwd):
        check_git_args(args)
        self.calls += 1
        record = self.child([GIT, *args], 'git-%03d-%s' % (self.calls, args[0]), cwd)
        return stdout_text(record)


class Session:
    """The only way a route runs a child.

    `effigy` runs an allowlisted tail of the pinned install. `engine_call` runs
    a read or an allowlisted engine verb through the pinned Colima profile.
    Every call carries the pinned checkout as cwd and a sequence-numbered
    label, so each original stream stays separate and unique.
    """

    def __init__(self, transport, binary, checkout, tails, engine, sequence=None):
        if not os.path.isabs(binary):
            raise Refused('argv-not-pinned', 'binary must be absolute')
        self.transport = transport
        self.binary = binary
        self.checkout = checkout
        self.tails = frozenset(tails)
        self.engine = engine
        # One counter per occurrence, shared by `at`, so labels never collide across checkouts.
        self.sequence = sequence if sequence is not None else [0]

    def _label(self, label):
        self.sequence[0] += 1
        return '%03d-%s' % (self.sequence[0], label)

    def at(self, checkout):
        """The same transport, binary, allowlist and sequence, at another pinned checkout."""
        return Session(self.transport, self.binary, checkout, self.tails, self.engine,
                       self.sequence)

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


def read_running(session, env, label, field='running', schema=None):
    record = session.effigy(('container', env, 'status', '--json'), label)
    return parse_running(stdout_text(record), field, schema)


def arm(route, tails, execute, request_path, request_sha256, evidence_dir, *, transport=None,
        manifest=None, git=None, now_ns=None, roots=EVIDENCE_ROOTS, verify_more=None):
    """The armed sequence. Every gate that reads no child runs before the evidence claim.

    A refusal before the claim leaves no evidence. A refusal or failure after it
    spends the occurrence and records its code, so a retry cannot reuse it.
    `transport` and `git` default to the supervised production path. `verify_more`,
    when a route gives one, runs the route's further closure reads with the same
    recording reader, still before any engine or consumer effect.
    """
    manifest = manifest if manifest is not None else load_manifest()
    require_route_frozen(manifest, route)
    request, digest = load_request(request_path, request_sha256)
    validate_request_shape(request, route)
    check_bindings(request)
    now = monotonic_now() if now_ns is None else now_ns
    check_window(request, manifest, now, route)
    check_budgets(request, manifest, route)
    check_resources(request, manifest, route)
    checkout = resolve_checkout(manifest['routes'][route]['checkout'])
    binary = verify_install(request)
    evidence = claim_evidence(evidence_dir, roots)
    try:
        sup = Supervisor(request['window']['deadline_monotonic_ns'], request['budgets'],
                         evidence / 'children')
        reads = []
        reader = recording_git(git or sup.git, reads)
        write_binding(evidence, digest, request, module_hashes())
        try:
            verify_helper_head(reader, request)
            verify_consumer(reader, request, manifest, route, checkout)
            if verify_more is not None:
                verify_more(reader, request, manifest)
        finally:
            write_closure(evidence, reads)
        engine = manifest['routes'][route]['engine']
        session = Session(transport or sup.transport, binary, checkout, tails, engine)
        result = execute(session, engine, manifest, request)
    except Refused as exc:
        write_occurrence(evidence, exc.code, exc.detail)
        raise
    except Exception as exc:
        write_occurrence(evidence, 'error', type(exc).__name__)
        raise
    write_occurrence(evidence, 'passed', 'armed sequence settled')
    return result

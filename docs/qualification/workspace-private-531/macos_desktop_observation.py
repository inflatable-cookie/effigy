"""Receipt-bound observation of the failed 779 Desktop check container. Source only.

One fixed, proof-only entry: it reports honestly whether the exact container the failed
effigy#779 D3 disposable check left on the Desktop engine is terminal, still retained
(nonterminal), proven absent by its exact full ID, mismatched with the original identity,
or unknown. It is read-only: the only operations are one engine-identity read (`docker
info --format {{json .ID}}`) and one exact full-ID `docker container inspect` through the
pinned original D0 Desktop context. It never starts, stops, removes, restarts, waits for
or configures anything, queries no other object, and requests no environment, secrets,
container logs or unrelated inventory. It cannot make 779 pass, mint D3 creation credit,
authorize adoption or removal, refund the consumed allowance, or qualify effigy#622.

External provenance is the existing Nucleus exact merged-commit source-immutable
proof-only launch under recorded same-sequence authority. A request file, this program's
existence or its environment is not that authorization. Every request is re-derived from
the pinned retained originals, the merged helper source and the reviewed tools, and any
request that differs refuses before the one-use claim and before any child runs.

The retained effigy#779 record is consumed as bytes, never inferred from age or absence:
the stage receipt, the failed step receipt, its resource observations and intent, the
retained cidfile, the D2 prepared-image receipt, the original engine read, the parent
retention index and the parent bindings all carry pinned SHA-256 binds.

Run: python3 -I -B macos_desktop_observation.py plan EVIDENCE_ROOT ACTIVATION_ID
     python3 -I -B macos_desktop_observation.py REQUEST SHA256
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import macos_preparation as prep  # noqa: E402
import macos_stage_executor as executor  # noqa: E402
import macos_stage_proof as proof  # noqa: E402
import preparation_stage as stage  # noqa: E402

Refusal = stage.Refusal

SCHEMA_REQUEST = 'macos-desktop-observation-request-v1'
SCHEMA_RECEIPT = 'macos-desktop-observation-receipt-v1'
TOKEN = 'e272a9cc'
ENGINE_ID = executor.RECORDED_ENGINE_ID
CONTAINER_ID = '2a35eb59a74953c0c78c252a1fcbea83463cd488e9d701c2d3292ae366ee2f2f'
CONTAINER_NAME = 'effigy-4f6a-dcheck-' + TOKEN
CONTAINER_LABEL = 'effigy.qual=' + TOKEN
IMAGE_ID = 'sha256:8abc0af233d602d407ef5582777b589431e04d137f803ab32ec5d075689b264d'
FAILED_ACTIVATION = 'd3-ff3fb9d8-9407d5a921dd'
# The genuine retained 779 record under the parent's private child. Every entry is consumed
# by exact hash; nothing here is re-derived from age, list emptiness or any narrative.
RETAINED = executor.LINUX_ARTIFACT_DIR + '/779-desktop-runtime-failed-retained.70n66jyg'
D3 = RETAINED + '/originals/evidence/D3-' + FAILED_ACTIVATION
ORIGINALS = {
    'parent_index': (RETAINED + '/parent-retention-index.json',
                     '7a2b4282be3c691c015e478a28a3eab4b344e481e547e226db018b00275c60d2'),
    'bindings': (RETAINED + '/originals/bindings.json',
                 '781e6757b9e4a41bb3a0ca9ed022f5858c11ba432525c85aa7aef676132abb20'),
    'stage_receipt': (D3 + '/stage-receipt.json',
                      '405668a34906857f602370ecf9cc7c46efebf49842b8d947ed88f9ffae2b626d'),
    'engine_info_receipt': (D3 + '/d3-engine-info/receipt.json',
                            '298a1bbca3562077ac4e51c75d9d9fa7c178be3f4d569f9c44c36717068869f1'),
    'engine_info_stdout': (D3 + '/d3-engine-info/stdout',
                           '112bf5f7bb6469aef126e6af3d17b4fc3cc1f6f203dac3ab1bd4504a622724ee'),
    'step_receipt': (D3 + '/d3-disposable-check/receipt.json',
                     '198a59ea3e5291931f27fc2d9969d6a1af9aecc2a3966e7e8c3686a5f289fa99'),
    'observations': (D3 + '/d3-disposable-check/observations.json',
                     '15198c8be31a7b1e99798130c5248b859f4f202fdd4df85ef217fcdd17ee5140'),
    'intent': (D3 + '/d3-disposable-check/intent.json',
               '136e9738369512aa66ab421be3c0990da978d201c54f353425dd0831773accc9'),
    'cidfile': (RETAINED + '/observed-cidfile-original',
                '966a6f61e871da2af97a91c5bb4fae175b551a56756d9a407af827a112298f3a'),
    'd2_receipt': (RETAINED + '/originals/originals/D2-stage-receipt.json',
                   'ad90733e8d6e0b31f1db60192b156f64f4e4c38eb44a158d5de3b6e3067bdbc1'),
}
# Originals the parent retention index must also name with the same hash: the index is the
# parent's custody record for its copies. The cidfile is pinned above (its custody lives in
# the offline zero-cap control), and the index is not self-listed.
INDEXED_ROLES = ('bindings', 'stage_receipt', 'engine_info_receipt', 'engine_info_stdout',
                 'step_receipt', 'observations', 'intent', 'd2_receipt')
# One finite monotonic window; per-step allowances sit inside it. Nothing waits unbounded.
DURATION = 120
ENGINE_ALLOWANCE = 30
INSPECT_ALLOWANCE = 60
OUTPUT_CAP_BYTES = 262144
STORAGE_CAP_BYTES = 1024 * 1024
DOCKER = prep.DOCKER
REVIEWED_DOCKER_SHA256 = executor.HISTORICAL_TOOL_HASHES['docker']
# The two fixed read-only operations. The inspect template selects exactly the identity and
# state fields the verdicts need: no environment, no secrets, no logs, no other object.
STEP_ARGVS = {
    'engine-identity': lambda: [DOCKER, 'info', '--format', '{{json .ID}}'],
    'container-inspect': lambda: [DOCKER, 'container', 'inspect', '--format',
                                  '{"id":{{json .Id}},"name":{{json .Name}},'
                                  '"image":{{json .Image}},"labels":{{json .Config.Labels}},'
                                  '"state":{{json .State}}}', CONTAINER_ID],
}
STEP_ALLOWANCES = {'engine-identity': ENGINE_ALLOWANCE, 'container-inspect': INSPECT_ALLOWANCE}
CHILD_ENV_SLOTS = ('PATH', 'LANG', 'DOCKER_CONFIG', 'DOCKER_HOST')
CHILD_PATH = '/usr/bin:/bin:/usr/sbin:/sbin'
# The supported exact-ID not-found answer, pinned from the pinned 29.8.0 CLI's own source:
# moby client's objectNotFoundError prints "Error: No such container: <id>" and the CLI's
# cobra printer emits that one line on stderr with exit 1 and empty stdout. Any other shape
# (transport, daemon, timeout, extra text) stays unknown.
NOT_FOUND_PREFIX = 'Error: No such container: '
TERMINAL_STATES = frozenset({'exited', 'dead'})
NONTERMINAL_STATES = frozenset({'created', 'running', 'restarting', 'paused', 'removing'})
ESTABLISHED = frozenset({'terminal', 'nonterminal', 'absent', 'mismatched'})
EMPTY_SHA256 = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
REQUIRES_OPERATOR_GO = (
    'Contact the pinned Desktop engine socket read-only: one engine-identity `docker info` '
    'read and one exact full-ID `docker container inspect`. Nothing is started, stopped, '
    'removed, restarted, waited for or configured; no other object is queried and no '
    'environment, secrets, logs or inventory beyond the two fixed answers is retained.')


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def parse_unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise Refusal('duplicate request key')
        value[key] = item
    return value


def read_original(role):
    """One retained original by exact pinned hash. The bytes are custody: a changed,
    missing, aliased or synthesized original refuses here, before any request can bind it."""
    path, expected = ORIGINALS[role]
    try:
        raw = executor.read_checked_file(path, must_be_in=prep.STASH, read_only=False)
    except Refusal as exc:
        raise Refusal(f'{role}: retained original unreadable: {exc}') from exc
    if hashlib.sha256(raw).hexdigest() != expected:
        raise Refusal(f'{role}: retained original differs from its pinned hash')
    return raw


def retained_bindings(raw):
    """The original D3 chain bindings, from the retained bytes only. The control paths,
    endpoint and token are never caller input."""
    value = json.loads(raw, object_pairs_hook=parse_unique)
    expected_keys = {'check_source', 'colima_cache_home', 'colima_docker_config', 'colima_home',
                     'colima_input_dir', 'control', 'desktop_cidfile', 'desktop_config',
                     'desktop_context_dir', 'desktop_host', 'desktop_iidfile', 'desktop_input_dir',
                     'qualification_root', 'route_echo', 'token'}
    if not isinstance(value, dict) or set(value) != expected_keys:
        raise Refusal('retained bindings do not have the original shape')
    control = value['control']
    if (value['token'] != TOKEN or control != prep.STASH + '/mac-4f6a-' + TOKEN
            or value['desktop_host'] != prep.DESKTOP_HOST
            or value['desktop_config'] != control + '/d/docker-config'
            or value['desktop_context_dir'] != control + '/d/ctx'
            or value['desktop_cidfile'] != control + '/d/container-id'):
        raise Refusal('retained bindings do not name the pinned original Desktop chain')
    return {key: value[key] for key in ('control', 'desktop_config', 'desktop_context_dir',
                                        'desktop_cidfile', 'desktop_host')}


def check_step_receipt(raw):
    """The failed disposable-check receipt: genuinely failed and spent, with both original
    streams empty. No container-closure fact is claimed from it."""
    value = json.loads(raw, object_pairs_hook=parse_unique)
    if not isinstance(value, dict) or set(value) != {
            'disposition', 'elapsed_seconds', 'guest_closure', 'identity_probe', 'observed_bytes',
            'process', 'resource_error', 'resource_usage', 'streams', 'waited_exit'}:
        raise Refusal('failed step receipt does not have the original shape')
    if (value['disposition'] != 'resource-refusal' or value['waited_exit'] != -9
            or value['resource_error'] != 'resource cap exceeded'
            or value.get('guest_closure') != 'not-applicable'):
        raise Refusal('failed step receipt is not the recorded resource refusal')
    streams = value['streams']
    for name in ('stdout', 'stderr'):
        if (streams.get(name, {}).get('bytes') != 0
                or streams.get(name, {}).get('sha256') != EMPTY_SHA256):
            raise Refusal(f'failed step receipt {name} is not the empty original')
    if value['observed_bytes'] != {'stderr': 0, 'stdout': 0}:
        raise Refusal('failed step receipt observed bytes differ')
    if value['resource_usage'] != {'storage': 0}:
        raise Refusal('failed step receipt resource usage differs')
    process = value['process']
    if (not isinstance(process, dict) or not isinstance(process.get('pid'), int)
            or not isinstance(process.get('pgid'), int) or not isinstance(process.get('start_utc'), str)):
        raise Refusal('failed step receipt has no process identity')
    return value


def check_stage_receipt(raw, bindings, d2_sha):
    """The original D3 stage receipt: the spent failed stage on this exact chain, naming the
    retained D2 predecessor by its exact hash."""
    value = json.loads(raw, object_pairs_hook=parse_unique)
    if not isinstance(value, dict) or value.get('schema') != executor.SCHEMA:
        raise Refusal('stage receipt is not an executor receipt')
    if (value.get('stage') != 'D3' or value.get('closure') != 'stopped'
            or value.get('token') != TOKEN or value.get('control') != bindings['control']
            or value.get('engine_id') != ENGINE_ID
            or value.get('desktop_host') != bindings['desktop_host']):
        raise Refusal('stage receipt is not the failed 779 D3 stage on this chain')
    prior = value.get('prior')
    if (not isinstance(prior, dict) or prior.get('stage') != 'D2'
            or prior.get('sha256') != d2_sha):
        raise Refusal('stage receipt does not name the retained D2 predecessor by its hash')
    steps = value.get('steps')
    if (not isinstance(steps, list) or len(steps) != 2
            or [s.get('id') for s in steps if isinstance(s, dict)] != ['d3-engine-info', 'd3-disposable-check']):
        raise Refusal('stage receipt step inventory differs')
    return value


def check_intent(raw, bindings):
    """The original run's intent: the exact argv that created the observed container. The
    name, label, image and cidfile identity are read from here, never from caller input."""
    value = json.loads(raw, object_pairs_hook=parse_unique)
    if not isinstance(value, dict) or set(value) != {
            'argv', 'cwd', 'deadline_monotonic', 'environment', 'guest', 'input_hashes',
            'output_cap_bytes_each', 'stdin_sha256', 'timeout_seconds'}:
        raise Refusal('original intent does not have the original shape')
    expected_argv = [prep.DOCKER, 'run', '--pull=never', '--rm', '--network', 'none', '--label',
                     CONTAINER_LABEL, '--name', CONTAINER_NAME, '--cidfile', bindings['desktop_cidfile'],
                     IMAGE_ID, 'python3', '-I', '-c', prep.CHECK_SOURCE]
    if value['argv'] != expected_argv:
        raise Refusal('original intent argv differs from the pinned disposable check')
    if value['environment'] != {'DOCKER_CONFIG': bindings['desktop_config'],
                                'DOCKER_HOST': bindings['desktop_host'],
                                'LANG': 'C', 'PATH': CHILD_PATH}:
        raise Refusal('original intent environment differs from the pinned Desktop context')
    if value['guest'] is not False or value['stdin_sha256'] is not None:
        raise Refusal('original intent is not the plain host run')
    return value


def check_parent_index(raw):
    """The parent custody record: this retained child, its failed/spent status, and the exact
    copies every indexed original must appear under, with the same hashes."""
    value = json.loads(raw, object_pairs_hook=parse_unique)
    if not isinstance(value, dict) or set(value) != {
            'task', 'status', 'files', 'total_bytes', 'offline_control', 'stage_sha256', 'scope'}:
        raise Refusal('parent retention index does not have the original shape')
    if value['task'] != 'effigy#779' or not str(value['status']).startswith('failed/spent'):
        raise Refusal('parent retention index is not the failed 779 record')
    rows = {}
    for row in value['files']:
        if not isinstance(row, dict) or set(row) < {'original', 'copy', 'bytes', 'sha256'}:
            raise Refusal('parent retention index row is incomplete')
        rows[row['copy']] = row['sha256']
    return rows


def check_d2_receipt(raw):
    """The retained D2 predecessor: the closed stage that produced the prepared image on the
    recorded engine."""
    value = json.loads(raw, object_pairs_hook=parse_unique)
    if not isinstance(value, dict) or value.get('schema') != executor.SCHEMA:
        raise Refusal('D2 receipt is not an executor receipt')
    if (value.get('stage') != 'D2' or value.get('closure') != 'closed'
            or value.get('token') != TOKEN or value.get('engine_id') != ENGINE_ID):
        raise Refusal('D2 receipt is not the closed prepared-image stage')
    images = [r for r in value.get('receipts', []) if isinstance(r, dict) and r.get('kind') == 'image']
    if len(images) != 1 or images[0].get('id') != IMAGE_ID or images[0].get('label') != CONTAINER_LABEL:
        raise Refusal('D2 receipt does not name the prepared image by its receipt')
    return value


def derive_originals():
    """Consume the whole retained record by hash and derive the observed identity block from
    it. Every derived value must equal its source constant: the bytes and the pinned facts
    agree, or the entry refuses."""
    raws = {role: read_original(role) for role in ORIGINALS}
    bindings = retained_bindings(raws['bindings'])
    stage_receipt = check_stage_receipt(raws['stage_receipt'], bindings, ORIGINALS['d2_receipt'][1])
    step_receipt = check_step_receipt(raws['step_receipt'])
    intent = check_intent(raws['intent'], bindings)
    d2_receipt = check_d2_receipt(raws['d2_receipt'])
    observations = json.loads(raws['observations'], object_pairs_hook=parse_unique)
    if (not isinstance(observations, dict)
            or set(observations) != {'before', 'maxima', 'post', 'post_refusal'}
            or observations['post_refusal'] != 'resource cap exceeded'):
        raise Refusal('resource observations do not have the original shape')
    cid = prep.read_cidfile(raws['cidfile'].decode('ascii'))
    if cid != CONTAINER_ID:
        raise Refusal('retained cidfile does not name the pinned container ID')
    original_engine = prep.engine_id_from_info(raws['engine_info_stdout'].decode())
    if original_engine != ENGINE_ID:
        raise Refusal('original engine read does not name the recorded engine')
    engine_receipt = json.loads(raws['engine_info_receipt'], object_pairs_hook=parse_unique)
    if (engine_receipt.get('disposition') != 'exited' or engine_receipt.get('waited_exit') != 0
            or engine_receipt.get('streams', {}).get('stdout', {}).get('sha256')
            != ORIGINALS['engine_info_stdout'][1]):
        raise Refusal('original engine receipt does not close its retained stdout')
    indexed = check_parent_index(raws['parent_index'])
    for role in INDEXED_ROLES:
        path, sha = ORIGINALS[role]
        if indexed.get(path) != sha:
            raise Refusal(f'{role}: not in the parent retention index under this hash')
    if intent['argv'][0] != prep.DOCKER or stage_receipt.get('engine_id') != d2_receipt.get('engine_id'):
        raise Refusal('retained record disagrees about the original tool or engine')
    return dict(bindings=bindings, original_engine=original_engine,
                failed=dict(disposition=step_receipt['disposition'],
                            waited_exit=step_receipt['waited_exit'],
                            resource_error=step_receipt['resource_error']),
                d2_activation=d2_receipt.get('activation_id'))


def safe_originals():
    """Consume the retained record, naming any malformed shape as the refusal it is."""
    try:
        return derive_originals()
    except ValueError as exc:
        if isinstance(exc, Refusal):
            raise
        raise Refusal('retained record is malformed: ' + str(exc)) from exc
    except (KeyError, TypeError, AttributeError, UnicodeDecodeError) as exc:
        raise Refusal('retained record is malformed: ' + type(exc).__name__ + ': ' + str(exc)) from exc


def check_tool():
    if executor.sha_file(DOCKER) != REVIEWED_DOCKER_SHA256:
        raise Refusal('Desktop CLI differs from the source-reviewed 29.8.0 build 88096ef')


def private_root(path):
    """A canonical, existing, owned, 0700 evidence root under Stash scratch, with its identity."""
    if not isinstance(path, str) or os.path.realpath(path) != path \
            or not path.startswith(prep.STASH + '/'):
        raise Refusal('evidence root must be canonical under Stash scratch')
    try:
        info = os.lstat(path)
    except OSError as exc:
        raise Refusal('evidence root unavailable: ' + str(exc)) from exc
    if not stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode) \
            or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
        raise Refusal('evidence root must be an owned private directory')
    return [info.st_dev, info.st_ino, info.st_uid, stat.S_IMODE(info.st_mode)]


def nested(a, b):
    return a == b or b.startswith(a + '/')


def roots_identity(bindings):
    """The original D0 chain's own identities for the control root and the two Docker
    directories, re-observed and pinned. A replaced root refuses here."""
    prior = executor.read_receipt(proof.ORIGINAL_D0)
    if prior.get('engine_id') != ENGINE_ID:
        raise Refusal('original D0 engine identity differs')
    pins = executor.Pins()
    try:
        for name in ('control', 'desktop_config', 'desktop_context_dir'):
            pins.hold(bindings[name])
        executor.check_d0_identities(prior, bindings, pins)
        return {path: list(identity) for path, identity in pins.items.items()}
    finally:
        pins.close()


def plan(evidence_root, activation_id):
    """Read-only prospective request. Nothing is created, claimed or dispatched; the request
    is the hash-pinned binding a later Nucleus launch verifies."""
    if not isinstance(activation_id, str) or not executor.ACTIVATION_ID.fullmatch(activation_id) \
            or activation_id.startswith('531'):
        raise Refusal('invalid activation id')
    root_identity = private_root(evidence_root)
    check_tool()
    facts = safe_originals()
    bindings = facts['bindings']
    if nested(bindings['control'], evidence_root) or nested(evidence_root, bindings['control']):
        raise Refusal('evidence root and control root must not contain each other')
    identity, _runtime = executor.stable_input_manifest()
    return {
        'schema': SCHEMA_REQUEST,
        'entry_sha256': executor.sha_file(__file__),
        'inputs': identity,
        'inputs_sha256': executor.canon_sha(identity),
        'originals': {role: {'path': path, 'sha256': sha} for role, (path, sha) in ORIGINALS.items()},
        'engine_id': ENGINE_ID,
        'token': TOKEN,
        'container': {'id': CONTAINER_ID, 'name': CONTAINER_NAME, 'label': CONTAINER_LABEL,
                      'image': IMAGE_ID},
        'failed': dict(facts['failed'], activation=FAILED_ACTIVATION),
        'control': bindings,
        'roots': roots_identity(bindings),
        'd0': {'stage': proof.ORIGINAL_D0['stage'], 'path': proof.ORIGINAL_D0['path'],
               'sha256': proof.ORIGINAL_D0['sha256']},
        'activation_id': activation_id,
        'duration_seconds': DURATION,
        'evidence_root': evidence_root,
        'evidence_root_identity': root_identity,
        'claim_path': executor.CLAIMS_DIR + '/obs779-' + activation_id + '.claim',
        'evidence_path': evidence_root + '/observation-' + activation_id + '/observation-receipt.json',
        'requiresOperatorGo': REQUIRES_OPERATOR_GO,
    }


def read_request(path, expected):
    if os.path.realpath(path) != path or not path.startswith(prep.STASH + '/'):
        raise Refusal('request must be canonical under Stash scratch')
    try:
        info = os.lstat(path)
    except OSError as exc:
        raise Refusal('request unreadable: ' + str(exc)) from exc
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_nlink != 1:
        raise Refusal('request must be one owned regular file')
    raw = Path(path).read_bytes()
    if len(raw) > 65536 or hashlib.sha256(raw).hexdigest() != expected:
        raise Refusal('request hash or size mismatch')
    return json.loads(raw, object_pairs_hook=parse_unique)


class ControlWatch:
    """Live refusal for any observed change anywhere under the original control root while a
    child runs. The observation writes nothing here, so any sampled change is a mutation
    attempt and stops the child with the change recorded. Sampling is not a quota: a change
    made and removed between samples is not observable."""

    def __init__(self, root):
        self.root = root
        self.initial = self._inventory()

    @staticmethod
    def _identity(info):
        return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_nlink, info.st_size,
                info.st_blocks, info.st_mtime_ns, info.st_ctime_ns)

    def _inventory(self):
        out, stack = {}, [self.root]
        while stack:
            path = stack.pop()
            info = os.lstat(path)
            out[path] = self._identity(info)
            if stat.S_ISDIR(info.st_mode):
                with os.scandir(path) as entries:
                    stack.extend(e.path for e in entries)
        return out

    def check(self):
        current = self._inventory()
        if set(current) != set(self.initial):
            raise Refusal('control-tree inventory changed during the observation')
        for path, before in self.initial.items():
            if before != current[path]:
                raise Refusal('control-tree identity changed during the observation: ' + path)


class ObservationMonitor:
    """The evidence-root storage envelope plus the strict control-tree watch, in the shape
    preparation_stage.run_stage expects."""

    def __init__(self, private, watch):
        self.private, self.watch = private, watch
        self.last, self.maxima = private.last, private.maxima

    def check(self):
        used = self.private.check()
        self.last, self.maxima = self.private.last, self.private.maxima
        self.watch.check()
        return dict(private=used, control_entries=len(self.watch.initial))


def step_argv(name):
    """The fixed read-only operations and nothing else. Any other step name has no argv at
    all, so no mutation-shaped or unrelated command can be constructed here."""
    builder = STEP_ARGVS.get(name)
    if builder is None:
        raise Refusal('unsupported observation step: ' + str(name))
    return builder()


def observation_step(destination, name, inputs, environment, monitor, deadline):
    """One supervised fixed child in its own private step directory: separate original
    streams, waited exit, process identity, input-hash custody and bounded allowance, all
    recorded by the shared stage primitive."""
    if name not in STEP_ALLOWANCES:
        raise Refusal('unsupported observation step: ' + str(name))
    now = time.monotonic()
    step_deadline = min(now + STEP_ALLOWANCES[name], deadline)
    if step_deadline <= now:
        raise Refusal(name + ': observation allowance exhausted')
    step_root = destination + '/' + name
    executor.create_private_dir(step_root)
    result = stage.run_stage(step_root, step_argv(name), step_deadline - now, OUTPUT_CAP_BYTES, inputs,
                             cwd=step_root, environment=environment, resource_monitor=monitor,
                             deadline=step_deadline, cleanup_deadline=deadline,
                             identity_probe=executor.start_identity)
    return step_root, result


def read_streams(step_root):
    return {name: Path(step_root, name).read_bytes() for name in ('stdout', 'stderr')}


def parse_inspection(stdout):
    """The fixed template's answer: exactly one JSON object with exactly the five selected
    identity/state fields. Anything else is malformed and stays unknown."""
    data = json.loads(stdout)
    if not isinstance(data, dict) or set(data) != {'id', 'name', 'image', 'labels', 'state'}:
        raise ValueError('inspection response does not carry exactly the selected fields')
    if not isinstance(data['id'], str) or not re.fullmatch(r'[0-9a-f]{64}', data['id']):
        raise ValueError('inspection response ID is not one full container ID')
    if not isinstance(data['name'], str) or not isinstance(data['image'], str):
        raise ValueError('inspection response name or image is missing')
    image = prep.normalise_image_id(data['image'])
    if not isinstance(data['state'], dict) or not isinstance(data['state'].get('Status'), str):
        raise ValueError('inspection response carries no state status')
    return {'id': data['id'], 'name': data['name'].lstrip('/'), 'image': image,
            'labels': data['labels'] if isinstance(data['labels'], dict) else None,
            'state': data['state']}


def compare_facts(facts, container):
    """Identity first, then state. A present object that differs in any identity fact is
    mismatched; a matching object is classified by its own state status only."""
    labels = facts['labels'] or {}
    mismatches = []
    if facts['id'] != container['id']:
        mismatches.append('id')
    if facts['name'] != container['name']:
        mismatches.append('name')
    if facts['image'] != container['image']:
        mismatches.append('image')
    if labels.get('effigy.qual') != TOKEN:
        mismatches.append('label')
    if mismatches:
        return 'mismatched', {'facts': facts, 'mismatch': mismatches}
    status = facts['state']['Status']
    if status in TERMINAL_STATES:
        return 'terminal', {'facts': facts}
    if status in NONTERMINAL_STATES:
        return 'nonterminal', {'facts': facts}
    return 'unknown', {'facts': facts, 'reason': 'unrecognised container status: ' + status}


def classify_inspection(result, stdout, stderr, container):
    """Distinguish the supported exact-ID absence from every daemon, context, transport,
    parse, timeout and nonzero failure, which all stay unknown."""
    if result.get('disposition') == 'exited' and result.get('waited_exit') == 0:
        try:
            facts = parse_inspection(stdout)
        except (ValueError, UnicodeDecodeError) as exc:
            return 'unknown', {'reason': 'malformed inspection response: ' + str(exc),
                               'disposition': result.get('disposition'),
                               'waited_exit': result.get('waited_exit')}
        return compare_facts(facts, container)
    if (result.get('disposition') == 'failed' and result.get('waited_exit') == 1 and not stdout
            and stderr.count(b'\n') == 1
            and stderr.rstrip(b'\n') == (NOT_FOUND_PREFIX + container['id']).encode()):
        return 'absent', {'facts': None}
    reason = ('inspection is neither a well-formed response nor the supported exact-ID '
              'not-found answer')
    unknown = {'reason': reason, 'disposition': result.get('disposition'),
               'waited_exit': result.get('waited_exit'), 'stdout_bytes': len(stdout),
               'stderr_bytes': len(stderr)}
    if result.get('resource_error'):
        unknown['resource_error'] = result['resource_error']
    return 'unknown', unknown


def execute(request, request_path, request_sha):
    """Verify the whole request against a fresh derivation, then spend the one-use claim and
    run at most the two fixed read-only operations inside one finite window."""
    if not isinstance(request, dict) or request.get('schema') != SCHEMA_REQUEST:
        raise Refusal('unsupported observation request')
    activation_id = request.get('activation_id')
    evidence_root = request.get('evidence_root')
    # Everything up to and including this comparison happens before the claim and before any
    # child: an ordinary, unbound or drifted request refuses without effect.
    if request != plan(evidence_root, activation_id):
        raise Refusal('request bindings drifted or differ from the derived plan')
    if private_root(evidence_root) != request['evidence_root_identity']:
        raise Refusal('evidence root identity changed')
    runtime = executor.stable_input_manifest()[1]
    inputs = dict(runtime)
    for role, (path, sha) in ORIGINALS.items():
        if inputs.setdefault(path, sha) != sha:
            raise Refusal('original aliases a bound input')
    inputs[request_path] = request_sha
    started_utc = datetime.datetime.now(datetime.timezone.utc).isoformat()
    deadline = time.monotonic() + request['duration_seconds']
    destination = evidence_root + '/observation-' + activation_id
    receipt_path = request['evidence_path']
    if (os.path.lexists(destination) or os.path.lexists(receipt_path)
            or os.path.lexists(request['claim_path'])):
        raise Refusal('observation destination or claim already exists; no replay')
    claim = executor.claim_activation('obs779-' + activation_id, request_sha)
    executor.create_private_dir(destination)
    pins = executor.Pins()
    receipt = {'schema': SCHEMA_RECEIPT, 'request_sha256': request_sha, 'activation_id': activation_id,
               'entry_sha256': request['entry_sha256'], 'claim': claim, 'started_utc': started_utc,
               'duration_seconds': request['duration_seconds'], 'engine_id': request['engine_id'],
               'container': request['container'], 'engine_observed': None, 'engine_step': None,
               'inspect_step': None, 'verdict': 'unknown', 'facts': None, 'reason': None,
               'originals': {role: sha for role, (_, sha) in ORIGINALS.items()},
               'control': request['control']}
    try:
        for path, identity in request['roots'].items():
            pins.hold(path)
            if list(pins.items[path]) != identity:
                raise Refusal('control identity changed since the request was frozen')
        watch = ControlWatch(request['control']['control'])
        monitor = ObservationMonitor(
            executor.default_monitor({'storage': STORAGE_CAP_BYTES}, [destination]), watch)
        environment = {'PATH': CHILD_PATH, 'LANG': 'C',
                       'DOCKER_CONFIG': request['control']['desktop_config'],
                       'DOCKER_HOST': request['control']['desktop_host']}
        if set(environment) != set(CHILD_ENV_SLOTS):
            raise Refusal('child environment slots differ')

        def record(key, step_root, result):
            receipt[key] = dict(step_root=step_root, receipt=step_root + '/receipt.json',
                                receipt_sha256=executor.checked_sha(step_root + '/receipt.json',
                                                                   must_be_in=destination),
                                disposition=result.get('disposition'),
                                waited_exit=result.get('waited_exit'))

        step_root, result = observation_step(destination, 'engine-identity', inputs, environment,
                                             monitor, deadline)
        record('engine_step', step_root, result)
        stdout = Path(step_root, 'stdout').read_bytes()
        if result.get('disposition') != 'exited' or result.get('waited_exit') != 0:
            receipt['reason'] = ('engine identity is unknown: the engine read did not close '
                                 '(disposition %s, waited_exit %r)'
                                 % (result.get('disposition'), result.get('waited_exit')))
            if result.get('resource_error'):
                receipt['reason'] += '; ' + result['resource_error']
            return receipt
        try:
            engine_observed = json.loads(stdout)
        except ValueError:
            engine_observed = None
        if not isinstance(engine_observed, str):
            receipt['reason'] = 'engine identity is unknown: malformed engine read response'
            return receipt
        receipt['engine_observed'] = engine_observed
        if engine_observed != request['engine_id']:
            receipt['verdict'] = 'refused'
            receipt['reason'] = ('fresh engine identity differs from the original receipt: '
                                 'refuse before inspection')
            raise Refusal(receipt['reason'])
        step_root, result = observation_step(destination, 'container-inspect', inputs, environment,
                                             monitor, deadline)
        record('inspect_step', step_root, result)
        streams = read_streams(step_root)
        verdict, facts = classify_inspection(result, streams['stdout'], streams['stderr'],
                                             request['container'])
        receipt['verdict'] = verdict
        receipt['facts'] = facts
        if verdict == 'unknown':
            receipt['reason'] = facts.get('reason')
        return receipt
    finally:
        receipt['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        if os.path.isdir(destination) and not os.path.lexists(receipt_path):
            executor.write_json_new(destination, 'observation-receipt.json', receipt)
        pins.close()


def main(argv):
    if len(argv) == 4 and argv[1] == 'plan':
        print(json.dumps(plan(os.path.realpath(argv[2]), argv[3]), indent=2, sort_keys=True))
        return 0
    if len(argv) != 3:
        raise Refusal('observation entry requires a request and its SHA-256, '
                      'or plan EVIDENCE_ROOT ACTIVATION_ID')
    request = read_request(argv[1], argv[2])
    receipt = execute(request, argv[1], argv[2])
    print(json.dumps(receipt, sort_keys=True))
    return 0 if receipt['verdict'] in ESTABLISHED else 1


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except Refusal as exc:
        print('refused: ' + str(exc), file=sys.stderr)
        sys.exit(2)

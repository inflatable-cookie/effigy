"""Controls for macos_stage_executor. Local child processes and injected transports only: no Colima,
Docker Desktop, guest, network, install or credential read. Every fixture is a fresh mkdtemp directory
under $TMPDIR. STASH, the activation and claims directories, the tool binaries and the desktop socket
are redirected into it, so no real evidence root or claim is touched.

The genuine chain runs through execute_stage with real child processes: D0, then D1, D2, D3 and D4. D2 is
admitted: its prefetch helper fetches the fixture's tiny genuine-byte stand-in for the pinned OCI+deb set
(never the production payloads) and the production handler stages the Dockerfile. C2 and C3 close through
injected helper envelopes (no simulation). C4 runs through its production path too: the reviewed host
handlers run in-process, the pinned guest transfer and pre-build re-derivation run against this fixture's own
owned parent directory, and only the transport is substituted. Its guest steps close on authenticated
envelopes, never on a simulation. C5 and C6 cannot be reached through a genuine chain (C5's remote run is
still the local ssh client's), so their dispatch tests replace only verify_chain, and say so. The chain
verifier has its own negative tests on genuine receipts.

Run: python3 -I -B macos_stage_executor_tests.py
"""
import contextlib
import datetime
import hashlib
import io
import json
import os
import plistlib
import re
from pathlib import Path
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import guest_inputs  # noqa: E402
import macos_guest_helper as helper  # noqa: E402
import macos_preparation as prep  # noqa: E402
import macos_stage_executor as ex  # noqa: E402
import macos_stage_proof as proof  # noqa: E402
import preparation_resources as resources  # noqa: E402
import preparation_stage as primitive  # noqa: E402
import runtime_image_inputs as runtime_inputs  # noqa: E402

TOKEN = '0a1b2c3d'
ENGINE = ex.RECORDED_ENGINE_ID
IMAGE = 'sha256:' + 'ab' * 32
CID = 'a' * 64
OTHER_ID = 'b' * 64
FAKE_METADATA_JSON = b'{"SchemaVersion":"0.1.0","Vendor":"Docker Inc.","Version":"v0.37.1"}'
FAKE_METADATA_LINE = b"printf '%s' '" + FAKE_METADATA_JSON + b"'\n"
FAKE_BUILDX = (b'#!/bin/sh\n'
               b"printf '%s' '{\"SchemaVersion\":\"0.1.0\",\"Vendor\":\"Docker Inc.\",\"Version\":\"v0.37.1\"}'\n")
NOW = datetime.datetime.now(datetime.timezone.utc)
REAL_STASH = prep.STASH  # captured before any fixture patches it: the literal entry runs against the real root
MIB = 1024 * 1024
CHILD_ENV = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin'}
ENGINE_JSON = '{"ID":"%s"}' % ENGINE
FACTS = '{"uid":0,"python":"3.11.9","ssl":"OpenSSL 3.0","loader":"GNU C Library 2.35","coreutils":"coreutils 9"}'
IMAGE_NAME = 'effigy-4f6a-desktop-runtime:' + TOKEN
READBACK = json.dumps({'Id': IMAGE, 'Os': 'linux', 'Architecture': 'arm64', 'RepoTags': [IMAGE_NAME],
                       'Created': '2026-11-08T09:20:00Z', 'Config': {'Labels': {'effigy.qual': TOKEN}}})
IMAGE_OBJECT = json.dumps({'Id': IMAGE, 'RepoTags': [IMAGE_NAME], 'Config': {'Labels': {'effigy.qual': TOKEN}}})


# A routed step that runs the manifest's remote tail locally, with the pinned guest source and its real arguments.
RUN_PINNED = object()
# The step's own argv runs, unchanged, with its manifest environment: the Buildx plugin route, reached through its link.
RUN_ARGV = object()
# A small stand-in for the pinned Colima image. Every fixture swaps the real 447 MB pin and the network opener for these,
# so no test fetches anything. The real pin is checked separately, against the preparation constants.
FAKE_IMAGE = b'effigy fixture image\n' * 64
FAKE_PIN = dict(url=prep.COLIMA_IMAGE_URL, sha512=hashlib.sha512(FAKE_IMAGE).hexdigest(), bytes=len(FAKE_IMAGE),
                name=prep.COLIMA_IMAGE_NAME)


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _oci(data, media_type, url):
    digest = 'sha256:' + _sha(data)
    return dict(digest=digest, bytes=len(data), media_type=media_type, url=url), data


def make_fake_runtime():
    """Tiny genuine-byte stand-in for the pinned OCI blobs and each named deb. Hashes match the bytes."""
    config_body = json.dumps({'os': 'linux', 'architecture': 'arm64', 'rootfs': {'type': 'layers',
                              'diff_ids': ['sha256:' + '11' * 32]}}, separators=(',', ':')).encode()
    layer_body = b'fake-ubuntu-layer\n'
    config, config_bytes = _oci(config_body, 'application/vnd.oci.image.config.v1+json',
                                'https://registry-1.docker.io/v2/library/ubuntu/blobs/' + 'sha256:' + _sha(config_body))
    config['url'] = 'https://registry-1.docker.io/v2/library/ubuntu/blobs/' + config['digest']
    layer, layer_bytes = _oci(layer_body, 'application/vnd.oci.image.layer.v1.tar+gzip',
                              'https://registry-1.docker.io/v2/library/ubuntu/blobs/unused')
    layer['url'] = 'https://registry-1.docker.io/v2/library/ubuntu/blobs/' + layer['digest']
    manifest_body = json.dumps({
        'schemaVersion': 2, 'mediaType': 'application/vnd.oci.image.manifest.v1+json',
        'config': {'mediaType': config['media_type'], 'size': config['bytes'], 'digest': config['digest']},
        'layers': [{'mediaType': layer['media_type'], 'size': layer['bytes'], 'digest': layer['digest']}],
    }, separators=(',', ':')).encode()
    manifest, manifest_bytes = _oci(manifest_body, 'application/vnd.oci.image.manifest.v1+json', 'unused')
    manifest['url'] = 'https://registry-1.docker.io/v2/library/ubuntu/manifests/' + manifest['digest']
    index_body = json.dumps({
        'schemaVersion': 2, 'mediaType': 'application/vnd.oci.image.index.v1+json',
        'manifests': [{'mediaType': manifest['media_type'], 'digest': manifest['digest'], 'size': manifest['bytes'],
                       'platform': {'architecture': 'arm64', 'os': 'linux', 'variant': 'v8'}}],
    }, separators=(',', ':')).encode()
    index, index_bytes = _oci(index_body, 'application/vnd.oci.image.index.v1+json', 'unused')
    index['url'] = 'https://registry-1.docker.io/v2/library/ubuntu/manifests/' + index['digest']
    bodies = {index['url']: index_bytes, manifest['url']: manifest_bytes, config['url']: config_bytes,
              layer['url']: layer_bytes}
    debs = []
    for i, item in enumerate(runtime_inputs.DEBS):
        data = ('deb-%d\n' % i).encode()
        url = 'https://ports.ubuntu.com/ubuntu-ports/fixture/' + item['name']
        debs.append(dict(name=item['name'], bytes=len(data), sha256=_sha(data), url=url, path='fixture/' + item['name']))
        bodies[url] = data
    pin = dict(bytes=sum(x['bytes'] for x in (index, manifest, config, layer)) + sum(d['bytes'] for d in debs),
               index=index, manifest=manifest, config=config, layer=layer, debs=tuple(debs),
               oci_blobs=(index, manifest, config, layer))
    return pin, bodies, dict(index=index_bytes, manifest=manifest_bytes, config=config_bytes, layer=layer_bytes)


FAKE_RUNTIME_PIN, FAKE_RUNTIME_BODIES, FAKE_RUNTIME_BYTES = make_fake_runtime()


def fake_runtime_opener(bodies=None, token=b'{"token":"fixture-token"}'):
    """Opener that serves the fixture's tiny OCI/deb bytes. The token is not a payload."""
    bodies = bodies if bodies is not None else FAKE_RUNTIME_BODIES

    def opener(url, headers=None):
        if url == runtime_inputs.AUTH_TOKEN_URL:
            return FakeConnection(), FakeResponse(token)
        if url not in bodies:
            raise AssertionError('the fixture may only fetch a pinned runtime URL, got %s' % url)
        return FakeConnection(), FakeResponse(bodies[url])
    return opener


class FakeResponse:
    """An HTTPResponse stand-in: a status, headers and a body that read(n) serves in finite pieces."""

    def __init__(self, body=FAKE_IMAGE, status=200, headers=None):
        self.body, self.status = io.BytesIO(body), status
        self.headers = dict({'Content-Length': str(len(body))}, **(headers or {}))

    def getheader(self, name, default=None):
        return next((v for k, v in self.headers.items() if k.lower() == name.lower()), default)

    def read(self, n=-1):
        return self.body.read(n)


class FakeConnection:
    def __init__(self):
        self.closed = False

    def close(self):
        self.closed = True


def fake_opener(response=None):
    """Returns an opener that yields one fake connection and the given response (default: the pinned bytes)."""
    def opener(url):
        assert url == prep.COLIMA_IMAGE_URL, 'the fixture may only fetch the pinned URL'
        return FakeConnection(), response or FakeResponse()
    return opener


def emit(text, exit_code=0, err=''):
    return 'import sys\nsys.stdout.write(%r)\nsys.stderr.write(%r)\nsys.exit(%d)\n' % (text, err, exit_code)


def closed_envelope(operation, nonce, inner_stdout, inner_stderr=b'', command=None, **fields):
    """A production helper envelope. Tests inject this through the real establish_guest_closure path.
    command is the argv a fixed remote command reports having run."""
    if isinstance(inner_stdout, str):
        inner_stdout = inner_stdout.encode()
    if isinstance(inner_stderr, str):
        inner_stderr = inner_stderr.encode()
    header = dict(nonce=nonce, operation=operation, boot_id=fields.get('boot_id', 'boot-1'),
                  pid=fields.get('pid', 11), start_time=fields.get('start_time', 100),
                  pgid=fields.get('pgid', 11), session_id=fields.get('session_id', 11),
                  source_sha256=fields.get('source_sha256', ex.GUEST_HELPER_SHA256))
    if command is not None:
        header['command'] = list(command)
    trailer = dict(waited_exit=fields.get('waited_exit', 0), child_pid=fields.get('child_pid', 12),
                   child_start_time=fields.get('child_start_time', 101),
                   child_pgid=fields.get('child_pgid', header['pgid']),
                   child_session_id=fields.get('child_session_id', header['session_id']),
                   group_empty=fields.get('group_empty', True), unreaped=fields.get('unreaped', False),
                   stdout_sha256=fields.get('stdout_sha256', hashlib.sha256(inner_stdout).hexdigest()),
                   stderr_sha256=fields.get('stderr_sha256', hashlib.sha256(inner_stderr).hexdigest()))
    return helper.format_envelope(header, inner_stdout, inner_stderr, trailer)


def closed_guest(inner, err=b'', exit_code=0, command=None, **fields):
    """Routed transport: the local child writes an authenticated helper envelope for this step's nonce."""
    def route(kw):
        argv = kw['argv']
        c_idx = argv.index('-c')
        operation = fields.get('operation', argv[c_idx + 3])
        nonce = fields.get('nonce', argv[c_idx + 4])
        payload = inner(kw) if callable(inner) else inner
        extra = {k: v for k, v in fields.items() if k not in ('operation', 'nonce')}
        envelope = closed_envelope(operation, nonce, payload, err, command=command, **extra)
        return ('import sys\nsys.stdout.buffer.write(%r)\nsys.stderr.buffer.write(%r)\nsys.exit(%d)\n'
                % (envelope, b'', exit_code))
    return route


def volume_for(activation_id):
    """The engine-generated name a D1 create prints: 64 lowercase hex. A fixed stand-in per activation."""
    return hashlib.sha256(activation_id.encode()).hexdigest()


def create_child(volume):
    """`docker volume create` without a name: the engine's generated name, printed on one line."""
    return ("import os,sys\nassert os.path.exists('intent.json')\n"
            "sys.stdout.write(%r)\n" % (volume + '\n'))


def inspect_object(volume):
    return json.dumps({'Name': volume, 'Driver': 'local', 'Scope': 'local',
                       'Mountpoint': '/var/lib/docker/volumes/%s/_data' % volume,
                       'CreatedAt': '2026-11-08T09:20:01Z', 'Labels': {'effigy.qual': TOKEN}})


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def tree(root):
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        out += [os.path.join(dirpath, n) for n in dirnames + filenames]
    return sorted(out)


class Fixture(unittest.TestCase):
    def setUp(self):
        base = os.environ.get('TMPDIR') or None
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix='macos-executor-', dir=base))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        os.chmod(self.tmp, 0o700)
        self.evidence = self.tmp + '/evidence'
        self.activations = self.tmp + '/activations'
        for d in (self.evidence, self.activations, self.tmp + '/claims'):
            os.mkdir(d, 0o700)
        self.colima = self.tmp + '/colima'
        self.docker = self.tmp + '/docker'
        Path(self.colima).write_bytes(b'fake colima\n')
        Path(self.docker).write_bytes(b'fake docker\n')
        # The socket lives in this fixture's own directory under TMPDIR (a fresh child, removed with the fixture).
        # AF_UNIX paths are limited to 104 bytes; a long TMPDIR fails here, loudly, rather than binding elsewhere.
        self.socket_path = self.tmp + '/d.sock'
        if len(self.socket_path.encode()) > 103:
            raise AssertionError(f'TMPDIR is too long for an AF_UNIX socket path: {self.socket_path}')
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.bind(self.socket_path)
        self.addCleanup(self.sock.close)
        for target, name, value in ((prep, 'STASH', self.tmp),
                                    # Fixture sockets are Docker doubles only; simulated Lima paths are longer.
                                    (prep, 'SOCKET_LIMIT_BYTES', 256), (ex, 'CLAIMS_DIR', self.tmp + '/claims'),
                                    (ex, 'ACTIVATIONS_DIR', self.activations),
                                    (ex, 'TOOL_PATHS', (self.colima, self.docker)),
                                    (ex, 'DESKTOP_SOCKET', self.socket_path),
                                    (ex, 'colima_image_pin', lambda manifest: dict(FAKE_PIN)),
                                    (ex, 'open_pinned_https', fake_opener()),
                                    (runtime_inputs, 'runtime_image_pin', lambda manifest: dict(FAKE_RUNTIME_PIN)),
                                    (runtime_inputs, 'open_runtime_https', fake_runtime_opener())):
            patcher = mock.patch.object(target, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        # The Desktop Buildx route is a fixture: a plugin and a bundle plist of known bytes, pinned to their own sha256.
        # The host's Docker.app is never read by a test.
        desktop = self.tmp + '/desktop'
        os.mkdir(desktop, 0o700)
        plugin = desktop + '/docker-buildx'
        Path(plugin).write_bytes(FAKE_BUILDX)
        os.chmod(plugin, 0o755)
        plist = desktop + '/Info.plist'
        Path(plist).write_bytes(plistlib.dumps({'CFBundleShortVersionString': ex.DESKTOP_BUNDLE_VERSION}))
        for target, name, value in ((ex, 'DESKTOP_BUILDX', plugin), (ex, 'DESKTOP_APP_PLIST', plist),
                                    (ex, 'DESKTOP_BUILDX_SHA256', hashlib.sha256(FAKE_BUILDX).hexdigest())):
            patcher = mock.patch.object(target, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.bindings = self.make_bindings(TOKEN)
        os.mkdir(self.bindings['control'], 0o700)
        # Test-only seams: execution in production is refused (no owner mechanism), and stages whose
        # production admission reasons are present run only here, with those reasons lifted.
        seam = mock.patch.dict(ex.TEST_SEAM, {'owner_activation': True, 'admit_unadmitted': True})
        seam.start()
        self.addCleanup(seam.stop)

    def guest_parent(self):
        parent = self.tmp + '/guest-parent'
        if not os.path.isdir(parent):
            os.mkdir(parent, 0o700)
        info = os.lstat(parent)
        return {'path': parent, 'device': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid}

    def make_bindings(self, token):
        paths = prep.control_paths(token)
        b = {k: v for k, v in paths.items() if k in prep.SLOTS}
        b.update(desktop_host=prep.DESKTOP_HOST, token=token, check_source=prep.CHECK_SOURCE,
                 route_echo=prep.ROUTE_ECHO)
        return b

    def d2_out(self, activation_id='act-d2-0001'):
        """The fresh D2 occurrence this activation derives: its input directory, build context and iidfile."""
        return ex.d2_occurrence(self.bindings, activation_id)

    def ref(self, stage, activation_id):
        root = self.stage_dir(stage, activation_id)
        return dict(stage=stage, path=root + '/stage-receipt.json', sha256=ex.sha_file(root + '/stage-receipt.json'))

    def stage_dir(self, stage, activation_id):
        return f'{self.evidence}/{stage}-{activation_id}'

    def activation(self, stage, prior_ref, activation_id, *, bindings=None, expires=10, issued=-1,
                   decision=prep.DECISION, candidate=None, inputs=None):
        manifest = ex.load_manifest()
        go = dict(decision=decision, decision_version=prep.DECISION_VERSION, stage=stage,
                  manifest_sha256=ex.sha_file(ex.MANIFEST_PATH), source_sha256=ex.sha_file(ex.__file__),
                  inputs_sha256=ex.canon_sha(inputs if inputs is not None else ex.stable_input_manifest()[0]),
                  activation_id=activation_id,
                  issued_utc=(NOW + datetime.timedelta(minutes=issued)).isoformat(),
                  expires_utc=(NOW + datetime.timedelta(minutes=expires)).isoformat(),
                  scope='single-activation', candidate_head=candidate or manifest['source_head'],
                  bindings_sha256=ex.canon_sha(bindings or self.bindings))
        if prior_ref is not None:
            go['prior_receipt'] = prior_ref
        data = (json.dumps(go, sort_keys=True) + '\n').encode()
        path = f'{self.activations}/{activation_id}.json'
        if os.path.exists(path):
            os.chmod(path, 0o600)
            os.remove(path)
        Path(path).write_bytes(data)
        os.chmod(path, 0o400)  # the owner's placement: read-only in the activation directory
        return path, sha_bytes(data)

    def execute(self, stage, transport, *, prior_ref=None, activation_id, clock=time.monotonic,
            monitor_factory=None, bindings=None, inputs=None, expires=19, outer_deadline=None):
        bindings = bindings or self.bindings
        path, sha = self.activation(stage, prior_ref, activation_id, bindings=bindings, inputs=inputs, expires=expires)
        return ex.execute_stage(stage, bindings, activation_path=path, activation_sha256=sha,
                                evidence_root=self.evidence, transport=transport, clock=clock,
                                utc_now=NOW, monitor_factory=monitor_factory, outer_deadline=outer_deadline)

    def routed(self, routes, calls, *, before=None, after=None, clock=None):
        """Transport: each step runs a local child chosen by its step id. A step without a route fails the
        test, so a later step that must not run cannot slip through. A fake stage clock maps onto the real
        monotonic clock the primitive reads. A route may be a callable taking the transport kwargs, or RUN_PINNED,
        which runs the manifest's own remote tail locally (the pinned guest source with its real arguments)."""
        def transport(**kw):
            calls.append((kw['step'], list(kw['argv']), dict(kw['environment'])))
            if before is not None:
                before(kw)
            if kw['step'] not in routes:
                raise AssertionError(f"unexpected step {kw['step']}")
            route = routes[kw['step']]
            if route is RUN_ARGV:
                result = primitive.run_stage(kw['root'], list(kw['argv']), kw['timeout'], kw['output_cap'], kw['input_hashes'],
                                             guest=kw['guest'], cwd=kw['cwd'], environment=kw['environment'],
                                             resource_monitor=kw['monitor'], deadline=kw['deadline'],
                                             stdin_path=kw['stdin_path'], stdin_sha256=kw['stdin_sha256'],
                                             identity_probe=kw['identity_probe'])
                if after is not None:
                    after(kw)
                return result
            if route is RUN_PINNED:
                tail = kw['argv'][kw['argv'].index('python3'):]
                argv = [sys.executable] + tail[1:]
            else:
                argv = [sys.executable, '-I', '-B', '-c', route(kw) if callable(route) else route]
            deadline = kw['deadline']
            if clock is not None:
                deadline = time.monotonic() + (kw['deadline'] - clock())
            result = primitive.run_stage(kw['root'], argv, kw['timeout'],
                                         kw['output_cap'], kw['input_hashes'], guest=kw['guest'], cwd=kw['cwd'],
                                         environment=CHILD_ENV, resource_monitor=kw['monitor'],
                                         deadline=deadline, stdin_path=kw['stdin_path'],
                                         stdin_sha256=kw['stdin_sha256'], identity_probe=kw['identity_probe'])
            if after is not None:
                after(kw)
            return result
        return transport

    def assert_dispatch(self, stage, calls, run_bindings):
        """Every exec call ran the argv and env that the manifest resolves for its step, under the runtime
        bindings. Cleanup steps are checked against their selected identity, not the plan placeholder.
        guest_nonce is minted per step at launch, so the dispatched nonce is rebound for the comparison."""
        steps = {s['id']: s for s in ex.stage_of(ex.load_manifest(), stage)['steps'] if s['kind'] == 'exec'}
        for step, argv, env in calls:
            if step in steps:
                bindings = dict(run_bindings)
                raw = steps[step]['argv']
                if '{guest_nonce}' in raw:
                    bindings['guest_nonce'] = argv[raw.index('{guest_nonce}')]
                elif step in ex.REMOTE_COMMAND_STEPS:
                    bindings['guest_nonce'] = argv[argv.index('-c') + 4]
                expected = ex.resolve_step(steps[step], bindings)
                if step in ex.REMOTE_COMMAND_STEPS:
                    expected = dict(expected, argv=ex.dispatch_guest_command(
                        expected, bindings, bindings['guest_nonce']))
                self.assertEqual(argv, expected['argv'], step)
                self.assertEqual(env, expected['env'], step)

    def d0(self):
        if not hasattr(self, '_d0'):
            routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit(''), 'd0-network': emit(''), 'd0-volume': emit('')}
            calls = []
            # D0 runs through its fixed helper supervisor, as production does: its child receipt is the identity D1 binds.
            with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}):
                self._d0_receipt = self.execute('D0', self.routed(routes, calls), activation_id='act-d0-0001')
            self._d0_calls = calls
            self._d0 = self.ref('D0', 'act-d0-0001')
        return self._d0

    def d1(self, prior, activation_id='act-d1-0001', calls=None):
        volume = volume_for(activation_id)
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                  'd1-volume-create': create_child(volume), 'd1-volume-inspect': emit(inspect_object(volume))}
        calls = [] if calls is None else calls
        receipt = self.execute('D1', self.routed(routes, calls), prior_ref=prior, activation_id=activation_id)
        return receipt, self.ref('D1', activation_id)

    def d2(self, prior, activation_id='act-d2-0001', calls=None, build=None, wrap=None):
        """D2 through the stage loop. build replaces the build route; wrap adapts the transport (e.g. a shorter allowance)."""
        def write_iid(kw):
            Path(kw['argv'][kw['argv'].index('--iidfile') + 1]).write_text(IMAGE + '\n')
            return emit('')
        routes = {'d2-engine-info': emit(ENGINE_JSON), 'd2-buildx-metadata': RUN_ARGV, 'd2-docker-build': build or write_iid,
                  'd2-readback': emit(READBACK)}
        calls = [] if calls is None else calls
        transport = self.routed(routes, calls)
        if wrap is not None:
            transport = wrap(transport)
        receipt = self.execute('D2', transport, prior_ref=prior, activation_id=activation_id)
        return receipt, self.ref('D2', activation_id)

    @staticmethod
    def cap(step, seconds):
        """Wraps a transport so one step's allowance is cut to `seconds`: a deliberately short, unknown closure."""
        def wrap(transport):
            def capped(**kw):
                if kw['step'] == step:
                    kw = dict(kw, timeout=min(kw['timeout'], seconds))
                return transport(**kw)
            return capped
        return wrap

    @staticmethod
    def d4_routes(volume, *, ps='', vol_rows=None, inventory=(IMAGE,), post_ps='', post_vol=None, post_inventory=None,
                  overrides=None):
        """Routes for every D4 step. The per-target inspects return the facts the D1-D3 receipts recorded, so each check
        passes unless a test overrides that step. inventory is the complete image list before removal; post_inventory
        the list after it (default: the same IDs minus the receipted image)."""
        rows = lambda ids: '\n'.join(json.dumps({'ID': i}) for i in ids)
        if vol_rows is None:
            vol_rows = json.dumps({'Name': volume, 'Labels': 'effigy.qual=' + TOKEN}) + '\n'
        container_inspect = json.dumps({'Id': CID, 'Name': '/' + ex.d3_occurrence(dict(prep.control_paths(TOKEN), token=TOKEN), 'act-d3-0001')['desktop_container_name'],
                                        'Config': {'Labels': {'effigy.qual': TOKEN}}})
        if post_inventory is None:
            post_inventory = [i for i in inventory if i != IMAGE]
        routes = {
            'd4-observe-info': emit(ENGINE_JSON), 'd4-observe-ps': emit(ps), 'd4-observe-volume': emit(vol_rows),
            'd4-observe-images': emit(rows(inventory) + '\n'), 'd4-observe-image': emit(IMAGE_OBJECT),
            'd4-container-info': emit(ENGINE_JSON), 'd4-container-inspect': emit(container_inspect),
            'd4-owned-rm': emit(''),
            'd4-volume-info': emit(ENGINE_JSON), 'd4-volume-inspect': emit(inspect_object(volume)),
            'd4-owned-volume-rm': emit(''),
            'd4-image-info': emit(ENGINE_JSON), 'd4-image-inspect': emit(READBACK), 'd4-owned-image-rm': emit(''),
            'd4-post-ps': emit(post_ps), 'd4-post-volume': emit(post_vol if post_vol is not None else ''),
            'd4-post-images': emit(rows(post_inventory) + '\n'),
        }
        routes.update(overrides or {})
        return routes

    def d3(self, prior, activation_id='act-d3-0001', calls=None):
        def write_cid(kw):
            Path(kw['argv'][kw['argv'].index('--cidfile') + 1]).write_text(CID)
            return emit(FACTS)
        routes = {'d3-engine-info': emit(ENGINE_JSON), 'd3-disposable-check': write_cid}
        calls = [] if calls is None else calls
        receipt = self.execute('D3', self.routed(routes, calls), prior_ref=prior, activation_id=activation_id)
        return receipt, self.ref('D3', activation_id)


class FixedHelperTests(Fixture):
    def admitted_d0(self, activation_id, transport, monitor_factory=None):
        admitted = dict(ex.UNADMITTED)
        admitted.pop('D0', None)
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}), \
             mock.patch.object(ex, 'UNADMITTED', admitted):
            return self.execute('D0', transport, activation_id=activation_id, monitor_factory=monitor_factory)

    @staticmethod
    def quiet_monitor(limits, roots):
        return resources.Monitor({'storage': 1}, lambda: {'storage': 0})

    def test_real_d0_helpers_run_under_the_stage_supervisor(self):
        routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit(''),
                  'd0-network': emit(''), 'd0-volume': emit('')}
        calls = []

        def monitor_factory(limits, roots):
            return resources.Monitor({'storage': 1}, lambda: {'storage': 0})

        admitted = dict(ex.UNADMITTED)
        admitted.pop('D0', None)
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}), \
             mock.patch.object(ex, 'UNADMITTED', admitted):
            receipt = self.execute('D0', self.routed(routes, calls), activation_id='fixed-helper-d0',
                                   monitor_factory=monitor_factory)
        self.assertEqual(receipt['closure'], 'closed', receipt)
        for step in receipt['steps'][:2]:
            root = Path(receipt['stage_root'], step['id'])
            child = json.loads((root / 'receipt.json').read_text())
            self.assertEqual(child['disposition'], 'exited')
            self.assertEqual(child['waited_exit'], 0)
            self.assertGreater(child['process']['pid'], 0)
            self.assertTrue((root / 'stdout').is_file())
            self.assertTrue((root / 'stderr').is_file())
        ex.verify_receipt_tree(receipt, 'D0', ex.chain_key(self.bindings), ex.load_manifest())
        Path(receipt['stage_root'], 'd0-make-owned-dirs', 'stdout').write_text('{}\n')
        with self.assertRaisesRegex(ex.Refusal, 'helper stdout changed'):
            ex.verify_receipt_tree(receipt, 'D0', ex.chain_key(self.bindings), ex.load_manifest())

    def test_live_zero_cap_stops_a_helper_blocked_on_a_fifo(self):
        calls = []
        spawned = []
        real_popen = primitive.subprocess.Popen
        fifo = self.colima

        def launch(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            spawned.append(child.pid)
            os.rename(fifo, fifo + '.original')
            os.mkfifo(fifo, 0o600)
            return child

        def sample():
            if not spawned:
                return {'storage': 0}
            try:
                fd = os.open(fifo, os.O_WRONLY | os.O_NONBLOCK)
            except OSError:
                return {'storage': 0}
            os.close(fd)
            return {'storage': 1}

        def monitor_factory(limits, roots):
            monitor = resources.Monitor({'storage': 1}, sample)
            monitor.limits = {'storage': 0}
            return monitor

        admitted = dict(ex.UNADMITTED)
        admitted.pop('D0', None)
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}), \
             mock.patch.object(ex, 'UNADMITTED', admitted), \
             mock.patch.object(primitive.subprocess, 'Popen', side_effect=launch):
            receipt = self.execute('D0', self.routed({}, calls), activation_id='fixed-blocked-d0',
                                   monitor_factory=monitor_factory)
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(calls, [])
        self.assertEqual([item['id'] for item in receipt['steps']], ['d0-make-owned-dirs'])
        step = receipt['steps'][0]
        self.assertEqual(step['closure'], 'unknown_retained')
        self.assertEqual(step['disposition'], 'resource-refusal')
        self.assertEqual(step['process']['pid'], spawned[0])
        self.assertIsNotNone(step['waited_exit'])

    def test_nonzero_cap_stops_a_helper_blocked_on_a_fifo(self):
        calls, children, writers = [], [], []
        real_popen = primitive.subprocess.Popen

        def launch(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            os.rename(self.colima, self.colima + '.original')
            os.mkfifo(self.colima, 0o600)
            return child

        def sample():
            if children and not writers:
                try:
                    writers.append(os.open(self.colima, os.O_WRONLY | os.O_NONBLOCK))
                except OSError:
                    return {'storage': 0}
            return {'storage': 2 if writers else 0}

        try:
            with mock.patch.object(primitive.subprocess, 'Popen', side_effect=launch):
                receipt = self.admitted_d0('fixed-nonzero-cap', self.routed({}, calls),
                                           lambda limits, roots: resources.Monitor({'storage': 1}, sample))
            self.assertTrue(writers, 'the child reached the FIFO')
            self.assertEqual(receipt['closure'], 'stopped')
            self.assertEqual(receipt['steps'][0]['disposition'], 'resource-refusal')
            self.assertIsNotNone(receipt['steps'][0]['waited_exit'])
            self.assertEqual(calls, [])
            self.assertEqual([item['id'] for item in receipt['steps']], ['d0-make-owned-dirs'])
        finally:
            for fd in writers:
                os.close(fd)

    def test_simulated_external_proof_entry_binds_and_claims_once(self):
        path, activation_sha = self.activation('D0', None, 'proof-stage-0001')
        inputs = ex.stable_input_manifest()[0]
        request = {'stage': 'D0', 'activation_path': path, 'activation_sha256': activation_sha,
                   'bindings': self.bindings, 'evidence_root': self.evidence,
                   'inputs': inputs, 'inputs_sha256': ex.canon_sha(inputs), 'manifest_sha256': ex.sha_file(ex.MANIFEST_PATH),
                   'source_sha256': ex.sha_file(ex.__file__)}
        request_path = self.tmp + '/proof-request.json'
        Path(request_path).write_text(json.dumps(request, sort_keys=True))
        os.chmod(request_path, 0o400)
        digest = ex.sha_file(request_path)
        admitted = dict(ex.UNADMITTED)
        admitted.pop('D0', None)
        with mock.patch.object(ex, 'UNADMITTED', admitted), \
             mock.patch.object(ex, '_execute_stage', return_value={'closure': 'closed'}) as stage_call, \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(proof.main(['proof', request_path, digest]), 0)
            with self.assertRaisesRegex(ex.Refusal, 'already claimed'):
                proof.main(['proof', request_path, digest])
            with self.assertRaisesRegex(ex.Refusal, 'hash or size'):
                proof.main(['proof', request_path, '0' * 64])
        self.assertEqual(stage_call.call_count, 1)

    def test_simulated_external_proof_refuses_changed_bindings_before_claim(self):
        path, activation_sha = self.activation('D0', None, 'proof-binding-0001')
        inputs = ex.stable_input_manifest()[0]
        request = {'stage': 'D0', 'activation_path': path, 'activation_sha256': activation_sha,
                   'bindings': self.bindings, 'evidence_root': self.evidence,
                   'inputs': inputs, 'inputs_sha256': ex.canon_sha(inputs), 'manifest_sha256': ex.sha_file(ex.MANIFEST_PATH),
                   'source_sha256': ex.sha_file(ex.__file__)}
        admitted = dict(ex.UNADMITTED)
        admitted.pop('D0', None)
        with mock.patch.object(ex, 'UNADMITTED', admitted), \
             mock.patch.object(ex, '_execute_stage') as stage_call:
            for field in ('inputs_sha256', 'manifest_sha256', 'source_sha256', 'activation_sha256'):
                altered = dict(request, **{field: '0' * 64})
                request_path = self.tmp + '/proof-' + field
                Path(request_path).write_text(json.dumps(altered, sort_keys=True))
                os.chmod(request_path, 0o400)
                with self.subTest(field=field), self.assertRaises(ex.Refusal):
                    proof.main(['proof', request_path, ex.sha_file(request_path)])
            unsupported = dict(request, stage='C7')  # not a reviewed proof stage
            request_path = self.tmp + '/proof-unadmitted'
            Path(request_path).write_text(json.dumps(unsupported, sort_keys=True))
            os.chmod(request_path, 0o400)
            with self.assertRaisesRegex(ex.Refusal, 'supports only C0, D0, D1, C1, C2, C3, C4, C5, C6, D2, D3 or D4'):
                proof.main(['proof', request_path, ex.sha_file(request_path)])

            oversized = self.tmp + '/proof-oversized'
            Path(oversized).write_bytes(b'x' * 65537)
            os.chmod(oversized, 0o400)
            with self.assertRaisesRegex(ex.Refusal, 'size limit'):
                proof.main(['proof', oversized, ex.sha_file(oversized)])
            self.assertEqual(os.listdir(self.tmp + '/claims'), [])
            stage_call.assert_not_called()

    def test_malformed_oversized_and_nonzero_children_stop_dispatch(self):
        # Inject adversarial child output at the process boundary; the adapter,
        # supervisor, stage loop and fixed dispatch remain the production path.
        scripts = ("print('not-json')", "print('x' * 10000)", "raise SystemExit(7)")
        real_popen = primitive.subprocess.Popen
        for index, script in enumerate(scripts):
            with self.subTest(script=script):
                calls = []

                def launch(*args, **kwargs):
                    return real_popen([sys.executable, '-I', '-B', '-c', script], **kwargs)

                with mock.patch.object(primitive.subprocess, 'Popen', side_effect=launch):
                    receipt = self.admitted_d0('fixed-invalid-%d' % index, self.routed({}, calls), self.quiet_monitor)
                self.assertEqual(receipt['closure'], 'stopped')
                self.assertEqual(calls, [])
                self.assertEqual(receipt['steps'][0]['closure'], 'unknown_retained')
                self.assertIsNotNone(receipt['steps'][0]['waited_exit'])

    def test_replaced_control_root_stops_after_the_fixed_helper(self):
        calls = []
        real_popen = primitive.subprocess.Popen
        control = self.bindings['control']

        def launch(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            os.rename(control, control + '.moved')
            os.mkdir(control, 0o700)
            return child

        with mock.patch.object(primitive.subprocess, 'Popen', side_effect=launch):
            receipt = self.admitted_d0('fixed-root-drift', self.routed({}, calls), self.quiet_monitor)
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(calls, [])
        self.assertEqual(receipt['steps'][0]['closure'], 'unknown_retained')

    def test_input_drift_stops_the_fixed_helper(self):
        calls = []
        real_popen = primitive.subprocess.Popen

        def launch(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            os.rename(self.colima, self.colima + '.original')
            Path(self.colima).write_bytes(b'changed after launch')
            return child

        with mock.patch.object(primitive.subprocess, 'Popen', side_effect=launch):
            receipt = self.admitted_d0('fixed-input-drift', self.routed({}, calls), self.quiet_monitor)
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(calls, [])
        self.assertEqual(receipt['steps'][0]['closure'], 'unknown_retained')

    def test_blocked_helper_exhausts_the_stage_budget_before_reserve(self):
        calls, writers = [], []
        real_popen = primitive.subprocess.Popen
        ticks = []

        def launch(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            os.rename(self.colima, self.colima + '.original')
            os.mkfifo(self.colima, 0o600)
            return child

        def sample():
            if not writers and os.path.exists(self.colima + '.original'):
                try:
                    writers.append(os.open(self.colima, os.O_WRONLY | os.O_NONBLOCK))
                except OSError:
                    pass
            return {'storage': 0}

        def clock():
            now = time.monotonic()
            if not ticks:
                ticks.append(True)
                return now - 360 + 60 + 0.6
            return now

        admitted = dict(ex.UNADMITTED)
        admitted.pop('D0', None)
        try:
            with mock.patch.object(primitive.subprocess, 'Popen', side_effect=launch), \
                 mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}), \
                 mock.patch.object(ex, 'UNADMITTED', admitted):
                receipt = self.execute('D0', self.routed({}, calls), activation_id='fixed-deadline', clock=clock,
                                       monitor_factory=lambda limits, roots: resources.Monitor({'storage': 1}, sample))
            self.assertTrue(writers, 'the child reached the FIFO')
            self.assertEqual(receipt['closure'], 'stopped')
            self.assertEqual(receipt['steps'][0]['disposition'], 'timeout')
            self.assertIsNotNone(receipt['steps'][0]['waited_exit'])
            self.assertEqual(calls, [])
            self.assertEqual([item['id'] for item in receipt['steps']], ['d0-make-owned-dirs'])
        finally:
            for fd in writers:
                os.close(fd)

    def counter_loss_case(self, unreapable):
        calls, writers, children = [], [], []
        real_popen = primitive.subprocess.Popen
        real_wait = real_popen.wait

        def launch(*args, **kwargs):
            child = real_popen(*args, **kwargs)
            children.append(child)
            os.rename(self.colima, self.colima + '.original')
            os.mkfifo(self.colima, 0o600)
            return child

        def sample():
            if children and not writers:
                try:
                    writers.append(os.open(self.colima, os.O_WRONLY | os.O_NONBLOCK))
                except OSError:
                    return {'storage': 0}
            if writers:
                raise ValueError('simulated counter loss')
            return {'storage': 0}

        def no_wait(child, timeout=None):
            raise primitive.subprocess.TimeoutExpired(child.args, timeout)

        try:
            with contextlib.ExitStack() as stack:
                if unreapable:
                    stack.enter_context(mock.patch.object(real_popen, 'wait', no_wait))
                stack.enter_context(mock.patch.object(primitive.subprocess, 'Popen', side_effect=launch))
                receipt = self.admitted_d0('fixed-counter-loss', self.routed({}, calls),
                                           lambda limits, roots: resources.Monitor({'storage': 1}, sample))
            self.assertTrue(writers, 'the child reached the FIFO')
            self.assertEqual(receipt['closure'], 'stopped')
            step = receipt['steps'][0]
            self.assertEqual(step['closure'], 'unknown_retained')
            self.assertEqual(step['process']['pid'], children[0].pid)
            if unreapable:
                self.assertEqual(step['disposition'], 'unknown')
                self.assertIsNone(step['waited_exit'])
            else:
                self.assertEqual(step['disposition'], 'resource-refusal')
                self.assertIsNotNone(step['waited_exit'])
            self.assertEqual(calls, [])
        finally:
            for fd in writers:
                os.close(fd)
            for child in children:
                real_wait(child, timeout=5)

    def test_counter_loss_while_fixed_helper_is_blocked(self):
        self.counter_loss_case(False)

    def test_simulated_unreapable_fixed_helper_stays_unknown(self):
        self.counter_loss_case(True)


class PlanTests(Fixture):
    def test_socket_plan_bounds_before_any_claim(self):
        manifest = ex.load_manifest()
        with mock.patch.object(prep, 'STASH', '/Volumes/Stash/scratch/effigy'), \
                mock.patch.object(prep, 'SOCKET_LIMIT_BYTES', 104):
            bindings = self.make_bindings('9b47cb21')
            before = tree(self.tmp)
            for stage in ('C0', 'C1', 'C2', 'C3', 'C4', 'C5', 'C6'):
                plan = ex.resolve_stage(stage, bindings, manifest)
                self.assertTrue(plan['socket_paths'])
                for row in plan['socket_paths']:
                    self.assertEqual(row['bytes'], len(row['path'].encode()))
                    self.assertLessEqual(row['bytes'] + 5, 104)
            legacy = json.loads(json.dumps(manifest).replace('q4', 'effigy-4f6a-qual'))
            old = dict(bindings, colima_home='/Volumes/Stash/scratch/effigy/mac-4f6a-9b47cb21/c')
            with self.assertRaisesRegex(ex.Refusal, r'ssh.sock.1234567890123456 is 105 bytes'):
                ex.resolve_stage('C1', old, legacy)
            self.assertEqual(before, tree(self.tmp))

    def test_plan_is_pure_and_writes_nothing(self):
        before = tree(self.tmp)
        plans = [ex.resolve_stage(s['id'], self.bindings) for s in ex.load_manifest()['stages']]
        self.assertEqual(before, tree(self.tmp))
        self.assertEqual(len(plans), 12)

    def test_admission_is_exact_and_each_refusal_is_named(self):
        plans = {s['id']: ex.resolve_stage(s['id'], self.bindings) for s in ex.load_manifest()['stages']}
        self.assertEqual({k for k, v in plans.items() if not v['static_admission']},
                         {'C0', 'C1', 'C2', 'C3', 'C4', 'C5', 'C6', 'D0', 'D1', 'D2', 'D3', 'D4'})
        for plan in plans.values():  # every resolved request carries the global refusal and its prerequisite
            self.assertEqual(plan['admission'][0], ex.OWNER_ACTIVATION_REASON)
            self.assertFalse(plan['execution']['admitted'])
            self.assertIn('owner-controlled', plan['execution']['prerequisite'])
        self.assertEqual(plans['D1']['static_admission'], [])
        self.assertEqual(plans['D2']['static_admission'], [])
        self.assertEqual(plans['D3']['static_admission'], [])
        self.assertEqual(plans['D4']['static_admission'], [])
        self.assertEqual(plans['C6']['static_admission'], [])
        self.assertEqual(plans['C4']['static_admission'], [])
        self.assertEqual(plans['C2']['static_admission'], [])
        self.assertEqual(plans['C3']['static_admission'], [])
        self.assertEqual(plans['C1']['static_admission'], [])  # the pinned image fetch is admitted; execution still refuses
        self.assertEqual(plans['C5']['static_admission'], [])

    def test_every_helper_has_a_handler_or_an_explicit_reason(self):
        unimplemented = {'verify_frozen_artifacts'}
        self.assertEqual(set(ex.HANDLERS) | unimplemented, set(prep.HELPERS))

    def test_argv_and_env_pins_for_every_exec_step(self):
        for stage in ex.load_manifest()['stages']:
            for step in ex.resolve_stage(stage['id'], self.bindings)['steps']:
                if step['kind'] != 'exec':
                    continue
                for arg in step['argv']:
                    self.assertFalse(any('{' + s + '}' in arg for s in prep.SLOTS), (step['id'], arg))
                env = step['env']
                if step['argv'][0] == prep.COLIMA:
                    self.assertTrue(env['PATH'].startswith(self.bindings['control'] + '/bin:'))
                    self.assertEqual(env['HOME'], self.bindings['control'] + '/h')
                    self.assertNotIn('DOCKER_HOST', env)
                elif step['argv'][0] == self.bindings['desktop_config'] + '/cli-plugins/docker-buildx':
                    self.assertEqual(env['DOCKER_HOST'], prep.DESKTOP_HOST)
                    self.assertEqual(env['DOCKER_CONFIG'], self.bindings['desktop_config'])
                    self.assertEqual(env['PATH'], ex.DESKTOP_BASE_PATH)
                else:
                    self.assertEqual(step['argv'][0], prep.DOCKER)
                    self.assertEqual(env['DOCKER_HOST'], prep.DESKTOP_HOST)
                    self.assertEqual(env['DOCKER_CONFIG'], self.bindings['desktop_config'])
                    self.assertNotIn('DOCKER_CONTEXT', env)
                    self.assertNotIn('HOME', env)
                    self.assertEqual(env['PATH'], ex.DESKTOP_BASE_PATH)

    def test_colima_start_argv_is_the_accepted_zero_share_form(self):
        steps = {s['id']: s for s in ex.resolve_stage('C1', self.bindings)['steps']}
        argv = steps['c1-colima-start']['argv']
        for flag in ('--ssh-config=false', '--activate=false', '--cpus'):
            self.assertIn(flag, argv)
        self.assertEqual(argv[argv.index('--disk-image') + 1], self.bindings['colima_cache_home'] + '/effigy-image/' + prep.COLIMA_IMAGE_NAME)
        self.assertEqual(argv[argv.index('--mount') + 1], 'none')

    def test_plan_shows_runtime_derived_values_as_placeholders_with_their_source(self):
        plan = ex.resolve_stage('D4', self.bindings)
        self.assertIn('<image_id>', [a for s in plan['steps'] if s['id'] == 'd4-observe-image' for a in s['argv']])
        self.assertIn('image_id', plan['runtime_derived'])
        self.assertIn('d4-post-volume', [s['id'] for s in plan['steps']])

    def test_bindings_must_be_the_reviewed_ones_and_derived_or_unpinned_refuse(self):
        cases = [('desktop_host', 'unix:///tmp/other.sock'), ('check_source', 'print(1)'),
                 ('control', self.tmp + '/elsewhere'), ('route_echo', 'printf'), ('not_a_slot', 'x'),
                 ('image_id', IMAGE), ('owned_volume_name', 'x'), ('probe_source', 'print(1)'),
                 ('guest_context', '/x'), ('token32', '0' * 32)]
        for key, value in cases:
            with self.subTest(key=key), self.assertRaises(ex.Refusal):
                ex.resolve_stage('D0', dict(self.bindings, **{key: value}))

    def test_plan_cli_writes_only_stdout(self):
        path = self.tmp + '/bindings.json'
        Path(path).write_text(json.dumps(self.bindings))
        before = tree(self.tmp)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(ex.main(['x', 'plan', path]), 0)
        self.assertEqual(before, tree(self.tmp))
        self.assertEqual(len(json.loads(out.getvalue())), 12)


class ActivationAndClaimTests(Fixture):
    def test_execution_is_refused_without_an_owner_controlled_mechanism(self):
        calls = []
        path, sha = self.activation('D0', None, 'act-noowner-1')
        with mock.patch.dict(ex.TEST_SEAM, {'owner_activation': False}):
            with self.assertRaises(ex.Refusal) as caught:
                ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256=sha,
                                 evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertIn('no owner-controlled activation mechanism', str(caught.exception))
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(self.evidence), [])
        self.assertEqual(os.listdir(self.tmp + '/claims'), [])

    def test_d1_is_admitted_statically_and_ordinary_execution_still_refuses(self):
        d0 = self.d0()
        calls = []
        self.assertEqual(ex.resolve_stage('D1', self.bindings)['static_admission'], [])
        path, sha = self.activation('D1', d0, 'act-d1-unadm')
        with mock.patch.dict(ex.TEST_SEAM, {'owner_activation': False, 'admit_unadmitted': False}):
            with self.assertRaises(ex.Refusal) as caught:
                ex.execute_stage('D1', self.bindings, activation_path=path, activation_sha256=sha,
                                 evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertIn('no owner-controlled activation mechanism', str(caught.exception))
        self.assertEqual(calls, [])

    def test_activation_outside_the_owner_directory_refuses_before_effects(self):
        calls = []
        path = self.tmp + '/loose.json'
        Path(path).write_text('{}')
        with self.assertRaises(ex.Refusal):
            ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256=ex.sha_file(path),
                             evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(self.evidence), [])

    def test_writable_activation_refuses(self):
        path, sha = self.activation('D0', None, 'act-writable-1')
        os.chmod(path, 0o600)
        with self.assertRaises(ex.Refusal):
            ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256=sha,
                             evidence_root=self.evidence, transport=self.routed({}, []), utc_now=NOW)

    def test_wrong_hash_wrong_decision_expired_or_candidate_refuse_with_no_call(self):
        calls = []
        cases = [dict(decision='00000000-0000-0000-0000-000000000000'), dict(expires=-5, issued=-10),
                 dict(candidate='0' * 40)]
        for n, kw in enumerate(cases):
            with self.subTest(kw), self.assertRaises(ex.Refusal):
                path, sha = self.activation('D0', None, f'act-bad-{n:02d}', **kw)
                ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256=sha,
                                 evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        with self.assertRaises(ex.Refusal):
            path, _ = self.activation('D0', None, 'act-wrongsha-1')
            ex.execute_stage('D0', self.bindings, activation_path=path, activation_sha256='0' * 64,
                             evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(self.evidence), [])

    def test_claims_directory_replaced_by_a_link_refuses_and_writes_nothing_elsewhere(self):
        other = self.tmp + '/other-claims'
        os.mkdir(other, 0o700)
        os.rmdir(self.tmp + '/claims')
        os.symlink(other, self.tmp + '/claims')
        with self.assertRaises(ex.Refusal):
            self.execute('D0', self.routed({}, []), activation_id='act-link-0001')
        self.assertEqual(os.listdir(other), [])

    def test_activation_claim_refuses_replay_in_another_evidence_root(self):
        d0 = self.d0()
        self.d1(d0, 'act-once-0001')
        path = f'{self.activations}/act-once-0001.json'
        sha = ex.sha_file(path)
        other = self.tmp + '/evidence-two'
        os.mkdir(other, 0o700)
        calls = []
        with self.assertRaises(ex.Refusal):
            ex.execute_stage('D1', self.bindings, activation_path=path, activation_sha256=sha,
                             evidence_root=other, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(other), [])

    def test_stage_root_collision_refuses_before_any_call(self):
        d0 = self.d0()
        calls = []
        os.mkdir(self.stage_dir('D1', 'act-collide-0001'), 0o700)
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed({}, calls), prior_ref=d0, activation_id='act-collide-0001')
        self.assertEqual(calls, [])


class HelperDispatchTests(Fixture):
    """Each helper handler, with controlled inputs: the accepted and the refused shapes."""

    def ctx(self, **extra):
        root = tempfile.mkdtemp(prefix='step-', dir=self.tmp)
        os.chmod(root, 0o700)
        base = dict(bindings=self.bindings, outputs={}, step_root=root)
        base.update(extra)
        return base

    def write(self, name, text):
        path = self.tmp + '/' + name
        Path(path).write_text(text)
        return path

    def test_route_echo_is_byte_exact_before_any_payload(self):
        route = self.write('route.out', 'wrong')
        ctx = self.ctx(outputs={'c2-route-echo': {'stdout': route}})
        step = {'inputs': ['c2-route-echo.stdout']}
        with self.assertRaises(ex.Refusal):
            ex.h_compare_route_echo(ctx, step)
        Path(route).write_bytes(prep.ROUTE_ECHO.encode())
        self.assertEqual(ex.h_compare_route_echo(ctx, step), {'matched': True})

    def test_recipe_stdin_is_the_reviewed_bytes_with_readback(self):
        probe = self.write('probe.out', json.dumps({'parent': {'path': '/g', 'device': 1, 'inode': 2, 'uid': 501}}))
        ctx = self.ctx(token32='0' * 31 + '1', outputs={'c2-guest-probe': {'stdout': probe}})
        out = ex.h_render_guest_spec(ctx, {'id': 'c3-render-guest-spec', 'inputs': []})
        self.assertEqual((out['sha256'], out['bytes']), (ex.RECIPE_SHA256, ex.RECIPE_BYTES))
        self.assertEqual(Path(out['path']).read_bytes(), ex.RECIPE_PATH.read_bytes())
        self.assertEqual(json.loads(ctx['bindings']['guest_spec'])['name'], 'effigy-531-image-' + '0' * 31 + '1')
        with self.assertRaises(ex.Refusal):  # no token32 from the chain: the spec refuses, it is not guessed
            ex.h_render_guest_spec(self.ctx(outputs={'c2-guest-probe': {'stdout': probe}}),
                                   {'id': 'c3-render-guest-spec', 'inputs': []})

    def test_host_stage_dockerfile_writes_the_reviewed_recipe_exclusively(self):
        context = self.bindings['desktop_context_dir']
        os.makedirs(os.path.dirname(context), 0o700, exist_ok=True)  # D0 creates this parent; here it is made directly
        os.mkdir(context, 0o700)
        target = context + '/Dockerfile'
        ctx = self.ctx()
        out = ex.h_host_stage_dockerfile(ctx, {'inputs': ['recipe_file', target]})
        self.assertEqual((out['sha256'], out['bytes']), (ex.RECIPE_SHA256, ex.RECIPE_BYTES))
        self.assertEqual(Path(target).read_bytes(), ex.RECIPE_PATH.read_bytes())
        with self.assertRaises(ex.Refusal):  # an existing Dockerfile is a collision: never adopted or overwritten
            ex.h_host_stage_dockerfile(ctx, {'inputs': ['recipe_file', target]})
        with self.assertRaises(ex.Refusal):  # only the reviewed recipe is staged
            ex.h_host_stage_dockerfile(ctx, {'inputs': ['other.Dockerfile', context + '/x']})
        with self.assertRaises(ex.Refusal):  # only the context's own Dockerfile
            ex.h_host_stage_dockerfile(ctx, {'inputs': ['recipe_file', context + '/nested/Dockerfile']})

    def test_c3_spec_is_derived_from_the_probe_parent_and_token32(self):
        spec = json.loads(ex.build_guest_spec({'path': '/g', 'device': 1, 'inode': 2, 'uid': 501}, '0' * 31 + '1'))
        self.assertEqual(spec, {'parent': {'path': '/g', 'device': 1, 'inode': 2, 'uid': 501},
                                'name': 'effigy-531-image-' + '0' * 31 + '1', 'sha256': ex.RECIPE_SHA256})
        with self.assertRaises(ex.Refusal):
            ex.build_guest_spec({'path': '/g'}, '0' * 31 + '1')
        with self.assertRaises(ex.Refusal):
            ex.build_guest_spec({'path': '/g', 'device': 1, 'inode': 2, 'uid': 501}, 'short')

    def test_c4_verify_spec_is_derived_from_the_c3_stage_output_or_refused(self):
        stage_out = {'parent': {'path': '/g', 'device': 1, 'inode': 2, 'uid': 501},
                     'path': '/g/effigy-531-image-' + '0' * 31 + '1', 'device': 3, 'inode': 4, 'uid': 501,
                     'file_device': 5, 'file_inode': 6, 'sha256': ex.RECIPE_SHA256}
        spec = json.loads(ex.build_verify_spec(json.dumps(stage_out), FAKE_RUNTIME_PIN))
        self.assertEqual(spec['name'], 'effigy-531-image-' + '0' * 31 + '1')
        self.assertEqual(spec['parent'], stage_out['parent'])
        self.assertEqual(spec['layout'], guest_inputs.pin_layout(FAKE_RUNTIME_PIN))
        self.assertNotIn('parent', spec['context'])
        with self.assertRaises(ex.Refusal):
            ex.build_verify_spec(json.dumps({k: v for k, v in stage_out.items() if k != 'parent'}), FAKE_RUNTIME_PIN)
        with self.assertRaises(ex.Refusal):
            ex.build_verify_spec(json.dumps(dict(stage_out, path='/elsewhere/effigy-531-image-' + '0' * 31 + '1')),
                                 FAKE_RUNTIME_PIN)

    def test_the_historical_531_helper_is_unchanged_and_the_macos_copy_is_pinned(self):
        self.assertEqual(ex.sha_file(HERE / 'image_context.py'),
                         'af74047f640111907930273f7cc7a2b46eac4cb27943f0a512fccf56a1b3b322')
        self.assertEqual(ex.sha_file(ex.GUEST_CONTEXT_PATH), ex.GUEST_CONTEXT_SHA256)
        self.assertEqual(ex.sha_file(ex.GUEST_INPUTS_PATH), ex.GUEST_INPUTS_SHA256)
        sources = ex.guest_source_bindings()
        self.assertEqual(set(sources), {'probe_source', 'stage_source', 'verify_source', 'transfer_source',
                                        'input_verify_source', 'guest_helper_hex'})
        self.assertEqual(sources['probe_source'], ex.GUEST_CONTEXT_PATH.read_text())
        self.assertEqual(sources['transfer_source'], ex.GUEST_INPUTS_PATH.read_text())
        self.assertEqual(sources['input_verify_source'], ex.GUEST_INPUTS_PATH.read_text())
        self.assertEqual(sources['guest_helper_hex'], ex.GUEST_HELPER_PATH.read_bytes().hex())

    def test_read_iidfile_accepts_one_sha256_and_refuses_a_tag(self):
        good = self.write('iid', IMAGE + '\n')
        self.assertEqual(ex.h_read_iidfile(self.ctx(), {'inputs': [good]}), {'image_id': IMAGE})
        bad = self.write('iid2', 'effigy-4f6a-desktop-runtime:latest')
        with self.assertRaises(ValueError):
            ex.h_read_iidfile(self.ctx(), {'inputs': [bad]})

    def test_check_image_identity_refuses_a_different_id_and_wrong_platform(self):
        out = self.write('inspect.out', READBACK)
        ctx = self.ctx(bindings=dict(self.bindings, image_id=IMAGE), outputs={'x': {'stdout': out}})
        self.assertEqual(ex.h_check_image_identity(ctx, {'inputs': ['x.stdout']}), {'verified': IMAGE})
        wrong = self.write('wrong.out', json.dumps({'Id': 'sha256:' + '0' * 64, 'Os': 'linux', 'Architecture': 'arm64'}))
        with self.assertRaises(ValueError):
            ex.h_check_image_identity(self.ctx(bindings=dict(self.bindings, image_id=IMAGE), outputs={'x': {'stdout': wrong}}),
                                      {'inputs': ['x.stdout']})

    def test_check_build_help_needs_the_three_flags(self):
        ok = self.write('help.out', '--iidfile --platform --provenance --network --build-context')
        self.assertEqual(ex.h_check_build_help(self.ctx(outputs={'h': {'stdout': ok}}), {'inputs': ['h.stdout']}),
                         {'verified': True})
        short = self.write('help2.out', '--iidfile')
        with self.assertRaises(ValueError):
            ex.h_check_build_help(self.ctx(outputs={'h': {'stdout': short}}), {'inputs': ['h.stdout']})

    def test_check_runtime_facts_needs_exit_zero_and_root_facts(self):
        out = self.write('facts.out', FACTS)
        ctx = self.ctx(outputs={'c': {'stdout': out, 'exit': 0}})
        self.assertIn('facts', ex.h_check_runtime_facts(ctx, {'inputs': ['c.stdout', 'c.exit']}))
        ctx_bad = self.ctx(outputs={'c': {'stdout': out, 'exit': 1}})
        with self.assertRaises(ValueError):
            ex.h_check_runtime_facts(ctx_bad, {'inputs': ['c.stdout', 'c.exit']})

    def test_read_cidfile_needs_one_container_id(self):
        self.d0()
        activation_id = 'act-cid-0001'
        cid = ex.d3_occurrence(self.bindings, activation_id)['desktop_cidfile']
        Path(cid).write_text(CID)
        ctx = self.ctx(activation_id=activation_id)
        self.assertEqual(ex.h_read_cidfile(ctx, {'inputs': [cid]}),
                         {'container_id': CID, 'path': cid, 'sha256': sha_bytes(CID.encode())})
        Path(cid).write_text('nope')
        with self.assertRaises(ValueError):
            ex.h_read_cidfile(ctx, {'inputs': [cid]})
        with self.assertRaises(ex.Refusal):
            ex.h_read_cidfile(ctx, {'inputs': [self.tmp + '/other']})

    def test_make_owned_dir_refuses_paths_outside_the_control_root(self):
        ctx = self.ctx()
        with self.assertRaises(ex.Refusal):
            ex.h_make_owned_dir(dict(ctx, pins=ex.Pins()), {'inputs': [self.tmp + '/elsewhere']})

    def test_claim_intent_refuses_a_changed_claim(self):
        claim = self.tmp + '/claims/x.claim'
        primitive.record(claim, {'activation_id': 'x', 'go_sha256': 'y'})
        ctx = self.ctx(claim={'path': claim, 'sha256': '0' * 64})
        with self.assertRaises(ex.Refusal):
            ex.h_claim_intent(ctx, {})

    def test_desktop_endpoint_must_be_the_pinned_socket(self):
        with self.assertRaises(ex.Refusal):
            ex.h_verify_desktop_endpoint({'desktop_socket': self.tmp + '/other.sock'}, {})
        self.assertEqual(ex.h_verify_desktop_endpoint({}, {})['socket'], self.socket_path)

    def test_engine_template_and_volume_authority(self):
        with self.assertRaises(ValueError):
            prep.engine_id_from_info('{"ID":"","Containers":0}')
        name = volume_for('act-x-0001')
        self.assertEqual(ex.volume_creation_receipt(name + '\n', TOKEN, inspect_object(name), ENGINE)['id'], name)
        with self.assertRaises(ex.Refusal):  # a token alone: no create output
            ex.volume_creation_receipt('', TOKEN, inspect_object(name), ENGINE)
        with self.assertRaises(ex.Refusal):  # a create that printed no newline-terminated single name
            ex.volume_creation_receipt(name, TOKEN, inspect_object(name), ENGINE)
        with self.assertRaises(ex.Refusal):  # a caller-chosen name, not the engine's 64-hex form
            ex.volume_creation_receipt('effigy-4f6a-qual-x\n', TOKEN, inspect_object('effigy-4f6a-qual-x'), ENGINE)
        with self.assertRaises(ex.Refusal):  # two names in one create output
            ex.volume_creation_receipt(name + '\n' + name + '\n', TOKEN, inspect_object(name), ENGINE)
        with self.assertRaises(ex.Refusal):  # docker's --format object, not a one-element array
            ex.volume_creation_receipt(name + '\n', TOKEN, '[' + inspect_object(name) + ']', ENGINE)
        with self.assertRaises(ex.Refusal):  # an inspect of another volume
            ex.volume_creation_receipt(name + '\n', TOKEN, inspect_object('other'), ENGINE)
        with self.assertRaises(ex.Refusal):  # the same name without this occurrence's label
            ex.volume_creation_receipt(name + '\n', TOKEN, json.dumps({'Name': name, 'Labels': {'effigy.qual': 'other'}}), ENGINE)

    def test_cleanup_selects_only_receipted_creations(self):
        name = volume_for('act-x-0001')
        receipts = [ex.volume_creation_receipt(name + '\n', TOKEN, inspect_object(name), ENGINE)]
        live = [dict(kind='volume', id=name, names=[name], labels=['effigy.qual=' + TOKEN], engine_id=ENGINE),
                dict(kind='volume', id='effigy-4f6a-qual-other', names=['effigy-4f6a-qual-other'],
                     labels=['effigy.qual=' + TOKEN], engine_id=ENGINE)]
        out = prep.select_cleanup_targets(ENGINE, receipts, live)
        self.assertEqual(out['volumes'], [name])
        self.assertEqual(out['unauthorized'], ['effigy-4f6a-qual-other'])

    def test_guest_exit_is_unknown_closure_not_closed(self):
        self.assertEqual(ex.step_closure({'disposition': 'exited', 'waited_exit': 0}, guest=True), 'guest_unknown')
        self.assertEqual(ex.step_closure({'disposition': 'exited', 'waited_exit': 0, 'guest_closure': 'closed'},
                                         guest=True), 'closed')
        self.assertEqual(ex.step_closure({'disposition': 'exited', 'waited_exit': 0}, guest=False), 'closed')
        self.assertEqual(ex.step_closure({'disposition': 'timeout', 'waited_exit': -9}, guest=False), 'unknown_retained')

    def test_prefetch_of_a_pinned_artifact_is_exact_and_exclusive(self):
        data = b'pinned-artifact-bytes'
        digest = hashlib.sha512(data).hexdigest()
        dest = self.tmp + '/artifact.raw'
        import io as _io
        self.assertEqual(ex.prefetch_pinned_artifact(_io.BytesIO(data), dest, len(data), digest),
                         {'bytes': len(data), 'sha512': digest})
        self.assertEqual(stat.S_IMODE(os.lstat(dest).st_mode), 0o600)
        with self.assertRaises(ex.Collision):  # an existing name is a refusal, never adopted
            ex.prefetch_pinned_artifact(_io.BytesIO(data), dest, len(data), digest)
        with self.assertRaises(ex.Refusal):
            ex.prefetch_pinned_artifact(_io.BytesIO(data + b'x'), self.tmp + '/big.raw', len(data), digest)
        with self.assertRaises(ex.Refusal):
            ex.prefetch_pinned_artifact(_io.BytesIO(data), self.tmp + '/bad.raw', len(data), '0' * 128)

    def test_allocated_bytes_sees_real_blocks_and_skips_the_vm_store(self):
        root = self.bindings['control']
        os.makedirs(root + '/_lima', exist_ok=True)
        before = ex.allocated_bytes(root)
        Path(root + '/blob').write_bytes(b'x' * 65536)
        grown = ex.allocated_bytes(root)
        self.assertGreater(grown, before)
        Path(root + '/_lima/disk').write_bytes(b'y' * 65536)
        self.assertEqual(ex.allocated_bytes(root), grown)
        sampler = ex.AllocatedBytes([root], min_interval=0)
        first = sampler()['storage']
        os.remove(root + '/blob')
        self.assertGreaterEqual(sampler()['storage'], first)


class GenuineChainTests(Fixture):
    """Every link is a real execution: D0, D1, D2, D3, D4. Nothing is fabricated."""

    def test_d0_observes_the_engine_and_closes(self):
        d0 = self.d0()
        receipt = self._d0_receipt
        self.assertEqual((receipt['closure'], receipt['engine_id']), ('closed', ENGINE))
        self.assertEqual([c[0] for c in self._d0_calls], ['d0-info', 'd0-ps', 'd0-network', 'd0-volume'])
        self.assert_dispatch('D0', self._d0_calls, self.bindings)
        self.assertTrue(os.path.isdir(self.bindings['control'] + '/d/docker-config'))
        self.assertEqual(d0['stage'], 'D0')

    def test_d0_empty_engine_template_refuses_and_stops(self):
        calls = []
        with self.assertRaises(ex.Refusal):
            self.execute('D0', self.routed({'d0-info': emit('{"ID":"","Containers":0}')}, calls), activation_id='act-d0-empty')
        self.assertEqual([c[0] for c in calls], ['d0-info'])
        self.assertEqual(json.loads(Path(self.stage_dir('D0', 'act-d0-empty') + '/stage-receipt.json').read_text())['closure'], 'refused')

    def test_d1_creates_only_after_observing_the_engine_and_the_label_absent(self):
        d0 = self.d0()
        calls = []
        receipt, ref = self.d1(d0, calls=calls)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info', 'd1-label-absent', 'd1-volume-create', 'd1-volume-inspect'])
        volume = volume_for('act-d1-0001')
        # The create names no volume: the engine generates the name and prints it, and the inspect reads that name back.
        self.assertEqual(calls[2][1], [prep.DOCKER, 'volume', 'create', '--label', 'effigy.qual=' + TOKEN])
        self.assertEqual(calls[3][1][-1], volume)
        self.assertIn('label=effigy.qual=' + TOKEN, calls[1][1])  # the absence observation is by this occurrence's label
        [rec] = receipt['receipts']
        self.assertEqual((rec['kind'], rec['id'], rec['engine_id']), ('volume', volume, ENGINE))
        self.assertEqual((rec['driver'], rec['scope'], rec['created_at']), ('local', 'local', '2026-11-08T09:20:01Z'))
        self.assertEqual(rec['labels'], {'effigy.qual': TOKEN})

    def test_d1_existing_label_refuses_before_create(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(inspect_object('x') + '\n')}
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-d1-exists')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info', 'd1-label-absent'])

    def test_d1_engine_change_refuses_before_absence(self):
        d0 = self.d0()
        calls = []
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed({'d1-engine-info': emit('{"ID":"other"}')}, calls), prior_ref=d0,
                     activation_id='act-d1-engine')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info'])

    def test_d1_array_inspect_is_refused(self):
        d0 = self.d0()
        volume = volume_for('act-d1-array')
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                  'd1-volume-create': create_child(volume), 'd1-volume-inspect': emit('[' + inspect_object(volume) + ']')}
        calls = []
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-d1-array')
        self.assertEqual(calls[-1][0], 'd1-volume-inspect')

    def test_early_failure_keeps_custody_and_stops_later_steps(self):
        d0 = self.d0()
        volume = volume_for('act-fail-0001')
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                  'd1-volume-create': emit('', 3, 'denied')}
        calls = []
        receipt = self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-fail-0001')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(calls[-1][0], 'd1-volume-create')
        step = receipt['steps'][-1]
        self.assertEqual((step['disposition'], step['waited_exit'], step['closure']), ('failed', 3, 'failed_closed'))
        self.assertEqual(Path(self.stage_dir('D1', 'act-fail-0001') + '/d1-volume-create/stderr').read_text(), 'denied')
        self.assertNotIn(volume, [c[1][-1] for c in calls if c[0] == 'd1-volume-inspect'])

    def test_deadline_is_unknown_retained_and_stops(self):
        d0 = self.d0()
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': 'import time; time.sleep(30)'}
        calls = []
        transport = self.routed(routes, calls)

        def short(**kw):
            return transport(**dict(kw, timeout=min(kw['timeout'], 1.0)))
        receipt = self.execute('D1', short, prior_ref=d0, activation_id='act-unknown-0001')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info', 'd1-label-absent'])
        self.assertEqual(receipt['steps'][-1]['closure'], 'unknown_retained')

    def test_shared_deadline_reduces_later_timeouts_and_keeps_the_reserve(self):
        clock = [1000.0]
        seen = []

        def record(kw):
            seen.append((kw['step'], kw['timeout'], kw['deadline']))

        def spend(kw):
            if kw['step'] == 'd0-info':
                clock[0] += 290.0  # the first step spends most of the stage budget, after it returns
        calls = []
        routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit(''), 'd0-network': emit(''), 'd0-volume': emit('')}
        transport = self.routed(routes, calls, before=record, after=spend, clock=lambda: clock[0])
        receipt = self.execute('D0', transport, activation_id='act-clock-0001', clock=lambda: clock[0])
        self.assertEqual(receipt['closure'], 'closed')
        first, second = seen[0], seen[1]
        self.assertEqual(first[1], 60)
        self.assertAlmostEqual(second[1], 10.0)
        self.assertEqual(second[2], 1000.0 + 360.0 - 60.0)

    def test_budget_spent_before_launch_refuses_without_a_call(self):
        calls = []
        ticks = iter([1000.0] + [1400.0] * 100)
        with self.assertRaises(ex.Refusal):
            self.execute('D0', self.routed({}, calls), activation_id='act-late-0001', clock=lambda: next(ticks))
        self.assertEqual(calls, [])

    def test_live_counter_interrupts_inside_the_wait(self):
        d0 = self.d0()
        # initial sample, the loop's pre-step check, the launch check; growth past the 1 MiB cap comes next
        samples = iter([0, 0, 0] + [2 * MIB] * 10000)
        calls = []
        routes = {'d1-engine-info': 'import time; time.sleep(30)', 'd1-label-absent': emit('')}

        def factory(limits, roots):
            return resources.Monitor(limits, lambda: {'storage': next(samples)})
        receipt = self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-counter-0001',
                           monitor_factory=factory)
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual([c[0] for c in calls], ['d1-engine-info'])
        self.assertEqual(receipt['steps'][-1]['disposition'], 'resource-refusal')

    def test_growth_between_windows_counts_against_the_same_stage_cap(self):
        """A file that appears in the owned control root after the first window is growth the next window
        must see: the one stage-wide monitor refuses before the second step launches."""
        calls = []

        def grow(kw):
            Path(self.bindings['control'] + '/grown').write_bytes(b'x' * 65536)
        routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit('')}

        def uncached(limits, roots):
            monitor = resources.Monitor({'storage': 1}, ex.AllocatedBytes(roots, min_interval=0))
            monitor.limits = dict(limits)  # the exact requested limit, zero included
            return monitor
        receipt = self.execute('D0', self.routed(routes, calls, after=grow), activation_id='act-growth-0001',
                               monitor_factory=uncached)
        self.assertEqual(receipt['closure'], 'stopped')  # the post observation refused; retained, not advanced
        self.assertEqual([c[0] for c in calls], ['d0-info'])
        obs = json.loads(Path(self.stage_dir('D0', 'act-growth-0001') + '/d0-info/observations.json').read_text())
        self.assertIn('cap', obs['post_refusal'])  # the monitor's cap refusal, retained

    def test_control_root_replaced_between_steps_refuses_before_the_next_step(self):
        calls = []
        control = self.bindings['control']

        def replace(kw):
            os.rename(control, control + '.moved')
            os.mkdir(control, 0o700)
        routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit('')}
        with self.assertRaises(ex.Refusal):
            self.execute('D0', self.routed(routes, calls, after=replace), activation_id='act-replace-0001')
        self.assertEqual([c[0] for c in calls], ['d0-info'])

    def test_observations_identity_and_separate_streams_are_retained(self):
        calls = []
        self.execute('D0', self.routed({'d0-info': emit(ENGINE_JSON, err='engine-stderr'), 'd0-ps': emit('ps-out'),
                                    'd0-network': emit(''), 'd0-volume': emit('')}, calls), activation_id='act-obs-0001')
        d0 = self.stage_dir('D0', 'act-obs-0001') + '/d0-info'
        obs = json.loads(Path(d0 + '/observations.json').read_text())
        self.assertEqual(set(obs), {'before', 'post', 'maxima', 'post_refusal'})
        self.assertIsNone(obs['post_refusal'])
        self.assertIn('start_monotonic', json.loads(Path(d0 + '/process.json').read_text()))
        self.assertEqual(Path(d0 + '/stdout').read_text(), ENGINE_JSON)
        self.assertEqual(Path(d0 + '/stderr').read_text(), 'engine-stderr')

    def test_intent_is_visible_before_the_child_starts(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': "import os,sys\nsys.stdout.write('%s' if os.path.exists('intent.json') else 'missing')\n" % ENGINE_JSON,
                  'd1-label-absent': emit(''), 'd1-volume-create': emit(''),
                  'd1-volume-inspect': emit('')}
        with self.assertRaises(ex.Refusal):  # the empty create output refuses at its post-check
            self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-intent-0001')
        self.assertEqual(Path(self.stage_dir('D1', 'act-intent-0001') + '/d1-engine-info/stdout').read_text(), ENGINE_JSON)

    def test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        d2_calls, d3_calls, d4_calls = [], [], []
        d2, d2_ref = self.d2(d1_ref, calls=d2_calls)
        self.assertEqual(d2['closure'], 'closed')
        self.assertEqual([r['kind'] for r in d2['receipts']], ['volume', 'image'])
        self.assertEqual(Path(self.d2_out()['desktop_context_dir'] + '/Dockerfile').read_bytes(), ex.RECIPE_PATH.read_bytes())
        layout = Path(self.d2_out()['desktop_input_dir'] + '/oci')
        self.assertEqual((layout / 'oci-layout').read_bytes(), b'{"imageLayoutVersion":"1.0.0"}\n')
        self.assertTrue((layout / 'index.json').is_file())
        self.assertEqual(len(list(Path(self.d2_out()['desktop_input_dir'] + '/debs').iterdir())), len(runtime_inputs.DEBS))
        self.assert_dispatch('D2', d2_calls, dict(self.bindings, **self.d2_out(), image_id=IMAGE, owned_volume_name=volume_for('act-d1-0001')))
        d3, d3_ref = self.d3(d2_ref, calls=d3_calls)
        self.assertEqual(d3['closure'], 'closed')
        self.assertEqual({r['kind'] for r in d3['receipts']}, {'volume', 'image', 'container'})
        self.assert_dispatch('D3', d3_calls, dict(self.bindings, **ex.d3_occurrence(self.bindings, 'act-d3-0001'), image_id=IMAGE))
        volume = volume_for('act-d1-0001')
        ps_rows = '\n'.join([json.dumps({'ID': CID, 'Names': ex.d3_occurrence(self.bindings, 'act-d3-0001')['desktop_container_name'], 'Labels': 'effigy.qual=' + TOKEN}),
                             json.dumps({'ID': 'b' * 64, 'Names': 'effigy-4f6a-dcheck-other', 'Labels': 'effigy.qual=' + TOKEN}),
                             json.dumps({'ID': 'c' * 64, 'Names': 'web', 'Labels': ''})])
        foreign_ps = '\n'.join([json.dumps({'ID': 'b' * 64, 'Names': 'effigy-4f6a-dcheck-other', 'Labels': 'effigy.qual=' + TOKEN}),
                                json.dumps({'ID': 'c' * 64, 'Names': 'web', 'Labels': ''})])
        vol_rows = '\n'.join([json.dumps({'Name': volume, 'Labels': 'effigy.qual=' + TOKEN}),
                              json.dumps({'Name': 'foreign-vol', 'Labels': ''})])
        routes = self.d4_routes(volume, ps=ps_rows, vol_rows=vol_rows, inventory=(IMAGE, 'sha256:' + 'cc' * 32),
                                post_ps=foreign_ps, post_vol=json.dumps({'Name': 'foreign-vol', 'Labels': ''}))
        d4 = self.execute('D4', self.routed(routes, d4_calls), prior_ref=d3_ref, activation_id='act-d4-0001')
        self.assertEqual(d4['closure'], 'closed')
        steps = {c[0]: c[1] for c in d4_calls}
        self.assertEqual(steps['d4-owned-rm'][-1], CID)
        self.assertEqual(steps['d4-owned-volume-rm'][-1], volume)
        self.assertEqual(steps['d4-owned-image-rm'][-1], IMAGE)
        seen = [a for _, argv, _ in d4_calls for a in argv]
        self.assertFalse(any(x in a for a in seen for x in ('b' * 64, 'c' * 64, 'foreign-vol', 'cc' * 32)))
        self.assertFalse(any(a in ('-f', '--force', 'prune') for a in seen))
        # Each removal is preceded, in its own class, by a fresh engine observation and an exact inspect.
        order = [c[0] for c in d4_calls]
        for kind in ('container', 'volume', 'image'):
            self.assertLess(order.index(f'd4-{kind}-inspect'), order.index(f'd4-owned-{kind}-rm' if kind != 'container' else 'd4-owned-rm'))

    def test_d4_refuses_when_a_foreign_container_appears_during_cleanup(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        _, d2_ref = self.d2(d1_ref)
        _, d3_ref = self.d3(d2_ref)
        volume = volume_for('act-d1-0001')
        ps_rows = json.dumps({'ID': CID, 'Names': ex.d3_occurrence(self.bindings, 'act-d3-0001')['desktop_container_name'], 'Labels': 'effigy.qual=' + TOKEN})
        routes = self.d4_routes(volume, ps=ps_rows, post_ps=json.dumps({'ID': 'd' * 64, 'Names': 'new', 'Labels': ''}))
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('D4', self.routed(routes, []), prior_ref=d3_ref, activation_id='act-d4-foreign')
        self.assertIn('foreign list changed', str(caught.exception))

    def test_d4_skips_removal_when_nothing_is_receipted(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        _, d2_ref = self.d2(d1_ref)
        _, d3_ref = self.d3(d2_ref)
        routes = self.d4_routes(volume_for('act-d1-0001'), ps='', vol_rows='', inventory=('sha256:' + 'cd' * 32,),
                                post_inventory=['sha256:' + 'cd' * 32], overrides={
                                    'd4-observe-image': emit(IMAGE_OBJECT.replace('ab' * 32, 'cd' * 32)),
                                    'd4-post-images': emit(json.dumps({'ID': 'cd' * 32}) + '\n')})
        calls = []
        d4 = self.execute('D4', self.routed(routes, calls), prior_ref=d3_ref, activation_id='act-d4-skip')
        self.assertEqual(d4['closure'], 'closed')
        skipped = [s['id'] for s in d4['steps'] if s.get('skipped')]
        self.assertEqual(sorted(skipped), sorted(ex.CLEANUP_CLASS))

    def test_d2_positive_stages_the_dockerfile_then_builds_and_reads_back(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        calls = []
        receipt, _ = self.d2(d1_ref, calls=calls)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['d2-engine-info', 'd2-buildx-metadata', 'd2-docker-build', 'd2-readback'])
        steps = {s['id']: s for s in receipt['steps']}
        self.assertEqual(steps['d2-host-stage']['closure'], 'closed')
        self.assertEqual(steps['d2-check-image-identity']['closure'], 'closed')

    def test_d2_unknown_build_closure_stops_before_any_readback_or_image_receipt(self):
        """The build's allowance runs out: its closure is unknown, so the stage stops. The iidfile is never read,
        and no image receipt is written, though the reviewed Dockerfile was staged before the build."""
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        calls = []
        receipt, _ = self.d2(d1_ref, activation_id='act-d2-unknown', calls=calls,
                             build='import time; time.sleep(30)', wrap=self.cap('d2-docker-build', 1.0))
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual([c[0] for c in calls], ['d2-engine-info', 'd2-buildx-metadata', 'd2-docker-build'])
        steps = {s['id']: s for s in receipt['steps']}
        self.assertEqual(steps['d2-docker-build']['closure'], 'unknown_retained')
        self.assertNotIn('d2-read-iidfile', steps)
        self.assertNotIn('image', [r['kind'] for r in receipt['receipts']])
        self.assertEqual(Path(self.d2_out('act-d2-unknown')['desktop_context_dir'] + '/Dockerfile').read_bytes(), ex.RECIPE_PATH.read_bytes())

    def test_d2_refuses_a_pre_existing_occurrence_directory_before_the_build(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        out = self.d2_out('act-d2-stale')
        os.mkdir(out['desktop_context_dir'], 0o700)  # an earlier occurrence's directory is a collision, never adopted
        Path(out['desktop_context_dir'] + '/Dockerfile').write_text('stale')
        calls = []
        with self.assertRaises(ex.Refusal):
            self.d2(d1_ref, activation_id='act-d2-stale', calls=calls)
        self.assertEqual([c[0] for c in calls], ['d2-engine-info', 'd2-buildx-metadata'])
        self.assertEqual(json.loads(Path(self.stage_dir('D2', 'act-d2-stale') + '/stage-receipt.json').read_text())['closure'], 'refused')
        self.assertEqual(Path(out['desktop_context_dir'] + '/Dockerfile').read_text(), 'stale')

class ChainVerifierTests(Fixture):
    """The verifier refuses fabricated, altered or incomplete chains. Each test changes one thing in a
    genuine chain and expects a refusal."""

    def genuine_d2(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        receipt, d2_ref = self.d2(d1_ref)
        return d1_ref, d2_ref

    def test_a_fabricated_closed_predecessor_with_no_steps_is_refused(self):
        _, d2_ref = self.genuine_d2()
        body = json.loads(Path(d2_ref['path']).read_text())
        body['steps'] = []
        Path(d2_ref['path']).write_text(json.dumps(body, sort_keys=True, indent=2) + '\n')
        forged = dict(d2_ref, sha256=ex.sha_file(d2_ref['path']))
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('D3', self.routed({}, []), prior_ref=forged, activation_id='act-forge-0001')
        self.assertIn('inventory', str(caught.exception))

    def test_a_predecessor_without_its_claim_is_refused(self):
        _, d2_ref = self.genuine_d2()
        os.remove(json.loads(Path(d2_ref['path']).read_text())['claim']['path'])
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=d2_ref, activation_id='act-noclaim-0001')

    def test_altered_step_output_breaks_the_recorded_stream_hash(self):
        _, d2_ref = self.genuine_d2()
        out = Path(self.stage_dir('D1', 'act-d1-0001') + '/d1-volume-inspect/stdout')
        out.write_text(inspect_object('other'))
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=d2_ref, activation_id='act-altered-0001')

    def test_a_creation_receipt_not_derived_from_its_outputs_is_refused(self):
        _, d2_ref = self.genuine_d2()
        body = json.loads(Path(d2_ref['path']).read_text())
        rec = next(r for r in body['receipts'] if r['kind'] == 'volume')
        # Rewrite the volume receipt's name and the stage record to match, keep the old evidence hash.
        rec['id'] = rec['name'] = 'effigy-4f6a-qual-' + TOKEN + '-act-other'
        Path(d2_ref['path']).write_text(json.dumps(body, sort_keys=True, indent=2) + '\n')
        forged = dict(d2_ref, sha256=ex.sha_file(d2_ref['path']))
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=forged, activation_id='act-derive-0001')

    def altered_d2(self, mutate):
        """A genuine D2 predecessor whose volume receipt is rewritten by mutate(rec) and whose record hash is
        recomputed: the file is self-consistent, so only field-level verification can refuse it."""
        if not hasattr(self, '_d2_ref'):  # the genuine chain is built once; each mutation starts from its bytes
            _, self._d2_ref = self.genuine_d2()
            self._d2_bytes = Path(self._d2_ref['path']).read_bytes()
        Path(self._d2_ref['path']).write_bytes(self._d2_bytes)
        body = json.loads(self._d2_bytes)
        rec = next(r for r in body['receipts'] if r['kind'] == 'volume')
        mutate(rec)
        Path(self._d2_ref['path']).write_text(json.dumps(body, sort_keys=True, indent=2) + '\n')
        return dict(self._d2_ref, sha256=ex.sha_file(self._d2_ref['path']))

    def test_a_self_consistent_altered_name_label_engine_or_kind_is_refused(self):
        mutations = {
            'name': lambda r: r.update(name='effigy-4f6a-qual-' + TOKEN + '-act-x'),
            'label': lambda r: r.update(label='effigy.qual=ffffffff'),
            'engine': lambda r: r.update(engine_id='other-engine'),
            'kind': lambda r: r.update(kind='container'),
        }
        for field, mutate in mutations.items():
            with self.subTest(field=field):
                forged = self.altered_d2(mutate)
                with self.assertRaises(ex.Refusal):
                    self.execute('D3', self.routed({}, []), prior_ref=forged, activation_id='act-field-' + field)

    def test_an_altered_recorded_create_argv_that_no_longer_names_the_volume_is_refused(self):
        """The creating step's recorded argv is the source of the creation fields. A self-consistent edit to it
        (with its stage receipt hash recomputed) must be refused by the field-level check."""
        d2_ref = self.altered_d2(lambda rec: None)  # restores the genuine D2 bytes first
        d1_root = self.stage_dir('D1', 'act-d1-0001')
        receipt = json.loads(Path(d1_root + '/stage-receipt.json').read_text())
        for step in receipt['steps']:
            if step['id'] == 'd1-volume-create':
                step['argv'] = step['argv'] + ['effigy-other']  # a caller-chosen name in the create
        Path(d1_root + '/stage-receipt.json').write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
        d2_body = json.loads(Path(d2_ref['path']).read_text())
        d2_body['prior'] = dict(d2_body['prior'], sha256=ex.sha_file(d1_root + '/stage-receipt.json'))
        Path(d2_ref['path']).write_text(json.dumps(d2_body, sort_keys=True, indent=2) + '\n')
        forged = dict(d2_ref, sha256=ex.sha_file(d2_ref['path']))
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=forged, activation_id='act-argv-alt-0001')

    def test_a_private_docker_config_with_a_changed_mode_refuses_before_effects(self):
        d0 = self.d0()
        os.chmod(self.bindings['desktop_config'], 0o755)
        calls = []
        try:
            with self.assertRaises(ex.Refusal):
                self.d1(d0, calls=calls)
        finally:
            os.chmod(self.bindings['desktop_config'], 0o700)
        self.assertEqual(calls, [])

    def test_the_wrong_preceding_stage_is_refused(self):
        d1_ref, _ = self.genuine_d2()
        with self.assertRaises(ex.Refusal):
            self.execute('D3', self.routed({}, []), prior_ref=d1_ref, activation_id='act-wrongprior-0001')

    def test_a_stopped_predecessor_cannot_be_consumed(self):
        d0 = self.d0()
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                  'd1-volume-create': emit('', 3, 'denied')}
        self.execute('D1', self.routed(routes, []), prior_ref=d0, activation_id='act-stopped-d1')
        stopped = self.ref('D1', 'act-stopped-d1')
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('D2', self.routed({}, []), prior_ref=stopped, activation_id='act-stopped-d2')
        self.assertIn('not a closed receipt', str(caught.exception))


class DispatchSeamTests(Fixture):
    """C6 is dispatched with verify_chain replaced by a stub that supplies only the receipts the stage reads.
    This is labelled dispatch, not chain verification. C5 runs on the genuine C4 receipt (C5ChainTests)."""

    def setUp(self):
        super().setUp()
        patcher = mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN)
        patcher.start()
        self.addCleanup(patcher.stop)
        roots = {}
        for name, rel in (('control', ''), ('colima_home', '/c'), ('qualification_root', '/s')):
            path = self.bindings['control'] + rel
            if rel:
                os.mkdir(path, 0o700)
            info = os.lstat(path)
            roots[name] = dict(path=path, device=info.st_dev, inode=info.st_ino, entries=len(os.listdir(path)))
        patcher = mock.patch.object(ex, 'RECORDED_ROOTS', roots)
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_c6_host_step_dispatches_under_the_recorded_roots_and_closes(self):
        stub = {'receipts': [], 'engine_id': None}
        calls = []
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute('C6', self.routed({'c6-colima-list': emit('[]')}, calls), prior_ref=None,
                               activation_id='act-c6-0001')
        self.assertEqual(receipt['closure'], 'closed')
        self.assert_dispatch('C6', calls, self.bindings)
        self.assertEqual(os.readlink(self.bindings['control'] + '/bin/docker'), prep.DOCKER)


# C5 on the genuine C4 receipt (C4 runs for real through run_c4; C5 reads its receipt from disk). The guest is a shim:
# the reviewed spec is validated by the production helper (inner_argv), the reviewed argv runs for real against a fake
# sudo and nerdctl whose lifecycle is simulated in a fixture state file, and the production envelope format carries the
# result back. Only the Linux /proc identity of the guest and the nerdctl lifecycle are fixtures.
FAKE_SUDO = '''#!{python}
import os, sys
args = sys.argv[1:]
if args[:1] == ['-n']:
    args = args[1:]
os.execvp(args[0], args)
'''
FAKE_NERDCTL = '''#!{python}
import hashlib, json, os, subprocess, sys
state_path = os.environ['FAKE_NERDCTL_STATE']
def load():
    try:
        with open(state_path) as handle:
            return json.load(handle)
    except FileNotFoundError:
        return {'containers': {}}
def save(state):
    with open(state_path, 'w') as handle:
        json.dump(state, handle)
FACTS = '{"uid":0,"python":"3.11.9","ssl":"OpenSSL 3.0","loader":"GNU C Library 2.35","coreutils":"coreutils 9"}'
args = sys.argv[1:]
state = load()
if args == ['--version']:
    print(os.environ.get('FAKE_NERDCTL_VERSION', 'nerdctl version 2.3.1'))
    sys.exit(0)
if args[:1] == ['run']:
    cidfile = args[args.index('--cidfile') + 1]
    name = args[args.index('--name') + 1]
    image = args[args.index('--name') + 2]
    if os.path.exists(cidfile):
        sys.stderr.write('container ID file found, make sure the other container is not running\\n')
        sys.exit(1)
    cid = hashlib.sha256((name + image).encode()).hexdigest()
    with open(cidfile, 'w') as handle:
        handle.write(os.environ.get('FAKE_CIDFILE_TEXT', cid + '\\n'))
    state['containers'][cid] = {'Id': cid, 'Image': image, 'Status': os.environ.get('FAKE_RUN_STATUS', 'exited'),
                                'ExitCode': int(os.environ.get('FAKE_RUN_EXIT', '0'))}
    if os.environ.get('FAKE_LEFTOVER'):
        child = subprocess.Popen(['sleep', '30'], start_new_session=True)
        with open(os.environ['FAKE_PID_FILE'], 'w') as handle:
            handle.write(str(child.pid))
    save(state)
    print(FACTS)
    sys.exit(int(os.environ.get('FAKE_RUN_EXIT', '0')))
if args[:2] == ['container', 'inspect']:
    cid = args[2]
    if os.environ.get('FAKE_INSPECT_MISSING') or cid not in state['containers']:
        sys.stderr.write('No such container: ' + cid + '\\n')
        sys.exit(1)
    row = state['containers'][cid]
    print(json.dumps([{'Id': os.environ.get('FAKE_INSPECT_ID', row['Id']),
                       'Image': os.environ.get('FAKE_INSPECT_IMAGE', row['Image']),
                       'State': {'Status': row['Status'], 'ExitCode': row['ExitCode']}}]))
    sys.exit(0)
if args[:2] == ['container', 'rm']:
    cid = args[2]
    if cid not in state['containers']:
        sys.stderr.write('No such container: ' + cid + '\\n')
        sys.exit(1)
    if state['containers'][cid]['Status'] == 'running':
        sys.stderr.write('container ' + cid + ' is in running status. unpause/stop container first or force removal\\n')
        sys.exit(1)
    if int(os.environ.get('FAKE_RM_EXIT', '0')) != 0:
        sys.exit(int(os.environ['FAKE_RM_EXIT']))
    if not os.environ.get('FAKE_RM_NOOP'):
        del state['containers'][cid]
        save(state)
    print(cid)
    sys.exit(0)
if args[:2] == ['container', 'ls']:
    ids = list(state['containers'])
    if os.environ.get('FAKE_FOREIGN'):
        ids.append(os.environ['FAKE_FOREIGN'])
    print('\\n'.join(ids) + ('\\n' if ids else ''), end='')
    sys.exit(0)
sys.stderr.write('unsupported fake nerdctl invocation: ' + repr(args) + '\\n')
sys.exit(2)
'''
GUEST_SHIM = '''import hashlib, json, subprocess, sys
here, op, nonce, spec, env_json, pin = sys.argv[1:7]
sys.path.insert(0, here)
import macos_guest_helper as helper
argv = helper.inner_argv(op, [spec])
env = json.loads(env_json)
done = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, timeout=30)
out, err = done.stdout, done.stderr
header = dict(nonce=nonce, operation=op, boot_id='boot-1', pid=11, start_time=100, pgid=11, session_id=11,
              source_sha256=pin)
if op in helper.REMOTE:
    header['command'] = list(argv)
if op == 'disposable-check' and env.get('FAKE_TAMPER_COMMAND'):
    header['command'] = ['sudo', '-n', 'nerdctl', 'run', '--rm']
trailer = dict(waited_exit=done.returncode, child_pid=12, child_start_time=101, child_pgid=11, child_session_id=11,
               group_empty=not (op == 'disposable-check' and env.get('FAKE_LEFTOVER')), unreaped=False,
               stdout_sha256=hashlib.sha256(out).hexdigest(), stderr_sha256=hashlib.sha256(err).hexdigest())
sys.stdout.buffer.write(helper.format_envelope(header, out, err, trailer))
'''


class DispatchSweepTests(Fixture):
    """C1 to C4 run through the adapter's entry point. Each stage's predecessor is stubbed (verify_chain), since
    C0 and the guest stages cannot produce genuine receipts here. Local children stand in for the tools. Each
    test checks the argv and env the adapter dispatched against the manifest, the helpers it ran, and
    authenticated guest closure for the fixed echo/probe/stage/verify steps. C4's engine-side build stays
    guest_unknown. C0 runs its reviewed helpers under the fixture seam."""

    def setUp(self):
        super().setUp()
        for patcher in (mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN),):
            patcher.start()
            self.addCleanup(patcher.stop)
        for rel in ('/c', '/s'):  # the recorded roots exist before their identities and entry counts are read
            os.mkdir(self.bindings['control'] + rel, 0o700)
        roots = {}
        for name, rel in (('control', ''), ('colima_home', '/c'), ('qualification_root', '/s')):
            path = self.bindings['control'] + rel
            info = os.lstat(path)
            roots[name] = dict(path=path, device=info.st_dev, inode=info.st_ino, entries=len(os.listdir(path)))
        patcher = mock.patch.object(ex, 'RECORDED_ROOTS', roots)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.roots_at_start = roots

    def run_sweep(self, stage, routes, prior_stub=None):
        calls = []
        stub = prior_stub if prior_stub is not None else {'receipts': [], 'engine_id': None}
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute(stage, self.routed(routes, calls), prior_ref=None,
                                   activation_id=f'act-sweep-{stage.lower()}')
        return receipt, calls

    def test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche(self):
        self.use_historical()
        fresh = self.make_bindings('f0f0f0f0')
        calls = []
        routes = {'c0-colima-start-help': emit('help')}
        receipt = self.execute('C0', self.routed(routes, calls), prior_ref=None, activation_id='act-c0-fresh-1',
                               bindings=fresh)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['c0-colima-start-help'])
        self.assertTrue(os.path.isdir(fresh['control'] + '/s'))
        self.assertTrue(os.path.isdir(fresh['colima_home']))
        self.assertEqual([s['id'] for s in receipt['steps']],
                         ['c0-verify-tools', 'c0-verify-tranche', 'c0-claim-intent',
                          'c0-make-owned-roots', 'c0-colima-start-help'])

    def test_fixed_c0_helpers_use_the_real_supervised_dispatch(self):
        self.use_historical()
        fresh = self.make_bindings('f0f0f0a1')
        calls = []
        admitted = dict(ex.UNADMITTED)
        admitted.pop('C0', None)
        hashes = {'colima': ex.sha_file(self.colima), 'docker': ex.sha_file(self.docker)}
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}), \
             mock.patch.object(ex, 'UNADMITTED', admitted), \
             mock.patch.object(ex, 'HISTORICAL_TOOL_HASHES', hashes):
            receipt = self.execute('C0', self.routed({'c0-colima-start-help': emit('help')}, calls),
                                   activation_id='fixed-c0', bindings=fresh,
                                   monitor_factory=FixedHelperTests.quiet_monitor)
        failure = receipt['steps'][-1] if receipt['steps'] else {}
        stderr = Path(receipt['stage_root'], failure.get('id', ''), 'stderr')
        self.assertEqual(receipt['closure'], 'closed', (receipt, stderr.read_text() if stderr.exists() else ''))
        self.assertEqual([s['id'] for s in receipt['steps'][:4]],
                         ['c0-verify-tools', 'c0-verify-tranche', 'c0-claim-intent', 'c0-make-owned-roots'])
        for step in receipt['steps'][:4]:
            self.assertEqual(step['waited_exit'], 0)
            self.assertIn('helper_receipt', step)
        self.assertEqual([entry[0] for entry in calls], ['c0-colima-start-help'])

    def test_origin_token32_is_the_c0_mint_read_from_its_verified_receipt_and_refuses_other_chains(self):
        self.use_historical()
        fresh = self.make_bindings('f0f0f0f2')
        self.execute('C0', self.routed({'c0-colima-start-help': emit('help')}, []), prior_ref=None,
                     activation_id='act-c0-origin-1', bindings=fresh)
        root = self.stage_dir('C0', 'act-c0-origin-1')
        c0 = json.loads(Path(root + '/stage-receipt.json').read_text())
        minted = c0['origin']['token32']
        self.assertEqual(json.loads(Path(root + '/token32.json').read_text()), {'token32': minted})
        self.assertEqual(ex.origin_token32(c0, 'f0f0f0f2'), minted)
        self.assertRegex(minted, r'^[0-9a-f]{32}$')
        with self.assertRaises(ex.Refusal):  # another token's origin
            ex.origin_token32(c0, TOKEN)
        with self.assertRaises(ex.Refusal):  # a chain with no executor origin
            ex.origin_token32({'stage': 'C2', 'receipts': []}, 'f0f0f0f2')
        with self.assertRaises(ex.Refusal):  # a historical C0 minted no token32
            ex.origin_token32(dict(stage='C0', closure='closed', historical=True, token=TOKEN), TOKEN)
        tampered = dict(c0, origin=dict(c0['origin'], token32='f' * 32))  # the receipt's token differs from its file
        with self.assertRaises(ex.Refusal):
            ex.origin_token32(tampered, 'f0f0f0f2')

    def test_each_c0_activation_mints_its_own_origin_token32(self):
        self.use_historical()
        tokens = []
        for n, token in enumerate(('f0f0f0f3', 'f0f0f0f4')):
            self.execute('C0', self.routed({'c0-colima-start-help': emit('help')}, []), prior_ref=None,
                         activation_id=f'act-c0-mint-{n}', bindings=self.make_bindings(token))
            receipt = json.loads(Path(self.stage_dir('C0', f'act-c0-mint-{n}') + '/stage-receipt.json').read_text())
            tokens.append(receipt['origin']['token32'])
        self.assertNotEqual(tokens[0], tokens[1])
        for value in tokens:
            self.assertRegex(value, r'^[0-9a-f]{32}$')

    def test_c0_refuses_on_a_tool_changed_since_the_historical_record_before_creating_roots(self):
        self.use_historical()
        fresh = self.make_bindings('f0f0f0f1')
        Path(self.docker).write_bytes(b'a different docker\n')
        calls = []
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('C0', self.routed({}, calls), prior_ref=None, activation_id='act-c0-tool-1', bindings=fresh)
        self.assertIn('docker differs', str(caught.exception))
        self.assertEqual(calls, [])
        self.assertFalse(os.path.exists(fresh['control']))

    def use_historical(self):
        """A fixture 688 C0 record and tranche summary, pinned by their own sha256 for this test only. The record
        names the fixture's tool bytes and the fixture's roots, so the binding checks run for real."""
        hist = self.tmp + '/historical'
        os.mkdir(hist, 0o700)
        tranche = hist + '/summary.json'
        Path(tranche).write_bytes(b'{"tranche": "fixture"}\n')
        tranche_sha = ex.sha_file(tranche)
        record = dict(status='passed: fixture', token=ex.RECORDED_TOKEN, zero_share_present=True, roots_safe=True,
                      failed_at=None,
                      tools={'colima': {'sha256': ex.sha_file(self.colima)}, 'docker': {'sha256': ex.sha_file(self.docker)}},
                      tranche_summary_sha256=tranche_sha,
                      roots_final={v['path']: dict(device=v['device'], inode=v['inode'], entries=v['entries'])
                                   for v in self.roots_at_start.values()})
        data = (json.dumps(record, sort_keys=True, indent=2) + '\n').encode()
        record_path = hist + '/C0.stage.json'
        Path(record_path).write_bytes(data)
        patches = [mock.patch.dict(ex.HISTORICAL_C0, dict(path=record_path, sha256=sha_bytes(data),
                                                          tranche_path=tranche, tranche_sha256=tranche_sha)),
                   mock.patch.object(ex, 'LINUX_ARTIFACT_DIR', hist)]
        for patcher in patches:
            patcher.start()
            self.addCleanup(patcher.stop)
        self.historical = dict(path=record_path, sha256=sha_bytes(data))
        return self.historical

    def test_c1_binds_the_pinned_historical_c0_by_identity_and_dispatches_its_colima_start(self):
        hist = self.use_historical()
        routes = {'c1-colima-start': emit('started')}
        calls = []
        ref = dict(stage='C0', historical=True, path=hist['path'], sha256=hist['sha256'])
        receipt = self.execute('C1', self.routed(routes, calls), prior_ref=ref, activation_id='act-c1-hist-1')
        self.assertEqual(receipt['prior']['stage'], 'C0')
        receipt_calls = [c[0] for c in calls]
        self.assertEqual(receipt_calls, ['c1-colima-start'])
        self.assertTrue(os.path.isdir(self.bindings['colima_cache_home']))
        self.assertEqual(receipt['closure'], 'closed')

    def test_c1_refuses_a_changed_historical_record_a_changed_tool_or_a_wrong_reference(self):
        hist = self.use_historical()
        ref = dict(stage='C0', historical=True, path=hist['path'], sha256=hist['sha256'])
        calls = []
        with self.assertRaises(ex.Refusal):  # wrong pinned reference
            self.execute('C1', self.routed({}, calls), prior_ref=dict(ref, sha256='0' * 64), activation_id='act-c1-bad-ref')
        Path(hist['path']).write_bytes(Path(hist['path']).read_bytes() + b' ')  # the record's bytes change
        with self.assertRaises(ex.Refusal):
            self.execute('C1', self.routed({}, calls), prior_ref=ref, activation_id='act-c1-bad-bytes')
        self.assertEqual(calls, [])

    def test_c1_refuses_when_the_tool_differs_from_the_recorded_one(self):
        hist = self.use_historical()
        ref = dict(stage='C0', historical=True, path=hist['path'], sha256=hist['sha256'])
        Path(self.colima).write_bytes(b'a different colima\n')
        calls = []
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('C1', self.routed({}, calls), prior_ref=ref, activation_id='act-c1-tool')
        self.assertIn('colima differs', str(caught.exception))
        self.assertEqual(calls, [])

    def test_bounded_helper_is_interrupted_at_its_allowance(self):
        import time as _time
        started = _time.monotonic()
        with self.assertRaises(ex.Refusal) as caught:
            ex.call_bounded(lambda: [None for _ in iter(int, 1)], 0.2)  # spins until the timer fires
        self.assertIn('allowance', str(caught.exception))
        self.assertLess(_time.monotonic() - started, 5)
        self.assertEqual(ex.signal.getitimer(ex.signal.ITIMER_REAL), (0.0, 0.0))
        with self.assertRaises(ex.Refusal):
            ex.call_bounded(lambda: None, 0)

    def test_nested_helper_preserves_outer_deadline(self):
        def outer():
            ex.call_bounded(lambda: time.sleep(0.05), 1)
            time.sleep(1)
        started = time.monotonic()
        with self.assertRaises(ex.Refusal):
            ex.call_bounded(outer, 0.2)
        self.assertLess(time.monotonic() - started, 1)
        self.assertEqual(ex.signal.getitimer(ex.signal.ITIMER_REAL), (0.0, 0.0))

    def test_caught_inner_timeout_still_preserves_outer_deadline(self):
        def outer():
            try:
                ex.call_bounded(lambda: time.sleep(1), 0.05)
            except ex.Refusal:
                pass
            time.sleep(1)
        started = time.monotonic()
        with self.assertRaises(ex.Refusal):
            ex.call_bounded(outer, 0.2)
        self.assertLess(time.monotonic() - started, 1)

    def test_start_identity_is_a_spawn_record_with_no_subprocess(self):
        self.assertFalse(hasattr(ex, 'subprocess'))  # the spawn record runs no subprocess
        with mock.patch.object(os, 'getpgid', return_value=12345), mock.patch.object(os, 'getsid', return_value=12345):
            record = ex.start_identity(12345)
        self.assertIn('start_monotonic', record)
        self.assertIn('not observed', record['start_identity'])

    def test_c1_dispatches_its_owned_dirs_and_zero_share_colima_start_under_a_stub(self):
        routes = {'c1-colima-start': emit('started')}
        receipt, calls = self.run_sweep('C1', routes)
        self.assertEqual([c[0] for c in calls], ['c1-colima-start'])
        steps = {s['id']: s for s in ex.stage_of(ex.load_manifest(), 'C1')['steps']}
        expected = ex.resolve_step(steps['c1-colima-start'], dict(self.bindings))
        self.assertEqual(calls[0][1], expected['argv'])
        self.assertEqual(calls[0][2], expected['env'])
        self.assertIn('--ssh-config=false', calls[0][1])
        self.assertTrue(os.path.isdir(self.bindings['colima_cache_home']))
        self.assertTrue(os.path.isdir(self.bindings['colima_docker_config']))
        self.assertEqual(receipt['closure'], 'closed')
        self.assertTrue(os.path.islink(self.bindings['control'] + '/bin/docker'))

    def test_c2_route_echo_is_the_first_guest_effect_and_no_source_payload_is_dispatched(self):
        parent = self.guest_parent()
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': closed_guest(prep.ROUTE_ECHO),
                  'c2-guest-probe': closed_guest(json.dumps({'parent': parent}))}
        receipt, calls = self.run_sweep('C2', routes)
        self.assertEqual([c[0] for c in calls], ['c2-colima-status', 'c2-route-echo', 'c2-guest-probe'])
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual(receipt['guest_closure'], 'closed')
        self.assertFalse(receipt['closure_simulated'])
        self.assertEqual(calls[1][1][-1], prep.ROUTE_ECHO)  # the echo still carries the route constant last
        echo_argv = calls[1][1]
        self.assertEqual(echo_argv[echo_argv.index('-c') + 1], helper.LAUNCHER)
        self.assertEqual(echo_argv[echo_argv.index('-c') + 2], ex.GUEST_HELPER_PATH.read_bytes().hex())
        self.assertNotIn(ex.GUEST_CONTEXT_PATH.read_text(), echo_argv)  # probe source is not on the echo
        self.assertEqual(calls[2][1][-1], ex.GUEST_CONTEXT_PATH.read_text())
        echoed = Path(self.stage_dir('C2', 'act-sweep-c2') + '/c2-route-echo/stdout')
        self.assertEqual(echoed.read_bytes(), prep.ROUTE_ECHO.encode())
        self.assertTrue(Path(self.stage_dir('C2', 'act-sweep-c2') + '/c2-route-echo/stdout.transport').is_file())
        self.assertEqual(ex.h_compare_route_echo({'bindings': self.bindings, 'outputs': {'c2-route-echo': {'stdout': str(echoed)}}},
                                                 {'inputs': ['c2-route-echo.stdout']}), {'matched': True})
        self.assert_dispatch('C2', calls, self.run_bindings())

    T32 = '0' * 31 + '1'

    def run_bindings(self):
        """The bindings a guest stage resolves against: the reviewed bindings and the pinned sources. Derived values
        (guest_spec, guest_context, guest_iidfile, token32) come from the run, never from a caller."""
        return dict(self.bindings, **ex.guest_source_bindings())

    def tail_transport(self, calls):
        """Replaces only the ssh wrapper: the manifest's remote command, from `python3` on, runs locally with the
        same argv tail, so the pinned guest source is the code that runs."""
        def transport(**kw):
            calls.append((kw['step'], list(kw['argv']), dict(kw['environment'])))
            argv = kw['argv']
            if 'python3' not in argv:
                raise AssertionError(f"the tail transport runs guest python steps only: {kw['step']}")
            tail = argv[argv.index('python3'):]
            return primitive.run_stage(kw['root'], [sys.executable] + tail[1:], kw['timeout'], kw['output_cap'],
                                       kw['input_hashes'], guest=kw['guest'], cwd=kw['cwd'], environment=CHILD_ENV,
                                       resource_monitor=kw['monitor'], deadline=kw['deadline'],
                                       stdin_path=kw['stdin_path'], stdin_sha256=kw['stdin_sha256'],
                                       identity_probe=kw['identity_probe'])
        return transport

    def c2_stub(self):
        """Stub C2 predecessor: its guest probe output (the parent the probe observed), in a stub stage root. The
        C3 render reads that output through the same prior-output route a verified predecessor uses."""
        parent = self.guest_parent()
        root = self.tmp + '/c2-stub'
        os.mkdir(root, 0o700)
        os.mkdir(root + '/c2-guest-probe', 0o700)
        Path(root + '/c2-guest-probe/stdout').write_text(json.dumps({'parent': parent}))
        stub = {'receipts': [], 'engine_id': None, 'stage_root': root,
                'steps': [{'id': 'c2-guest-probe', 'kind': 'exec'}]}
        return parent, stub

    def c2_inner(self, parent):
        return json.dumps({'parent': parent})

    def c3_inner(self, parent):
        return json.dumps({'parent': parent, 'path': parent['path'] + '/effigy-531-image-' + self.T32,
                           'sha256': ex.RECIPE_SHA256})

    def genuine_c2(self, parent, activation_id='act-c2-closed'):
        """C2 through the production guest-helper path: injected envelopes, no closure simulation."""
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': closed_guest(prep.ROUTE_ECHO),
                  'c2-guest-probe': closed_guest(self.c2_inner(parent))}
        stub = {'receipts': [], 'engine_id': None}
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute('C2', self.routed(routes, []), prior_ref=None, activation_id=activation_id)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertFalse(receipt['closure_simulated'])
        return receipt

    def run_c3(self, activation_id='act-c3-guest-1'):
        """C3 through the stage loop. Stubbed: the predecessor (labelled) and the token32 origin, which the C0 origin
        walk tests separately on a genuine C0 receipt. The stage helper closes through an injected envelope."""
        parent = self.guest_parent()
        c2 = self.genuine_c2(parent, activation_id + '-c2')
        spec = ex.build_guest_spec(parent, self.T32)
        calls = []
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: c2), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            receipt = self.execute('C3', self.routed({'c3-stage-context': closed_guest(self.c3_inner(parent))}, calls),
                                   prior_ref=None, activation_id=activation_id)
        return receipt, calls, spec

    def test_c2_comparison_runs_through_the_adapter_and_the_probe_is_recorded_not_dispatched(self):
        """A local client exit with no helper envelope cannot close the echo, so the probe is not dispatched."""
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': emit(prep.ROUTE_ECHO)}
        receipt, calls = self.run_sweep('C2', routes)
        self.assertEqual(receipt['closure'], 'guest_unknown')
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c2-compare-route-echo']['kind'], 'helper')
        self.assertEqual(by_id['c2-compare-route-echo']['closure'], 'closed')  # a pure read, after the unknown echo
        self.assertEqual(by_id['c2-guest-probe']['closure'], 'not_dispatched')  # the effect never ran
        self.assertNotIn('c2-guest-probe', [c[0] for c in calls])
        result = json.loads(Path(self.stage_dir('C2', 'act-sweep-c2') + '/c2-compare-route-echo/helper.json').read_text())
        self.assertEqual(result['result'], {'matched': True})

    def test_c2_refuses_a_route_echo_that_does_not_match_before_any_payload(self):
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': emit('wrong echo')}
        with self.assertRaises(ex.Refusal) as caught:
            self.run_sweep('C2', routes)
        self.assertIn('route echo mismatch', str(caught.exception))
        # the echo is the only guest effect; the probe never reaches dispatch
        receipts = json.loads(Path(self.stage_dir('C2', 'act-sweep-c2') + '/stage-receipt.json').read_text())
        self.assertEqual(receipts['closure'], 'refused')
        self.assertNotIn('c2-guest-probe', [x['id'] for x in receipts['steps'] if x.get('closure') == 'closed'])

    def test_c3_runs_the_pinned_stage_source_with_the_reviewed_recipe_as_its_stdin(self):
        receipt, calls, spec = self.run_c3()
        self.assertEqual(receipt['closure'], 'closed')
        self.assertFalse(receipt['closure_simulated'])
        self.assertEqual([c[0] for c in calls], ['c3-stage-context'])
        argv = calls[0][1]
        self.assertEqual(argv[argv.index('-c') + 1], helper.LAUNCHER)
        self.assertEqual(argv[argv.index('-c') + 2], ex.GUEST_HELPER_PATH.read_bytes().hex())
        self.assertEqual(argv[argv.index('-c') + 3], 'stage')
        self.assertEqual(argv[-2], ex.GUEST_CONTEXT_PATH.read_text())
        self.assertEqual(argv[-1], spec)
        context = json.loads(Path(self.stage_dir('C3', 'act-c3-guest-1') + '/c3-stage-context/stdout').read_text())
        self.assertEqual(context['parent']['path'], json.loads(spec)['parent']['path'])
        recipe = Path(self.stage_dir('C3', 'act-c3-guest-1') + '/c3-render-guest-spec/recipe.Dockerfile')
        self.assertEqual(recipe.read_bytes(), ex.RECIPE_PATH.read_bytes())
        self.assert_dispatch('C3', calls, dict(self.run_bindings(), guest_spec=spec, token32=self.T32))

    def test_c2_unknown_route_echo_timeout_stops_before_the_probe_is_reached(self):
        """The echo's allowance runs out: its closure is unknown, so the stage stops and the probe is never reached."""
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': 'import time; time.sleep(30)'}
        calls = []
        stub = {'receipts': [], 'engine_id': None}
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute('C2', self.cap('c2-route-echo', 1.0)(self.routed(routes, calls)), prior_ref=None,
                                   activation_id='act-c2-echo-timeout')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual([c[0] for c in calls], ['c2-colima-status', 'c2-route-echo'])
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c2-route-echo']['closure'], 'unknown_retained')
        self.assertNotIn('c2-guest-probe', by_id)

    # C4 through the production path. The reviewed host handlers run in-process; the guest transfer and the
    # pre-build re-derivation run the pinned guest source against this fixture's own owned parent directory; and
    # only the transport is substituted (one authenticated envelope per guest step, naming the command that ran).
    # No engine, VM, SSH, network or native effect runs.

    def c4_fixture(self, activation_id):
        """A genuine closed C3 predecessor for C4, and the context it staged."""
        c3, _, _ = self.run_c3(activation_id + '-c3')
        root = self.stage_dir('C3', activation_id + '-c3')
        stub = {'receipts': [], 'engine_id': None, 'stage_root': root, 'steps': c3['steps']}
        prior = dict(stage='C3', path=root + '/stage-receipt.json', sha256=ex.sha_file(root + '/stage-receipt.json'))
        staged = json.loads(Path(root + '/c3-stage-context/stdout').read_text())
        return stub, prior, staged

    @staticmethod
    def c4_entries(staged):
        spec = guest_inputs.transfer_spec(FAKE_RUNTIME_PIN, staged['parent'], Path(staged['path']).name)
        return guest_inputs.layout_entries(spec['layout'], FAKE_RUNTIME_BYTES['index'])[0]

    def c4_transfer(self, tamper=None):
        """The pinned guest transfer handler over the dispatched stream and this fixture's real guest parent.
        A guest refusal is a non-zero guest exit with its reason on stderr, exactly as in the guest."""
        def produce(kw):
            spec = guest_inputs.canonical_spec(kw['argv'][-1])
            with open(kw['stdin_path'], 'rb') as stream:
                receipt = guest_inputs.transfer(spec, stream)
            return json.dumps(tamper(receipt) if tamper is not None else receipt, sort_keys=True)
        return self.c4_guest_route(produce)

    def c4_check(self, tamper=None):
        """The pinned guest pre-build re-derivation over the directories the transfer wrote."""
        def produce(kw):
            spec = guest_inputs.canonical_spec(kw['argv'][-1])
            receipt = guest_inputs.check_inputs(spec)
            return json.dumps(tamper(receipt) if tamper is not None else receipt, sort_keys=True)
        return self.c4_guest_route(produce)

    @staticmethod
    def c4_guest_route(produce):
        def route(kw):
            try:
                receipt = produce(kw)
            except (guest_inputs.Refusal, OSError, ValueError) as exc:
                return emit('', 2, 'refused: ' + str(exc))
            return closed_guest(receipt)(kw)
        return route

    @staticmethod
    def c4_command(kw):
        """The reviewed remote argv that crossed in the step's hex spec."""
        return json.loads(bytes.fromhex(kw['argv'][-1]).decode())['argv']

    def c4_remote(self, payload, **fields):
        """One authenticated envelope whose header names the command the helper ran."""
        def route(kw):
            command = self.c4_command(kw)
            text = payload(kw, command) if callable(payload) else payload
            return closed_guest(text, command=command, **fields)(kw)
        return route

    def c4_verify(self, staged):
        return closed_guest(json.dumps({k: v for k, v in staged.items() if k != 'parent'}))

    def c4_routes(self, staged, *, transfer=None, check=None, help_text=None, build=None):
        def write_iid(kw, command):
            Path(command[command.index('--iidfile') + 1]).write_text(IMAGE + '\n')
            return ''
        return {
            'c4-transfer-inputs': transfer or self.c4_transfer(),
            'c4-verify-context': self.c4_verify(staged),
            'c4-verify-inputs': check or self.c4_check(),
            'c4-nerdctl-build-help': self.c4_remote(
                help_text if help_text is not None else
                'usage: nerdctl build --iidfile --platform --provenance --network --build-context'),
            'c4-nerdctl-build': self.c4_remote(build or write_iid),
            'c4-iidfile-read': self.c4_remote(lambda kw, command: Path(command[3]).read_text()),
            'c4-image-inspect': self.c4_remote(
                json.dumps([{'Id': IMAGE, 'Os': 'linux', 'Architecture': 'arm64'}])),
        }

    def run_c4(self, activation_id='act-c4-closed', routes_for=None, transport_for=None):
        stub, prior, staged = self.c4_fixture(activation_id)
        routes = (routes_for or (lambda value: self.c4_routes(value)))(staged)
        calls = []
        transport = self.routed(routes, calls) if transport_for is None else transport_for(routes, calls)
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            receipt = self.execute('C4', transport, prior_ref=prior, activation_id=activation_id, expires=19)
        return receipt, calls, staged

    def test_c4_stage_closes_with_the_production_transfer_and_remote_commands(self):
        receipt, calls, staged = self.run_c4()
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual(receipt['guest_closure'], 'closed')
        self.assertFalse(receipt['closure_simulated'])
        manifest_steps = [s['id'] for s in ex.stage_of(ex.load_manifest(), 'C4')['steps']]
        self.assertEqual([x['id'] for x in receipt['steps']], manifest_steps)
        self.assertEqual([c[0] for c in calls], ['c4-transfer-inputs', 'c4-verify-context', 'c4-verify-inputs',
                                                 'c4-nerdctl-build-help', 'c4-nerdctl-build', 'c4-iidfile-read',
                                                 'c4-image-inspect'])
        by_id = {x['id']: x for x in receipt['steps']}
        for step in ('c4-transfer-inputs', 'c4-verify-inputs', 'c4-nerdctl-build'):
            self.assertEqual(by_id[step]['closure'], 'closed', step)
            self.assertEqual(by_id[step]['guest_closure'], 'closed', step)
            self.assertFalse(by_id[step]['closure_simulated'], step)
        entries = self.c4_entries(staged)
        root = self.stage_dir('C4', 'act-c4-closed')
        transfer = json.loads(Path(root + '/c4-transfer-inputs/stdout').read_text())
        verify = json.loads(Path(root + '/c4-verify-inputs/stdout').read_text())
        self.assertEqual(transfer['entries'], entries)
        self.assertEqual(verify['entries'], entries)
        self.assertEqual(transfer['derived']['total_bytes'], sum(entry['bytes'] for entry in entries))
        self.assertEqual(transfer['dirs'], verify['dirs'])
        # The build ran the exact reviewed nerdctl argv the guest reported, with local contexts only.
        build = by_id['c4-nerdctl-build']['guest_command']
        self.assertEqual(build, prep.guest_build_command(staged['path']))
        self.assertEqual(build[build.index('--network') + 1], 'none')
        self.assertFalse([a for a in build if a.startswith('--pull')])
        self.assertEqual([a for a in build if a.startswith('ubuntu=') or a.startswith('debs=')],
                         ['ubuntu=oci-layout://' + staged['path'] + '.oci',
                          'debs=' + staged['path'] + '.debs'])
        self.assertEqual(by_id['c4-nerdctl-build-help']['guest_command'], ['nerdctl', 'build', '--help'])
        self.assertEqual(by_id['c4-iidfile-read']['guest_command'],
                         ['sudo', '-n', 'cat', staged['path'] + '.iid'])
        self.assertEqual(by_id['c4-image-inspect']['guest_command'],
                         ['sudo', '-n', 'nerdctl', 'image', 'inspect', IMAGE])
        # The transfer dispatched the framed stream the render step recorded, and nothing else.
        render = json.loads(Path(root + '/c4-render-input-transfer/helper.json').read_text())['result']
        self.assertEqual(by_id['c4-transfer-inputs']['stdin_sha256'], render['sha256'])
        self.assertEqual(by_id['c4-transfer-inputs']['transfer_stream']['sha256'], render['sha256'])
        self.assertEqual(Path(root + '/c4-render-input-transfer/transfer.bin').stat().st_size, render['bytes'])
        self.assert_dispatch('C4', calls, dict(self.run_bindings(), token32=self.T32,
                                                guest_verify_spec=self.stage_file(
                                                    'C4', 'act-c4-closed',
                                                    'c4-render-verify-spec/c4-render-verify-spec.json').read_text(),
                                                guest_context=staged['path'],
                                                guest_iidfile=staged['path'] + '.iid',
                                                guest_input_spec=Path(
                                                    root + '/c4-render-input-specs/guest-input-spec.hex').read_text(),
                                                guest_oci=staged['path'] + '.oci',
                                                guest_debs=staged['path'] + '.debs', image_id=IMAGE))
        # The receipt re-derives its own transfer from the retained spec, host inputs, stream and guest receipts.
        body = json.loads(Path(root + '/stage-receipt.json').read_text())
        ex.verify_c4_inputs(root, body, ex.load_manifest())

    def test_c4_refuses_before_the_claim_when_the_reviewed_pin_changes(self):
        stub, prior, staged = self.c4_fixture('act-c4-pin')
        changed = dict(FAKE_RUNTIME_PIN, debs=tuple(dict(deb, bytes=deb['bytes'] + 1) for deb in FAKE_RUNTIME_PIN['debs']))
        calls = []
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub), \
                mock.patch.object(runtime_inputs, 'runtime_image_pin', lambda manifest: dict(changed)), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            with self.assertRaisesRegex(ex.Refusal, 'prefetch|pin|size'):
                self.execute('C4', self.routed(self.c4_routes(staged), calls), prior_ref=prior,
                             activation_id='act-c4-pin')
        self.assertEqual([c[0] for c in calls], [])

    def test_c4_stops_before_the_build_when_a_host_input_is_replaced(self):
        def replace():
            victim = sorted(Path(self.bindings['colima_input_dir'] + '/debs').iterdir())[0]
            os.chmod(victim, 0o600)
            victim.write_bytes(b'x' * victim.stat().st_size)
        self.assert_replaced_input_refused(replace, 'source differs from its pinned sha256', 'act-c4-host-replaced')

    def test_c4_stops_before_the_build_when_a_host_input_disappears(self):
        def remove():
            sorted(Path(self.bindings['colima_input_dir'] + '/debs').iterdir())[0].unlink()
        self.assert_replaced_input_refused(remove, 'missing or extra inputs', 'act-c4-host-gone')

    def assert_replaced_input_refused(self, tamper, reason, activation_id):
        stub, prior, staged = self.c4_fixture(activation_id)
        calls = []
        original = ex.HANDLERS['render_input_transfer']

        def tampering(ctx, step):
            tamper()
            return original(ctx, step)
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub), \
                mock.patch.dict(ex.HANDLERS, {'render_input_transfer': tampering}), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            with self.assertRaisesRegex(ex.Refusal, reason):
                self.execute('C4', self.routed(self.c4_routes(staged), calls), prior_ref=prior,
                             activation_id=activation_id)
        self.assertNotIn('c4-transfer-inputs', [call[0] for call in calls])

    def test_c4_refuses_a_transfer_receipt_that_names_other_inputs(self):
        def tamper(receipt):
            receipt = json.loads(json.dumps(receipt))
            receipt['entries'][0]['sha256'] = 'ab' * 32
            return receipt
        routes_for = lambda staged: self.c4_routes(staged, transfer=self.c4_transfer(tamper))
        with self.assertRaisesRegex(ex.Refusal, 'entries differ'):
            self.run_c4('act-c4-badentry', routes_for)
        self.assertFalse(os.path.exists(self.stage_dir('C4', 'act-c4-badentry') + '/c4-nerdctl-build'))

    def test_c4_stops_before_the_build_when_a_guest_input_directory_is_replaced(self):
        stub, prior, staged = self.c4_fixture('act-c4-replaced')
        calls = []

        def before(kw):
            if kw['step'] == 'c4-verify-inputs':
                debs = Path(staged['path'] + '.debs')
                debs.rename(str(debs) + '.moved')
                os.mkdir(debs, 0o700)
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            receipt = self.execute('C4', self.routed(self.c4_routes(staged), calls, before=before),
                                   prior_ref=prior, activation_id='act-c4-replaced')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c4-verify-inputs']['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls], ['c4-transfer-inputs', 'c4-verify-context', 'c4-verify-inputs'])
        self.assertIn('missing or extra entries',
                      Path(self.stage_dir('C4', 'act-c4-replaced') + '/c4-verify-inputs/stderr').read_text())
        for step in ('c4-check-input-verify', 'c4-nerdctl-build-help', 'c4-nerdctl-build', 'c4-iidfile-read',
                     'c4-image-inspect'):
            self.assertEqual(by_id[step]['closure'], 'not_dispatched', step)

    def test_c4_stops_without_a_later_effect_when_the_transfer_closure_is_unknown(self):
        stub, prior, staged = self.c4_fixture('act-c4-transfer-timeout')
        calls = []
        routes = self.c4_routes(staged)
        routes['c4-transfer-inputs'] = 'import time; time.sleep(30)'
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            receipt = self.execute('C4', self.cap('c4-transfer-inputs', 1.0)(self.routed(routes, calls)),
                                   prior_ref=prior, activation_id='act-c4-transfer-timeout')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(receipt['guest_closure'], 'unknown')
        self.assertEqual([c[0] for c in calls], ['c4-transfer-inputs'])
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c4-transfer-inputs']['closure'], 'unknown_retained')
        for step in ('c4-verify-context', 'c4-verify-inputs', 'c4-nerdctl-build', 'c4-iidfile-read',
                     'c4-image-inspect'):
            self.assertNotIn(step, by_id)

    def test_c4_stops_before_the_build_when_the_guest_helper_did_not_close(self):
        stub, prior, staged = self.c4_fixture('act-c4-nonzero')
        calls = []
        routes = self.c4_routes(staged)
        routes['c4-transfer-inputs'] = emit('', 2, 'transfer refused')
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            receipt = self.execute('C4', self.routed(routes, calls), prior_ref=prior,
                                   activation_id='act-c4-nonzero')
        self.assertEqual(receipt['closure'], 'guest_unknown')  # a local client exit proves nothing about the guest
        self.assertEqual([c[0] for c in calls], ['c4-transfer-inputs'])
        self.assertEqual(Path(self.stage_dir('C4', 'act-c4-nonzero') + '/c4-transfer-inputs/stderr').read_text(),
                         'transfer refused')

    def test_c4_stops_before_the_build_when_a_build_flag_is_unsupported(self):
        routes_for = lambda staged: self.c4_routes(staged, help_text='usage: nerdctl build --iidfile')
        with self.assertRaisesRegex(ex.Refusal, 'nerdctl build help lacks'):
            self.run_c4('act-c4-noflag', routes_for)
        self.assertFalse(os.path.exists(self.stage_dir('C4', 'act-c4-noflag') + '/c4-nerdctl-build'))

    def test_c4_stops_after_a_nonzero_build(self):
        self.assert_build_unknown('nonzero', self.c4_remote('', exit_code=1))

    def test_c4_stops_after_a_lost_or_unreaped_build(self):
        self.assert_build_unknown('unreaped', self.c4_remote('', child_pgid=99, group_empty=False))

    def assert_build_unknown(self, name, build_route):
        stub, prior, staged = self.c4_fixture('act-c4-build-' + name)
        calls = []

        def write_iid(kw, command):
            Path(command[command.index('--iidfile') + 1]).write_text(IMAGE + '\n')
            return ''
        routes = self.c4_routes(staged, build=write_iid)
        routes['c4-nerdctl-build'] = build_route
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            receipt = self.execute('C4', self.routed(routes, calls), prior_ref=prior,
                                   activation_id='act-c4-build-' + name)
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls], ['c4-transfer-inputs', 'c4-verify-context', 'c4-verify-inputs',
                                                 'c4-nerdctl-build-help', 'c4-nerdctl-build'])
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c4-nerdctl-build']['closure'], 'guest_unknown')
        self.assertEqual(by_id['c4-iidfile-read']['closure'], 'not_dispatched')
        self.assertEqual(by_id['c4-image-inspect']['closure'], 'not_dispatched')
        self.assertNotIn('c4-read-iidfile', [x['id'] for x in receipt['steps'] if x.get('closure') == 'closed'])

    def test_every_c4_remote_dispatch_rebuilds_the_reviewed_command_in_the_fixed_helper(self):
        """The production dispatch and the fixed helper agree for every C4 remote command.

        The manifest states each reviewed argv; the executor re-renders it as the fixed helper launch and appends
        the same command as bounded hex; the helper decodes that argv and rebuilds the command it will run. This
        control runs the real dispatch output back through the helper's own argv builder, so a dispatch shape the
        helper refuses (or a command it would have altered) fails here rather than only in a stubbed envelope."""
        context = '/root/effigy-531-image-' + self.T32
        manifest = ex.load_manifest()
        c4 = ex.stage_of(manifest, 'C4')
        bindings = dict(self.run_bindings(), guest_context=context, guest_iidfile=context + '.iid', image_id=IMAGE)
        steps = {step['id']: step for stage in ('C4', 'C5') for step in ex.stage_of(manifest, stage)['steps']}
        bindings['guest_check_cid'] = context + '.check.cid'
        bindings['check_container_id'] = 'a' * 64
        self.assertEqual(sorted(ex.REMOTE_COMMAND_STEPS),
                         sorted(['c4-image-inspect', 'c4-iidfile-read', 'c4-nerdctl-build',
                                 'c4-nerdctl-build-help', 'c5-container-inspect', 'c5-container-list',
                                 'c5-container-rm', 'c5-disposable-check', 'c5-guest-nerdctl-version',
                                 'c5-read-check-cid']))
        for step_id, operation in sorted(ex.REMOTE_COMMAND_STEPS.items()):
            with self.subTest(step=step_id):
                resolved = ex.resolve_step(steps[step_id], bindings)
                _, reviewed = ex.guest_remote_tail(resolved, bindings)
                dispatched = ex.dispatch_guest_command(resolved, bindings, 'ab' * 32)
                # The helper is launched as `python3 -I -B -c LAUNCHER <hex> <operation> <nonce> <spec>`; the launcher
                # execs the decoded helper, so the helper's own argv is what this reconstructs.
                c_idx = dispatched.index('-c')
                self.assertEqual(dispatched[c_idx + 1], helper.LAUNCHER)
                launched = [dispatched[c_idx], dispatched[c_idx + 2]] + dispatched[c_idx + 3:]
                args = helper.operation_argv(launched)
                self.assertEqual(args[0], operation)
                self.assertEqual(args[1], 'ab' * 32)
                rebuilt = helper.inner_argv(operation, args[2:])
                self.assertEqual(rebuilt, reviewed, step_id)
                # And the guest-side spec really is the reviewed command, canonically encoded.
                self.assertEqual(json.loads(bytes.fromhex(dispatched[-1]).decode()), {'argv': reviewed})
                # The review guard: the helper refuses a command that is not the reviewed one for this step.
                tampered = reviewed + ['--pull']
                spec = json.dumps({'argv': tampered}, sort_keys=True, separators=(',', ':')).encode().hex()
                with self.assertRaises(helper.Refusal):
                    helper.inner_argv(operation, [spec])

    def test_c4_refuses_a_remote_command_that_is_not_the_reviewed_form(self):
        context = '/root/effigy-531-image-' + self.T32
        bindings = dict(self.bindings, image_id='effigy-4f6a-runtime:r1', guest_context=context)
        resolved = {'id': 'c4-image-inspect', 'argv': [prep.COLIMA, 'ssh', '-p', prep.PROFILE, '--',
                                                       'sudo', '-n', 'nerdctl', 'image', 'inspect',
                                                       'effigy-4f6a-runtime:r1'], 'env': {}}
        with self.assertRaisesRegex(ValueError, 'reviewed C4 or C5 forms'):
            ex.guest_remote_tail(resolved, bindings)
        # A command that names another context refuses too: the four reviewed shapes are the only ones allowed.
        resolved = {'id': 'c4-nerdctl-build', 'argv': [prep.COLIMA, 'ssh', '-p', prep.PROFILE, '--',
                                                       'sudo', '-n', 'env', 'DOCKER_CONFIG=/other/docker-config',
                                                       'nerdctl', 'build'], 'env': {}}
        with self.assertRaisesRegex(ValueError, 'reviewed C4 or C5 forms'):
            ex.guest_remote_tail(resolved, bindings)

    def test_c4_refuses_a_build_spec_that_is_not_the_pinned_platform(self):
        context = '/root/effigy-531-image-' + self.T32
        good = prep.guest_build_command(context)
        self.assertTrue(prep.check_guest_remote_command(good, dict(self.bindings, guest_context=context)))
        for changed in ([a if a != 'linux/arm64' else 'linux/amd64' for a in good],
                        [a if a != '--provenance=mode=max' else '--provenance=false' for a in good],
                        good + ['--pull'],
                        [a for a in good if a not in ('--network', 'none')]):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                prep.check_guest_remote_command(changed, dict(self.bindings, guest_context=context))

    def test_c4_chain_verifier_refuses_a_tampered_stream_or_guest_receipt(self):
        receipt, _, _ = self.run_c4('act-c4-verify')
        root = self.stage_dir('C4', 'act-c4-verify')
        body = json.loads(Path(root + '/stage-receipt.json').read_text())
        ex.verify_c4_inputs(root, body, ex.load_manifest())
        stream = Path(root + '/c4-render-input-transfer/transfer.bin')
        original = stream.read_bytes()
        stream.write_bytes(original[:-1] + b'x')
        with self.assertRaisesRegex(ex.Refusal, 'stream differs'):
            ex.verify_c4_inputs(root, body, ex.load_manifest())
        stream.write_bytes(original)
        stdout = Path(root + '/c4-transfer-inputs/stdout')
        saved = stdout.read_bytes()
        altered = json.loads(saved)
        altered['derived']['total_bytes'] += 1
        stdout.write_bytes(json.dumps(altered).encode())
        with self.assertRaisesRegex(ex.Refusal, 'derived facts'):
            ex.verify_c4_inputs(root, body, ex.load_manifest())
        stdout.write_bytes(saved)
        ex.verify_c4_inputs(root, body, ex.load_manifest())

    def test_a_helper_blocked_in_a_native_read_is_interrupted_at_its_allowance(self):
        import time as _time
        fifo = self.tmp + '/no-writer.fifo'
        os.mkfifo(fifo)
        started = _time.monotonic()
        with self.assertRaises(ex.Refusal) as caught:
            ex.call_bounded(lambda: open(fifo, 'rb').read(), 0.3)  # blocks in the kernel until a writer appears
        self.assertIn('allowance', str(caught.exception))
        self.assertLess(_time.monotonic() - started, 5)

    def test_c5_is_refused_before_claim_by_its_static_reason(self):
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}):  # the production admission, not the fixture's
            self._static_refusals()

    def test_c3_refuses_a_stage_parent_that_differs_from_the_probe(self):
        parent = self.guest_parent()
        c2 = self.genuine_c2(parent, 'act-c3-drift-c2')
        other = dict(parent, inode=parent['inode'] + 1)
        calls = []
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: c2), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            with self.assertRaisesRegex(ex.Refusal, 'parent differs'):
                self.execute('C3', self.routed({'c3-stage-context': closed_guest(self.c3_inner(other))}, calls),
                             prior_ref=None, activation_id='act-c3-parent-drift')
        self.assertEqual([c[0] for c in calls], ['c3-stage-context'])

    def _static_refusals(self):
        for stage in ('C5',):
            with self.subTest(stage=stage), self.assertRaises(ex.Refusal) as caught:
                calls = []
                self.execute(stage, self.routed({}, calls), prior_ref=None, activation_id=f'act-unpin-{stage.lower()}')
            self.assertIn('C4 receipt', str(caught.exception))
            self.assertEqual(calls, [])
            self.assertFalse(os.path.exists(self.tmp + f'/claims/act-unpin-{stage.lower()}.claim'))

    # Closure simulation (test-only). The stage loop runs past each guest step: a guest exit 0 is treated as closed,
    # and every receipt says so. Predecessor links are stubs: each stage's verify_chain returns the genuine receipt
    # of the stage before it, which the stage then reads as its prior outputs. The C0 origin is patched in C3 and C4.

    def simulated(self):
        return mock.patch.dict(ex.TEST_SEAM, {'simulate_guest_closure': True})

    def stage_file(self, stage, activation_id, relative):
        return Path(self.stage_dir(stage, activation_id) + '/' + relative)

    def test_c2_closure_simulation_dispatches_the_probe_with_exact_argv_env_and_ordering(self):
        parent = self.guest_parent()
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': emit(prep.ROUTE_ECHO),
                  'c2-guest-probe': emit(json.dumps({'parent': parent}))}
        calls = []
        stub = {'receipts': [], 'engine_id': None}
        with self.simulated(), mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute('C2', self.routed(routes, calls), prior_ref=None, activation_id='act-sim-c2')
        self.assertEqual(receipt['closure'], 'closed')
        self.assertTrue(receipt['closure_simulated'])
        self.assertEqual([c[0] for c in calls], ['c2-colima-status', 'c2-route-echo', 'c2-guest-probe'])
        self.assert_dispatch('C2', calls, self.run_bindings())
        probe_argv = calls[2][1]
        self.assertEqual(probe_argv[probe_argv.index('-c') + 1], helper.LAUNCHER)
        self.assertEqual(probe_argv[probe_argv.index('-c') + 2], ex.GUEST_HELPER_PATH.read_bytes().hex())
        self.assertEqual(probe_argv[probe_argv.index('-c') + 3], 'probe')
        self.assertEqual(probe_argv[-1], ex.GUEST_CONTEXT_PATH.read_text())
        self.assertEqual(calls[1][1][-1], prep.ROUTE_ECHO)  # the echo carries only the route constant
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual([x['id'] for x in receipt['steps']],
                         ['c2-colima-status', 'c2-route-echo', 'c2-compare-route-echo', 'c2-guest-probe'])
        self.assertEqual(by_id['c2-guest-probe']['closure'], 'closed')
        self.assertTrue(by_id['c2-guest-probe']['closure_simulated'])
        self.assertEqual(self.stage_file('C2', 'act-sim-c2', 'c2-guest-probe/stdout').read_text(), json.dumps({'parent': parent}))
        self.assertEqual(self.stage_file('C2', 'act-sim-c2', 'c2-route-echo/stdout').read_bytes(), prep.ROUTE_ECHO.encode())

    def test_c3_closure_simulation_stages_the_recipe_from_exact_bounded_stdin(self):
        parent = self.guest_parent()
        c2 = self.sim_c2_body(parent)
        spec = ex.build_guest_spec(parent, self.T32)
        calls, stdin_seen = [], []
        before = lambda kw: stdin_seen.append((kw['step'], kw['stdin_path'], kw['stdin_sha256']))
        with self.simulated(), mock.patch.object(ex, 'verify_chain', lambda *a, **k: c2), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            receipt = self.execute('C3', self.routed({'c3-stage-context': closed_guest(self.c3_inner(parent))},
                                                     calls, before=before),
                                   prior_ref=None, activation_id='act-sim-c3')
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['c3-stage-context'])
        self.assert_dispatch('C3', calls, dict(self.run_bindings(), guest_spec=spec, token32=self.T32))
        self.assertEqual(calls[0][1][-1], spec)  # the guest spec is the argument the probe's parent and token32 derive
        [(step, path, sha)] = stdin_seen
        self.assertEqual((step, sha), ('c3-stage-context', ex.RECIPE_SHA256))
        self.assertEqual(Path(path).read_bytes(), ex.RECIPE_PATH.read_bytes())  # the exact bytes, hash bound
        context = json.loads(self.stage_file('C3', 'act-sim-c3', 'c3-stage-context/stdout').read_text())
        self.assertEqual(context['parent']['path'], parent['path'])
        self.assertEqual(context['path'], parent['path'] + '/effigy-531-image-' + self.T32)

    def sim_c2_body(self, parent):
        """A C2 receipt from a closure-simulated run, for use as a C3 predecessor."""
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': emit(prep.ROUTE_ECHO),
                  'c2-guest-probe': emit(json.dumps({'parent': parent}))}
        with self.simulated(), mock.patch.object(ex, 'verify_chain', lambda *a, **k: {'receipts': [], 'engine_id': None}):
            receipt = self.execute('C2', self.routed(routes, []), prior_ref=None, activation_id='act-sim-c2-pred')
        self.assertEqual(receipt['closure'], 'closed')
        return receipt

    def test_c4_closure_simulation_dispatches_every_step_in_order(self):
        stub, prior, staged = self.c4_fixture('act-sim-c4')
        calls = []
        with self.simulated(), mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            receipt = self.execute('C4', self.routed(self.c4_routes(staged), calls), prior_ref=prior,
                                   activation_id='act-sim-c4')
        self.assertEqual(receipt['closure'], 'closed')
        self.assertTrue(receipt['closure_simulated'])
        manifest_steps = [s['id'] for s in ex.stage_of(ex.load_manifest(), 'C4')['steps']]
        self.assertEqual([x['id'] for x in receipt['steps']], manifest_steps)
        self.assertEqual([c[0] for c in calls], ['c4-transfer-inputs', 'c4-verify-context', 'c4-verify-inputs',
                                                 'c4-nerdctl-build-help', 'c4-nerdctl-build', 'c4-iidfile-read',
                                                 'c4-image-inspect'])
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertTrue(by_id['c4-nerdctl-build']['closure_simulated'])
        # The build's dispatched argv is the fixed helper launch carrying the reviewed command as hex, never the
        # bare nerdctl argv: the whole remote invocation is contained.
        dispatched = next(c[1] for c in calls if c[0] == 'c4-nerdctl-build')
        self.assertNotIn('nerdctl', dispatched)
        self.assertEqual(json.loads(bytes.fromhex(dispatched[-1]).decode())['argv'],
                         prep.guest_build_command(staged['path']))
        self.assertEqual(json.loads(self.stage_file('C4', 'act-sim-c4',
                                                    'c4-render-verify-spec/c4-render-verify-spec.json').read_text())['layout'],
                         guest_inputs.pin_layout(FAKE_RUNTIME_PIN))
        # A simulated receipt is never a chain receipt.
        root = self.stage_dir('C4', 'act-sim-c4')
        with self.assertRaisesRegex(ex.Refusal, 'simulated'):
            ex.verify_receipt_tree(json.loads(Path(root + '/stage-receipt.json').read_text()), 'C4',
                                   ex.chain_key(self.bindings), ex.load_manifest())

    def test_static_admission_and_owner_refusal_still_refuse_with_the_simulation_on(self):
        calls = []
        with self.simulated():
            with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}):
                for stage in ('C5',):
                    with self.subTest(stage=stage), self.assertRaises(ex.Refusal) as caught:
                        self.execute(stage, self.routed({}, calls), prior_ref=None, activation_id=f'act-simadm-{stage}')
                    self.assertIn('C4 receipt', str(caught.exception))
            with mock.patch.dict(ex.TEST_SEAM, {'owner_activation': False}), self.assertRaises(ex.Refusal) as caught:
                self.execute('C2', self.routed({}, calls), prior_ref=None, activation_id='act-simowner-c2')
            self.assertIn('no owner-controlled activation mechanism', str(caught.exception))
        self.assertEqual(calls, [])
        self.assertFalse(os.path.exists(self.tmp + '/claims/act-simadm-C2.claim'))

    def test_a_simulated_receipt_is_never_accepted_as_a_chain_predecessor(self):
        parent = self.guest_parent()
        self.sim_c2_body(parent)
        path = self.stage_dir('C2', 'act-sim-c2-pred') + '/stage-receipt.json'
        with self.assertRaises(ex.Refusal) as caught:
            ex.verify_receipt_tree(json.loads(Path(path).read_text()), 'C2', None, ex.load_manifest())
        self.assertIn('simulated', str(caught.exception))


    # C5 on the genuine C4 receipt (C4 runs for real through run_c4; C5 reads its receipt from disk). The guest runs
    # through the shim above: the reviewed argv is validated and rebuilt by the production helper and then runs for real
    # against fake sudo and nerdctl, whose lifecycle lives in a fixture state file.

    def c5_env(self, **scenario):
        bin_dir = self.tmp + '/fake-bin'
        if not os.path.isdir(bin_dir):
            os.mkdir(bin_dir, 0o700)
            for name, text in (('sudo', FAKE_SUDO), ('nerdctl', FAKE_NERDCTL)):
                path = bin_dir + '/' + name
                Path(path).write_text(text.replace('{python}', sys.executable))
                os.chmod(path, 0o755)
        env = {'PATH': bin_dir + ':/usr/bin:/bin:/usr/sbin:/sbin', 'FAKE_NERDCTL_STATE': self.tmp + '/nerdctl-state.json',
               'FAKE_PID_FILE': self.tmp + '/leftover.pid'}
        env.update({key.upper(): value for key, value in scenario.items()})
        self.addCleanup(self.reap_leftover)
        return env

    def reap_leftover(self):
        """Only the process this test's fake nerdctl started, by its recorded PID."""
        pid_file = self.tmp + '/leftover.pid'
        if os.path.exists(pid_file):
            try:
                os.kill(int(Path(pid_file).read_text()), 9)
            except (OSError, ValueError):
                pass

    def run_c5(self, activation_id='act-c5-0001', *, expires=10, outer_deadline=None, **scenario):
        _, _, staged = self.run_c4()
        c4_ref = self.ref('C4', 'act-c4-closed')
        body = json.loads(Path(c4_ref['path']).read_text())
        env = self.c5_env(**scenario)

        def guest(kw):
            argv = kw['argv']
            index = argv.index(helper.LAUNCHER)
            args = [str(HERE), argv[index + 2], argv[index + 3], argv[index + 4], json.dumps(env), ex.GUEST_HELPER_SHA256]
            return 'import sys\nsys.argv = %r\n' % (['shim'] + args) + GUEST_SHIM

        routes = {step: guest for step in ('c5-guest-nerdctl-version', 'c5-disposable-check', 'c5-read-check-cid',
                                           'c5-container-inspect', 'c5-container-rm', 'c5-container-list')}
        calls = []
        transport = self.routed(routes, calls)
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: body), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32):
            receipt = self.execute('C5', transport, prior_ref=c4_ref, activation_id=activation_id,
                                   expires=expires, outer_deadline=outer_deadline)
        return receipt, calls, staged['path']

    def c5_check_id(self):
        return hashlib.sha256(('effigy-4f6a-check-' + TOKEN + IMAGE).encode()).hexdigest()

    def c5_state(self):
        path = self.tmp + '/nerdctl-state.json'
        return json.loads(Path(path).read_text()) if os.path.exists(path) else {'containers': {}}

    def test_c5_closes_through_the_real_argv_lifecycle_and_removal(self):
        receipt, calls, context = self.run_c5()
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual(receipt['guest_closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['c5-guest-nerdctl-version', 'c5-disposable-check', 'c5-read-check-cid',
                                                 'c5-container-inspect', 'c5-container-rm', 'c5-container-list'])
        by_id = {x['id']: x for x in receipt['steps']}
        for step in calls:
            self.assertEqual(by_id[step[0]]['closure'], 'closed', step[0])
            self.assertEqual(by_id[step[0]]['guest_closure'], 'closed', step[0])
            self.assertFalse(by_id[step[0]]['closure_simulated'], step[0])
        self.assertEqual(by_id['c5-guest-nerdctl-version']['guest_command'], ['nerdctl', '--version'])
        self.assertEqual(by_id['c5-disposable-check']['guest_command'], prep.guest_check_command(context, TOKEN, IMAGE))
        cid = self.c5_check_id()
        self.assertEqual(by_id['c5-container-inspect']['guest_command'], ['sudo', '-n', 'nerdctl', 'container', 'inspect', cid])
        self.assertEqual(by_id['c5-container-rm']['guest_command'], ['sudo', '-n', 'nerdctl', 'container', 'rm', cid])
        self.assertNotIn('-f', by_id['c5-container-rm']['guest_command'])
        stage = self.stage_dir('C5', 'act-c5-0001')
        self.assertEqual(json.loads(Path(stage + '/c5-check-inspect/helper.json').read_text())['result'],
                         {'container_id': cid, 'image_id': IMAGE, 'status': 'exited', 'exit_code': 0})
        self.assertEqual(json.loads(Path(stage + '/c5-check-absent/helper.json').read_text())['result'],
                         {'container_id': cid, 'absent': True})
        self.assertEqual(self.c5_state()['containers'], {})  # the exact-ID removal happened in the lifecycle itself
        self.assertEqual(Path(context + '.check.cid').read_text(), cid + '\n')

    def test_c5_nerdctl_of_another_version_stops_before_any_container_effect(self):
        with self.assertRaisesRegex(ex.Refusal, 'source-qualified version'):
            self.run_c5('act-c5-version', FAKE_NERDCTL_VERSION='nerdctl version 2.4.0')
        receipt = json.loads(Path(self.stage_dir('C5', 'act-c5-version') + '/stage-receipt.json').read_text())
        self.assertEqual(receipt['closure'], 'refused')
        self.assertEqual(self.c5_state()['containers'], {})
        self.assertEqual([x['id'] for x in receipt['steps']], ['c5-guest-nerdctl-version'])

    def test_c5_nonzero_check_is_guest_unknown_and_keeps_its_container_for_residue(self):
        receipt, calls, _ = self.run_c5('act-c5-nonzero', FAKE_RUN_EXIT='1')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls], ['c5-guest-nerdctl-version', 'c5-disposable-check'])
        self.assertTrue(self.c5_state()['containers'])  # kept: the exact-ID removal is never reached

    def test_c5_running_container_stops_before_removal(self):
        with self.assertRaisesRegex(ex.Refusal, 'terminal exited state'):
            self.run_c5('act-c5-running', FAKE_RUN_STATUS='running')
        self.assertTrue(self.c5_state()['containers'])
        receipt = json.loads(Path(self.stage_dir('C5', 'act-c5-running') + '/stage-receipt.json').read_text())
        self.assertNotIn('c5-container-rm', [x['id'] for x in receipt['steps']])

    def test_c5_inspected_container_from_another_image_refuses_and_keeps_it(self):
        with self.assertRaisesRegex(ex.Refusal, 'predecessor image'):
            self.run_c5('act-c5-image', FAKE_INSPECT_IMAGE='sha256:' + 'cd' * 32)
        self.assertTrue(self.c5_state()['containers'])

    def test_c5_inspection_of_a_replaced_container_id_refuses(self):
        with self.assertRaisesRegex(ex.Refusal, 'not the cidfile container'):
            self.run_c5('act-c5-replaced', FAKE_INSPECT_ID='e' * 64)

    def test_c5_container_missing_at_inspection_is_unknown_and_stops(self):
        receipt, calls, _ = self.run_c5('act-c5-missing', FAKE_INSPECT_MISSING='1')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls][-1], 'c5-container-inspect')

    def test_c5_removal_failure_stops_before_the_listing(self):
        receipt, calls, _ = self.run_c5('act-c5-rmfail', FAKE_RM_EXIT='1')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls][-1], 'c5-container-rm')

    def test_c5_removal_that_leaves_the_id_listed_refuses(self):
        with self.assertRaisesRegex(ex.Refusal, 'still listed after its run'):
            self.run_c5('act-c5-noop', FAKE_RM_NOOP='1')

    def test_c5_foreign_container_in_the_listing_does_not_block_closure(self):
        receipt, _, _ = self.run_c5('act-c5-foreign', FAKE_FOREIGN=OTHER_ID)
        self.assertEqual(receipt['closure'], 'closed')

    def test_c5_cidfile_that_is_not_one_full_id_refuses_before_the_inspection(self):
        with self.assertRaisesRegex(ex.Refusal, 'does not hold one full container ID'):
            self.run_c5('act-c5-badcid', FAKE_CIDFILE_TEXT='abc\n')
        receipt = json.loads(Path(self.stage_dir('C5', 'act-c5-badcid') + '/stage-receipt.json').read_text())
        self.assertNotIn('c5-container-inspect', [x['id'] for x in receipt['steps']])

    def test_c5_leftover_process_in_the_group_stays_unknown_and_is_reaped_by_its_recorded_pid(self):
        receipt, calls, _ = self.run_c5('act-c5-leftover', FAKE_LEFTOVER='1')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertEqual([c[0] for c in calls][-1], 'c5-disposable-check')
        self.assertTrue(os.path.exists(self.tmp + '/leftover.pid'))

    def test_c5_guest_reporting_another_command_refuses(self):
        with self.assertRaisesRegex(ex.Refusal, 'ran another command'):
            self.run_c5('act-c5-tamper', FAKE_TAMPER_COMMAND='1')

    def test_c5_remaining_allowance_that_cannot_contain_the_run_guard_refuses_before_dispatch(self):
        with self.assertRaisesRegex(ex.Refusal, 'cannot contain the guest guard'):
            self.run_c5('act-c5-allowance', outer_deadline=time.monotonic() + 120)
        receipt = json.loads(Path(self.stage_dir('C5', 'act-c5-allowance') + '/stage-receipt.json').read_text())
        self.assertNotIn('c5-disposable-check', [x['id'] for x in receipt['steps']])
        self.assertEqual(self.c5_state()['containers'], {})

    def test_c5_one_use_claim_refuses_a_replay_before_any_effect(self):
        self.run_c5('act-c5-once')
        calls = []
        with self.assertRaises(ex.Refusal):
            self.execute('C5', self.routed({}, calls), prior_ref=self.ref('C4', 'act-c4-closed'), activation_id='act-c5-once')
        self.assertEqual(calls, [])

    def test_c5_refuses_a_predecessor_spec_that_differs_from_its_recorded_hash_before_the_claim(self):
        _, _, staged = self.run_c4()
        c4_root = self.stage_dir('C4', 'act-c4-closed')
        body = json.loads(Path(c4_root + '/stage-receipt.json').read_text())
        spec = Path(c4_root + '/c4-render-verify-spec/c4-render-verify-spec.json')
        spec.write_bytes(spec.read_bytes().replace(b'effigy-531-image-', b'effigy-531-image-x'))
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: body), \
                mock.patch.object(ex, 'origin_token32', lambda *a, **k: self.T32), \
                self.assertRaisesRegex(ex.Refusal, 'hash its helper recorded'):
            self.execute('C5', self.routed({}, []), prior_ref=self.ref('C4', 'act-c4-closed'), activation_id='act-c5-tamper')
        self.assertFalse(os.path.lexists(self.tmp + '/claims/act-c5-tamper.claim'))

@contextlib.contextmanager
def copied_checkout(parent, name):
    root = Path(parent) / name
    old_root = ex.REPO_ROOT
    sources = list(ex.PROGRAM_FILES) + [ex.MANIFEST_PATH, ex.RECIPE_PATH,
                                         old_root / 'effigy.toml', old_root / 'config/tasks.toml']
    for source in sources:
        target = root / source.relative_to(old_root)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    with contextlib.ExitStack() as stack:
        for attr in ('PROGRAM_FILES', 'MANIFEST_PATH', 'RECIPE_PATH', 'GUEST_CONTEXT_PATH', 'GUEST_HELPER_PATH'):
            value = getattr(ex, attr)
            mapped = tuple(root / p.relative_to(old_root) for p in value) if isinstance(value, tuple) else root / value.relative_to(old_root)
            stack.enter_context(mock.patch.object(ex, attr, mapped))
        stack.enter_context(mock.patch.object(ex, 'REPO_ROOT', root))
        yield root


class WindowProofTests(Fixture):
    def request(self, activation_id='window-proof-0001'):
        options = dict(activation_id=activation_id, duration_seconds=300, evidence_root=self.evidence)
        return proof.window_recipe('D0', self.bindings, options)

    def freeze(self, request, name='window-request.json'):
        path = self.tmp + '/' + name
        Path(path).write_text(json.dumps(request, sort_keys=True))
        os.chmod(path, 0o400)
        return path, ex.sha_file(path)

    def test_delayed_simulated_launch_runs_real_entry_and_keeps_window(self):
        with copied_checkout(self.tmp, 'window-first'):
            request = self.request()
            path, digest = self.freeze(request)
        calls, deadlines = [], []
        routes = {'d0-info': emit(ENGINE_JSON), 'd0-ps': emit(''),
                  'd0-network': emit(''), 'd0-volume': emit('')}
        admitted = dict(ex.UNADMITTED)
        admitted.pop('D0', None)
        prepared = datetime.datetime.now(datetime.timezone.utc)
        before = prepared + datetime.timedelta(seconds=1800)
        real_datetime = datetime.datetime
        class LaunchClock(real_datetime):
            @classmethod
            def now(cls, tz=None):
                return before
        with copied_checkout(self.tmp, 'window-second'), \
             mock.patch.object(proof.datetime, 'datetime', LaunchClock), \
             mock.patch.object(ex, 'UNADMITTED', admitted), \
             mock.patch.object(ex, 'default_transport', self.routed(routes, calls, before=lambda kw: deadlines.append(kw['cleanup_deadline']))), \
             mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(proof.main(['proof', path, digest]), 0)
            with self.assertRaises(ex.Refusal):
                proof.main(['proof', path, digest])
        retained = json.loads(Path(request['evidence_path']).read_text())
        go = retained['activation']
        issued = datetime.datetime.fromisoformat(go['issued_utc'])
        expires = datetime.datetime.fromisoformat(go['expires_utc'])
        self.assertGreater((issued - prepared).total_seconds(), 1200)
        self.assertGreaterEqual(issued, before)
        self.assertEqual((expires - issued).total_seconds(), 300)
        raw = Path(request['activation_path']).read_bytes()
        self.assertEqual(raw, retained['activation_bytes_utf8'].encode())
        self.assertEqual(sha_bytes(raw), retained['activation_sha256'])
        self.assertEqual(retained['request_sha256'], digest)
        self.assertEqual(go['source_sha256'], request['source_sha256'])
        self.assertEqual(json.loads(Path(request['claim_path']).read_text())['go_sha256'], digest)
        self.assertTrue(calls)
        self.assertEqual(len(set(deadlines)), 1)

    def test_window_refuses_drift_collision_and_partial_write_without_dispatch(self):
        request = self.request()
        for index, change in enumerate(({'duration_seconds': 1201}, {'stage': 'C1'},
                                        {'bindings_sha256': '0' * 64}, {'roots': {}})):
            path, digest = self.freeze(dict(request, **change), 'bad-%d' % index)
            with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaises(ex.Refusal):
                proof.main(['proof', path, digest])
            effect.assert_not_called()
        path, digest = self.freeze(request)
        original_write = ex.write_new_file
        def partial(directory, name, data, **kwargs):
            if directory == self.activations:
                Path(directory, name).write_bytes(data[:3])
                raise OSError('simulated partial write')
            return original_write(directory, name, data, **kwargs)
        with mock.patch.object(ex, 'write_new_file', side_effect=partial), \
             mock.patch.object(ex, '_execute_stage') as effect, self.assertRaises(OSError):
            proof.main(['proof', path, digest])
        effect.assert_not_called()
        with self.assertRaises(ex.Refusal):
            proof.main(['proof', path, digest])

    def test_reaped_timeout_through_entry_keeps_spent_window(self):
        request = self.request('window-timeout')
        path, digest = self.freeze(request)
        calls = []
        transport = self.routed({'d0-info': 'import time; time.sleep(30)'}, calls)
        def short(**kw):
            return transport(**dict(kw, timeout=min(kw['timeout'], 0.1)))
        with mock.patch.object(ex, 'default_transport', short), \
             mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(proof.main(['proof', path, digest]), 1)
            with self.assertRaises(ex.Refusal):
                proof.main(['proof', path, digest])
        receipt = json.loads(Path(self.evidence, 'D0-window-timeout', 'stage-receipt.json').read_text())
        self.assertEqual(receipt['steps'][-1]['closure'], 'unknown_retained')
        self.assertIsNotNone(receipt['steps'][-1]['waited_exit'])
        self.assertEqual([call[0] for call in calls], ['d0-info'])
        self.assertTrue(Path(request['evidence_path']).exists())

    def test_window_refuses_replaced_root_and_collision_before_claim(self):
        request = self.request()
        path, digest = self.freeze(request)
        os.rename(self.evidence, self.evidence + '-original')
        os.mkdir(self.evidence, 0o700)
        with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaises(ex.Refusal):
            proof.main(['proof', path, digest])
        effect.assert_not_called()
        self.assertEqual(os.listdir(self.tmp + '/claims'), [])
        fresh = self.request('window-collision')
        Path(fresh['activation_path']).write_text('occupied')
        path, digest = self.freeze(fresh, 'collision-request')
        with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaises(ex.Refusal):
            proof.main(['proof', path, digest])
        effect.assert_not_called()
        self.assertEqual(os.listdir(self.tmp + '/claims'), [])

    def assert_replaced_d0_destination_refused(self, key):
        parent = os.path.dirname(self.bindings[key])
        os.mkdir(parent, 0o700)
        os.mkdir(self.bindings[key], 0o700)
        request = self.request('window-replaced-' + key)
        path, digest = self.freeze(request)
        os.rename(self.bindings[key], self.bindings[key] + '-old')
        os.mkdir(self.bindings[key], 0o700)
        with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaises(ex.Refusal):
            proof.main(['proof', path, digest])
        effect.assert_not_called()
        self.assertEqual(os.listdir(self.tmp + '/claims'), [])
        self.assertFalse(os.path.exists(request['activation_path']))
        self.assertFalse(os.path.exists(request['evidence_path']))

    def test_replaced_desktop_config_refuses_before_claim(self):
        self.assert_replaced_d0_destination_refused('desktop_config')

    def test_replaced_desktop_context_refuses_before_claim(self):
        self.assert_replaced_d0_destination_refused('desktop_context_dir')

    def test_appearing_or_symlinked_d0_destination_refuses_before_claim(self):
        request = self.request()
        path, digest = self.freeze(request)
        os.symlink(self.evidence, os.path.dirname(self.bindings['desktop_config']))
        with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaises(ex.Refusal):
            proof.main(['proof', path, digest])
        effect.assert_not_called()
        self.assertEqual(os.listdir(self.tmp + '/claims'), [])

    def test_new_d0_directory_refuses_before_claim(self):
        request = self.request()
        path, digest = self.freeze(request)
        os.mkdir(os.path.dirname(self.bindings['desktop_config']), 0o700)
        os.mkdir(self.bindings['desktop_context_dir'], 0o700)
        with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaises(ex.Refusal):
            proof.main(['proof', path, digest])
        effect.assert_not_called()
        self.assertEqual(os.listdir(self.tmp + '/claims'), [])

    def test_window_duration_boundaries_and_input_template_drift(self):
        options = dict(activation_id='window-boundary', duration_seconds=61, evidence_root=self.evidence)
        for duration in (61, 360):
            self.assertEqual(proof.window_recipe('D0', self.bindings, dict(options, duration_seconds=duration))
                             ['duration_seconds'], duration)
        for duration in (0, 60, 361, 1200, 1201, True):
            with self.subTest(duration=duration), self.assertRaises(ex.Refusal):
                proof.window_recipe('D0', self.bindings, dict(options, duration_seconds=duration))
        request = self.request()
        changed = json.loads(json.dumps(request))
        changed['inputs']['repository'][next(iter(changed['inputs']['repository']))] = '0' * 64
        for index, altered in enumerate((changed, dict(request, issued_utc='2026-01-01T00:00:00Z'),
                                         dict(request, absent_paths=[]))):
            path, digest = self.freeze(altered, 'window-drift-%d' % index)
            with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaises(ex.Refusal):
                proof.main(['proof', path, digest])
            effect.assert_not_called()
        self.assertEqual(os.listdir(self.tmp + '/claims'), [])

    def test_window_plan_has_no_effects_and_rejects_duplicate_options(self):
        before = tree(self.tmp)
        request = self.request()
        self.assertEqual(before, tree(self.tmp))
        self.assertNotIn('issued_utc', request)
        self.assertNotIn('expires_utc', request)
        with self.assertRaises(ex.Refusal):
            proof.unique_object([('duration_seconds', 300), ('duration_seconds', 300)])


class StableProofTests(Fixture):
    def test_two_checkouts_bind_same_request_and_retain_distinct_maps(self):
        with copied_checkout(self.tmp, 'first'):
            identity, first = ex.stable_input_manifest()
            activation, sha = self.activation('D0', None, 'cross-checkout', inputs=identity)
            data = proof.prospective('D0', self.bindings)
        with copied_checkout(self.tmp, 'second'):
            other, second = ex.stable_input_manifest()
            self.assertEqual(identity, other)
            self.assertNotEqual(first, second)
            for name, inputs in (('first-map.json', first), ('second-map.json', second)):
                Path(self.tmp, name).write_text(json.dumps(inputs, sort_keys=True))
                self.assertEqual(json.loads(Path(self.tmp, name).read_text()), inputs)
            request = {key: data[key] for key in ('stage', 'bindings', 'inputs', 'inputs_sha256',
                                                   'manifest_sha256', 'source_sha256')}
            request.update(activation_path=activation, activation_sha256=sha, evidence_root=self.evidence)
            path = self.tmp + '/cross-request.json'
            Path(path).write_text(json.dumps(request, sort_keys=True))
            os.chmod(path, 0o400)
            with mock.patch.object(ex, '_execute_stage', return_value={'closure': 'closed'}) as effect, \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(proof.main(['proof', path, ex.sha_file(path)]), 0)
                with self.assertRaises(ex.Refusal):
                    proof.main(['proof', path, ex.sha_file(path)])
                effect.assert_called_once()

    def test_changed_inputs_and_declared_manifest_refuse_before_effect(self):
        with copied_checkout(self.tmp, 'changed') as root:
            interpreter = Path(self.tmp) / 'python-fixture'
            interpreter.write_bytes(b'interpreter')
            with mock.patch.object(ex.sys, 'executable', str(interpreter)):
                identity, _ = ex.stable_input_manifest()
                activation, sha = self.activation('D0', None, 'drift-binding', inputs=identity)
                data = proof.prospective('D0', self.bindings)
                request = {key: data[key] for key in ('stage', 'bindings', 'inputs', 'inputs_sha256',
                                                       'manifest_sha256', 'source_sha256')}
                request.update(activation_path=activation, activation_sha256=sha, evidence_root=self.evidence)
                path = self.tmp + '/drift-request.json'
                def refused(value):
                    if os.path.exists(path):
                        os.chmod(path, 0o600)
                    Path(path).write_text(json.dumps(value, sort_keys=True))
                    os.chmod(path, 0o400)
                    with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaises(ex.Refusal):
                        proof.main(['proof', path, ex.sha_file(path)])
                    effect.assert_not_called()
                    self.assertEqual(os.listdir(self.tmp + '/claims'), [])
                duplicate = self.tmp + '/duplicate-request.json'
                Path(duplicate).write_text('{"stage":"D0","stage":"D0"}')
                os.chmod(duplicate, 0o400)
                with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaisesRegex(ex.Refusal, 'duplicate'):
                    proof.main(['proof', duplicate, ex.sha_file(duplicate)])
                effect.assert_not_called()
                for key in ('missing', 'extra', 'traversal'):
                    altered = json.loads(json.dumps(request))
                    repo = altered['inputs']['repository']
                    if key == 'missing':
                        repo.pop(next(iter(repo)))
                    elif key == 'extra':
                        repo['extra'] = '0' * 64
                    else:
                        repo['../escape'] = repo.pop(next(iter(repo)))
                    refused(altered)
                paths = [ex.PROGRAM_FILES[0], root / 'effigy.toml', ex.MANIFEST_PATH,
                         ex.RECIPE_PATH, Path(self.colima), interpreter]
                for changed in paths:
                    original = changed.read_bytes()
                    changed.write_bytes(original + b'\n')
                    try:
                        refused(request)
                    finally:
                        changed.write_bytes(original)

    def test_declared_set_alias_escape_and_drift_refuse(self):
        with copied_checkout(self.tmp, 'inputs') as root:
            identity, _ = ex.stable_input_manifest()
            programs = ex.PROGRAM_FILES
            for altered in (programs[:-1], programs + (programs[0],),
                            (root / 'wrong.py',) + programs[1:]):
                with mock.patch.object(ex, 'PROGRAM_FILES', altered), self.assertRaises(ex.Refusal):
                    ex.stable_input_manifest()
            target = programs[0]
            target.unlink()
            target.symlink_to(self.colima)
            with self.assertRaises(ex.Refusal):
                ex.stable_input_manifest()
            target.unlink()
            target.write_bytes(b'changed')
            self.assertNotEqual(identity, ex.stable_input_manifest()[0])


class ProgramBindingTests(Fixture):
    """The guest helper is a bound program file. The GO commits to the bytes the executor's input set derives, so a
    change after binding refuses before any dispatch. The pin is the reviewed content check; the binding is what
    the activation commits to. Each test uses a copy under the fixture, never the reviewed file."""

    def test_the_guest_helper_is_bound_with_the_program_files(self):
        self.assertIn(ex.GUEST_CONTEXT_PATH, ex.PROGRAM_FILES)
        self.assertIn(ex.GUEST_HELPER_PATH, ex.PROGRAM_FILES)
        self.assertEqual(ex.derive_inputs()[str(ex.GUEST_CONTEXT_PATH.resolve())], ex.GUEST_CONTEXT_SHA256)
        self.assertEqual(ex.derive_inputs()[str(ex.GUEST_HELPER_PATH.resolve())], ex.GUEST_HELPER_SHA256)

    def test_a_change_to_the_guest_helper_after_binding_refuses_before_dispatch(self):
        with copied_checkout(self.tmp, 'drift') as root:
            identity, _ = ex.stable_input_manifest()
            path, sha = self.activation('C2', None, 'act-drift-0001', inputs=identity)
            copy = root / ex.GUEST_CONTEXT_PATH.relative_to(root)
            drifted = copy.read_bytes() + b'\n'
            copy.write_bytes(drifted)
            calls = []
            # Move the pin with the changed bytes so the activation input binding alone detects drift.
            with mock.patch.object(ex, 'GUEST_CONTEXT_SHA256', hashlib.sha256(drifted).hexdigest()):
                with self.assertRaises(ex.Refusal) as caught:
                    ex.execute_stage('C2', self.bindings, activation_path=path, activation_sha256=sha,
                                     evidence_root=self.evidence, manifest_path=ex.MANIFEST_PATH,
                                     transport=self.routed({}, calls), utc_now=NOW)
            self.assertIn('input drift', str(caught.exception))
            self.assertEqual(calls, [])
            self.assertEqual(os.listdir(self.evidence), [])
            self.assertFalse(os.path.exists(self.tmp + '/claims/act-drift-0001.claim'))

    def test_a_changed_pin_refuses_before_any_plan_or_dispatch(self):
        with mock.patch.object(ex, 'GUEST_CONTEXT_SHA256', '0' * 64):
            with self.assertRaises(ex.Refusal):
                ex.resolve_stage('C2', self.bindings)
        with mock.patch.object(ex, 'GUEST_HELPER_SHA256', '0' * 64):
            with self.assertRaises(ex.Refusal):
                ex.resolve_stage('C2', self.bindings)


class PinnedProbeTests(Fixture):
    """The C2 probe's argv renders the pinned guest helper wrapping the pinned guest source. The helper needs
    Linux /proc, so on this host it must fail closed with no envelope; that is the proof here."""

    def test_c2_probe_argv_is_the_pinned_source_and_fails_closed_on_this_host(self):
        steps = {x['id']: x for x in ex.stage_of(ex.load_manifest(), 'C2')['steps']}
        step = ex.resolve_step(steps['c2-guest-probe'], dict(self.bindings, **ex.guest_source_bindings(),
                                                            guest_nonce='ab' * 32))
        argv = step['argv']
        self.assertEqual(argv[argv.index('-c') + 1], helper.LAUNCHER)
        self.assertEqual(argv[argv.index('-c') + 2], ex.GUEST_HELPER_PATH.read_bytes().hex())
        self.assertEqual(argv[argv.index('-c') + 3], 'probe')
        self.assertEqual(argv[-1], ex.GUEST_CONTEXT_PATH.read_text())
        if sys.platform != 'darwin':
            self.skipTest('the fail-closed proof runs on the macOS host')
        home = self.tmp + '/home'
        os.mkdir(home, 0o700)
        root = self.tmp + '/probe-step'
        os.mkdir(root, 0o700)
        tail = argv[argv.index('python3'):]
        result = primitive.run_stage(root, [sys.executable] + tail[1:], 30, 262144, {}, cwd=root,
                                     environment={'PATH': '/usr/bin:/bin', 'HOME': home})
        self.assertNotEqual(result['waited_exit'], 0)
        self.assertEqual(result['streams']['stdout']['bytes'], 0)  # no envelope is emitted without /proc
        stderr = Path(root, 'stderr').read_bytes()
        self.assertIn(b'refused:', stderr)
        self.assertIn(b'guest boot or process identity unavailable', stderr)
        self.assertNotIn(b'helper source bytes unavailable', stderr)


class FreshChainRootTests(Fixture):
    """A genuine fresh C0 receipt supplies the owned roots of the C stages that follow it: the binding is the
    chain's own verified creation record, never the pinned 688 records and never a caller path. C0 runs the real
    fixed helpers over a fixture 688 record for its tool and tranche checks; C1 and C2 run through the adapter's
    entry point with controlled transports. No native effect, credential or network read."""

    def use_historical(self, roots):
        """A fixture 688 C0 record and tranche summary, pinned by their own sha256 for this test only. The record's
        token is the recorded one and it names this fixture's tool bytes, so C0's tool and tranche helpers run for
        real. `roots` becomes the record's roots_final, used only by the historical C1 binding."""
        hist = self.tmp + '/historical'
        os.mkdir(hist, 0o700)
        tranche = hist + '/summary.json'
        Path(tranche).write_bytes(b'{"tranche": "fixture"}\n')
        tranche_sha = ex.sha_file(tranche)
        record = dict(status='passed: fixture', token=ex.RECORDED_TOKEN, zero_share_present=True, roots_safe=True,
                      failed_at=None,
                      tools={'colima': {'sha256': ex.sha_file(self.colima)},
                             'docker': {'sha256': ex.sha_file(self.docker)}},
                      tranche_summary_sha256=tranche_sha,
                      roots_final={r['path']: dict(device=r['device'], inode=r['inode'], entries=r['entries'])
                                   for r in roots.values()})
        data = (json.dumps(record, sort_keys=True, indent=2) + '\n').encode()
        record_path = hist + '/C0.stage.json'
        Path(record_path).write_bytes(data)
        for patcher in (mock.patch.dict(ex.HISTORICAL_C0, dict(path=record_path, sha256=sha_bytes(data),
                                                              tranche_path=tranche, tranche_sha256=tranche_sha)),
                        mock.patch.object(ex, 'LINUX_ARTIFACT_DIR', hist)):
            patcher.start()
            self.addCleanup(patcher.stop)
        return dict(path=record_path, sha256=sha_bytes(data))

    def run_c0(self, token, activation_id):
        """C0 through the real fixed helpers: no helper receipt is fabricated, and the stage closes on a
        controlled transport for its one exec step."""
        bindings = self.make_bindings(token)
        calls = []
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}), \
             mock.patch.object(ex, 'HISTORICAL_TOOL_HASHES',
                               {'colima': ex.sha_file(self.colima), 'docker': ex.sha_file(self.docker)}):
            receipt = self.execute('C0', self.routed({'c0-colima-start-help': emit('help')}, calls), prior_ref=None,
                                   activation_id=activation_id, bindings=bindings,
                                   monitor_factory=FixedHelperTests.quiet_monitor)
        self.assertEqual(receipt['closure'], 'closed', receipt)
        self.assertEqual([c[0] for c in calls], ['c0-colima-start-help'])
        for step in receipt['steps'][:4]:  # every helper ran as a supervised child with its own receipt
            self.assertIn('helper_receipt', step)
            self.assertEqual((step['closure'], step['waited_exit']), ('closed', 0))
        return bindings, receipt, self.ref('C0', activation_id)

    def c0_body(self, receipt):
        return json.loads(Path(receipt['stage_root'], 'stage-receipt.json').read_text())

    def assert_no_claim(self, stage, activation_id):
        self.assertFalse(os.path.lexists(self.tmp + f'/claims/{activation_id}.claim'))
        self.assertFalse(os.path.lexists(self.stage_dir(stage, activation_id)))

    def test_a_genuine_fresh_c0_supplies_the_roots_without_the_historical_binding(self):
        """The contradiction this task removes: a verified fresh C0 used to reach a root check that demanded the
        688 records. The same chain now binds the roots its own C0 created, and the 688 records are not read."""
        self.use_historical({})
        bindings, c0, c0_ref = self.run_c0('f0f0f0b1', 'fresh-c0-0001')
        body = self.c0_body(c0)
        ex.verify_receipt_tree(body, 'C0', ex.chain_key(bindings), ex.load_manifest())  # the origin is genuine
        self.assertNotEqual(bindings['token'], ex.RECORDED_TOKEN)
        wrong = {name: dict(rec, device=0, inode=0) for name, rec in ex.RECORDED_ROOTS.items()}
        calls = []
        with mock.patch.object(ex, 'RECORDED_ROOTS', wrong):  # consulted, these records would refuse the run
            receipt = self.execute('C1', self.routed({'c1-colima-start': emit('started')}, calls),
                                   prior_ref=c0_ref, activation_id='fresh-c1-0001', bindings=bindings)
        self.assertEqual(receipt['closure'], 'closed', receipt)
        self.assertEqual([c[0] for c in calls], ['c1-colima-start'])
        self.assertEqual(receipt['prior']['stage'], 'C0')
        self.assertEqual(receipt['token'], bindings['token'])

    def test_the_verified_chain_carries_the_fresh_roots_to_a_later_c_stage(self):
        """C2 consumes the genuine C1 receipt, so its root check walks two verified links to the C0 creation
        record. Authenticated helper envelopes close the echo and dispatch the probe."""
        self.use_historical({})
        bindings, _, c0_ref = self.run_c0('f0f0f0b5', 'fresh-c0-0005')
        c1 = self.execute('C1', self.routed({'c1-colima-start': emit('started')}, []), prior_ref=c0_ref,
                          activation_id='fresh-c1-0005', bindings=bindings)
        self.assertEqual(c1['closure'], 'closed')
        c1_ref = self.ref('C1', 'fresh-c1-0005')
        parent = self.guest_parent()
        routes = {'c2-colima-status': emit('{"status":"running"}'), 'c2-route-echo': closed_guest(prep.ROUTE_ECHO),
                  'c2-guest-probe': closed_guest(json.dumps({'parent': parent}))}
        calls = []
        receipt = self.execute('C2', self.routed(routes, calls), prior_ref=c1_ref,
                               activation_id='fresh-c2-0005', bindings=bindings)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertFalse(receipt['closure_simulated'])
        self.assertEqual([c[0] for c in calls], ['c2-colima-status', 'c2-route-echo', 'c2-guest-probe'])
        ex.verify_receipt_tree(json.loads(Path(receipt['stage_root'], 'stage-receipt.json').read_text()),
                               'C2', ex.chain_key(bindings), ex.load_manifest())
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}):
            receipt2 = self.execute('C2', self.routed(routes, []), prior_ref=c1_ref,
                                    activation_id='fresh-c2-admitted', bindings=bindings)
        self.assertEqual(receipt2['closure'], 'closed')

    def test_origin_roots_are_the_c0_creation_record_by_shape_identity_mode_and_entries(self):
        self.use_historical({})
        bindings, c0, _ = self.run_c0('f0f0f0b2', 'fresh-c0-0002')
        body = self.c0_body(c0)
        roots = ex.origin_roots(body, bindings, ex.load_manifest())
        self.assertEqual(set(roots), {'control', 'colima_home', 'qualification_root'})
        self.assertEqual(roots['control']['entries'], ['bin', 'c', 'h', 's'])  # what C0 created and pinned
        self.assertEqual((roots['colima_home']['entries'], roots['qualification_root']['entries']), ([], []))
        for name, rec in roots.items():
            info = os.lstat(rec['path'])
            self.assertEqual(rec['path'], prep.control_paths(bindings['token'])[name], name)
            self.assertEqual((rec['device'], rec['inode'], rec['uid'], rec['mode']),
                             (info.st_dev, info.st_ino, info.st_uid, stat.S_IMODE(info.st_mode)), name)
        pins = ex.Pins()
        self.addCleanup(pins.close)
        ex.check_recorded_roots(body, bindings, pins, first_use=True, manifest=ex.load_manifest())
        root_paths = {r['path'] for r in roots.values()}
        self.assertEqual(set(pins.items), root_paths)
        self.assertEqual(set(pins.held), root_paths)  # the roots' own descriptors are held, not re-observed
        pins.recheck()

    @staticmethod
    def swapping_open(target):
        """An os.open that renames `target` and puts an empty directory in its place right after that path has been
        opened: a same-user swap that lands inside the window the check is looking at."""
        swapped = []
        real_open = os.open

        def opener(path, flags, *args, **kwargs):
            fd = real_open(path, flags, *args, **kwargs)
            if path == target and not swapped:
                swapped.append(True)
                os.rename(target, target + '.moved')
                os.mkdir(target, 0o700)
            return fd
        return opener, swapped

    def test_the_pin_is_the_descriptor_that_was_compared_not_a_second_path_look(self):
        """The comparison and the pin share one held descriptor. No path observation runs inside the check, and a
        same-user swap after the open is recorded as the directory that was opened, so the next recheck refuses
        before any effect and the replacement is neither pinned nor written into."""
        self.use_historical({})
        bindings, c0, _ = self.run_c0('f0f0f0c6', 'fresh-c0-0006')
        body = self.c0_body(c0)
        manifest = ex.load_manifest()
        target = bindings['qualification_root']

        def reobserved(*args, **kwargs):
            raise AssertionError('a pinned root was re-observed by path')

        quiet = ex.Pins()
        self.addCleanup(quiet.close)
        with mock.patch.object(ex.Pins, 'observe', side_effect=reobserved), \
                mock.patch.object(ex.Pins, 'pin', side_effect=reobserved):
            ex.check_recorded_roots(body, bindings, quiet, first_use=True, manifest=manifest)
        self.assertEqual(set(quiet.held), {r['path'] for r in ex.origin_roots(body, bindings, manifest).values()})

        pins = ex.Pins()
        self.addCleanup(pins.close)
        opener, swapped = self.swapping_open(target)
        with mock.patch.object(ex.os, 'open', side_effect=opener):
            ex.check_recorded_roots(body, bindings, pins, first_use=True, manifest=manifest)
        self.assertTrue(swapped, 'the swap ran inside the check window')
        moved = os.lstat(target + '.moved')
        self.assertEqual(pins.items[target], (moved.st_dev, moved.st_ino, moved.st_uid, stat.S_IMODE(moved.st_mode)))
        with self.assertRaisesRegex(ex.Refusal, 'pinned directory identity, owner or mode changed'):
            pins.recheck()  # the replacement is caught before any step

        # The same window through the entry point: the stage refuses before its first step, so the replacement is
        # never reached and never adopted.
        other, _, other_ref = self.run_c0('f0f0f0c7', 'fresh-c0-0007b')
        opener, swapped = self.swapping_open(other['qualification_root'])
        calls = []
        with mock.patch.object(ex.os, 'open', side_effect=opener):
            with self.assertRaisesRegex(ex.Refusal, 'pinned directory identity, owner or mode changed'):
                self.execute('C1', self.routed({}, calls), prior_ref=other_ref,
                             activation_id='fresh-c1-swap', bindings=other)
        self.assertTrue(swapped)
        self.assertEqual(calls, [])
        self.assertFalse(os.path.lexists(other['control'] + '/cache'))  # C1's own creation never ran

    def test_a_conflicting_pin_refuses_as_a_refusal_and_never_closes_the_callers_descriptor(self):
        """Review 5815984482: adopt must not close a descriptor it refuses, or the caller's own close becomes a
        second close and a drift refusal surfaces as OSError. The same conflict on the check path raises Refusal,
        leaves nothing held, and leaks no descriptor."""
        self.use_historical({})
        bindings, c0, _ = self.run_c0('f0f0f0c8', 'fresh-c0-0008b')
        body = self.c0_body(c0)
        manifest = ex.load_manifest()
        control = bindings['control']
        foreign = (0, 0, os.geteuid(), 0o700)  # a conflicting pin, as a drift would leave it

        def open_descriptors():
            return len(os.listdir('/dev/fd'))

        before = open_descriptors()
        pins = ex.Pins()
        self.addCleanup(pins.close)
        pins.items[control] = foreign
        fd = os.open(control, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            with self.assertRaisesRegex(ex.Refusal, 'held directory differs from its pin'):
                pins.adopt(control, fd)
            self.assertNotIn(control, pins.held)
            self.assertEqual(pins.items[control], foreign)
            os.close(fd)  # the caller keeps ownership: exactly one close, and it must not raise EBADF
            fd = None
        finally:
            if fd is not None:
                os.close(fd)

        pins = ex.Pins()
        self.addCleanup(pins.close)
        pins.items[control] = foreign
        with self.assertRaisesRegex(ex.Refusal, 'held directory differs from its pin'):
            ex.check_recorded_roots(body, bindings, pins, first_use=True, manifest=manifest)
        self.assertEqual(pins.held, {})
        self.assertEqual(set(pins.items), {control})  # the refusal came from the first root, before any other pin
        self.assertEqual(open_descriptors(), before)  # every descriptor the check opened was closed exactly once

    def test_hold_refuses_a_mismatched_pin_as_a_refusal_and_keeps_descriptor_accounting(self):
        """Review 5815996833: Pins.hold's refusal path is production code (the fixed-helper anchors and created
        directories), so it is pinned here. A pre-pinned foreign identity refuses as Refusal with no OSError, takes
        no second descriptor and leaks none; a matching pin still holds, so the refusal is the mismatch itself."""
        self.use_historical({})
        bindings, _, _ = self.run_c0('f0f0f0c9', 'fresh-c0-0009b')
        control = bindings['control']
        foreign = (0, 0, os.geteuid(), 0o700)

        def open_descriptors():
            return len(os.listdir('/dev/fd'))

        before = open_descriptors()
        pins = ex.Pins()
        self.addCleanup(pins.close)
        pins.items[control] = foreign
        with self.assertRaisesRegex(ex.Refusal, 'held directory differs from its pin'):
            pins.hold(control)
        self.assertEqual(pins.held, {})
        self.assertEqual(pins.items[control], foreign)
        self.assertEqual(open_descriptors(), before)  # hold's own descriptor was closed exactly once

        del pins.items[control]  # with no conflicting pin the same hold succeeds and holds one descriptor
        pins.hold(control)
        self.assertIn(control, pins.held)
        pins.close()
        self.assertEqual(pins.held, {})
        self.assertEqual(open_descriptors(), before)

    def test_a_tampered_c0_creation_record_refuses_where_the_field_is_read(self):
        """A self-consistent edit of the C0 creation helper's record re-records every hash the receipt names, so
        only the field-level binding can refuse it. The live roots are untouched and never adopted."""
        self.use_historical({})
        bindings, c0, c0_ref = self.run_c0('f0f0f0b6', 'fresh-c0-0006')

        def rewrite(mutate):
            """Re-record every hash and stream the C0 receipt, its helper evidence and its child receipt name, so
            the receipt stays self-consistent and only the field-level checks can refuse it."""
            step_root = Path(c0['stage_root'], 'c0-make-owned-roots')
            evidence_path = step_root / 'helper.json'
            helper = json.loads(evidence_path.read_text())
            mutate(helper['result'])
            evidence_path.write_text(json.dumps(helper, sort_keys=True, indent=2) + '\n')
            stdout = json.loads((step_root / 'stdout').read_text())
            stdout['result'] = helper['result']
            data = json.dumps(stdout, sort_keys=True).encode()
            (step_root / 'stdout').write_bytes(data)
            stream = dict(sha256=sha_bytes(data), bytes=len(data))
            receipt = json.loads(Path(c0_ref['path']).read_text())
            step = next(s for s in receipt['steps'] if s['id'] == 'c0-make-owned-roots')
            child = json.loads(Path(step['helper_receipt']['path']).read_text())
            child['streams']['stdout'] = stream
            if isinstance(child.get('observed_bytes'), dict) and 'stdout' in child['observed_bytes']:
                child['observed_bytes']['stdout'] = len(data)
            Path(step['helper_receipt']['path']).write_text(json.dumps(child, sort_keys=True, indent=2) + '\n')
            step['helper_receipt']['sha256'] = ex.sha_file(step['helper_receipt']['path'])
            step['evidence']['sha256'] = ex.sha_file(evidence_path)
            step['streams']['stdout'] = stream
            Path(c0_ref['path']).write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
            return dict(c0_ref, sha256=ex.sha_file(c0_ref['path']))

        def row(result, name):
            return next(item for item in result['created']
                        if item['path'] == prep.control_paths(bindings['token'])[name])

        def inode(result):
            row(result, 'control')['identity'][1] += 1

        def uid(result):
            row(result, 'qualification_root')['identity'][2] = os.geteuid() + 1

        def mode(result):
            row(result, 'colima_home')['identity'][3] = 0o755

        def missing(result):
            result['created'] = [item for item in result['created']
                                 if item['path'] != prep.control_paths(bindings['token'])['qualification_root']]

        cases = {'inode': (inode, 'root identity changed since the C0 origin creation record'),
                 'uid': (uid, "recorded another user's root"),
                 'mode': (mode, 'was not mode 0700'),
                 'missing': (missing, 'did not create exactly')}
        for name, (mutate, message) in cases.items():
            with self.subTest(field=name):
                forged = rewrite(mutate)
                calls = []
                with self.assertRaisesRegex(ex.Refusal, message):
                    self.execute('C1', self.routed({}, calls), prior_ref=forged,
                                 activation_id=f'fresh-c1-tamper-{name}', bindings=bindings)
                self.assertEqual(calls, [])
                self.assert_no_claim('C1', f'fresh-c1-tamper-{name}')
        # The origin's own control binding is a field of the receipt: an edit refuses before any root is read.
        receipt = json.loads(Path(c0_ref['path']).read_text())
        receipt['control'] = self.tmp + '/mac-4f6a-elsewhere'
        Path(c0_ref['path']).write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
        with self.assertRaisesRegex(ex.Refusal, "does not name this token's control root"):
            self.execute('C1', self.routed({}, []), prior_ref=dict(c0_ref, sha256=ex.sha_file(c0_ref['path'])),
                         activation_id='fresh-c1-control', bindings=bindings)
        self.assert_no_claim('C1', 'fresh-c1-control')

    def test_original_creation_output_and_evidence_tampering_refuse(self):
        self.use_historical({})
        bindings, c0, c0_ref = self.run_c0('f0f0f0b7', 'fresh-c0-0007')
        step_root = Path(c0['stage_root'], 'c0-make-owned-roots')
        stdout = step_root / 'stdout'
        original = stdout.read_bytes()
        data = json.loads(original)
        data['result']['created'][0]['identity'][1] += 1
        stdout.write_text(json.dumps(data, sort_keys=True))
        calls = []
        with self.assertRaisesRegex(ex.Refusal, 'helper stdout changed'):
            self.execute('C1', self.routed({}, calls), prior_ref=c0_ref, activation_id='fresh-c1-output',
                         bindings=bindings)
        self.assertEqual(calls, [])
        stdout.write_bytes(original)
        evidence = step_root / 'helper.json'
        helper = json.loads(evidence.read_text())
        helper['result']['created'][0]['identity'][1] += 1
        evidence.write_text(json.dumps(helper, sort_keys=True, indent=2) + '\n')
        with self.assertRaisesRegex(ex.Refusal, 'differs from its recorded hash'):
            self.execute('C1', self.routed({}, calls), prior_ref=c0_ref, activation_id='fresh-c1-evidence',
                         bindings=bindings)
        self.assertEqual(calls, [])
        self.assert_no_claim('C1', 'fresh-c1-evidence')

    def test_a_replaced_remodelled_or_extended_root_refuses_before_any_call(self):
        self.use_historical({})
        for token, case in (('f0f0f0c1', 'replaced'), ('f0f0f0c2', 'mode'), ('f0f0f0c3', 'entries')):
            with self.subTest(case=case):
                bindings, _, c0_ref = self.run_c0(token, f'fresh-c0-{case}')
                control = bindings['control']
                if case == 'replaced':
                    os.rename(control, control + '.moved')
                    os.mkdir(control, 0o700)
                    message = 'root identity changed since the C0 origin creation record'
                elif case == 'mode':
                    os.chmod(bindings['colima_home'], 0o755)
                    message = 'root mode changed'
                else:
                    Path(control, 'unexpected').write_bytes(b'x')
                    message = 'root entries differ from the C0 origin creation record'
                calls = []
                with self.assertRaisesRegex(ex.Refusal, message):
                    self.execute('C1', self.routed({}, calls), prior_ref=c0_ref,
                                 activation_id=f'fresh-c1-{case}', bindings=bindings)
                self.assertEqual(calls, [])
                self.assert_no_claim('C1', f'fresh-c1-{case}')

    def test_a_missing_root_or_an_alias_refuses_before_any_call(self):
        """A later C stage checks identity only, so the absent and aliased shapes are read through the same check
        after C1 has populated the control root."""
        self.use_historical({})
        for token, case in (('f0f0f0c4', 'missing'), ('f0f0f0c5', 'alias')):
            with self.subTest(case=case):
                bindings, c0, c0_ref = self.run_c0(token, f'fresh-c0-{case}')
                body = self.c0_body(c0)
                path = bindings['qualification_root'] if case == 'missing' else bindings['colima_home']
                os.rename(path, path + '.moved')
                if case == 'alias':
                    os.symlink(bindings['control'], path)
                    message = 'alias or non-directory root'
                else:
                    message = 'recorded root is absent'
                with self.assertRaisesRegex(ex.Refusal, message):
                    ex.check_recorded_roots(body, bindings, ex.Pins(), first_use=False, manifest=ex.load_manifest())

    def test_the_recorded_token_keeps_its_own_pinned_records_and_never_gets_a_minted_origin(self):
        """The historical binding stays exact and distinct: for the recorded token the root check reads the 688
        identities, not a chain's creation record, and a historical C0 cannot supply fresh roots."""
        self.use_historical({})
        bindings, c0, _ = self.run_c0('f0f0f0b8', 'fresh-c0-0008')
        body = self.c0_body(c0)
        roots = ex.origin_roots(body, bindings, ex.load_manifest())
        recorded = {name: dict(rec, inode=rec['inode'] + 1) for name, rec in roots.items()}
        with mock.patch.object(ex, 'RECORDED_ROOTS', recorded):
            with self.assertRaisesRegex(ex.Refusal, 'root identity changed since the recorded C0 receipt'):
                ex.check_recorded_roots(body, dict(bindings, token=ex.RECORDED_TOKEN), ex.Pins(),
                                        first_use=True, manifest=ex.load_manifest())
        historical = dict(stage='C0', closure='closed', historical=True, token=bindings['token'],
                          control=bindings['control'])
        with self.assertRaisesRegex(ex.Refusal, 'minted no token32'):
            ex.origin_roots(historical, bindings, ex.load_manifest())
        with self.assertRaisesRegex(ex.Refusal, 'the chain has no executor C0 origin'):
            ex.origin_roots(ex.executor_origin({'stage': 'C1', 'receipts': []}), bindings, ex.load_manifest())

    def test_ordinary_entry_and_unadmitted_c1_refuse_before_effects(self):
        self.use_historical({})
        bindings, _, c0_ref = self.run_c0('f0f0f0b9', 'fresh-c0-0009')
        calls = []
        path, sha = self.activation('C1', c0_ref, 'fresh-c1-refused', bindings=bindings)
        with mock.patch.dict(ex.TEST_SEAM, {'owner_activation': False}):
            with self.assertRaisesRegex(ex.Refusal, 'no owner-controlled activation mechanism'):
                ex.execute_stage('C1', bindings, activation_path=path, activation_sha256=sha,
                                 evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        # C1 and C4 are admitted now (their fetches are pinned). C5 without its C4 receipt refuses before any claim.
        c5_path, c5_sha = self.activation('C5', c0_ref, 'fresh-c5-refused', bindings=bindings)
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}):
            with self.assertRaisesRegex(ex.Refusal, 'C4 receipt'):
                ex.execute_stage('C5', bindings, activation_path=c5_path, activation_sha256=c5_sha,
                                 evidence_root=self.evidence, transport=self.routed({}, calls), utc_now=NOW)
        self.assertEqual(calls, [])
        self.assert_no_claim('C5', 'fresh-c5-refused')

    def test_a_chain_that_is_not_this_tokens_origin_refuses_before_the_roots_are_read(self):
        self.use_historical({})
        bindings, c0, c0_ref = self.run_c0('f0f0f0ba', 'fresh-c0-000a')
        other = self.make_bindings('f0f0f0bb')
        calls = []
        with self.assertRaisesRegex(ex.Refusal, 'another backend chain'):
            self.execute('C1', self.routed({}, calls), prior_ref=c0_ref, activation_id='fresh-c1-other',
                         bindings=other)
        self.assertEqual(calls, [])
        self.assertFalse(os.path.lexists(other['control']))


class CoverageLedgerTests(unittest.TestCase):
    """Every manifest step is accounted for, with the test that exercises it and how:
    executed (the adapter ran it with a real local child or the real handler); executed_stub_chain (the adapter
    ran it, with the predecessor stubbed); helper_direct (its handler ran on controlled inputs, not through a stage);
    executed_closure_simulated (the stage loop dispatched it under the test-only closure simulation, with its exact
    argv, env, stdin and outputs asserted). A not_dispatched step is refusal coverage only, never dispatch coverage.
    A step with no entry fails."""

    LEDGER = {
        'c0-verify-tools': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c0-verify-tranche': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c0-claim-intent': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c0-make-owned-roots': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c0-colima-start-help': ('executed', 'DispatchSweepTests.test_c0_runs_on_a_fresh_token_against_the_pinned_tools_and_tranche'),
        'c1-make-owned-dirs': ('executed', 'DispatchSweepTests.test_c1_binds_the_pinned_historical_c0_by_identity_and_dispatches_its_colima_start'),
        'c1-colima-start': ('executed', 'DispatchSweepTests.test_c1_binds_the_pinned_historical_c0_by_identity_and_dispatches_its_colima_start'),
        'c1-prefetch-colima-image': ('executed', 'ColimaImageFetchTests.test_exact_bytes_are_fetched_once_into_the_owned_directory'),
        'c2-colima-status': ('executed', 'DispatchSweepTests.test_c2_route_echo_is_the_first_guest_effect_and_no_source_payload_is_dispatched'),
        'c2-route-echo': ('executed', 'DispatchSweepTests.test_c2_route_echo_is_the_first_guest_effect_and_no_source_payload_is_dispatched'),
        'c2-compare-route-echo': ('executed', 'DispatchSweepTests.test_c2_comparison_runs_through_the_adapter_and_the_probe_is_recorded_not_dispatched'),
        'c2-guest-probe': ('executed', 'DispatchSweepTests.test_c2_route_echo_is_the_first_guest_effect_and_no_source_payload_is_dispatched'),
        'c3-render-guest-spec': ('executed_stub_chain', 'DispatchSweepTests.test_c3_runs_the_pinned_stage_source_with_the_reviewed_recipe_as_its_stdin'),
        'c3-stage-context': ('executed_stub_chain', 'DispatchSweepTests.test_c3_runs_the_pinned_stage_source_with_the_reviewed_recipe_as_its_stdin'),
        'c4-render-verify-spec': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-make-input-dir': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-prefetch-runtime-inputs': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-render-input-specs': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-render-input-transfer': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-transfer-inputs': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-check-input-transfer': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-verify-context': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-verify-inputs': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-check-input-verify': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-nerdctl-build-help': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-check-build-help': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-nerdctl-build': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-iidfile-read': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-read-iidfile': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-image-inspect': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c4-check-image-identity': ('executed', 'DispatchSweepTests.test_c4_stage_closes_with_the_production_transfer_and_remote_commands'),
        'c5-guest-nerdctl-version': ('executed_stub_chain', 'DispatchSweepTests.test_c5_nerdctl_of_another_version_stops_before_any_container_effect'),
        'c5-check-nerdctl-version': ('executed_stub_chain', 'DispatchSweepTests.test_c5_nerdctl_of_another_version_stops_before_any_container_effect'),
        'c5-disposable-check': ('executed_stub_chain', 'DispatchSweepTests.test_c5_closes_through_the_real_argv_lifecycle_and_removal'),
        'c5-check-facts': ('helper_direct', 'HelperDispatchTests.test_check_runtime_facts_needs_exit_zero_and_root_facts'),
        'c5-read-check-cid': ('executed_stub_chain', 'DispatchSweepTests.test_c5_closes_through_the_real_argv_lifecycle_and_removal'),
        'c5-check-cid': ('executed_stub_chain', 'DispatchSweepTests.test_c5_cidfile_that_is_not_one_full_id_refuses_before_the_inspection'),
        'c5-container-inspect': ('executed_stub_chain', 'DispatchSweepTests.test_c5_inspected_container_from_another_image_refuses_and_keeps_it'),
        'c5-check-inspect': ('executed_stub_chain', 'DispatchSweepTests.test_c5_running_container_stops_before_removal'),
        'c5-container-rm': ('executed_stub_chain', 'DispatchSweepTests.test_c5_closes_through_the_real_argv_lifecycle_and_removal'),
        'c5-container-list': ('executed_stub_chain', 'DispatchSweepTests.test_c5_removal_that_leaves_the_id_listed_refuses'),
        'c5-check-absent': ('executed_stub_chain', 'DispatchSweepTests.test_c5_removal_that_leaves_the_id_listed_refuses'),
        'd2-buildx-metadata': ('executed', 'D2BuildxRouteTests.test_the_link_is_one_owned_symlink_to_the_pinned_plugin_before_the_fetch'),
        'd2-check-buildx-metadata': ('executed', 'D2BuildxRouteTests.test_the_link_is_one_owned_symlink_to_the_pinned_plugin_before_the_fetch'),
        'd2-check-buildx-custody': ('executed', 'D2BuildxRouteTests.test_a_changed_plugin_after_the_metadata_check_refuses_before_the_build'),
        'c5-check-facts': ('helper_direct', 'HelperDispatchTests.test_check_runtime_facts_needs_exit_zero_and_root_facts'),
        'c6-colima-list': ('executed_stub_chain', 'DispatchSeamTests.test_c6_host_step_dispatches_under_the_recorded_roots_and_closes'),
        'd0-make-owned-dirs': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-verify-endpoint': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-info': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-ps': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-network': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd0-volume': ('executed', 'GenuineChainTests.test_d0_observes_the_engine_and_closes'),
        'd1-engine-info': ('executed', 'GenuineChainTests.test_d1_creates_only_after_observing_the_engine_and_the_label_absent'),
        'd1-label-absent': ('executed', 'GenuineChainTests.test_d1_creates_only_after_observing_the_engine_and_the_label_absent'),
        'd1-volume-create': ('executed', 'GenuineChainTests.test_d1_creates_only_after_observing_the_engine_and_the_label_absent'),
        'd1-volume-inspect': ('executed', 'GenuineChainTests.test_d1_creates_only_after_observing_the_engine_and_the_label_absent'),
        'd2-engine-info': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-make-input-dir': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-link-buildx-plugin': ('executed', 'D2BuildxRouteTests.test_the_link_is_one_owned_symlink_to_the_pinned_plugin_before_the_fetch'),
        'd2-make-context-dir': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-prefetch-runtime-inputs': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-host-stage': ('executed', 'GenuineChainTests.test_d2_positive_stages_the_dockerfile_then_builds_and_reads_back'),
        'd2-docker-build': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-read-iidfile': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-readback': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd2-check-image-identity': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd3-engine-info': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd3-disposable-check': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd3-read-cidfile': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd3-check-facts': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-observe-info': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-observe-ps': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-observe-volume': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-observe-image': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-select-owned': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-owned-rm': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-owned-volume-rm': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-owned-image-rm': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-post-ps': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-post-volume': ('executed', 'GenuineChainTests.test_d4_refuses_when_a_foreign_container_appears_during_cleanup'),
        'd4-observe-images': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-container-info': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-container-inspect': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-container-check': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-volume-info': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-volume-inspect': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-volume-check': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-image-info': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-image-inspect': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-image-check': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
        'd4-post-images': ('executed', 'GenuineChainTests.test_d2_d3_d4_chain_runs_end_to_end_with_no_fabricated_receipt'),
    }

    def test_every_manifest_step_has_an_entry(self):
        steps = {x['id'] for stage in ex.load_manifest()['stages'] for x in stage['steps']}
        self.assertEqual(set(self.LEDGER), steps)

    def test_every_referenced_test_exists_and_the_kinds_are_known(self):
        kinds = {'executed', 'executed_stub_chain', 'helper_direct', 'executed_closure_simulated'}
        for step, (kind, ref) in self.LEDGER.items():
            with self.subTest(step=step):
                self.assertIn(kind, kinds)
                cls, _, method = ref.partition('.')
                self.assertTrue(hasattr(globals()[cls], method), ref)


class PrimitiveTests(Fixture):
    def stage_dir(self, name):
        path = self.tmp + '/' + name
        os.mkdir(path, 0o700)
        return path

    def test_intent_file_is_durable_before_the_spawn(self):
        root = self.stage_dir('intent')
        code = "import os,sys\nsys.stdout.write('ok' if os.path.exists('intent.json') else 'missing')\n"
        result = primitive.run_stage(root, [sys.executable, '-I', '-c', code], 10, 1024, {}, cwd=root,
                                     environment=CHILD_ENV, identity_probe=ex.start_identity)
        self.assertEqual(Path(root + '/stdout').read_text(), 'ok')
        self.assertIn('start_monotonic', result['process'])

    def test_streams_are_captured_independently_and_exactly(self):
        root = self.stage_dir('streams')
        result = primitive.run_stage(root, [sys.executable, '-I', '-c',
                                            'import sys; sys.stdout.write("OUT"); sys.stderr.write("ERR!")'],
                                     10, 1024, {}, environment=CHILD_ENV)
        self.assertEqual(Path(root + '/stdout').read_bytes(), b'OUT')
        self.assertEqual(Path(root + '/stderr').read_bytes(), b'ERR!')
        self.assertNotEqual(result['streams']['stdout']['sha256'], result['streams']['stderr']['sha256'])

    def test_bounded_stdin_is_hash_bound_and_refused_on_drift(self):
        root = self.stage_dir('stdin')
        data = b'recipe bytes'
        path = self.tmp + '/recipe'
        Path(path).write_bytes(data)
        primitive.run_stage(root, [sys.executable, '-I', '-c', 'import sys; sys.stdout.write(sys.stdin.read())'],
                            10, 1024, {}, environment=CHILD_ENV, stdin_path=path, stdin_sha256=sha_bytes(data))
        self.assertEqual(Path(root + '/stdout').read_bytes(), data)
        bad = self.stage_dir('stdin-bad')
        with self.assertRaises(primitive.Refusal):
            primitive.run_stage(bad, [sys.executable, '-I', '-c', 'pass'], 10, 1024, {}, environment=CHILD_ENV,
                                stdin_path=path, stdin_sha256='0' * 64)
        self.assertFalse(os.path.exists(bad + '/intent.json'))

    def test_shared_deadline_kills_before_the_step_timeout(self):
        root = self.stage_dir('deadline')
        result = primitive.run_stage(root, [sys.executable, '-I', '-c', 'import time; time.sleep(30)'], 60, 1024, {},
                                     environment=CHILD_ENV, deadline=time.monotonic() + 0.5)
        self.assertEqual(result['disposition'], 'timeout')


class CreationAttributionTests(Fixture):
    """D1 creation attribution through the stage loop: the engine names the volume, the create prints that name, and
    only the inspect of that exact name, carrying this occurrence's label, forms a receipt. Malformed, racing, drifted
    or unknown output stops the stage before any receipt or later effect."""

    def test_malformed_create_output_refuses_before_any_inspect(self):
        d0 = self.d0()
        volume = volume_for('act-malformed')
        outputs = {
            'empty': '', 'no-newline': volume, 'two-names': volume + '\n' + volume + '\n',
            'caller-name': 'effigy-4f6a-qual-x\n', 'uppercase': volume.upper() + '\n', 'trailing-text': volume + ' extra\n',
        }
        for label, text in outputs.items():
            with self.subTest(output=label):
                calls = []
                routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                          'd1-volume-create': emit(text), 'd1-volume-inspect': emit(inspect_object(volume))}
                with self.assertRaises(ex.Refusal):
                    self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-m-' + label[:6].replace('-', 'x'))
                self.assertEqual([c[0] for c in calls][-1], 'd1-volume-create')

    def test_named_create_racing_a_foreign_object_is_refused(self):
        d0 = self.d0()
        volume = volume_for('act-race')
        foreign = json.dumps({'Name': volume, 'Driver': 'local', 'Scope': 'local', 'Mountpoint': '/x',
                              'CreatedAt': '2026-11-01T00:00:00Z', 'Labels': {'owner': 'someone-else'}})
        calls = []
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                  'd1-volume-create': create_child(volume), 'd1-volume-inspect': emit(foreign)}
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed(routes, calls), prior_ref=d0, activation_id='act-race')
        self.assertEqual(calls[-1][0], 'd1-volume-inspect')

    def test_inspect_of_a_different_name_than_the_create_printed_is_refused(self):
        d0 = self.d0()
        volume, other = volume_for('act-x'), volume_for('act-y')
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                  'd1-volume-create': create_child(volume), 'd1-volume-inspect': emit(inspect_object(other))}
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed(routes, []), prior_ref=d0, activation_id='act-name-differs')

    def test_timeout_on_the_create_is_unknown_and_nothing_later_runs(self):
        d0 = self.d0()
        calls = []
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                  'd1-volume-create': 'import time; time.sleep(30)', 'd1-volume-inspect': emit('')}
        transport = self.cap('d1-volume-create', 1.0)(self.routed(routes, calls))
        receipt = self.execute('D1', transport, prior_ref=d0, activation_id='act-create-timeout')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(receipt['steps'][-1]['closure'], 'unknown_retained')
        self.assertNotIn('d1-volume-inspect', [c[0] for c in calls])
        self.assertEqual(receipt['receipts'], [])

    def test_failed_inspect_leaves_the_create_unreceipted_and_stops(self):
        d0 = self.d0()
        volume = volume_for('act-no-inspect')
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                  'd1-volume-create': create_child(volume), 'd1-volume-inspect': emit('', 1, 'Error: no such volume')}
        receipt = self.execute('D1', self.routed(routes, []), prior_ref=d0, activation_id='act-no-inspect')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(receipt['receipts'], [])
        self.assertEqual(receipt['steps'][-1]['waited_exit'], 1)

    def test_a_reused_activation_refuses_before_any_effect(self):
        d0 = self.d0()
        self.d1(d0, activation_id='act-once')
        calls = []
        with self.assertRaises(ex.Refusal):
            self.d1(d0, activation_id='act-once', calls=calls)
        self.assertEqual(calls, [])

    def test_input_drift_refuses_before_any_effect(self):
        d0 = self.d0()
        drifted = dict(ex.stable_input_manifest()[0])
        drifted['repository'] = dict(drifted['repository'], **{'docs/qualification/workspace-private-531/macos_preparation.py': '0' * 64})
        calls = []
        with self.assertRaises(ex.Refusal):
            self.execute('D1', self.routed({'d1-engine-info': emit(ENGINE_JSON)}, calls), prior_ref=d0,
                         activation_id='act-drift', inputs=drifted)
        self.assertEqual(calls, [])

    def test_d2_cannot_consume_the_d0_receipt_directly(self):
        d0 = self.d0()
        with self.assertRaises(ex.Refusal):
            self.d2(d0, activation_id='act-swap-0001')

    def test_d1_refuses_a_docker_directory_the_original_d0_did_not_record(self):
        d0 = self.d0()
        ctx = self.bindings['desktop_context_dir']
        os.rmdir(ctx)
        os.mkdir(ctx, 0o700)  # same name, new identity: the D0 chain recorded the old one
        calls = []
        with self.assertRaises(ex.Refusal) as caught:
            self.execute('D1', self.routed({'d1-engine-info': emit(ENGINE_JSON)}, calls), prior_ref=d0,
                         activation_id='act-ctx-replaced')
        self.assertIn('original D0 chain recorded', str(caught.exception))
        self.assertEqual(calls, [])

    def test_d0_identities_come_from_the_verified_original_chain(self):
        d0 = self.d0()
        identities = ex.d0_identities(ex.read_receipt(d0), self.bindings)
        self.assertEqual(set(identities), {self.bindings['control'], self.bindings['desktop_config'],
                                           self.bindings['desktop_context_dir']})
        for path, identity in identities.items():
            with self.subTest(path=path):
                self.assertEqual(identity, ex.Pins.observe(path))


class CleanupBindingTests(Fixture):
    """D4 through the stage loop: per-target engine and inspect before each removal, complete image inventory before and
    after, and no removal after a refused, failed or unknown step. The chain is genuine D0 to D3."""

    def d3_ref(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        _, d2_ref = self.d2(d1_ref)
        _, d3_ref = self.d3(d2_ref)
        return d3_ref

    def d4(self, overrides, calls, activation_id='act-d4-b'):
        d3_ref = self.d3_ref()
        volume = volume_for('act-d1-0001')
        routes = self.d4_routes(volume, overrides=overrides)
        return self.execute('D4', self.routed(routes, calls), prior_ref=d3_ref, activation_id=activation_id)

    def test_target_replaced_between_selection_and_its_check_is_refused(self):
        drifted = emit(inspect_object(volume_for('act-d1-0001')).replace('2026-11-08T09:20:01Z', '2026-11-09T00:00:00Z'))
        calls = []
        with self.assertRaises(ex.Refusal) as caught:
            self.d4({'d4-volume-inspect': drifted}, calls)
        self.assertIn('differs from its creation receipt', str(caught.exception))
        self.assertNotIn('d4-owned-volume-rm', [c[0] for c in calls])

    def test_missing_per_target_inspect_refuses_before_its_removal(self):
        calls = []
        with self.assertRaises(ex.Refusal):
            self.d4({'d4-image-inspect': emit('')}, calls)
        steps = [c[0] for c in calls]
        self.assertIn('d4-owned-volume-rm', steps)  # the earlier, fully checked class ran
        self.assertNotIn('d4-owned-image-rm', steps)

    def test_failed_per_target_inspect_stops_before_the_removal(self):
        calls = []
        receipt = self.d4({'d4-volume-inspect': emit('', 1, 'Error: no such volume')}, calls)
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertNotIn('d4-owned-volume-rm', [c[0] for c in calls])
        self.assertNotIn('d4-owned-image-rm', [c[0] for c in calls])

    def test_engine_observed_before_each_removal_is_the_chain_engine(self):
        calls = []
        with self.assertRaises(ex.Refusal) as caught:
            self.d4({'d4-image-info': emit('{"ID":"other-engine"}')}, calls)
        self.assertIn('engine changed before this removal', str(caught.exception))
        self.assertNotIn('d4-owned-image-rm', [c[0] for c in calls])

    def test_partial_cleanup_stops_before_later_removals(self):
        calls = []
        receipt = self.d4({'d4-owned-volume-rm': emit('', 1, 'volume is in use')}, calls)
        self.assertEqual(receipt['closure'], 'stopped')
        steps = [c[0] for c in calls]
        self.assertNotIn('d4-owned-image-rm', steps)
        self.assertNotIn('d4-post-images', steps)

    def test_unknown_removal_closure_is_retained_and_stops(self):
        calls = []
        transport = self.cap('d4-owned-volume-rm', 1.0)(self.routed(self.d4_routes(volume_for('act-d1-0001'),
                                                                     overrides={'d4-owned-volume-rm': 'import time; time.sleep(30)'}), calls))
        d3_ref = self.d3_ref()
        receipt = self.execute('D4', transport, prior_ref=d3_ref, activation_id='act-d4-unknown')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual([s for s in receipt['steps'] if s['id'] == 'd4-owned-volume-rm'][0]['closure'], 'unknown_retained')
        self.assertNotIn('d4-owned-image-rm', [c[0] for c in calls])

    def test_a_foreign_image_removed_as_a_side_effect_is_detected(self):
        foreign = 'sha256:' + 'cc' * 32
        before = emit(json.dumps({'ID': IMAGE}) + '\n' + json.dumps({'ID': foreign}) + '\n')
        calls = []
        with self.assertRaises(ex.Refusal) as caught:  # the foreign image is gone after cleanup: observation retained
            self.d4({'d4-observe-images': before, 'd4-post-images': emit('')}, calls)
        self.assertIn('foreign list changed', str(caught.exception))
        self.assertIn('d4-owned-image-rm', [c[0] for c in calls])  # the receipted removal did run

    def test_selected_image_absent_from_the_complete_inventory_refuses(self):
        calls = []
        with self.assertRaises(ex.Refusal) as caught:
            self.d4({'d4-observe-images': emit(json.dumps({'ID': 'cc' * 32}) + '\n')}, calls)
        self.assertIn('complete image inventory', str(caught.exception))

    def test_cleanup_argv_never_forces_or_prunes_and_names_only_receipted_targets(self):
        steps = {x['id']: x for x in ex.stage_of(ex.load_manifest(), 'D4')['steps']}
        for step in steps.values():
            if step['kind'] != 'exec':
                continue
            with self.subTest(step=step['id']):
                self.assertFalse(set(step['argv']) & {'-f', '--force'})
                self.assertNotIn('prune', step['argv'])
        self.assertEqual(steps['d4-owned-rm']['argv'][-1], '{owned_container_ids}')
        self.assertEqual(steps['d4-owned-volume-rm']['argv'][-1], '{owned_volume_name}')
        self.assertEqual(steps['d4-owned-image-rm']['argv'][-1], '{owned_image_id}')


class D1WindowTests(Fixture):
    """D1 through the proof entry's window: the request names the original D0 receipt it consumes and pins the Docker
    directories that receipt recorded. A non-window D1 request is refused."""

    def request(self, prior, activation_id='d1-window-0001', **extra):
        options = dict(activation_id=activation_id, duration_seconds=300, evidence_root=self.evidence)
        if prior is not None:
            options['prior_receipt'] = prior
        options.update(extra)
        return proof.window_recipe('D1', self.bindings, options)

    def freeze(self, request, name='d1-window-request.json'):
        path = self.tmp + '/' + name
        Path(path).write_text(json.dumps(request, sort_keys=True))
        os.chmod(path, 0o400)
        return path, ex.sha_file(path)

    def run_entry(self, path, digest, calls):
        routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                  'd1-volume-create': create_child(volume_for('d1-window-0001')),
                  'd1-volume-inspect': emit(inspect_object(volume_for('d1-window-0001')))}
        with mock.patch.object(ex, 'default_transport', self.routed(routes, calls)), \
             mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
             contextlib.redirect_stdout(io.StringIO()) as out:
            code = proof.main(['proof', path, digest])
        return code, json.loads(out.getvalue())

    def test_d1_window_runs_through_the_entry_and_its_go_names_the_d0_receipt(self):
        d0 = self.d0()
        request = self.request(d0)
        self.assertEqual(request['prior_receipt'], d0)
        path, digest = self.freeze(request)
        calls = []
        code, receipt = self.run_entry(path, digest, calls)
        self.assertEqual(code, 0)
        self.assertEqual(receipt['closure'], 'closed')
        go = json.loads(Path(request['activation_path']).read_text())
        self.assertEqual(go['prior_receipt'], d0)
        self.assertEqual([c[0] for c in calls], ['d1-engine-info', 'd1-label-absent', 'd1-volume-create', 'd1-volume-inspect'])

    def test_d1_window_is_one_use(self):
        d0 = self.d0()
        path, digest = self.freeze(self.request(d0))
        self.run_entry(path, digest, [])
        calls = []
        with self.assertRaises(ex.Refusal):
            self.run_entry(path, digest, calls)
        self.assertEqual(calls, [])

    def test_d1_window_without_a_d0_reference_or_with_a_wrong_hash_refuses(self):
        d0 = self.d0()
        with self.assertRaises(ex.Refusal):
            proof.window_recipe('D1', self.bindings, dict(activation_id='d1-noref-0001', duration_seconds=300,
                                                          evidence_root=self.evidence))
        with self.assertRaises(ex.Refusal):
            self.request(dict(d0, sha256='0' * 64), activation_id='d1-badhash-0001')
        with self.assertRaises(ex.Refusal):  # a D2 receipt is not the D0 reference D1 names
            self.request(dict(d0, stage='D2'), activation_id='d1-wrongstage-0001')

    def test_d1_non_window_request_refuses(self):
        d0 = self.d0()
        inputs = ex.stable_input_manifest()[0]
        path, sha = self.activation('D1', d0, 'd1-legacy-0001')
        request = {'stage': 'D1', 'activation_path': path, 'activation_sha256': sha, 'bindings': self.bindings,
                   'evidence_root': self.evidence, 'inputs': inputs, 'inputs_sha256': ex.canon_sha(inputs),
                   'manifest_sha256': ex.sha_file(ex.MANIFEST_PATH), 'source_sha256': ex.sha_file(ex.__file__)}
        raw = self.tmp + '/d1-legacy-request.json'
        Path(raw).write_text(json.dumps(request, sort_keys=True))
        os.chmod(raw, 0o400)
        with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaisesRegex(ex.Refusal, 'only through a window'):
            proof.main(['proof', raw, ex.sha_file(raw)])
        effect.assert_not_called()

    def test_d1_window_refuses_a_replaced_docker_directory_before_the_claim(self):
        d0 = self.d0()
        path, digest = self.freeze(self.request(d0, activation_id='d1-swap-0001'))
        ctx = self.bindings['desktop_context_dir']
        os.rmdir(ctx)
        os.mkdir(ctx, 0o700)
        with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaisesRegex(ex.Refusal, 'root identity differs'):
            proof.main(['proof', path, digest])
        effect.assert_not_called()
        self.assertFalse(os.path.lexists(self.tmp + '/claims/d1-swap-0001.claim'))


class D2WindowTests(Fixture):
    """D2 through the proof entry's window: the request names the original D1 receipt, pins the Docker
    directories that D0 recorded, and requires desktop_input_dir absent. A non-window D2 request is refused."""

    def request(self, prior, activation_id='d2-window-0001', **extra):
        options = dict(activation_id=activation_id, duration_seconds=600, evidence_root=self.evidence)
        if prior is not None:
            options['prior_receipt'] = prior
        options.update(extra)
        return proof.window_recipe('D2', self.bindings, options)

    def freeze(self, request, name='d2-window-request.json'):
        path = self.tmp + '/' + name
        Path(path).write_text(json.dumps(request, sort_keys=True))
        os.chmod(path, 0o400)
        return path, ex.sha_file(path)

    def run_entry(self, path, digest, calls):
        def write_iid(kw):
            Path(kw['argv'][kw['argv'].index('--iidfile') + 1]).write_text(IMAGE + '\n')
            return emit('')
        routes = {'d2-engine-info': emit(ENGINE_JSON), 'd2-buildx-metadata': RUN_ARGV, 'd2-docker-build': write_iid,
                  'd2-readback': emit(READBACK)}
        with mock.patch.object(ex, 'default_transport', self.routed(routes, calls)), \
             mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
             contextlib.redirect_stdout(io.StringIO()) as out:
            code = proof.main(['proof', path, digest])
        return code, json.loads(out.getvalue())

    def test_d2_window_runs_through_the_entry_and_its_go_names_the_d1_receipt(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        request = self.request(d1_ref)
        self.assertEqual(request['prior_receipt'], d1_ref)
        self.assertEqual(request['absent_paths'], sorted(self.d2_out('d2-window-0001').values()))
        for path in request['absent_paths']:
            self.assertFalse(os.path.lexists(path))
        path, digest = self.freeze(request)
        calls = []
        code, receipt = self.run_entry(path, digest, calls)
        self.assertEqual(code, 0)
        self.assertEqual(receipt['closure'], 'closed')
        go = json.loads(Path(request['activation_path']).read_text())
        self.assertEqual(go['prior_receipt'], d1_ref)
        self.assertEqual([c[0] for c in calls], ['d2-engine-info', 'd2-buildx-metadata', 'd2-docker-build', 'd2-readback'])
        self.assertTrue(os.path.isdir(self.d2_out('d2-window-0001')['desktop_input_dir'] + '/oci'))
        self.assertEqual(len(list(Path(self.d2_out('d2-window-0001')['desktop_input_dir'] + '/debs').iterdir())),
                         len(runtime_inputs.DEBS))

    def test_d2_window_is_one_use(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        path, digest = self.freeze(self.request(d1_ref))
        self.run_entry(path, digest, [])
        calls = []
        with self.assertRaises(ex.Refusal):
            self.run_entry(path, digest, calls)
        self.assertEqual(calls, [])

    def test_d2_window_without_a_d1_reference_or_with_a_wrong_hash_refuses(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        with self.assertRaises(ex.Refusal):
            proof.window_recipe('D2', self.bindings, dict(activation_id='d2-noref-0001', duration_seconds=600,
                                                          evidence_root=self.evidence))
        with self.assertRaises(ex.Refusal):
            self.request(dict(d1_ref, sha256='0' * 64), activation_id='d2-badhash-0001')
        with self.assertRaises(ex.Refusal):  # a D0 receipt is not the D1 reference D2 names
            self.request(dict(d1_ref, stage='D0'), activation_id='d2-wrongstage-0001')

    def test_d2_non_window_request_refuses(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        inputs = ex.stable_input_manifest()[0]
        path, sha = self.activation('D2', d1_ref, 'd2-legacy-0001')
        request = {'stage': 'D2', 'activation_path': path, 'activation_sha256': sha, 'bindings': self.bindings,
                   'evidence_root': self.evidence, 'inputs': inputs, 'inputs_sha256': ex.canon_sha(inputs),
                   'manifest_sha256': ex.sha_file(ex.MANIFEST_PATH), 'source_sha256': ex.sha_file(ex.__file__)}
        raw = self.tmp + '/d2-legacy-request.json'
        Path(raw).write_text(json.dumps(request, sort_keys=True))
        os.chmod(raw, 0o400)
        with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaisesRegex(ex.Refusal, 'only through a window'):
            proof.main(['proof', raw, ex.sha_file(raw)])
        effect.assert_not_called()

    def test_d2_window_refuses_an_existing_input_dir_before_the_claim(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        path, digest = self.freeze(self.request(d1_ref, activation_id='d2-present-0001'))
        os.mkdir(self.d2_out('d2-present-0001')['desktop_input_dir'], 0o700)
        with mock.patch.object(ex, '_execute_stage') as effect, self.assertRaisesRegex(ex.Refusal, 'named destination is not absent'):
            proof.main(['proof', path, digest])
        effect.assert_not_called()
        self.assertFalse(os.path.lexists(self.tmp + '/claims/d2-present-0001.claim'))


class DesktopReadyTests(Fixture):
    """Entry-path simulations use local children and never confer native receipt credit."""
    def setUp(self):
        super().setUp()
        prior = self.d0()
        self.disk = self.tmp + '/disk'
        Path(self.disk).write_bytes(b'disk')
        for target, name, value in ((proof, 'ORIGINAL_D0', prior), (proof, 'DESKTOP_DISK', self.disk),
                                    (proof, 'OPEN', self.docker)):
            patcher = mock.patch.object(target, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        patcher = mock.patch.object(proof, 'app_inputs', return_value={'bundle_sha256': 'a' * 64,
                                                                     'open_sha256': ex.sha_file(self.docker)})
        patcher.start()
        self.addCleanup(patcher.stop)

    def request(self, name='ready-test-0001', duration=300):
        return proof.ready_recipe(self.bindings, dict(activation_id=name, duration_seconds=duration,
                                                       evidence_root=self.evidence))

    def entry(self, request, routes, calls, d1_overrides=None):
        path = self.tmp + '/request-' + request['d1']['activation_id']
        if not os.path.exists(path):
            Path(path).write_text(json.dumps(request, sort_keys=True))
            os.chmod(path, 0o400)
        real_run = primitive.run_stage
        d1calls = []
        volume = volume_for(request['d1']['activation_id'])
        d1routes = {'d1-engine-info': emit(ENGINE_JSON), 'd1-label-absent': emit(''),
                    'd1-volume-create': create_child(volume), 'd1-volume-inspect': emit(inspect_object(volume))}
        d1routes.update(d1_overrides or {})

        def local(root, argv, *args, **kwargs):
            name = os.path.basename(str(root))
            if '/ready-' in str(root):
                calls.append((name, argv))
                if name not in routes:
                    raise AssertionError('unexpected readiness step ' + name)
                argv = [sys.executable, '-I', '-B', '-c', routes[name]]
                # This local-child route is explicitly simulated; production uses the fixed-query guard.
                kwargs['fixed_context_query'] = False
            return real_run(root, argv, *args, **kwargs)

        with mock.patch.object(primitive, 'run_stage', side_effect=local), \
             mock.patch.object(ex, 'default_transport', self.routed(d1routes, d1calls)), \
             mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
             contextlib.redirect_stdout(io.StringIO()) as out:
            code = proof.main(['proof', path, ex.sha_file(path)])
        return code, json.loads(out.getvalue()), d1calls

    def routes(self):
        error = ('failed to connect to the docker API at ' + prep.DESKTOP_HOST +
                 '; check if the path is correct and if the daemon is running: dial unix ' +
                 self.socket_path + ': connect: no such file or directory\n')
        return {'context-before': emit('desktop-linux\n'), 'context-after': emit('desktop-linux\n'),
                'engine-before': emit(ENGINE_JSON) if os.path.lexists(self.socket_path) else emit('', 1, error),
                'engine-info': emit(ENGINE_JSON)}

    def test_ready_skips_start_and_d1_receipt_is_simulation(self):
        request = self.request()
        calls = []
        code, receipt, d1calls = self.entry(request, self.routes(), calls)
        self.assertEqual(code, 0)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertTrue(receipt['closure_simulated'])
        self.assertEqual([x[0] for x in calls], ['context-before', 'engine-before', 'context-after'])
        self.assertEqual([x[0] for x in d1calls], ['d1-engine-info', 'd1-label-absent',
                                                   'd1-volume-create', 'd1-volume-inspect'])
        with self.assertRaisesRegex(ex.Refusal, 'simulated'):
            ex.verify_receipt_tree(receipt, 'D1', ex.chain_key(self.bindings), ex.load_manifest())

    def test_ready_preserves_non_desktop_context(self):
        request = self.request('ready-other-0001')
        routes = dict(self.routes(), **{'context-before': emit('other\n'), 'context-after': emit('other\n')})
        calls = []
        code, _, _ = self.entry(request, routes, calls)
        self.assertEqual(code, 0)
        self.assertNotIn('app-start', [item[0] for item in calls])

    def test_absent_socket_launches_once_then_reaches_d1(self):
        os.unlink(self.socket_path)
        request = self.request('ready-start-0001')
        routes = self.routes()
        routes['app-start'] = ('import socket\ns=socket.socket(socket.AF_UNIX)\ns.bind(%r)\n' % self.socket_path)
        calls = []
        code, _, d1calls = self.entry(request, routes, calls)
        self.assertEqual(code, 0)
        self.assertLess([x[0] for x in calls].index('engine-before'), [x[0] for x in calls].index('app-start'))
        self.assertEqual([x[0] for x in calls].count('app-start'), 1)
        self.assertTrue(d1calls)
        self.assertTrue(os.path.exists(self.tmp + '/claims/start-ready-start-0001.claim'))

    def test_retained_702_output_shape_starts_then_reaches_d1(self):
        recorded = Path(ex.LINUX_ARTIFACT_DIR) / 'd1-proof-preparation.ep88ymux' / 'evidence' / \
            'D1-d1-2e23b881db80c5652152fded0ad195bb' / 'd1-engine-info'
        if not (recorded / 'stdout').is_file() or not (recorded / 'stderr').is_file():
            self.skipTest('the retained 702 streams are not present on this machine')
        stdout, stderr = (recorded / 'stdout').read_bytes(), (recorded / 'stderr').read_bytes()
        self.assertEqual(sha_bytes(stdout), '01077956601bca3d0dc9638082e9f0b7863418540f3d7c15af0cadad397ca923')
        self.assertEqual(sha_bytes(stderr), '02ddce4680313daa323875915ce802d357efb5619e24600f29bd695d32393cd4')
        self.assertEqual(len(stdout), 1380)
        os.unlink(self.socket_path)
        request = self.request('ready-702-shape-0001')
        # The fixture's Unix socket is private; only the dial path in the retained error is redirected.
        error = stderr.decode().replace('dial unix ' + prep.DESKTOP_HOST[len('unix://'):] + ':',
                                        'dial unix ' + self.socket_path + ':')
        routes = dict(self.routes(), **{'engine-before': emit(stdout.decode(), 1, error),
            'app-start': 'import socket\ns=socket.socket(socket.AF_UNIX)\ns.bind(%r)\n' % self.socket_path})
        calls = []
        code, receipt, d1calls = self.entry(request, routes, calls)
        self.assertEqual(code, 0)
        self.assertEqual([x[0] for x in calls].count('app-start'), 1)
        self.assertTrue(d1calls)
        self.assertTrue(receipt['closure_simulated'])
        before = Path(self.evidence) / ('ready-' + request['d1']['activation_id']) / 'engine-before' / 'stdout'
        self.assertEqual(sha_bytes(before.read_bytes()), sha_bytes(stdout))

    def test_start_polls_a_temporarily_unavailable_engine(self):
        os.unlink(self.socket_path)
        request = self.request('ready-poll-0001')
        error = ('failed to connect to the docker API at ' + prep.DESKTOP_HOST +
                 '; check if the path is correct and if the daemon is running: dial unix ' +
                 self.socket_path + ': connect: connection refused\n')
        routes = dict(self.routes(), **{'app-start': 'import socket\ns=socket.socket(socket.AF_UNIX)\ns.bind(%r)\n' % self.socket_path,
                                        'engine-info': emit('', 1, error),
                                        'engine-info-0002': emit(ENGINE_JSON)})
        calls = []
        code, _, d1calls = self.entry(request, routes, calls)
        self.assertEqual(code, 0)
        self.assertEqual([x[0] for x in calls].count('app-start'), 1)
        self.assertIn('engine-info-0002', [x[0] for x in calls])
        self.assertTrue(d1calls)

    def test_existing_socket_with_refused_connection_starts_once(self):
        request = self.request('ready-refused-0001')
        error = ('failed to connect to the docker API at ' + prep.DESKTOP_HOST +
                 '; check if the path is correct and if the daemon is running: dial unix ' +
                 self.socket_path + ': connect: connection refused\n')
        routes = dict(self.routes(), **{'engine-before': emit('', 1, error), 'app-start': emit('')})
        calls = []
        code, _, d1calls = self.entry(request, routes, calls)
        self.assertEqual(code, 0)
        self.assertLess([x[0] for x in calls].index('engine-before'), [x[0] for x in calls].index('app-start'))
        self.assertEqual([x[0] for x in calls].count('app-start'), 1)
        self.assertTrue(d1calls)

    def test_context_change_and_bad_engine_stop_before_d1(self):
        for name, override in [('context', {'context-after': emit('other\n')}),
                               ('engine', {'engine-before': emit('{"ID":"other"}')})]:
            request = self.request('ready-' + name + '-0001')
            calls = []
            with self.assertRaises(ex.Refusal):
                self.entry(request, dict(self.routes(), **override), calls)
            self.assertFalse(os.path.exists(self.evidence + '/D1-' + request['d1']['activation_id']))
            self.assertNotIn('app-start', [x[0] for x in calls])

    def test_unexpected_read_failure_never_starts_desktop(self):
        request = self.request('ready-read-failure-0001')
        calls = []
        with self.assertRaises(ex.Refusal):
            self.entry(request, dict(self.routes(), **{'engine-before': emit('', 1, 'unexpected error\n')}), calls)
        self.assertNotIn('app-start', [x[0] for x in calls])
        self.assertFalse(os.path.exists(self.evidence + '/D1-' + request['d1']['activation_id']))

    def test_nonempty_stdout_does_not_admit_an_unexpected_error(self):
        request = self.request('ready-wrong-error-0001')
        calls = []
        with self.assertRaises(ex.Refusal):
            self.entry(request, dict(self.routes(), **{'engine-before': emit('client facts\n', 1, 'unexpected error\n')}), calls)
        self.assertNotIn('app-start', [x[0] for x in calls])

    def test_successful_read_without_owned_socket_refuses(self):
        os.unlink(self.socket_path)
        request = self.request('ready-missing-socket-0001')
        calls = []
        with self.assertRaisesRegex(ex.Refusal, 'endpoint disappeared'):
            self.entry(request, dict(self.routes(), **{'engine-before': emit(ENGINE_JSON)}), calls)
        self.assertNotIn('app-start', [x[0] for x in calls])
        self.assertFalse(os.path.exists(self.evidence + '/D1-' + request['d1']['activation_id']))

    def test_wrong_context_and_launch_failure_stop_before_d1(self):
        os.unlink(self.socket_path)
        for name, routes in [('context', dict(self.routes(), **{'context-before': emit('other\n')})),
                             ('failure', dict(self.routes(), **{'app-start': emit('', 1)}))]:
            request = self.request('start-' + name + '-0001')
            calls = []
            with self.assertRaises(ex.Refusal):
                self.entry(request, routes, calls)
            self.assertFalse(os.path.exists(self.evidence + '/D1-' + request['d1']['activation_id']))
            if name == 'context':
                self.assertNotIn('app-start', [x[0] for x in calls])

    def test_successful_launcher_without_endpoint_times_out_before_d1(self):
        os.unlink(self.socket_path)
        request = self.request('ready-timeout-0001', duration=61)
        calls = []
        with self.assertRaises(ex.Refusal):
            self.entry(request, dict(self.routes(), **{'app-start': emit('')}), calls)
        self.assertFalse(os.path.exists(self.evidence + '/D1-' + request['d1']['activation_id']))
        self.assertTrue(os.path.exists(self.tmp + '/claims/start-ready-timeout-0001.claim'))

    def test_failed_create_keeps_original_no_replay_disposition(self):
        request = self.request('ready-createfail-0001')
        code, receipt, calls = self.entry(request, self.routes(), [], {'d1-volume-create': emit('', 1)})
        self.assertEqual(code, 1)
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertEqual(receipt['steps'][-1]['closure'], 'failed_closed')
        self.assertEqual(calls[-1][0], 'd1-volume-create')
        with self.assertRaises(ex.Refusal):
            self.entry(request, self.routes(), [])

    def test_backing_file_replaced_or_aliased_refuses(self):
        request = self.request('ready-disk-0001')
        os.rename(self.disk, self.disk + '.old')
        Path(self.disk).write_bytes(b'replacement')
        with self.assertRaises(ex.Refusal):
            self.entry(request, self.routes(), [])
        os.link(self.disk, self.disk + '.link')
        with self.assertRaises(ex.Refusal):
            self.request('ready-link-0001')

    def test_input_drift_and_reused_start_claim_stop_before_launch(self):
        os.unlink(self.socket_path)
        request = self.request('ready-reuse-0001')
        ex.claim_activation('start-ready-reuse-0001', 'a' * 64)
        calls = []
        with self.assertRaisesRegex(ex.Refusal, 'already claimed'):
            self.entry(request, self.routes(), calls)
        self.assertNotIn('app-start', [x[0] for x in calls])
        request = self.request('ready-drift-0001')
        request['app_inputs'] = dict(request['app_inputs'], bundle_sha256='b' * 64)
        with self.assertRaises(ex.Refusal):
            self.entry(request, self.routes(), [])

    def test_spent_d1_claim_refuses_before_startup(self):
        os.unlink(self.socket_path)
        request = self.request('ready-d1-spent-0001')
        ex.claim_activation('ready-d1-spent-0001', 'a' * 64)
        calls = []
        with self.assertRaisesRegex(ex.Refusal, 'no replay'):
            self.entry(request, self.routes(), calls)
        self.assertEqual(calls, [])

    def test_root_or_prior_drift_refuses(self):
        request = self.request('ready-root-0001')
        directory = self.bindings['desktop_context_dir']
        os.rmdir(directory)
        os.mkdir(directory, 0o700)
        with self.assertRaises(ex.Refusal):
            self.entry(request, self.routes(), [])
        with mock.patch.object(proof, 'ORIGINAL_D0', dict(proof.ORIGINAL_D0, sha256='0' * 64)):
            with self.assertRaises(ex.Refusal):
                self.request('ready-prior-0001')

    def test_growth_limit_and_missing_file_refuse(self):
        disk = proof.DesktopDisk()
        self.addCleanup(disk.close)
        disk.initial -= 512 * 1024 * 1024 + 1
        with self.assertRaisesRegex(ex.Refusal, 'growth'):
            disk.check()
        os.rename(self.disk, self.disk + '.missing')
        with self.assertRaises(ex.Refusal):
            disk.check()

    def test_growth_during_start_stops_before_d1(self):
        os.unlink(self.socket_path)
        request = self.request('ready-growth-0001')
        routes = dict(self.routes(), **{'app-start':
            'with open(%r, "ab") as f: f.write(b"x" * 1048576)\n' % self.disk})
        calls = []
        with mock.patch.object(proof, 'DESKTOP_GROWTH_CAP', 0), self.assertRaises(ex.Refusal):
            self.entry(request, routes, calls)
        self.assertIn('app-start', [x[0] for x in calls])
        self.assertFalse(os.path.exists(self.evidence + '/D1-' + request['d1']['activation_id']))

    def test_pinned_cli_context_query_uses_only_synthetic_config(self):
        docker = '/usr/local/bin/docker'
        if not os.path.isfile(docker) or ex.sha_file(docker) != ex.HISTORICAL_TOOL_HASHES['docker']:
            self.fail('source-reviewed Docker CLI is unavailable or changed')
        config = self.tmp + '/synthetic-config'
        os.mkdir(config, 0o700)
        Path(config + '/config.json').write_text(json.dumps({'currentContext': 'desktop-linux',
            'auths': {'example.invalid': {'auth': 'ZHVtbXk6ZHVtbXk='}}, 'credsStore': 'sentinel'}))
        helper = self.tmp + '/docker-credential-sentinel'
        marker = self.tmp + '/helper-called'
        Path(helper).write_text('#!/bin/sh\n: > "$SENTINEL_MARKER"\nexit 1\n')
        os.chmod(helper, 0o700)
        result = subprocess.run([docker, 'context', 'show'], stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15, check=False,
            env={'HOME': self.tmp, 'DOCKER_CONFIG': config, 'PATH': self.tmp + ':/usr/bin:/bin',
                 'SENTINEL_MARKER': marker, 'DOCKER_CLI_HOOKS': 'false', 'DOCKER_CLI_HINTS': 'false',
                 'OTEL_SDK_DISABLED': 'true'})
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b'desktop-linux\n', b''))
        self.assertFalse(os.path.lexists(marker))

    def test_context_supervisor_suppresses_untrusted_config_error(self):
        docker = '/usr/local/bin/docker'
        self.assertEqual(ex.sha_file(docker), ex.HISTORICAL_TOOL_HASHES['docker'])
        config = self.tmp + '/bad-context-config'
        os.mkdir(config, 0o700)
        Path(config + '/config.json').write_text(json.dumps({'currentContext': 'desktop-linux',
            'aliases': {'dummy-private-value': 'example'}}))
        root = self.tmp + '/context-guard'
        os.mkdir(root, 0o700)
        real_popen = subprocess.Popen
        def synthetic(argv, **kwargs):
            kwargs['env'] = dict(kwargs['env'], HOME=self.tmp, DOCKER_CONFIG=config)
            return real_popen(argv, **kwargs)
        env = {'HOME': '/Users/tom', 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'LANG': 'C',
               'DOCKER_CLI_HINTS': 'false', 'DOCKER_CLI_HOOKS': 'false', 'OTEL_SDK_DISABLED': 'true'}
        good_root = self.tmp + '/context-guard-good'
        os.mkdir(good_root, 0o700)
        Path(config + '/config.json').write_text(json.dumps({'currentContext': 'desktop-linux',
            'auths': {'example.invalid': {'auth': 'ZHVtbXk6ZHVtbXk='}}, 'credsStore': 'sentinel'}))
        with mock.patch.object(primitive.subprocess, 'Popen', side_effect=synthetic):
            good = primitive.run_stage(good_root, [docker, 'context', 'show'], 15, 4096,
                       {os.path.realpath(docker): ex.sha_file(docker)}, cwd=good_root,
                       environment=env, fixed_context_query=True)
        self.assertEqual((good['disposition'], good['context_streams']), ('exited', 'original-non-secret'))
        self.assertEqual(Path(good_root + '/stdout').read_bytes(), b'desktop-linux\n')
        self.assertEqual(Path(good_root + '/stderr').read_bytes(), b'')
        Path(config + '/config.json').write_text(json.dumps({'currentContext': 'desktop-linux',
            'aliases': {'dummy-private-value': 'example'}}))
        with mock.patch.object(primitive.subprocess, 'Popen', side_effect=synthetic):
            result = primitive.run_stage(root, [docker, 'context', 'show'], 15, 4096,
                       {os.path.realpath(docker): ex.sha_file(docker)}, cwd=root,
                       environment=env, fixed_context_query=True)
        self.assertEqual(result['context_streams'], 'suppressed-untrusted')
        self.assertNotEqual(result['disposition'], 'exited')
        self.assertEqual(Path(root + '/stdout').read_bytes(), b'')
        self.assertEqual(Path(root + '/stderr').read_bytes(), b'')
        self.assertNotIn('dummy-private-value', Path(root + '/receipt.json').read_text())
        with self.assertRaises(primitive.Refusal):
            primitive.run_stage(root, [docker, 'context', 'ls'], 15, 4096, {}, cwd=root,
                                environment=env, fixed_context_query=True)


class OriginalD0EvidenceTests(unittest.TestCase):
    """The original D0 receipt on Stash, read-only: it verifies with the current verifier, its recorded identities bind
    the D1 roots, and a controlled prospective D1 window names it. No effect and no claim is made."""
    ORIGINAL = ('/Volumes/Stash/scratch/effigy/linux-artifact-preparation.l0tx7tam/d0-proof-preparation.wmn5kjr9/'
                'evidence/D0-d0-22d86f3ea89ca8f1bbdc8b71ae471067/stage-receipt.json')
    ORIGINAL_SHA256 = 'fa32183b7788cd35107997ab9d637252c63db43bd310d3d4e483fb86ca24c015'
    TOKEN = 'e272a9cc'

    def setUp(self):
        self.scratch = os.path.realpath(tempfile.mkdtemp(prefix='d1-prospective-', dir=os.environ.get('TMPDIR') or None))
        self.addCleanup(shutil.rmtree, self.scratch, True)
        os.chmod(self.scratch, 0o700)
        if not os.path.isfile(self.ORIGINAL):
            self.skipTest('the original D0 receipt is not present on this machine')

    def test_original_d0_verifies_and_binds_a_prospective_d1_window(self):
        bindings = {k: v for k, v in prep.control_paths(self.TOKEN).items() if k in prep.SLOTS}
        bindings.update(desktop_host=prep.DESKTOP_HOST, token=self.TOKEN, check_source=prep.CHECK_SOURCE,
                        route_echo=prep.ROUTE_ECHO)
        manifest = ex.load_manifest()
        ref = dict(stage='D0', path=self.ORIGINAL, sha256=self.ORIGINAL_SHA256)
        prior = proof.checked_prior(ref, bindings, manifest)
        self.assertEqual(prior, ref)
        identities = ex.d0_identities(ex.read_receipt(ref), bindings)
        self.assertEqual(len(identities), 3)
        options = dict(activation_id='d1-prospective-0001', duration_seconds=300, evidence_root=self.scratch,
                       prior_receipt=ref)
        request = proof.window_recipe('D1', bindings, options)
        self.assertEqual(request['prior_receipt'], ref)
        self.assertEqual(request['stage'], 'D1')
        self.assertEqual(request['activation_path'], ex.ACTIVATIONS_DIR + '/d1-prospective-0001.json')
        out = self.scratch + '/d1-prospective-request.json'
        Path(out).write_text(json.dumps(request, sort_keys=True, indent=2) + '\n')
        self.assertFalse(os.path.lexists(request['claim_path']))
        self.assertFalse(os.path.lexists(request['activation_path']))


# The real pin function and opener, captured before any fixture swaps them.
REAL_PIN = ex.colima_image_pin
REAL_OPEN = ex.open_pinned_https
REAL_RUNTIME_PIN = runtime_inputs.runtime_image_pin
REAL_RUNTIME_OPEN = runtime_inputs.open_runtime_https


def scripted_https(script, seen):
    """A stand-in for http.client.HTTPSConnection: each request pops the next scripted response and records its host and
    target, so a test can see exactly which hosts were contacted."""
    class Connection:
        def __init__(self, host, port, timeout=None, context=None):
            seen.append(host)

        def request(self, method, target, headers=None):
            seen.append(target)

        def getresponse(self):
            return script.pop(0)

        def close(self):
            pass
    return Connection


class RuntimeImageInputTests(Fixture):
    """Pinned Ubuntu 22.04 arm64 OCI blobs and offline debs: exact bytes, exclusive custody, every
    refusal before or after the transfer. Tiny genuine-byte fixtures only; never the production payloads."""

    def setUp(self):
        super().setUp()
        self.dest = self.bindings['desktop_input_dir']
        os.makedirs(os.path.dirname(self.dest), 0o700, exist_ok=True)
        os.mkdir(self.dest, 0o700)
        self.monitor = mock.Mock(check=mock.Mock(return_value={'storage': 0}))
        self.ctx = dict(bindings=self.bindings, manifest=ex.load_manifest(), monitor=self.monitor)

    def fetch(self, pin=None, opener=None):
        return runtime_inputs.fetch_runtime_inputs(self.dest, pin or FAKE_RUNTIME_PIN,
                                                   opener=opener or fake_runtime_opener(),
                                                   checkpoint=lambda: self.monitor.check())

    def test_exact_bytes_are_fetched_once_into_the_owned_directory(self):
        result = ex.h_prefetch_runtime_inputs(dict(self.ctx, pins=ex.Pins()),
                                              dict(id='d2-prefetch-runtime-inputs', inputs=[self.dest]))
        self.assertEqual(result['bytes'], FAKE_RUNTIME_PIN['bytes'])
        self.assertEqual(result['oci'], self.dest + '/oci')
        self.assertEqual(result['debs'], self.dest + '/debs')
        identities = [(f['bytes'], f['sha256']) for f in result['files']]
        self.assertEqual(identities, runtime_inputs.pin_file_identities(FAKE_RUNTIME_PIN))
        for f in result['files']:
            self.assertEqual(hashlib.sha256(Path(f['path']).read_bytes()).hexdigest(), f['sha256'])
            self.assertEqual(os.path.getsize(f['path']), f['bytes'])
        self.assertEqual((Path(result['oci']) / 'oci-layout').read_bytes(), b'{"imageLayoutVersion":"1.0.0"}\n')
        index = json.loads((Path(result['oci']) / 'index.json').read_bytes())
        self.assertEqual(len(index['manifests']), 1)
        self.assertEqual(index['manifests'][0]['digest'], FAKE_RUNTIME_PIN['manifest']['digest'])
        self.assertEqual(index['manifests'][0]['platform']['architecture'], 'arm64')
        self.assertEqual(len(list(Path(result['debs']).iterdir())), len(runtime_inputs.DEBS))
        self.assertTrue(self.monitor.check.called)

    def test_only_the_pinned_urls_and_the_token_are_fetched(self):
        seen = []
        inner = fake_runtime_opener()

        def opener(url, headers=None):
            seen.append(url)
            return inner(url, headers)

        self.fetch(opener=opener)
        self.assertEqual(seen[0], runtime_inputs.AUTH_TOKEN_URL)
        expected = {FAKE_RUNTIME_PIN[k]['url'] for k in ('index', 'manifest', 'config', 'layer')}
        expected.update(d['url'] for d in FAKE_RUNTIME_PIN['debs'])
        self.assertEqual(set(seen[1:]), expected)
        self.assertNotIn('https://archive.ubuntu.com/ubuntu/pool/main/x.deb', seen)

    def test_a_destination_off_the_reviewed_path_refuses(self):
        other = os.path.dirname(self.dest) + '/other-inputs'
        os.mkdir(other, 0o700)
        with self.assertRaises(primitive.Refusal):
            ex.h_prefetch_runtime_inputs(dict(self.ctx, pins=ex.Pins()),
                                         dict(id='d2-prefetch-runtime-inputs', inputs=[other]))
        with self.assertRaises(primitive.Refusal):
            ex.h_prefetch_runtime_inputs(dict(self.ctx, pins=ex.Pins()),
                                         dict(id='c4-prefetch-runtime-inputs', inputs=[self.dest]))
        self.assertEqual(os.listdir(other), [])

    def test_a_missing_or_nonempty_input_directory_refuses(self):
        os.rmdir(self.dest)
        with self.assertRaises(primitive.Refusal):
            self.fetch()
        os.mkdir(self.dest, 0o700)
        Path(self.dest + '/leftover').write_bytes(b'x')
        with self.assertRaises(primitive.Refusal):
            self.fetch()
        os.remove(self.dest + '/leftover')
        os.rmdir(self.dest)
        elsewhere = self.tmp + '/elsewhere-inputs'
        os.mkdir(elsewhere, 0o700)
        os.symlink(elsewhere, self.dest)
        with self.assertRaises(primitive.Refusal):
            self.fetch()
        self.assertEqual(os.listdir(elsewhere), [])

    def test_unknown_or_mismatched_length_refuses_before_any_payload_byte_is_written(self):
        first = FAKE_RUNTIME_PIN['index']
        for headers in ({'Content-Length': None}, {'Content-Length': str(first['bytes'] + 1)}):
            bodies = dict(FAKE_RUNTIME_BODIES)
            dest = self.tmp + '/len-' + str(headers.get('Content-Length'))
            os.mkdir(dest, 0o700)

            def opener(url, hdrs=None, headers=headers):
                if url == runtime_inputs.AUTH_TOKEN_URL:
                    return FakeConnection(), FakeResponse(b'{"token":"fixture-token"}')
                if url == first['url']:
                    return FakeConnection(), FakeResponse(FAKE_RUNTIME_BYTES['index'], headers=headers)
                raise AssertionError('payload fetch must not start after a length refusal, got %s' % url)

            with self.assertRaises(primitive.Refusal):
                runtime_inputs.fetch_runtime_inputs(dest, FAKE_RUNTIME_PIN, opener=opener)
            self.assertEqual(sorted(os.listdir(dest)), ['debs', 'oci'])
            self.assertFalse(os.path.lexists(os.path.join(dest, 'oci', 'index.upstream.json')))

    def test_encoding_chunking_and_status_refuse_before_write(self):
        first = FAKE_RUNTIME_PIN['index']
        cases = (
            FakeResponse(FAKE_RUNTIME_BYTES['index'], headers={'Content-Encoding': 'gzip'}),
            FakeResponse(FAKE_RUNTIME_BYTES['index'], headers={'Transfer-Encoding': 'chunked'}),
            FakeResponse(FAKE_RUNTIME_BYTES['index'], status=500),
            FakeResponse(FAKE_RUNTIME_BYTES['index'], status=404),
        )
        for i, response in enumerate(cases):
            dest = self.tmp + '/enc-%d' % i
            os.mkdir(dest, 0o700)

            def opener(url, headers=None, response=response):
                if url == runtime_inputs.AUTH_TOKEN_URL:
                    return FakeConnection(), FakeResponse(b'{"token":"fixture-token"}')
                if url == first['url']:
                    return FakeConnection(), response
                raise AssertionError('payload fetch must not start after an encoding or status refusal')

            with self.assertRaises(primitive.Refusal):
                runtime_inputs.fetch_runtime_inputs(dest, FAKE_RUNTIME_PIN, opener=opener)
            self.assertFalse(os.path.lexists(os.path.join(dest, 'oci', 'index.upstream.json')))

    def test_over_cap_and_short_bodies_refuse_and_retain_the_partial_file(self):
        first = FAKE_RUNTIME_PIN['index']
        body = FAKE_RUNTIME_BYTES['index']
        dest = self.tmp + '/oversize'
        os.mkdir(dest, 0o700)

        def oversize(url, headers=None):
            if url == runtime_inputs.AUTH_TOKEN_URL:
                return FakeConnection(), FakeResponse(b'{"token":"fixture-token"}')
            if url == first['url']:
                return FakeConnection(), FakeResponse(body + b'x', headers={'Content-Length': str(len(body))})
            raise AssertionError('later blobs must not be fetched after an oversize refusal')

        with self.assertRaises(primitive.Refusal):
            runtime_inputs.fetch_runtime_inputs(dest, FAKE_RUNTIME_PIN, opener=oversize)
        self.assertTrue((Path(dest) / 'oci' / 'index.upstream.json').is_file())
        dest2 = self.tmp + '/short'
        os.mkdir(dest2, 0o700)

        def short(url, headers=None):
            if url == runtime_inputs.AUTH_TOKEN_URL:
                return FakeConnection(), FakeResponse(b'{"token":"fixture-token"}')
            if url == first['url']:
                return FakeConnection(), FakeResponse(body[:-1], headers={'Content-Length': str(len(body))})
            raise AssertionError('later blobs must not be fetched after a short-body refusal')

        with self.assertRaises(primitive.Refusal):
            runtime_inputs.fetch_runtime_inputs(dest2, FAKE_RUNTIME_PIN, opener=short)
        self.assertEqual((Path(dest2) / 'oci' / 'index.upstream.json').read_bytes(), body[:-1])

    def test_a_same_length_wrong_body_refuses_on_sha256(self):
        first = FAKE_RUNTIME_PIN['index']
        dest = self.tmp + '/wrong-digest'
        os.mkdir(dest, 0o700)
        wrong = b'z' * first['bytes']

        def opener(url, headers=None):
            if url == runtime_inputs.AUTH_TOKEN_URL:
                return FakeConnection(), FakeResponse(b'{"token":"fixture-token"}')
            if url == first['url']:
                return FakeConnection(), FakeResponse(wrong)
            raise AssertionError('later blobs must not be fetched after a digest refusal')

        with self.assertRaises(primitive.Refusal):
            runtime_inputs.fetch_runtime_inputs(dest, FAKE_RUNTIME_PIN, opener=opener)
        self.assertEqual((Path(dest) / 'oci' / 'index.upstream.json').read_bytes(), wrong)

    def test_an_existing_blob_name_is_a_collision_and_is_never_adopted(self):
        data = b'pinned-sha256-bytes'
        dest = self.tmp + '/artifact.blob'
        self.assertEqual(runtime_inputs.prefetch_pinned_sha256(io.BytesIO(data), dest, len(data), _sha(data)),
                         dict(bytes=len(data), sha256=_sha(data)))
        with self.assertRaises(primitive.Refusal):
            runtime_inputs.prefetch_pinned_sha256(io.BytesIO(data), dest, len(data), _sha(data))
        self.assertEqual(Path(dest).read_bytes(), data)
        os.remove(dest)
        os.symlink(self.tmp + '/elsewhere.blob', dest)
        with self.assertRaises(primitive.Refusal):
            runtime_inputs.prefetch_pinned_sha256(io.BytesIO(data), dest, len(data), _sha(data))
        self.assertFalse(os.path.lexists(self.tmp + '/elsewhere.blob'))

    def test_wrong_architecture_in_the_index_or_config_refuses_and_retains(self):
        amd64 = json.dumps({
            'schemaVersion': 2, 'mediaType': 'application/vnd.oci.image.index.v1+json',
            'manifests': [{'mediaType': FAKE_RUNTIME_PIN['manifest']['media_type'],
                           'digest': FAKE_RUNTIME_PIN['manifest']['digest'],
                           'size': FAKE_RUNTIME_PIN['manifest']['bytes'],
                           'platform': {'architecture': 'amd64', 'os': 'linux'}}],
        }, separators=(',', ':')).encode()
        item, _ = _oci(amd64, 'application/vnd.oci.image.index.v1+json', 'unused')
        item['url'] = 'https://registry-1.docker.io/v2/library/ubuntu/manifests/' + item['digest']
        pin = dict(FAKE_RUNTIME_PIN, index=item, bytes=FAKE_RUNTIME_PIN['bytes'] - FAKE_RUNTIME_PIN['index']['bytes'] + item['bytes'])
        bodies = dict(FAKE_RUNTIME_BODIES)
        bodies[item['url']] = amd64
        dest = self.tmp + '/amd64'
        os.mkdir(dest, 0o700)
        with self.assertRaises(primitive.Refusal) as caught:
            runtime_inputs.fetch_runtime_inputs(dest, pin, opener=fake_runtime_opener(bodies))
        self.assertIn('linux/arm64', str(caught.exception))
        self.assertTrue((Path(dest) / 'oci' / 'index.upstream.json').is_file())
        with self.assertRaises(primitive.Refusal) as caught:
            runtime_inputs.verify_arm64_config(b'{"os":"linux","architecture":"amd64"}')
        self.assertIn('linux/arm64', str(caught.exception))

    def test_malformed_index_json_refuses_and_retains(self):
        raw = b'{"this is not an index"'
        item, _ = _oci(raw, 'application/vnd.oci.image.index.v1+json', 'unused')
        item['url'] = 'https://registry-1.docker.io/v2/library/ubuntu/manifests/' + item['digest']
        pin = dict(FAKE_RUNTIME_PIN, index=item,
                   bytes=FAKE_RUNTIME_PIN['bytes'] - FAKE_RUNTIME_PIN['index']['bytes'] + item['bytes'])
        bodies = dict(FAKE_RUNTIME_BODIES)
        bodies[item['url']] = raw
        dest = self.tmp + '/malformed'
        os.mkdir(dest, 0o700)
        with self.assertRaises(primitive.Refusal):
            runtime_inputs.fetch_runtime_inputs(dest, pin, opener=fake_runtime_opener(bodies))
        self.assertEqual((Path(dest) / 'oci' / 'index.upstream.json').read_bytes(), raw)

    def test_token_over_cap_or_malformed_refuses_before_any_payload(self):
        dest = self.tmp + '/token-cap'
        os.mkdir(dest, 0o700)
        with self.assertRaises(primitive.Refusal):
            runtime_inputs.fetch_runtime_inputs(
                dest, FAKE_RUNTIME_PIN, opener=fake_runtime_opener(token=b'{"token":"' + b'a' * 20000 + b'"}'))
        self.assertEqual(os.listdir(dest), [])
        dest2 = self.tmp + '/token-bad'
        os.mkdir(dest2, 0o700)
        with self.assertRaises(primitive.Refusal):
            runtime_inputs.fetch_runtime_inputs(
                dest2, FAKE_RUNTIME_PIN, opener=fake_runtime_opener(token=b'not-json'))
        self.assertEqual(os.listdir(dest2), [])

    def test_a_lost_live_counter_stops_the_transfer_with_the_partial_retained(self):
        calls = []

        def checkpoint():
            calls.append(1)
            raise primitive.Refusal('live counter lost')

        with self.assertRaises(primitive.Refusal):
            runtime_inputs.fetch_runtime_inputs(self.dest, FAKE_RUNTIME_PIN, opener=fake_runtime_opener(),
                                                checkpoint=checkpoint)
        self.assertEqual(calls, [1])
        self.assertTrue(os.path.exists(self.dest + '/oci/index.upstream.json'))

    def test_an_undeclared_url_is_never_fetched_by_the_fixture_opener(self):
        with self.assertRaises(AssertionError):
            fake_runtime_opener()('https://example.com/ubuntu.img', None)


class RuntimeRedirectTests(Fixture):
    """The real runtime opener, with http.client replaced by a scripted connection."""

    def test_redirects_follow_only_allowlisted_https_hosts(self):
        seen, headers_seen = [], []

        class Connection:
            def __init__(self, host, port, timeout=None, context=None):
                seen.append(host)

            def request(self, method, target, headers=None):
                headers_seen.append(dict(headers or {}))
                seen.append(target)

            def getresponse(self):
                if len(headers_seen) == 1:
                    return FakeResponse(b'', 307, {'Location': 'https://production.cloudfront.docker.com/blob'})
                return FakeResponse(b'ok')

            def close(self):
                pass

        with mock.patch('http.client.HTTPSConnection', Connection):
            conn, response = REAL_RUNTIME_OPEN(
                'https://registry-1.docker.io/v2/library/ubuntu/blobs/sha256:' + 'ab' * 32,
                {'Authorization': 'Bearer secret'})
        self.assertEqual(response.status, 200)
        self.assertEqual(seen[0], 'registry-1.docker.io')
        self.assertEqual(seen[2], 'production.cloudfront.docker.com')
        self.assertEqual(headers_seen[0].get('Authorization'), 'Bearer secret')
        self.assertNotIn('Authorization', headers_seen[1])

    def test_a_redirect_off_the_allowlist_or_off_https_refuses_before_connecting(self):
        for location in ('https://evil.example/x', 'http://ports.ubuntu.com/x'):
            seen = []
            with mock.patch('http.client.HTTPSConnection', scripted_https(
                    [FakeResponse(b'', 302, {'Location': location})], seen)):
                with self.assertRaises(primitive.Refusal):
                    REAL_RUNTIME_OPEN('https://ports.ubuntu.com/ubuntu-ports/pool/x.deb')
            self.assertNotIn('evil.example', seen)
            self.assertEqual(seen.count('ports.ubuntu.com'), 1)

    def test_the_first_url_must_be_allowlisted(self):
        seen = []
        with mock.patch('http.client.HTTPSConnection', scripted_https([], seen)):
            with self.assertRaises(primitive.Refusal):
                REAL_RUNTIME_OPEN('https://archive.ubuntu.com/ubuntu/pool/x.deb')
        self.assertEqual(seen, [])


class ColimaImageFetchTests(Fixture):
    """The one pinned Colima image: exact bytes, exclusive custody, every refusal before or after the transfer."""

    def setUp(self):
        super().setUp()
        self.image_dir = self.bindings['colima_cache_home'] + '/' + ex.COLIMA_IMAGE_DIR
        os.makedirs(self.bindings['colima_cache_home'], 0o700, exist_ok=True)
        os.mkdir(self.image_dir, 0o700)
        self.dest = self.image_dir + '/' + prep.COLIMA_IMAGE_NAME
        self.monitor = mock.Mock(check=mock.Mock(return_value={'storage': 0}))
        self.ctx = dict(bindings=self.bindings, manifest=ex.load_manifest(), monitor=self.monitor)

    def fetch(self, response, dest=None):
        conn = FakeConnection()
        return ex.fetch_colima_image(dest or self.dest, FAKE_PIN, opener=lambda url: (conn, response)), conn

    def test_exact_bytes_are_fetched_once_into_the_owned_directory(self):
        result = ex.h_prefetch_colima_image(self.ctx, dict(inputs=[self.dest]))
        self.assertEqual(result, dict(bytes=len(FAKE_IMAGE), sha512=FAKE_PIN['sha512']))
        self.assertEqual(Path(self.dest).read_bytes(), FAKE_IMAGE)
        self.assertEqual(stat.S_IMODE(os.stat(self.dest).st_mode), 0o600)
        self.assertTrue(self.monitor.check.called)  # the live counter sampled the transfer

    def test_a_destination_off_the_reviewed_path_refuses(self):
        with self.assertRaises(primitive.Refusal):
            ex.h_prefetch_colima_image(self.ctx, dict(inputs=[self.image_dir + '/other.raw.gz']))
        self.assertEqual(os.listdir(self.image_dir), [])

    def test_a_missing_image_directory_refuses_as_a_refusal(self):
        os.rmdir(self.image_dir)
        with self.assertRaises(primitive.Refusal):
            ex.h_prefetch_colima_image(self.ctx, dict(inputs=[self.dest]))

    def test_the_image_directory_must_be_empty_owned_and_real(self):
        Path(self.image_dir + '/leftover').write_bytes(b'x')
        with self.assertRaises(primitive.Refusal):
            ex.h_prefetch_colima_image(self.ctx, dict(inputs=[self.dest]))
        os.remove(self.image_dir + '/leftover')
        os.rmdir(self.image_dir)
        elsewhere = self.tmp + '/elsewhere'
        os.mkdir(elsewhere, 0o700)
        os.symlink(elsewhere, self.image_dir)
        with self.assertRaises(primitive.Refusal):
            ex.h_prefetch_colima_image(self.ctx, dict(inputs=[self.dest]))
        self.assertEqual(os.listdir(elsewhere), [])

    def test_unknown_or_mismatched_length_refuses_before_any_byte_is_written(self):
        for headers in ({'Content-Length': None}, {'Content-Length': str(len(FAKE_IMAGE) + 1)}):
            with self.assertRaises(primitive.Refusal):
                self.fetch(FakeResponse(headers=headers))
            self.assertFalse(os.path.lexists(self.dest))

    def test_encoding_chunking_and_status_refuse(self):
        cases = (FakeResponse(headers={'Content-Encoding': 'gzip'}), FakeResponse(headers={'Transfer-Encoding': 'chunked'}),
                 FakeResponse(status=500), FakeResponse(status=404))
        for response in cases:
            with self.assertRaises(primitive.Refusal):
                self.fetch(response)
            self.assertFalse(os.path.lexists(self.dest))

    def test_over_cap_and_short_bodies_refuse_and_retain_the_partial_file(self):
        with self.assertRaises(primitive.Refusal):
            self.fetch(FakeResponse(body=FAKE_IMAGE + b'x', headers={'Content-Length': str(len(FAKE_IMAGE))}))
        self.assertTrue(os.path.exists(self.dest))  # retained, not removed; never a verified image
        os.remove(self.dest)
        with self.assertRaises(primitive.Refusal):
            self.fetch(FakeResponse(body=FAKE_IMAGE[:-1], headers={'Content-Length': str(len(FAKE_IMAGE))}))
        self.assertEqual(Path(self.dest).read_bytes(), FAKE_IMAGE[:-1])

    def test_a_same_length_wrong_body_refuses_on_sha512(self):
        with self.assertRaises(primitive.Refusal):
            self.fetch(FakeResponse(body=b'z' * len(FAKE_IMAGE)))
        self.assertTrue(os.path.exists(self.dest))

    def test_an_existing_name_or_symlink_is_a_collision_and_is_never_adopted(self):
        Path(self.dest).write_bytes(b'foreign')
        with self.assertRaises(ex.Collision):
            self.fetch(FakeResponse())
        self.assertEqual(Path(self.dest).read_bytes(), b'foreign')
        os.remove(self.dest)
        target = self.image_dir + '/elsewhere.raw.gz'
        os.symlink(target, self.dest)
        with self.assertRaises(ex.Collision):
            self.fetch(FakeResponse())
        self.assertFalse(os.path.lexists(target))

    def test_a_lost_live_counter_stops_the_transfer_with_the_partial_retained(self):
        calls = []

        def checkpoint():
            calls.append(1)
            raise primitive.Refusal('live counter lost')
        with self.assertRaises(primitive.Refusal):
            ex.prefetch_pinned_artifact(FakeResponse(), self.dest, len(FAKE_IMAGE), FAKE_PIN['sha512'], checkpoint=checkpoint)
        self.assertEqual(calls, [1])
        self.assertTrue(os.path.exists(self.dest))


class RedirectPolicyTests(Fixture):
    """The real opener, with http.client replaced by a scripted connection: redirects stay on allowlisted HTTPS hosts."""

    def open(self, script, seen):
        with mock.patch('http.client.HTTPSConnection', scripted_https(script, seen)):
            return REAL_OPEN(prep.COLIMA_IMAGE_URL)

    def test_redirects_follow_only_allowlisted_https_hosts(self):
        seen = []
        conn, response = self.open([FakeResponse(b'', 302, {'Location': 'https://release-assets.githubusercontent.com/a?x=1'}),
                                    FakeResponse()], seen)
        self.assertEqual(response.status, 200)
        self.assertEqual(seen[0], 'github.com')
        self.assertEqual(seen[2], 'release-assets.githubusercontent.com')
        self.assertEqual(seen[3], '/a?x=1')

    def test_a_redirect_off_the_allowlist_or_off_https_refuses_before_connecting(self):
        for location in ('https://evil.example/x', 'http://release-assets.githubusercontent.com/x'):
            seen = []
            with self.assertRaises(primitive.Refusal):
                self.open([FakeResponse(b'', 302, {'Location': location})], seen)
            self.assertNotIn('evil.example', seen)
            self.assertEqual(seen.count('github.com'), 1)  # the first hop only

    def test_redirect_loops_and_missing_locations_refuse(self):
        with self.assertRaises(primitive.Refusal):
            self.open([FakeResponse(b'', 302, {'Location': 'https://github.com/loop'})] * 4, [])
        with self.assertRaises(primitive.Refusal):
            self.open([FakeResponse(b'', 302, {})], [])

    def test_the_first_url_must_be_allowlisted(self):
        seen = []
        with mock.patch('http.client.HTTPSConnection', scripted_https([], seen)):
            with self.assertRaises(primitive.Refusal):
                REAL_OPEN('https://example.com/x')
        self.assertEqual(seen, [])


class C1ProofEntryTests(Fixture):
    """C1 is admitted in the proof entry as a plain request. Its window needs its original C0 receipt, and its
    prospective data binds the same pinned inputs the executor plans. C2-C6 keep plain-request-only windows."""

    def test_c1_prospective_data_binds_the_pinned_inputs(self):
        self.assertIn('C1', proof.PROOF_STAGES)
        data = proof.prospective('C1', self.bindings)
        self.assertEqual(data['stage'], 'C1')
        self.assertEqual(data['inputs_sha256'], ex.canon_sha(ex.stable_input_manifest()[0]))
        self.assertEqual(data['bindings_sha256'], ex.canon_sha(self.bindings))

    def test_c1_window_names_its_c0_and_unreviewed_windows_still_refuse(self):
        options = dict(activation_id='c1-window', duration_seconds=600, evidence_root=self.evidence)
        with self.assertRaisesRegex(ex.Refusal, 'unsupported window options'):  # a C1 window names its C0 receipt
            proof.window_recipe('C1', self.bindings, options)
        with self.assertRaisesRegex(ex.Refusal, 'no window recipe'):
            proof.window_recipe('C4', self.bindings, options)
        with self.assertRaisesRegex(ex.Refusal, 'supports only C0, D0, D1, C1, C2, C3, C4, C5, C6, D2, D3 or D4'):
            proof.prospective('C9', self.bindings)


class C1WindowTests(Fixture):
    """C1 through the proof entry's launch-time window: the request names the original executor C0 receipt it
    consumes, that genuine closed chain is re-verified at the plan and at launch, and the token's first-use
    cache, image, Docker-config and profile/VM state must be absent before the claim. The plain C1 request
    keeps its pre-created absolute-time activation route. Fixed fake children only: no Colima, no network."""

    def use_historical(self):
        """A fixture 688 C0 record and tranche summary for the genuine C0's supervised helpers only. The
        window's chain checks never read it: the prior is the executor C0 receipt, not the fixture."""
        if getattr(self, 'historical', None) is not None:
            return self.historical
        hist = self.tmp + '/historical'
        os.mkdir(hist, 0o700)
        tranche = hist + '/summary.json'
        Path(tranche).write_bytes(b'{"tranche": "fixture"}\n')
        tranche_sha = ex.sha_file(tranche)
        record = dict(status='passed: fixture', token=ex.RECORDED_TOKEN, zero_share_present=True, roots_safe=True,
                      failed_at=None,
                      tools={'colima': {'sha256': ex.sha_file(self.colima)}, 'docker': {'sha256': ex.sha_file(self.docker)}},
                      tranche_summary_sha256=tranche_sha, roots_final={})
        data = (json.dumps(record, sort_keys=True, indent=2) + '\n').encode()
        record_path = hist + '/C0.stage.json'
        Path(record_path).write_bytes(data)
        for patcher in (mock.patch.dict(ex.HISTORICAL_C0, dict(path=record_path, sha256=sha_bytes(data),
                                                              tranche_path=tranche, tranche_sha256=tranche_sha)),
                        mock.patch.object(ex, 'LINUX_ARTIFACT_DIR', hist)):
            patcher.start()
            self.addCleanup(patcher.stop)
        self.historical = dict(path=record_path, sha256=sha_bytes(data))
        return self.historical

    def run_c0(self, token, activation_id):
        """A genuine C0 through the real fixed helpers over the fixture record: the window consumes its receipt,
        never the fixture itself."""
        self.use_historical()
        bindings = self.make_bindings(token)
        calls = []
        with mock.patch.dict(ex.TEST_SEAM, {'admit_unadmitted': False}), \
             mock.patch.object(ex, 'HISTORICAL_TOOL_HASHES',
                               {'colima': ex.sha_file(self.colima), 'docker': ex.sha_file(self.docker)}):
            receipt = self.execute('C0', self.routed({'c0-colima-start-help': emit('help')}, calls),
                                   prior_ref=None, activation_id=activation_id, bindings=bindings,
                                   monitor_factory=FixedHelperTests.quiet_monitor)
        self.assertEqual(receipt['closure'], 'closed', receipt)
        self.assertEqual([c[0] for c in calls], ['c0-colima-start-help'])
        return bindings, receipt, self.ref('C0', activation_id)

    def request(self, bindings, prior, activation_id='c1-window-0001', **extra):
        options = dict(activation_id=activation_id, duration_seconds=900, evidence_root=self.evidence)
        if prior is not None:
            options['prior_receipt'] = prior
        options.update(extra)
        return proof.window_recipe('C1', bindings, options)

    def freeze(self, request, name='c1-window-request.json'):
        path = self.tmp + '/' + name
        Path(path).write_text(json.dumps(request, sort_keys=True))
        os.chmod(path, 0o400)
        return path, ex.sha_file(path)

    def run_entry(self, path, digest, calls):
        routes = {'c1-colima-start': emit('started')}
        with mock.patch.object(ex, 'default_transport', self.routed(routes, calls)), \
             mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
             contextlib.redirect_stdout(io.StringIO()) as out:
            code = proof.main(['proof', path, digest])
        return code, json.loads(out.getvalue())

    @staticmethod
    def first_use_paths(bindings):
        return sorted({bindings['colima_cache_home'], bindings['colima_docker_config'],
                       bindings['colima_cache_home'] + '/' + ex.COLIMA_IMAGE_DIR,
                       bindings['colima_home'] + '/' + prep.PROFILE,
                       bindings['colima_home'] + '/_lima'})

    def test_c1_window_runs_through_the_entry_and_its_go_names_the_c0_receipt(self):
        bindings, _, c0_ref = self.run_c0('f0f0f0e1', 'c1-window-c0')
        wrong = {name: dict(rec, device=0, inode=0) for name, rec in ex.RECORDED_ROOTS.items()}
        request = self.request(bindings, c0_ref)
        self.assertEqual(request['prior_receipt'], c0_ref)
        self.assertEqual(request['duration_seconds'], 900)
        self.assertEqual(request['absent_paths'], self.first_use_paths(bindings))
        for path in request['absent_paths']:
            self.assertFalse(os.path.lexists(path))
        path, digest = self.freeze(request)
        calls = []
        with mock.patch.object(ex, 'RECORDED_ROOTS', wrong):  # consulted, these records would refuse the run
            code, receipt = self.run_entry(path, digest, calls)
        self.assertEqual(code, 0)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['c1-colima-start'])
        self.assertEqual(receipt['prior']['stage'], 'C0')
        self.assertEqual(receipt['token'], bindings['token'])
        go = json.loads(Path(request['activation_path']).read_text())
        self.assertEqual(go['prior_receipt'], c0_ref)
        run_bindings = {k: v for k, v in bindings.items() if k not in ex.NOT_SUPPLIED}
        run_bindings.update(ex.guest_source_bindings())
        self.assert_dispatch('C1', calls, run_bindings)

    def test_delayed_c1_launch_starts_the_window_at_launch_and_is_one_use(self):
        bindings, _, c0_ref = self.run_c0('f0f0f0e2', 'c1-delayed-c0')
        request = self.request(bindings, c0_ref, activation_id='c1-delayed-0001')
        path, digest = self.freeze(request, 'c1-delayed-request.json')
        prepared = datetime.datetime.now(datetime.timezone.utc)
        before = prepared + datetime.timedelta(seconds=1800)
        real_datetime = datetime.datetime

        class LaunchClock(real_datetime):
            @classmethod
            def now(cls, tz=None):
                return before

        calls = []
        with mock.patch.object(proof.datetime, 'datetime', LaunchClock), \
             mock.patch.object(ex, 'default_transport', self.routed({'c1-colima-start': emit('started')}, calls)), \
             mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(proof.main(['proof', path, digest]), 0)
            with self.assertRaises(ex.Refusal):  # one use: the spent claim replays nothing
                proof.main(['proof', path, digest])
        self.assertEqual([c[0] for c in calls], ['c1-colima-start'])
        go = json.loads(Path(request['activation_path']).read_text())
        issued = datetime.datetime.fromisoformat(go['issued_utc'])
        expires = datetime.datetime.fromisoformat(go['expires_utc'])
        self.assertGreater((issued - prepared).total_seconds(), 1200)  # no prelaunch clock consumed
        self.assertGreaterEqual(issued, before)
        self.assertEqual((expires - issued).total_seconds(), 900)
        retained = json.loads(Path(request['evidence_path']).read_text())
        self.assertEqual(retained['request_sha256'], digest)
        self.assertEqual(retained['activation_sha256'], sha_bytes(Path(request['activation_path']).read_bytes()))
        self.assertEqual(json.loads(Path(request['claim_path']).read_text())['go_sha256'], digest)

    def test_c1_window_without_a_c0_reference_or_with_a_wrong_hash_refuses(self):
        bindings, _, c0_ref = self.run_c0('f0f0f0e3', 'c1-noref-c0')
        with self.assertRaises(ex.Refusal):
            proof.window_recipe('C1', bindings, dict(activation_id='c1-noref-0001', duration_seconds=900,
                                                     evidence_root=self.evidence))
        with self.assertRaises(ex.Refusal):
            self.request(bindings, dict(c0_ref, sha256='0' * 64), activation_id='c1-badhash-0001')
        with self.assertRaises(ex.Refusal):  # a D0 receipt is not the C0 reference C1 names
            self.request(bindings, dict(c0_ref, stage='D0'), activation_id='c1-wrongstage-0001')

    def test_c1_window_refuses_a_fixture_or_simulated_predecessor(self):
        bindings, c0, c0_ref = self.run_c0('f0f0f0e4', 'c1-fixture-c0')
        hist = self.historical
        with self.assertRaises(ex.Refusal):  # the pinned fixture record is not an executor receipt
            self.request(bindings, dict(stage='C0', path=hist['path'], sha256=hist['sha256']),
                         activation_id='c1-fixture-0001')
        with self.assertRaises(ex.Refusal):  # a window ref never carries the historical binding flag
            self.request(bindings, dict(c0_ref, historical=True), activation_id='c1-histflag-0001')
        body = json.loads(Path(c0['stage_root'], 'stage-receipt.json').read_text())
        doctored = dict(body, closure_simulated=True)
        raw = (json.dumps(doctored, sort_keys=True, indent=2) + '\n').encode()
        Path(c0['stage_root'], 'stage-receipt.json').write_bytes(raw)
        with self.assertRaises(ex.Refusal):  # a simulated closure is never a chain receipt
            self.request(bindings, dict(c0_ref, sha256=sha_bytes(raw)), activation_id='c1-simulated-0001')

    def test_c1_window_refuses_a_wrong_token_predecessor(self):
        _, _, other_ref = self.run_c0('f0f0f0e5', 'c1-wrongtoken-c0')
        with self.assertRaisesRegex(ex.Refusal, 'another backend chain'):
            self.request(self.bindings, other_ref, activation_id='c1-wrongtoken-0001')

    def test_c1_window_refuses_a_changed_root_before_the_claim(self):
        bindings, _, c0_ref = self.run_c0('f0f0f0e6', 'c1-swap-c0')
        path, digest = self.freeze(self.request(bindings, c0_ref, activation_id='c1-swap-0001'))
        os.rmdir(bindings['colima_home'])
        os.mkdir(bindings['colima_home'], 0o700)
        calls = []
        with self.assertRaisesRegex(ex.Refusal, 'root identity changed'):
            self.run_entry(path, digest, calls)
        self.assertEqual(calls, [])
        self.assertFalse(os.path.lexists(self.tmp + '/claims/c1-swap-0001.claim'))
        self.assertFalse(os.path.lexists(self.evidence + '/C1-c1-swap-0001'))

    def test_c1_window_refuses_preexisting_first_use_state_before_the_claim(self):
        bindings, _, c0_ref = self.run_c0('f0f0f0e7', 'c1-present-c0')
        for index, present in enumerate((bindings['colima_cache_home'],
                                         bindings['colima_home'] + '/' + prep.PROFILE)):
            activation = 'c1-present-%04d' % index
            os.mkdir(present, 0o700)
            request = self.request(bindings, c0_ref, activation_id=activation)
            frozen, digest = self.freeze(request, activation + '.json')
            with mock.patch.object(ex, '_execute_stage') as effect, \
                    self.assertRaisesRegex(ex.Refusal, 'named destination is not absent'):
                self.run_entry(frozen, digest, [])
            effect.assert_not_called()
            self.assertFalse(os.path.lexists(self.tmp + '/claims/' + activation + '.claim'))
            os.rmdir(present)

    def test_c1_window_duration_boundaries(self):
        bindings, _, c0_ref = self.run_c0('f0f0f0e8', 'c1-bounds-c0')
        options = dict(activation_id='c1-bounds-0001', evidence_root=self.evidence)
        for duration in (61, 300, 900):
            request = proof.window_recipe('C1', bindings, dict(options, duration_seconds=duration,
                                                               prior_receipt=c0_ref))
            self.assertEqual(request['duration_seconds'], duration)
        for duration in (0, 60, 901, 1200, 1201, True):
            with self.subTest(duration=duration), self.assertRaises(ex.Refusal):
                proof.window_recipe('C1', bindings, dict(options, duration_seconds=duration, prior_receipt=c0_ref))

    def test_c1_window_timeout_retains_and_never_replays(self):
        bindings, _, c0_ref = self.run_c0('f0f0f0e9', 'c1-timeout-c0')
        request = self.request(bindings, c0_ref, activation_id='c1-timeout-0001')
        path, digest = self.freeze(request, 'c1-timeout-request.json')
        calls = []
        transport = self.routed({'c1-colima-start': 'import time; time.sleep(30)'}, calls)

        def short(**kw):
            return transport(**dict(kw, timeout=min(kw['timeout'], 0.1)))

        with mock.patch.object(ex, 'default_transport', short), \
             mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(proof.main(['proof', path, digest]), 1)
            with self.assertRaises(ex.Refusal):  # the spent window never replays
                proof.main(['proof', path, digest])
        receipt = json.loads(Path(self.evidence, 'C1-c1-timeout-0001', 'stage-receipt.json').read_text())
        self.assertEqual(receipt['steps'][-1]['closure'], 'unknown_retained')
        self.assertIsNotNone(receipt['steps'][-1]['waited_exit'])
        self.assertEqual([c[0] for c in calls], ['c1-colima-start'])
        self.assertTrue(os.path.isdir(bindings['colima_cache_home'] + '/' + ex.COLIMA_IMAGE_DIR))
        self.assertTrue(Path(request['evidence_path']).exists())

    def test_c1_plain_request_still_runs_with_a_precreated_activation(self):
        bindings, _, c0_ref = self.run_c0('f0f0f0ea', 'c1-plain-c0')
        inputs = ex.stable_input_manifest()[0]
        path, sha = self.activation('C1', c0_ref, 'c1-plain-0001', bindings=bindings, inputs=inputs)
        request = {'stage': 'C1', 'activation_path': path, 'activation_sha256': sha, 'bindings': bindings,
                   'evidence_root': self.evidence, 'inputs': inputs, 'inputs_sha256': ex.canon_sha(inputs),
                   'manifest_sha256': ex.sha_file(ex.MANIFEST_PATH), 'source_sha256': ex.sha_file(ex.__file__)}
        raw = self.tmp + '/c1-plain-request.json'
        Path(raw).write_text(json.dumps(request, sort_keys=True))
        os.chmod(raw, 0o400)
        calls = []
        with mock.patch.object(ex, 'default_transport', self.routed({'c1-colima-start': emit('started')}, calls)), \
             mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
             contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(proof.main(['proof', raw, ex.sha_file(raw)]), 0)
        receipt = json.loads(out.getvalue())
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['c1-colima-start'])
        self.assertEqual(receipt['prior']['stage'], 'C0')


class C2C3ProofEntryTests(Fixture):
    """C2 and C3 are admitted in the proof entry as plain requests. They have no window recipe."""

    def test_c2_and_c3_prospective_data_bind_the_pinned_inputs(self):
        for stage in ('C2', 'C3'):
            with self.subTest(stage=stage):
                self.assertIn(stage, proof.PROOF_STAGES)
                data = proof.prospective(stage, self.bindings)
                self.assertEqual(data['stage'], stage)
                self.assertEqual(data['inputs_sha256'], ex.canon_sha(ex.stable_input_manifest()[0]))

    def test_c2_and_c3_have_no_window_recipe(self):
        options = dict(activation_id='c2-window', duration_seconds=300, evidence_root=self.evidence)
        for stage in ('C2', 'C3'):
            with self.subTest(stage=stage), self.assertRaisesRegex(ex.Refusal, 'no window recipe'):
                proof.window_recipe(stage, self.bindings, options)


class GuestClosureNegativeTests(Fixture):
    """Production establish_guest_closure refusals and Unknowns. Injected envelopes, no simulation."""

    def setUp(self):
        super().setUp()
        patcher = mock.patch.object(ex, 'RECORDED_TOKEN', TOKEN)
        patcher.start()
        self.addCleanup(patcher.stop)
        for rel in ('/c', '/s'):
            path = self.bindings['control'] + rel
            if not os.path.isdir(path):
                os.mkdir(path, 0o700)
        roots = {}
        for name, rel in (('control', ''), ('colima_home', '/c'), ('qualification_root', '/s')):
            path = self.bindings['control'] + rel
            info = os.lstat(path)
            roots[name] = dict(path=path, device=info.st_dev, inode=info.st_ino, entries=len(os.listdir(path)))
        patcher = mock.patch.object(ex, 'RECORDED_ROOTS', roots)
        patcher.start()
        self.addCleanup(patcher.stop)

    def _run_named(self, routes, activation_id):
        calls = []
        stub = {'receipts': [], 'engine_id': None}
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute('C2', self.routed(routes, calls), prior_ref=None, activation_id=activation_id)
        return receipt, calls

    def status_and(self, **routes):
        out = {'c2-colima-status': emit('{"status":"running"}')}
        out.update(routes)
        return out

    def test_wrong_nonce_refuses_before_the_probe(self):
        with self.assertRaisesRegex(ex.Refusal, 'guest nonce mismatch'):
            self._run_named(self.status_and( **{'c2-route-echo': closed_guest(prep.ROUTE_ECHO, nonce='00' * 32)}),
                            'act-c2-bad-nonce')

    def test_wrong_helper_source_hash_refuses(self):
        with self.assertRaisesRegex(ex.Refusal, 'guest helper source hash mismatch'):
            self._run_named(self.status_and(**{'c2-route-echo': closed_guest(prep.ROUTE_ECHO, source_sha256='00' * 32)}),
                            'act-c2-bad-source')

    def test_boot_drift_between_echo_and_probe_refuses(self):
        parent = self.guest_parent()
        with self.assertRaisesRegex(ex.Refusal, 'guest boot identity drift'):
            self._run_named(self.status_and(**{
                'c2-route-echo': closed_guest(prep.ROUTE_ECHO, boot_id='boot-1'),
                'c2-guest-probe': closed_guest(json.dumps({'parent': parent}), boot_id='boot-2'),
            }), 'act-c2-boot-drift')
        receipt = json.loads(Path(self.stage_dir('C2', 'act-c2-boot-drift') + '/stage-receipt.json').read_text())
        self.assertEqual(receipt['closure'], 'refused')
        self.assertEqual(receipt['guest_closure'], 'unknown')

    def test_duplicate_nonce_refuses_on_the_second_guest_step(self):
        parent = self.guest_parent()
        routes = self.status_and(**{'c2-route-echo': closed_guest(prep.ROUTE_ECHO),
                                    'c2-guest-probe': closed_guest(json.dumps({'parent': parent}))})
        with mock.patch.object(os, 'urandom', return_value=b'\x11' * 32):
            with self.assertRaisesRegex(ex.Refusal, 'guest nonce collision'):
                self._run_named(routes, 'act-c2-dup-nonce')

    def test_leftover_group_stays_unknown_and_does_not_dispatch_the_probe(self):
        receipt, calls = self._run_named(self.status_and(**{'c2-route-echo': closed_guest(prep.ROUTE_ECHO, group_empty=False)}),
                                         'act-c2-group')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertNotIn('c2-guest-probe', [c[0] for c in calls])

    def test_unreaped_and_setsid_child_stay_unknown(self):
        receipt, _ = self._run_named(self.status_and(**{'c2-route-echo': closed_guest(prep.ROUTE_ECHO, unreaped=True)}),
                                     'act-c2-unreaped')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        receipt, _ = self._run_named(self.status_and(**{'c2-route-echo': closed_guest(prep.ROUTE_ECHO, child_session_id=99)}),
                                     'act-c2-setsid')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c2-route-echo']['guest_closure'], 'unknown')

    def test_truncated_envelope_refuses_before_the_probe(self):
        with self.assertRaisesRegex(ex.Refusal, 'route echo mismatch'):
            self._run_named(self.status_and(**{'c2-route-echo': emit('EFFIGY-COLIMA-GUEST-1\nHEADER\n')}),
                            'act-c2-trunc')

    def test_invalid_pid_refuses_before_the_probe(self):
        with self.assertRaisesRegex(ex.Refusal, 'guest process identity invalid'):
            self._run_named(self.status_and(**{'c2-route-echo': closed_guest(prep.ROUTE_ECHO, pid=0)}),
                            'act-c2-bad-pid')

    def test_missing_echo_stdout_stays_unknown_and_does_not_dispatch_the_probe(self):
        calls = []

        def after(kw):
            if kw['step'] == 'c2-route-echo':
                os.unlink(kw['root'] + '/stdout')

        stub = {'receipts': [], 'engine_id': None}
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute(
                'C2', self.routed(self.status_and(**{'c2-route-echo': closed_guest(prep.ROUTE_ECHO)}),
                                  calls, after=after),
                prior_ref=None, activation_id='act-c2-lost-stdout')
        self.assertEqual(receipt['closure'], 'guest_unknown')
        self.assertNotIn('c2-guest-probe', [c[0] for c in calls])
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c2-compare-route-echo']['closure'], 'not_dispatched')

    def test_exact_zero_output_cap_stops_the_echo(self):
        """Shared primitive accepts cap 0 (retain no bytes, disposition output-cap). The echo cannot
        close, so the probe is never reached."""
        calls = []
        inner = self.routed(self.status_and(**{'c2-route-echo': closed_guest(prep.ROUTE_ECHO)}), calls)

        def transport(**kw):
            if kw['step'] == 'c2-route-echo':
                kw = dict(kw, output_cap=0)
            return inner(**kw)

        stub = {'receipts': [], 'engine_id': None}
        with mock.patch.object(ex, 'verify_chain', lambda *a, **k: stub):
            receipt = self.execute('C2', transport, prior_ref=None, activation_id='act-c2-zerocap')
        self.assertEqual(receipt['closure'], 'stopped')
        self.assertNotIn('c2-guest-probe', [c[0] for c in calls])
        by_id = {x['id']: x for x in receipt['steps']}
        self.assertEqual(by_id['c2-route-echo']['closure'], 'unknown_retained')
        self.assertEqual(by_id['c2-route-echo']['disposition'], 'output-cap')
        self.assertNotIn('c2-guest-probe', by_id)

class ColimaImagePinTests(unittest.TestCase):
    def test_the_real_manifest_pins_the_reviewed_image_and_its_cap(self):
        manifest = ex.load_manifest()
        pin = REAL_PIN(manifest)
        self.assertEqual(pin, dict(url=prep.COLIMA_IMAGE_URL, sha512=prep.COLIMA_IMAGE_SHA512, bytes=446992920,
                                   name=prep.COLIMA_IMAGE_NAME))
        c1 = ex.stage_of(manifest, 'C1')
        self.assertEqual(c1['download_cap_bytes'], pin['bytes'])
        self.assertEqual([s['id'] for s in c1['steps']],
                         ['c1-make-owned-dirs', 'c1-prefetch-colima-image', 'c1-colima-start'])
        self.assertIn('prefetch_colima_image', ex.HANDLERS)

    def test_a_changed_size_or_url_refuses_before_any_fetch(self):
        manifest = ex.load_manifest()
        for key, value in (('bytes', 446992921), ('url', prep.COLIMA_IMAGE_URL + '?x')):
            changed = json.loads(json.dumps(manifest))
            changed['binding_inputs']['colima_image'][key] = value
            with self.assertRaises(primitive.Refusal):
                REAL_PIN(changed)

    def test_only_the_reviewed_fetches_are_admitted_from_the_download_stages(self):
        manifest = ex.load_manifest()
        for stage_id in ('C1', 'C4', 'C5', 'C6', 'D2'):
            self.assertNotIn(stage_id, ex.UNADMITTED)
            self.assertEqual(ex.stage_admission(ex.stage_of(manifest, stage_id)), [], stage_id)
        self.assertEqual(ex.UNADMITTED, {})
        # C4 fetches the same pinned OCI/deb set D2 fetches, with the same cap and the same official sources.
        self.assertEqual(ex.stage_of(manifest, 'C4')['download_cap_bytes'],
                         manifest['binding_inputs']['runtime_image']['bytes'])
        self.assertEqual(ex.stage_of(manifest, 'C4')['network_sources'],
                         ex.stage_of(manifest, 'D2')['network_sources'])


class RuntimeImagePinTests(unittest.TestCase):
    def test_the_real_manifest_pins_the_reviewed_oci_and_deb_set(self):
        manifest = ex.load_manifest()
        pin = REAL_RUNTIME_PIN(manifest)
        self.assertEqual(pin['bytes'], runtime_inputs.INPUT_BYTES)
        self.assertEqual(pin['index']['digest'], runtime_inputs.INDEX['digest'])
        self.assertEqual(pin['layer']['bytes'], runtime_inputs.LAYER['bytes'])
        self.assertEqual(len(pin['debs']), 22)
        d2 = ex.stage_of(manifest, 'D2')
        self.assertEqual(d2['download_cap_bytes'], pin['bytes'])
        self.assertEqual(d2['network_sources'], list(runtime_inputs.NETWORK_SOURCES))
        self.assertIn('prefetch_runtime_inputs', ex.HANDLERS)
        self.assertEqual([s['id'] for s in d2['steps'] if s['kind'] == 'helper'],
                         ['d2-link-buildx-plugin', 'd2-check-buildx-metadata', 'd2-make-input-dir', 'd2-make-context-dir',
                          'd2-prefetch-runtime-inputs', 'd2-host-stage', 'd2-check-buildx-custody', 'd2-read-iidfile',
                          'd2-check-image-identity'])

    def test_a_changed_size_or_digest_refuses_before_any_fetch(self):
        manifest = ex.load_manifest()
        for key, value in (('bytes', runtime_inputs.INPUT_BYTES + 1),
                           ('layer_sha256', '0' * 64), ('deb_count', 21)):
            changed = json.loads(json.dumps(manifest))
            changed['binding_inputs']['runtime_image'][key] = value
            with self.assertRaises(primitive.Refusal):
                REAL_RUNTIME_PIN(changed)




class C5HelperFormTests(unittest.TestCase):
    """The three C5 remote forms, as the fixed helper and the production dispatch check them. A form that is not
    exactly reviewed refuses before any guest child could run."""

    CTX = '/home/u/effigy-531-image-' + '1' * 32
    IMG = 'sha256:' + 'ab' * 32

    def run_argv(self, **changes):
        argv = helper.check_run_command(self.CTX, 'effigy-4f6a-check-0a1b2c3d', self.IMG, prep.CHECK_SOURCE)
        for index, value in changes.items():
            argv[int(index)] = value
        return argv

    def test_the_reviewed_forms_are_accepted_by_the_helper_and_the_production_builder(self):
        argv = self.run_argv()
        self.assertEqual(helper.checked_remote('disposable-check', {'argv': argv}), argv)
        self.assertEqual(argv, prep.guest_check_command(self.CTX, '0a1b2c3d', self.IMG))
        self.assertEqual(helper.checked_remote('read-check-cid', {'argv': ['sudo', '-n', 'cat', self.CTX + '.check.cid']}),
                         ['sudo', '-n', 'cat', self.CTX + '.check.cid'])
        self.assertEqual(helper.checked_remote('container-list', {'argv': list(helper.CONTAINER_LIST)}),
                         list(helper.CONTAINER_LIST))

    def test_a_changed_check_source_image_name_cidfile_or_flag_refuses(self):
        base = helper.check_run_command(self.CTX, 'effigy-4f6a-check-0a1b2c3d', self.IMG, prep.CHECK_SOURCE)
        cases = [
            base[:-1] + [prep.CHECK_SOURCE + '\n'],  # the pinned source, with one byte added
            base[:-5] + ['sha256:' + 'AB' * 32] + base[-4:],  # an image ID that is not lowercase hex
            base[:-6] + ['effigy-4f6a-check-XYZ'] + base[-5:],  # a name outside the reviewed shape
            base[:base.index('--cidfile') + 1] + ['/etc/x.check.cid'] + base[base.index('--cidfile') + 2:],
            base[:base.index('--network') + 1] + ['host'] + base[base.index('--network') + 2:],
            base + ['--privileged'],
        ]
        for argv in cases:
            with self.subTest(argv=argv[:12]), self.assertRaises(helper.Refusal):
                helper.checked_remote('disposable-check', {'argv': argv})

    def test_a_read_or_listing_with_any_extra_argument_refuses(self):
        with self.assertRaises(helper.Refusal):
            helper.checked_remote('read-check-cid', {'argv': ['sudo', '-n', 'cat', '/etc/passwd']})
        with self.assertRaises(helper.Refusal):
            helper.checked_remote('container-list', {'argv': list(helper.CONTAINER_LIST) + ['--filter', 'name=x']})

    def test_the_production_check_refuses_any_tail_that_is_not_reviewed(self):
        bindings = dict(token='0a1b2c3d', image_id=self.IMG, guest_context=self.CTX,
                        guest_check_cid=self.CTX + '.check.cid', check_source=prep.CHECK_SOURCE)
        self.assertTrue(prep.check_guest_remote_command(prep.guest_check_command(self.CTX, '0a1b2c3d', self.IMG), bindings))
        self.assertTrue(prep.check_guest_remote_command(list(helper.CONTAINER_LIST), bindings))
        for tail in (['sudo', '-n', 'cat', '/etc/shadow'], prep.guest_check_command(self.CTX, '0a1b2c3d', 'sha256:' + 'cd' * 32),
                     prep.guest_check_command(self.CTX, '0a1b2c3d', self.IMG)[:-1] + ['x']):
            with self.subTest(tail=tail[:4]), self.assertRaises(ValueError):
                prep.check_guest_remote_command(tail, bindings)

    def test_the_check_pin_is_the_reviewed_source_and_the_helper_agrees_with_it(self):
        self.assertEqual(hashlib.sha256(prep.CHECK_SOURCE.encode()).hexdigest(), helper.CHECK_SOURCE_SHA256)
        self.assertEqual(hashlib.sha256(prep.CHECK_SOURCE.encode()).hexdigest(),
                         ex.load_manifest()['binding_inputs']['check_source_sha256'])

    def test_the_manifest_c5_steps_fit_their_stage_budget_and_keep_their_guest_clocks(self):
        stage = ex.stage_of(ex.load_manifest(), 'C5')
        total = sum(step['timeout_seconds'] for step in stage['steps'])
        self.assertLessEqual(total, stage['seconds'] - stage['reserve_seconds'])
        by_id = {step['id']: step for step in stage['steps']}
        run = by_id['c5-disposable-check']
        self.assertEqual(run['guest_deadline'], {'kill_grace_seconds': 5, 'seconds': 200})
        self.assertEqual(run['timeout_seconds'], 240)
        self.assertEqual(run['argv'][run['argv'].index('--') + 1:run['argv'].index('--') + 5], ['timeout', '-k', '5', '200'])
        self.assertNotIn('--rm', run['argv'])
        for step in stage['steps']:  # every guest guard fits its own host allowance with the receipt margin
            guard = ex.guest_guard(step['argv']) if step['kind'] == 'exec' else None
            if guard is not None:
                self.assertGreaterEqual(step['timeout_seconds'], sum(guard) + 1, step['id'])

class C5C6ProofEntryTests(Fixture):
    """C5 and C6 are admitted proof entries as plain requests, with no window: they consume the C4 and C5 receipts."""

    def test_c5_and_c6_prospective_data_bind_the_pinned_inputs(self):
        for stage in ('C5', 'C6'):
            with self.subTest(stage=stage):
                self.assertIn(stage, proof.PROOF_STAGES)
                data = proof.prospective(stage, self.bindings)
                self.assertEqual(data['stage'], stage)
                self.assertEqual(data['inputs_sha256'], ex.canon_sha(ex.stable_input_manifest()[0]))

    def test_c5_and_c6_have_no_window_recipe(self):
        options = dict(activation_id='c5-window', duration_seconds=300, evidence_root=self.evidence)
        for stage in ('C5', 'C6'):
            with self.subTest(stage=stage), self.assertRaisesRegex(ex.Refusal, 'no window recipe'):
                proof.window_recipe(stage, self.bindings, options)


class D2BuildxRouteTests(Fixture):
    """D2's Buildx route and its fresh occurrence, through the stage loop. The Desktop bundle, plist and plugin are
    fixtures pinned to their own bytes. Each refusal happens before the fetch: no input, context or iidfile is created,
    and the legacy builder is never reached."""

    def d1_ref(self):
        _, ref = self.d1(self.d0())
        return ref

    def test_the_link_is_one_owned_symlink_to_the_pinned_plugin_before_the_fetch(self):
        receipt, _ = self.d2(self.d1_ref(), activation_id='act-d2-link')
        self.assertEqual(receipt['closure'], 'closed')
        link = self.d2_out('act-d2-link')['desktop_config'] + '/cli-plugins/docker-buildx'
        self.assertEqual(os.readlink(link), ex.DESKTOP_BUILDX)
        ids = [s['id'] for s in receipt['steps']]
        self.assertLess(ids.index('d2-link-buildx-plugin'), ids.index('d2-prefetch-runtime-inputs'))
        self.assertLess(ids.index('d2-make-context-dir'), ids.index('d2-host-stage'))
        self.assertTrue(os.path.isdir(self.d2_out('act-d2-link')['desktop_input_dir'] + '/oci'))

    def test_a_changed_plugin_refuses_before_any_fetch_and_creates_nothing(self):
        calls = []
        with mock.patch.object(ex, 'DESKTOP_BUILDX_SHA256', '0' * 64), \
                self.assertRaisesRegex(ex.Refusal, 'pinned sha256'):
            self.d2(self.d1_ref(), activation_id='act-d2-badsha', calls=calls)
        self.assertEqual([c[0] for c in calls], ['d2-engine-info'])
        self.assertFalse(os.path.lexists(self.d2_out('act-d2-badsha')['desktop_config'] + '/cli-plugins'))
        self.assertFalse(os.path.lexists(self.d2_out('act-d2-badsha')['desktop_input_dir']))

    def test_a_changed_bundle_version_refuses_before_any_fetch(self):
        Path(ex.DESKTOP_APP_PLIST).write_bytes(plistlib.dumps({'CFBundleShortVersionString': '4.91.0'}))
        calls = []
        with self.assertRaisesRegex(ex.Refusal, 'bundle version'):
            self.d2(self.d1_ref(), activation_id='act-d2-version', calls=calls)
        self.assertEqual([c[0] for c in calls], ['d2-engine-info'])

    def test_a_missing_plugin_is_the_legacy_fallback_shape_and_refuses_before_the_build(self):
        calls = []
        with mock.patch.object(ex, 'DESKTOP_BUILDX', self.tmp + '/no-such-buildx'), \
                self.assertRaisesRegex(ex.Refusal, 'invalid machine input'):
            self.d2(self.d1_ref(), activation_id='act-d2-missing', calls=calls)
        self.assertNotIn('d2-docker-build', [c[0] for c in calls])
        self.assertFalse(os.path.lexists(self.d2_out('act-d2-missing')['desktop_config'] + '/cli-plugins'))

    def test_the_775_plugin_link_is_preserved_and_a_fresh_occurrence_is_isolated(self):
        d1 = self.d1_ref()  # D0 creates the original private config; the 775 run linked its plugin there
        plugins = self.bindings['desktop_config'] + '/cli-plugins'
        os.mkdir(plugins, 0o700)
        os.symlink(self.colima, plugins + '/docker-buildx')
        receipt, _ = self.d2(d1, activation_id='act-d2-fresh')
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual(os.readlink(plugins + '/docker-buildx'), self.colima)  # the earlier link is never adopted or touched
        fresh = self.d2_out('act-d2-fresh')['desktop_config']
        self.assertNotEqual(fresh, self.bindings['desktop_config'])
        self.assertEqual(os.readlink(fresh + '/cli-plugins/docker-buildx'), ex.DESKTOP_BUILDX)

    def test_a_pre_existing_fresh_config_refuses_before_any_step(self):
        d1 = self.d1_ref()
        fresh = self.d2_out('act-d2-taken')['desktop_config']
        os.mkdir(fresh, 0o700)
        calls = []
        with self.assertRaisesRegex(ex.Refusal, 'refuse, do not adopt'):
            self.d2(d1, activation_id='act-d2-taken', calls=calls)
        self.assertEqual(calls, [])
        self.assertEqual(os.listdir(fresh), [])
        self.assertEqual(json.loads(Path(self.stage_dir('D2', 'act-d2-taken') + '/stage-receipt.json').read_text())['closure'], 'refused')

    def test_a_symlinked_fresh_config_refuses_and_is_never_followed(self):
        d1 = self.d1_ref()
        fresh = self.d2_out('act-d2-alias')['desktop_config']
        os.symlink(self.bindings['desktop_config'], fresh)  # an alias to the original config is a foreign target
        calls = []
        with self.assertRaises(ex.Refusal):
            self.d2(d1, activation_id='act-d2-alias', calls=calls)
        self.assertEqual(calls, [])
        self.assertFalse(os.path.lexists(self.bindings['desktop_config'] + '/cli-plugins'))  # the original config is untouched

    def test_d2_steps_use_the_fresh_config_and_the_plugin_checks_precede_the_build(self):
        calls = []
        self.d2(self.d1_ref(), activation_id='act-d2-env', calls=calls)
        fresh = self.d2_out('act-d2-env')['desktop_config']
        by_step = {c[0]: c for c in calls}
        for step in ('d2-engine-info', 'd2-buildx-metadata', 'd2-docker-build', 'd2-readback'):
            self.assertEqual(by_step[step][2]['DOCKER_CONFIG'], fresh, step)
        self.assertEqual(by_step['d2-buildx-metadata'][1][0], fresh + '/cli-plugins/docker-buildx')
        self.assertNotIn(self.bindings['desktop_config'], [c[2].get('DOCKER_CONFIG') for c in calls])
        ids = [c[0] for c in calls]
        self.assertLess(ids.index('d2-buildx-metadata'), ids.index('d2-docker-build'))

    def test_each_activation_derives_fresh_occurrence_paths_never_the_original_ones(self):
        first = ex.d2_occurrence(self.bindings, 'occ-a')
        second = ex.d2_occurrence(self.bindings, 'occ-b')
        self.assertEqual(first, ex.d2_occurrence(self.bindings, 'occ-a'))  # derived, not random
        self.assertEqual(len(set(first.values()) | set(second.values())), 8)
        originals = {self.bindings[k] for k in ('desktop_config', 'desktop_input_dir', 'desktop_context_dir', 'desktop_iidfile')}
        for path in list(first.values()) + list(second.values()):
            self.assertNotIn(path, originals)
            self.assertTrue(path.startswith(self.bindings['control'] + '/'))

    def test_the_original_d0_d1_paths_and_their_earlier_payloads_are_never_touched(self):
        d1_ref = self.d1_ref()  # D0 creates the original context directory
        original = self.bindings['desktop_context_dir']
        Path(original + '/Dockerfile').write_text('earlier payload')
        receipt, _ = self.d2(d1_ref, activation_id='act-d2-keep')
        self.assertEqual(receipt['closure'], 'closed')
        self.assertFalse(os.path.lexists(self.bindings['desktop_input_dir']))
        self.assertEqual(Path(original + '/Dockerfile').read_text(), 'earlier payload')


    def plugin_bytes(self, body):
        """Replace the fixture plugin with other bytes, pinned to their own sha256 (an executable script)."""
        data = b'#!/bin/sh\n' + body
        Path(ex.DESKTOP_BUILDX).write_bytes(data)
        os.chmod(ex.DESKTOP_BUILDX, 0o755)
        return hashlib.sha256(data).hexdigest()

    def test_a_changed_plugin_after_the_metadata_check_refuses_before_the_build(self):
        """The bytes Docker's discovery answered are the bytes the build would run: a replacement after the metadata
        step refuses at the custody check, before the build, and no later effect runs."""
        def replace(kw):
            if kw['step'] == 'd2-buildx-metadata':
                self.plugin_bytes(FAKE_METADATA_LINE + b"# replaced\n")
        d1 = self.d1_ref()
        calls = []
        with self.assertRaisesRegex(ex.Refusal, 'pinned sha256'):
            self.execute('D2', self.routed({'d2-engine-info': emit(ENGINE_JSON), 'd2-buildx-metadata': RUN_ARGV,
                                             'd2-docker-build': emit(''), 'd2-readback': emit(READBACK)}, calls, after=replace),
                         prior_ref=d1, activation_id='act-d2-swap')
        self.assertNotIn('d2-docker-build', [c[0] for c in calls])

    def test_a_plugin_of_another_schema_refuses_before_the_fetch(self):
        d1 = self.d1_ref()
        digest = self.plugin_bytes(b"printf '%s' '" + FAKE_METADATA_JSON.replace(b"0.1.0", b"0.2.0") + b"'\n")
        calls = []
        with mock.patch.object(ex, 'DESKTOP_BUILDX_SHA256', digest), self.assertRaisesRegex(ex.Refusal, 'pinned schema'):
            self.execute('D2', self.routed({'d2-engine-info': emit(ENGINE_JSON), 'd2-buildx-metadata': RUN_ARGV}, calls),
                         prior_ref=d1, activation_id='act-d2-schema')
        self.assertEqual([c[0] for c in calls], ['d2-engine-info', 'd2-buildx-metadata'])
        self.assertFalse(os.path.lexists(self.d2_out('act-d2-schema')['desktop_input_dir']))

    def test_a_plugin_that_does_not_answer_the_metadata_stops_before_the_build(self):
        """The legacy-builder shape: a candidate Docker cannot discover answers with an error, and nothing is fetched."""
        d1 = self.d1_ref()
        digest = self.plugin_bytes(b'echo "unknown flag: --provenance" >&2\nexit 125\n')
        calls = []
        with mock.patch.object(ex, 'DESKTOP_BUILDX_SHA256', digest):
            receipt = self.execute('D2', self.routed({'d2-engine-info': emit(ENGINE_JSON), 'd2-buildx-metadata': RUN_ARGV,
                                                      'd2-docker-build': emit(''), 'd2-readback': emit(READBACK)}, calls),
                                   prior_ref=d1, activation_id='act-d2-legacy')
        self.assertNotEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['d2-engine-info', 'd2-buildx-metadata'])
        self.assertFalse(os.path.lexists(self.d2_out('act-d2-legacy')['desktop_input_dir']))


class D34WindowTests(Fixture):
    """D3 and D4 through the proof entry's window, each naming the receipt it consumes (D2 for D3, D3 for D4). A window
    whose predecessor did not close is refused before any claim, so a failed predecessor cannot trigger cleanup."""

    def request(self, stage, prior, activation_id):
        options = dict(activation_id=activation_id, duration_seconds=300, evidence_root=self.evidence, prior_receipt=prior)
        return proof.window_recipe(stage, self.bindings, options)

    def freeze(self, request, name):
        path = self.tmp + '/' + name
        Path(path).write_text(json.dumps(request, sort_keys=True))
        os.chmod(path, 0o400)
        return path, ex.sha_file(path)

    def run_d3(self, path, digest, calls, cid_route=None):
        def write_cid(kw):
            Path(kw['argv'][kw['argv'].index('--cidfile') + 1]).write_text(CID)
            return emit(FACTS)
        routes = {'d3-engine-info': emit(ENGINE_JSON), 'd3-disposable-check': cid_route or write_cid}
        with mock.patch.object(ex, 'default_transport', self.routed(routes, calls)), \
                mock.patch.object(ex, 'default_monitor', FixedHelperTests.quiet_monitor), \
                contextlib.redirect_stdout(io.StringIO()) as out:
            code = proof.main(['proof', path, digest])
        return code, json.loads(out.getvalue())

    def d2_ref(self):
        d0 = self.d0()
        _, d1_ref = self.d1(d0)
        _, d2_ref = self.d2(d1_ref)
        return d2_ref

    def test_d3_window_runs_through_the_entry_and_its_go_names_the_d2_receipt(self):
        d2_ref = self.d2_ref()
        request = self.request('D3', d2_ref, 'd3-window-0001')
        self.assertEqual(request['prior_receipt'], d2_ref)
        self.assertEqual(request['absent_paths'], [ex.d3_occurrence(self.bindings, 'd3-window-0001')['desktop_cidfile']])
        path, digest = self.freeze(request, 'd3-window-request.json')
        calls = []
        code, receipt = self.run_d3(path, digest, calls)
        self.assertEqual(code, 0)
        self.assertEqual(receipt['closure'], 'closed')
        self.assertEqual([c[0] for c in calls], ['d3-engine-info', 'd3-disposable-check'])
        go = json.loads(Path(request['activation_path']).read_text())
        self.assertEqual(go['prior_receipt'], d2_ref)

    def test_d3_window_refuses_an_existing_cidfile_before_the_claim(self):
        d2_ref = self.d2_ref()
        path, digest = self.freeze(self.request('D3', d2_ref, 'd3-present-0001'), 'd3-present.json')
        Path(ex.d3_occurrence(self.bindings, 'd3-present-0001')['desktop_cidfile']).write_text(CID)
        with mock.patch.object(ex, '_execute_stage') as effect, \
                self.assertRaisesRegex(ex.Refusal, 'named destination is not absent'):
            proof.main(['proof', path, digest])
        effect.assert_not_called()
        self.assertFalse(os.path.lexists(self.tmp + '/claims/d3-present-0001.claim'))


    def test_d3_real_monitor_and_fixed_fake_client(self):
        """Exercise the plan and executor with a fixed local client, not fabricated receipts."""
        d2_ref = self.d2_ref()
        original = Path(self.bindings['desktop_cidfile'])
        original.write_bytes(b'old occurrence')
        prior_bytes = Path(d2_ref['path']).read_bytes()
        seen = []
        saved_monitor = ex.default_monitor
        for mode in ('zero', 'finite', 'oversized', 'unexpected'):
            activation = 'd3-control-' + mode
            request = self.request('D3', d2_ref, activation)
            path, digest = self.freeze(request, activation + '.json')
            fake = Path(self.tmp) / ('docker-' + mode + '.py')
            fake.write_text(
                "import os,sys,time\n"
                "a=sys.argv[1:]\n"
                "if a[0]=='info': print(%r); sys.exit(0)\n" % ENGINE_JSON +
                "p=a[a.index('--cidfile')+1]\n"
                "with open(p,'wb') as f: f.write(%r); f.flush(); os.fsync(f.fileno())\n" %
                (b'x' * 8192 if mode == 'oversized' else CID.encode()) +
                ("open(os.path.join(os.path.dirname(p),'unexpected'),'wb').write(b'x')\n"
                 if mode == 'unexpected' else '') +
                "time.sleep(0.7)\nprint(%r)\n" % FACTS)

            def fixed_transport(**kw):
                argv = [sys.executable, '-I', '-B', str(fake)] + kw['argv'][1:]
                return primitive.run_stage(kw['root'], argv, kw['timeout'], kw['output_cap'], kw['input_hashes'],
                                           guest=False, cwd=kw['cwd'], environment=CHILD_ENV,
                                           resource_monitor=kw['monitor'], deadline=kw['deadline'],
                                           identity_probe=kw['identity_probe'])

            def old_cap(limits, roots):
                return saved_monitor({'storage': 0}, roots)
            factory = old_cap if mode == 'zero' else ex.default_monitor
            with mock.patch.object(ex, 'default_transport', fixed_transport), \
                    mock.patch.object(ex, 'default_monitor', factory), \
                    contextlib.redirect_stdout(io.StringIO()) as out:
                code = proof.main(['proof', path, digest])
            receipt = json.loads(out.getvalue())
            seen.append(request['occurrence'])
            if mode == 'finite':
                self.assertEqual(code, 0)
                self.assertEqual(receipt['closure'], 'closed')
                self.assertEqual(next(r for r in receipt['receipts'] if r['kind'] == 'container')['name'],
                                 request['occurrence']['desktop_container_name'])
            else:
                self.assertEqual(code, 1)
                self.assertEqual(receipt['closure'], 'stopped')
                step = next(s for s in receipt['steps'] if s['id'] == 'd3-disposable-check')
                self.assertEqual(step['closure'], 'unknown_retained')
            self.assertEqual(original.read_bytes(), b'old occurrence')
            self.assertEqual(Path(d2_ref['path']).read_bytes(), prior_bytes)
        self.assertEqual(len({row['desktop_cidfile'] for row in seen}), 4)
        self.assertEqual(len({row['desktop_container_name'] for row in seen}), 4)


    def test_d3_alias_and_readback_drift_refuse(self):
        d2_ref = self.d2_ref()
        request = self.request('D3', d2_ref, 'd3-alias-0001')
        path, digest = self.freeze(request, 'd3-alias.json')
        os.symlink(self.bindings['desktop_cidfile'], request['absent_paths'][0])
        with self.assertRaisesRegex(ex.Refusal, 'named destination is not absent'):
            self.run_d3(path, digest, [])
        _, d3_ref = self.d3(d2_ref)
        cidfile = ex.d3_occurrence(self.bindings, 'act-d3-0001')['desktop_cidfile']
        Path(cidfile).write_text('b' * 64)
        with self.assertRaises(ex.Refusal):
            self.request('D4', d3_ref, 'd4-drift-0001')
        self.assertFalse(os.path.lexists(self.tmp + '/claims/d4-drift-0001.claim'))

    def test_d3_window_names_only_the_d2_receipt_and_is_one_use(self):
        d2_ref = self.d2_ref()
        with self.assertRaisesRegex(ex.Refusal, 'original D2 receipt'):
            self.request('D3', dict(d2_ref, stage='D1'), 'd3-wrong-0001')
        path, digest = self.freeze(self.request('D3', d2_ref, 'd3-once-0001'), 'd3-once.json')
        self.run_d3(path, digest, [])
        calls = []
        with self.assertRaises(ex.Refusal):
            self.run_d3(path, digest, calls)
        self.assertEqual(calls, [])

    def test_d4_window_refuses_a_failed_d3_predecessor_before_any_cleanup(self):
        d2_ref = self.d2_ref()
        calls = []
        failed = self.execute('D3', self.routed({'d3-engine-info': emit(ENGINE_JSON),
                                                  'd3-disposable-check': emit(FACTS, exit_code=1)}, calls),
                              prior_ref=d2_ref, activation_id='act-d3-failed')
        self.assertNotEqual(failed['closure'], 'closed')
        failed_ref = self.ref('D3', 'act-d3-failed')
        with self.assertRaises(ex.Refusal):
            self.request('D4', failed_ref, 'd4-failed-0001')
        self.assertFalse(os.path.lexists(self.tmp + '/claims/d4-failed-0001.claim'))
        self.assertFalse(any(c[0].startswith('d4-') for c in calls))

    def test_d4_window_names_the_d3_receipt_only(self):
        d2_ref = self.d2_ref()
        _, d3_ref = self.d3(d2_ref)
        request = self.request('D4', d3_ref, 'd4-window-0001')
        self.assertEqual(request['prior_receipt'], d3_ref)
        with self.assertRaisesRegex(ex.Refusal, 'original D3 receipt'):
            self.request('D4', dict(d3_ref, stage='D2'), 'd4-wrongstage-0001')


class LiteralEntryTests(Fixture):
    """The literal proof-entry commands run as the entry runs them: a fresh interpreter, the shipped file, no effect.
    `plan` and `plan-window` are pure reads; an unarmed request refuses before any effect. Where the host's installed
    Docker Desktop is absent, the plan refuses on the bound machine inputs, which is the correct answer, not a pass."""

    def entry(self, *args):
        return subprocess.run([sys.executable, '-I', '-B', str(HERE / 'macos_stage_proof.py'), *args],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)

    def bindings_file(self):
        """The reviewed bindings for this token at the real control path: pure, nothing is created."""
        with mock.patch.object(prep, 'STASH', REAL_STASH):
            real = {k: v for k, v in prep.control_paths(TOKEN).items() if k in prep.SLOTS}
        real.update(desktop_host=prep.DESKTOP_HOST, token=TOKEN, check_source=prep.CHECK_SOURCE, route_echo=prep.ROUTE_ECHO)
        path = self.tmp + '/bindings.json'
        Path(path).write_text(json.dumps(real, sort_keys=True))
        return path

    def test_the_new_plan_commands_are_executable_and_bind_the_pinned_inputs(self):
        bindings = self.bindings_file()
        for stage in ('C5', 'C6', 'D3', 'D4'):
            with self.subTest(stage=stage):
                done = self.entry('plan', stage, bindings)
                if os.path.exists(ex.DESKTOP_BUILDX):
                    self.assertEqual(done.returncode, 0, done.stderr)
                    self.assertEqual(json.loads(done.stdout)['stage'], stage)
                else:
                    self.assertEqual(done.returncode, 2)
                    self.assertIn(b'invalid machine input', done.stderr)

    def test_the_unarmed_request_refuses_before_any_effect(self):
        request = self.tmp + '/unarmed-c5.json'
        Path(request).write_text(json.dumps({'stage': 'C5'}, sort_keys=True))
        done = self.entry(request, '0' * 64)
        self.assertEqual(done.returncode, 2)
        self.assertIn(b'refused', done.stderr)
        self.assertEqual(os.listdir(self.tmp + '/claims'), [])

    def test_a_window_without_its_predecessor_refuses_on_the_entry_path(self):
        options = self.tmp + '/d3-options.json'
        Path(options).write_text(json.dumps({'activation_id': 'literal-d3', 'duration_seconds': 300,
                                             'evidence_root': self.evidence}, sort_keys=True))
        done = self.entry('plan-window', 'D3', self.bindings_file(), options)
        self.assertEqual(done.returncode, 2)


class ProofEnvelopeTests(unittest.TestCase):
    """The containing selector clock holds the largest declared stage plus margin, and no stage clock was widened."""

    def selector_ms(self):
        text = (HERE.parents[2] / 'config/tasks.toml').read_text()
        line = next(l for l in text.splitlines() if l.startswith('"qualification:macos:proof-only"'))
        return int(re.search(r'timeout_ms = (\d+)', line).group(1))

    def test_the_proof_selector_contains_the_largest_declared_proof_stage(self):
        stages = {s['id']: s for s in ex.load_manifest()['stages'] if s['id'] in proof.PROOF_STAGES}
        largest = max(stage['seconds'] for stage in stages.values())
        self.assertEqual(largest, 1200)  # C4 and D2: 1200 seconds including the 120-second reserve
        self.assertGreaterEqual(self.selector_ms(), (largest + 60) * 1000)
        self.assertLessEqual(self.selector_ms(), (largest + 300) * 1000)  # margin, not a widened native allowance

    def test_the_new_proof_stages_keep_their_declared_clocks(self):
        manifest = ex.load_manifest()
        self.assertEqual({k: (ex.stage_of(manifest, k)['seconds'], ex.stage_of(manifest, k)['reserve_seconds'])
                          for k in ('C4', 'C5', 'C6', 'D2', 'D3', 'D4')},
                         {'C4': (1200, 120), 'C5': (360, 60), 'C6': (120, 60), 'D2': (1200, 120),
                          'D3': (360, 60), 'D4': (600, 60)})



if __name__ == '__main__':
    unittest.main(verbosity=2)

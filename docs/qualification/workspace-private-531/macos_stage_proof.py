"""Nucleus proof-only entry for a single admitted stage.

External precondition: Nucleus launches this exact selector at its pinned commit and
inputs after the native operator GO and records the proof execution. This program
cannot establish that provenance against another process of the same user. A request
file or this program's existence is not an activation.
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import macos_stage_executor as executor  # noqa: E402


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise executor.Refusal('duplicate request key')
        result[key] = value
    return result


# D1 is reached only through a window request, which names the original D0 receipt it consumes.
PROOF_STAGES = ('C0', 'D0', 'D1')

DESKTOP_DISK = '/Users/tom/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw'
DESKTOP_APP = '/Applications/Docker.app'
OPEN = '/usr/bin/open'
ORIGINAL_D0 = {'stage': 'D0', 'path': executor.LINUX_ARTIFACT_DIR + '/d0-proof-preparation.wmn5kjr9/'
               'evidence/D0-d0-22d86f3ea89ca8f1bbdc8b71ae471067/stage-receipt.json',
               'sha256': 'fa32183b7788cd35107997ab9d637252c63db43bd310d3d4e483fb86ca24c015'}
CONTEXT_DISCLOSURE = ('The fixed docker context show reads the default Docker configuration; its normal loader '
                      'may internally decode stored auths. No credential values are requested or retained.')


class DesktopDisk:
    """Hold the named file and observe allocation; sampled growth is not a quota."""
    def __init__(self):
        if os.path.realpath(DESKTOP_DISK) != DESKTOP_DISK:
            raise executor.Refusal('Desktop backing file path is aliased')
        try:
            self.fd = os.open(DESKTOP_DISK, os.O_RDONLY | os.O_NOFOLLOW)
            info = os.fstat(self.fd)
            self.identity = (info.st_dev, info.st_ino)
            self.initial = self.high = info.st_blocks * 512
            self.check()
        except OSError as exc:
            if hasattr(self, 'fd'):
                os.close(self.fd)
            raise executor.Refusal('Desktop backing file telemetry unavailable') from exc
        except BaseException:
            if hasattr(self, 'fd'):
                os.close(self.fd)
            raise

    def check(self):
        try:
            held, named = os.fstat(self.fd), os.lstat(DESKTOP_DISK)
            if (os.path.realpath(DESKTOP_DISK) != DESKTOP_DISK or not stat.S_ISREG(held.st_mode)
                    or not stat.S_ISREG(named.st_mode) or held.st_nlink != 1 or named.st_nlink != 1
                    or (held.st_dev, held.st_ino) != self.identity
                    or (named.st_dev, named.st_ino) != self.identity):
                raise executor.Refusal('Desktop backing file lost, replaced or aliased')
            current = held.st_blocks * 512
            self.high = max(self.high, current)
            if self.high - self.initial > 512 * 1024 * 1024:
                raise executor.Refusal('Desktop backing file growth exceeds 512 MiB')
            return dict(device=held.st_dev, inode=held.st_ino, allocated=current,
                        initial=self.initial, maximum=self.high)
        except OSError as exc:
            raise executor.Refusal('Desktop backing file telemetry unavailable') from exc

    def close(self):
        os.close(self.fd)


class CombinedMonitor:
    def __init__(self, private, disk, pins):
        self.private, self.disk, self.pins = private, disk, pins
        self.last, self.maxima = private.last, private.maxima

    def check(self):
        self.pins.recheck()
        usage = self.private.check()
        self.last, self.maxima = self.private.last, self.private.maxima
        return dict(private=usage, desktop_disk=self.disk.check())


def app_inputs(sample=None):
    """Bind every bundle member, its type and content, without following bundle symlinks."""
    if os.path.realpath(DESKTOP_APP) != DESKTOP_APP or not os.path.isdir(DESKTOP_APP):
        raise executor.Refusal('installed Desktop app missing or aliased')
    members = {}
    def failed(exc):
        raise executor.Refusal('Desktop bundle cannot be fully observed') from exc
    for root, dirs, files in os.walk(DESKTOP_APP, followlinks=False, onerror=failed):
        for name in sorted(dirs + files):
            path = os.path.join(root, name)
            rel = os.path.relpath(path, DESKTOP_APP)
            info = os.lstat(path)
            if stat.S_ISLNK(info.st_mode):
                if not os.path.realpath(path).startswith(DESKTOP_APP + '/'):
                    raise executor.Refusal('Desktop bundle symlink escapes the installed app')
                members[rel] = ['link', os.readlink(path)]
            elif stat.S_ISDIR(info.st_mode):
                members[rel] = ['directory', stat.S_IMODE(info.st_mode)]
            elif stat.S_ISREG(info.st_mode):
                digest = hashlib.sha256()
                with open(path, 'rb') as source:
                    while True:
                        data = source.read(1024 * 1024)
                        if not data:
                            break
                        digest.update(data)
                        if sample is not None:
                            sample()
                members[rel] = ['file', stat.S_IMODE(info.st_mode), digest.hexdigest()]
            else:
                raise executor.Refusal('unsupported Desktop bundle member')
    if not members or not os.path.isfile(OPEN):
        raise executor.Refusal('Desktop app or fixed launcher missing')
    return dict(bundle_sha256=executor.canon_sha(members), open_sha256=executor.sha_file(OPEN))


def ready_recipe(bindings, options):
    if set(options) != {'activation_id', 'duration_seconds', 'evidence_root'}:
        raise executor.Refusal('unsupported Desktop readiness options')
    duration = options['duration_seconds']
    if type(duration) is not int or not 60 < duration <= 900:
        raise executor.Refusal('Desktop readiness window must be 61 to 900 seconds')
    if executor.sha_file(executor.prep.DOCKER) != executor.HISTORICAL_TOOL_HASHES['docker']:
        raise executor.Refusal('Desktop CLI differs from source-reviewed 29.8.0 build 88096ef')
    d1_options = dict(options, duration_seconds=300, prior_receipt=ORIGINAL_D0)
    d1 = window_recipe('D1', bindings, d1_options)
    prior = executor.read_receipt(ORIGINAL_D0)
    if prior.get('engine_id') != executor.RECORDED_ENGINE_ID:
        raise executor.Refusal('original D0 engine identity differs')
    pins = executor.Pins()
    try:
        for name in ('control', 'desktop_config', 'desktop_context_dir'):
            pins.hold(bindings[name])
        executor.check_d0_identities(prior, bindings, pins)
    finally:
        pins.close()
    disk = DesktopDisk()
    try:
        identity = list(disk.identity)
    finally:
        disk.close()
    return dict(schema='macos-desktop-ready-d1-v1', d1=d1, duration_seconds=duration,
                desktop_disk_identity=identity, app_inputs=app_inputs(),
                requiresOperatorGo=('Start /Applications/Docker.app if needed and create one empty labelled '
                                    'volume. ' + CONTEXT_DISCLOSURE))


def execute_ready(request, request_sha):
    start = time.monotonic()
    deadline = start + request['duration_seconds']
    hard = min(start + 600, deadline - 60)
    d1 = request['d1']
    bindings = d1['bindings']
    options = dict(activation_id=d1['activation_id'], duration_seconds=request['duration_seconds'],
                   evidence_root=d1['evidence_root'])
    expected = executor.call_bounded(lambda: ready_recipe(bindings, options), hard - time.monotonic())
    if request != expected or any(value is None for value in d1['roots'].values()):
        raise executor.Refusal('Desktop readiness request or root identity differs')
    pins = executor.Pins()
    disk = None
    root = d1['evidence_root'] + '/ready-' + d1['activation_id']
    receipt = dict(schema='macos-desktop-readiness-receipt-v1', request_sha256=request_sha,
                   closure='refused', startup='not-dispatched', samples=[], steps=[],
                   simulation=bool(executor.TEST_SEAM['admit_unadmitted'] or executor.TEST_SEAM['simulate_guest_closure']))
    try:
        for path, identity in d1['roots'].items():
            pins.hold(path)
            if list(pins.items[path]) != identity:
                raise executor.Refusal('Desktop readiness root identity changed')
        executor.check_d0_identities(executor.read_receipt(ORIGINAL_D0), bindings, pins)
        disk = DesktopDisk()
        if list(disk.identity) != request['desktop_disk_identity']:
            raise executor.Refusal('Desktop backing file identity changed since request')
        d1_paths = (d1['activation_path'], d1['claim_path'], d1['evidence_path'],
                    d1['evidence_root'] + '/D1-' + d1['activation_id'],
                    executor.CLAIMS_DIR + '/' + d1['activation_id'] + '.claim')
        if os.path.lexists(root) or any(os.path.lexists(path) for path in d1_paths):
            raise executor.Refusal('readiness or D1 destination already exists; no replay')
        receipt['claim'] = executor.claim_activation('ready-' + d1['activation_id'], request_sha)
        executor.create_private_dir(root)
        pins.hold(root)
        private = executor.default_monitor({'storage': 1024 * 1024}, [bindings['control']])
        monitor = CombinedMonitor(private, disk, pins)
        input_identity, runtime_inputs = executor.stable_input_manifest()
        if input_identity != d1['inputs']:
            raise executor.Refusal('Desktop readiness inputs changed before dispatch')
        docker_target = os.path.realpath(executor.prep.DOCKER)
        runtime_inputs[os.path.realpath(OPEN)] = request['app_inputs']['open_sha256']

        def sample():
            if os.path.realpath(executor.prep.DOCKER) != docker_target:
                raise executor.Refusal('Docker CLI path changed during readiness')
            value = monitor.check()
            receipt['samples'].append(dict(monotonic=time.monotonic(), **value))
            return value

        last_app_sample = [0.0]
        def sample_app():
            now = time.monotonic()
            if now - last_app_sample[0] >= 0.5:
                sample()
                last_app_sample[0] = now

        class RecordingMonitor:
            @property
            def last(self):
                return monitor.last

            @property
            def maxima(self):
                return monitor.maxima

            def check(self):
                return sample()

        def child(name, argv, env, allowance=30, retry_endpoint=False):
            now = time.monotonic()
            step_deadline = min(now + allowance, hard)
            if step_deadline <= now:
                raise executor.Refusal(name + ': readiness allowance exhausted')
            pins.recheck()
            path = root + '/' + name
            executor.create_private_dir(path)
            pins.hold(path)
            result = executor.call_bounded(
                lambda: executor.primitive.run_stage(path, argv, step_deadline - now, 262144, runtime_inputs,
                            cwd=path, environment=env, resource_monitor=RecordingMonitor(), deadline=step_deadline,
                            cleanup_deadline=deadline, identity_probe=executor.start_identity), deadline - time.monotonic())
            receipt['steps'].append(dict(id=name, result=result, receipt=path + '/receipt.json',
                                         receipt_sha256=executor.checked_sha(path + '/receipt.json', must_be_in=root)))
            if retry_endpoint and result.get('disposition') == 'failed' and result.get('waited_exit') == 1:
                out = executor.read_checked_file(path + '/stdout', must_be_in=root, read_only=False, max_bytes=262144)
                err = executor.read_checked_file(path + '/stderr', must_be_in=root, read_only=False, max_bytes=262144)
                fixed = ('failed to connect to the docker API at ' + executor.prep.DESKTOP_HOST +
                         '; check if the path is correct and if the daemon is running: dial unix ' +
                         executor.DESKTOP_SOCKET + ': connect: ')
                if out == b'' and err in ((fixed + 'connection refused\n').encode(),
                                           (fixed + 'no such file or directory\n').encode()):
                    return None
            if result.get('disposition') != 'exited' or result.get('waited_exit') != 0:
                raise executor.Refusal(name + ': command failed or closure unknown')
            return executor.read_checked_file(path + '/stdout', must_be_in=root, read_only=False, max_bytes=262144)

        def context(name):
            env = {'HOME': '/Users/tom', 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'LANG': 'C',
                   'DOCKER_CLI_HINTS': 'false', 'DOCKER_CLI_HOOKS': 'false', 'OTEL_SDK_DISABLED': 'true'}
            data = child(name, [executor.prep.DOCKER, 'context', 'show'], env)
            if not data.endswith(b'\n') or data.count(b'\n') != 1:
                raise executor.Refusal('context name is missing or malformed')
            try:
                value = data[:-1].decode('ascii')
            except UnicodeDecodeError as exc:
                raise executor.Refusal('context name is invalid') from exc
            if not executor.re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,127}', value):
                raise executor.Refusal('context name is invalid')
            return value

        def socket_present():
            try:
                info = os.lstat(executor.DESKTOP_SOCKET)
            except FileNotFoundError:
                return False
            if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.geteuid():
                raise executor.Refusal('Desktop endpoint is not the owned socket')
            return True

        sample()
        before_context = context('context-before')
        if not socket_present():
            if before_context != 'desktop-linux':
                raise executor.Refusal('startup refused: default context is not desktop-linux')
            if executor.call_bounded(lambda: app_inputs(sample_app), hard - time.monotonic()) != request['app_inputs']:
                raise executor.Refusal('Desktop app input changed before startup')
            receipt['startup_claim'] = executor.claim_activation('start-' + d1['activation_id'], request_sha)
            receipt['startup'] = 'claimed'
            child('app-start', [OPEN, '-a', DESKTOP_APP],
                  {'HOME': '/Users/tom', 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'LANG': 'C'})
            receipt['startup'] = 'launched'
            while not socket_present():
                sample()
                remaining = hard - time.monotonic()
                if remaining <= 0:
                    raise executor.Refusal('Desktop endpoint did not become available')
                time.sleep(min(0.5, remaining))
        else:
            receipt['startup'] = 'skipped'
        attempt = 0
        while True:
            if not socket_present():
                raise executor.Refusal('Desktop endpoint disappeared before readiness read')
            attempt += 1
            name = 'engine-info' if attempt == 1 else 'engine-info-%04d' % attempt
            info = child(name, [executor.prep.DOCKER, 'info', '--format', '{{json .}}'],
                         {'PATH': executor.DESKTOP_BASE_PATH, 'LANG': 'C',
                          'DOCKER_CONFIG': bindings['desktop_config'], 'DOCKER_HOST': bindings['desktop_host']},
                         retry_endpoint=receipt['startup'] == 'launched')
            if info is not None:
                break
            remaining = hard - time.monotonic()
            if remaining <= 0:
                raise executor.Refusal('Desktop engine did not become ready')
            sample()
            time.sleep(min(0.5, remaining))
        try:
            engine = executor.prep.engine_id_from_info(info.decode())
        except (ValueError, UnicodeDecodeError) as exc:
            raise executor.Refusal('Desktop engine facts invalid') from exc
        if engine != executor.RECORDED_ENGINE_ID:
            raise executor.Refusal('Desktop engine differs from original D0')
        after_context = context('context-after')
        if after_context != before_context:
            raise executor.Refusal('global context changed during readiness')
        if executor.call_bounded(lambda: app_inputs(sample_app), hard - time.monotonic()) != request['app_inputs']:
            raise executor.Refusal('Desktop app inputs changed during readiness')
        sample()
        if deadline - time.monotonic() <= 60:
            raise executor.Refusal('D1 reserve exhausted before dispatch')
        receipt.update(closure='closed', engine_id=engine, context=before_context)
        executor.write_json_new(root, 'readiness-receipt.json', receipt)
        manifest = executor.load_manifest()
        inputs = executor.canon_sha(executor.stable_input_manifest()[0])
        def stage_monitor(_limits, _roots):
            return RecordingMonitor()
        return executor.call_bounded(
            lambda: execute_window(d1, request_sha, manifest, inputs, outer_deadline=deadline,
                                   monitor_factory=stage_monitor, force_simulation=receipt['simulation']),
            deadline - time.monotonic())
    except BaseException as exc:
        receipt['error'] = str(exc)
        if os.path.isdir(root) and not os.path.lexists(root + '/readiness-receipt.json'):
            executor.write_json_new(root, 'readiness-receipt.json', receipt)
        raise
    finally:
        try:
            if disk is not None and os.path.isdir(root):
                try:
                    receipt['samples'].append(dict(monotonic=time.monotonic(), desktop_disk=disk.check()))
                except Exception as exc:
                    receipt['samples'].append(dict(monotonic=time.monotonic(), telemetry_error=str(exc)))
                    executor.write_json_new(root, 'disk-observations.json', dict(samples=receipt['samples']))
                    raise
                executor.write_json_new(root, 'disk-observations.json', dict(samples=receipt['samples']))
        finally:
            if disk is not None:
                disk.close()
            pins.close()


def prospective(stage_id, bindings):
    """Read-only binding data for a later externally authorised proof launch."""
    if stage_id not in PROOF_STAGES:
        raise executor.Refusal('proof entry supports only admitted C0, D0 or D1')
    manifest = executor.load_manifest()
    plan = executor.resolve_stage(stage_id, bindings, manifest)
    if plan['static_admission']:
        raise executor.Refusal('stage unadmitted: ' + '; '.join(plan['static_admission']))
    identity, _ = executor.stable_input_manifest()
    return dict(stage=stage_id, bindings=bindings, inputs=identity,
                inputs_sha256=executor.canon_sha(identity),
                manifest_sha256=executor.sha_file(executor.MANIFEST_PATH),
                source_sha256=executor.sha_file(executor.__file__),
                candidate_head=manifest['source_head'], bindings_sha256=executor.canon_sha(bindings))



def checked_prior(ref, bindings, manifest):
    """The original D0 receipt a D1 window names: exact canonical path and hash, and its complete chain verified now
    for this token, control root and endpoint. A different preparation head does not relabel it."""
    if not isinstance(ref, dict) or set(ref) != {'stage', 'path', 'sha256'} or ref.get('stage') != 'D0':
        raise executor.Refusal('D1 names the original D0 receipt by stage, path and sha256')
    body = executor.read_receipt(ref)
    executor.verify_receipt_tree(body, 'D0', executor.chain_key(bindings), manifest)
    return dict(stage='D0', path=ref['path'], sha256=ref['sha256'])


def window_recipe(stage_id, bindings, options):
    """Prospective request. Directory identities must be captured after provisioning."""
    base_options = {'activation_id', 'duration_seconds', 'evidence_root'}
    if set(options) != base_options | ({'prior_receipt'} if stage_id == 'D1' else set()):
        raise executor.Refusal('unsupported window options')
    identity = prospective(stage_id, bindings)
    prior = checked_prior(options['prior_receipt'], bindings, executor.load_manifest()) if stage_id == 'D1' else None
    manifest = executor.load_manifest()
    stage = executor.stage_of(manifest, stage_id)
    duration = options['duration_seconds']
    if type(duration) is not int or not stage['reserve_seconds'] < duration <= min(stage['seconds'], executor.prep.STAGE_CAP_SECONDS):
        raise executor.Refusal('invalid window duration')
    activation_id = options['activation_id']
    if not isinstance(activation_id, str) or not executor.ACTIVATION_ID.fullmatch(activation_id) or activation_id.startswith('531'):
        raise executor.Refusal('invalid activation id')
    evidence = options['evidence_root']
    if not isinstance(evidence, str) or os.path.realpath(evidence) != evidence or not evidence.startswith(executor.prep.STASH + '/'):
        raise executor.Refusal('evidence root must be canonical under scratch')
    roots = [executor.ACTIVATIONS_DIR, executor.CLAIMS_DIR, evidence,
             os.path.dirname(bindings['control']) if stage_id == 'C0' else bindings['control']]
    if stage_id == 'D1':  # the Docker directories the original D0 chain created are pinned by identity
        roots += [bindings['desktop_config'], bindings['desktop_context_dir']]
    if len(set(roots)) != len(roots):
        raise executor.Refusal('root alias')
    observed = {}
    for root in roots:
        try:
            observed[root] = list(executor.Pins.observe(root))
        except FileNotFoundError:
            observed[root] = None  # Explicit provisioning precondition; execution refuses it.
    # D0 creates these paths exclusively; their approved identity is absence.
    absent_paths = ([os.path.dirname(bindings['desktop_config']), bindings['desktop_config'],
                     bindings['desktop_context_dir']] if stage_id == 'D0' else [])
    return dict(schema='macos-stage-window-v1', stage=stage_id, bindings=bindings,
                bindings_sha256=executor.canon_sha(bindings), inputs=identity['inputs'],
                inputs_sha256=identity['inputs_sha256'], manifest_sha256=identity['manifest_sha256'],
                source_sha256=identity['source_sha256'], candidate_head=identity['candidate_head'],
                decision=executor.prep.DECISION, decision_version=executor.prep.DECISION_VERSION,
                scope='single-activation', activation_id=activation_id, duration_seconds=duration,
                activation_path=executor.ACTIVATIONS_DIR + '/' + activation_id + '.json',
                claim_path=executor.CLAIMS_DIR + '/proof-' + activation_id + '.claim',
                evidence_path=evidence + '/proof-' + activation_id + '.json',
                evidence_root=evidence, roots=observed, absent_paths=absent_paths, prior_receipt=prior)


def execute_window(request, request_sha, manifest, inputs_hash, *, outer_deadline=None, monitor_factory=None,
                   force_simulation=False):
    options = {k: request[k] for k in ('activation_id', 'duration_seconds', 'evidence_root')}
    if request['stage'] == 'D1':
        options['prior_receipt'] = request['prior_receipt']
    expected = window_recipe(request['stage'], request['bindings'], options)
    if request != expected or any(value is None for value in expected['roots'].values()):
        raise executor.Refusal('window request, policy or root identity differs')
    if any(os.path.lexists(path) for path in request['absent_paths']):
        raise executor.Refusal('D0 destination is not absent')
    paths = (request['activation_path'], request['claim_path'], request['evidence_path'],
             request['evidence_root'] + '/' + request['stage'] + '-' + request['activation_id'],
             executor.CLAIMS_DIR + '/' + request['activation_id'] + '.claim')
    if len(set(paths)) != len(paths) or any(os.path.lexists(path) for path in paths):
        raise executor.Refusal('window destination collision')
    pins = executor.Pins()
    try:
        for root, identity in request['roots'].items():
            pins.hold(root)
            if list(pins.items[root]) != identity:
                raise executor.Refusal('root identity changed')
        pins.recheck()
        if any(os.path.lexists(path) for path in request['absent_paths']):
            raise executor.Refusal('D0 destination is not absent')
        executor.claim_activation('proof-' + request['activation_id'], request_sha)
        pins.recheck()
        now = datetime.datetime.now(datetime.timezone.utc)
        go = dict(decision=request['decision'], decision_version=request['decision_version'],
                  stage=request['stage'], manifest_sha256=request['manifest_sha256'],
                  source_sha256=request['source_sha256'], inputs_sha256=inputs_hash,
                  activation_id=request['activation_id'], issued_utc=now.isoformat(),
                  expires_utc=(now + datetime.timedelta(seconds=request['duration_seconds'])).isoformat(),
                  scope=request['scope'], candidate_head=manifest['source_head'],
                  bindings_sha256=request['bindings_sha256'])
        if request['prior_receipt'] is not None:
            go['prior_receipt'] = request['prior_receipt']
        raw = (json.dumps(go, sort_keys=True, indent=2) + '\n').encode()
        digest = hashlib.sha256(raw).hexdigest()
        executor.write_new_file(executor.ACTIVATIONS_DIR, os.path.basename(request['activation_path']), raw, mode=0o400)
        pins.recheck()
        executor.write_json_new(request['evidence_root'], os.path.basename(request['evidence_path']),
                                dict(request_sha256=request_sha, activation_sha256=digest,
                                     activation=go, activation_bytes_utf8=raw.decode(),
                                     claim_path=request['claim_path']))
        pins.recheck()
        return executor._execute_stage(request['stage'], request['bindings'],
                                       activation_path=request['activation_path'], activation_sha256=digest,
                                       evidence_root=request['evidence_root'], manifest_path=executor.MANIFEST_PATH,
                                       outer_deadline=outer_deadline, monitor_factory=monitor_factory,
                                       force_simulation=force_simulation)
    finally:
        pins.close()

def main(argv):
    if len(argv) == 4 and argv[1] == 'plan-ready':
        bindings = json.loads(Path(argv[2]).read_text(), object_pairs_hook=unique_object)
        options = json.loads(Path(argv[3]).read_text(), object_pairs_hook=unique_object)
        print(json.dumps(ready_recipe(bindings, options), indent=2, sort_keys=True))
        return 0
    if len(argv) == 5 and argv[1] == 'plan-window':
        bindings = json.loads(Path(argv[3]).read_text(), object_pairs_hook=unique_object)
        options = json.loads(Path(argv[4]).read_text(), object_pairs_hook=unique_object)
        print(json.dumps(window_recipe(argv[2], bindings, options), indent=2, sort_keys=True))
        return 0
    if len(argv) == 4 and argv[1] == 'plan':
        bindings = json.loads(Path(argv[3]).read_text())
        print(json.dumps(prospective(argv[2], bindings), indent=2, sort_keys=True))
        return 0
    if len(argv) != 3:
        raise executor.Refusal('proof entry requires a request and its SHA-256')
    path = str(Path(argv[1]).resolve())
    if path != argv[1]:
        raise executor.Refusal('proof request path must be canonical')
    raw = executor.read_checked_file(path, must_be_in=executor.prep.STASH, read_only=True, max_bytes=65536)
    if hashlib.sha256(raw).hexdigest() != argv[2]:
        raise executor.Refusal('proof request hash or size mismatch')
    request = json.loads(raw, object_pairs_hook=unique_object)
    if isinstance(request, dict) and request.get('schema') == 'macos-desktop-ready-d1-v1':
        if set(request) != {'schema', 'd1', 'duration_seconds', 'desktop_disk_identity', 'app_inputs',
                            'requiresOperatorGo'}:
            raise executor.Refusal('unsupported Desktop readiness request')
        receipt = execute_ready(request, argv[2])
        print(json.dumps(receipt, sort_keys=True))
        return 0 if receipt['closure'] == 'closed' else 1
    window = isinstance(request, dict) and request.get('schema') == 'macos-stage-window-v1'
    window_keys = {'schema', 'stage', 'bindings', 'bindings_sha256', 'inputs', 'inputs_sha256',
                   'manifest_sha256', 'source_sha256', 'candidate_head', 'decision', 'decision_version',
                   'scope', 'activation_id', 'duration_seconds', 'activation_path', 'claim_path',
                   'evidence_path', 'evidence_root', 'roots', 'absent_paths', 'prior_receipt'}
    if not isinstance(request, dict) or (window and set(request) != window_keys) or (not window and set(request) != {
        'stage', 'activation_path', 'activation_sha256', 'bindings', 'evidence_root',
        'inputs', 'inputs_sha256', 'manifest_sha256', 'source_sha256',
    }):
        raise executor.Refusal('unsupported proof request')
    stage_id = request['stage']
    if stage_id not in PROOF_STAGES:
        raise executor.Refusal('proof entry supports only admitted C0, D0 or D1')
    if stage_id == 'D1' and not window:
        raise executor.Refusal('D1 runs only through a window request that names its D0 receipt')
    manifest = executor.load_manifest()
    plan = executor.resolve_stage(stage_id, request['bindings'], manifest)
    if plan['static_admission']:
        raise executor.Refusal('stage unadmitted: ' + '; '.join(plan['static_admission']))
    identity, inputs = executor.stable_input_manifest()
    inputs_hash = executor.canon_sha(identity)
    if (request['inputs'] != identity or request['inputs_sha256'] != inputs_hash or
            request['manifest_sha256'] != executor.sha_file(executor.MANIFEST_PATH) or
            request['source_sha256'] != executor.sha_file(executor.__file__)):
        raise executor.Refusal('proof input binding differs')
    if window:
        receipt = execute_window(request, argv[2], manifest, inputs_hash)
        print(json.dumps(receipt, sort_keys=True))
        return 0 if receipt['closure'] == 'closed' else 1
    activation = executor.read_checked_file(request['activation_path'],
                                             must_be_in=executor.ACTIVATIONS_DIR, read_only=True)
    now = datetime.datetime.now(datetime.timezone.utc)
    executor.check_activation(activation, request['activation_sha256'], stage_id=stage_id,
                              bindings=request['bindings'], manifest=manifest,
                              manifest_path=executor.MANIFEST_PATH, inputs=identity, now=now)
    fd = executor.private_dir(request['evidence_root'])
    os.close(fd)
    claim_hash = executor.canon_sha({'request_sha256': argv[2], 'inputs_sha256': inputs_hash})
    executor.claim_activation('proof-' + claim_hash, argv[2])
    receipt = executor._execute_stage(stage_id, request['bindings'],
                                      activation_path=request['activation_path'],
                                      activation_sha256=request['activation_sha256'],
                                      evidence_root=request['evidence_root'], utc_now=now)
    print(json.dumps(receipt, sort_keys=True))
    return 0 if receipt['closure'] == 'closed' else 1


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except executor.Refusal as exc:
        print('refused: ' + str(exc), file=sys.stderr)
        sys.exit(2)

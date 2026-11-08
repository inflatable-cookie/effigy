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
import sys

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


def execute_window(request, request_sha, manifest, inputs_hash):
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
                                       evidence_root=request['evidence_root'], manifest_path=executor.MANIFEST_PATH)
    finally:
        pins.close()

def main(argv):
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

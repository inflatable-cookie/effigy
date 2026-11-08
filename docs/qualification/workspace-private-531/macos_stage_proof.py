"""Nucleus proof-only entry for a single admitted stage.

External precondition: Nucleus launches this exact selector at its pinned commit and
inputs after the native operator GO and records the proof execution. This program
cannot establish that provenance against another process of the same user. A request
file or this program's existence is not an activation.
"""
import datetime
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import macos_stage_executor as executor  # noqa: E402


def main(argv):
    if len(argv) != 3:
        raise executor.Refusal('proof entry requires a request and its SHA-256')
    path = str(Path(argv[1]).resolve())
    if path != argv[1]:
        raise executor.Refusal('proof request path must be canonical')
    raw = executor.read_checked_file(path, must_be_in=executor.prep.STASH, read_only=True, max_bytes=65536)
    if hashlib.sha256(raw).hexdigest() != argv[2]:
        raise executor.Refusal('proof request hash or size mismatch')
    request = json.loads(raw)
    if not isinstance(request, dict) or set(request) != {
        'stage', 'activation_path', 'activation_sha256', 'bindings', 'evidence_root',
        'inputs_sha256', 'manifest_sha256', 'source_sha256',
    }:
        raise executor.Refusal('unsupported proof request')
    stage_id = request['stage']
    if stage_id not in ('C0', 'D0'):
        raise executor.Refusal('proof entry supports only admitted C0 or D0')
    manifest = executor.load_manifest()
    plan = executor.resolve_stage(stage_id, request['bindings'], manifest)
    if plan['static_admission']:
        raise executor.Refusal('stage unadmitted: ' + '; '.join(plan['static_admission']))
    inputs = executor.derive_inputs()
    inputs_hash = executor.canon_sha(inputs)
    if (request['inputs_sha256'] != inputs_hash or
            request['manifest_sha256'] != executor.sha_file(executor.MANIFEST_PATH) or
            request['source_sha256'] != executor.sha_file(executor.__file__)):
        raise executor.Refusal('proof input binding differs')
    activation = executor.read_checked_file(request['activation_path'],
                                             must_be_in=executor.ACTIVATIONS_DIR, read_only=True)
    now = datetime.datetime.now(datetime.timezone.utc)
    executor.check_activation(activation, request['activation_sha256'], stage_id=stage_id,
                              bindings=request['bindings'], manifest=manifest,
                              manifest_path=executor.MANIFEST_PATH, inputs=inputs, now=now)
    fd = executor.private_dir(request['evidence_root'])
    import os
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

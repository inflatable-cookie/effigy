#!/usr/bin/env python3
"""Nucleus proof-only entry for the task-engine real-container signal packet.

Queue task effigy#739 pins this selector (`qualification:task-engine-signals:proof-only`)
for one later source-immutable Nucleus launch.

External precondition: Nucleus launches this exact selector at the exact merged commit,
with the pinned inputs, under already-recorded same-sequence authority, and records that
launch. This program cannot authenticate that provenance against another process of the
same user. A request file, an environment token, or this file's existence is not that
authority, and this entry reads no environment.

The entry does one thing. It turns a bounded, schema-strict, hash-pinned request into the
in-memory armed overlay that the packet GO must match, then calls the existing
`launch.main` path with no transport, which is the production HostCliDriver. The committed
manifest stays unarmed. Ordinary `launch.py --execute` still refuses.
"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import launch  # noqa: E402
from common import (  # noqa: E402
    ALLOWED_KINDS, FORBIDDEN_CONTEXTS, HEX40, HEX64, IMAGE_DIGEST, SCRATCH,
    Refused, digest_bytes, parse_identity, read_exclusive_file,
    require_canonical_scratch, verify_go_receipt, verify_packet_bundle,
)

SCHEMA = 'effigy.task-engine-signal-proof-request.r1'
SELECTOR = 'task-engine-signals'
REQUEST_NAME = 'proof-request.json'
REQUEST_MAX_BYTES = 8192
REQUEST_KEYS = frozenset({
    'schema', 'selector', 'bundle_sha256', 'go_sha256', 'source',
    'binary', 'binary_sha256', 'observer', 'observer_sha256', 'owner_root',
    'engine', 'identity', 'context', 'image_digest', 'window', 'inventory',
})


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise Refused('duplicate proof request key: %s' % key)
        result[key] = value
    return result


def _abs_under(value, root):
    return (isinstance(value, str) and value.startswith('/') and '..' not in Path(value).parts
            and value.startswith(str(root) + '/'))


def _text(obj, key):
    value = obj.get(key)
    if not isinstance(value, str) or not value:
        raise Refused('proof request %s is not a non-empty string' % key)
    return value


def load_request(scratch, path_arg, sha_arg):
    """Read the one pinned request: exact path, bounded, owner-only, hash-matched, strict schema."""
    if not isinstance(sha_arg, str) or not HEX64.fullmatch(sha_arg):
        raise Refused('proof request SHA-256 is not 64 hex digits')
    pinned = Path(scratch) / REQUEST_NAME
    if path_arg != str(pinned):
        raise Refused('proof request is not at its pinned path')
    try:
        raw = read_exclusive_file(pinned, max_bytes=REQUEST_MAX_BYTES)
    except OSError as exc:
        raise Refused('proof request is a symlink or unreadable: %s' % exc) from exc
    if digest_bytes(raw) != sha_arg:
        raise Refused('proof request hash or size mismatch')
    try:
        obj = json.loads(raw.decode('utf-8'), object_pairs_hook=unique_object)
    except (ValueError, UnicodeDecodeError) as exc:
        raise Refused('proof request is not strict JSON') from exc
    if not isinstance(obj, dict) or set(obj) != REQUEST_KEYS:
        raise Refused('proof request keys differ from the pinned schema')
    if obj['schema'] != SCHEMA or obj['selector'] != SELECTOR:
        raise Refused('proof request schema or selector differs')
    if not HEX64.fullmatch(str(obj['bundle_sha256'])) or not HEX64.fullmatch(str(obj['go_sha256'])):
        raise Refused('proof request bundle or GO hash is not 64 hex digits')
    if not HEX40.fullmatch(str(obj['source'])):
        raise Refused('proof request source is not the 40-hex merged commit')
    for key in ('binary', 'observer', 'owner_root', 'inventory'):
        _text(obj, key)
    for key in ('binary_sha256', 'observer_sha256'):
        if not HEX64.fullmatch(str(obj[key])):
            raise Refused('proof request %s is not 64 hex digits' % key)
    if not obj['binary'].startswith('/') or '..' in Path(obj['binary']).parts:
        raise Refused('proof request binary is not an absolute owned path')
    if not obj['observer'].startswith('/') or '..' in Path(obj['observer']).parts:
        raise Refused('proof request observer is not an absolute owned path')
    if not _abs_under(obj['owner_root'], scratch) or not _abs_under(obj['inventory'], scratch):
        raise Refused('proof request owner root or inventory is outside the pinned scratch root')
    engine = _text(obj, 'engine')
    if engine not in ALLOWED_KINDS:
        raise Refused('proof request engine is not a production Compose backend')
    parsed_engine, _full_id, parsed_context = parse_identity(_text(obj, 'identity'))
    if parsed_engine != engine or parsed_context != _text(obj, 'context'):
        raise Refused('proof request identity does not match engine and context')
    if _text(obj, 'context') in FORBIDDEN_CONTEXTS:
        raise Refused('proof request context is the operator profile')
    if not IMAGE_DIGEST.fullmatch(_text(obj, 'image_digest')):
        raise Refused('proof request image-digest is not sha256:<64-hex>')
    _text(obj, 'window')
    return obj


def compare_go(request, go, bundle_sha, go_sha):
    """Every identity in the request must equal the GO receipt, and the GO must name this packet."""
    if request['bundle_sha256'] != bundle_sha or go['bundle_sha256'] != bundle_sha:
        raise Refused('proof request names a different packet bundle')
    if request['go_sha256'] != go_sha:
        raise Refused('proof request names a different GO receipt')
    pairs = (
        ('source', 'source'), ('binary', 'binary'), ('binary_sha256', 'binary_sha256'),
        ('observer', 'observer'), ('observer_sha256', 'observer_sha256'),
        ('owner_root', 'owner_root'), ('engine', 'engine'), ('identity', 'identity'),
        ('context', 'context'), ('image_digest', 'image_digest'), ('window', 'window'),
        ('inventory', 'inventory'),
    )
    for request_key, go_key in pairs:
        if request[request_key] != go[go_key]:
            raise Refused('proof request %s differs from GO' % request_key)


def build_overlay(request):
    """The in-memory armed overlay, built only from request fields that equal the GO."""
    return {
        'armed': True,
        'candidate': {
            'source_commit': request['source'],
            'binary_path': request['binary'],
            'binary_sha256': request['binary_sha256'],
            'observer_path': request['observer'],
            'observer_sha256': request['observer_sha256'],
            'owner_root': request['owner_root'],
        },
        'engine': {
            'kind': request['engine'],
            'identity': request['identity'],
            'context': request['context'],
            'image_digest': request['image_digest'],
        },
    }


def run(argv, scratch_dir=None, transport=None):
    """Return launch.main's exit code. `transport` is for tests only; the CLI passes None."""
    argv = list(argv)
    if len(argv) != 2:
        raise Refused('proof entry requires a request path and its SHA-256')
    scratch = Path(scratch_dir) if scratch_dir is not None else SCRATCH
    require_canonical_scratch(scratch)
    bundle = verify_packet_bundle()
    request = load_request(scratch, argv[0], argv[1])
    go = verify_go_receipt(scratch / 'go.txt', bundle['bundle_sha256'], scratch_dir=scratch)
    compare_go(request, go, bundle['bundle_sha256'], go['sha256'])
    return launch.main(
        ['--execute'], scratch_dir=scratch, overlay=build_overlay(request), transport=transport,
    )


if __name__ == '__main__':
    try:
        raise SystemExit(run(sys.argv[1:]))
    except Refused as exc:
        sys.stderr.write('refused: %s\n' % exc)
        raise SystemExit(2)

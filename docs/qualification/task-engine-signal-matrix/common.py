"""Fail-closed machinery for the task-engine signal-matrix packet (effigy#579).

Preparation only. Nothing here starts a VM, engine, container, image pull,
install, or native task. Admission tests drive these verifiers against
disposable fixture metadata. A constructed plan is not qualification.
"""
import hashlib
import json
import os
import re
import stat
from pathlib import Path

DIR = Path(__file__).resolve().parent
SCRATCH = Path('/Volumes/Builds/scratch/effigy/579')
MANIFEST = DIR / 'manifest-r1.json'
SCHEMA = 'effigy.qualification.task-engine-signal-matrix.r1'
UNARMED = 'UNARMED'
HEX64 = re.compile(r'^[0-9a-f]{64}$')
HEX40 = re.compile(r'^[0-9a-f]{40}$')
IMAGE_DIGEST = re.compile(r'^sha256:[0-9a-f]{64}$')
ALLOWED_KINDS = ('colima-nerdctl', 'docker-compose')
FORBIDDEN_CONTEXTS = ('effigy',)
FAKE_BACKENDS = ('fake', 'TEST_REMOTE', 'model', 'in-process')
GO_TOKENS = (
    'GO',
    'Tom',
    'task-engine-signals',
)
FORBIDDEN_TEXT = (
    'colima start',
    'colima stop',
    'colima restart',
    'docker pull',
    'docker build',
    'nerdctl pull',
    'nerdctl build',
    'install:local',
    'pkill',
    'killall',
    'task#531',
    'task#574',
    'effigy#531',
    'effigy#574',
)
SIGNAL_CELLS = (
    ('container-int', 'INT'),
    ('container-term', 'TERM'),
    ('container-hup', 'HUP'),
    ('container-kill', 'KILL'),
)


class Refused(Exception):
    """A fail-closed refusal with the exact reason recorded in evidence."""


def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()


def digest_file(path):
    return digest_bytes(Path(path).read_bytes())


def bundle_digest(manifest=None):
    """Live SHA-256 of named helpers: sorted name + NUL + content."""
    obj = manifest if manifest is not None else load_manifest()
    names = obj.get('helpers')
    if not isinstance(names, list) or not names:
        raise Refused('manifest lacks the helpers list')
    h = hashlib.sha256()
    for name in sorted(names):
        if not isinstance(name, str) or '/' in name or name in ('.', '..'):
            raise Refused('helper name is not a plain file: %s' % name)
        path = DIR / name
        if path.is_symlink() or not path.is_file():
            raise Refused('committed helper missing or symlink: ' + name)
        h.update(name.encode('utf-8'))
        h.update(b'\0')
        h.update(path.read_bytes())
    return h.hexdigest()


def load_manifest():
    if MANIFEST.is_symlink() or not MANIFEST.is_file():
        raise Refused('committed manifest missing')
    obj = json.loads(MANIFEST.read_text())
    if obj.get('schema') != SCHEMA:
        raise Refused('manifest schema mismatch: %s' % obj.get('schema'))
    return obj


def verify_packet_bundle():
    obj = load_manifest()
    if obj.get('status') != 'prepared-not-executed':
        raise Refused('manifest status is not prepared-not-executed')
    if obj.get('armed') is not False:
        raise Refused('committed packet must stay unarmed')
    if obj.get('qualification') is not False:
        raise Refused('committed packet must not claim qualification')
    if obj.get('scratch_root') != str(SCRATCH):
        raise Refused('scratch root is not the owned 579 path')
    for name in obj['helpers']:
        path = DIR / name
        if path.is_symlink() or not path.is_file():
            raise Refused('committed helper missing or symlink: ' + name)
    sha = bundle_digest(obj)
    return {'bundle_sha256': sha, 'manifest': obj}


def read_exclusive_file(path, max_bytes=8192):
    p = Path(path)
    if not os.path.lexists(p):
        raise Refused('receipt file missing: %s' % p)
    fd = os.open(p, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_CLOEXEC', 0))
    try:
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode) or st.st_uid != os.getuid() or st.st_nlink != 1 \
                or st.st_mode & 0o077:
            raise Refused('receipt file mode/owner/type invalid: %s' % p)
        if st.st_size > max_bytes:
            raise Refused('receipt file exceeds %d bytes: %s' % (max_bytes, p))
        data = os.read(fd, max_bytes + 1)
        if len(data) > max_bytes:
            raise Refused('receipt file exceeds %d bytes: %s' % (max_bytes, p))
    finally:
        os.close(fd)
    return data


def parse_identity(encoded):
    if not isinstance(encoded, str) or encoded.count(':') != 1 or encoded.count('@') != 1:
        raise Refused('identity is not engine:full-id@context')
    engine, rest = encoded.split(':', 1)
    full_id, context = rest.split('@', 1)
    if engine not in ALLOWED_KINDS:
        raise Refused('engine kind is not a production Compose backend: %s' % engine)
    if engine in FAKE_BACKENDS or full_id in FAKE_BACKENDS:
        raise Refused('fake engine is not real-engine credit')
    if not HEX64.fullmatch(full_id):
        raise Refused('container full id is not 64 hex digits')
    if not context or context in FORBIDDEN_CONTEXTS:
        raise Refused('context is forbidden or empty: %s' % context)
    return engine, full_id, context


def _line(text, prefix):
    match = re.search(r'^' + re.escape(prefix) + r' (.+)$', text, re.M)
    if not match:
        raise Refused('GO receipt lacks %r line' % prefix)
    return match.group(1).strip()


def verify_go_receipt(path, bundle_sha, scratch_dir=SCRATCH):
    scratch = Path(scratch_dir)
    if Path(path) != scratch / 'go.txt' or Path(path).is_symlink():
        raise Refused('GO receipt is not at its exact pinned scratch path')
    data = read_exclusive_file(path)
    text = data.decode('utf-8')
    match = re.search(r'^packet ([0-9a-f]{64})$', text, re.M)
    if not match or match.group(1) != bundle_sha:
        raise Refused('GO receipt does not name this packet SHA')
    for token in GO_TOKENS:
        if token not in text.split():
            raise Refused('GO receipt lacks required token %r' % token)
    lowered = text.lower()
    for fragment in FORBIDDEN_TEXT:
        if fragment.lower() in lowered:
            raise Refused('GO receipt names a forbidden action: %s' % fragment)
    engine = _line(text, 'engine')
    if engine in FAKE_BACKENDS:
        raise Refused('fake engine is not real-engine credit')
    if engine not in ALLOWED_KINDS:
        raise Refused('engine kind is not a production Compose backend: %s' % engine)
    identity = _line(text, 'engine-id')
    parsed_engine, _full_id, parsed_context = parse_identity(identity)
    if parsed_engine != engine:
        raise Refused('engine-id engine does not match engine line')
    context = _line(text, 'context')
    if context != parsed_context:
        raise Refused('context line does not match engine-id')
    if context in FORBIDDEN_CONTEXTS:
        raise Refused('context is the operator Colima profile; #531 resources are off-limits')
    image = _line(text, 'image-digest')
    if not IMAGE_DIGEST.fullmatch(image):
        raise Refused('image-digest is not sha256:<64-hex>; missing image is a preparation prerequisite')
    binary = _line(text, 'binary')
    if not binary.startswith('/') or '..' in Path(binary).parts:
        raise Refused('binary path is not an absolute owned path')
    binary_sha = _line(text, 'binary-sha256')
    if not HEX64.fullmatch(binary_sha):
        raise Refused('binary-sha256 is not 64 hex digits')
    source = _line(text, 'source')
    if not HEX40.fullmatch(source):
        raise Refused('source is not a 40-hex commit')
    window = _line(text, 'window')
    if not window:
        raise Refused('window is empty')
    inventory = _line(text, 'inventory')
    inv = Path(inventory)
    if not str(inv).startswith(str(scratch) + os.sep):
        raise Refused('inventory path is not under the owned scratch root')
    return {
        'path': str(path),
        'sha256': digest_bytes(data),
        'bundle_sha256': bundle_sha,
        'engine': engine,
        'identity': identity,
        'context': context,
        'image_digest': image,
        'binary': binary,
        'binary_sha256': binary_sha,
        'source': source,
        'window': window,
        'inventory': inventory,
    }


def _has_unarmed(obj):
    if obj == UNARMED:
        return True
    if isinstance(obj, dict):
        return any(_has_unarmed(v) for v in obj.values())
    if isinstance(obj, list):
        return any(_has_unarmed(v) for v in obj)
    return False


def require_armed(manifest):
    if manifest.get('armed') is not True:
        raise Refused('packet remains unarmed')
    if _has_unarmed(manifest.get('candidate')) or _has_unarmed(manifest.get('engine')):
        raise Refused('packet remains unarmed: UNARMED placeholders')
    engine = manifest.get('engine') or {}
    kind = engine.get('kind')
    if kind in FAKE_BACKENDS:
        raise Refused('fake engine is not real-engine credit')
    if kind not in ALLOWED_KINDS:
        raise Refused('engine kind is not a production Compose backend: %s' % kind)
    parse_identity(engine.get('identity'))
    if engine.get('context') in FORBIDDEN_CONTEXTS:
        raise Refused('context is the operator Colima profile; #531 resources are off-limits')
    if not IMAGE_DIGEST.fullmatch(str(engine.get('image_digest') or '')):
        raise Refused('image-digest is not sha256:<64-hex>; missing image is a preparation prerequisite')
    candidate = manifest.get('candidate') or {}
    if not HEX40.fullmatch(str(candidate.get('source_commit') or '')):
        raise Refused('source_commit is not a 40-hex commit')
    if not HEX64.fullmatch(str(candidate.get('binary_sha256') or '')):
        raise Refused('binary_sha256 is not 64 hex digits')
    binary = candidate.get('binary_path')
    if not isinstance(binary, str) or not binary.startswith('/'):
        raise Refused('binary_path is not an absolute owned path')


def apply_overlay(manifest, overlay):
    out = json.loads(json.dumps(manifest))
    if not overlay:
        return out
    for key in ('candidate', 'engine'):
        if key in overlay and isinstance(overlay[key], dict):
            out.setdefault(key, {}).update(overlay[key])
    if 'armed' in overlay:
        out['armed'] = overlay['armed']
    return out


def _scan_forbidden(plan):
    probe = {
        'argv': plan.get('argv'),
        'binary': plan.get('binary'),
        'identity': plan.get('identity'),
        'engine_kind': plan.get('engine_kind'),
        'context': plan.get('context'),
        'cells': plan.get('cells'),
        'scratch': plan.get('scratch'),
    }
    blob = json.dumps(probe).lower()
    for fragment in FORBIDDEN_TEXT:
        if fragment.lower() in blob:
            raise Refused('plan names a forbidden action: %s' % fragment)


def construct_plan(manifest, overlay=None):
    """In-process owned command plan. Never spawns."""
    m = apply_overlay(manifest, overlay)
    require_armed(m)
    engine = m['engine']
    candidate = m['candidate']
    parsed_engine, full_id, context = parse_identity(engine['identity'])
    if parsed_engine != engine['kind'] or context != engine['context']:
        raise Refused('engine identity does not match kind/context')
    fixture = m.get('fixture_task') or {}
    if fixture.get('run_in') != 'container' or fixture.get('discard') or \
            fixture.get('workspace_private_apply'):
        raise Refused('fixture is not an ordinary run_in=container task')
    deadlines = m.get('deadlines_s') or {}
    if deadlines != {'ack': 60, 'call': 20, 'grace': 3, 'signal_exit': 10}:
        raise Refused('deadlines are not the original remote budgets')
    closure = m.get('closure') or {}
    if closure.get('infer_from_owner_exit') or closure.get('infer_from_empty_list') \
            or closure.get('infer_from_fake_transport') or closure.get('infer_from_reused_pid') \
            or closure.get('infer_from_label_match') or closure.get('infer_from_no_output') \
            or closure.get('client_exit_is_remote_closure'):
        raise Refused('closure rules infer remote end from a forbidden proxy')
    if closure.get('require_proc_helper_identity_recheck') is not True:
        raise Refused('plan omits /proc helper identity recheck')
    cells = []
    for cell_id, signal in SIGNAL_CELLS:
        cells.append({
            'id': cell_id,
            'kind': 'container',
            'signal': signal,
            'adapter': 'ComposeRemote<HostCli>',
            'identity': engine['identity'],
        })
    cells.append({
        'id': 'container-foreign-unknown',
        'kind': 'container',
        'signal': None,
        'obligation': 'strict foreign/unknown refusal',
        'adapter': 'ComposeRemote<HostCli>',
        'identity': engine['identity'],
    })
    plan = {
        'spawn': False,
        'qualification': False,
        'execute': False,
        'adapter': 'ComposeRemote<HostCli>',
        'call_path': 'src/runner/task_engine/remote.rs::run_remote',
        'identity_format': 'engine:full-id@context',
        'identity': engine['identity'],
        'engine_kind': engine['kind'],
        'context': engine['context'],
        'full_id': full_id,
        'image_digest': engine['image_digest'],
        'deadlines_s': dict(deadlines),
        'scratch': str(SCRATCH),
        'binary': candidate['binary_path'],
        'binary_sha256': candidate['binary_sha256'],
        'source': candidate['source_commit'],
        'argv': [candidate['binary_path'], fixture.get('selector', 'hold')],
        'fixture_task': {
            'selector': fixture.get('selector', 'hold'),
            'run_in': 'container',
            'discard': False,
            'workspace_private_apply': False,
        },
        'cells': cells,
        'closure': dict(closure),
        'forbidden': list(m.get('forbidden') or []),
    }
    _scan_forbidden(plan)
    return plan

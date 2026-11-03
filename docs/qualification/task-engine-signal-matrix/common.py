"""Fail-closed machinery for the task-engine signal-matrix packet.

Queue tasks effigy#579 (index + unarmed packet), effigy#582 (executable
path) and effigy#585 (real HostCli identity/generation binding). Admission
and plan construction never start a VM, engine, container, image pull,
install, or native task. A constructed plan is not execution. `--execute`
is a real pass after exact GO/hash/engine/image/root admission; this
module stays spawn-free. Native HostCli lives in execute.py.
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
HELPER_NAME = 'effigy-custody'
NONCE_HEX_LEN = 64
STATUS_SCHEMA = 'effigy.tasks-status.v3'
REMOTE_RS = DIR.parents[2] / 'crates' / 'effigy-containers' / 'src' / 'environment' / 'remote.rs'
ATTEMPT_RECEIPT_KEYS = (
    'invocation',
    'inventory_pre',
    'inventory_post',
    'cleanup',
    'error',
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
    observer = _line(text, 'observer')
    observer_sha = _line(text, 'observer-sha256')
    owner_root = _line(text, 'owner-root')
    if not observer.startswith('/') or '..' in Path(observer).parts or not HEX64.fullmatch(observer_sha):
        raise Refused('observer path or hash is invalid')
    if not owner_root.startswith(str(scratch) + os.sep) or '..' in Path(owner_root).parts:
        raise Refused('owner root is not under the owned scratch root')
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
        'observer': observer,
        'observer_sha256': observer_sha,
        'owner_root': owner_root,
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
    if not isinstance(candidate.get('observer_path'), str) or not candidate['observer_path'].startswith('/'):
        raise Refused('observer_path is not an absolute owned path')
    if not HEX64.fullmatch(str(candidate.get('observer_sha256') or '')):
        raise Refused('observer_sha256 is not 64 hex digits')
    if not isinstance(candidate.get('owner_root'), str) or not candidate['owner_root'].startswith('/'):
        raise Refused('owner_root is not an absolute owned path')


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


def match_go(manifest, go):
    """GO must quote the same exact inputs the working packet is armed with."""
    engine = manifest.get('engine') or {}
    candidate = manifest.get('candidate') or {}
    if engine.get('kind') != go['engine']:
        raise Refused('GO engine does not match packet')
    if engine.get('identity') != go['identity']:
        raise Refused('GO engine-id does not match packet')
    if engine.get('context') != go['context']:
        raise Refused('GO context does not match packet')
    if engine.get('image_digest') != go['image_digest']:
        raise Refused('GO image-digest does not match packet')
    if candidate.get('binary_path') != go['binary']:
        raise Refused('GO binary does not match packet')
    if candidate.get('binary_sha256') != go['binary_sha256']:
        raise Refused('GO binary-sha256 does not match packet')
    for field, key in (('observer_path', 'observer'), ('observer_sha256', 'observer_sha256'), ('owner_root', 'owner_root')):
        if candidate.get(field) != go[key]:
            raise Refused('GO %s does not match packet' % key)
    if candidate.get('source_commit') != go['source']:
        raise Refused('GO source does not match packet')


def verify_binary(path, expected_sha):
    p = Path(path)
    if p.is_symlink() or not p.is_file():
        raise Refused('candidate binary missing or symlink')
    actual = digest_file(p)
    if actual != expected_sha:
        raise Refused('candidate binary sha256 mismatch')
    return actual


def receipt_exclusive(path, data):
    """Write a 0600 O_EXCL receipt. An existing file is a prior attempt: no retry."""
    p = Path(path)
    if p.is_symlink() or os.path.lexists(p):
        raise Refused('already attempted; no retry: %s' % p.name)
    payload = data if isinstance(data, (bytes, bytearray)) else (
        json.dumps(data, indent=2, sort_keys=True) + '\n'
    ).encode()
    fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    try:
        os.write(fd, payload)
        os.fsync(fd)
    finally:
        os.close(fd)
    os.chmod(p, 0o600)
    return digest_file(p)


def refuse_prior_attempt(scratch, manifest):
    receipts = manifest.get('receipts') or {}
    for key in ATTEMPT_RECEIPT_KEYS:
        name = receipts.get(key)
        if not name:
            continue
        path = Path(scratch) / name
        if path.is_symlink() or os.path.lexists(path):
            raise Refused('already attempted; no retry: %s' % name)


def docker_target(context):
    flag = '--host' if '://' in context else '--context'
    return [flag, context]


def runtime_argv(kind, context, tail):
    """HostCli argv frame from ComposeRemote::runtime. Never a fake engine."""
    if kind == 'colima-nerdctl':
        return 'colima', ['nerdctl', '--profile', context, '--'] + list(tail)
    if kind == 'docker-compose':
        return 'docker', docker_target(context) + list(tail)
    raise Refused('engine kind is not a production Compose backend: %s' % kind)


def production_helper():
    """The shipped in-container /proc helper body. Refuse if it cannot be read."""
    if REMOTE_RS.is_symlink() or not REMOTE_RS.is_file():
        raise Refused('production remote.rs missing')
    text = REMOTE_RS.read_text()
    marker = 'pub(crate) const HELPER: &str = r#"'
    start = text.find(marker)
    if start < 0:
        raise Refused('production HELPER constant missing')
    start += len(marker)
    end = text.find('"#;', start)
    if end < 0:
        raise Refused('production HELPER constant unterminated')
    body = text[start:end]
    if '/proc/' not in body or 'boot_id' not in body:
        raise Refused('production HELPER is not the /proc identity helper')
    return body


def helper_tail(full_id, mode_args, helper_script=None):
    script = helper_script if helper_script is not None else production_helper()
    tail = ['exec', '-i', '-u', '0', full_id, 'sh', '-c', script, HELPER_NAME]
    tail.extend(mode_args)
    return tail


def helper_kill_tail(full_id, boot, ns, sig, kind, pid, start, helper_script=None):
    """Helper kill argv using observed boot+namespace, never literal placeholders."""
    if not boot or not ns or ns in ('ns', 'boot') or boot == 'boot':
        raise Refused('helper-kill lacks observed generation')
    if not str(ns).startswith('pid:[') or not str(ns).endswith(']'):
        raise Refused('helper-kill namespace is not pid:[N]')
    return helper_tail(
        full_id,
        ['kill', str(boot), str(ns), str(sig), str(kind), str(pid), str(start)],
        helper_script,
    )


def generate_nonce():
    """64 hex digits, the same width as the kernel remote Digest nonce."""
    return os.urandom(NONCE_HEX_LEN // 2).hex()


def decode_output(value):
    if value is None:
        return ''
    if isinstance(value, bytes):
        return value.decode('utf-8', 'replace')
    return str(value)


def parse_inspect(stdout, full_id, image_digest):
    """Production inspect JSON: PascalCase Id/Created/State.Running plus image.

    Exit 0 is not verification. Unparsable, the wrong id, a stopped container,
    or an image that is not the GO digest returns None.
    """
    try:
        rows = json.loads(decode_output(stdout))
    except (TypeError, ValueError, json.JSONDecodeError):
        return None
    if not isinstance(rows, list) or len(rows) != 1 or not isinstance(rows[0], dict):
        return None
    row = rows[0]
    ident = row.get('Id')
    if ident != full_id:
        return None
    created = row.get('Created')
    if not isinstance(created, str) or not created or any(ch.isspace() for ch in created):
        return None
    state = row.get('State')
    if not isinstance(state, dict) or state.get('Running') is not True:
        return None
    config = row.get('Config') if isinstance(row.get('Config'), dict) else {}
    images = (row.get('Image'), row.get('ImageID'), config.get('Image'))
    if image_digest not in images:
        return None
    return {
        'id': ident,
        'created': created,
        'running': True,
        'image': image_digest,
    }


def parse_snapshot(stdout):
    """Complete helper inventory: boot, pid:[N], and every `p` line.

    None is failed-observe. A verified empty inventory still has boot+ns and
    members=[] only in that verified case.
    """
    boot = None
    ns = None
    members = []
    seen = set()
    text = decode_output(stdout)
    for line in text.splitlines():
        fields = line.split(' ')
        if len(fields) == 2 and fields[0] == 'boot' and fields[1]:
            if boot is not None:
                return None
            boot = fields[1]
            continue
        if len(fields) == 2 and fields[0] == 'ns' and fields[1]:
            if ns is not None:
                return None
            ns = fields[1]
            continue
        if len(fields) == 8 and fields[0] == 'p':
            pid, state, ppid, pgrp, sid, start, nonce = fields[1:]
            if nonce not in ('y', 'n', '?', '-'):
                return None
            try:
                if (int(pid) <= 0 or int(ppid) < 0 or int(pgrp) <= 0 or int(sid) <= 0
                        or int(start) <= 0 or pid in seen or len(state) != 1):
                    return None
            except ValueError:
                return None
            seen.add(pid)
            members.append({
                'pid': pid,
                'state': state,
                'ppid': ppid,
                'pgrp': pgrp,
                'sid': sid,
                'start': start,
                'nonce': nonce,
            })
            continue
        if line.strip():
            return None
    if not boot or not ns:
        return None
    if not ns.startswith('pid:[') or not ns.endswith(']'):
        return None
    try:
        pid_ns = int(ns[5:-1])
    except ValueError:
        return None
    if pid_ns <= 0:
        return None
    return {
        'boot': boot,
        'ns': ns,
        'pid_ns': pid_ns,
        'members': members,
    }


def observe_snapshot(result, nonce_supplied=False):
    """Distinguish failed-observe from a verified empty inventory."""
    stdout = result.get('stdout') if isinstance(result, dict) else None
    code = result.get('code') if isinstance(result, dict) else None
    kind = result.get('kind') if isinstance(result, dict) else None
    base = {
        'stdout': decode_output(stdout),
        'code': code,
        'kind': kind,
    }
    if kind != 'verified' or code not in (0, None):
        base['observe'] = 'failed'
        base['reason'] = 'snap did not verify'
        return base
    parsed = parse_snapshot(stdout)
    if parsed is None:
        base['observe'] = 'failed'
        base['reason'] = 'snap unparsable or incomplete'
        return base
    if nonce_supplied:
        for member in parsed['members']:
            if member['nonce'] not in ('y', 'n'):
                base['observe'] = 'failed'
                base['reason'] = 'snap nonce unread'
                return base
    base['observe'] = 'verified'
    base.update(parsed)
    return base


def parse_status(stdout):
    """tasks-status.v3 JSON. Substring 'not-recorded' is not a gate."""
    try:
        obj = json.loads(decode_output(stdout))
    except (TypeError, ValueError, json.JSONDecodeError):
        return None
    if not isinstance(obj, dict) or obj.get('schema') != STATUS_SCHEMA:
        return None
    state = obj.get('state')
    if not isinstance(state, str) or not state:
        return None
    return {'schema': obj['schema'], 'state': state}


def owned_members(members):
    return [
        m for m in members
        if m.get('nonce') == 'y' and m.get('state') not in ('Z', 'X')
    ]


def foreign_members(members):
    return [m for m in members if m.get('nonce') != 'y']


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
        'observer': candidate['observer_path'],
        'observer_sha256': candidate['observer_sha256'],
        'owner_root': candidate['owner_root'],
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

"""Pure macOS (Colima + Docker Desktop) preparation controls. No effects on import.

Every function takes injected observations and returns a decision or raises.
Native actions live in the staged manifest (macos-4f6a5ed3-stages.json) and are
activated only by a fresh GO. Nothing here starts, stops, connects or writes outside
the paths it is given.
"""
import datetime
import re

STASH = '/Volumes/Stash/scratch/effigy'
BUILDS = '/Volumes/Builds'
PROFILE = 'effigy-4f6a-qual'
PROTECTED_PROFILES = frozenset({'effigy', 'effigy-release', 'effigy-531-qualification'})
DECISION = '63c338e2-1c50-435e-9783-e78e9d8558e6'
DECISION_VERSION = 2
# macOS sun_path is 104 bytes including the NUL: encoded paths must stay below it.
SOCKET_LIMIT_BYTES = 104
STAGE_CAP_SECONDS = 1200
OUTPUT_CAP_BYTES = 256 * 1024
TOKEN = re.compile(r'[0-9a-f]{8}')
SHA = re.compile(r'[0-9a-f]{64}')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def control_paths(token):
    """Fresh owned Colima paths for one token. Pure: no filesystem access."""
    require(TOKEN.fullmatch(token or ''), 'eight lowercase hex token required')
    require(PROFILE not in PROTECTED_PROFILES, 'planned profile must not be a protected backend')
    control = f'{STASH}/mac-4f6a-{token}'
    home = f'{control}/c'
    lima = f'{home}/_lima'
    instance = f'colima-{PROFILE}'
    return dict(
        control=control,
        colima_home=home,
        shared_root=f'{control}/s',
        profile=PROFILE,
        instance=instance,
        lima_ha_socket=f'{lima}/{instance}/ha.sock',
        lima_ga_socket=f'{lima}/{instance}/ga.sock',
        docker_socket=f'{home}/{PROFILE}/docker.sock',
    )


def check_paths(paths, existing):
    """Refuse absent-by-construction violations: Builds, aliases, collisions, long sockets."""
    for key, value in paths.items():
        if key == 'profile' or key == 'instance':
            continue
        require(value.startswith(STASH + '/'), f'{key} must live under Stash scratch')
        require(not value.startswith(BUILDS), f'{key} must not use Builds')
        require('..' not in value.split('/'), f'{key} must be canonical')
        require(value not in existing, f'{key} collides with an existing path')
    for key in ('lima_ha_socket', 'lima_ga_socket', 'docker_socket'):
        require(len(paths[key].encode('utf-8')) < SOCKET_LIMIT_BYTES,
                f'{key} exceeds the Unix socket limit')
    require(paths['profile'] == PROFILE, 'planned profile only')
    return paths


def check_profiles(listed):
    """listed: profile names from local `colima list`. The planned name must be fresh."""
    require(PROFILE not in listed, 'planned profile already exists: refuse, do not adopt')
    return True


def check_root(observed, expected):
    """observed/expected: dict(device, inode, uid, mode, realpath, path, entries)."""
    require(observed['realpath'] == observed['path'], 'alias or symlink qualification root')
    require(observed['mode'] == 0o700, 'qualification root must be 0700')
    require(observed['uid'] == expected['uid'], 'qualification root owned by another user')
    require(observed['entries'] == 0, 'qualification root must be empty')
    require((observed['device'], observed['inode']) == (expected['device'], expected['inode']),
            'qualification root identity changed')
    return True


def check_engine(observed, paths):
    """Colima engine identity from local observation: planned profile, expected home only."""
    require(observed['profile'] == paths['profile'], 'engine profile mismatch')
    require(observed['colima_home'] == paths['colima_home'], 'engine home mismatch')
    require(observed.get('lima_home', paths['colima_home'] + '/_lima') == paths['colima_home'] + '/_lima',
            'foreign LIMA_HOME')
    require(observed['shares'] in ([], [paths['shared_root']]), 'host shares beyond the planned set')
    return True


def check_desktop_resources(resources, token, context):
    """Split Docker Desktop resources into owned (by exact label) and foreign (never touched)."""
    require(context['name'] == 'desktop-linux', 'only the local desktop-linux context is observed')
    require(context['host'] == 'unix:///Users/tom/.docker/run/docker.sock', 'unexpected endpoint')
    label = f'effigy.qual={token}'
    owned, foreign = [], []
    for item in resources:
        (owned if label in item.get('labels', []) and item.get('name', '').startswith('effigy-4f6a-')
         else foreign).append(item['id'])
    return dict(owned=sorted(owned), foreign=sorted(foreign))


def check_drift(pinned, observed):
    """Any pinned hash or identity that changed refuses the stage. Keys are compared exactly."""
    drift = sorted(k for k, v in pinned.items() if observed.get(k) != v)
    require(not drift, 'drift: ' + ', '.join(drift))
    return True


def parse_utc(value):
    return datetime.datetime.fromisoformat(value.replace('Z', '+00:00'))


def check_go(go, *, stage, manifest_sha, source_sha, inputs_sha, now, consumed):
    """A fresh per-window GO: names the standing decision, the stage, hashes and expiry."""
    require(go.get('decision') == DECISION and go.get('decision_version') == DECISION_VERSION,
            'standing decision v2 required')
    require(go.get('stage') == stage, 'GO names a different stage')
    require(go.get('manifest_sha256') == manifest_sha, 'manifest drift')
    require(go.get('source_sha256') == source_sha, 'source drift')
    require(go.get('inputs_sha256') == inputs_sha, 'input drift')
    require(go.get('activation_id') not in consumed, 'consumed activation reused')
    require(not str(go.get('activation_id', '')).startswith('531'), 'old 531 marker is consumed history')
    issued, expires = parse_utc(go['issued_utc']), parse_utc(go['expires_utc'])
    require(issued <= now < expires, 'GO outside its validity window')
    require((expires - issued).total_seconds() <= STAGE_CAP_SECONDS, 'GO window exceeds the stage cap')
    require(go.get('scope') == 'single-activation', 'blanket authority is not a GO')
    return True


def check_stage(stage):
    """Bounds: every command argv list, numeric timeouts, output caps, zero retries, stage cap."""
    total = stage['seconds']
    require(0 < total <= STAGE_CAP_SECONDS, 'stage over the 1200 second cap')
    require(stage['reserve_seconds'] >= 60, 'admission and report reserve required')
    step_total = 0
    for step in stage['steps']:
        argv = step['argv']
        require(isinstance(argv, list) and argv and all(isinstance(a, str) for a in argv),
                f'{step["id"]}: argv must be a list, never a shell string')
        require(step['timeout_seconds'] > 0, f'{step["id"]}: numeric timeout required')
        require(step['output_cap_bytes'] <= OUTPUT_CAP_BYTES, f'{step["id"]}: output cap too large')
        require(step.get('retries', 0) == 0, f'{step["id"]}: no retries')
        step_total += step['timeout_seconds']
    require(step_total <= total - stage['reserve_seconds'], 'step deadlines exceed the stage budget')
    return True


def check_receipt(receipt, expected_sha, expected_bytes):
    """Placement needs a complete, byte-identical receipt. Partial or synthesized receipts refuse."""
    for key in ('sha256', 'bytes', 'owner_uid', 'mode', 'readback_sha256', 'fsynced'):
        require(key in receipt, f'receipt missing {key}')
    require(receipt['sha256'] == expected_sha and receipt['readback_sha256'] == expected_sha,
            'receipt hash does not match the source bytes')
    require(receipt['bytes'] == expected_bytes, 'receipt size mismatch')
    require(receipt['fsynced'] is True, 'receipt must be fsynced')
    require(not receipt.get('synthesized', False), 'producer receipts are not synthesized')
    return True


def classify_closure(record):
    """Closed only with a waited exit and no cap or deadline. A host-side kill does not prove
    remote closure, so a deadline or cap is Unknown and retained; Unknown never replays."""
    if record.get('waited_exit') is None:
        return 'unknown_retained'
    if record.get('outcome') in ('deadline_exceeded', 'output_cap_exceeded'):
        return 'unknown_retained'
    return 'closed' if record['waited_exit'] == 0 else 'failed_closed'


def check_intent(intent_id, claimed):
    """One intent per activation; a reused intent id refuses before any effect."""
    require(intent_id not in claimed, 'intent reused: refuse before effects')
    return True

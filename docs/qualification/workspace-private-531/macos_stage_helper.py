"""Fixed C0/D0 helper entry. Authority remains with the supervising stage."""
import hashlib
import json
import os
from pathlib import Path
import stat
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import macos_stage_executor as executor  # noqa: E402

ALLOWED = {
    'C0': {'c0-verify-tools', 'c0-verify-tranche', 'c0-claim-intent', 'c0-make-owned-roots'},
    'D0': {'d0-make-owned-dirs', 'd0-verify-endpoint'},
}


def created_directories(request, step):
    anchor = request['anchor']
    fd = request['anchor_fd']
    info = os.fstat(fd)
    path_info = os.lstat(anchor)
    if ([info.st_dev, info.st_ino, info.st_uid, stat.S_IMODE(info.st_mode)] != request['anchor_identity']
            or (info.st_dev, info.st_ino) != (path_info.st_dev, path_info.st_ino)
            or not stat.S_ISDIR(info.st_mode) or os.path.realpath(anchor) != anchor
            or not (anchor == executor.prep.STASH or anchor.startswith(executor.prep.STASH + '/'))):
        raise executor.Refusal('anchor descriptor changed')
    bindings = request['bindings']
    token = bindings.get('token')
    if not isinstance(token, str) or not executor.prep.TOKEN.fullmatch(token):
        raise executor.Refusal('invalid token')
    control = bindings.get('control')
    if (not isinstance(control, str) or os.path.basename(control) != 'mac-4f6a-' + token
            or anchor != (os.path.dirname(control) if request['stage'] == 'C0' else control)):
        raise executor.Refusal('control and anchor differ from the fixed operation')
    required = {'colima_home': control + '/c', 'qualification_root': control + '/s',
                'desktop_config': control + '/d/docker-config', 'desktop_context_dir': control + '/d/ctx'}
    if any(bindings.get(key) != path for key, path in required.items()):
        raise executor.Refusal('owned directory bindings differ from the fixed operation')
    targets = [executor.render1(ref, request['bindings']) for ref in step['inputs']]
    opened = {'': os.dup(fd)}
    created = []
    try:
        for target in targets:
            relative = os.path.relpath(target, anchor)
            parts = relative.split(os.sep)
            if not parts or any(part in ('', '.', '..') for part in parts):
                raise executor.Refusal('directory target escapes anchor')
            parent = ''
            for part in parts:
                path = os.path.join(parent, part) if parent else part
                if path not in opened:
                    os.mkdir(part, 0o700, dir_fd=opened[parent])
                    os.fsync(opened[parent])
                    child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=opened[parent])
                    item = os.fstat(child)
                    if (not stat.S_ISDIR(item.st_mode) or item.st_uid != os.geteuid()
                            or stat.S_IMODE(item.st_mode) != 0o700):
                        raise executor.Refusal('created directory identity invalid')
                    opened[path] = child
                    created.append({'path': os.path.join(anchor, path), 'identity': [item.st_dev, item.st_ino,
                                                                                   item.st_uid, stat.S_IMODE(item.st_mode)]})
                parent = path
        return {'created': created}
    finally:
        for handle in opened.values():
            os.close(handle)


def main(argv):
    if len(argv) != 3:
        raise executor.Refusal('fixed helper requires one hash-bound request')
    raw = Path(argv[1]).read_bytes()
    if len(raw) > 65536 or hashlib.sha256(raw).hexdigest() != argv[2]:
        raise executor.Refusal('helper request hash or size mismatch')
    request = json.loads(raw)
    keys = {'stage', 'step', 'bindings', 'inputs', 'claim', 'anchor', 'anchor_fd', 'anchor_identity', 'endpoint',
            'manifest_path', 'historical', 'historical_root', 'historical_token', 'tool_paths'}
    if not isinstance(request, dict) or set(request) != keys:
        raise executor.Refusal('invalid helper request')
    stage_id, step_id = request['stage'], request['step']
    if step_id not in ALLOWED.get(stage_id, set()):
        raise executor.Refusal('step is not a fixed C0/D0 helper')
    for path, digest in request['inputs'].items():
        if executor.sha_file(path) != digest:
            raise executor.Refusal('helper input changed')
    if (request['manifest_path'] != str(executor.MANIFEST_PATH.resolve())
            or request['manifest_path'] not in request['inputs']):
        raise executor.Refusal('manifest is not bound')
    stage = executor.stage_of(executor.load_manifest(request['manifest_path']), stage_id)
    matches = [item for item in stage['steps'] if item['id'] == step_id and item['kind'] == 'helper']
    if len(matches) != 1:
        raise executor.Refusal('helper step is not in the pinned manifest')
    step = matches[0]
    executor.HISTORICAL_C0 = request['historical']
    executor.LINUX_ARTIFACT_DIR = request['historical_root']
    executor.RECORDED_TOKEN = request['historical_token']
    executor.TOOL_PATHS = tuple(request['tool_paths'])
    if step_id in ('c0-make-owned-roots', 'd0-make-owned-dirs'):
        result = created_directories(request, step)
    elif step_id == 'c0-verify-tools':
        result = {'verified': executor.verify_tool_sha(executor.TOOL_PATHS)}
    elif step_id == 'c0-verify-tranche':
        result = executor.verify_tranche_summary()
    elif step_id == 'c0-claim-intent':
        result = {'sha256': executor.checked_sha(request['claim']['path'],
                                                 must_be_in=os.path.dirname(request['claim']['path']))}
    else:
        path = request['endpoint']
        info = os.lstat(path)
        if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.geteuid():
            raise executor.Refusal('desktop endpoint must be the owned user socket')
        result = {'socket': path, 'device': info.st_dev, 'inode': info.st_ino}
    observed = {path: executor.sha_file(path) for path in request['inputs']}
    if observed != request['inputs']:
        raise executor.Refusal('helper input changed during execution')
    print(json.dumps({'result': result, 'inputs_sha256': executor.canon_sha(observed)}, sort_keys=True))
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except Exception as exc:
        print('refused: ' + str(exc), file=sys.stderr)
        sys.exit(2)

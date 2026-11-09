"""Nucleus proof-only rescue input entry. Nucleus must pin the merged head and request.

The external Nucleus launch is the authority, never this file, a request, a path or the environment.
No acquisition is run by importing the module or by its ordinary unarmed invocation.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import linux_rescue_input as rescue
import preparation_stage as stage

SCRATCH = '/Volumes/Stash/scratch/effigy'
REPO = HERE.parents[2]
META = HERE / 'ubuntu-2604-metadata'
GPG = '/opt/homebrew/Cellar/gnupg/2.5.24/bin/gpg'
GPGV = '/opt/homebrew/Cellar/gnupg/2.5.24/bin/gpgv'
GPG_SHA = '0d1fe57b0c2c3351645b616ece9132927450d9a638417a506fc8c2eb07030b76'
GPGV_SHA = 'a5dde1f1f3e3cd7adf3c62b42a8a3951644af4a8013dd891eebcebca1ef5f099'
META_HASHES = {'SHA256SUMS': '0daa248426442137c56a5122b89bda08e5854f565a6eed26dec5622094d3e5a3',
               'SHA256SUMS.gpg': 'bfa9b6e8b87298c3dfed6310029e370c4bd5f553175fb2c57705cd11bfb84086',
               'ubuntu-cdimage-2012.asc': '337fa0013bb263aebcef98fa7b7af18e186d89565a5019f7295beb42ebee5290',
               'ubuntu-26.04.1-live-server-arm64.manifest': 'ad43a50074efc81bc77c4ef20339f4897fc8264eea5196655fe240fab0d6c86e'}
SOURCES = ('linux_rescue_input_proof.py', 'linux_rescue_input.py', 'runtime_image_inputs.py', 'preparation_stage.py')
CHILD = '''import json, sys, time
sys.path.insert(0, sys.argv[1])
import linux_rescue_input as r
end = float(sys.argv[3])
def checkpoint():
    if time.monotonic() >= end:
        raise r.Refusal("acquisition deadline reached")
print(json.dumps(r.fetch_fixed(sys.argv[2], checkpoint=checkpoint), sort_keys=True))
'''


def digest(path):
    h = hashlib.sha256()
    with open(path, 'rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def root_identity(path):
    if not isinstance(path, str) or not path.startswith(SCRATCH + '/') or os.path.realpath(path) != path:
        raise stage.Refusal('root is not canonical under removable Stash scratch')
    st = os.lstat(path)
    if not stat.S_ISDIR(st.st_mode) or st.st_uid != os.geteuid() or stat.S_IMODE(st.st_mode) != 0o700:
        raise stage.Refusal('root is not an owned mode-0700 directory')
    if os.path.realpath('/Volumes/Stash') != '/Volumes/Stash' or not os.path.ismount('/Volumes/Stash'):
        raise stage.Refusal('Stash is not the canonical mounted volume')
    volume = os.stat('/Volumes/Stash')
    if st.st_dev != volume.st_dev:
        raise stage.Refusal('root is not on the bound Stash volume')
    return {'device': st.st_dev, 'inode': st.st_ino, 'uid': st.st_uid, 'mode': stat.S_IMODE(st.st_mode),
            'volume_path': '/Volumes/Stash', 'volume_device': volume.st_dev, 'volume_inode': volume.st_ino,
            'volume_uid': volume.st_uid, 'volume_mode': stat.S_IMODE(volume.st_mode)}


def source_inputs():
    """Return stable repository roles separately from canonical machine paths."""
    root = REPO.resolve(strict=True)
    repository = {}
    paths = [(HERE / name, 'docs/qualification/workspace-private-531/' + name) for name in SOURCES]
    paths += [(META / name, 'docs/qualification/workspace-private-531/ubuntu-2604-metadata/' + name) for name in META_HASHES]
    paths += [(root / 'config/tasks.toml', 'config/tasks.toml')]
    expected_roles = {role for _, role in paths}
    if len(expected_roles) != len(paths):
        raise stage.Refusal('repository input roles collide')
    for path, role in paths:
        try:
            resolved = path.resolve(strict=True)
            if resolved != root / role or not resolved.is_file():
                raise ValueError('alias or escape')
            value = digest(resolved)
        except (OSError, ValueError) as exc:
            raise stage.Refusal('invalid repository input: ' + role) from exc
        expected = META_HASHES.get(Path(role).name) if 'ubuntu-2604-metadata/' in role else None
        if expected is not None and value != expected:
            raise stage.Refusal('release metadata differs from its pin: ' + Path(role).name)
        repository[role] = value
    values = {'repository': repository}
    for path, expected in ((GPG, GPG_SHA), (GPGV, GPGV_SHA)):
        try:
            valid = os.path.realpath(path) == path and digest(path) == expected
        except OSError as exc:
            raise stage.Refusal('pinned verifier unavailable: ' + path) from exc
        if not valid:
            raise stage.Refusal('pinned verifier unavailable or changed: ' + path)
        values[path] = expected
    git = os.path.realpath('/usr/bin/git')
    values[git] = digest(git)
    python = os.path.realpath(sys.executable)
    values[python] = digest(python)
    values['machine'] = {key: value for key, value in values.items() if key != 'repository'}
    return {'repository': repository, 'machine': values['machine']}


def runtime_inputs(identity):
    current = source_inputs()
    if current != identity:
        raise stage.Refusal('pinned source inputs changed')
    root = REPO.resolve(strict=True)
    paths = {role: root / role for role in identity['repository']}
    runtime = {str(paths[role].resolve(strict=True)): value for role, value in identity['repository'].items()}
    if set(runtime) & set(identity['machine']):
        raise stage.Refusal('repository and machine input roles collide')
    return runtime | identity['machine']


def plan(fixture, evidence, head, nonce):
    if not re.fullmatch(r'[0-9a-f]{40}', head or '') or not re.fullmatch(r'[0-9a-f]{64}', nonce or ''):
        raise stage.Refusal('exact head and one-use nonce required')
    fixture_identity, evidence_identity = root_identity(fixture), root_identity(evidence)
    if fixture == evidence or fixture.startswith(evidence + '/') or evidence.startswith(fixture + '/'):
        raise stage.Refusal('fixture and evidence roots overlap')
    if os.listdir(fixture):
        raise stage.Refusal('acquisition fixture is not empty')
    fs = os.statvfs(fixture)
    if fs.f_bavail * fs.f_frsize < rescue.SIZE:
        raise stage.Refusal('fixture does not have the pinned image size available')
    inputs = source_inputs()
    return {'schema': 'rescue-input-v1', 'head': head, 'nonce': nonce, 'fixture': fixture,
            'fixture_identity': fixture_identity, 'evidence': evidence, 'evidence_identity': evidence_identity,
            'destination': fixture + '/' + rescue.NAME, 'url': rescue.URL, 'bytes': rescue.SIZE,
            'sha256': rescue.SHA256, 'publisher_fingerprint': rescue.FINGERPRINT,
            'budget_bytes': rescue.FIXTURE_CAP, 'duration_seconds': rescue.DURATION,
            'inputs': inputs, 'python': os.path.realpath(sys.executable)}


def good(result):
    return result.get('disposition') == 'exited' and type(result.get('waited_exit')) is int and result['waited_exit'] == 0


def verified_signature(result, status):
    valid = [line.split() for line in status.splitlines() if line.startswith('[GNUPG:] VALIDSIG ')]
    return (good(result) and len(valid) == 1 and len(valid[0]) == 12
            and valid[0][2] == rescue.FINGERPRINT and valid[0][-1] == rescue.FINGERPRINT)


def execute(request, request_path, request_hash, deadline):
    """External Nucleus proof launch only. Input files describe scope; they grant no authority."""
    if request != plan(request['fixture'], request['evidence'], request['head'], request['nonce']):
        raise stage.Refusal('pinned acquisition bindings drifted')
    head = subprocess.check_output(['/usr/bin/git', '-C', str(REPO), 'rev-parse', 'HEAD'], timeout=10).decode().strip()
    dirty = subprocess.check_output(['/usr/bin/git', '-C', str(REPO), 'status', '--porcelain'], timeout=10)
    if head != request['head'] or dirty:
        raise stage.Refusal('source head changed or checkout is dirty')
    held = os.open(request['fixture'], os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        st = os.fstat(held)
        if (st.st_dev, st.st_ino, st.st_uid) != tuple(request['fixture_identity'][x] for x in ('device', 'inode', 'uid')):
            raise stage.Refusal('held fixture identity differs')
        evidence = Path(request['evidence'])
        claim = evidence / ('rescue-' + request['nonce'] + '.claim')
        stage.exclusive(claim, (request_hash + '\n').encode())
        evidence_fd = os.open(evidence, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            os.fsync(evidence_fd)
        finally:
            os.close(evidence_fd)
        root = evidence / ('rescue-' + request['nonce'])
        root.mkdir(mode=0o700)
        home = root / 'keyring-home'
        home.mkdir(mode=0o700)
        keyring = root / 'ubuntu.gpg'
        inputs = runtime_inputs(request['inputs'])
        inputs[request_path] = request_hash
        def call(index, argv, end):
            held_now = os.fstat(held)
            if ((held_now.st_dev, held_now.st_ino, held_now.st_uid) !=
                    tuple(request['fixture_identity'][x] for x in ('device', 'inode', 'uid')) or
                    root_identity(request['fixture']) != request['fixture_identity'] or
                    root_identity(request['evidence']) != request['evidence_identity']):
                raise stage.Refusal('bound root identity changed')
            step = root / ('step-%02d' % index)
            step.mkdir(mode=0o700)
            return stage.run_stage(step, argv, min(1800, max(0.001, end - time.monotonic() - 30)), 65536,
                                   inputs, deadline=end, cleanup_deadline=deadline)
        reserve_end = deadline - 30
        dearmor = call(1, [GPG, '--no-options', '--homedir', str(home), '--batch', '--dearmor',
                           '--output', str(keyring), str(META / 'ubuntu-cdimage-2012.asc')], reserve_end)
        if not good(dearmor):
            raise stage.Refusal('publisher keyring preparation did not complete')
        inputs[str(keyring)] = digest(keyring)
        verify = call(2, [GPGV, '--keyring', str(keyring), '--status-fd', '1',
                          str(META / 'SHA256SUMS.gpg'), str(META / 'SHA256SUMS')], reserve_end)
        status = (root / 'step-02' / 'stdout').read_text()
        if not verified_signature(verify, status):
            raise stage.Refusal('publisher detached signature was not verified with the full fingerprint')
        sums = (META / 'SHA256SUMS').read_text()
        manifest = (META / 'ubuntu-26.04.1-live-server-arm64.manifest').read_text()
        if sums.splitlines().count(rescue.SHA256 + ' *' + rescue.NAME) != 1 or manifest.splitlines().count('e2fsprogs\t1.47.2-3ubuntu4') != 1:
            raise stage.Refusal('signed image pin or tool manifest differs')
        child = [request['python'], '-I', '-B', '-c', CHILD, str(HERE), request['destination'], str(reserve_end)]
        result = call(3, child, reserve_end)
        if not good(result) or root_identity(request['fixture']) != request['fixture_identity']:
            raise stage.Refusal('acquisition failed, timed out or fixture changed; preserve partial bytes')
        output = json.loads((root / 'step-03' / 'stdout').read_text())
        image = os.lstat(request['destination'])
        if (not stat.S_ISREG(image.st_mode) or image.st_nlink != 1 or image.st_uid != os.geteuid() or
                image.st_dev != st.st_dev or image.st_size != rescue.SIZE or
                os.path.realpath(request['destination']) != request['destination']):
            raise stage.Refusal('acquired image identity, type or size differs')
        if output != {'bytes': rescue.SIZE, 'sha256': rescue.SHA256} or os.listdir(request['fixture']) != [rescue.NAME]:
            raise stage.Refusal('acquisition output or fixture entries differ')
        if time.monotonic() >= deadline or source_inputs() != request['inputs']:
            raise stage.Refusal('deadline or pinned source inputs changed before receipt')
        final_head = subprocess.check_output(['/usr/bin/git', '-C', str(REPO), 'rev-parse', 'HEAD'], timeout=10).decode().strip()
        final_dirty = subprocess.check_output(['/usr/bin/git', '-C', str(REPO), 'status', '--porcelain'], timeout=10)
        if final_head != head or final_dirty or time.monotonic() >= deadline:
            raise stage.Refusal('source checkout changed or deadline expired before receipt')
        stage.record(root / 'result.json', {'status': 'acquired', 'request_sha256': request_hash,
                                           'head': head, 'image': output, 'vm_effect': False})
        return root
    finally:
        os.close(held)


def main(argv):
    deadline = time.monotonic() + rescue.DURATION
    if len(argv) == 6 and argv[1] == 'plan':
        print(json.dumps(plan(argv[2], argv[3], argv[4], argv[5]), sort_keys=True, indent=2))
        return
    if len(argv) != 3:
        raise stage.Refusal('ordinary execution is unarmed; Nucleus proof request and digest required')
    path, expected = argv[1:]
    if os.path.realpath(path) != path or not path.startswith(SCRATCH + '/') or not re.fullmatch(r'[0-9a-f]{64}', expected):
        raise stage.Refusal('request path or digest is invalid')
    st = os.lstat(path)
    if not stat.S_ISREG(st.st_mode) or st.st_uid != os.geteuid() or st.st_nlink != 1 or st.st_mode & 0o222:
        raise stage.Refusal('request must be an owned read-only regular file')
    if st.st_size > 65536:
        raise stage.Refusal('request exceeds its byte cap')
    with open(path, 'rb') as source:
        raw = source.read(65537)
    if len(raw) > 65536 or hashlib.sha256(raw).hexdigest() != expected:
        raise stage.Refusal('request is oversized or differs from its digest')
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise stage.Refusal('duplicate request key')
            result[key] = value
        return result
    execute(json.loads(raw, object_pairs_hook=unique), path, expected, deadline)


if __name__ == '__main__':
    try:
        main(sys.argv)
    except (stage.Refusal, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        print('refused: ' + str(exc), file=sys.stderr)
        sys.exit(2)

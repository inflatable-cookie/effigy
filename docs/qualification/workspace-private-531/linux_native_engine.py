"""Unsealed inputs for the task-829 native engine exploration; no proof activation."""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_diagnostic_proof as diagnostic
import runtime_image_inputs as inputs


def runtime_recipe():
    """Share the pinned package closure without rewriting historical packets."""
    packet = Path(__file__).resolve().parent
    return (packet / 'runtime.Dockerfile').read_bytes() + b'\n' + (packet / 'linux-native-runtime.fragment').read_bytes()


def fetch(root):
    diagnostic.private_root(str(root))
    destination = root / ('runtime-inputs-' + uuid.uuid4().hex)
    destination.mkdir(mode=0o700)
    result = inputs.fetch_runtime_inputs(str(destination), inputs.production_pin(),
                                        opener=inputs.open_runtime_https)
    context = destination / 'context'
    context.mkdir(mode=0o700)
    recipe = context / 'Dockerfile'
    recipe.write_bytes(runtime_recipe())
    result['recipe'] = {'path': str(recipe), 'sha256': hashlib.sha256(recipe.read_bytes()).hexdigest()}
    result.update(exploratory=True, qualification_credit=False)
    (destination / 'outcome.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))


def build_resident(iid):
    """Rebuild the current recipe offline from independently re-hashed payloads."""
    if sys.platform != 'linux' or os.geteuid() != 1001:
        raise RuntimeError('native dedicated build account required')
    client = Path('/var/lib/effigy-829/rootful/client')
    work = Path(tempfile.mkdtemp(prefix='runtime-', dir=os.environ.get('TMPDIR', '/Volumes/Stash/scratch/effigy/linux-native-829/artifact-tmp')))
    source = client / 'inputs'
    pin = inputs.production_pin()

    def copy(item, relative, destination):
        path = source / relative
        info = path.lstat()
        if (path.resolve(strict=True) != path or not stat.S_ISREG(info.st_mode)
                or info.st_nlink != 1 or info.st_size != item['bytes']):
            raise RuntimeError('offline runtime payload identity differs')
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != inputs.item_sha256(item):
            raise RuntimeError('offline runtime payload digest differs')
        destination.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        destination.write_bytes(data)
        return data

    oci, debs, context = (work / name for name in ('oci', 'debs', 'context'))
    index = copy(pin['index'], Path('oci/index.upstream.json'), oci / 'index.upstream.json')
    descriptor = inputs.select_arm64_descriptor(index, pin)
    for key in ('manifest', 'config', 'layer'):
        name = inputs.item_sha256(pin[key])
        copy(pin[key], Path('oci/blobs/sha256') / name, oci / 'blobs/sha256' / name)
    inputs.verify_arm64_manifest((oci / 'blobs/sha256' / inputs.item_sha256(pin['manifest'])).read_bytes(), pin)
    inputs.verify_arm64_config((oci / 'blobs/sha256' / inputs.item_sha256(pin['config'])).read_bytes())
    inputs.assemble_oci_layout(oci, descriptor)
    for item in pin['debs']:
        copy(item, Path('debs') / item['name'], debs / item['name'])
    context.mkdir(mode=0o700)
    recipe = runtime_recipe()
    (context / 'Dockerfile').write_bytes(recipe)
    iid = iid.resolve()
    if not iid.is_relative_to(Path('/Volumes/Stash/scratch/effigy/linux-native-829/artifacts')) or iid.exists():
        raise RuntimeError('fresh owned runtime image binding required')
    env = dict(os.environ, HOME=str(client / 'home'), TMPDIR=str(client / 'tmp'),
               DOCKER_CONFIG=str(client / 'config'))
    subprocess.run(['/usr/bin/docker', '--context', 'effigy-829-rootful', 'buildx', 'build', '--load',
                    '--iidfile', str(iid), '--network', 'none', '--pull=false',
                    '--build-context', 'ubuntu=oci-layout://' + str(oci),
                    '--build-context', 'debs=' + str(debs), str(context)], env=env, timeout=180, check=True)
    result = {'exploratory': True, 'qualification_credit': False, 'image': iid.read_text().strip(),
              'recipe_sha256': hashlib.sha256(recipe).hexdigest(), 'context': str(work)}
    iid.with_suffix('.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result))


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--build-resident':
        build_resident(Path(sys.argv[2]))
    elif len(sys.argv) == 2:
        fetch(Path(sys.argv[1]))
    else:
        raise SystemExit('usage: qualification:linux:runtime-inputs -- OWNED_EVIDENCE_ROOT')

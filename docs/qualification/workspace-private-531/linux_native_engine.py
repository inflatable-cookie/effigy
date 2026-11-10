"""Unsealed inputs for the task-829 native engine exploration; no proof activation."""
import hashlib
import json
from pathlib import Path
import sys
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


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: qualification:linux:runtime-inputs -- OWNED_EVIDENCE_ROOT')
    fetch(Path(sys.argv[1]))

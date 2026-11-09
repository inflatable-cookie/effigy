"""Unsealed native exploration. Never supplies a qualification receipt or proof credit.

Requires the caller's explicit authority over the fixed VM. The sealed entry still
uses its reviewed snapshot pin; this entry binds the current inventory for one run.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parent))
import linux_diagnostic_proof as diagnostic
import linux_setup as setup


def main(argv):
    if len(argv) != 2:
        raise SystemExit('usage: qualification:linux:native-smoke -- OWNED_EVIDENCE_ROOT')
    root = str(Path(argv[1]).resolve(strict=True))
    diagnostic.private_root(root)
    snapshots = subprocess.run(
        [diagnostic.PRLCTL, 'snapshot-list', setup.VM_UUID, '--json'],
        capture_output=True, timeout=10, check=True)
    pin = hashlib.sha256(diagnostic.canonical(json.loads(
        snapshots.stdout, object_pairs_hook=diagnostic.parse_unique))).hexdigest()
    head = subprocess.check_output([diagnostic.GIT, 'rev-parse', 'HEAD'], timeout=10).decode().strip()
    request = diagnostic.plan(root, 'explore-' + uuid.uuid4().hex, head)
    raw = diagnostic.canonical(request)
    request_path = Path(root) / (request['activation_id'] + '.request.json')
    diagnostic.stage.exclusive(request_path, raw)
    request_path.chmod(0o400)
    result = diagnostic.execute(request, str(request_path), hashlib.sha256(raw).hexdigest(),
                                exploratory_snapshot=pin)
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if not result['findings'] and result['vm_terminal'] == 'stopped' else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))

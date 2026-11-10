"""Authenticated admission receipts: OpenSSH signatures over the receipt bytes.

A GO or containment receipt is a JSON file signed with `ssh-keygen -Y sign` under the
namespace below. It is accepted only when:
- its bytes hash to the sha256 the plan binds,
- its signature file hashes to the bound signature sha256,
- `ssh-keygen -Y verify` accepts it against the allowed-signers file, whose bytes hash
  to the bound trust sha256 and whose principal is the bound principal,
- its fields name the gate, status "approved", this exact plan digest, a decision id and
  a 40-hex source head.

The signer is the operator's key, held outside the repository. The allowed-signers file
and its hash are plan facts. A receipt signed by any other key, or edited after
signing, fails. An unsigned receipt has no signature file and fails.
"""
import hashlib
import json
import re
import subprocess
from pathlib import Path

NAMESPACE = 'effigy-go-receipt'
HEX64 = re.compile(r'^[0-9a-f]{64}$')
HEX40 = re.compile(r'^[0-9a-f]{40}$')


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def run_keygen_verify(argv, stdin_bytes):
    """Real verifier call. Returns the process return code. Tests may inject a different runner."""
    proc = subprocess.run(argv, input=stdin_bytes, capture_output=True, timeout=60)
    return proc.returncode


def verify_signed_receipt(entry, gate, plan_digest, trust, expected_head, verify=run_keygen_verify):
    """(ok, reason). Refuses with a reason; never raises for a bad receipt."""
    if not isinstance(entry, dict) or not HEX64.fullmatch(entry.get('sha256') or '') or not entry.get('path'):
        return False, 'receipt entry lacks a path or sha256'
    if not isinstance(trust, dict) or not HEX64.fullmatch(trust.get('sha256') or '') or not trust.get('allowed_signers'):
        return False, 'trust anchor is not bound (allowed signers and sha256)'
    if not entry.get('signature_path') or not HEX64.fullmatch(entry.get('signature_sha256') or ''):
        return False, 'receipt is unsigned (no signature bound)'
    try:
        data = Path(entry['path']).read_bytes()
        signature = Path(entry['signature_path']).read_bytes()
        signers = Path(trust['allowed_signers']).read_bytes()
    except OSError:
        return False, 'receipt, signature or allowed-signers file unreadable'
    if _sha(data) != entry['sha256']:
        return False, 'receipt bytes differ from the bound sha256'
    if _sha(signature) != entry['signature_sha256']:
        return False, 'signature bytes differ from the bound sha256'
    if _sha(signers) != trust['sha256']:
        return False, 'allowed-signers bytes differ from the bound trust sha256'
    argv = ['ssh-keygen', '-Y', 'verify', '-f', trust['allowed_signers'], '-I', trust.get('principal', 'effigy-go'),
            '-n', NAMESPACE, '-s', entry['signature_path']]
    if verify(argv, data) != 0:
        return False, 'signature does not verify against the trusted signer'
    try:
        receipt = json.loads(data)
    except ValueError:
        return False, 'receipt is not JSON'
    if receipt.get('gate') != gate or receipt.get('status') != 'approved':
        return False, 'receipt is not an approved ' + gate + ' receipt'
    if receipt.get('plan_sha256') != plan_digest:
        return False, 'receipt is bound to a different plan (stale)'
    if not receipt.get('decision_id') or not HEX40.fullmatch(receipt.get('source_head') or ''):
        return False, 'receipt lacks a decision id or source head'
    if not HEX40.fullmatch(expected_head or '') or receipt.get('source_head') != expected_head:
        return False, 'receipt is bound to a different reviewed head'
    return True, None

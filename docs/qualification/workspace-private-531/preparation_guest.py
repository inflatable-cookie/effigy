"""Guest-local deadline runner, invoked only over an authenticated SSH channel.

A process group can escape through setsid; this records terminal status but
leaves closure unknown until independently contained descendants are accounted.
"""
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from preparation_stage import run_stage, Refusal


def linux_identity(pid):
    boot = Path('/proc/sys/kernel/random/boot_id').read_text().strip()
    # comm is parenthesized and may itself contain spaces or parentheses.
    text = Path('/proc', str(pid), 'stat').read_text()
    fields = text[text.rfind(')') + 2:].split()
    session = int(fields[3])
    if not boot or session != pid:
        raise Refusal('guest boot or session identity unavailable')
    return {'boot_id': boot, 'start_time': int(fields[19])}


def main():
    if len(sys.argv) != 2:
        raise Refusal('one exact request file required')
    request = json.loads(Path(sys.argv[1]).read_text())
    if set(request) != {'stage_dir', 'argv', 'timeout_seconds', 'output_cap_bytes', 'input_hashes', 'nonce', 'cwd', 'environment'}:
        raise Refusal('unsupported request')
    nonce = request['nonce']
    if not isinstance(nonce, str) or len(nonce) != 64 or any(c not in '0123456789abcdef' for c in nonce):
        raise Refusal('nonce must be 256-bit hexadecimal')
    def identity(pid):
        return dict(linux_identity(pid), nonce=nonce)
    result = run_stage(request['stage_dir'], request['argv'], request['timeout_seconds'],
                       request['output_cap_bytes'], request['input_hashes'],
                       guest=True, identity_probe=identity, cwd=request['cwd'],
                       environment=request['environment'])
    # A terminal exit is not a closure claim; all evidence remains in stage_dir.
    return 0 if result['disposition'] == 'exited' else 1


if __name__ == '__main__':
    sys.exit(main())

"""One bounded local read-only Colima/Docker metadata tranche for macOS preparation.

Effects: five child processes, each a local metadata command (no daemon, guest,
socket, credential or config.json read by this script). Importing has no effects.
Usage: python3 -I -B macos_metadata_tranche.py <fresh-parent-dir>
The parent must be the existing 0700 owner-only scratch directory; the child is
created fresh inside it and is the only place anything is written.
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time

TRANCHE_SECONDS = 600
REPORT_RESERVE_SECONDS = 120
COMMAND_SECONDS = 30
OUTPUT_CAP_BYTES = 256 * 1024
DESKTOP_CONTEXT = 'desktop-linux'
# Observed only as the exact tools and contexts named here; no daemon route is argv.
TOOLS = ('colima', 'docker')


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for chunk in iter(lambda: handle.read(1 << 16), b''):
            digest.update(chunk)
    return digest.hexdigest()


def utc_now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def plan(tool_paths):
    """Pure: the exact five argv lists. tool_paths maps 'colima'/'docker' to resolved paths."""
    colima, docker = tool_paths['colima'], tool_paths['docker']
    return [
        dict(id='colima-version', argv=[colima, 'version']),
        dict(id='docker-version-client', argv=[docker, '--version']),
        dict(id='colima-list', argv=[colima, 'list', '--json']),
        dict(id='docker-context-ls', argv=[docker, 'context', 'ls', '--format', 'json']),
        dict(id='docker-context-inspect', argv=[docker, 'context', 'inspect', DESKTOP_CONTEXT]),
    ]


def child_environment():
    """Minimal, daemon-route-free environment; no DOCKER_HOST/DOCKER_CONTEXT/LIMA_HOME/COLIMA_HOME."""
    keep = ('PATH', 'HOME', 'USER', 'LANG', 'LC_ALL')
    env = {k: os.environ[k] for k in keep if k in os.environ}
    env['TMPDIR'] = os.environ.get('TMPDIR', '')
    return env


def open_parent(path):
    path = Path(path)
    require(path.is_absolute() and path.is_dir() and not path.is_symlink(), 'absolute owned parent required')
    info = path.lstat()
    require(stat.S_IMODE(info.st_mode) == 0o700 and info.st_uid == os.geteuid(),
            'parent must be owner-only 0700 and owned by this user')
    return path


def run_bounded(step, child, started, deadline):
    """Run one argv; kill only this process group on timeout or output cap. Retains exact streams."""
    stdout_path = child / f'{step["id"]}.stdout'
    stderr_path = child / f'{step["id"]}.stderr'
    fds = [os.open(p, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600) for p in (stdout_path, stderr_path)]
    record = dict(id=step['id'], argv=step['argv'], started_utc=utc_now(),
                  started_monotonic=time.monotonic(), cap_bytes=OUTPUT_CAP_BYTES,
                  timeout_seconds=COMMAND_SECONDS)
    budget = min(COMMAND_SECONDS, max(0.0, deadline - time.monotonic()))
    try:
        proc = subprocess.Popen(step['argv'], stdin=subprocess.DEVNULL, stdout=fds[0], stderr=fds[1],
                                env=child_environment(), start_new_session=True, close_fds=True)
    finally:
        for fd in fds:
            os.close(fd)
    record['pid'] = proc.pid
    end = time.monotonic() + budget
    outcome = None
    while proc.poll() is None:
        size = sum(p.stat().st_size for p in (stdout_path, stderr_path))
        if size > OUTPUT_CAP_BYTES:
            outcome = 'output_cap_exceeded'
        elif time.monotonic() >= end:
            outcome = 'deadline_exceeded'
        if outcome:
            os.killpg(proc.pid, signal.SIGKILL)
            break
        time.sleep(0.05)
    returncode = proc.wait()
    record.update(ended_utc=utc_now(), ended_monotonic=time.monotonic(), waited_exit=returncode,
                  outcome=outcome or ('exited' if returncode == 0 else 'nonzero_exit'))
    record['stdout'] = dict(path=stdout_path.name, bytes=stdout_path.stat().st_size,
                            sha256=sha256_file(stdout_path))
    record['stderr'] = dict(path=stderr_path.name, bytes=stderr_path.stat().st_size,
                            sha256=sha256_file(stderr_path))
    return record


def extract_inspect(raw):
    """Pure: nonsecret endpoint and public-path metadata from `docker context inspect` JSON."""
    entries = json.loads(raw)
    require(isinstance(entries, list) and len(entries) == 1, 'one context expected')
    entry = entries[0]
    docker = entry.get('Endpoints', {}).get('docker', {})
    storage = entry.get('Storage') or {}
    # Public paths only: TLS/metadata directory paths are recorded, never their contents.
    return dict(name=entry.get('Name'), host=docker.get('Host'),
                skip_tls_verify=docker.get('SkipTLSVerify'),
                metadata_path=storage.get('MetadataPath'), tls_path=storage.get('TLSPath'))


def extract_colima_profiles(raw):
    """Pure: profile name/status/runtime only from `colima list --json` JSON lines."""
    profiles = []
    for line in raw.splitlines():
        if line.strip():
            item = json.loads(line)
            profiles.append(dict(name=item.get('name'), status=item.get('status'),
                                 runtime=item.get('runtime'), arch=item.get('arch')))
    return profiles


def main(argv):
    require(len(argv) == 2, 'usage: macos_metadata_tranche.py <owned-0700-parent>')
    parent = open_parent(argv[1])
    tools = {}
    for name in TOOLS:
        resolved = shutil.which(name)
        require(resolved is not None, f'{name} missing from PATH: exact missing prerequisite')
        tools[name] = dict(path=resolved, realpath=os.path.realpath(resolved),
                           sha256=sha256_file(os.path.realpath(resolved)))
    steps = plan({name: tools[name]['realpath'] for name in TOOLS})
    child = Path(tempfile.mkdtemp(prefix='macos-metadata-', dir=parent))
    started = time.monotonic()
    deadline = started + TRANCHE_SECONDS - REPORT_RESERVE_SECONDS
    intent = dict(tranche='macos-4f6a5ed3-metadata-r1', created_utc=utc_now(), runner_sha256=sha256_file(__file__),
                  tools=tools, steps=steps, tranche_seconds=TRANCHE_SECONDS,
                  report_reserve_seconds=REPORT_RESERVE_SECONDS, command_seconds=COMMAND_SECONDS,
                  output_cap_bytes=OUTPUT_CAP_BYTES, retries=0, child=child.name)
    (child / 'intent.json').write_text(json.dumps(intent, indent=2, sort_keys=True) + '\n')
    records = []
    for step in steps:
        if time.monotonic() >= deadline:
            records.append(dict(id=step['id'], outcome='not_run_tranche_deadline'))
            continue
        records.append(run_bounded(step, child, started, deadline))
    summary = dict(intent_sha256=sha256_file(child / 'intent.json'), records=records,
                   total_monotonic_seconds=round(time.monotonic() - started, 3))
    by_id = {r['id']: r for r in records}
    observed = {}
    for key, step_id, parse in (
        ('colima_profiles', 'colima-list', extract_colima_profiles),
        ('desktop_context', 'docker-context-inspect', extract_inspect),
    ):
        record = by_id.get(step_id, {})
        if record.get('waited_exit') == 0:
            raw = (child / record['stdout']['path']).read_text()
            observed[key] = parse(raw)
        else:
            observed[key] = dict(missing_prerequisite=step_id, outcome=record.get('outcome'))
    summary['observed'] = observed
    (child / 'summary.json').write_text(json.dumps(summary, indent=2, sort_keys=True) + '\n')
    print(json.dumps(dict(child=str(child), summary=summary), indent=2, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))

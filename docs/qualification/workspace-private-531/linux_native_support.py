"""Live Linux VM identity and bounded host transport; no quota execution."""
import os
from pathlib import Path
import select
import shlex
import signal
import stat
import subprocess
import time

from preparation_stage import Refusal
import preparation_resources as resources
import preparation_observations as observations

SSH_ALIAS = 'effigy-531-vault'
MAX_DEADLINE = 1200

VM_UUID = '433c9e5b-c7b9-469f-9633-24b4addcd3b5'
DISK_UID = '{44b5d35e-b053-4aae-9140-266a00bbef9a}'
PVM_HOME = '/Volumes/Stash/parallels/Ubuntu 26.04 ARM64.pvm'
CONFIG_NAME = 'config.pvs'
DISK_DIR = 'harddisk1.hdd'
DESCRIPTOR_NAME = 'DiskDescriptor.xml'
PROBE_SHA256 = '6bde748a456617737fb4928d347d55fa96e5a6ff0dac119ace373692ef41fc39'
STAGE1_SYSTEM_STATES = frozenset({
    'initializing', 'starting', 'running', 'degraded', 'maintenance',
    'stopping', 'offline', 'unknown',
})
CANDIDATE_H_HEAD = '4f6a5ed3078bc6e354c8a7738ea5663c8ffefa02'   # source-bound candidate H, distinct from K


def _owned_regular(path):
    """A regular, non-symlink file owned by the operator; returns its identity."""
    info = Path(path).lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid():
        raise Refusal('registered file is not a regular file of the operator: ' + str(path))
    return {'dev': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid}


def _owned_dir(path):
    info = Path(path).lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid():
        raise Refusal('registered directory is not a directory of the operator: ' + str(path))
    return {'dev': info.st_dev, 'inode': info.st_ino, 'uid': info.st_uid}


def stage1_accept_readiness(result):
    """Accept exactly running/exit 0. Degraded, unknown, transport, nonzero or missing refuse."""
    if not isinstance(result, dict):
        raise Refusal('Stage1 readiness terminal facts are missing')
    disposition = result.get('disposition')
    waited = result.get('waited_exit')
    stdout = result.get('stdout')
    if disposition == 'timeout':
        raise Refusal('Stage1 readiness timed out before a terminal systemd state')
    if disposition not in ('exited', 'failed'):
        raise Refusal('Stage1 readiness transport did not produce a terminal fact: ' + str(disposition))
    if stdout is None:
        raise Refusal('Stage1 readiness stdout is missing')
    if isinstance(stdout, bytes):
        try:
            text = stdout.decode('utf-8', 'strict').strip()
        except UnicodeDecodeError as exc:
            raise Refusal('Stage1 readiness stdout is not UTF-8') from exc
    else:
        text = str(stdout).strip()
    if text not in STAGE1_SYSTEM_STATES:
        raise Refusal('Stage1 readiness printed an unknown systemd state: ' + text)
    if text != 'running' or waited != 0 or disposition != 'exited':
        raise Refusal('Stage1 readiness is not running/exit 0; observed state=%s waited_exit=%s disposition=%s'
                      % (text, waited, disposition))
    return text


def _kill_group(proc):
    """Kill the process group. An already-exited group reports EPERM or ESRCH on macOS; both mean nothing is left."""
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass


def drain(procs, timeout, output_cap, monitor=None, poll=0.05):
    """Read the streams of every process until all exit or the deadline or a refusal.

    Returns (failure, exit_codes, streams). failure is None or (disposition, reason).
    """
    if not isinstance(output_cap, int) or output_cap <= 0 or not 0 < timeout <= MAX_DEADLINE:
        raise Refusal('invalid output cap or deadline')
    streams = {proc: {'stdout': bytearray(), 'stderr': bytearray()} for proc in procs}
    owners = {}
    for proc in procs:
        owners[proc.stdout] = (proc, 'stdout')
        owners[proc.stderr] = (proc, 'stderr')
    open_pipes = set(owners)
    failure = None
    started = time.monotonic()
    try:
        while open_pipes and failure is None:
            if time.monotonic() - started > timeout:
                failure = ('timeout', 'deadline reached')
                break
            if monitor is not None:
                try:
                    monitor.check()
                except resources.Refusal as exc:
                    failure = ('resource-refusal', str(exc))
                    break
            ready, _, _ = select.select(list(open_pipes), [], [], poll)
            for pipe in ready:
                chunk = os.read(pipe.fileno(), 65536)
                if not chunk:
                    open_pipes.discard(pipe)
                    continue
                proc, name = owners[pipe]
                streams[proc][name].extend(chunk)
                if len(streams[proc][name]) > output_cap:
                    failure = ('output-cap', 'stream exceeded the cap; partial output is not accepted')
                    break
    finally:
        if failure is not None:
            for proc in procs:
                _kill_group(proc)
        codes = []
        for proc in procs:
            remaining = max(0.1, timeout - (time.monotonic() - started))
            try:
                codes.append(proc.wait(timeout=remaining))
            except subprocess.TimeoutExpired:
                _kill_group(proc)
                codes.append(proc.wait(timeout=10))
                failure = failure or ('timeout', 'process outlived the deadline')
        for proc in procs:
            proc.stdout.close()
            proc.stderr.close()
    if failure is None and monitor is not None:
        try:
            monitor.check()
        except resources.Refusal as exc:
            failure = ('resource-refusal', str(exc))
    return failure, codes, {proc: {k: bytes(v) for k, v in streams[proc].items()} for proc in procs}


def run_transfer(argv, stdin_path, timeout, output_cap, monitor=None, popen=subprocess.Popen,
                 environment=None):
    """One bounded local process, stdin from a file. Exit status is the waited result, never closure.
    environment, when given, is the child's exact environment (the fixed Parallels CLI read pins
    the retained LANG/PATH); the default inherits this process's environment."""
    stdin = open(stdin_path, 'rb') if stdin_path else subprocess.DEVNULL
    try:
        popen_kwargs = {} if environment is None else {'env': dict(environment)}
        proc = popen(argv, stdin=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                     start_new_session=True, **popen_kwargs)
    finally:
        if stdin_path:
            stdin.close()
    failure, codes, streams = drain([proc], timeout, output_cap, monitor)
    out = streams[proc]
    if failure is not None:
        return {'disposition': failure[0], 'reason': failure[1], 'waited_exit': codes[0],
                'stdout': out['stdout'], 'stderr': out['stderr']}
    disposition = 'exited' if codes[0] == 0 else 'failed'
    return {'disposition': disposition, 'reason': None, 'waited_exit': codes[0],
            'stdout': out['stdout'], 'stderr': out['stderr']}


def ssh_argv(config, remote_argv):
    """Pinned-config SSH shape: one fixed alias, `--`, then ONE remote command string.

    ssh joins the remote words with spaces and the remote shell re-parses them, so the
    remote argv is quoted into a single string here.
    """
    if not isinstance(config, str) or not config.startswith('/') or '..' in Path(config).parts:
        raise Refusal('ssh config must be an absolute canonical path')
    if not remote_argv or any(not isinstance(a, str) or '\x00' in a for a in remote_argv):
        raise Refusal('remote argv must be non-empty strings')
    return ['/usr/bin/ssh', '-F', config, SSH_ALIAS, '--', shlex.join(remote_argv)]


def host_credit(expected, candidate, observation):
    """Credit closure only when the guest candidate equals the host's own observation.

    The guest's candidate is never authenticated by itself. The authenticator compares it
    with the independent VM-route observation, and the 686 contract then checks identity,
    unit, supervisor and terminal basis. The candidate must also carry the cgroup the
    monitor observed while the gate was closed.
    """
    if candidate is None or observation is None or observation.get('control_group'):
        return 'unknown'
    if not candidate.get('observed_control_group'):
        return 'unknown'

    def authenticate(receipt):
        return all(receipt.get(k) == observation.get(k)
                   for k in ('boot_id', 'nonce', 'unit', 'invocation_id', 'active_state', 'sub_state', 'result', 'exec_main_status', 'control_group'))
    try:
        return observations.verify_contained_closure(expected, candidate, authenticate)
    except observations.Refusal as exc:
        raise Refusal(str(exc)) from exc


def verify_disk_association(facts, supported=None):
    """Fail closed unless every linux_disk_association premise passes on a complete bound observation.

    A record stored by `linux_disk_association.stored_facts` (the document one observation stage
    keeps) is decoded here first, so the setup rows re-verify the same facts the stage observed
    rather than a reduced summary.
    """
    import linux_disk_association as disk_association
    supported = {disk_association.MECHANISM} if supported is None else supported
    return disk_association.verify(disk_association.load_facts(facts), supported)

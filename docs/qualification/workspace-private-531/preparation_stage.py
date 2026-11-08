"""One-use host stage primitive. Guest closure needs a separate guest receipt."""
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import time


class Refusal(ValueError):
    pass


# Nested helpers may consume the remaining Recipe E producer allowance (kit E14).
# Existing callers still pass at most 900 seconds; this does not enlarge native windows.
STAGE_TIMEOUT_CAP = 1800


def exclusive(path, data):
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0)
    fd = os.open(path, flags, 0o600)
    with os.fdopen(fd, 'wb') as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())


def record(path, value):
    exclusive(path, (json.dumps(value, sort_keys=True, indent=2) + '\n').encode())


def run_stage(root, argv, timeout, output_cap, input_hashes, guest=False, identity_probe=None, cwd=None, environment=None, resource_monitor=None, deadline=None, stdin_path=None, stdin_sha256=None, cleanup_deadline=None, pass_fds=(), verify_in_fixed_helper=False, fixed_context_query=False):
    """Spend a stage occurrence before launch. A missing receipt is unknown.

    The caller supplies an already private, exclusive directory and exact, reviewed
    argv and input hashes. This primitive cannot establish guest identity or close
    a guest session; it never treats an SSH transport exit as guest closure.

    deadline: optional absolute time.monotonic() cap shared by a multi-step stage; the
    launch limit is the earlier of start + timeout and deadline. stdin_path/stdin_sha256:
    optional hash-bound bytes delivered as the child's stdin (default: /dev/null).
    cleanup_deadline bounds the wait after termination; a missing waited exit stays unknown.
    verify_in_fixed_helper is restricted to the fixed macOS helper, which checks the
    bound inputs in the monitored child before and after its operation.
    """
    root = Path(root)
    info = root.lstat()
    if not root.is_dir() or root.is_symlink() or info.st_uid != os.geteuid() or info.st_mode & 0o777 != 0o700:
        raise Refusal('stage directory must be owned and mode 0700')
    if (not isinstance(argv, list) or not argv or (not isinstance(argv[0], str) or not argv[0] or any(not isinstance(x, str) or '\x00' in x for x in argv))
            or not 0 < timeout <= STAGE_TIMEOUT_CAP or not 0 < output_cap <= 64 * 1024 * 1024):
        raise Refusal('invalid argv, deadline, or output cap')
    if deadline is not None and deadline - time.monotonic() <= 0:
        raise Refusal('stage deadline already passed: refuse before launch')
    if cleanup_deadline is not None and (deadline is None or cleanup_deadline < deadline):
        raise Refusal('cleanup deadline must include the launch deadline')
    if not isinstance(input_hashes, dict) or any(len(v) != 64 or any(c not in '0123456789abcdef' for c in v) for v in input_hashes.values()):
        raise Refusal('input hashes must be SHA-256')
    if (stdin_path is None) != (stdin_sha256 is None):
        raise Refusal('stdin needs both a path and its SHA-256')
    if verify_in_fixed_helper:
        import sys
        helper = str(Path(__file__).resolve().with_name('macos_stage_helper.py'))
        if (len(argv) != 6 or os.path.realpath(argv[0]) != os.path.realpath(sys.executable)
                or argv[1:4] != ['-I', '-B', helper] or helper not in input_hashes
                or argv[4] not in input_hashes or input_hashes[argv[4]] != argv[5]
                or guest or stdin_path is not None):
            raise Refusal('deferred input checking is restricted to the fixed macOS helper')
    cwd = str(Path(cwd or os.getcwd()).resolve())
    environment = environment if environment is not None else {k: os.environ[k] for k in ('PATH', 'HOME', 'TMPDIR', 'TMP', 'TEMP', 'LANG') if k in os.environ}
    if not isinstance(environment, dict) or any(not isinstance(k, str) or not isinstance(v, str) or '=' in k or '\x00' in k + v for k, v in environment.items()):
        raise Refusal('invalid environment')
    if fixed_context_query:
        expected_env = {'HOME': '/Users/tom', 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'LANG': 'C',
                        'DOCKER_CLI_HINTS': 'false', 'DOCKER_CLI_HOOKS': 'false', 'OTEL_SDK_DISABLED': 'true'}
        if (argv != ['/usr/local/bin/docker', 'context', 'show'] or environment != expected_env
                or guest or stdin_path is not None or output_cap > 4096 or pass_fds or verify_in_fixed_helper):
            raise Refusal('context stream protection requires the fixed local context-name query')
    if stdin_path is not None:
        with open(stdin_path, 'rb') as source:
            if hashlib.sha256(source.read()).hexdigest() != stdin_sha256:
                raise Refusal('stdin hash drift: refuse before launch')
    def check_inputs():
        for path, expected in input_hashes.items():
            with open(path, 'rb') as source:
                digest = hashlib.sha256()
                for block in iter(lambda: source.read(1024 * 1024), b''):
                    digest.update(block)
                actual = digest.hexdigest()
            if actual != expected:
                raise Refusal('input hash drift: ' + path)
    if not verify_in_fixed_helper:
        check_inputs()
    if resource_monitor is not None:
        resource_monitor.check()
    record(root / 'intent.json', {'argv': argv, 'cwd': cwd, 'environment': environment, 'timeout_seconds': timeout,
                                  'output_cap_bytes_each': output_cap, 'input_hashes': input_hashes,
                                  'guest': guest, 'stdin_sha256': stdin_sha256, 'deadline_monotonic': deadline})
    # The intent's directory entry must be durable before anything can be spawned.
    root_fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(root_fd)
    finally:
        os.close(root_fd)
    child = None
    streams = {}
    guarded = {'stdout': bytearray(), 'stderr': bytearray()}
    stdin_file = None
    result = {'disposition': 'unknown', 'guest_closure': 'unknown' if guest else 'not-applicable'}
    try:
        for name in ('stdout', 'stderr'):
            fd = os.open(root / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
            streams[name] = os.fdopen(fd, 'wb')
        if not verify_in_fixed_helper:
            check_inputs()
        if stdin_path is not None:
            stdin_file = open(stdin_path, 'rb')
            if hashlib.sha256(stdin_file.read()).hexdigest() != stdin_sha256:
                raise Refusal('stdin hash drift: refuse before launch')
            stdin_file.seek(0)
        start = time.monotonic()
        launch_limit = start + timeout if deadline is None else min(start + timeout, deadline)
        child = subprocess.Popen(argv, stdin=stdin_file if stdin_file is not None else subprocess.DEVNULL,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
                                 cwd=cwd, env=environment, pass_fds=pass_fds)
        identity = {'pid': child.pid, 'pgid': child.pid, 'session_id': child.pid,
                    'deadline_monotonic': launch_limit}
        result['process'] = identity
        if identity_probe is not None:
            identity.update(identity_probe(child.pid))
        record(root / 'process.json', identity)
        sel = selectors.DefaultSelector()
        for name in ('stdout', 'stderr'):
            pipe = getattr(child, name)
            os.set_blocking(pipe.fileno(), False)
            sel.register(pipe, selectors.EVENT_READ, name)
        counts = {'stdout': 0, 'stderr': 0}
        reason = None
        killed_at = None

        def stop_group():
            # Darwin killpg on an exited unreaped leader is EPERM; ESRCH is ProcessLookupError.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
        while True:
            # Do not reap the group leader while a stream remains open: the group
            # may still need to be stopped, and a reaped PID could be reused.
            if not sel.get_map() and child.poll() is not None:
                break
            if resource_monitor is not None and reason is None:
                try:
                    result['resource_usage'] = resource_monitor.check()
                except Exception as exc:
                    reason = 'resource-refusal'
                    result['resource_error'] = str(exc)
                    stop_group()
                    killed_at = time.monotonic()
            if time.monotonic() >= launch_limit and reason is None:
                reason = 'timeout'
                stop_group()
                killed_at = time.monotonic()
            if killed_at is not None and (time.monotonic() - killed_at > 1 or
                                          (cleanup_deadline is not None and time.monotonic() >= cleanup_deadline)):
                for key in list(sel.get_map().values()):
                    sel.unregister(key.fileobj)
                    key.fileobj.close()
                break
            wait = 0.05
            if killed_at is not None and cleanup_deadline is not None:
                wait = min(wait, max(0, cleanup_deadline - time.monotonic()))
            for key, _ in sel.select(wait):
                data = os.read(key.fileobj.fileno(), 65536)
                if not data:
                    sel.unregister(key.fileobj)
                    key.fileobj.close()
                    continue
                name = key.data
                remaining = max(0, output_cap - counts[name])
                if fixed_context_query:
                    if name == 'stdout':
                        guarded[name].extend(data[:remaining])
                else:
                    streams[name].write(data[:remaining])
                counts[name] += len(data)
                if counts[name] > output_cap and reason is None:
                    reason = 'output-cap'
                    stop_group()
                    killed_at = time.monotonic()
        try:
            exit_code = child.wait(timeout=None if cleanup_deadline is None else
                                   max(0, cleanup_deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            exit_code = None
        if resource_monitor is not None and reason is None:
            try:
                result['resource_usage'] = resource_monitor.check()
            except Exception as exc:
                reason = 'resource-refusal'
                result['resource_error'] = str(exc)
        result.update(disposition='unknown' if exit_code is None else reason or ('exited' if exit_code == 0 else 'failed'),
                      waited_exit=exit_code, elapsed_seconds=time.monotonic() - start,
                      observed_bytes=counts)
        if fixed_context_query:
            if (result['disposition'] == 'exited' and counts['stderr'] == 0
                    and re.fullmatch(rb'[A-Za-z0-9][A-Za-z0-9_.-]{0,127}\n', guarded['stdout'])):
                streams['stdout'].write(guarded['stdout'])
                result['context_streams'] = 'original-non-secret'
            else:
                result['context_streams'] = 'suppressed-untrusted'
                if result['disposition'] == 'exited':
                    result['disposition'] = 'context-output-refused'
    except BaseException as exc:
        if child is not None and child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
            try:
                result['waited_exit'] = child.wait(timeout=None if cleanup_deadline is None else
                                                   max(0, cleanup_deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                result['waited_exit'] = None
        elif child is not None:
            result['waited_exit'] = child.returncode
        result.update(disposition='unknown' if child is not None and result.get('waited_exit') is None
                      else 'local-error', error=type(exc).__name__)
        raise
    finally:
        if stdin_file is not None:
            stdin_file.close()
        for stream in streams.values():
            stream.flush()
            os.fsync(stream.fileno())
            stream.close()
        result['streams'] = {}
        for name in streams:
            with open(root / name, 'rb') as source:
                data = source.read()
            result['streams'][name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
        record(root / 'receipt.json', result)
    return result


def main():
    import sys
    if len(sys.argv) != 3:
        raise Refusal('usage: preparation_stage.py REQUEST EXPECTED_SHA256')
    request_path = str(Path(sys.argv[1]).resolve())
    with open(request_path, 'rb') as source:
        raw = source.read(1024 * 1024 + 1)
    if len(raw) > 1024 * 1024 or hashlib.sha256(raw).hexdigest() != sys.argv[2]:
        raise Refusal('request hash mismatch or size exceeded')
    request = json.loads(raw)
    required = {'stage_dir', 'argv', 'timeout_seconds', 'output_cap_bytes', 'input_hashes', 'guest', 'cwd', 'environment', 'resources'}
    if set(request) != required:
        raise Refusal('unsupported request schema')
    if request['guest'] is not False:
        raise Refusal('guest requests require authenticated SSH transport')
    inputs = dict(request['input_hashes'])
    if request_path in inputs:
        raise Refusal('request must not alias a separate input')
    inputs[request_path] = sys.argv[2]
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from preparation_resources import CounterFiles, Monitor
    resource = request['resources']
    if not isinstance(resource, dict) or set(resource) != {'counter_files', 'limits'}:
        raise Refusal('explicit resource accounting required')
    monitor = Monitor(resource['limits'], CounterFiles(resource['counter_files']))
    result = run_stage(request['stage_dir'], request['argv'], request['timeout_seconds'],
                       request['output_cap_bytes'], inputs, request['guest'],
                       cwd=request['cwd'], environment=request['environment'], resource_monitor=monitor)
    print(json.dumps(result, sort_keys=True))
    return 0 if result['disposition'] == 'exited' else 1


if __name__ == '__main__':
    import sys
    sys.exit(main())

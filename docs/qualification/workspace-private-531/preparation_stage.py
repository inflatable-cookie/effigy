"""One-use host stage primitive. Guest closure needs a separate guest receipt."""
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import time


class Refusal(ValueError):
    pass


def exclusive(path, data):
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0)
    fd = os.open(path, flags, 0o600)
    with os.fdopen(fd, 'wb') as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())


def record(path, value):
    exclusive(path, (json.dumps(value, sort_keys=True, indent=2) + '\n').encode())


def run_stage(root, argv, timeout, output_cap, input_hashes, guest=False, identity_probe=None, cwd=None, environment=None):
    """Spend a stage occurrence before launch. A missing receipt is unknown.

    The caller supplies an already private, exclusive directory and exact, reviewed
    argv and input hashes. This primitive cannot establish guest identity or close
    a guest session; it never treats an SSH transport exit as guest closure.
    """
    root = Path(root)
    info = root.lstat()
    if not root.is_dir() or root.is_symlink() or info.st_uid != os.geteuid() or info.st_mode & 0o777 != 0o700:
        raise Refusal('stage directory must be owned and mode 0700')
    if (not isinstance(argv, list) or not argv or any(not isinstance(x, str) or not x or '\x00' in x for x in argv)
            or not 0 < timeout <= 900 or not 0 < output_cap <= 64 * 1024 * 1024):
        raise Refusal('invalid argv, deadline, or output cap')
    if not isinstance(input_hashes, dict) or any(len(v) != 64 or any(c not in '0123456789abcdef' for c in v) for v in input_hashes.values()):
        raise Refusal('input hashes must be SHA-256')
    cwd = str(Path(cwd or os.getcwd()).resolve())
    environment = environment if environment is not None else {k: os.environ[k] for k in ('PATH', 'HOME', 'TMPDIR', 'TMP', 'TEMP', 'LANG') if k in os.environ}
    if not isinstance(environment, dict) or any(not isinstance(k, str) or not isinstance(v, str) or '=' in k or '\x00' in k + v for k, v in environment.items()):
        raise Refusal('invalid environment')
    def check_inputs():
        for path, expected in input_hashes.items():
            with open(path, 'rb') as source:
                actual = hashlib.file_digest(source, 'sha256').hexdigest()
            if actual != expected:
                raise Refusal('input hash drift: ' + path)
    check_inputs()
    record(root / 'intent.json', {'argv': argv, 'cwd': cwd, 'environment': environment, 'timeout_seconds': timeout,
                                  'output_cap_bytes_each': output_cap, 'input_hashes': input_hashes,
                                  'guest': guest})
    child = None
    streams = {}
    result = {'disposition': 'unknown', 'guest_closure': 'unknown' if guest else 'not-applicable'}
    try:
        for name in ('stdout', 'stderr'):
            fd = os.open(root / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
            streams[name] = os.fdopen(fd, 'wb')
        check_inputs()
        start = time.monotonic()
        child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, start_new_session=True, cwd=cwd, env=environment)
        identity = {'pid': child.pid, 'pgid': child.pid, 'session_id': child.pid,
                    'deadline_monotonic': start + timeout}
        if identity_probe is not None:
            identity.update(identity_probe(child.pid))
        record(root / 'process.json', identity)
        result['process'] = identity
        sel = selectors.DefaultSelector()
        for name in ('stdout', 'stderr'):
            pipe = getattr(child, name)
            os.set_blocking(pipe.fileno(), False)
            sel.register(pipe, selectors.EVENT_READ, name)
        counts = {'stdout': 0, 'stderr': 0}
        reason = None
        killed_at = None

        def stop_group():
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        while sel.get_map() or child.poll() is None:
            if time.monotonic() - start >= timeout and reason is None:
                reason = 'timeout'
                stop_group()
                killed_at = time.monotonic()
            if killed_at is not None and time.monotonic() - killed_at > 1:
                for key in list(sel.get_map().values()):
                    sel.unregister(key.fileobj)
                    key.fileobj.close()
                break
            for key, _ in sel.select(0.05):
                data = os.read(key.fileobj.fileno(), 65536)
                if not data:
                    sel.unregister(key.fileobj)
                    key.fileobj.close()
                    continue
                name = key.data
                remaining = max(0, output_cap - counts[name])
                streams[name].write(data[:remaining])
                counts[name] += len(data)
                if counts[name] > output_cap and reason is None:
                    reason = 'output-cap'
                    stop_group()
                    killed_at = time.monotonic()
        exit_code = child.wait()
        result.update(disposition=reason or ('exited' if exit_code == 0 else 'failed'),
                      waited_exit=exit_code, elapsed_seconds=time.monotonic() - start,
                      observed_bytes=counts)
    except BaseException as exc:
        if child is not None and child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
        result.update(disposition='local-error', error=type(exc).__name__)
        raise
    finally:
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

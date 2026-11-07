"""Fail-closed resource accounting for injected, authenticated counters.

Samples are observations, not kernel quotas: callers must provide a separately
verified hard limit for effects whose writes cannot be interrupted at the cap.
"""


class Refusal(ValueError):
    pass


class Monitor:
    def __init__(self, limits, sample):
        if not limits or any(not isinstance(n, str) or not n or not isinstance(v, int) or isinstance(v, bool) or v <= 0 for n, v in limits.items()):
            raise Refusal('invalid resource limits')
        self.limits = dict(limits)
        self.sample = sample
        self.initial = self._read()
        self.last = dict(self.initial)
        self.maxima = dict(self.initial)

    def _read(self):
        values = self.sample()
        if not isinstance(values, dict) or set(values) != set(self.limits):
            raise Refusal('resource telemetry unavailable')
        if any(not isinstance(v, int) or isinstance(v, bool) or v < 0 for v in values.values()):
            raise Refusal('invalid resource counter')
        return values

    def check(self):
        values = self._read()
        if any(values[k] < self.last[k] for k in values):
            raise Refusal('resource counter identity or monotonicity drift')
        self.last = dict(values)
        self.maxima = {k: max(self.maxima[k], values[k]) for k in values}
        used = {k: values[k] - self.initial[k] for k in values}
        if any(used[k] > self.limits[k] for k in values):
            raise Refusal('resource cap exceeded')
        return used


class CounterFiles:
    """Read explicit monotonic byte counters and refuse replacement or parse drift."""
    def __init__(self, paths):
        import os
        from pathlib import Path
        import stat
        if not paths:
            raise Refusal('no resource counters')
        self.paths = {}
        for name, value in paths.items():
            path = Path(value)
            info = path.lstat()
            if path.resolve() != path or not stat.S_ISREG(info.st_mode):
                raise Refusal('counter must be a canonical regular file')
            self.paths[name] = (path, info.st_dev, info.st_ino)

    def __call__(self):
        import os
        result = {}
        for name, (path, device, inode) in self.paths.items():
            fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
            with os.fdopen(fd, 'rb') as source:
                info = os.fstat(source.fileno())
                if (info.st_dev, info.st_ino) != (device, inode):
                    raise Refusal('resource counter identity changed')
                value = source.read(65).strip()
            if not value or len(value) > 64 or not value.isdigit():
                raise Refusal('invalid resource counter content')
            result[name] = int(value)
        return result


def bounded_copy(source, destination, limit):
    """Exclusive streaming transfer; keep a capped partial file on refusal."""
    import hashlib
    import os
    if not isinstance(limit, int) or isinstance(limit, bool) or limit <= 0:
        raise Refusal('invalid transfer cap')
    fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    total = 0
    digest = hashlib.sha256()
    with os.fdopen(fd, 'wb') as output:
        while True:
            chunk = source.read(min(1024 * 1024, limit - total + 1))
            if not chunk:
                break
            allowed = min(len(chunk), limit - total)
            output.write(chunk[:allowed])
            digest.update(chunk[:allowed])
            total += allowed
            if allowed != len(chunk):
                output.flush()
                os.fsync(output.fileno())
                raise Refusal('transfer cap exceeded; partial file retained')
        output.flush()
        os.fsync(output.fileno())
    return {'bytes': total, 'sha256': digest.hexdigest()}

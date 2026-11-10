"""Tasks in the disposable fixture. The guest must forward these to its host."""
import json
import os
import platform
import socket
from pathlib import Path
import signal
import sys
import time

mode = sys.argv[1]
if mode == 'stream':
    assert not any(os.isatty(fd) for fd in (0, 1, 2)), 'unexpected PTY'
    assert sys.stdin.buffer.read() == b'', 'stdin must be EOF'
    Path('stream-host').write_text(json.dumps({'system': platform.system(), 'hostname': socket.gethostname()}))
    print('531-stdout-first', flush=True)
    print('531-stderr-first', file=sys.stderr, flush=True)
    while not Path('stream-continue').exists():
        time.sleep(.01)
    time.sleep(2)
    print('531-stdout-last', flush=True)
    print('531-stderr-last', file=sys.stderr, flush=True)
    sys.exit(23)
elif mode == 'signal':
    os.kill(os.getpid(), signal.SIGTERM)
elif mode == 'owner':
    Path('owner-started').write_text(str(os.getpid()))
    while True:
        time.sleep(1)
elif mode == 'gap':
    # The driver gives the real guest client a slow stdout consumer.
    Path('gap-started').write_text(str(os.getpid()))
    while not Path('gap-go').exists():
        time.sleep(.01)
    # 5.5 MiB exceeds the frozen four 1-MiB log segments.
    sys.stdout.buffer.write(b'531-rotate\n' * (512 * 1024))
    sys.stdout.buffer.flush()
elif mode == 'truncated':
    # A single over-limit line exercises truncation independently of rotation.
    sys.stdout.buffer.write(b'x' * (2 * 1024 * 1024) + b'\n')
    sys.stdout.buffer.flush()
else:
    raise RuntimeError('unknown fixture task')

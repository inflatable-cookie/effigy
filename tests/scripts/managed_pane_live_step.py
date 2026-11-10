"""Prove the non-streaming animation plane through a caller-owned real PTY."""
import os
import re
import signal
import time


def prove_animation(start, drain):
    child, master = start()
    raw = bytearray()
    frames = 0
    try:
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            drain(master, raw)
            frames = len(re.findall(rb"\r\x1b\[2K[^\r\n]*fixture-animation", raw))
            if frames >= 5:
                break
            assert child.poll() is None, (child.returncode, bytes(raw[-4000:]))
        assert frames >= 5, ("non-streaming animation stopped", frames, bytes(raw[-4000:]))
        assert b"\x1b" in raw
        assert child.poll() is None, "animation fixture completed before release"
        os.write(master, b"release\n")
        while child.poll() is None and time.monotonic() < deadline:
            drain(master, raw)
        assert child.poll() == 0, (child.poll(), bytes(raw[-4000:]))
        return bytes(raw)
    finally:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=15)
        os.close(master)

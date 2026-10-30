#!/usr/bin/env python3
"""Prove the canonical safety assertion fails when inverted, then restores."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
command = [str(root / "target/debug/effigy"), "test:focused", "--", "-p", "effigy-kernel", "--features", "fault-injection", "--lib", "-E", "test(generations_messages_and_tap_share_one_redaction_and_durability_boundary)"]
with tempfile.TemporaryDirectory(prefix="managed-output-tripwire-") as scratch:
    receipts = []
    for inverted in (True, False):
        environment = os.environ.copy()
        environment.pop("EFFIGY_TEST_CANONICAL_TRIPWIRE", None)
        if inverted:
            environment["EFFIGY_TEST_CANONICAL_TRIPWIRE"] = "1"
        result = subprocess.run(command, cwd=root, env=environment, capture_output=True, text=True, check=False)
        Path(scratch, "inverted.log" if inverted else "restored.log").write_text(result.stdout + result.stderr)
        receipts.append({"inverted": inverted, "exit": result.returncode})
        if inverted and (result.returncode != 100 or "deliberately inverted canonical safety assertion" not in result.stdout + result.stderr):
            raise SystemExit(f"expected the deliberately inverted assertion to fail; receipts={receipts}; output={result.stdout}{result.stderr}")
        if not inverted and result.returncode:
            raise SystemExit(f"restored assertion failed; receipts={receipts}; output={result.stdout}{result.stderr}")
    print(json.dumps({"proof": "canonical-safety-tripwire", "receipts": receipts, "restored": True}))

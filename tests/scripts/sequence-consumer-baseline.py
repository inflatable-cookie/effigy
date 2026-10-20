#!/usr/bin/env python3
"""Build the exact dispatch baseline inside a fresh private fixture directory."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--commit", required=True)
parser.add_argument("--receipt", required=True, type=Path)
args = parser.parse_args()
commit = subprocess.check_output(["git", "rev-parse", "--verify", args.commit + "^{commit}"], text=True).strip()
root = Path(tempfile.mkdtemp(prefix="effigy-sequence-baseline-"))
archive = root / "source.tar"
with archive.open("wb") as output:
    subprocess.run(["git", "archive", commit], stdout=output, check=True, timeout=60)
source = root / "source"
source.mkdir()
with tarfile.open(archive) as entries:
    for entry in entries:
        # Do not reproduce checkout links or trust paths outside this fixture.
        relative = Path(entry.name)
        if relative.is_absolute() or ".." in relative.parts:
            raise RuntimeError(f"unsafe archive path: {entry.name}")
        target = source / relative
        if entry.isdir():
            target.mkdir(parents=True, exist_ok=True)
        elif entry.isfile():
            target.parent.mkdir(parents=True, exist_ok=True)
            with entries.extractfile(entry) as data:
                target.write_bytes(data.read())
            target.chmod(entry.mode & 0o777)
(source / ".git").mkdir()
env = os.environ.copy()
env["CARGO_TARGET_DIR"] = str(root / "target")
subprocess.run(["cargo", "build", "--locked", "--offline", "--profile", "discovery-proof", "--bin", "effigy"], cwd=source, env=env, check=True, timeout=1200)
receipt = {"commit": commit, "source": str(source), "binary": str(root / "target/discovery-proof/effigy")}
args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt), flush=True)

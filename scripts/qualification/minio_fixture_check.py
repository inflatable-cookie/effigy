#!/usr/bin/env python3
"""Compile the maintained Python entry points without writing checkout caches."""
import ast
import json
from pathlib import Path
import subprocess

from minio_fixture_package import INPUTS, validate_inputs
from minio_fixture_native import script

paths = list(Path(__file__).parent.glob("minio_fixture*.py"))
paths.append(Path("tests/scripts/minio-fixture-controls.py"))
for path in paths:
    ast.parse(path.read_bytes(), filename=str(path))
validate_inputs(json.loads(INPUTS.read_text()))
shell = subprocess.run(["/bin/sh", "-n"], input=script("fixture:private", "public-test-user", "public-test-value", "sha256:" + "0" * 64), capture_output=True, timeout=5)
if shell.returncode:
    raise SystemExit("native shell syntax validation failed")
print(json.dumps({"compiledScripts": len(paths), "inputPins": "valid", "runtimeEffects": False}))

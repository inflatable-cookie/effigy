"""Exercise the public contract checker under one real private QA parent."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


binary = Path(sys.argv[1]).resolve()
mode = sys.argv[2]
assert mode in ("--fast", "--full")
root = Path.cwd()
assert os.environ.get("HOST_RUN_TOKEN") and os.environ.get("HOST_RUN_ID")
assert os.environ.get("EFFIGY_HOST_RUN_ROOT"), "private endpoint required"
index = json.loads((root / "docs/knowledge/contracts/json-schema-index.json").read_text())
rows = [row for row in index["schemas"] if row["status"] == "active"]
assert not any(row["schema"] == "effigy.test.results.v1" for row in rows)
evidence = root / ".effigy/reports/contract-fixtures-public" / str(os.getpid())
evidence.mkdir(parents=True)


def run(label, args, expected, environment=None):
    result = subprocess.run(
        [str(binary), "--json", *args], cwd=root,
        env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    for stream in ("stdout", "stderr"):
        path = evidence / (label + "." + stream + ".log")
        path.write_bytes(getattr(result, stream))
        path.chmod(0o600)
    assert result.returncode == expected, (label, result.returncode, expected)
    payload = json.loads(result.stdout)
    if payload.get("schema") == "effigy.command.v1":
        return payload.get("result") or payload.get("error", {}).get("details") or payload
    return payload


if mode == "--fast":
    with tempfile.TemporaryDirectory(prefix="effigy-contract-outside-") as outside:
        outside = Path(outside)
        source, consumer, group = (outside / name for name in ("source", "consumer", "group"))
        for directory in (source, consumer, group):
            directory.mkdir()
            (directory / "package.json").write_text("{}\n")
        (source / "effigy.toml").write_text(
            '[catalog]\nalias = "contract-negative"\n\n'
            '[tasks.probe]\nrun = "printf effect > effect.marker"\nrun_in = "host"\n'
        )
        (consumer / "effigy.toml").write_text('[catalog]\nalias = "consumer"\n')
        (group / "effigy.toml").write_text(
            '[tasks.probe-ok]\nrun = "printf effect > effect.marker"\nrun_in = "host"\n\n'
            '[qa.groups.release-probe]\nlifecycle = "maintained"\npurpose = "Outside-root control"\n'
            'scope_policy = "advisory"\nexpected_wall_ms = 600000\n'
            'expectation_basis = "Deterministic marker"\nproof_limits = ["Fixture only"]\n'
            'members = [{ id = "first", kind = "test", surface = "published", '
            'task = "probe-ok", args = [], targets = ["workspace:root"], '
            'covers = ["workspace:root"], limits = ["Fixture only"] }]\n'
        )
        skill = ["skill", "run", "--path", str(source), "contract-negative/probe", "--repo", str(consumer)]
        qa = ["tasks", "qa-group", "run", "release-probe", "--repo", str(group)]
        refusal = run("outside-skill", skill, 77)
        assert "invalid_parent_token" in refusal["error"]["message"]
        assert not (consumer / "effect.marker").exists()
        refusal = run("outside-group", qa, 1)
        assert refusal["members"][0]["state"] == "failed"
        assert "invalid_parent_token" in refusal["members"][0]["summary"]
        assert not (group / "effect.marker").exists()
    
        # Only these explicit no-parent controls remove the parent identity. The
        # positive checker cases below retain its original token and trusted root.
        no_parent = os.environ.copy()
        no_parent.pop("HOST_RUN_TOKEN", None)
        no_parent.pop("HOST_RUN_ID", None)
        run("no-parent-skill", skill, 0, no_parent)
        assert (consumer / "effect.marker").is_file()
        run("no-parent-group", qa, 0, no_parent)
        assert (group / "effect.marker").is_file()

execution_fixtures = root / ".effigy/tmp/contract-fixtures"
execution_fixtures.mkdir(parents=True, exist_ok=True)
before = set(execution_fixtures.iterdir())
with tempfile.NamedTemporaryFile(dir=execution_fixtures, prefix="foreign-sentinel-") as sentinel:
    report = run(mode[2:], ["contracts", "check-json", mode], 0)
    assert report["schema"] == "effigy.contracts.check-json.v1"
    assert report["ok"] and report["checks"] == len(rows)
    assert report["skipped"] == 0 and report["failures"] == []
    assert set(execution_fixtures.iterdir()) == before | {Path(sentinel.name)}
    assert Path(sentinel.name).is_file()
    print(f"public contract {mode}: {len(rows)} rows passed, zero skipped; owned roots cleaned")
assert set(execution_fixtures.iterdir()) == before
if mode == "--fast":
    print("outside skill/group refused before effects; no-parent controls passed; foreign sentinel preserved")
print(f"native public contract logs: {evidence}")

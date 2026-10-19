#!/usr/bin/env python3
"""Read-only visibility evidence using the current build and existing consumers."""
import json
import re
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path("target/debug/effigy").resolve()
evidence = Path(tempfile.mkdtemp(prefix="effigy-task-visibility-"))
for arg in sys.argv[1:]:
    label, directory = arg.split("=", 1)
    if not re.fullmatch(r"[a-z][a-z0-9-]*", label):
        raise ValueError("evidence labels must be lowercase names")
    root = Path(directory).resolve()
    output = subprocess.run([str(binary), "tasks", "--json"], cwd=root, capture_output=True, text=True, check=True)
    data = json.loads(output.stdout)["result"]
    (evidence / f"{label}-after.json").write_text(output.stdout)
    text = subprocess.run([str(binary), "tasks"], cwd=root, capture_output=True, text=True, check=True)
    (evidence / f"{label}-human.txt").write_text(text.stdout)
    rows = [r for r in data["catalog_tasks"] if r["task"] is not None]
    public = [r["task"] for r in rows if r["visibility"] == "public"]
    internal = [r["task"] for r in rows if r["visibility"] == "internal"]
    print(json.dumps({"consumer": label, "root": str(root), "total": len(rows), "public": len(public), "internal": len(internal), "public_selectors": public if len(public) <= 30 else "see snapshot"}))
    inspect = subprocess.run([str(binary), "bundle", "inspect", "--json"], cwd=root, capture_output=True, text=True)
    if inspect.returncode == 0:
        source = json.loads(inspect.stdout)["result"].get("source")
        if source and source.get("local_path"):
            export = Path(source["local_path"]) / "export.toml"
            if export.is_file():
                names = {quoted or bare for quoted, bare in re.findall(r'^\[tasks\.(?:"([^"\n]+)"|([^]\n]+))\]', export.read_text(), re.M)}
                bundle_rows = [r for r in rows if r["task"] in names]
                cohort = {"consumer": label + " bundle exports", "revision": source.get("version_hint"), "total": len(bundle_rows), "public": [r["task"] for r in bundle_rows if r["visibility"] == "public"], "internal": [r["task"] for r in bundle_rows if r["visibility"] == "internal"]}
                (evidence / f"{label}-bundle.json").write_text(json.dumps(cohort, indent=2))
                print(json.dumps(cohort))
print(f"evidence: {evidence}")

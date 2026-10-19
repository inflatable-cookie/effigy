#!/usr/bin/env python3
"""Inventory declared sequences and check their plans without executing tasks.

This proves manifest/selection compatibility, not live application behavior.
Consumer roots are read only. Pass a report path inside a fresh temporary directory.
"""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import re
import subprocess


def inventory(binary, root):
    # Use the runner's effective catalog inventory, including composed bundles
    # and includes; filesystem scans also find unrelated starter/fixture files.
    completed = subprocess.run([str(binary), "--json", "tasks"], cwd=root,
                               capture_output=True, text=True, timeout=90)
    payload = json.loads(completed.stdout)
    if completed.returncode != 0 or not payload.get("ok"):
        raise RuntimeError(f"cannot inventory {root}: {payload.get('error') or completed.stderr}")
    result = payload["result"]
    catalogs = {catalog["manifest"]: catalog for catalog in result["catalogs"]}
    rows = []
    for task in result["catalog_tasks"]:
        sequence = re.fullmatch(r"<sequence:(\d+)>", (task.get("run") or ""))
        if sequence is None:
            continue
        catalog = catalogs[task["manifest"]]
        selector = task["task"]
        rows.append({"selector": selector, "task": task["task"].rsplit("/", 1)[-1], "source": task["manifest"], "step_count": int(sequence[1])})
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--root", required=True, action="append", type=Path)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--selector", action="append", help="check only these exact selectors for incremental evidence")
    args = parser.parse_args()
    binary = args.binary.resolve()
    baseline = args.baseline.resolve()
    report = {"proof": "read-only sequence plans; no consumer workloads executed", "roots": []}
    failures = []
    for root_index, path in enumerate(args.root):
        root = path.resolve()
        rows = inventory(binary, root)
        if args.selector:
            rows = [row for row in rows if row["selector"] in args.selector]
            if not rows:
                raise RuntimeError(f"no requested sequence selectors in {root}")
        for row in rows:
            try:
                completed = subprocess.run([str(binary), "--json", "--plan", row["selector"]], cwd=root,
                                           capture_output=True, text=True, timeout=90)
                payload = json.loads(completed.stdout)
                plan = payload.get("result") or {}
                before_run = subprocess.run([str(baseline), "--json", "--plan", row["selector"]], cwd=root,
                                            capture_output=True, text=True, timeout=90)
                before = json.loads(before_run.stdout)
                row["before"] = before
                row["after"] = payload
                before_plan = before.get("result") or {}
                comparable = ["catalog", "cwd", "task", "selector", "runtime"]
                row["routing_changes"] = {key: {"before": before_plan.get(key), "after": plan.get(key)}
                                          for key in comparable if before_plan and before_plan.get(key) != plan.get(key)}
                row["before_exit"] = before_run.returncode
                row["after_exit"] = completed.returncode
                digest = hashlib.sha256(row["selector"].encode()).hexdigest()[:16]
                diff = "".join(difflib.unified_diff(json.dumps(before, indent=2, sort_keys=True).splitlines(True),
                                                   json.dumps(payload, indent=2, sort_keys=True).splitlines(True),
                                                   fromfile="before/" + row["selector"], tofile="after/" + row["selector"]))
                diff_path = args.report.parent / f"root-{root_index}-{digest}.diff"
                diff_path.write_text(diff)
                row["diff"] = str(diff_path)
                row["diff_sha256"] = hashlib.sha256(diff.encode()).hexdigest()
                row["runtime"] = plan.get("runtime")
                row["ok"] = (completed.returncode == 0 and payload.get("ok") is True
                             and plan.get("executed") is False and plan.get("task") == row["task"]
                             and plan.get("catalog", {}).get("manifest") == row["source"]
                             and plan.get("command", "").startswith(f"sequence ({row['step_count']} ordered steps):")
                             and not row["routing_changes"])
                if not row["ok"]:
                    row["error"] = payload.get("error") or completed.stderr or plan
            except (subprocess.TimeoutExpired, json.JSONDecodeError, OSError) as error:
                row.update(ok=False, error=str(error))
            if not row["ok"]:
                failures.append(f"{root}: {row['selector']}: {row['error']}")
        report["roots"].append({"root": str(root), "sequences": rows})
        print(f"{root}: {len(rows)} sequence plans, {sum(not row['ok'] for row in rows)} failures", flush=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    for failure in failures[:10]:
        print(failure)
    if failures:
        print(f"{len(failures)} failures; full details: {args.report}")
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())

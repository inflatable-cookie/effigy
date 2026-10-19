#!/usr/bin/env python3
"""Read-only shadow diff of selector resolution across consumer roots.

Runs `effigy tasks --resolve <selector> --json` with an old and a new binary
over every declared selector of every consumer root, from the root and from
inside each member catalog, and reports each difference. Consumer
repositories are never touched: manifests, includes, root markers and bundle
caches are mirrored into a private temporary tree first, Git state becomes an
empty `.git` directory, bundle remote checks are disabled, and both binaries
run with a private HOME.

Usage:
  resolver-shadow-diff.py --old OLD_EFFIGY --new NEW_EFFIGY --roots ROOTS_FILE
      [--base ~/Dev] [--report REPORT.json] [--jobs N]

ROOTS_FILE lists one consumer root per line (absolute, or relative to
--base). Exit status is 0 whether or not differences exist; the report is
the result.
"""

import argparse
import concurrent.futures
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

PRUNE = [
    "node_modules", "vendor", "target", ".git", ".paseo", "dist", "build",
    ".next", ".turbo", ".cache", "coverage", "storage",
]
MIRRORED = ["*.toml", "package.json", "composer.json", ".env.schema"]


def mirror(source: Path, destination: Path) -> None:
    """Copies only what selection reads: manifests, includes, root markers,
    env schemas and the checkout's bundle cache."""
    args = ["rsync", "-a", "--prune-empty-dirs"]
    args += ["--include=/.effigy/", "--include=/.effigy/cache/", "--include=/.effigy/cache/bundles/***"]
    for name in PRUNE + [".effigy"]:
        args.append(f"--exclude={name}/")
    args.append("--include=*/")
    for pattern in MIRRORED:
        args.append(f"--include={pattern}")
    args += ["--exclude=*", f"{source}/", f"{destination}/"]
    destination.mkdir(parents=True, exist_ok=True)
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if (source / ".git").exists():
        (destination / ".git").mkdir(exist_ok=True)


def run(binary: str, cwd: Path, home: Path, args: list) -> dict:
    env = {
        "HOME": str(home),
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "EFFIGY_GIT_BUNDLE_REMOTE_CHECK_TTL_SECS": "999999999",
        "EFFIGY_OUTPUT": "plain",
        "NO_COLOR": "1",
    }
    try:
        completed = subprocess.run(
            [binary, *args], cwd=cwd, env=env, capture_output=True, text=True, timeout=60
        )
    except subprocess.TimeoutExpired:
        return {"_error": "timeout"}
    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError:
        return {"_error": (completed.stderr or completed.stdout).strip()[:400]}
    if not isinstance(payload, dict):
        return {"_error": "shape"}
    if "result" in payload and payload["result"] is None:
        return {"_error": json.dumps(payload.get("error"))[:400]}
    return payload.get("result", payload)


def classify(probe: dict) -> dict:
    """Comparable facts of one resolution, from either binary's shape."""
    if "_error" in probe:
        return {"route": "unavailable", "detail": probe["_error"]}
    resolve = probe.get("resolve") or {}
    status = resolve.get("status")
    route = resolve.get("route")
    if route is None:  # old probe: no route field
        if status != "ok":
            route = "error"
        elif resolve.get("catalog"):
            route = "task"
        else:
            route = "builtin"
    facts = {"route": route}
    if route in ("task", "defer"):
        facts["catalog_root"] = resolve.get("catalog_root")
    if route == "error":
        facts["error"] = (resolve.get("error") or "")[:200]
    return facts


def member_dirs(listing: dict, root: Path) -> list:
    manifests = {Path(task["manifest"]).parent for task in listing.get("catalog_tasks", [])}
    return sorted(path for path in manifests if path != root and root in path.parents)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--old", required=True)
    parser.add_argument("--new", required=True)
    parser.add_argument("--roots", required=True)
    parser.add_argument("--base", default=str(Path.home() / "Dev"))
    parser.add_argument("--report")
    parser.add_argument("--jobs", type=int, default=max(2, (os.cpu_count() or 4) // 2))
    options = parser.parse_args()

    options.old = str(Path(options.old).expanduser().resolve())
    options.new = str(Path(options.new).expanduser().resolve())
    base = Path(options.base).expanduser().resolve()
    roots = []
    for line in Path(options.roots).read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        path = Path(line).expanduser()
        roots.append((path if path.is_absolute() else base / path).resolve())

    work = Path(tempfile.mkdtemp(prefix="effigy-shadow-diff-"))
    mirror_base = work / "mirror"
    differences = []
    compared = 0
    try:
        for root in roots:
            if not (root / "effigy.toml").is_file():
                differences.append({"root": str(root), "kind": "missing-root"})
                continue
            mirror(root, mirror_base / root.relative_to(base))
        cases = []
        for root in roots:
            mirrored = mirror_base / root.relative_to(base)
            if not (mirrored / "effigy.toml").is_file():
                continue
            homes = {name: work / f"home-{name}" / root.relative_to(base) for name in ("old", "new")}
            for home in homes.values():
                home.mkdir(parents=True, exist_ok=True)
            listing = run(options.new, mirrored, homes["new"], ["tasks", "--json"])
            if "_error" in listing:
                old_listing = run(options.old, mirrored, homes["old"], ["tasks", "--json"])
                differences.append(
                    {
                        "root": str(root.relative_to(base)),
                        "kind": "listing-unavailable",
                        "new": listing["_error"],
                        "old": old_listing.get("_error", "ok"),
                    }
                )
                continue
            selectors = sorted(
                {item["selector"] for item in listing.get("selectors", []) if item.get("kind") == "task"}
            )
            root_cases = [(mirrored, selector) for selector in selectors]
            for member in member_dirs(listing, mirrored):
                bare = sorted(
                    {
                        task["task"]
                        for task in listing.get("catalog_tasks", [])
                        if Path(task["manifest"]).parent == member
                    }
                )
                root_cases += [(member, name) for name in bare]
            cases += [(root, homes, cwd, selector) for cwd, selector in root_cases]
        print(f"roots={len(roots)} cases={len(cases)} jobs={options.jobs}", file=sys.stderr, flush=True)

        def strip(facts):
            return {
                key: str(value).replace(str(mirror_base), "~/Dev") if isinstance(value, str) else value
                for key, value in facts.items()
            }

        def probe(case):
            root, homes, cwd, selector = case
            args = ["tasks", "--resolve", selector, "--json"]
            old = strip(classify(run(options.old, cwd, homes["old"], args)))
            new = strip(classify(run(options.new, cwd, homes["new"], args)))
            if old == new:
                return None
            return {
                "root": str(root.relative_to(base)),
                "cwd": str(cwd.relative_to(mirror_base)),
                "selector": selector,
                "old": old,
                "new": new,
            }

        with concurrent.futures.ThreadPoolExecutor(max_workers=options.jobs) as pool:
            for index, result in enumerate(pool.map(probe, cases), start=1):
                compared += 1
                if result is not None:
                    differences.append(result)
                if index % 500 == 0:
                    print(f"compared {index}/{len(cases)}", file=sys.stderr, flush=True)
        report = {"roots": len(roots), "compared": compared, "differences": differences}
        text = json.dumps(report, indent=2)
        if options.report:
            Path(options.report).write_text(text + "\n")
        print(f"roots={len(roots)} compared={compared} differences={len(differences)}")
        kinds = {}
        for item in differences:
            if "kind" in item:
                key = (item["kind"], item["kind"])
            else:
                key = (item["old"].get("route"), item["new"].get("route"))
            kinds[key] = kinds.get(key, 0) + 1
        for (old_route, new_route), count in sorted(kinds.items(), key=lambda kv: -kv[1]):
            print(f"  {old_route} -> {new_route}: {count}")
        return 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())

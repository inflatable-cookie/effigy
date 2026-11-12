#!/usr/bin/env python3
"""Report drift between the frozen test quality tables and the current tree."""

from __future__ import annotations

import argparse
import ast
import csv
import hashlib
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
TABLE_DIR = ROOT / "docs/knowledge/quality/tests"
RATED_COMMIT = "b5b54db3538b57416f8cc1b8d0fb1e8ec0d0bae8"
EXPECTED_ROWS = {"T1": 1317, "T2": 627, "T3": 858, "T4": 1797, "T5": 1859, "T6": 1264}
_GIT_CACHE: dict[tuple[str, str], str | None] = {}


def sha256(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def git_text(revision: str, path: str) -> str | None:
    key = (revision, path)
    if key in _GIT_CACHE:
        return _GIT_CACHE[key]
    result = subprocess.run(
        ["git", "show", f"{revision}:{path}"], cwd=ROOT,
        capture_output=True, text=True, check=False,
    )
    value = result.stdout if result.returncode == 0 else None
    _GIT_CACHE[key] = value
    return value


def rows() -> list[dict[str, str]]:
    result = []
    for table in sorted(TABLE_DIR.glob("T[1-6].csv")):
        with table.open(newline="", encoding="utf-8") as source:
            result.extend(csv.DictReader(source))
    return result


def rust_function(text: str, name: str) -> str | None:
    """Return a Rust function item, balancing braces from its body opener."""
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*(?:<[^{{}}]*>)?\s*\([^{{}}]*\)[^{{;]*\{{", text, re.S)
    if not match:
        return None
    start = match.start()
    depth = 0
    in_string = False
    escaped = False
    for index in range(match.end() - 1, len(text)):
        char = text[index]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return text[start:index + 1]
    return None


def python_function(text: str, class_name: str, name: str) -> str | None:
    try:
        tree = ast.parse(text)
    except SyntaxError:
        return None
    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef) and node.name == class_name:
            for child in node.body:
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)) and child.name == name:
                    segment = ast.get_source_segment(text, child)
                    return segment
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == name:
            return ast.get_source_segment(text, node)
    return None


def has_test_attribute(text: str, function_start: int) -> bool:
    """Require a test attribute to be attached directly to this Rust item."""
    prefix = text[:function_start]
    matches = list(re.finditer(r"#\[(?:tokio::)?test(?:\s*\([^]]*\))?\]", prefix))
    if not matches:
        return False
    gap = prefix[matches[-1].end():]
    # A test item may carry `async`, `unsafe`, `const`, `extern "..."` or a
    # visibility modifier between its last attribute and `fn`.
    return re.fullmatch(
        r"\s*(?:#\[[^]]+\]\s*)*(?:(?:pub(?:\([^)]*\))?|async|unsafe|const|extern\s+\"[^\"]*\")\s+)*",
        gap,
    ) is not None


def locate_body(revision: str, row: dict[str, str]) -> str | None:
    locator = row["file_line"]
    path, _, line_text = locator.rpartition(":")
    text = git_text(revision, path)
    if text is None:
        return None
    test_id = row["id"]
    if test_id.startswith("py:"):
        parts = test_id[3:].split("::")
        if len(parts) >= 3:
            return python_function(text, parts[-2], parts[-1])
        if len(parts) == 2:
            return python_function(text, "", parts[-1])
        return None
    name = test_id.rsplit("::", 1)[-1]
    body = rust_function(text, name)
    if body:
        return body
    return None


def discover_current() -> tuple[dict[str, str], set[str], int]:
    """Find directly declared Rust tests and Python unittest-style tests."""
    found: dict[str, str] = {}
    macro_count = 0
    for path in ROOT.rglob("*.rs"):
        if any(part in {"target", ".git", "vendor"} for part in path.parts):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for match in re.finditer(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^{}]*>)?\s*\([^{}]*\)[^{;]*\{", text, re.S):
            if not has_test_attribute(text, match.start()):
                continue
            name = match.group(1)
            body = rust_function(text[match.start():], name)
            if body:
                key = f"{path.relative_to(ROOT).as_posix()}::{name}"
                found[key] = sha256(body)
    for path in ROOT.rglob("*.py"):
        if any(part in {".git", "target", "venv"} for part in path.parts):
            continue
        try:
            text = path.read_text(encoding="utf-8")
            tree = ast.parse(text)
        except (OSError, UnicodeDecodeError, SyntaxError):
            continue
        rel = path.relative_to(ROOT).as_posix()
        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name.startswith("test_"):
                segment = ast.get_source_segment(text, node)
                if segment:
                    found[f"py:{rel}::{node.name}"] = sha256(segment)
    return found, set(), macro_count


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--summary", action="store_true", help="print the milestone summary line only")
    args = parser.parse_args()

    rated_rows = rows()
    if not rated_rows:
        print(f"quality-table: no rating rows found in {TABLE_DIR}", file=sys.stderr)
        return 2
    current, _, _ = discover_current()
    by_id = {row["id"]: row for row in rated_rows}
    row_keys = {}
    for row in rated_rows:
        source_path = row["file_line"].rsplit(":", 1)[0]
        name = row["id"].rsplit("::", 1)[-1]
        key = f"py:{source_path}::{name}" if row["id"].startswith("py:") else f"{source_path}::{name}"
        row_keys[key] = row
    base_direct: set[str] = set()
    macro_untracked = []
    for row in rated_rows:
        body = locate_body(RATED_COMMIT, row)
        if body is None:
            macro_untracked.append(row["id"])
            continue
        base_direct.add(row["id"])
    # The checked-in body_sha records the base snapshot, including macro
    # source-line hashes for cases the source scanner cannot enumerate.
    added_keys = sorted(set(current) - set(row_keys))
    gone = sorted(row["id"] for key, row in row_keys.items() if key not in current and row["id"] in base_direct)
    changed = sorted(row["id"] for key, row in row_keys.items() if key in current and row.get("body_sha") and current[key] != row["body_sha"])
    added = [key.rsplit("::", 1)[-1] + " (" + key.rsplit("::", 1)[0] + ")" for key in added_keys]
    fresh = max(0, len(by_id) - len(gone) - len(changed))
    stale = len(added) + len(changed) + len(gone)
    denominator = len(by_id) + len(added)
    percent = (100 * stale / denominator) if denominator else 0.0
    if args.summary:
        print(f"quality tables: {len(by_id)} rows, {len(added)} new, {len(changed)} changed, {len(gone)} gone, {percent:.1f}% stale since {RATED_COMMIT[:9]}")
        return 0
    print(f"quality tables: {len(by_id)} rows; {len(added)} new; {len(changed)} changed; {len(gone)} gone; {fresh} fresh; {percent:.1f}% stale since {RATED_COMMIT[:9]}")
    for label, items in (("new (no row)", added), ("changed (body hash differs)", changed), ("gone (row no longer exists)", gone)):
        print(f"\n{label}: {len(items)}")
        for item in items:
            print(f"  {item}")
    print(f"\nmacro cases not tracked: {len(macro_untracked)} generated/group rows are not directly declared at the rated commit and are excluded from new/changed/gone totals.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

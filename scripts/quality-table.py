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
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
TABLE_DIR = ROOT / "docs/knowledge/quality/tests"
RATED_COMMIT = "7a502af038b7fbc4889d6484ea78deb25c7daac5"
CODE_TABLE_DIR = ROOT / "docs/knowledge/quality/code"
CODE_RATED_COMMIT = "e458614d83b20685d9018943d71ae3eb79e88556"
EXPECTED_ROWS = {"T1": 1305, "T2": 623, "T3": 858, "T4": 1793, "T5": 1857, "T6": 1264}
CODE_EXPECTED_ROWS = {"C1": 196, "C2": 74, "C3": 105, "C4": 182, "C5": 242}
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


def junit_cases(paths: list[Path]) -> list[dict[str, object]]:
    """Read nextest JUnit files, retaining the source filename as selector."""
    cases = []
    for path in paths:
        root = ET.parse(path).getroot()
        report_id = root.get("uuid", "")
        report_time = root.get("timestamp", "")
        for suite in root.iter("testsuite"):
            binary = suite.get("name", "unknown")
            for case in suite.findall("testcase"):
                try:
                    seconds = float(case.get("time", ""))
                except ValueError:
                    continue
                cases.append({
                    "selector": path.stem,
                    "binary": binary,
                    "name": case.get("name", ""),
                    "classname": case.get("classname", ""),
                    "duration_ms": round(seconds * 1000),
                    "report_id": report_id,
                    "report_time": report_time,
                })
    return cases


def group_filters(config: Path) -> dict[str, list[str]]:
    """Collect test-name predicates for serialized nextest groups."""
    def test_patterns(expression: str) -> list[str]:
        patterns = []
        cursor = 0
        while True:
            start = expression.find("test(", cursor)
            if start < 0:
                break
            depth = 1
            end = start + len("test(")
            while end < len(expression) and depth:
                if expression[end] == "(":
                    depth += 1
                elif expression[end] == ")":
                    depth -= 1
                end += 1
            if depth:
                break
            item = expression[start + len("test("):end - 1].strip()
            if item.startswith("/") and item.endswith("/"):
                patterns.append(item[1:-1])
            elif item:
                patterns.append(re.escape(item))
            cursor = end
        return patterns

    text = config.read_text(encoding="utf-8")
    groups: dict[str, list[str]] = {}
    for block in re.split(r"(?=\[\[profile\.[^\]]+\.overrides\]\])", text):
        group = re.search(r"test-group\s*=\s*['\"]([^'\"]+)['\"]", block)
        selected = re.search(r"filter\s*=\s*'([^']*)'", block)
        if not group or not selected:
            continue
        patterns = test_patterns(selected.group(1))
        groups.setdefault(group.group(1), []).extend(patterns)
    return groups


def group_limits(config: Path) -> dict[str, int]:
    """Return configured concurrency limits for nextest test groups."""
    text = config.read_text(encoding="utf-8")
    limits = {}
    for match in re.finditer(r"\[test-groups\.([^\]]+)\]([^\[]*)", text):
        threads = re.search(r"max-threads\s*=\s*(\d+)", match.group(2))
        if threads:
            limits[match.group(1)] = int(threads.group(1))
    return limits


def match_quality_row(case: dict[str, object], rated_rows: list[dict[str, str]]) -> str | None:
    """Bind a JUnit identity to one rated row without guessing between siblings."""
    classname = str(case["classname"])
    name = str(case["name"])
    exact = f"{classname}::{name}" if classname else name
    normalize = lambda value: value.replace("::tests::", "::")
    matches = [row["id"] for row in rated_rows if normalize(row["id"]) == normalize(exact)]
    if len(matches) == 1:
        return matches[0]

    package = classname.split("::", 1)[0]
    wanted = normalize(exact).split("::")
    candidates = [row["id"] for row in rated_rows
                  if row["id"].startswith(package + "::")]
    scored = []
    for candidate in candidates:
        actual = normalize(candidate).split("::")
        common = 0
        for left, right in zip(reversed(wanted), reversed(actual)):
            if left != right:
                break
            common += 1
        if common:
            scored.append((common, candidate))
    if not scored:
        return None
    best = max(score for score, _ in scored)
    best_matches = [candidate for score, candidate in scored if score == best]
    return best_matches[0] if len(best_matches) == 1 else None


def print_durations(paths: list[Path], limit: int, write_tables: bool = True) -> int:
    cases = junit_cases(paths)
    if not cases:
        print("quality-table: no testcase durations found", file=sys.stderr)
        return 2
    print(f"JUnit: {len(cases)} testcases from {len(paths)} report(s)")
    reports = {}
    for case in cases:
        if case["report_id"]:
            reports[str(case["selector"])] = (case["report_id"], case["report_time"])
    for selector, (report_id, report_time) in sorted(reports.items()):
        print(f"  source {selector}: {report_id} ({report_time})")
    print(f"\nTop {limit} slowest tests:")
    for case in sorted(cases, key=lambda item: int(item["duration_ms"]), reverse=True)[:limit]:
        print(f"  {int(case['duration_ms']):>9} ms  {case['selector']} / {case['binary']}::{case['name']}")
    for selector in sorted({str(case["selector"]) for case in cases}):
        selected = [case for case in cases if case["selector"] == selector]
        print(f"\nSelector {selector}: {sum(int(case['duration_ms']) for case in selected)} ms total")
        binaries = sorted({str(case["binary"]) for case in selected})
        for binary in binaries:
            members = [case for case in selected if case["binary"] == binary]
            print(f"  binary {binary}: {sum(int(case['duration_ms']) for case in members)} ms")

    root = ROOT
    grouped = group_filters(root / ".config/nextest.toml")
    grouped.update(group_filters(root / "tests/config/kernel-faults-nextest.toml"))
    grouped.update(group_filters(root / "tests/config/gateway-nextest.toml"))
    limits = group_limits(root / ".config/nextest.toml")
    limits.update(group_limits(root / "tests/config/kernel-faults-nextest.toml"))
    limits.update(group_limits(root / "tests/config/gateway-nextest.toml"))
    for name, patterns in sorted(grouped.items()):
        members = [case for case in cases
                   if any(re.search(pattern, str(case["name"])) for pattern in patterns)]
        if members:
            print(f"\nTest group {name}: {sum(int(case['duration_ms']) for case in members)} ms "
                  f"({len(members)} tests, max threads {limits.get(name, 'unknown')})")

    rated_rows = rows()
    by_test: dict[str, list[dict[str, str]]] = {}
    for row in rated_rows:
        by_test.setdefault(row["id"], []).append(row)
    updates: dict[str, int] = {}
    unmatched = 0
    for case in cases:
        row_id = match_quality_row(case, rated_rows)
        if row_id:
            updates[row_id] = max(updates.get(row_id, 0), int(case["duration_ms"]))
        else:
            unmatched += 1
    for table in sorted(TABLE_DIR.glob("T[1-6].csv")) if write_tables else []:
        with table.open(newline="", encoding="utf-8") as source:
            reader = csv.DictReader(source)
            fields = list(reader.fieldnames or [])
            table_rows = list(reader)
        if "duration_ms" not in fields:
            fields.append("duration_ms")
        for row in table_rows:
            if row["id"] in updates:
                row["duration_ms"] = str(updates[row["id"]])
            else:
                row.setdefault("duration_ms", "")
        with table.open("w", newline="", encoding="utf-8") as destination:
            writer = csv.DictWriter(destination, fieldnames=fields, lineterminator="\n")
            writer.writeheader()
            writer.writerows(table_rows)
    action = "updated" if write_tables else "would update"
    print(f"\nQuality tables {action}: {len(updates)} rows; {unmatched} JUnit cases unmatched or ambiguous")
    return 0


# A raw string keeps its braces: `r"..."`, `r#"..."#`, `br#"..."#`, `cr#"..."#`.
RAW_STRING = re.compile(r'(?:b|c)?r(#*)"')


def skip_quoted(text: str, index: int) -> int:
    """Return the index after a `"`-quoted Rust string, honouring escapes."""
    index += 1
    while index < len(text):
        char = text[index]
        if char == "\\":
            index += 2
            continue
        if char == '"':
            return index + 1
        index += 1
    return index


def skip_block_comment(text: str, index: int) -> int:
    """Return the index after a nested Rust block comment starting at `/*`."""
    depth = 0
    while index < len(text):
        if text.startswith("/*", index):
            depth += 1
            index += 2
            continue
        if text.startswith("*/", index):
            depth -= 1
            index += 2
            if depth == 0:
                return index
            continue
        index += 1
    return index


def char_literal_end(text: str, index: int) -> int | None:
    """Return the index after a char literal; None for a lifetime or label."""
    if index + 1 >= len(text):
        return None
    if text[index + 1] == "\\":
        cursor = index + 2
        while cursor < len(text):
            if text[cursor] == "\\":
                cursor += 2
                continue
            if text[cursor] == "'":
                return cursor + 1
            cursor += 1
        return None
    if index + 2 < len(text) and text[index + 2] == "'" and text[index + 1] != "\n":
        return index + 3
    return None


def rust_function(text: str, name: str) -> str | None:
    """Return a Rust function item, balancing braces from its body opener."""
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*(?:<[^{{}}]*>)?\s*\([^{{}}]*\)[^{{;]*\{{", text, re.S)
    if not match:
        return None
    start = match.start()
    depth = 0
    index = match.end() - 1
    while index < len(text):
        char = text[index]
        if text.startswith("//", index):
            newline = text.find("\n", index)
            index = len(text) if newline < 0 else newline
            continue
        if text.startswith("/*", index):
            index = skip_block_comment(text, index)
            continue
        raw = RAW_STRING.match(text, index)
        if raw:
            terminator = '"' + raw.group(1)
            end = text.find(terminator, raw.end())
            index = len(text) if end < 0 else end + len(terminator)
            continue
        if char == '"':
            index = skip_quoted(text, index)
            continue
        if char == "'":
            end = char_literal_end(text, index)
            index = index + 1 if end is None else end
            continue
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return text[start:index + 1]
        index += 1
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


def code_rows(table_dir: Path = CODE_TABLE_DIR) -> list[dict[str, str]]:
    """Read the frozen code-module tables C1..C5.csv."""
    result = []
    for table in sorted(table_dir.glob("C[1-5].csv")):
        with table.open(newline="", encoding="utf-8") as source:
            for row in csv.DictReader(source):
                row["files"] = [part.strip() for part in row["files"].split(";") if part.strip()]
                result.append(row)
    return result


def code_body(files: list[str], blobs: dict[str, bytes | None]) -> str | None:
    """sha256 over concatenated file bytes in sorted path order.

    `blobs` maps a repo-relative path to its bytes (None when missing).
    Returns None when any listed file is missing, so a partial file set
    never compares equal to the frozen whole-module hash.
    """
    parts = []
    for path in sorted(files):
        blob = blobs.get(path)
        if blob is None:
            return None
        parts.append(blob)
    return hashlib.sha256(b"".join(parts)).hexdigest()


# Test-support files that never carry a module row of their own (named in the
# per-domain rating reports as excluded test obligations).
CODE_KNOWN_TEST_SUPPORT = frozenset({
    "src/runner/container_command/qualification.rs",
    "src/runner/workspace_command/permissions_gnu_accept.rs",
    "crates/effigy-tui/src/multiprocess/startup_visibility.rs",
    "src/runner/scripted_doctor.rs",
    "src/runner/gateway_command/fixture.rs",
    "crates/effigy-gateway/src/server/endpoint_faults.rs",
})


def is_code_test_only(path: str) -> bool:
    """Test-only product-tree files carry no module row of their own."""
    base = path.rsplit("/", 1)[-1]
    lowered = base.lower()
    if path in CODE_KNOWN_TEST_SUPPORT:
        return True
    if path.startswith("crates/effigy-test-support/"):
        return True
    return ("/tests/" in path or base == "tests.rs"
            or "test_support" in lowered or "test" in lowered
            or "fixture" in lowered or "fake" in lowered)


def discover_product_files(root: Path = ROOT) -> set[str]:
    """Repo-relative .rs files under src/ and crates/*/src/, minus test-only."""
    found = set()
    for base in [root / "src", *sorted((root / "crates").glob("*/src"))]:
        if not base.is_dir():
            continue
        for path in base.rglob("*.rs"):
            if any(part in {"target", ".git", "vendor"} for part in path.parts):
                continue
            rel = path.relative_to(root).as_posix()
            if not is_code_test_only(rel):
                found.add(rel)
    return found


def code_tree_bytes(files: set[str], root: Path = ROOT) -> dict[str, bytes | None]:
    """Read working-tree bytes for every path in `files`."""
    blobs: dict[str, bytes | None] = {}
    for path in files:
        candidate = root / path
        try:
            blobs[path] = candidate.read_bytes()
        except OSError:
            blobs[path] = None
    return blobs


def code_drift(rated_rows: list[dict[str, str]],
               blobs: dict[str, bytes | None],
               product_files: set[str]) -> dict[str, object]:
    """Pure drift classification: unchanged, changed, gone, new, splits.

    `blobs` maps every row-listed path (plus anything else needed) to its
    working-tree bytes or None. `product_files` is today's product-tree
    file set for new-file detection.
    """
    covered: set[str] = set()
    for row in rated_rows:
        covered.update(row["files"])  # type: ignore[arg-type]
    changed: list[str] = []
    gone: list[tuple[str, list[str]]] = []
    for row in rated_rows:
        missing = [path for path in sorted(row["files"]) if blobs.get(path) is None]  # type: ignore[union-attr]
        if missing:
            gone.append((row["id"], missing))
            continue
        if code_body(row["files"], blobs) != row["body_sha"]:  # type: ignore[arg-type]
            changed.append(row["id"])
    gone_ids = {row_id for row_id, _ in gone}
    new_files = sorted(product_files - covered)
    # A "split: re-rate" group is a gone row whose path still owns new files.
    # A gone single file `a/b/c.rs` watches only its stem directory `a/b/c/`:
    # a split lands its products there (`lifecycle.rs` -> `lifecycle/`), while
    # a new sibling elsewhere in the parent directory is just a new file.
    # Multi-file rows watch their longest common tree.
    splits: list[tuple[str, list[str]]] = []
    for row_id, _ in gone:
        row = next(item for item in rated_rows if item["id"] == row_id)
        paths = sorted(row["files"])  # type: ignore[union-attr]
        if len(paths) == 1:
            only = paths[0]
            stem = only[:-len(".rs")] if only.endswith(".rs") else only
            prefix = stem + "/"
        else:
            import os.path
            prefix = os.path.commonpath(paths) + "/"
        owned = [path for path in new_files if path.startswith(prefix)]
        if owned:
            splits.append((row_id, owned))
    return {"changed": sorted(changed), "gone": sorted(gone), "new": new_files, "splits": sorted(splits)}


def code_report(drift: dict[str, object], total: int) -> str:
    """Render the full human-readable code drift report."""
    changed = drift["changed"]  # type: ignore[index]
    gone = drift["gone"]  # type: ignore[index]
    new_files = drift["new"]  # type: ignore[index]
    splits = drift["splits"]  # type: ignore[index]
    fresh = max(0, total - len(gone) - len(changed))
    stale = len(new_files) + len(changed) + len(gone)
    denominator = total + len(new_files)
    percent = (100 * stale / denominator) if denominator else 0.0
    lines = [f"code quality: {total} rows; {len(new_files)} new; {len(changed)} changed; "
             f"{len(gone)} gone; {fresh} fresh; {percent:.1f}% stale since {CODE_RATED_COMMIT[:9]}"]
    lines.append(f"\nchanged (body hash differs): {len(changed)}")
    lines.extend(f"  {row_id}" for row_id in changed)
    lines.append(f"\ngone (row files missing): {len(gone)}")
    for row_id, missing in gone:
        lines.append(f"  {row_id} (missing: {', '.join(missing)})")
    lines.append(f"\nnew product files (no row): {len(new_files)}")
    lines.extend(f"  {path}" for path in new_files)
    lines.append(f"\nsplit: re-rate groups (new files under a gone row's path): {len(splits)}")
    for row_id, owned in splits:
        lines.append(f"  {row_id}")
        lines.extend(f"    {path}" for path in owned)
    return "\n".join(lines) + "\n"


def code_summary_line(drift: dict[str, object], total: int) -> str:
    changed = drift["changed"]  # type: ignore[index]
    gone = drift["gone"]  # type: ignore[index]
    new_files = drift["new"]  # type: ignore[index]
    stale = len(new_files) + len(changed) + len(gone)
    denominator = total + len(new_files)
    percent = (100 * stale / denominator) if denominator else 0.0
    return (f"code quality: {total} rows, {len(new_files)} new, {len(changed)} changed, "
            f"{len(gone)} gone, {percent:.1f}% stale since {CODE_RATED_COMMIT[:9]}\n")


def run_code_mode(summary: bool, table_dir: Path = CODE_TABLE_DIR, root: Path = ROOT) -> int:
    rated_rows = code_rows(table_dir)
    if not rated_rows:
        print(f"quality-table: no code rating rows found in {table_dir}", file=sys.stderr)
        return 2
    wanted: set[str] = set()
    for row in rated_rows:
        wanted.update(row["files"])  # type: ignore[arg-type]
    blobs = code_tree_bytes(wanted, root)
    drift = code_drift(rated_rows, blobs, discover_product_files(root))
    if summary:
        print(code_summary_line(drift, len(rated_rows)), end="")
    else:
        print(code_report(drift, len(rated_rows)), end="")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--summary", action="store_true", help="print the milestone summary line only")
    parser.add_argument("--code", action="store_true", help="report drift in the code-module tables instead of the test tables")
    parser.add_argument("--durations", nargs="+", metavar="JUNIT", type=Path,
                        help="summarize nextest JUnit reports and fill duration_ms")
    parser.add_argument("--top", type=int, default=50, help="slowest tests to show (default: 50)")
    parser.add_argument("--report-only", action="store_true", help="summarize without changing CSV tables")
    args = parser.parse_args([item for item in sys.argv[1:] if item != "--"])

    if args.code:
        if args.durations:
            print("quality-table: --code does not take --durations", file=sys.stderr)
            return 2
        return run_code_mode(args.summary)

    if args.durations:
        missing = [path for path in args.durations if not path.is_file()]
        if missing:
            print("quality-table: missing JUnit report(s): " + ", ".join(map(str, missing)), file=sys.stderr)
            return 2
        return print_durations(args.durations, max(1, args.top), not args.report_only)

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

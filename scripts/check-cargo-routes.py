#!/usr/bin/env python3
"""Fail when an Effigy route runs plain Cargo without being recorded.

Every route that runs Cargo is either transitioned to Oxide or recorded in
`docs/knowledge/cargo-routes.md`, with its rule number and one reason. This
check is the drift guard: a new or edited selector that runs plain `cargo`
fails here until the record names it, and a record entry whose route no longer
runs plain Cargo fails as stale.

Not counted, by design:

* comments in the catalog files;
* `cargo-*` binaries, `$CARGO` and `${CARGO_*}`;
* a route whose command is `scripts/oxide-selector.sh exec -- <tool>`, because
  the nested `cargo` word belongs to the tool that Oxide already leases;
* a line naming `EFFIGY_OXIDE_ESCAPE=cargo`, the documented whole-selector
  escape, in a catalog route or a route script.

Usage: scripts/check-cargo-routes.py   (exit 1 on any unrecorded or stale route)
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RECORD = os.path.join("docs", "knowledge", "cargo-routes.md")
CATALOG = "effigy.toml"

CARGO_WORD = re.compile(r'(?<![A-Za-z0-9_/$-])cargo(?=[\s"\',]|$)')
TASK_KEY = re.compile(r'^\s*(?:"([^"]+)"|([A-Za-z0-9_][A-Za-z0-9:_.-]*))\s*=')
SECTION = re.compile(r'^\s*\[([^\]]+)\]\s*$')
INCLUDE_ITEM = re.compile(r'"([^"]+\.toml)"')
# Any repository-relative script path with a `scripts` component, including a
# crate-owned one such as `crates/effigy-kernel/tests/scripts/linux-custody.sh`:
# a catalog route that calls a script is a route whatever directory owns it.
SCRIPT_TOKEN = re.compile(
    r'(?<![\w./-])((?:[A-Za-z0-9_.+-]+/)+[A-Za-z0-9_./+-]+\.(?:sh|py|rhai))'
)
# A path in this catalog is only a candidate when it names a script directory;
# the membership check keeps a stray asset path out of the route scan.
SCRIPT_DIRECTORY = re.compile(r'(^|/)scripts/')
# The guard itself names `cargo` in its rules and runs no Cargo.
SKIP_SCRIPTS = ("scripts/check-cargo-routes.py",)


def strip_comment(line):
    """Drop a trailing `#` comment that is not inside a quoted string."""
    quote = None
    for index, char in enumerate(line):
        if quote:
            if char == quote and line[index - 1] != "\\":
                quote = None
        elif char in ('"', "'"):
            quote = char
        elif char == "#":
            return line[:index]
    return line


def catalog_paths():
    """The root catalog plus every file its `include` list names."""
    paths = [CATALOG]
    text = open(os.path.join(ROOT, CATALOG), encoding="utf-8").read()
    lines = text.splitlines()
    collecting = False
    for line in lines:
        code = strip_comment(line)
        if not collecting:
            if re.match(r"^\s*include\s*=\s*\[", code):
                collecting = True
                code = code.split("[", 1)[1]
            else:
                continue
        if "]" in code:
            code = code.split("]", 1)[0]
            collecting = False
        for match in INCLUDE_ITEM.finditer(code):
            paths.append(match.group(1))
    return [path for path in paths if os.path.isfile(os.path.join(ROOT, path))]


def cargo_routes_in_catalog(path):
    """(route, line) for each plain-Cargo command word in one catalog."""
    routes = []
    key = None
    section = ""
    for number, raw in enumerate(open(os.path.join(ROOT, path), encoding="utf-8"), 1):
        section_match = SECTION.match(strip_comment(raw))
        if section_match:
            section = section_match.group(1)
            key = None
            continue
        key_match = TASK_KEY.match(raw)
        if key_match:
            key = key_match.group(1) or key_match.group(2)
            if section not in ("tasks", "test.suites") and section:
                key = "%s.%s" % (section, key)
        code = strip_comment(raw)
        if not CARGO_WORD.search(code):
            continue
        route = key or path
        if "oxide-selector.sh exec --" in code:
            continue
        if "EFFIGY_OXIDE_ESCAPE=cargo" in code:
            continue
        routes.append((route, "%s:%d: %s" % (path, number, code.strip())))
    return routes


def cargo_routes_in_script(path):
    """(path, line) for each plain-Cargo command word in one route script."""
    routes = []
    for number, raw in enumerate(open(os.path.join(ROOT, path), encoding="utf-8"), 1):
        code = strip_comment(raw)
        if "EFFIGY_OXIDE_ESCAPE=cargo" in code:
            continue
        if CARGO_WORD.search(code):
            routes.append((path, "%s:%d: %s" % (path, number, code.strip())))
    return routes


def referenced_scripts():
    """Script files the catalogs call, then the scripts those scripts call."""
    scripts = []
    pending = list(catalog_paths())
    while pending:
        source = pending.pop(0)
        for raw in open(os.path.join(ROOT, source), encoding="utf-8"):
            for match in SCRIPT_TOKEN.finditer(strip_comment(raw)):
                # `./scripts/x.sh` and `scripts/x.sh` name one route.
                candidate = os.path.normpath(match.group(1))
                if candidate in SKIP_SCRIPTS or candidate in scripts:
                    continue
                if not SCRIPT_DIRECTORY.search(candidate):
                    continue
                if not os.path.isfile(os.path.join(ROOT, candidate)):
                    continue
                scripts.append(candidate)
                pending.append(candidate)
    return scripts


def recorded_routes():
    """The first column of every table row in the record, unquoted."""
    recorded = []
    for raw in open(os.path.join(ROOT, RECORD), encoding="utf-8"):
        line = raw.strip()
        if not line.startswith("|") or line.startswith("| Route"):
            continue
        if set(line.replace("|", "").strip()) <= set("-: "):
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        route = cells[0].strip("`")
        if route:
            recorded.append(route)
    return recorded


def main():
    live = {}
    for path in catalog_paths():
        for route, where in cargo_routes_in_catalog(path):
            live.setdefault(route, []).append(where)
    for script in referenced_scripts():
        for route, where in cargo_routes_in_script(script):
            live.setdefault(route, []).append(where)
    recorded = recorded_routes()

    unrecorded = sorted(route for route in live if route not in recorded)
    stale = sorted(route for route in recorded if route not in live)
    for route in unrecorded:
        print("[error] unrecorded plain-Cargo route: %s" % route)
        for where in live[route]:
            print("        %s" % where)
    for route in stale:
        print("[error] recorded route no longer runs plain Cargo: %s" % route)
    if unrecorded or stale:
        print(
            "\nEvery route that runs Cargo is either routed through Oxide or "
            "recorded in %s with its rule number and reason." % RECORD
        )
        return 1
    print(
        "[ok] %d plain-Cargo routes, all recorded in %s"
        % (len(live), RECORD)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

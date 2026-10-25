#!/usr/bin/env python3
"""Fail when a harness `mktemp` ignores the run's temporary directory.

A host run on a configured `[scratch]` volume (effigy#244) sees that volume as
`TMPDIR`, and every harness directory is supposed to land there. On macOS
`mktemp` without a template ignores `TMPDIR` and writes to the internal
disk's `/var/folders/.../T` anyway, and a hardcoded `/tmp/...` template
ignores it everywhere. Both put a test run's scratch back on the internal SSD.

Every `mktemp` in the scanned harness directories must therefore take a
template built from the run's temp directory, as in

    fixture=$(mktemp -d "${TMPDIR:-/tmp}/effigy-fixture.XXXXXX")

A script that runs inside a Linux guest or container keeps the guest's own
`/tmp`, because `TMPDIR` there is not the host's. That call carries an
annotation on the same line or the line above:

    native_dir=$(mktemp -d)  # mktemp-exempt: container guest temp

Usage: scripts/check-mktemp.py   (exit 1 on any bare or `/tmp`-template call)
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCANNED = (
    "config",
    "scripts",
    "tests/scripts",
)
CRATE_SCRIPTS = re.compile(r"^crates/[^/]+/tests/scripts$")
# The guard names the offending forms in its own rules and runs none of them.
SKIP = ("scripts/check-mktemp.py",)
EXEMPT = re.compile(r"mktemp-exempt:\s*(\S.*)")
MKTEMP = re.compile(r"(?<![\w./-])mktemp(?![\w.-])")
# A `mktemp` only counts as a call at a command position, not inside a string.
COMMAND_POSITION = re.compile(
    r"(\$\(|`|[\s;&|({]$|^|\b(?:then|do|else|command|exec)\s+)$"
)
WORD = re.compile(
    r'"[^"]*"|\'[^\']*\'|\$\{[^}]*\}[^\s);|&]*|\$[A-Za-z_][A-Za-z0-9_]*[^\s);|&]*|[^\s);|&]+'
)
# A template under one of these ignores TMPDIR on any platform.
HARDCODED = ("/tmp/", "/var/folders/", "/private/var/folders/")
HARDCODED_EXACT = ("/tmp", "/var/folders", "/private/var/folders")


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


def scanned_files():
    paths = []
    for directory in SCANNED:
        paths.extend(walk(directory))
    crates = os.path.join(ROOT, "crates")
    for name in sorted(os.listdir(crates)):
        relative = os.path.join("crates", name, "tests/scripts")
        if CRATE_SCRIPTS.match(relative) and os.path.isdir(os.path.join(ROOT, relative)):
            paths.extend(walk(relative))
    return paths


def walk(relative):
    paths = []
    for base, _, names in os.walk(os.path.join(ROOT, relative)):
        for name in sorted(names):
            path = os.path.join(base, name)
            relative_path = os.path.relpath(path, ROOT)
            if relative_path in SKIP:
                continue
            if os.path.isfile(path):
                paths.append(relative_path)
    return paths


def template_of(code, start):
    """The first non-option argument after `mktemp`, or None."""
    for word in WORD.findall(code[start:]):
        if word.startswith("-"):
            continue
        return word
    return None


def exempt(lines, index):
    """Whether this line, or the line above it, claims a guest-side exemption."""
    for candidate in (lines[index], lines[index - 1] if index else ""):
        match = EXEMPT.search(candidate)
        if match:
            return True
    return False


def complaints(path):
    try:
        text = open(os.path.join(ROOT, path), encoding="utf-8").read()
    except (UnicodeDecodeError, OSError):
        return []
    lines = text.splitlines()
    problems = []
    for index, line in enumerate(lines):
        code = strip_comment(line)
        for match in MKTEMP.finditer(code):
            if not COMMAND_POSITION.search(code[: match.start()]):
                continue
            if exempt(lines, index):
                continue
            template = template_of(code, match.end())
            where = f"{path}:{index + 1}"
            if template is None:
                problems.append(
                    f"{where}: `mktemp` has no template, so it ignores TMPDIR and "
                    "writes to the internal disk; pass "
                    '"${TMPDIR:-/tmp}/<name>.XXXXXX"'
                )
            elif template.strip("\"'") in HARDCODED_EXACT or template.startswith(HARDCODED):
                problems.append(
                    f"{where}: `mktemp` template `{template}` ignores TMPDIR; use "
                    '"${TMPDIR:-/tmp}/<name>.XXXXXX"'
                )
            elif "XXXXXX" not in template:
                problems.append(
                    f"{where}: `mktemp` template `{template}` has no `XXXXXX`; use "
                    '"${TMPDIR:-/tmp}/<name>.XXXXXX"'
                )
    return problems


def main():
    problems = []
    for path in scanned_files():
        problems.extend(complaints(path))
    for problem in problems:
        print(problem)
    if problems:
        print(f"check-mktemp: {len(problems)} harness `mktemp` call(s) ignore TMPDIR")
        return 1
    print("check-mktemp: every harness `mktemp` honours TMPDIR")
    return 0


if __name__ == "__main__":
    sys.exit(main())

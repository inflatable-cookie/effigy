#!/usr/bin/env python3
"""Fail when Debug formatting grows in human-facing error construction.

Every error a person can see names what failed, the evidence and the next
action, in words (see `docs/knowledge/architecture/product-guardrails.md`,
"Human-Facing Feedback"). Rust `Debug` renderings, variant names and nested
`Unknown(StoreUnreadable(…))` chains never reach a person unrendered, so this
ratchet counts `{…:?}` / `{…:#?}` format specifiers inside the string macros of
files that define or construct human-facing errors, and compares the count with
a committed baseline.

A rise above the baseline fails: render the value through `Display` (or through
`effigy_ui::human::HumanError`) instead. A fall fails too, so the change that
lowers the count also lowers `config/human-error-debug-ratchet.json`:

    python3 scripts/check-human-error-debug.py --write-baseline

Human-facing error construction is a file that either implements `Display` for
an error-shaped type, or constructs an error in the runner, built-in, managed,
doctor, kernel or human-error vocabulary. Tests and test support are not
counted: a `Debug` string in a test is how a test names a value, not how a
person reads a failure.

Usage: scripts/check-human-error-debug.py [--write-baseline]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join("config", "human-error-debug-ratchet.json")
SCHEMA = 1

# String-producing macros: a Debug specifier inside one of these becomes text.
STRING_MACROS = (
    "format",
    "format_args",
    "write",
    "writeln",
    "panic",
    "print",
    "println",
    "eprint",
    "eprintln",
)
MACRO_START = re.compile(
    r"(?<![A-Za-z0-9_])(" + "|".join(STRING_MACROS) + r")!\s*[\(\[\{]"
)
# A format specifier that renders Debug: `{name:?}`, `{:#?}`, `{0:>8?}`.
DEBUG_SPEC = re.compile(r"\{[^{}]*\?[^{}]*\}")
# A file that implements Display for an error-shaped type.
DISPLAY_IMPL = re.compile(
    r"impl(?:\s*<[^>]*>)?\s+(?:(?:std|core|fmt)::)*(?:fmt::)?Display\s+for\s+"
)
# A file that constructs human-facing errors.
ERROR_VOCABULARY = re.compile(
    r"\b(?:RunnerError|BuiltinError|ManagedError|DoctorError|ContainerExecError|"
    r"UnknownReason|StoreError|RunError|GateError|FsError|HumanError)::"
    r"|\btask_invocation(?:_failed_\w+)?\("
)
# Test code is excluded by location: a Debug string in a test names a value.
TEST_PATH = re.compile(r"(^|/)(tests?|test_support)(/|\.rs$)")
# Fault-injection tooling is test-only and renders verdicts, not operator text.
TEST_TOOLING = re.compile(
    r"(^|/)(faults|fault_injection|strict_fake|fixture)(/|\.rs$)"
)
SOURCE_ROOTS = ("src", "crates")


def rust_sources():
    """Production Rust sources, repository relative, sorted."""
    found = []
    for root in SOURCE_ROOTS:
        for directory, subdirectories, files in os.walk(os.path.join(ROOT, root)):
            subdirectories[:] = sorted(
                name for name in subdirectories if name not in ("target", "fixtures")
            )
            for name in sorted(files):
                if not name.endswith(".rs"):
                    continue
                path = os.path.join(directory, name)
                relative = os.path.relpath(path, ROOT).replace(os.sep, "/")
                if TEST_PATH.search(relative) or TEST_TOOLING.search(relative):
                    continue
                found.append(relative)
    return sorted(set(found))


def read(relative):
    with open(os.path.join(ROOT, relative), encoding="utf-8") as handle:
        return handle.read()


def is_human_facing(source):
    if DISPLAY_IMPL.search(source):
        return True
    return ERROR_VOCABULARY.search(source) is not None


def skip_string(source, index):
    """Index just past the string or char literal starting at `index`."""
    raw = re.match(r'r(#+)?"', source[index:])
    if raw:
        hashes = raw.group(1) or ""
        closing = '"' + hashes
        end = source.find(closing, index + raw.end())
        return len(source) if end < 0 else end + len(closing)
    if source.startswith('r"', index):
        end = source.find('"', index + 2)
        return len(source) if end < 0 else end + 1
    quote = source[index]
    index += 1
    while index < len(source):
        char = source[index]
        if char == "\\":
            index += 2
            continue
        if char == quote:
            return index + 1
        index += 1
    return index


def macro_body(source, start):
    """The balanced span of a macro invocation, string literals respected."""
    depth = 0
    index = start
    while index < len(source):
        char = source[index]
        if char in ('"', "'"):
            index = skip_string(source, index)
            continue
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
            if depth == 0:
                return source[start:index]
        index += 1
    return source[start:]


def count_debug_specs(source):
    """Debug format specifiers inside this file's string macros."""
    total = 0
    for match in MACRO_START.finditer(source):
        total += len(DEBUG_SPEC.findall(macro_body(source, match.end() - 1)))
    return total


def inventory():
    counts = {}
    for relative in rust_sources():
        source = read(relative)
        if not is_human_facing(source):
            continue
        count = count_debug_specs(source)
        if count:
            counts[relative] = count
    return counts


def load_baseline():
    import json

    with open(os.path.join(ROOT, BASELINE), encoding="utf-8") as handle:
        baseline = json.load(handle)
    if baseline.get("schema") != SCHEMA:
        raise SystemExit(
            f"{BASELINE}: unsupported schema {baseline.get('schema')}; expected {SCHEMA}"
        )
    return {name: int(count) for name, count in baseline.get("files", {}).items()}


def write_baseline(counts):
    import json

    document = {
        "schema": SCHEMA,
        "description": (
            "Debug format specifiers in human-facing error construction. "
            "Lower with scripts/check-human-error-debug.py --write-baseline; "
            "never raise it."
        ),
        "files": dict(sorted(counts.items())),
    }
    path = os.path.join(ROOT, BASELINE)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(document, indent=2) + "\n")
    total = sum(counts.values())
    print(f"wrote {BASELINE}: {total} Debug specifiers in {len(counts)} file(s)")


def compare(current, baseline):
    findings = []
    for name, count in sorted(current.items()):
        old = baseline.get(name, 0)
        if count > old:
            findings.append(
                f"{name}: Debug formatting in human output rose {old} -> {count}; "
                "render the value through Display or HumanError instead"
            )
        elif count < old:
            findings.append(
                f"{name}: Debug formatting in human output fell {old} -> {count}; "
                "lower the baseline in this change with --write-baseline"
            )
    for name, count in sorted(baseline.items()):
        if name not in current and count:
            findings.append(
                f"{name}: no longer counts {count} Debug specifier(s); "
                "lower the baseline in this change with --write-baseline"
            )
    return findings


def main(argv):
    counts = inventory()
    if "--write-baseline" in argv:
        write_baseline(counts)
        return 0
    findings = compare(counts, load_baseline())
    if findings:
        print("human-error Debug ratchet failed:", file=sys.stderr)
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        return 1
    total = sum(counts.values())
    print(
        f"human-error Debug ratchet: {total} Debug specifier(s) in "
        f"{len(counts)} file(s) match the baseline"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

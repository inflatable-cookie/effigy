#!/usr/bin/env python3
"""Whole-suite test census helper (effigy#555).

Reconciles cargo-nextest's collected test IDs against a static source scan so
that every collected case carries source attribution (file, line, function,
module path), every declared case is explained (collected, ignored,
feature-gated, platform-gated, target gated by required-features), and every
discrepancy is named instead of silently dropped. Standard library only.

The census route is, in order:

  1. cargo metadata        workspace packages, targets, features (no build)
  2. cargo nextest list    collected IDs for the default workspace run
                           (`oxide nextest run --workspace`), including the
                           ignored family; builds, never executes
  3. static source scan    declared test fns, module paths, cfg gate stacks
  4. reconciliation        collected vs declared, with explicit classes
  5. doctest fence census  static doc-comment fences per lib target
  6. non-Rust route census python/rhai/shell validation routes in the catalog

Usage:
  python3 scripts/test-inventory.py census [--out DIR]
  python3 scripts/test-inventory.py self-test

`census` runs Cargo through scripts/oxide-selector.sh (the selector route) and
never runs a test body; `--nextest-list FILE` reuses an already collected list
JSON instead of building. All artifacts land under --out. `self-test` proves
the reconciliation logic on embedded fixtures: duplicate IDs, missing
attribution, and the conditional distinctions (ignored vs feature-gated vs
platform-gated vs target-gated), with no Cargo invocation.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SELECTOR = os.path.join("scripts", "oxide-selector.sh")
HOST_TARGET_OS = "macos"
HOST_TARGET_ARCH = "aarch64"

# Attributes that declare a test case. Plain `#[test]` and attribute-macro
# test harnesses (`#[tokio::test]`). The repo uses no parameterization crates,
# so no name-generating family is expected; the reconciler reports any.
TEST_ATTRS = {"test": "test", "tokio::test": "tokio-test"}
TARGET_GATES = {"target_os", "target_arch", "target_family", "target_env",
                "target_endian", "target_pointer_width", "target_vendor"}
TRUE_PREDS = {"test", "unix", "debug_assertions"}
FALSE_PREDS = {"windows", "proc_macro", "doctest"}


class Unknown(Exception):
    """A cfg predicate the evaluator cannot resolve."""


# --------------------------------------------------------------------------
# Rust source masking: replace comments and string literals with spaces so
# brace matching and item scanning operate on structure only. Comments and
# string literals are preserved in parallel lists for attribute and doctest
# parsing.
# --------------------------------------------------------------------------

def mask_rust(text):
    """Return (masked, strings, comments).

    `masked` has the same length as `text` with comment and string interiors
    replaced by spaces. `strings` maps an interior's start offset to its text.
    `comments` is a list of (start, end, text) including the markers.
    """
    out = list(text)
    strings = {}
    comments = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == "/" and i + 1 < n:
            if text[i + 1] == "/":
                start = i
                while i < n and text[i] != "\n":
                    out[i] = " "
                    i += 1
                comments.append((start, i, text[start:i]))
                continue
            if text[i + 1] == "*":
                start = i
                depth = 1
                i += 2
                while i < n and depth:
                    if text.startswith("/*", i):
                        depth += 1
                        i += 2
                    elif text.startswith("*/", i):
                        depth -= 1
                        i += 2
                    else:
                        i += 1
                comments.append((start, i, text[start:i]))
                continue
        if c == '"' or (c == "b" and text[i + 1:i + 2] == '"'):
            is_byte = c == "b"
            start = i
            if is_byte:
                out[i] = " "
                i += 1
            i += 1
            while i < n:
                if text[i] == "\\":
                    out[i] = " "
                    i += 2
                    continue
                if text[i] == '"':
                    break
                out[i] = " "
                i += 1
            i += 1
            strings[start] = text[start:i]
            continue
        if c == "r" or (c == "b" and text[i + 1:i + 2] == "r"):
            m = re.match(r'b?r(#*)"', text[i:i + 64])
            if m:
                hashes = m.group(1)
                start = i
                i += len(m.group(0))
                terminator = '"' + hashes
                end = text.find(terminator, i)
                if end == -1:
                    end = n
                else:
                    end += len(terminator)
                for j in range(start, end):
                    out[j] = " "
                strings[start] = text[start:end]
                i = end
                continue
        if c == "'":
            # char literal vs lifetime: a char literal is `'x'`, `'\n'` or
            # `'\u{...}'`; a lifetime is a quote followed by an identifier
            # start without a closing quote.
            m = re.match(r"'(\\.|\\u\{[0-9a-fA-F_]{1,6}\}|[^\\'])'", text[i:i + 16])
            if m:
                for j in range(i, i + m.end()):
                    out[j] = " "
                i += m.end()
                continue
            # lifetime: leave as code
            i += 1
            continue
        i += 1
    return "".join(out), strings, comments


# --------------------------------------------------------------------------
# cfg evaluation against the default workspace build context
# --------------------------------------------------------------------------

def tokenize_cfg(s):
    """Split a cfg expression body into ident/string/punct tokens."""
    out = []
    for m in re.finditer(r'"([^"]*)"|[A-Za-z_][A-Za-z0-9_]*|[(),=]', s):
        if m.group(0).startswith('"'):
            out.append(m.group(1))  # content without quotes
        else:
            out.append(m.group(0))
    return out


class _CfgParser:
    """Recursive-descent parser/evaluator for cfg() expression bodies.

    Rust cfg grammar: comma-separated predicates at every level (top level,
    inside parentheses, all(), any()); all() = AND, any() = OR, not() = NOT.
    """

    def __init__(self, tokens, features):
        self.toks = tokens
        self.i = 0
        self.features = features

    def peek(self):
        return self.toks[self.i] if self.i < len(self.toks) else None

    def next(self):
        t = self.peek()
        if t is not None:
            self.i += 1
        return t

    def parse(self):
        verdict, why = self.parse_list()
        if self.peek() is not None:
            raise Unknown(f"trailing token {self.peek()!r} in cfg expression")
        return verdict, why

    def parse_list(self):
        verdict, why = self.parse_atom()
        while self.peek() == ",":
            self.next()
            v2, w2 = self.parse_atom()
            if v2 and not verdict:
                verdict, why = True, ""
            elif not verdict and not v2 and not why:
                why = w2
        return verdict, why

    def parse_atom(self):
        tok = self.next()
        if tok is None:
            raise Unknown("unexpected end of cfg expression")
        if tok == "(":
            inner, why = self.parse_list()
            closing = self.next()
            if closing != ")":
                raise Unknown("unbalanced cfg parentheses")
            return inner, why
        if tok == "any":
            inner, why = self.paren_list(want_all=False)
            return inner, why
        if tok == "all":
            inner, why = self.paren_list(want_all=True)
            return inner, why
        if tok == "not":
            opening = self.next()
            if opening != "(":
                raise Unknown("not() without parentheses")
            inner, why = self.parse_list()
            closing = self.next()
            if closing != ")":
                raise Unknown("unbalanced cfg parentheses")
            return (not inner,
                    "negated cfg holds in the default build" if inner else "")
        nxt = self.peek()
        if nxt == "=":
            self.next()
            value = self.next()
            if value is None:
                raise Unknown(f"cfg {tok} missing value")
            return self.eval_pred(tok, value)
        return self.eval_pred(tok, None)

    def paren_list(self, want_all):
        opening = self.next()
        if opening != "(":
            raise Unknown(f"{'all' if want_all else 'any'}() without parentheses")
        verdicts = []
        while True:
            verdicts.append(self.parse_atom())
            tok = self.next()
            if tok == ")":
                break
            if tok != ",":
                raise Unknown("expected , or ) in cfg list")
        if want_all:
            for v, w in verdicts:
                if not v:
                    return False, w
            return True, ""
        for v, _w in verdicts:
            if v:
                return True, ""
        return False, verdicts[0][1] if verdicts else ""

    def eval_pred(self, pred, value):
        if pred == "test":
            return True, ""
        if pred == "feature":
            if value in self.features:
                return True, ""
            return False, (f'feature "{value}" is not enabled in the default '
                           "build")
        if pred in TRUE_PREDS:
            return True, ""
        if pred in FALSE_PREDS:
            return False, f"{pred} is false on this host"
        if pred == "target_os":
            if value == HOST_TARGET_OS:
                return True, ""
            return False, f'target_os "{value}" does not match this host'
        if pred == "target_arch":
            if value == HOST_TARGET_ARCH:
                return True, ""
            return False, f'target_arch "{value}" does not match this host'
        if pred == "target_pointer_width":
            if value == "64":
                return True, ""
            return False, f"target_pointer_width {value} does not match this host"
        if pred == "target_endian":
            if value == "little":
                return True, ""
            return False, f"target_endian {value} does not match this host"
        if pred == "target_family":
            if value == "unix":
                return True, ""
            return False, f'target_family "{value}" does not match this host'
        raise Unknown(f"unrecognized cfg predicate {pred}"
                      + (f'="{value}"' if value is not None else ""))


def eval_cfg(expr, features, reason):
    """Evaluate a cfg() expression body against the default build context.

    Returns (True, \'\') or (False, reason naming the excluding gate); raises
    Unknown for predicates outside the context so callers classify honestly.
    """
    return _CfgParser(tokenize_cfg(expr), features).parse()


# --------------------------------------------------------------------------
# Static scan of declared test functions
# --------------------------------------------------------------------------

def parse_attrs(masked, strings, pos, stop=None, max_count=None):
    """Parse a contiguous attribute run starting at `pos` (at `#`).

    Returns (attrs, newpos); positions are absolute into `masked`. Stops at
    the first non-attribute token, at `stop` (the region end), or after
    `max_count` attributes.
    """
    n = len(masked) if stop is None else stop
    attrs = []
    while True:
        if max_count is not None and len(attrs) >= max_count:
            return attrs, pos
        i = skip_ws(masked, pos)
        if i >= n or masked[i] != "#":
            return attrs, i
        j = i + 1
        while j < n and masked[j] in " !":
            j += 1
        if j >= n or masked[j] != "[":
            # a `#` that does not open an attribute (e.g. the `r#ref` raw
            # identifier): consume it so the caller always makes progress
            return attrs, i + 1
        depth = 0
        k = j
        while k < n:
            if masked[k] == "[":
                depth += 1
            elif masked[k] == "]":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        inner = restore_literals(masked, j + 1, k, strings)
        attrs.append(parse_one_attr(inner))
        pos = k + 1


def restore_literals(masked, region_start, region_end, strings):
    """Rebuild masked-region text with string literals spliced back in."""
    pieces = []
    last = region_start
    for start, text in sorted(strings.items()):
        if start < region_start or start >= region_end:
            continue
        pieces.append(masked[last:start])
        pieces.append(text)
        last = start + len(text)
    pieces.append(masked[last:region_end])
    return "".join(pieces)


def parse_one_attr(inner):
    m = re.match(r"\s*([A-Za-z_][A-Za-z0-9_:]*)(.*)$", inner, re.S)
    kind = m.group(1) if m else ""
    rest = m.group(2) if m else inner
    attr = {"kind": kind, "body": None, "value": None}
    rest = rest.lstrip()
    if rest.startswith("="):
        vm = re.match(r'=\s*"([^"]*)"', rest)
        if vm:
            attr["value"] = vm.group(1)
    elif rest.startswith("("):
        depth = 0
        end = 0
        for idx, ch in enumerate(rest):
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    end = idx
                    break
        attr["body"] = rest[1:end]
    return attr


def skip_ws(s, i):
    n = len(s)
    while i < n and s[i] in " \t\r\n":
        i += 1
    return i


def scan_file_for_tests(path, package, binary, target_kind, module_prefix,
                        inherited_gates, features, decls, children_dir=None,
                        visited=None, macros=None):
    """Scan one source file for test declarations; recurse into modules.

    `inherited_gates` is a list of cfg strings from enclosing modules.
    Appends declaration dicts to `decls` and macro-invocation candidates to
    `macros` (when provided). The file is masked exactly once; inline module
    regions reuse that mask (absolute positions throughout).
    `children_dir` is where this file's `mod X;` children live (2018 edition:
    a `dir/name.rs` file's children live in `dir/name/`). `visited` guards
    against resolution loops.
    """
    if visited is None:
        visited = set()
    path = os.path.abspath(path)
    key = (path, module_prefix, tuple(sorted(inherited_gates)))
    if key in visited:
        return
    visited.add(key)
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            text = f.read()
    except OSError:
        return
    masked, strings, _comments = mask_rust(text)
    if children_dir is None:
        children_dir = module_children_dir(path)
    if macros is None:
        macros = []
    scan_region(text, masked, strings, 0, len(masked), path,
                os.path.dirname(path), children_dir, package, binary,
                target_kind, module_prefix, inherited_gates, features, decls,
                visited, macros)


def module_children_dir(path):
    """Where this file's `mod X;` children live (2018 edition layout).

    A file module `dir/name.rs` keeps children in `dir/name/`; `lib.rs` and
    `main.rs` roots keep children beside themselves. Falls back to the file's
    own directory when no sibling module directory exists.
    """
    d = os.path.dirname(path)
    stem = os.path.basename(path)[:-3] if path.endswith(".rs") else path
    if stem in ("lib", "main"):
        return d
    sibling = os.path.join(d, stem)
    if os.path.isdir(sibling):
        return sibling
    return d


def scan_region(text, masked, strings, start, end, region_path, file_dir,
                base_dir, package, binary, target_kind, module_prefix,
                inherited_gates, features, decls, visited, macros):
    """Walk one brace-region of a file for test declarations.

    Positions are absolute into `masked`/`text` (same offsets). Inline module
    regions recurse on the same mask, so each file is masked exactly once.
    `file_dir` is the containing file's directory (#[path] values resolve
    there); `base_dir` is where this region's `mod X;` children resolve.
    """
    n = end
    pos = start
    pending = []
    # inner attributes at the region start (`#![...]`) gate the whole region
    while True:
        p = skip_ws(masked, pos)
        if p + 2 < n and masked[p] == "#" and masked[p + 1] == "!" \
                and masked[p + 2] == "[":
            attrs, pos2 = parse_attrs(masked, strings, p, stop=n, max_count=1)
            if pos2 <= p:
                break
            for a in attrs:
                if a["kind"] == "cfg" and a["body"]:
                    inherited_gates = inherited_gates + [a["body"].strip()]
            pos = pos2
        else:
            break
    while True:
        pos = skip_ws(masked, pos)
        if pos >= n:
            break
        c = masked[pos]
        if c == "#":
            attrs, pos = parse_attrs(masked, strings, pos, stop=n)
            pending.extend(attrs)
            continue
        if c == "{":
            if pending:
                # `#[cfg(...)] { ... }`: a block expression consumes the
                # attribute run; without this, statement-level cfg blocks
                # inside fn bodies leak gates onto later items
                pending = []
            pos += 1
            continue
        if c == "}":
            pos += 1
            continue
        if c == ";":
            # statement terminator: any pending attribute run is consumed
            pending = []
            pos += 1
            continue
        m = re.match(r"[A-Za-z_][A-Za-z0-9_]*", masked[pos:n])
        if not m:
            pos += 1
            continue
        word = m.group(0)
        pos += len(word)
        if word == "macro_rules":
            pos = skip_macro_definition(masked, pos, n)
            pending = []
            continue
        if word == "include":
            j = skip_ws(masked, pos)
            if j < n and masked[j] == "!":
                inc_file = parse_include_target(masked, strings, j + 1, n)
                if inc_file is not None and os.path.exists(inc_file):
                    scan_file_for_tests(inc_file, package, binary, target_kind,
                                        module_prefix, inherited_gates,
                                        features, decls,
                                        children_dir=os.path.dirname(inc_file),
                                        visited=visited, macros=macros)
                    pos = skip_delimited(masked, skip_ws(masked, j + 1), n)
                    pending = []
                    continue
        j2 = skip_ws(masked, pos)
        if pos < n and j2 < n and masked[j2] == "!" and word != "mod":
            # macro invocation: `name!(first_ident, ...)` may declare a test;
            # record the candidate name so the reconciler can attribute it
            pos = record_macro_invocation(masked, strings, pos, n, word,
                                          module_prefix, region_path, macros)
            pending = []
            continue
        if word == "mod":
            j = skip_ws(masked, pos)
            nm = re.match(r"[A-Za-z_][A-Za-z0-9_]*", masked[j:n])
            if not nm:
                pending = []
                continue
            name = nm.group(0)
            j += len(name)
            j2 = skip_ws(masked, j)
            gates, path_attr, _cfg_attrs = summarize_mod_attrs(pending)
            pending = []
            all_gates = inherited_gates + gates
            if j2 < n and masked[j2] == ";":
                pos = j2 + 1
                mod_file, mod_children = resolve_mod_file(file_dir, base_dir,
                                                          name, path_attr)
                if mod_file is not None:
                    scan_file_for_tests(mod_file, package, binary, target_kind,
                                        module_prefix + name + "::",
                                        all_gates, features, decls,
                                        children_dir=mod_children,
                                        visited=visited, macros=macros)
            elif j2 < n and masked[j2] == "{":
                close = matching_brace(masked, j2, n)
                scan_region(text, masked, strings, j2 + 1, close,
                            region_path, file_dir, base_dir, package, binary,
                            target_kind, module_prefix + name + "::",
                            all_gates, features, decls, visited, macros)
                pos = close + 1
            continue
        if word == "fn":
            j = skip_ws(masked, pos)
            nm = re.match(r"[A-Za-z_][A-Za-z0-9_]*", masked[j:n])
            if not nm:
                pending = []
                continue
            name = nm.group(0)
            line = text.count("\n", 0, j) + 1
            classify_fn(pending, inherited_gates, features, package, binary,
                        target_kind, module_prefix, name,
                        os.path.relpath(region_path, ROOT), line, decls)
            pending = []
            continue
        if word in ("use", "struct", "enum", "union", "type", "trait",
                    "impl", "const", "static", "macro"):
            # an item that consumes the pending attribute run
            pending = []
            continue
        if word in ("pub", "async", "unsafe", "extern", "crate", "self",
                    "super"):
            # a qualifier: pending attributes belong to the item that follows
            continue
    return


def resolve_mod_file(file_dir, base_dir, name, path_attr):
    """Resolve `mod name;` (optionally #[path = "..."]) to its file.

    Returns (file, children_dir). A #[path] value is relative to the
    directory of the file containing the declaration (rustc's rule);
    plain children follow 2018 edition layout: `dir/name.rs` keeps children
    in `dir/name/`, `dir/name/mod.rs` keeps them beside itself, with the
    sibling fallback for edition-2015 layouts.
    """
    if path_attr is not None:
        mod_file = os.path.normpath(os.path.join(file_dir, path_attr))
        if os.path.exists(mod_file):
            stem = mod_file[:-3] if mod_file.endswith(".rs") else mod_file
            children = (stem if os.path.isdir(stem)
                        else os.path.dirname(mod_file))
            return mod_file, children
        alt = os.path.join(mod_file, "mod.rs")
        if os.path.exists(alt):
            return alt, mod_file
        return None, None
    if os.path.isdir(os.path.join(base_dir, name)):
        children = os.path.join(base_dir, name)
        for cand in (os.path.join(children, name + ".rs"),
                     os.path.join(children, "mod.rs"),
                     os.path.join(base_dir, name + ".rs")):
            if os.path.exists(cand):
                return cand, children
    for cand in (os.path.join(base_dir, name + ".rs"),
                 os.path.join(base_dir, name, "mod.rs")):
        if os.path.exists(cand):
            return cand, (os.path.join(base_dir, name)
                          if cand.endswith(name + ".rs") else os.path.join(base_dir, name))
    return None, None


def skip_delimited(masked, pos, n):
    """Skip one balanced (), [] or {} group; return the position after it."""
    if pos >= n or masked[pos] not in "([{":
        return pos
    opener = masked[pos]
    closer = {"(": ")", "[": "]", "{": "}"}[opener]
    depth = 0
    while pos < n:
        if masked[pos] == opener:
            depth += 1
        elif masked[pos] == closer:
            depth -= 1
            if depth == 0:
                return pos + 1
        pos += 1
    return pos


def parse_include_target(masked, strings, pos, n):
    """Resolve `include!(concat!(env!("CARGO_MANIFEST_DIR"), "rel"))`.

    Returns the absolute file path, or None when the shape is not that
    repo-standard splice (other include! forms are reported, not guessed).
    """
    j = skip_ws(masked, pos)
    if j >= n or masked[j] != "(":
        return None
    k = matching_group(masked, j, n)
    if k >= n:
        return None
    inner = restore_literals(masked, j + 1, k, strings)
    parts = re.findall(r'"([^"]*)"', inner)
    if "CARGO_MANIFEST_DIR" not in inner or len(parts) != 2:
        return None
    rel = parts[1]
    if rel.startswith("/"):
        return os.path.normpath(os.path.join(ROOT, rel.lstrip("/")))
    return None


def record_macro_invocation(masked, strings, pos, n, word, module_prefix,
                            region_path, macros):
    """Record `word!(first_ident, ...)` as a generated-name candidate.

    Returns the position after the invocation's delimiter group.
    """
    j = skip_ws(masked, pos)
    if j >= n or masked[j] != "!":
        return pos
    j = skip_ws(masked, j + 1)
    if j >= n or masked[j] not in "([{":
        return j
    k = matching_group(masked, j, n)
    if k >= n:
        return j
    inner = restore_literals(masked, j + 1, k, strings)
    line = text_count_lines(masked, j)
    seen = set()
    for ident in re.findall(r"[A-Za-z_][A-Za-z0-9_]*", inner):
        if ident in seen:
            continue
        seen.add(ident)
        macros.append({
            "macro": word,
            "candidate": module_prefix + ident,
            "file": os.path.relpath(region_path, ROOT),
            "line": line,
        })
    return k + 1


def text_count_lines(s, idx):
    return s.count("\n", 0, idx) + 1


def matching_group(masked, pos, n):
    """Index of the delimiter matching the opener at pos; n when unbalanced."""
    if pos >= n:
        return n
    opener = masked[pos]
    closer = {"(": ")", "[": "]", "{": "}"}.get(opener)
    if closer is None:
        return n
    depth = 0
    j = pos
    while j < n:
        if masked[j] == opener:
            depth += 1
        elif masked[j] == closer:
            depth -= 1
            if depth == 0:
                return j
        j += 1
    return n


def matching_brace(masked, open_pos, n):
    """Index of the `}` matching the `{` at open_pos (masked text only)."""
    depth = 0
    j = open_pos
    while j < n:
        if masked[j] == "{":
            depth += 1
        elif masked[j] == "}":
            depth -= 1
            if depth == 0:
                return j
        j += 1
    return n


def skip_macro_definition(masked, pos, n):
    """Skip `! name { ... }` (or (...)) after `macro_rules`; return end pos."""
    j = skip_ws(masked, pos)
    if j < n and masked[j] == "!":
        j = skip_ws(masked, j + 1)
        m = re.match(r"[A-Za-z_][A-Za-z0-9_]*", masked[j:n])
        if m:
            j += len(m.group(0))
        j = skip_ws(masked, j)
        if j < n and masked[j] in "{(":
            opener = masked[j]
            closer = "}" if opener == "{" else ")"
            depth = 0
            while j < n:
                if masked[j] == closer:
                    depth -= 1
                    if depth == 0:
                        return j + 1
                elif masked[j] == opener:
                    depth += 1
                j += 1
    return pos


def summarize_mod_attrs(attrs):
    """Return (gates, path_value, cfg_added) for a module's attribute run."""
    gates = []
    path_value = None
    cfg_added = []
    for a in attrs:
        if a["kind"] == "cfg" and a["body"]:
            gates.append(a["body"].strip())
        elif a["kind"] == "path" and a["value"]:
            path_value = a["value"]
        elif a["kind"] == "cfg_attr" and a["body"]:
            cond, added = split_cfg_attr(a["body"])
            verdict, _why = eval_cfg(cond, set(), "")
            if verdict is True:
                for body in added:
                    kind = body.split("(")[0].strip()
                    if kind == "cfg" or kind == "path":
                        cfg_added.append(parse_one_attr(body))
    return gates, path_value, cfg_added


def split_cfg_attr(body):
    """Split `cond, attr, attr` at top-level commas. Returns (cond, [attrs])."""
    depth = 0
    parts = []
    last = 0
    for i, ch in enumerate(body):
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        elif ch == "," and depth == 0:
            parts.append(body[last:i])
            last = i + 1
    parts.append(body[last:])
    return parts[0].strip(), [p.strip() for p in parts[1:] if p.strip()]


def classify_fn(attrs, inherited_gates, features, package, binary, target_kind,
                module_prefix, name, file, line, decls):
    """Record a test declaration if the attribute run declares a test."""
    test_kind = None
    ignore = False
    ignore_reason = ""
    should_panic = False
    fn_gates = []
    expanded = []
    for a in attrs:
        if a["kind"] in TEST_ATTRS:
            test_kind = TEST_ATTRS[a["kind"]]
        elif a["kind"] == "ignore":
            ignore = True
            ignore_reason = a["body"] or a["value"] or ""
        elif a["kind"] == "should_panic":
            should_panic = True
        elif a["kind"] == "cfg" and a["body"]:
            fn_gates.append(a["body"].strip())
        elif a["kind"] == "cfg_attr" and a["body"]:
            cond, added = split_cfg_attr(a["body"])
            try:
                verdict, _ = eval_cfg(cond, features, "")
            except Unknown:
                verdict = True  # assume the expansion happens; reconcile names it
            if verdict:
                expanded.extend(added)
    for body in expanded:
        kind = body.split("(")[0].strip()
        if kind == "test":
            test_kind = "test"
        elif kind == "ignore":
            ignore = True
        elif kind == "should_panic":
            should_panic = True
        elif kind == "cfg":
            inner = body[len("cfg"):].strip().lstrip("(").rstrip(")").strip()
            fn_gates.append(inner)
    if test_kind is None:
        return
    # evaluate the full gate stack: inherited module gates then fn gates
    gate_results = []
    for gate in inherited_gates + fn_gates:
        try:
            verdict, why = eval_cfg(gate, features, "")
        except Unknown as e:
            gate_results.append({"cfg": gate, "verdict": "unknown",
                                 "reason": str(e)})
            continue
        gate_results.append({
            "cfg": gate,
            "verdict": "true" if verdict else "false",
            "reason": "" if verdict else why,
        })
    unknown = any(g["verdict"] == "unknown" for g in gate_results)
    gated_out = next((g for g in gate_results if g["verdict"] == "false"), None)
    decls.append({
        "package": package,
        "binary": binary,
        "target_kind": target_kind,
        "id": module_prefix + name,
        "fn": name,
        "file": file,
        "line": line,
        "test_kind": test_kind,
        "ignore": ignore,
        "ignore_reason": ignore_reason,
        "should_panic": should_panic,
        "gates": gate_results,
        "compiled_default": not unknown and gated_out is None,
        "gated_out": gated_out,
        "unknown_gated": unknown,
    })


# --------------------------------------------------------------------------
# Package / target model from cargo metadata
# --------------------------------------------------------------------------

def default_feature_closure(pkg):
    """Feature set enabled when the package builds with default features."""
    feats = set(pkg.get("features", {}).get("default", []))
    deps = {d["name"]: d for d in pkg.get("dependencies", [])}
    deps.update({d["name"]: d for d in pkg.get("dev-dependencies", [])})
    changed = True
    while changed:
        changed = False
        for f in sorted(feats):
            for spec in pkg.get("features", {}).get(f, []):
                if spec.startswith("dep:"):
                    continue
                if "/" in spec:
                    dep, _, df = spec.partition("/")
                    if "?" in dep:
                        dep = dep.rstrip("?")
                    if dep in deps and df not in feats:
                        # dep features are tracked by name for reporting
                        feats.add(f"{dep}/{df}")
                        changed = True
                elif spec not in feats:
                    feats.add(spec)
                    changed = True
    # feature flags without a declaration (e.g. implicit dep: features) stay
    # out; reconciliation names any mismatch.
    return feats


def workspace_packages():
    """Run cargo metadata and return (packages, members)."""
    cmd = ["sh", SELECTOR, "exec", "--", "cargo", "metadata",
           "--format-version", "1"]
    proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True,
                          timeout=300)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        raise SystemExit(f"cargo metadata failed ({proc.returncode})")
    meta = json.loads(proc.stdout)
    members = set(meta.get("workspace_members", []))
    packages = [p for p in meta.get("packages", []) if p["id"] in members]
    return meta, packages


def test_targets(pkg):
    """Test suite targets: lib, test, and bin targets with test = true."""
    out = []
    for t in pkg.get("targets", []):
        kinds = t.get("kind", [])
        if "lib" in kinds:
            out.append(("lib", t.get("name", pkg["name"]), t["src_path"],
                        t.get("required-features", []), t))
        elif "test" in kinds:
            out.append(("test", t["name"], t["src_path"],
                        t.get("required-features", []), t))
        elif "bin" in kinds and t.get("test", False):
            out.append(("bin", t["name"], t["src_path"],
                        t.get("required-features", []), t))
    return out


def scan_workspace(packages):
    """Scan every workspace package's test targets for declared tests."""
    decls = []
    macros = []
    targets_meta = []
    for pkg in packages:
        features = default_feature_closure(pkg)
        for kind, binname, src_path, req_features, t in test_targets(pkg):
            targets_meta.append({
                "package": pkg["name"],
                "binary": binname,
                "kind": kind,
                "src_path": os.path.relpath(src_path, ROOT),
                "required_features": req_features,
                "features_on": sorted(features),
            })
            scan_file_for_tests(src_path, pkg["name"], binname, kind, "",
                                [], features, decls, macros=macros)
    return decls, targets_meta, macros


# --------------------------------------------------------------------------
# Reconciliation
# --------------------------------------------------------------------------

def reconcile(decls, targets_meta, list_data, macros=None):
    """Classify declared vs collected. Returns rows + discrepancy lists.

    Macro-generated cases (collected IDs whose trailing name matches a
    `macro!(name, ...)` invocation in the same binary) are attributed to the
    invocation site; they are resolved macro families, not scanner misses.
    """
    macro_by_id = {}
    for m in (macros or []):
        macro_by_id.setdefault(m["candidate"], m)
    suites = list_data.get("rust-suites", {})
    collected = {}
    for key, suite in suites.items():
        for name, tc in suite.get("testcases", {}).items():
            collected[(key, name)] = {
                "binary_id": suite.get("binary-id"),
                "binary_name": suite.get("binary-name"),
                "package": suite.get("package-name"),
                "ignored": bool(tc.get("ignored")),
            }
    req_map = {(t["package"], t["binary"]): set(t["required_features"])
               for t in targets_meta}
    feats_map = {(t["package"], t["binary"]): set(t["features_on"])
                 for t in targets_meta}
    dup_ids = []
    groups = {}
    for d in decls:
        key = (d["package"], d["binary"], d["id"])
        groups.setdefault(key, []).append(d)
    for key, group in groups.items():
        # Known excluded variants cannot collide in this collection context.
        # Unknown gates remain possible collisions and must be reported.
        possible = [d for d in group if not d["gated_out"]]
        if len(possible) > 1:
            dup_ids.append(key)

    matched = 0
    matched_ids = set()
    no_declaration = []   # collected but not found by the static scan
    not_collected = []    # declared but not in the collected list
    rows = []
    for d in decls:
        suite_keys = [k for k in suites
                      if suites[k].get("package-name") == d["package"]
                      and suites[k].get("binary-name") == d["binary"]]
        hit = None
        for k in suite_keys:
            if d["id"] in suites[k]["testcases"]:
                hit = k
                break
        cls = ""
        why = ""
        if d["gated_out"]:
            cls = "gated-out"
            why = d["gated_out"]["reason"]
        elif d["unknown_gated"]:
            cls = "unknown-gated"
            why = "; ".join(g["reason"] for g in d["gates"]
                            if g["verdict"] == "unknown")
        else:
            req = req_map.get((d["package"], d["binary"]), set())
            on = feats_map.get((d["package"], d["binary"]), set())
            if req - on:
                cls = "target-gated"
                why = ("target required-features "
                       f"{sorted(req - on)} are not in the default build")
            elif hit:
                cls = "collected"
            else:
                cls = "declared-not-collected"
                why = "no collected suite entry matched this declaration"
        if hit:
            tc = suites[hit]["testcases"][d["id"]]
            matched_ids.add((d["package"], d["binary"], d["id"]))
            if bool(tc.get("ignored")) != d["ignore"]:
                why = (f"ignored flag mismatch: collected={tc.get('ignored')} "
                       f"declared={d['ignore']}")
                cls = "collected-ignored-mismatch"
        else:
            not_collected.append({"id": d["id"], "package": d["package"],
                                  "binary": d["binary"], "class": cls,
                                  "why": why, "file": d["file"],
                                  "line": d["line"]})
        rows.append({
            "package": d["package"], "binary": d["binary"], "id": d["id"],
            "file": d["file"], "line": d["line"], "class": cls, "why": why,
            "ignore": d["ignore"], "should_panic": d["should_panic"],
            "test_kind": d["test_kind"],
        })
    # collected IDs with no declaration; macro-generated ones are attributed
    declared_keys = {(d["package"], d["binary"], d["id"]) for d in decls}
    for (key, name), info in sorted(collected.items()):
        if not any(info["package"] == d["package"] and
                   info["binary_name"] == d["binary"] and
                   name == d["id"] for d in decls):
            entry = {
                "suite": key, "id": name, "package": info["package"],
                "binary": info["binary_name"],
            }
            m = macro_by_id.get(name)
            if m is not None:
                entry["class"] = "macro-generated"
                entry["macro"] = m["macro"]
                entry["file"] = m["file"]
                entry["line"] = m["line"]
            else:
                entry["class"] = "unattributed"
            no_declaration.append(entry)
    # platform-paired variants: the same id declared under complementary cfgs
    # (one compiled per host). A gated-out row whose id is collected through
    # its sibling is a pair member, not a missing case.
    collected_ids = {(r["package"], r["binary"], r["id"])
                     for r in rows if r["class"] == "collected"}
    for r in rows:
        if r["class"] in ("gated-out", "unknown-gated", "target-gated") and \
                (r["package"], r["binary"], r["id"]) in collected_ids:
            r["platform_pair"] = True
    return {
        "rows": rows,
        "matched": len(matched_ids),
        "no_declaration": no_declaration,
        "not_collected": not_collected,
        "duplicate_ids": dup_ids,
        "suite_count": len(suites),
        "collected_count": len(collected),
        "declared_count": len(decls),
    }


# --------------------------------------------------------------------------
# Doctest fence census (static estimate; rustdoc collects public-API docs)
# --------------------------------------------------------------------------

DOC_BLOCK_KINDS = {
    "": "run", "rust": "run", "rust,should_panic": "run",
    "should_panic": "run", "no_run": "compiled-not-run",
    "compile_fail": "compiled-expect-fail", "ignore": "ignored",
    "text": "not-rust", "ignore,should_panic": "ignored",
}


def census_doctests(packages):
    """Count doc-comment fenced blocks per lib package. Static estimate."""
    out = []
    for pkg in packages:
        libs = [t for t in pkg.get("targets", []) if "lib" in t.get("kind", [])]
        for lib in libs:
            totals = {}
            src = lib["src_path"]
            if not os.path.exists(src):
                continue
            # walk the crate's module files from lib.rs
            for path in crate_files(pkg, src):
                for kind in doc_fences(path):
                    totals[kind] = totals.get(kind, 0) + 1
            if totals:
                out.append({"package": pkg["name"], "lib": lib["name"],
                            "fences": totals,
                            "total": sum(totals.values())})
    return out


def crate_files(pkg, lib_rs):
    """All files reachable from lib.rs via `mod` declarations (approximate)."""
    seen = set()
    stack = [(lib_rs, os.path.dirname(lib_rs))]
    while stack:
        path, base = stack.pop()
        if path in seen or not os.path.exists(path):
            continue
        seen.add(path)
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            continue
        masked, _s, _c = mask_rust(text)
        for m in re.finditer(r"\bmod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", masked):
            name = m.group(1)
            for cand in (os.path.join(base, name + ".rs"),
                         os.path.join(base, name, "mod.rs")):
                if os.path.exists(cand):
                    stack.append((cand, os.path.dirname(cand)))
    return sorted(seen)


def doc_fences(path):
    """Fenced-block info strings in this file's doc comments."""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            text = f.read()
    except OSError:
        return []
    masked, _strings, comments = mask_rust(text)
    docs = []
    for start, end, raw in comments:
        if raw.startswith("///") or raw.startswith("//!"):
            body = raw[3:]
            if body.startswith("/"):
                body = body[1:]
            docs.append(body)
        elif raw.startswith("/**") or raw.startswith("/*!"):
            body = raw[3:-2] if raw.endswith("*/") else raw[3:]
            docs.append(body)
    joined = "\n".join(docs)
    kinds = []
    for m in re.finditer(r"```([^\n`]*)\n", joined):
        info = m.group(1).strip()
        # rustdoc runs fences annotated rust (or bare) plus modifiers
        base = info.split(",")[0].strip() if info else ""
        kinds.append(info)
    return kinds


# --------------------------------------------------------------------------
# Non-Rust validation route census (static triage of config/tasks.toml)
# --------------------------------------------------------------------------

ROUTE_SCRIPT = re.compile(
    r"(python3\s+[^\s\"']+\.py|sh\s+tests/scripts/[^\s\"']+\.sh|"
    r'rhai\s*=\s*"([^"]+\.rhai)"|python3\s+[^\s"\']+\.py)')


def census_routes():
    """Find non-Rust validation routes named by config/tasks.toml.

    Wrapper scripts that route Cargo (oxide-selector.sh, suite_guard.py,
    oxide-selector-pool.sh) are the mechanism, not validation payload, and
    are excluded.
    """
    path = os.path.join(ROOT, "config", "tasks.toml")
    with open(path, "r", encoding="utf-8") as f:
        lines = f.readlines()
    wrappers = {"scripts/oxide-selector.sh", "tests/scripts/suite_guard.py",
                "tests/scripts/oxide-selector-pool.sh"}
    out = []
    current = None
    selector_re = re.compile(r'^"([^"]+)"\s*=|^([A-Za-z0-9_:-]+)\s*=')
    py_re = re.compile(r"python3\s+(\S+\.(?:py|sh))")
    sh_re = re.compile(r"\bsh\s+(\S+\.sh)")
    rhai_re = re.compile(r'rhai\s*=\s*"([^"]+\.rhai)"')
    for ln, line in enumerate(lines, 1):
        sm = selector_re.match(line)
        if sm:
            current = sm.group(1) or sm.group(2)
        for rx in (py_re, sh_re, rhai_re):
            for m in rx.finditer(line):
                script = m.group(1)
                if script and script not in wrappers:
                    out.append({"selector": current, "line": ln,
                                "script": script})
    return out


# --------------------------------------------------------------------------
# Census driver
# --------------------------------------------------------------------------

def run_census(out_dir, reuse_list=None):
    os.makedirs(out_dir, exist_ok=True)
    provenance = {
        "root": ROOT,
        "head": git_head(),
        "selector_config": "default workspace suite (oxide nextest run --workspace)",
    }
    meta, packages = workspace_packages()
    with open(os.path.join(out_dir, "cargo-metadata.json"), "w") as f:
        json.dump({"packages": packages}, f, indent=1)
    if reuse_list:
        with open(reuse_list) as f:
            list_data = json.load(f)
    else:
        cmd = ["sh", SELECTOR, "nextest", "list", "--workspace",
               "--message-format", "json"]
        proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True,
                              timeout=3600)
        if proc.returncode != 0:
            sys.stderr.write(proc.stderr[-4000:])
            raise SystemExit(f"nextest list failed ({proc.returncode})")
        list_data = json.loads(proc.stdout)
        with open(os.path.join(out_dir, "nextest-list.json"), "w") as f:
            json.dump(list_data, f)
    decls, targets_meta, macros = scan_workspace(packages)
    report = reconcile(decls, targets_meta, list_data, macros)
    doctests = census_doctests(packages)
    routes = census_routes()
    provenance["nextest_list_source"] = reuse_list or "fresh nextest list"
    with open(os.path.join(out_dir, "inventory.jsonl"), "w") as f:
        for row in report["rows"]:
            f.write(json.dumps(row, sort_keys=True) + "\n")
    with open(os.path.join(out_dir, "targets.json"), "w") as f:
        json.dump(targets_meta, f, indent=1)
    with open(os.path.join(out_dir, "macro-candidates.json"), "w") as f:
        nd_ids = {nd["id"] for nd in report["no_declaration"]
                  if nd.get("class") == "macro-generated"}
        used = [m for m in macros if m["candidate"] in nd_ids]
        json.dump({"total_candidates": len(macros),
                   "attributed": used}, f, indent=1)
    with open(os.path.join(out_dir, "doctests.json"), "w") as f:
        json.dump(doctests, f, indent=1)
    with open(os.path.join(out_dir, "routes.json"), "w") as f:
        json.dump(routes, f, indent=1)
    with open(os.path.join(out_dir, "reconciliation.json"), "w") as f:
        json.dump({k: v for k, v in report.items() if k != "rows"}, f, indent=1)
    summary = render_summary(report, doctests, routes, provenance)
    with open(os.path.join(out_dir, "summary.txt"), "w") as f:
        f.write(summary)
    provenance_file = os.path.join(out_dir, "provenance.json")
    with open(provenance_file, "w") as f:
        json.dump(provenance, f, indent=1)
    print(summary)
    # census is only trustworthy when reconciliation is clean or named
    unattributed = [nd for nd in report["no_declaration"]
                    if nd.get("class") == "unattributed"]
    problems = (len(unattributed) + len(report["duplicate_ids"]))
    if problems:
        print(f"RECONCILIATION ATTENTION: {problems} discrepancy(ies); "
              f"see reconciliation.json", file=sys.stderr)
    return report


def git_head():
    try:
        proc = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT,
                              capture_output=True, text=True, timeout=30)
        return proc.stdout.strip()
    except Exception:
        return "unknown"


def render_summary(report, doctests, routes, provenance):
    rows = report["rows"]
    classes = {}
    for r in rows:
        classes[r["class"]] = classes.get(r["class"], 0) + 1
    lines = []
    ap = lines.append
    ap(f"Test-suite census @ {provenance.get('head', 'unknown')[:12]}")
    ap(f"  suites (binaries): {report['suite_count']}")
    ap(f"  collected cases:   {report['collected_count']}")
    ap(f"  declared cases:    {report['declared_count']}")
    ap(f"  matched:           {report['matched']}")
    for k in sorted(classes):
        ap(f"  {k:28s} {classes[k]}")
    if report["no_declaration"]:
        macro_nd = [nd for nd in report["no_declaration"]
                    if nd.get("class") == "macro-generated"]
        unatt = [nd for nd in report["no_declaration"]
                 if nd.get("class") == "unattributed"]
        ap(f"  macro-generated (attributed to invocation): {len(macro_nd)}")
        for nd in macro_nd[:10]:
            ap(f"    {nd['id'][:80]}  <- {nd.get('macro')} @ {nd.get('file')}:{nd.get('line')}")
        if unatt:
            ap(f"  UNATTRIBUTED collected cases: {len(unatt)}")
            for nd in unatt[:20]:
                ap(f"    {nd['suite']}  {nd['id']}")
    if report["duplicate_ids"]:
        ap(f"  duplicate declared IDs: {len(report['duplicate_ids'])}")
        for d in report["duplicate_ids"][:20]:
            ap(f"    {d}")
    ignored = sum(1 for r in rows if r["ignore"] and
                  r["class"] == "collected")
    ignored_gated = sum(1 for r in rows if r["ignore"] and
                        r["class"] != "collected")
    sp = sum(1 for r in rows if r["should_panic"])
    pairs = sum(1 for r in rows if r.get("platform_pair"))
    ap(f"  declared ignored:  {ignored} collected in the default build, "
       f"{ignored_gated} behind non-default gates   should_panic: {sp}")
    if pairs:
        ap(f"  platform-paired variants (complementary cfgs, one per host): "
           f"{pairs}")
    ap("")
    ap("Doctest fence census (static estimate; rustdoc collects public docs):")
    dt_total = 0
    for d in doctests:
        dt_total += d["total"]
        ap(f"  {d['package']:24s} {d['total']:4d}  {d['fences']}")
    ap(f"  total doc fences: {dt_total}")
    ap("")
    ap(f"Non-Rust validation routes in config/tasks.toml: {len(routes)}")
    seen_scripts = {}
    for r in routes:
        seen_scripts.setdefault(r["script"], []).append(r["selector"])
    for s in sorted(seen_scripts):
        ap(f"  {s}  <- {len(seen_scripts[s])} selector(s)")
    return "\n".join(lines) + "\n"


# --------------------------------------------------------------------------
# Self-test: embedded controls for the reconciliation logic
# --------------------------------------------------------------------------

def self_test():
    """Prove duplicate detection, missing attribution, and conditional classes."""
    tmp = tempfile.mkdtemp(prefix="effigy-test-inventory-selftest.")
    failures = []
    try:
        pkg_dir = os.path.join(tmp, "demo")
        src = os.path.join(pkg_dir, "src")
        os.makedirs(src)
        tests_dir = os.path.join(pkg_dir, "tests")
        os.makedirs(tests_dir)
        lib = r'''
//! demo crate
#![deny(missing_debug_implementations)]

#[cfg(feature = "nope")]
mod gated_mod;
#[cfg(test)]
mod m1 {
    #[test]
    fn plain_collected() {}
    #[test]
    #[ignore = "expensive baseline proof"]
    fn ignored_baseline() {}
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_only() {}
    #[test]
    #[cfg(feature = "extra")]
    fn feature_only() {}
    #[test]
    #[should_panic]
    fn panics() {}
    macro_rules! make_test {
        ($name:ident) => { #[test] fn $name() {} };
    }
    make_test!(generated_plain);
}
impl Demo {
    #[test]
    fn test_inside_impl() {}
}
pub struct Demo;
#[path = "demo_tests.rs"]
mod demo_tests;

fn host_fn() {
    #[cfg(target_os = "vms")]
    {
        unreachable!();
    }
}

#[cfg(test)]
mod m2 {
    #[test]
    fn no_leaked_gate() {}
}

mod plain_mod {
    #[test]
    fn inner_case() {}
}

fn statement_cfg_fn() {
    #[cfg(not(unix))]
    let owner_uid = 0;
    let other = owner_uid_placeholder();
}
fn owner_uid_placeholder() -> u32 { 0 }

#[cfg(test)]
mod m3 {
    #[test]
    fn after_statement_cfg() {}
}
'''
        with open(os.path.join(src, "lib.rs"), "w") as f:
            f.write(lib)
        with open(os.path.join(src, "demo_tests.rs"), "w") as f:
            f.write('''#[test]
fn path_module_case() {}
''')
        with open(os.path.join(src, "gated_mod.rs"), "w") as f:
            f.write('''#[test]
fn gated_mod_case() {}
''')
        with open(os.path.join(tests_dir, "integration.rs"), "w") as f:
            f.write('''#[test]
fn integration_case() {}
#[cfg(feature = "extra")]
#[test]
fn integration_feature_case() {}
''')
        with open(os.path.join(tests_dir, "gated_target.rs"), "w") as f:
            f.write("#[test]\nfn target_case() {}\n")
        pkg = {
            "name": "demo", "id": "demo",
            "features": {"default": [], "extra": ["dep:helper"]},
            "dependencies": [{"name": "helper", "optional": True}],
            "targets": [
                {"name": "demo", "kind": ["lib"],
                 "src_path": os.path.join(src, "lib.rs"),
                 "required-features": [], "test": True},
                {"name": "integration", "kind": ["test"],
                 "src_path": os.path.join(tests_dir, "integration.rs"),
                 "required-features": [], "test": True},
                {"name": "gated_target", "kind": ["test"],
                 "src_path": os.path.join(tests_dir, "gated_target.rs"),
                 "required-features": ["fault-x"], "test": True},
            ],
        }
        decls, targets_meta, macros = scan_workspace([pkg])
        ids = sorted(d["id"] for d in decls
                     if d["binary"] == "demo" and d["compiled_default"])
        expect = ["demo_tests::path_module_case", "m1::ignored_baseline",
                  "m1::panics", "m1::plain_collected", "m2::no_leaked_gate",
                  "m3::after_statement_cfg", "plain_mod::inner_case",
                  "test_inside_impl"]
        if ids != expect:
            failures.append(f"default-compiled lib decls {ids} != {expect}")
        feat = [d for d in decls if d["id"] == "m1::feature_only"]
        if len(feat) != 1 or feat[0]["compiled_default"] or \
                "extra" not in feat[0]["gated_out"]["reason"]:
            failures.append(f"feature-gated classification wrong: {feat}")
        plat = [d for d in decls if d["id"] == "m1::linux_only"]
        if len(plat) != 1 or plat[0]["compiled_default"] or \
                "target_os" not in plat[0]["gated_out"]["reason"]:
            failures.append(f"platform-gated classification wrong: {plat}")
        ign = [d for d in decls if d["id"] == "m1::ignored_baseline"]
        if len(ign) != 1 or not ign[0]["ignore"] or \
                not ign[0]["compiled_default"]:
            failures.append(f"ignored classification wrong: {ign}")
        sp = [d for d in decls if d["id"] == "m1::panics"]
        if len(sp) != 1 or not sp[0]["should_panic"]:
            failures.append(f"should_panic not recorded: {sp}")
        leak = [d for d in decls if d["id"] == "m2::no_leaked_gate"]
        if len(leak) != 1 or [g["cfg"] for g in leak[0]["gates"]] != ["test"]:
            failures.append(f"cfg-block leak control failed: {leak}")
        inner = [d for d in decls if d["id"] == "plain_mod::inner_case"]
        if len(inner) != 1 or inner[0]["gates"] != []:
            failures.append(f"inner-attr adjacency control failed: {inner}")
        gm = [d for d in decls if d["id"] == "gated_mod::gated_mod_case"]
        if len(gm) != 1 or gm[0]["compiled_default"] or \
                "nope" not in gm[0]["gated_out"]["reason"]:
            failures.append(f"adjacent outer cfg gate control failed: {gm}")
        stmt = [d for d in decls if d["id"] == "m3::after_statement_cfg"]
        if len(stmt) != 1 or [g["cfg"] for g in stmt[0]["gates"]] != ["test"]:
            failures.append(f"statement-cfg leak control failed: {stmt}")
        integ_feat = [d for d in decls
                      if d["id"] == "integration_feature_case"]
        if len(integ_feat) != 1 or integ_feat[0]["compiled_default"]:
            failures.append(f"integration feature gate wrong: {integ_feat}")

        # collected side: the macro-generated case exists at runtime only
        list_data = {
            "rust-suites": {
                "demo": {
                    "package-name": "demo", "binary-id": "demo",
                    "binary-name": "demo", "kind": "lib",
                    "testcases": {
                        "m1::plain_collected": {"kind": "test", "ignored": False},
                        "m1::ignored_baseline": {"kind": "test", "ignored": True},
                        "m1::panics": {"kind": "test", "ignored": False},
                        "test_inside_impl": {"kind": "test", "ignored": False},
                        "m1::generated_plain": {"kind": "test", "ignored": False},
                        "m1::duplicate_probe": {"kind": "test", "ignored": False},
                    },
                },
                "demo::integration": {
                    "package-name": "demo", "binary-id": "demo::integration",
                    "binary-name": "integration", "kind": "test",
                    "testcases": {
                        "integration_case":
                            {"kind": "test", "ignored": False},
                    },
                },
            },
        }
        # inject a duplicate declaration to prove duplicate detection
        active = next(d for d in decls if d["id"] == "m1::plain_collected")
        excluded = dict(active, gated_out={"reason": "cfg(not(test))"})
        paired = reconcile(decls + [excluded], targets_meta, list_data, macros)
        if paired["duplicate_ids"]:
            failures.append(f"excluded variant flagged as duplicate: {paired['duplicate_ids']}")
        decls_dup = decls + [dict(active)]
        report = reconcile(decls_dup, targets_meta, list_data, macros)
        target = [r for r in report["rows"] if r["id"] == "target_case"]
        if len(target) != 1 or target[0]["class"] != "target-gated" or \
                "fault-x" not in target[0]["why"]:
            failures.append(f"target required-features control failed: {target}")
        nd_by_id = {x["id"]: x for x in report["no_declaration"]}
        gen = nd_by_id.get("m1::generated_plain")
        if gen is None or gen.get("class") != "macro-generated" or \
                gen.get("macro") != "make_test":
            failures.append(f"macro-attribution control failed: {gen}")
        unk = nd_by_id.get("m1::duplicate_probe")
        if unk is None or unk.get("class") != "unattributed":
            failures.append(f"missing-attribution control failed: {unk}")
        if "integration_case" in nd_by_id:
            failures.append(f"integration root fn wrongly flagged as "
                            f"unattributed: {nd_by_id}")
        if not report["duplicate_ids"]:
            failures.append("duplicate-ID control failed")
        mis = [r for r in report["rows"] if r["class"] == "collected-ignored-mismatch"]
        if mis:
            failures.append(f"unexpected ignored-mismatch rows: {mis}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if failures:
        for f in failures:
            print(f"FAIL: {f}", file=sys.stderr)
        raise SystemExit(1)
    print("self-test: ok (duplicate IDs, missing attribution, macro-generated "
          "attribution, ignored / feature-gated / platform-gated / "
          "target-required distinctions)")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[1])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("census", help="full inventory + reconciliation")
    c.add_argument("--out", default=None)
    c.add_argument("--nextest-list", default=None,
                   help="reuse a collected nextest list JSON")
    sub.add_parser("self-test", help="prove the reconciliation logic")
    args = ap.parse_args()
    if args.cmd == "self-test":
        self_test()
        return
    out = args.out or os.environ.get("EFFIGY_TEST_INVENTORY_OUT") or \
        os.path.join(ROOT, "target", "test-inventory")
    run_census(out, args.nextest_list)


if __name__ == "__main__":
    main()

# 036 Papercuts Discovery Contract

Status: active
Updated: 2026-08-09

## Purpose

Define Effigy's read and write boundary for conventional root-level
`PAPERCUTS.md` queues without turning those observations into roadmap or issue
authority.

## Command Grammar

```text
effigy papercuts [--all] [--scope <PATH>] [--json]
effigy papercuts add <TITLE> --friction <TEXT> --impact <TEXT> \
  --fix <TEXT> --surface <TEXT> [--scope <PATH>] [--json]
```

The bare command is the list operation. It shows open entries only unless
`--all` is present. `papercuts` is a first-class command and must work without
an `effigy.toml` or other resolved catalog.

## Scope Resolution

- `--scope` defaults to the invocation working directory.
- If the scope is inside a project, the nearest ancestor containing `.git` or
  `effigy.toml` is the sole project root.
- Otherwise the scope is a collection root. Effigy inspects its immediate
  child directories and treats children containing `.git` or `effigy.toml` as
  project roots.
- Effigy reads only `<project-root>/PAPERCUTS.md`.
- Discovery does not recursively collect nested templates, fixtures, vendored
  repos, or descendant project queues.
- Symlinked directories are not followed during collection discovery.

## Markdown Input

An entry begins with `### [ ] <title> — YYYY-MM-DD`; `[x]` and `[X]` are
closed. The canonical named fields are `Friction`, `Impact`, `Possible fix`,
and `Surface`. Fields may continue across following lines until the next field
or entry.

Malformed headings or missing canonical fields produce diagnostics. One bad
entry does not hide valid entries from the same or another project.

## Output Contract

JSON output remains inside `effigy.command.v1`; discovery returns
`effigy.papercuts.v1` and contains the resolved scope and mode, project/file
and status counts, normalized entries, and non-fatal diagnostics. Each entry
includes project name/root, source path/line, status, title, date, canonical
fields, optional resolution/detail, and a deterministic content fingerprint.

Human output groups entries by project. Entries sort open before closed, then
newest date first and title ascending. JSON uses the same order.

Successful capture returns `effigy.papercuts.add.v1` with the normalized
inserted entry.

## Add Contract

- `add` requires a single project scope; collection scope is rejected.
- Missing queues are created with the canonical Northstar-compatible starter.
- New entries are inserted immediately below the `## Open` comment block.
- Exact normalized open-title duplicates are rejected.
- Writes use a same-directory temporary file and atomic rename while holding a
  per-file Effigy lock.
- Existing unrelated Markdown is preserved outside the insertion.

## Authority Boundary

Northstar owns the producer-side Markdown convention. Effigy consumes the
documented shape but does not require Northstar at runtime. Effigy owns scope
resolution, tolerant parsing, diagnostics, mutation safety, rendering, and the
`effigy.papercuts.v1` payload.

Papercuts remain observations. Effigy does not prioritize, semantically
deduplicate, create issues, promote entries, close entries, or alter roadmaps.

## Validation

- focused `effigy-papercuts` parser, scope, ordering, diagnostic, and add tests
- CLI parse/help/global JSON tests
- command-level project and collection fixtures, including a nested template
- JSON contract examples and repository docs checks

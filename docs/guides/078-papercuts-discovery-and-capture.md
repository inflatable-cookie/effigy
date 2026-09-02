# Papercuts Discovery And Capture

Use `effigy repo papercuts` to collect small, actionable execution-friction notes
from conventional project-root `PAPERCUTS.md` queues.

## Discover One Project

From anywhere inside a project:

```bash
effigy repo papercuts
effigy repo papercuts --all
effigy --json papercuts
```

Effigy finds the nearest ancestor containing `.git` or `effigy.toml`, then
reads only that root's `PAPERCUTS.md`. The default contains open entries;
`--all` also includes checked entries.

## Discover Sibling Projects

From a directory such as `~/Dev/projects`:

```bash
cd ~/Dev/projects
effigy repo papercuts
effigy --json papercuts
```

When the current directory is not inside a project, Effigy treats it as a
collection. It inspects immediate child directories that contain `.git` or
`effigy.toml` and reads each root queue. It does not recursively collect nested
templates, fixtures, vendor trees, or descendant repositories.

Use an explicit scope without changing directory:

```bash
effigy repo papercuts --scope ~/Dev/projects
effigy --json papercuts --scope ~/Dev/projects
```

## Capture One Entry

`add` accepts one project only:

```bash
effigy repo papercuts add "Graph output is noisy" \
  --friction "stale output floods agent context" \
  --impact "every orientation repeats the refresh step" \
  --fix "refresh once before returning query output" \
  --surface "Effigy graph"
```

If `PAPERCUTS.md` is missing, Effigy creates the canonical starter. Otherwise
it inserts the entry at the top of `## Open`, preserving existing content.
Exact normalized open-title duplicates are rejected. Collection targets are
also rejected rather than guessing which project owns the observation.

Writes use a per-queue lock and atomic replacement. A failed or concurrent add
does not leave a partial queue.

## Agent Triage

Periodic agents should consume JSON:

```bash
effigy --json papercuts --scope ~/Dev/projects
```

The `result` payload uses `effigy.papercuts.v1`. It includes normalized entries,
project roots, source paths and lines, fingerprints, counts, and non-fatal
parse diagnostics. Successful capture returns `effigy.papercuts.add.v1`.

Use the inventory to group duplicates and propose work. Do not treat every
entry as an automatic issue, backlog item, roadmap commitment, or priority.

## Input Convention

Effigy accepts the Northstar-compatible heading and named fields:

```markdown
### [ ] Short title — 2026-08-09
- Friction: what was harder than it should have been
- Impact: repeat cost, ambiguity, or failure mode
- Possible fix: smallest plausible improvement
- Surface: tool, document, script, or workflow
```

Fields may span multiple lines. Checked headings are closed. Missing fields or
malformed headings appear in `diagnostics` while other valid entries remain
available.

## Boundaries

- no semantic or LLM deduplication inside Effigy
- no prioritization, issue creation, promotion, close, or roadmap mutation
- no recursive arbitrary-directory scan
- no Northstar runtime dependency
- `--scope` is distinct from Effigy's repo-targeting `--repo`


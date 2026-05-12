---
name: effigy
description: >
  Effigy task-runner skill. Teaches an agent how to discover tasks, run common
  workflows (test, container up, release prepare), parse `--json` envelopes,
  and avoid release/CI footguns when working in any repo that uses Effigy.
  Use when: user mentions effigy, runs `effigy <command>`, edits `effigy.toml`,
  asks how to run tests / start dev / cut a release in a repo with
  `effigy.toml`, or invokes `/effigy`.
---

# Effigy Skill

## What Effigy is

Effigy is a unified **task runner** and **repo runtime** for monorepos. One CLI
(`effigy`) loads each repo's **`effigy.toml`** (often composed from included
files), then routes **selectors** (`dev`, `api/test`, `qa:ci:fast`, …) to
manifest tasks or **built-ins** (`test`, `init`, `doctor`, …).

Names like **`dev`** are normal **task names** the repo defines unless they
collide with a built-in. **`test`** is usually the **built-in** test orchestration
unless `tasks.test` overrides it. Treat `effigy <selector>` as the canonical
surface for that repo's project work — prefer it over raw `cargo`, `npm`,
`bun`, `docker compose`, etc., when an Effigy task covers the path.

## Footguns (read first)

Hard rules. Violating these causes broken releases, unauthorized CI changes,
or duplicate package surface that drifts.

- **Never modify `.github/workflows/`** without explicit human approval.
- **Never bypass release gates.** Fix the underlying issue.
- **Never re-tag a failed release.** Fix lands in next PATCH.
- **Never run release commands** (`effigy release prepare/execute`) without
  explicit human ask.
- **Don't add `package.json` scripts** that re-export Effigy tasks. Run
  `effigy <task>` directly; keep package scripts package-native.
- **Don't add `--repo .`** when already running inside the target repo.

Full rationale: `references/footguns.md`.

## Discovery loop in a strange repo

When entering a repo that contains `effigy.toml`, run these in order:

```bash
effigy doctor              # health check + routing diagnostic
effigy tasks               # list available tasks
effigy test --plan         # show resolved test plan (JSON-friendly)
effigy config              # show resolved config
effigy --json tasks        # machine-readable task inventory
```

If `effigy` is not on PATH, bootstrap with
`cargo run --bin effigy -- <command>` from inside the Effigy source repo, or
install: `npx skills add inflatable-cookie/effigy` covers this skill, but the
binary itself installs via the project's own README.

Details: `references/first-five-commands.md`.

## Common workflows

| Goal | Command |
|------|---------|
| Run tests | `effigy test` |
| Inspect test plan | `effigy test --plan` |
| Bring local dev up | `effigy container up` then `effigy dev` ( **`dev`** is the repo's task name) |
| Pre-push validation (fast) | `effigy qa:ci:fast` (example; **Effigy repo** and similar stacks define `qa:*`) |
| Pre-push validation (full local) | `effigy qa:ci:local` |
| Full QA | `effigy qa` |
| Scaffold manifest | `effigy init` then `effigy migrate` |
| Extract changelog section | `effigy changelog extract --version X.Y.Z` |
| List release gates | `effigy release gates` |

Details: `references/workflow-shortcuts.md`.

## Selector routing (60-second version)

A task selector (`test`, `api/test`, `qa:ci:fast`) resolves through:

1. **Alias prefix** match (e.g. `qa:` → `qa:ci:fast`)
2. **Path prefix** match (e.g. `api/test` → `api/` workspace's `test`)
3. **CWD-nearest** workspace
4. **Shallowest** match if still ambiguous

Use `effigy doctor <selector> <args...>` (explain mode) to see why a selector
resolved as it did. If routing is ambiguous and explain output does not
disambiguate, **stop and ask**.

Details: `references/selector-routing.md`. Full spec:
`docs/guides/016-task-routing-precedence.md`.

## JSON envelope

Every command run with `--json` returns an `effigy.command.v1` envelope:

```json
{
  "schema": "effigy.command.v1",
  "result": { ... },
  "error": null
}
```

On error: `result` is null and `error.details` carries a structured payload.
Parse `result.payload` for command-specific data.

Quick example — list task names:

```bash
effigy --json tasks | jq -r '.result.payload.tasks[].name'
```

Details: `references/json-envelope.md`. Full spec:
`docs/guides/017-json-output-contracts.md`.

## Bundle sources

Repos declare their bundle preset through `[bundle].base`:

```toml
[bundle]
base = { type = "path", dir = "bundles/acme" }
host = "acme.test"
project_name = "acme-dev"
```

Supported source types:

- `path` — local directory containing `bundle.toml` + `export.toml`
- `git` — clone-on-demand from a git URL (cached in `.effigy/cache/bundles/git/`)
- `oci` — pull-on-demand from a container registry (cached in `.effigy/cache/bundles/oci/`)

Bundle caches live inside the project `.effigy` directory so they are available
inside workspace containers.

Key commands:

```bash
effigy bundle inspect          # inspect active repo bundle source
effigy bundle sync             # refresh git/oci bundle caches
effigy --json bundle inspect   # machine-readable source metadata
```

The old `base = "underlay"` string form and the built-in shipped bundle catalog
are removed. Bundles must now come from a typed source.

Details: `references/bundle-sources.md`. Full reference:
`docs/guides/066-local-manifest-bundles.md`.

## Config shapes

`effigy.toml` and `config/tasks.toml` host these top-level sections:

- `[tasks]` — task definitions (shell strings, ref chains, Rhai scripts)
- `[systems.<name>]` — system-level orchestration (default workspace, etc.)
- `[containers.<name>]` — container catalog
- `[bootstrap]` — first-run setup steps
- `[release]` — release configuration
- `[bundle]` — bundle source and inputs

Details: `references/config-shapes.md`. Full reference:
`docs/guides/025-command-reference-matrix.md`.

## Release

Releases follow a strict gate sequence. **Do not run release commands without
explicit human instruction.**

The canonical sequence (for reference, not unprompted execution):

1. `effigy release simulate`
2. `effigy release status --check-gates`
3. `effigy release prepare --plan`
4. `effigy release prepare --yes --check-gates`
5. `effigy release execute --plan`
6. `effigy release execute --yes`
7. `effigy release verify-install --tag vX.Y.Z`
8. `effigy changelog extract CHANGELOG.md --version X.Y.Z`

If a gate fails: fix the underlying cause. Never bypass. Never re-tag.

Details: `references/release-protocol.md`. Full spec:
`docs/guides/051-release-orchestration.md`.

## When to stop and ask

Hand back to the human rather than guess when:

- Selector routing is ambiguous and `effigy doctor <selector> <args...>` does not resolve it.
- `effigy doctor` returns structural errors (manifest invalid, config malformed).
- A release command was not explicitly requested.
- The change requires editing `.github/workflows/` or `release/` workflow files.
- A failing gate suggests the fix is "skip the gate."
- The user asks for a behavior that conflicts with a footgun rule above.

## Deeper docs

For depth beyond this skill, read the canonical guides in the Effigy repo:

| Topic | Guide |
|-------|-------|
| Task routing | `docs/guides/016-task-routing-precedence.md` |
| JSON contracts | `docs/guides/017-json-output-contracts.md` |
| Quick start + cookbook | `docs/guides/021-quick-start-and-command-cookbook.md` |
| Command reference | `docs/guides/025-command-reference-matrix.md` |
| Bundle sources | `docs/guides/066-local-manifest-bundles.md` |
| Underlay starter | `docs/guides/065-underlay-starter.md` |
| Agent adoption | `docs/guides/047-agent-and-cross-repo-adoption.md` |
| CI + release protocol | `docs/guides/049-ci-binary-distribution-and-release-protocol.md` |
| Release orchestration | `docs/guides/051-release-orchestration.md` |
| Northstar consumer contract | `docs/guides/056-northstar-effigy-consumer-repo-contract.md` |

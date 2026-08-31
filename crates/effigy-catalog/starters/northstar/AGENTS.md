# Agent Instructions for &lt;PROJECT_NAME&gt;

This repo uses the Northstar + Effigy consumer contract. The docs,
planning shape, and validation surface are documented in
[`docs/guides/056-northstar-effigy-consumer-repo-contract.md`](https://github.com/inflatable-cookie/effigy/blob/main/docs/guides/056-northstar-effigy-consumer-repo-contract.md)
(Effigy upstream).

Effigy is **manifest-driven** (`effigy.toml`, often split across includes): most
`effigy <name>` calls are **repo tasks** (`qa`, `validate`, …). Built-ins include
`test`, `init`, `doctor`, and the short list from **`effigy --help`**.

## Operating Loop

Route by job, not by startup ritual:

1. Use `effigy graph` when the job is code understanding.
2. Use `effigy docs context "<question>"` when the job is documentation
   authority: which contract, roadmap, or decision governs the work. It returns
   exact sections with provenance, not a summary.
3. Use `effigy tasks` when you need selector inventory or QA surfaces.
4. Use `effigy doctor` (or `effigy health`) when routing is unclear or repo
   health is the task.
5. Inspect tests with `effigy test --plan` when test execution shape matters.
6. Prefer `effigy <task>` over raw tooling whenever a task covers the path.
7. Use `effigy --json <command>` when a machine consumer needs the output.
8. Only use `--repo <PATH>` when intentionally targeting a different repo.
   Never teach `--repo .` as a default.
9. Fall back to raw tools only when Effigy does not cover the path.

## Default test policy

`effigy test` is always the built-in orchestration entrypoint. Use automatic
Rust/Vitest detection for simple repos or declare named `[test.suites]` for
explicit polyglot and lifecycle-aware routing. Never define `tasks.test`.

## Docs authority

- `docs/README.md` names the docs authority for this repo.
- `docs/vision/README.md` is the product vision index.
- `docs/roadmaps/README.md` is the active milestone queue.
- `docs/logs/README.md` is the evidence and decision log.

Do not collapse these three into a single generic planning note.

`[docs_policy.graph]` in `effigy.toml` declares this repo's documentation
semantics: roots, kinds, authority weights, metadata fields, currentness values,
and typed relations. It is copied configuration owned by this repo. Effigy reads
that committed block and nothing else when answering `effigy docs context`; no
starter and no installed skill is consulted at runtime. Updating a template is
an explicit merge, never an automatic reinterpretation.

## Validation

Run `effigy qa` to validate the full contract:

- `validate` — repo-owned generic checks
- `qa:docs` — docs spine + README/agent-contract drift
- `qa:northstar` — indexes, next-actions, headings, forbidden defaults

Fix the underlying drift when `qa:northstar` fails. Do not suppress
checks to get a passing run.

## Changelog

Append user-facing changes to `CHANGELOG.md` under `[Unreleased]` as
you ship them. Categories: **Breaking**, **Added**, **Changed**,
**Fixed**.

## Release policy

- Never initiate a release without explicit human instruction.
- Never modify CI workflows without explicit approval.
- Never bypass release gates — fix the underlying issue instead.

Once release work is active, the canonical entrypoint is
`effigy release prepare` → `effigy release execute`.

## Fallback boundary

Raw tools are allowed only when Effigy does not cover the path (e.g.
rare migration scripts or one-off scratch). When you reach for raw
tooling, note it in the next log entry so the fallback boundary stays
visible.

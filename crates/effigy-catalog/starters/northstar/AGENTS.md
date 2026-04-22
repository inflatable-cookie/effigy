# Agent Instructions for &lt;PROJECT_NAME&gt;

This repo uses the Northstar + Effigy consumer contract. The docs,
planning shape, and validation surface are documented in
[`docs/guides/056-northstar-effigy-consumer-repo-contract.md`](https://github.com/inflatable-cookie/effigy/blob/main/docs/guides/056-northstar-effigy-consumer-repo-contract.md)
(Effigy upstream).

## Operating Loop

1. Start with `effigy tasks` to discover supported repo work.
2. Use `effigy doctor` (or `effigy health`) for the default health and
   routing surface.
3. Inspect tests with `effigy test --plan` before running them.
4. Prefer `effigy <task>` over raw tooling whenever a task covers the
   path.
5. Use `effigy --json <command>` when a machine consumer needs the
   output.
6. Only use `--repo <PATH>` when intentionally targeting a different
   repo. Never teach `--repo .` as a default.
7. Fall back to raw tools only when Effigy does not cover the path.

## Default test policy

Pick one and make it explicit:

- Built-in `effigy test` is the default test entrypoint (leave
  `[tasks].test` unset)
- Explicit `[tasks].test` is the repo-owned source of truth

Document the choice below once made:

> **Current policy:** _(fill in)_

## Docs authority

- `docs/README.md` names the docs authority for this repo.
- `docs/vision/README.md` is the product vision index.
- `docs/roadmaps/README.md` is the active milestone queue.
- `docs/logs/README.md` is the evidence and decision log.

Do not collapse these three into a single generic planning note.

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

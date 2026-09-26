# 056 — Northstar and Effigy in a consumer repository

This guide's path stays stable for consumers. A new repository can start with `effigy init northstar`; the bundled starter follows lean Northstar. Existing repositories should use the [lean adoption guide](https://github.com/inflatable-cookie/northstar) and migrate in one deliberate cut after draining Queue work.

## Repository shape

- `AGENTS.md` orients agents and names `effigy qa` as validation.
- `docs/README.md` describes the current state and links by topic.
- `docs/knowledge/` owns current vision, architecture, contracts, release procedure, retired concepts, and open questions.
- `docs/plan.md` holds prioritized intent; `docs/triage/` holds unresolved leads.
- `docs/guides/` remains product documentation at stable consumer paths.
- `.paseo/queue.json` uses schema `paseo.queue.control.v5`, `closeout: "queue"`, and `hooks: []` after Queue supports the cut.

Queue owns briefs, task status, review, closeout, and outcomes. A repository's committed `[docs_policy.graph]` profile is the only authority for `effigy docs context`; neither the starter nor an installed skill is read at query time.

## Effigy commands

Use `effigy graph` for code ownership, `effigy docs context` for document authority, `effigy tasks` for selector inventory, `effigy doctor` for routing or health, and `effigy test --plan` when test shape matters. Run `effigy qa` before a PR. A consumer's QA command must work from a fresh checkout with its own dependencies.

The [agent adoption guide](047-agent-and-cross-repo-adoption.md) covers Effigy command usage. The repository's own `docs/knowledge/contracts/release.md` must describe its actual release steps, even if it does not release yet.

# 056 — Northstar docs and Effigy in a consumer repository

This guide's path stays stable for consumers.

[Northstar](https://github.com/inflatable-cookie/northstar) is an opt-in
stack of agent skills. Its documentation structure is the `northstar-docs`
skill: `AGENTS.md` for agent orientation, `docs/README.md` as the entry point
to the docs, and `docs/knowledge/` for the why and depth, with one owning file
per fact. To adopt it, copy the skill's `template/` and follow its adoption
reference. Effigy bundles no Northstar starter; `effigy init` emits only the
generic `minimal` starter.

## Running the skill's checks

The skill's scripts are dependency-free Python. Run them directly, or through
the skill's Effigy catalog once the skill is installed:

```sh
effigy skill run northstar-docs/check-links
effigy skill run northstar-docs/changelog -- check
```

## Documentation graph profile

A repository that wants Effigy's documentation graph keeps one committed
`[docs_policy.graph]` block in its own `effigy.toml`. Effigy resolves
documentation semantics from that manifest only, never from an installed skill
directory or template. See
[`../knowledge/contracts/041-documentation-graph-profile-contract.md`](../knowledge/contracts/041-documentation-graph-profile-contract.md).

## Effigy commands

Use `rg` and `docs/README.md` for code and document authority, `effigy tasks` for selector inventory, `effigy doctor` for routing or health, and `effigy test --plan` when test shape matters. Run `effigy qa` before a PR. A consumer's QA command must work from a fresh checkout with its own dependencies.

The [agent adoption guide](047-agent-and-cross-repo-adoption.md) covers Effigy command usage. The repository's own `docs/knowledge/contracts/release.md` must describe its actual release steps, even if it does not release yet.

# 056 — Northstar and Effigy in a consumer repository

This guide's path stays stable for consumers. A new repository can start with `effigy init northstar`; the bundled starter follows lean Northstar. Existing repositories should use the [lean adoption guide](https://github.com/inflatable-cookie/northstar) and migrate in one deliberate cut after draining Queue work.

## Repository shape

- `AGENTS.md` orients agents and names `effigy qa` as validation.
- `docs/README.md` describes the current state and links by topic.
- `docs/knowledge/` owns current vision, architecture, contracts, release procedure, retired concepts, and open questions.
- Queue holds the plan (lanes, lane documents and order), leads, papercuts and draft briefs.
- `docs/guides/` remains product documentation at stable consumer paths.
- Queue closeout is the default when no control manifest applies. New lean
  repositories carry no Queue files. Per-repository overrides live in Queue's
  permanent `repository_settings` rows and use `repository.set`,
  `repository.get`, and `repositories`; fields include `closeout`,
  `validation`, `reviewer`, `qa`, `validationPolicy`, and `note`. Project
  membership is separate: `project.get` takes a key or structured repository
  identity, and `project.upsert` groups repositories. The operator CLI/API
  accepts an absolute existing checkout path or normalized `owner/name` origin
  for repository settings. The old settings-shaped
  `project.set` and string-shaped `project.get` calls are rejected. See
  [Queue Spec 022](https://github.com/inflatable-cookie/paseo-northstar-queue/blob/main/docs/knowledge/specs/022-queue-projects-without-manifests.md)
  and the [Queue RPC schema](https://github.com/inflatable-cookie/paseo-northstar-queue/blob/main/shared/contracts.ts).

Queue also owns task status, review, closeout, and outcomes. A repository's committed `[docs_policy.graph]` profile is the only authority for `effigy docs context`; neither the starter nor an installed skill is read at query time.

## Effigy commands

Use `effigy graph` for code ownership, `effigy docs context` for document authority, `effigy tasks` for selector inventory, `effigy doctor` for routing or health, and `effigy test --plan` when test shape matters. Run `effigy qa` before a PR. A consumer's QA command must work from a fresh checkout with its own dependencies.

The [agent adoption guide](047-agent-and-cross-repo-adoption.md) covers Effigy command usage. The repository's own `docs/knowledge/contracts/release.md` must describe its actual release steps, even if it does not release yet.

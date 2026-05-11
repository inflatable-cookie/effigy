# Effigy Docs

This is the docs front door: **goal-based links** into the guides (not the full
guide inventory—that lives in [`guides/README.md`](./guides/README.md)).

If you are new here, do not start by reading everything. Pick the job you are
trying to do, then follow one path.

**Need the binary first?** Install options (Homebrew, curl, `cargo install`) are
in the root [`README.md`](../README.md) under **Install**.

## Start Here

1. Use the root [`README.md`](../README.md) **Install** and **Start Fast** for
   the binary and the shortest first-run path.
2. Read [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
   for the first ten minutes.
3. Read [`guides/055-everyday-workflows.md`](./guides/055-everyday-workflows.md)
   when the basics are working and you want the normal day-to-day flow.

After that, choose one path below.

## Choose A Path

### I want to run tasks and get useful work done

**Start here (everyone):**
- [`guides/021-quick-start-and-command-cookbook.md`](./guides/021-quick-start-and-command-cookbook.md)
- [`guides/055-everyday-workflows.md`](./guides/055-everyday-workflows.md)

**Then go deeper:**
- [`guides/022-manifest-cookbook.md`](./guides/022-manifest-cookbook.md) — copy-paste patterns for `effigy.toml`
- [`guides/016-task-routing-precedence.md`](./guides/016-task-routing-precedence.md) — how task names resolve
- [`guides/023-troubleshooting-and-failure-recipes.md`](./guides/023-troubleshooting-and-failure-recipes.md) — fix common problems

### I want to run local dev environments with containers and services

Use this when a repo needs databases, caches, or language workspaces without
installing them directly on your machine.

**Start here:**
- [`guides/063-container-system-guide.md`](./guides/063-container-system-guide.md)
- [`guides/064-system-workspace-and-dev-contract.md`](./guides/064-system-workspace-and-dev-contract.md)

**Then go deeper:**
- [`guides/065-underlay-starter.md`](./guides/065-underlay-starter.md) — shipped bundle for Rust + Bun stacks
- [`guides/067-catalog-services-reference.md`](./guides/067-catalog-services-reference.md) — postgres, redis, and more
- [`guides/069-workspace-host-integration.md`](./guides/069-workspace-host-integration.md) — sibling repo mounts and Docker/Colima coexistence

**Use these when cleanup starts to matter:**
- [`guides/063-container-system-guide.md`](./guides/063-container-system-guide.md) — `container cache list/prune`, `container volume list/prune`
- [`guides/057-bootstrap-repo-bringup.md`](./guides/057-bootstrap-repo-bringup.md) — backend choice during repo bring-up

### I want to automate, integrate, or go deeper

**For CI and scripts:**
- [`guides/017-json-output-contracts.md`](./guides/017-json-output-contracts.md) — JSON output for automation
- [`guides/024-ci-and-automation-recipes.md`](./guides/024-ci-and-automation-recipes.md) — copy-paste CI workflows
- [`guides/050-env-schema-integration.md`](./guides/050-env-schema-integration.md) — `--env-schema` overrides and validation when tasks need typed env

**For demos and proofs:**
- [`guides/058-demo-system-guide.md`](./guides/058-demo-system-guide.md)

**For data artifacts, OCI, and bootstrap seed flows:**
- [`guides/072-artifact-commands-guide.md`](./guides/072-artifact-commands-guide.md)
- [`guides/057-bootstrap-repo-bringup.md`](./guides/057-bootstrap-repo-bringup.md)

**For release workflows:**
- [`guides/051-release-orchestration.md`](./guides/051-release-orchestration.md)
- [`guides/062-distribution-system-guide.md`](./guides/062-distribution-system-guide.md)

**For contributing to docs:**
- [`guides/037-documentation-contribution-playbook.md`](./guides/037-documentation-contribution-playbook.md)

## Other Areas

- Architecture: [`architecture/`](./architecture/)
- Contracts and JSON surfaces: [`contracts/README.md`](./contracts/README.md)
- Roadmaps: [`roadmaps/README.md`](./roadmaps/README.md)
- Research: [`research/README.md`](./research/README.md)
- Vision: [`vision/README.md`](./vision/README.md)
- Strict planning lane: [`specs/README.md`](./specs/README.md)

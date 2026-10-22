# 048 - Managed Bun Install Contract

Status: **retired in v0.15** (Q-009). Bootstrap no longer installs dependencies;
this document records what replaced the retired route and what still holds.

Owner: Bootstrap maintainers

Related: [020](020-remote-bundle-sources-git-and-oci-delivery-contract.md),
[047](047-bounded-doctor-and-scan-cache-contract.md),
[bootstrap guide 057](../../guides/057-bootstrap-repo-bringup.md),
[retired concepts](../retired.toml)

## What was retired

`effigy bootstrap deps sync`, its `--js-only`/`--rust-only`/`--refresh-lock`
flags, the `effigy.bootstrap.deps.v1` payload, the bootstrap-owned missing-lock
preflight, the Bun invocation and the Finder-metadata cleanup are gone. The
command refuses with the replacement, and the
`bootstrap-children-and-runtime-flags` entry in
[retired.toml](../retired.toml) names every retired spelling.

## What replaced it

Dependency preparation is a repo-owned task selector:

```toml
[bootstrap]
setup = ["workspace:js:prepare"]
start = "dev"
```

- `setup` holds ordinary task selectors, so the repo decides how to install
  (frozen lock, refresh path, pnpm, npm or something else) and how to clean up
  before or after. There is no bootstrap-owned package-manager policy.
- A lock refresh is a deliberate repo task, not a bootstrap flag.
- `effigy bootstrap` runs `setup` after it has acquired the checkout and its
  pinned sources, and reports the selector it ran.

## What still holds

- A built-in Bun test suite whose locked workspace has no `node_modules` fails
  with a bounded diagnostic before reporting source test failures, naming the
  repo's own dependency task (the one `[bootstrap].setup` selects). Suites with
  an explicit `bun install` setup step handle their own hydration.
- `effigy doctor --deep` reports the same gap as `health.task.bootstrap` with the
  observed install root, lock root and missing declared dependencies, and names
  the same repo-owned preparation (contract
  [047](047-bounded-doctor-and-scan-cache-contract.md)).
- Effigy never edits a sibling dependency checkout to make an install succeed.
  A failure stays an install failure of the task that ran it.

## Acceptance

- The retired `bootstrap deps sync` spellings refuse with a replacement and a
  changelog pointer.
- A missing local install is reported by `effigy doctor --deep` and by a
  selected built-in Bun suite, and both name the repo-owned dependency task.
- No bootstrap path runs a package manager.

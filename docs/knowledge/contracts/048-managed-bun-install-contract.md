# Managed Bun Install Contract

Owner: Bootstrap maintainers

## Purpose

Define reproducible Bun preparation through `effigy bootstrap deps sync`, an
explicit route to regenerate the text lockfile, and safe handling of Finder
metadata copied from local `file:` dependencies into the managed install.

## Bun Preparation

`bootstrap deps sync` selects Bun from `[package_manager].js` in the nearest
Effigy manifest.

- The normal path requires `bun.lock` or `bun.lockb` at the selected Bun
  workspace root and checks for it before invoking Bun.
- A missing lock is an actionable error that names the required refresh
  command. Effigy must not let Bun silently proceed from `package.json` with
  no lock.
- The normal path runs `bun install --frozen-lockfile`. This path does not
  update the committed lock.
- `--refresh-lock` is the explicit regeneration path. It is supported only
  for Bun and runs `bun install --save-text-lockfile`, which can create or
  update `bun.lock`.
- The operator reviews and commits the generated lock, then runs the normal
  frozen path.

Refreshing is a deliberate install action, not a fallback from frozen
preparation. It does not change `deps pin bun`: pinning edits the consumer
manifest only, and its install workflow remains owned by contract
[`040`](040-bun-committed-dependency-pinning-contract.md).

Before executing a selected built-in Bun test suite, Effigy checks whether its
locked workspace has `node_modules`. A fresh checkout without that directory
fails with a bounded diagnostic naming `effigy bootstrap deps sync <path>`.
Suites with an explicit `bun install` setup step handle their own hydration;
workspaces without a committed Bun lock do not receive this frozen route hint.

## Local File Dependency Metadata

After a successful managed Bun install, Effigy removes entries named
`.DS_Store`, prefixed `._`, or named `__MACOSX` from the selected workspace's
`node_modules` tree.

- Cleanup is limited to `node_modules` under the selected install root.
- Recursive cleanup does not follow symlinks. If `node_modules` itself is a
  symlink, Effigy leaves it alone.
- Source files in local dependency checkouts remain untouched.
- Other installed dependency files, including `package.json` and source
  files, remain intact.

Effigy does not edit a sibling dependency to make Bun install succeed. A Bun
failure remains an install failure; cleanup runs only after Bun succeeds.

## Ownership

- `effigy-cli` owns the `--refresh-lock` grammar and validation.
- The bootstrap runner owns missing-lock preflight, Bun invocation, and
  metadata cleanup.
- `effigy.bootstrap.deps.v1` stays stable; operation command text identifies
  the frozen or refresh invocation.

## Acceptance

- Missing lock fails before Bun starts with a diagnostic that names
  `--refresh-lock`.
- Refresh creates or updates `bun.lock`; a subsequent normal preparation uses
  `--frozen-lockfile`.
- A successful local `file:` dependency install leaves Finder metadata out of
  managed `node_modules` while preserving real package files and every source
  file.

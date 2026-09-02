# 077 - Local Dependency Linking

Use `effigy admin deps` when a consumer is pinned to a Git tag or published package
but you need it to resolve from a local library checkout while editing.

Choose the state contract first:

- `deps link` is ephemeral, machine-local, and invisible to Git.
- `deps pin bun` writes committed consumer `overrides` inherited by CI and
  teammates.

The committed manifest stays authoritative. Effigy creates machine-local
Cargo patches or save-less Bun symlinks, records the desired state, verifies
the complete matching closure, and removes only state it owns.

## Quick Path

Run these commands from the consumer repo:

```sh
effigy admin deps link cargo ../signal --dry-run
effigy admin deps link cargo ../signal
effigy admin deps status cargo

# edit ../signal and build or test the consumer

effigy admin deps unlink cargo ../signal --dry-run
effigy admin deps unlink cargo ../signal
```

Relative library paths resolve from the selected consumer repo. This remains
true when `--repo <PATH>` targets that repo from another working directory.
Absolute library paths are used unchanged.

Use `bun` instead of `cargo` for a Bun package library:

```sh
effigy admin deps link bun ../poodle --dry-run
effigy admin deps link bun ../poodle
effigy admin deps status bun

# edit ../poodle and build or test the consumer

effigy admin deps unlink bun ../poodle --dry-run
effigy admin deps unlink bun ../poodle
bun install
```

When the Bun graph crosses repository or `file:` boundaries, use a committed
pin instead:

```sh
effigy admin deps pin bun ../poodle --dry-run
effigy admin deps pin bun ../poodle
bun install

# edit ../poodle and build or type-check the consumer

effigy admin deps unpin bun ../poodle --dry-run
effigy admin deps unpin bun ../poodle
bun install
```

Pin and unpin edit only the root consumer `package.json`. Effigy reports
resolution as pending and leaves both installs and lockfile review to the
operator.

Bare `effigy admin deps` reports both managers. Add top-level JSON mode when another
tool or agent will consume the result:

```sh
effigy --json deps
effigy --json deps status cargo
effigy --json deps link bun ../poodle --dry-run
```

## What Stays Committed During Links

`Cargo.toml`, `package.json`, and their pinned dependency declarations remain
unchanged. Effigy never turns a local link into a manifest migration.

Machine-local state lives in:

| State | Purpose |
| --- | --- |
| `.effigy/local/dependency-links.json` | desired links for this consumer |
| `.cargo/config.toml` | Effigy-managed Cargo patch blocks |
| `~/.effigy/deps/bun-registrations.json` | Bun registration ownership and consumer references |
| Bun global link registry and consumer symlinks | ephemeral Bun resolution mechanism |

Effigy ensures `.effigy/` and an Effigy-created `.cargo/config.toml` are
ignored. It refuses a tracked Cargo config rather than hiding committed state.

Multiple libraries can be linked at once. Every operation is keyed by package
manager, consumer repo, and canonical library path, so unlink removes only the
selected link.

## Bun Committed Pin Workflow

`deps pin bun` inventories every named package in the local library and selects
the complete package-name closure already present in the consumer's Bun tree.
It then adds one relative `file:` override per match at the consumer root:

```json
{
  "overrides": {
    "@inflatable-cookie/poodle-core": "file:../poodle/packages/core",
    "@inflatable-cookie/poodle-svelte": "file:../poodle/packages/svelte/components"
  }
}
```

Pin normally reads the installed tree through `bun pm ls --all`. If Bun fails
to enumerate a valid text lockfile, pin can read that `bun.lock` as JSONC and
reports a `lockfile-enumeration-fallback` warning with the original Bun error.
Missing, malformed, or structurally unsafe lock data still refuses with no
manifest write. This fallback is pin-only: links continue to require the
process-resolved installed graph for their safety decisions.

The operation is atomic. An existing conflicting override, an overlapping
Effigy-managed link, a concurrent manifest edit, or a partial closure refuses
the whole write. Unrelated overrides, field order, indentation, final-newline
posture, and both Bun lockfile forms are preserved.

The checkout may sit outside the consumer repository, but the relative path is
only portable when teammates and CI reproduce that sibling layout. Pin emits a
warning for that case. It never writes an absolute path.

`deps unpin bun` removes only package/path pairs that exactly match the named
checkout. It does not restore hidden state or create a machine-local link.
After either manifest change, run `bun install` yourself and review the
lockfile separately.

The Soundcheck/Poodle acceptance proof covered a consumer with `file:` edges
through Soundcheck Library and Longhorn. One root pin redirected both Poodle
packages to one canonical checkout without touching either intermediate
repository. Physical linked-package contamination in Longhorn remained visible
through `deps status bun`; an override changes resolver policy, not the
underlying filesystem warning.

## Manifest Root Selection

Neither manager assumes the Git root is the package root.

Cargo library inventory anchors on the library's root `Cargo.toml` and takes
only that workspace's members. A repo often carries packages the root does not
list — self-contained prototype workspaces, or example packages with neither
membership nor their own `[workspace]` table. `cargo metadata` refuses on the
second kind, so walking every manifest failed the whole link. A library with no
root manifest still falls back to every workspace root in the tree.

Bun resolves the consumer root the same way. A root `package.json` owns the
tree. Without one, every manifest that has no package-root ancestor is an
independent Bun root — `harness/` and `apps/studio/` sit at different depths
and neither owns the other, so both are roots, while anything under a root is
that root's workspace member. The library then names the right one: the root
declaring a library package wins. When none or several do, Effigy refuses and
lists the candidates rather than picking a tree the caller did not name —
select one with `--repo <PATH>`.

A vendored clone carrying its own `.git` is an independent checkout. Discovery
stops at that boundary and planning refuses a consumer root inside one, so a
parent-level invocation never runs Bun or changes `node_modules` in a checkout
that owns its own link state.

Links are keyed by the resolved Bun package root, so a repo with Bun under
`studio/` records `studio/` as its consumer root. That key selects manifests,
`node_modules`, and every `bun` invocation. Machine-local state — the link
ledger, `.gitignore`, and link backups — belongs to the enclosing checkout, so
`effigy admin deps status` reports those links from the repo root as usual and unlink
removes the ledger entry it wrote.

Both identities are resolved from whichever path you name, so
`effigy admin deps link bun ../../longhorn --repo studio` and a bare
`effigy admin deps link bun ../longhorn` from the repo root produce the same link.
Relative library paths still resolve from the path you passed.

Every command that reads link state resolves the same single ledger, so
`deps status`, `deps pin`, and `doctor` see a nested-root link from either
entry point. In particular `deps pin bun --repo studio` still refuses while a
link overlaps, instead of reading an empty nested store and writing overrides
over it. A vendored checkout with its own `.git` owns its own state and is
never claimed by its parent.

## Cargo Workflow

Cargo mode inventories the library and every real consumer workspace. It
matches direct and transitive crates by exact Git source URL, then writes the
full matching closure into one repo-root `.cargo/config.toml`:

```sh
effigy admin deps link cargo ../signal --dry-run
effigy admin deps link cargo ../signal
```

The config uses canonical absolute paths. One repo-root config therefore also
covers nested workspaces without per-workspace relative paths. Effigy preserves
unrelated config and comments. A compatible pre-Effigy patch is migrated into
the managed block by `link`, or can be removed directly by `unlink`. Adoption
requires the source table to contain only crates from the requested library and
every path to resolve into that checkout; mixed or mismatched tables are
refused.

Cargo verification or a consumer build can rewrite affected `Cargo.lock`
entries while the patch is active. That is expected local state, but it is a
do-not-commit condition:

```sh
effigy admin deps status cargo
effigy doctor
```

Both commands are read-only. They report active linked lock resolution as an
error with the affected packages and remediation. Before a first link, Effigy
also refuses a dirty affected lockfile so it cannot confuse unrelated changes
with link-owned resolution.

Unlink through Effigy:

```sh
effigy admin deps unlink cargo ../signal --dry-run
effigy admin deps unlink cargo ../signal
```

Unlink re-resolves the committed Git/tag source and proves the affected lock
returns to its tracked baseline, or contains only another active link's owned
drift. It does not run `git checkout`, `git restore`, or discard unrelated
changes.

## Bun Workflow

Bun mode inventories the library's root/workspace packages and matches the
direct and transitive packages present in the consumer tree. It registers each
package, then links the full closure into the consumer in one operation:

```sh
effigy admin deps link bun ../poodle --dry-run
effigy admin deps link bun ../poodle
```

Every Bun process intent contains explicit `--no-save`. Effigy snapshots
`package.json`, `bun.lock`, and `bun.lockb`; any byte change fails the
operation. It never uses `--save`.

`bun install` can replace some or all consumer symlinks. Inspect and repair the
desired link rather than editing the manifest:

```sh
bun install
effigy admin deps status bun
effigy admin deps link bun ../poodle
```

Complete link loss is repairable drift. A mixed local/registry closure is an
error because duplicate shared types can result. Managed re-link restores
either shape idempotently. An unmanaged partial closure is rejected because
Effigy has no desired-state ownership proof.

When the duplicate closure crosses `file:` dependencies or repository
boundaries, consumer-side Bun links cannot redirect every transitive copy.
Effigy refuses the link and prints a paste-ready `overrides` block covering
every matched package from the local library. Merge that block into the
consumer `package.json`, then run `bun install`. Overrides are committed
resolver policy inherited by CI and teammates; `deps link bun` remains
machine-local, save-less, and never writes them.

Raw-source links can resolve a framework peer from two physical trees (consumer
hoist vs library `node_modules` / `.bun`). Same peer version is treated as
shared and does not fail the link. Mismatched peer versions are duplicate
errors — status and doctor report both paths and versions. Align the library
peer (or remove the mismatched local copy) so both resolve to one compatible
version.

Unlink removes only consumer symlinks that still target the expected library.
It unregisters a package only when Effigy created the registration, the target
still matches, and no other desired consumer remains:

```sh
effigy admin deps unlink bun ../poodle --dry-run
effigy admin deps unlink bun ../poodle
bun install
```

The final install rematerializes the consumer's published dependency tree. A
foreign, shared, stale, or unverifiable registration is retained and reported.

## Status And Doctor

Use status for dependency-specific desired/observed state:

```sh
effigy admin deps
effigy admin deps status cargo
effigy admin deps status bun
```

Bun status also inspects cross-repository `file:` dependencies. If the target
repository exposes a package symlink from its own `node_modules` into another
checkout, status names the `file:` dependency, linked package, symlink, and
resolved target. Bun's internal store links and links that stay inside the
target repository are ignored. The check is read-only.

Status also reports the local dependencies a repo already declares in committed
manifests. A Cargo `path` dependency or a Bun `file:`/`link:` specifier that
resolves into a sibling checkout is the local link already in force, so status
groups those declarations by library checkout and reports each group with
`committed_local` set and `desired` absent:

```text
[cargo] healthy: /Users/you/dev/longhorn
desired: committed
mechanism: cargo-path-dependency
consumers: /Users/you/dev/figmatic/crates/figmatic-studio
packages: longhorn-core, longhorn-windowing
- [information] cargo-committed-path-local: 2 committed Cargo path dependencies resolve `/Users/you/dev/longhorn` locally
```

These are observations, not links Effigy owns. `deps link` still refuses to
adopt them — a Cargo `[patch]` cannot redirect a path dependency, and a
committed Bun override outranks an ephemeral link — and `deps unlink` has
nothing to remove.

"Outside" means outside the checkout, not outside the directory you pointed
status at, so running from `crates/app` or `studio/` does not turn the repo's
own crates and packages into committed locals. A nested checkout's own
declarations belong to it, not to its parent. Declarations that do not resolve
are left to the package manager.

Status reports the target actually in force, not every declaration. A root
`overrides` or `resolutions` entry replaces the package's resolved target for
the whole install, so it supersedes the declared dependency — and an override
back to a registry version means there is no local dependency to report. Cargo
is different: two renamed path dependencies of the same package resolve to two
directories at once, and both are reported.

Use doctor when dependency health belongs in the repo-wide health report:

```sh
effigy doctor
effigy --json doctor
```

Key states:

| Observation | Result | Action |
| --- | --- | --- |
| full closure resolves locally | healthy/info | no action |
| committed path or `file:` local in force | healthy/info | no action; `deps link` correctly refuses it |
| Cargo lock contains linked path resolution | error | do not commit; unlink before handoff |
| complete Bun symlink loss | warning | re-run the same Bun link command |
| partial Bun closure | error | re-link when Effigy desired state exists |
| `file:` dependency exposes an external package link | warning | unlink it in the target repo or add a consumer override, then install |
| same-version Bun peer paths across repos | healthy/info | no action |
| mismatched Bun peer versions | error | align or remove the local peer copy |
| library checkout missing | error | restore the checkout or unlink using the recorded path |
| compatible pre-Effigy Cargo patch | migration | link to adopt it, or unlink to remove it directly |
| mixed/mismatched Cargo patch or foreign Bun registration | error | resolve ownership manually; Effigy will not overwrite it |

Status, doctor, and `--dry-run` never mutate manager state or lockfiles.

## Error Boundaries

- No matching dependency: the consumer does not resolve a package from that
  library. If it still uses path dependencies, migrate it to pinned Git or
  published sources first.
- Tracked `.cargo/config.toml`: untrack or relocate hand-managed policy before
  asking Effigy to own a machine-local patch.
- Dirty affected Cargo lock: finish or revert unrelated work explicitly, then
  retry. Effigy will not restore it through Git.
- Mixed or mismatched Cargo patch: split unrelated crates into another source
  table where possible, or resolve the conflicting path manually. Effigy claims
  only a table that points exclusively into the requested library.
- Conflicting Bun registration: inspect Bun's global link registration and
  resolve the foreign path; Effigy will not replace or claim it.
- Mixed Bun closure across `file:` or repository boundaries: merge the
  reported `overrides` block into the consumer manifest and run `bun install`.
  Effigy reports the committed mechanism but does not apply it through
  machine-local `deps link`.
- Already unlinked: unlink succeeds as a no-op when neither desired state nor a
  compatible legacy patch is present.

## Automation Contract

All operations use the standard `effigy.command.v1` envelope under global
`--json`:

- status: `effigy.deps.status.v1`
- link: `effigy.deps.link.v1`
- unlink: `effigy.deps.unlink.v1`
- pin and unpin: `effigy.deps.pin.v1`

Read `result` on success. Doctor failures carry the same dependency findings
under `error.details`. See
[`026-json-payload-examples.md`](./026-json-payload-examples.md) for payload
examples and
[`034-local-dependency-linking-contract.md`](../contracts/034-local-dependency-linking-contract.md)
for normative behavior.

## Proof State

Cargo behavior is proven against real flat and nested portfolio consumers.
Bun link behavior is proven with real Bun commands and registry-shaped
fixtures. Bun committed pinning is also proven in disposable clones of the
Soundcheck, Soundcheck Library, Longhorn, and Poodle repositories.

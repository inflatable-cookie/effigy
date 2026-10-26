# External Bundle Adoption

This guide covers consuming external Effigy bundle repos from a core Effigy
checkout. Core Effigy no longer ships product-specific starters for app
families such as Purpose-built external bundles or PHP app.

## Bundle Workflow

Use typed bundle sources in the consuming repo:

```toml
[bundle]
base = { type = "git", url = "git@github.com:example/acme-effigy-bundle.git", ref = "main" }
host = "acme.test"
project_name = "acme-dev"
```

Supported sources:

- `base = { type = "path", dir = "bundles/acme" }`
- `base = { type = "git", url = "...", ref = "main" }`
- `base = { type = "oci", url = "ghcr.io/acme/effigy-bundle:v1" }`

Legacy string bundle names are intentionally gone:

- `base = "workspace-app"`
- `base = "php-app"`

## Commands

```sh
effigy bundle inspect
effigy bundle sync
effigy config --inspect
effigy tasks
effigy doctor --verbose
```

`bundle inspect` reports the active source and materialized bundle root.

You never have to run `effigy bundle sync` in normal work. `effigy dev`,
`effigy bootstrap` and `effigy container up` update a stale source themselves,
before they compose the manifest from it, and say so in one line:

```text
[bundle] bundle updated 544c0cc9 → 7111bf60 (upstream moved)
```

An ordinary task run in a checkout that has never pinned the source acquires it
first, so a fresh worktree or clone runs its tasks straight away. It never moves
a pin: refreshing a stale source stays on the Prepare paths above.

`bundle sync` stays as the explicit command, and it is the only updater a source
declared `update = "manual"` has. It acquires and validates a new generation
before atomically selecting it in `.effigy/sources.lock.json`. A failed
acquisition keeps the previous bytes.

What a Prepare path updates:

- a changed declaration (url, ref or path) is re-acquired immediately;
- a path bundle is re-snapshotted when its content changed, so editing a local
  bundle is enough - `target/`, `node_modules/` and `.git/` are skipped;
- a floating Git source (no `ref`, or a branch or tag) is checked with one
  bounded `git ls-remote` at most once per checkout per 10 minutes, and the
  new revision is acquired when upstream moved;
- a full commit id in `ref`, an OCI digest and a tag still at its pinned commit
  never move;
- an offline or failed update keeps the current pins and warns once; only a
  changed declaration with no usable pin fails the command.

Help, version, completion, tasks, plans, status, config views, doctor and bundle
inspection read paths never fetch, check or write source state. When a source is
behind they name the locked and the available revision and say the next
`effigy dev` or `effigy container up` will update it.

Existing v0.14.1 cache bytes are imported automatically at the first Prepare.
Declared `[catalog.sources.<name>]` use the same typed grammar and checkout lock
and accept their own `update = "manual"`; see
[the source contract](../knowledge/contracts/020-remote-bundle-sources-git-and-oci-delivery-contract.md).

## Starter Boundary

`effigy init` only lists starters embedded in the core catalog. Product-specific
starter manifests should live with their external bundle repo and be copied or
bootstrapped by that repo's own onboarding path.

Core Effigy owns typed bundle loading, bundle source materialization, manifest
composition, task routing, and container/runtime primitives.

External bundle repos own product-specific starter content, bundle defaults,
exported manifest fragments, Rhai helpers, and onboarding docs.

## Adoption Checklist

1. Add a typed `[bundle].base` source.
2. Run `effigy dev` (which prepares the source) or `effigy bundle sync`, then
   `effigy bundle inspect`.
3. Run `effigy config --inspect` and check the composed manifest.
4. Run `effigy tasks` and `effigy doctor --verbose`.
5. Keep product-specific starter or migration checklists in the external
   bundle repo, not in core Effigy docs.

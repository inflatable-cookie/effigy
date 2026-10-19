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
`bundle sync` is the only updater for Git, OCI and path source pins. It acquires
and validates a new generation before atomically selecting it in
`.effigy/sources.lock.json`. A failed acquisition keeps the previous bytes.

Help, version, completion, tasks, plans, status, config views, doctor and graph
read paths never fetch or write source state. Before a remote source is prepared,
run `effigy bundle sync`, bootstrap, or the first `effigy dev`. Later dev runs
keep the same pin. A changed declaration or a version learned by another explicit
sync can produce `bundle behind: run `effigy bundle sync``; reads never probe
the remote to check.

Existing v0.14.1 cache bytes are imported automatically at the first Prepare.
For local bundle development, run `effigy bundle sync` after editing the source.
Declared `[catalog.sources.<name>]` use the same typed grammar and checkout lock;
see [the source contract](../knowledge/contracts/020-remote-bundle-sources-git-and-oci-delivery-contract.md).

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
2. Run `effigy bundle sync`, then `effigy bundle inspect`.
3. Run `effigy config --inspect` and check the composed manifest.
4. Run `effigy tasks` and `effigy doctor --verbose`.
5. Keep product-specific starter or migration checklists in the external
   bundle repo, not in core Effigy docs.

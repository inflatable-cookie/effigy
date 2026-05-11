# Local Manifest Bundles

Local manifest bundles are now the only bundle format Effigy supports.
The old compiled-in shipped bundle catalog is gone.

Use one of these typed source forms:

```toml
[bundle]
base = { type = "path", dir = "bundles/acme" }
```

```toml
[bundle]
base = { type = "git", url = "git@github.com:org/acme-bundle.git", ref = "main" }
```

```toml
[bundle]
base = { type = "oci", url = "ghcr.io/org/acme-bundle:v1" }
```

Legacy forms are removed:

- `base = "underlay"`
- `[bundle].name`
- `[bundle].base_path`

## Directory Contract

A local bundle directory contains:

```text
bundles/acme/
├── bundle.toml
├── export.toml
└── scripts/...
```

- `bundle.toml` declares bundle metadata and inputs
- `export.toml` is the defaults template rendered before merge
- optional assets and scripts live under the bundle root and can be referenced
  with `{{ bundle.root }}`

Paths in `base = { type = "path", dir = "..." }` resolve relative to the
consuming `effigy.toml` unless absolute.

## `bundle.toml`

```toml
[bundle]
name = "acme"
description = "Acme local dev stack."
defaults = "export.toml"

[[inputs]]
name = "host"
type = "string"
required = true
description = "Primary local hostname."
example = "acme.test"
```

Supported input types:

- `string`
- `integer`
- `bool`
- `list`

Every key under `[bundle]` other than `base` must be declared in
`bundle.toml`. Misspelled inputs fail manifest loading.

## Inspect and Sync

Use:

```sh
effigy bundle inspect
effigy bundle sync
```

- `bundle inspect` reports the active source type, local materialized path,
  version hint, and stale state
- `bundle sync` refreshes git and OCI sources
- local path bundles report `not-applicable` on sync

## Template Notes

`export.toml` is a Minijinja template. Resolved inputs are available under
`inputs`.

Bundle-owned assets are available under `{{ bundle.root }}`.

That keeps repo manifests small while still letting the bundle ship helper
scripts, env templates, and other glue beside the defaults template.

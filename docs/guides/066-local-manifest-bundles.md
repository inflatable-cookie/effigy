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

- `base = "workspace-app"`
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

## Override Semantics

A repo manifest wins wherever it sets a value; the bundle only fills what the
manifest leaves unset. Ordinary tables merge key by key, so a repo manifest can
add `[secrets.keys.*]` or one container route without restating the bundle.

Step-shaped tables are different. A table that names an executor (`run`,
`task`, `draft` or `rhai`) is a task step: a vault generator
(`[secrets.vault].generate`), a deploy hook, a bootstrap entry, or the
single-object `run = { ... }` shorthand. Naming an executor there replaces the
bundle's whole executor group, so this override is valid:

```toml
# bundle default: generate = { rhai = "generate-dev-secrets.rhai", run_in = "host" }
[secrets.vault]
generate = { task = "secrets generate" }
```

The resolved step is `{ task = "secrets generate", run_in = "host" }`: the
`rhai` executor is gone, and unset step metadata (`run_in`, `env`, `env_file`,
`id`, `timeout_ms`) still defaults from the bundle. Metadata the manifest sets
always wins. Composition never rewrites a manifest's own keys, so a step that
names two executors stays malformed and the exactly-one-executor validation
refuses it instead of one executor silently disappearing.

Bundle defaults in `export.toml` should therefore keep executors and step
metadata in the same table, and repo overrides should switch executors by
naming the new executor rather than deleting the old one.

## Inspect and Sync

Use:

```sh
effigy bundle inspect
effigy bundle sync
```

- `bundle inspect` reports the active source type, local materialized path,
  version hint, and stale state
- `bundle sync` snapshots path, Git and OCI sources and updates checkout pins
- after editing a local path bundle, run `bundle sync` to select the new bytes
- inspection and discovery never fetch or write source state

## Template Notes

`export.toml` is a Minijinja template. Resolved inputs are available under
`inputs`.

Bundle-owned assets are available under `{{ bundle.root }}`.

That keeps repo manifests small while still letting the bundle ship helper
scripts, env templates, and other glue beside the defaults template.

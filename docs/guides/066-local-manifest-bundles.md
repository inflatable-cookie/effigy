# Local Manifest Bundles

Local manifest bundles let a repo reference reusable Effigy defaults in
a directory instead of copying large `[systems]`, `[containers]`, or
task blocks into every consumer manifest.

Use shipped bundles with `[bundle].base`. Use local bundles with
`[bundle].base_path`.

```toml
[bundle]
base_path = "bundles/acme"
host = "acme.test"
project_name = "acme-dev"
```

`base` and `base_path` are mutually exclusive. `name` remains accepted
as a legacy alias for `base`, but new manifests should use `base`.

## Export a Shipped Bundle

Use `effigy bundle export` when a shipped bundle is close but needs local
ownership:

```sh
effigy bundle export underlay --path bundles/underlay
```

The export writes a local-bundle directory containing `bundle.toml`,
`effigy.toml`, any bundle-owned assets, and a short README. It refuses to
overwrite existing files.

Switch the consuming manifest from `base` to `base_path`:

```toml
[bundle]
base_path = "bundles/underlay"
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "underlay-reference"
database = "acme"
```

After that, edits in `bundles/underlay/` are repo-owned. They no longer
track updates to the compiled-in shipped bundle unless you export again
to a separate directory and merge the diff.

## Directory Contract

A local bundle directory contains:

```text
bundles/acme/
├── bundle.toml   # metadata and input schema
└── effigy.toml   # defaults template rendered before merge
```

The defaults file name is `effigy.toml` by default. Override it with
`[bundle].defaults` inside `bundle.toml` when needed.

Paths in `base_path` are resolved relative to the consuming
`effigy.toml`, unless absolute.

## `bundle.toml`

```toml
[bundle]
name = "acme"
description = "Acme local dev stack."

[[inputs]]
name = "host"
type = "string"
required = true
description = "Primary local hostname."
example = "acme.test"

[[inputs]]
name = "project_name"
type = "string"
required = true
description = "Compose project name."
example = "acme-dev"

[[inputs]]
name = "api_port"
type = "integer"
default = 41001
description = "API dev-server port."
```

Supported input types:

- `string`
- `integer`
- `bool`
- `list`

Input names must not collide with reserved `[bundle]` selector keys:
`base`, `base_path`, or legacy `name`.

Local bundles are strict. Every key under `[bundle]` other than
`base_path` must be declared in `bundle.toml`; misspelled inputs fail
manifest loading.

## Defaults Template

The bundle's `effigy.toml` is a Minijinja template. Resolved inputs are
available under `inputs`.

The bundle directory is available as `bundle.root`. Use it for
bundle-owned Rhai scripts, compose files, Dockerfiles, or other assets.
Effigy renders it as an absolute path, so external bundles can stay in
one place and consumers pick up source updates without copying assets
into the repo.

```toml
[containers]
default = "stack"

[containers.stack]
startup = "detached"
project_name = "{{ inputs.project_name }}"
primary_service = "workspace"

[containers.stack.dns]
routes = [
  { domain = "{{ inputs.host }}", tls = true, port = {{ inputs.api_port }}, service = "workspace" },
]

[tasks.dev]
run = "cargo run -- --host {{ inputs.host }} --port {{ inputs.api_port }}"

[tasks.setup]
run = [{ rhai = "{{ bundle.root }}/scripts/setup.rhai" }]
```

After rendering, Effigy parses the result as normal `effigy.toml`
content.

For shipped bundles compiled into the Effigy binary, `bundle.root` points
at a content-addressed materialized asset directory under
`.effigy/runtime/bundles/<bundle>/<hash>/`. Effigy refreshes that
directory when the embedded asset contents change.

Repo-owned run steps may also reference the active bundle root with
`{{ bundle.root }}`:

```toml
[tasks.dev]
run = [{ rhai = "{{ bundle.root }}/scripts/setup.rhai" }]
```

## Merge Precedence

Bundle defaults are lowest precedence.

1. Effigy composes the root manifest and any `[manifest].include`
   fragments.
2. Effigy renders and parses the selected bundle defaults.
3. Missing values are filled from the bundle.
4. Values already owned by the repo manifest win.

This means a consumer can override a bundle-provided path directly:

```toml
[bundle]
base_path = "bundles/acme"
host = "acme.test"
project_name = "acme-dev"

[containers.stack]
primary_service = "api"
```

## Validation

Use these commands while authoring a bundle:

```bash
effigy config --inspect
effigy config --inspect --path containers.stack
effigy tasks
effigy container status
```

`effigy config --inspect` shows local bundle-populated paths with the
bundle defaults file as the source.

`effigy bundle list`, `effigy bundle inspect <name>`, and
`effigy bundle export <name> --path <dir>` operate on shipped bundles.
Local bundles are repo-owned files, so inspect them through the consuming
manifest with `effigy config --inspect`.

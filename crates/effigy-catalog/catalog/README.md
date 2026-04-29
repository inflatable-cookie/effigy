# Catalog Source Layout

This directory is the source of truth for the shipped service catalog.

Each service lives in its own directory and can own:

- `service.toml`
  - parameter schema
  - capabilities
  - named volumes
  - default ports
  - optional service dependencies
- `compose.fragment.yml`
  - the rendered compose template
- `Dockerfile`
  - only when the catalog builds a custom image
- `configs/*.conf`
  - optional named config variants
- `variants/*.toml`
  - optional named parameter presets

The catalog is still compiled into a single binary. `effigy-catalog` embeds
this directory with `rust-embed` and loads the same files from disk when a
project-local or user-global override exists.

## What Still Lives In Rust

Most catalog shape now lives here. The remaining Rust-side behavior is narrower:

- `crates/effigy-catalog/src/assembly.rs`
  - default layering
  - variant preset merge order
  - `database` / `databases` normalization for database catalogs
- `crates/effigy-catalog/src/template.rs`
  - template rendering context and validation
- `crates/effigy-containers/src/workspace.rs`
  - host integration policy around mounts, SSH agent wiring, mkcert mounts,
    and isolation rewrites
- `crates/effigy-containers/src/lib.rs`
  - a few container-policy helpers

If a behavior is specific to one service and can be expressed as metadata, it
should live in `service.toml` instead of a Rust-side catalog-name switch.

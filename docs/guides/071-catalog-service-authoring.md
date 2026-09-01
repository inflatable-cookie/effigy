# 071 - Catalog Service Authoring

Use this guide when you want to add or change a shipped service catalog under
`crates/effigy-catalog/catalog/`.

This is the authoring guide. For the consumer-facing reference, use
[`067-catalog-services-reference.md`](./067-catalog-services-reference.md).

## What A Catalog Owns

Each catalog service lives in its own directory:

```text
crates/effigy-catalog/catalog/<name>/
├── service.toml
├── compose.fragment.yml
├── Dockerfile                # optional
├── configs/*.conf            # optional
└── variants/*.toml           # optional
```

Effigy embeds these files into the binary, but they are still the source of
truth. Change the files, not Rust string literals.

## What Goes Where

### `service.toml`

Use `service.toml` for the stable metadata contract:

- service name and description
- parameter schema and defaults
- exec/shell capability
- workspace host-integration flags
- mkcert trust-install flag
- loopback alias label and port
- shared-service eligibility
- shared-service host/port env var shims
- named volume declarations
- default ports
- optional service dependencies

Good fit:

- “this service publishes a TCP alias as `postgres.<host>` on 5432”
- “this service is allowed on the bounded `shared = true` path”
- “this workspace service supports host git/ssh integration”

Bad fit:

- merge order rules
- template rendering logic
- path expansion rules
- runtime rewrite algorithms

Those belong in Rust.

### `compose.fragment.yml`

Use this for the compose shape of the service itself:

- image
- build
- command
- environment
- healthcheck
- ports
- depends_on
- mounted config files

Keep it narrow. If the template starts carrying policy for unrelated runtime
behavior, that policy probably belongs in `service.toml` or Rust.

### `Dockerfile`

Only add a `Dockerfile` when the service truly needs a custom image.

Use it for:

- workspace images
- PHP images with extension install
- entrypoint hooks that the base image does not provide

Do not add a custom Dockerfile when a stock image plus compose config is
enough.

### `configs/*.conf`

Use `configs/` for named file variants that get rendered into the output.

Current example:

- `nginx/configs/*.conf`

Use this when the service needs a concrete config file, not just more env vars.

### `variants/*.toml`

Use `variants/` for named parameter preset bundles.

Good fit:

- a variant switches several parameters together
- the service keeps the same underlying compose template

Bad fit:

- variants that really need different config files
- variants that are only hiding one boolean toggle

If the behavior is mostly file-level, prefer `configs/`.

## Decision Rules

When you add behavior, ask these in order:

1. Is this stable metadata about the service?
   Put it in `service.toml`.
2. Is this concrete service runtime shape?
   Put it in `compose.fragment.yml`.
3. Does the image itself need new software or startup behavior?
   Put it in `Dockerfile`.
4. Is this a named preset of existing params?
   Put it in `variants/*.toml`.
5. Is this engine behavior across many services?
   Keep it in Rust.

## What Still Lives In Rust

Some behavior should stay code-owned:

- fragment loading and layered override resolution
- param validation and type checking
- variant merge order
- database/database list normalization
- template rendering context
- workspace mount rewrites
- host SSH / git / mkcert mount policy
- shared-service compose generation

If a rule is generic engine behavior rather than service metadata, keep it out
of the catalog files.

## Common Patterns

### Add a new stable capability flag

1. Add the field to [`crates/effigy-catalog/src/schema.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-catalog/src/schema.rs:1).
2. Set it in the affected `service.toml` files.
3. Replace any Rust-side catalog-name switch with resolver-backed metadata.
4. Add tests for both:
   - schema/fragment loading
   - the consuming runtime path

### Add a new parameter

1. Declare it under `[params.<name>]` in `service.toml`.
2. Use it from `compose.fragment.yml`.
3. Add or update an integration test that assembles the fragment.

### Add a new catalog

1. Create the directory under `crates/effigy-catalog/catalog/<name>/`.
2. Add `service.toml`.
3. Add `compose.fragment.yml`.
4. Add `Dockerfile` only if needed.
5. Add integration coverage in `crates/effigy-catalog/tests/integration/`.
6. Add or update consumer docs in [`067-catalog-services-reference.md`](./067-catalog-services-reference.md).

## Validation

Useful checks while authoring:

```sh
cargo test -p effigy-catalog --test integration -- --nocapture
cargo test -p effigy-containers --lib -- --nocapture
```

For focused fragment checks:

```sh
cargo test -p effigy-catalog --test integration resolve_php_fpm_fragment -- --nocapture
cargo test -p effigy-catalog --test integration resolve_workspace_rust_bun_fragment -- --nocapture
```

## Packaging Fragments For Distribution

The same fragment layout is what an independently versioned catalog pack
carries. Add a `pack.toml` at the root of a directory of fragments, then
install it with `effigy service pack install --path <DIR>` to test it against a
real resolver. Layer order, precedence, and recovery are documented in
[`067-catalog-services-reference.md`](./067-catalog-services-reference.md).

Nothing about fragment authoring changes inside a pack: `service.toml` schema,
variant merge order, and template rendering stay owned by
`crates/effigy-catalog`. A pack that declares an unknown fragment shape fails
validation before it can be activated.

## Related Guides

- Consumer reference: [`067-catalog-services-reference.md`](./067-catalog-services-reference.md)
- Local dev systems and containers: [`063-container-system-guide.md`](./063-container-system-guide.md)
- External bundle adoption: [`065-external-bundle-adoption.md`](./065-external-bundle-adoption.md)

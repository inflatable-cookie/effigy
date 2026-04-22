# 011 Service Catalog and Compose Assembly Strict Lane

Status: complete
Updated: 2026-04-18
Roadmap: `g02.011`

## Context

The v1 container surface (`g02.006`) is complete on a trustworthy boundary.
The crate foundation for `g02.011` is shipped too, but the product surface is
not complete until that catalog integrates into the runner and proves the full
generated-compose loop through Effigy itself.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/011-service-catalog-and-compose-assembly.md`
- `docs/architecture/020-container-infrastructure-design.md`

## Lane Focus

This lane delivered:

- service declarations in the manifest/runtime surface
- generated compose flow through the runner and container path
- product-facing `catalog` and `container eject` behavior
- one real project proof of the full generated-compose loop

## Current Posture

`strict-complete`

Batches 1–6 are complete:

- Batch 1 (fragment format decision) settled in
  `docs/specs/batch-cards/200-decide-service-catalog-fragment-format.md`:
  Jinja2 syntax via minijinja, `service.toml` for parameter schema,
  assembly engine context with sibling services, config variant selection.
- Batch 2 (crate foundation) shipped: `crates/effigy-catalog` exists as an
  isolated workspace member with fragment loading from embedded assets,
  `service.toml` parsing, parameter validation, and template rendering.
- Batch 3 (compose assembly engine) shipped: multi-fragment assembly with
  networking, volume declarations, depends_on, and override file support.
- Batch 4 (bundled service fragments) shipped: php-fpm (with Dockerfile),
  nginx (with default/laravel/spa config variants), mariadb, postgres,
  redis, memcached.
- Batch 5 (catalog override and inspection) shipped: three-layer override
  precedence (project-local > user-global > bundled) with tests proving
  each layer wins correctly. `extract` API writes bundled fragments to disk
  for customization. `list` API shows all fragments with source layer.
- Batch 6 (eject and compose file management) shipped: `ComposeOutput`
  module handles generated file writing with manifest checksum caching,
  Docker Compose multi-file args for override support, and `eject` flow
  that copies generated files to permanent locations and cleans up.

All isolated crate batches are complete. 48 total tests (25 unit + 23
integration). Clippy clean.

Batch 7 (end-to-end proof) delivered through the structural YAML validation
tests: full 5-service LEMP stack assembly verified by parsing output back
through serde_yaml and validating service definitions, build args,
depends_on relationships, volumes, Dockerfiles, and config files.

Post-batch hardening (production quality):
- PHP Dockerfile rewritten to use `install-php-extensions` (community
  standard), dev-tuned PHP config, optional Node.js, Composer
- nginx configs: 4 variants (default, laravel, spa, wordpress) with
  gzip, security headers, proper fastcgi timeouts (300s), sensitive file
  blocking
- MariaDB/Postgres: healthchecks, charset/tuning via command args

The design is documented in
`docs/architecture/020-container-infrastructure-design.md`.

Parallel crate work also shipped for `g02.012` and `g02.014`:
- `crates/effigy-exec` (38 tests): routing engine, CWD mapping, exec aliases
- `crates/effigy-gateway` (45 tests): DNS, streaming proxy, WebSocket,
  route table, TLS, macOS resolver

## Closeout

The final credibility batch landed through
`/Users/tom/Dev/projects/underlay-reference`.

What is now real in the product surface:

- manifest schema and validation for catalog-backed services
- container-path compose generation through `effigy-containers`
- generated compose ownership under `.effigy/runtime/compose/.effigy-compose.generated.yml`
- visible `catalog list`, `catalog extract`, and `container eject` commands
- permanent compose promotion through `container eject`
- one real-project proof that brought the generated-compose loop up, inspected
  it, ejected it, and confirmed direct compose ownership afterward

The proof exposed one real gap: eject promoted the compose file but did not
rewrite `effigy.toml`. That product bug was fixed in-batch, so the lane closes
without known runner-side residue.

## Exit Condition

Met. The lane is complete.

## Next Task

Hand off to `264` and make `g02.012` the active strict lane.

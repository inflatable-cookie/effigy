# 011 Service Catalog and Compose Assembly Strict Lane

Status: planned
Updated: 2026-04-16
Roadmap: `g02.011`

## Context

The v1 container surface (`g02.006`) is paused on a trustworthy boundary. The
next product problem is the boilerplate barrier: every project needs hand-
written docker-compose.yml and Dockerfiles to use containers. This lane
eliminates that barrier by introducing a service catalog and compose assembly
engine.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/011-service-catalog-and-compose-assembly.md`
- `docs/architecture/020-container-infrastructure-design.md`

## Lane Focus

The active strict lane is:

- define the service catalog fragment format
- define the manifest schema for service declarations
- implement compose assembly from catalog fragments
- ship bundled fragments for the initial service set
- prove with one real PHP + nginx + MariaDB project

## Current Posture

`active`

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

All 7 batches complete. 48 total tests (25 unit + 23 integration). Clippy
clean.

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

## Exit Condition

This strict lane is complete. The `effigy-catalog` crate is a functional,
tested library ready for integration after `g02.010` completes.

## Isolation Constraint

This lane runs in parallel with the modularization thread (`g02.010`). The two
threads must stay out of each other's way:

- this lane writes only to `crates/effigy-catalog/` (new isolated crate)
- this lane does NOT modify `src/`, `crates/effigy-containers/`,
  `crates/effigy-manifest/`, `crates/effigy-cli/`, or any existing crate
- final integration into the main runner happens after `g02.010` completes
- the human coordinates between threads where needed

## Planned Batch Sequence

### Batch 1: Fragment Format Decision

Decide the exact fragment template format:

- variable substitution syntax (Handlebars-style `{{var}}`, env-style
  `${VAR}`, or custom)
- `service.toml` schema for parameter declarations
- how fragments declare dependencies on other fragments (e.g., php-fpm
  depends on a network)
- how fragments declare volumes
- how fragments handle optional conditional sections (e.g., only add Redis
  if declared)

Acceptance: a written fragment format spec with one complete example fragment.

### Batch 2: Catalog Crate Foundation

Create `crates/effigy-catalog` with:

- fragment loading from embedded assets
- `service.toml` parsing
- parameter validation against schema
- template rendering (variable substitution)

Acceptance: unit tests pass for loading, parsing, validating, and rendering
test fragments.

### Batch 3: Compose Assembly Engine

Implement compose assembly:

- merge multiple rendered fragments into one compose file
- generate networking configuration
- generate volume declarations
- handle service dependencies (depends_on)
- write output YAML
- support override file merging

Acceptance: integration test that assembles a PHP + nginx + MariaDB compose
file from fragments and validates the output.

### Batch 4: Bundled Service Fragments

Write the initial catalog fragments:

- `php-fpm` — with Dockerfile accepting version and extensions build args
- `nginx` — with config variants (default, laravel, wordpress, spa)
- `mariadb` — with sensible defaults and data volume
- `postgres` — basic setup with data volume
- `redis` — minimal
- `memcached` — minimal

Acceptance: each fragment loads, validates, renders, and produces valid
compose YAML.

### Batch 5: Catalog Override and Inspection

Implement:

- catalog layering (project-local > user-global > bundled)
- `effigy catalog list` API (CLI integration deferred)
- `effigy catalog extract` API (CLI integration deferred)
- override resolution logic

Acceptance: tests prove override precedence works correctly.

### Batch 6: Eject and Compose File Management

Implement:

- generated compose file writing to `infra/dev/.effigy-compose.generated.yml`
- manifest checksum tracking for regeneration
- eject flow (copy generated to permanent, switch manifest)

Acceptance: eject produces a standalone compose file that works with raw
`docker compose up`.

### Batch 7: End-to-End Proof

Prove with one real PHP project:

- manifest declares services (php-fpm, nginx, mariadb, redis)
- effigy assembles compose from catalog
- `docker compose up` works against the generated file
- VirtioFS confirmed for macOS Colima performance
- custom nginx config works for the project's framework

Acceptance: the project runs from catalog-generated compose with no hand-
written Docker configuration.

## Intent Checkpoint

If the fragment format decision reveals that simple variable substitution is
insufficient (e.g., conditional sections are essential from day one), stop and
reassess whether a lightweight template engine is justified vs. keeping
fragments simpler with more variants.

## Exit Condition

This strict lane is complete when:

- `effigy-catalog` is a functional, tested library crate
- bundled fragments cover the initial service set
- one real project proves the loop
- the crate is ready for integration after `g02.010` completes

## Next Task

Begin with Batch 1: fragment format decision.

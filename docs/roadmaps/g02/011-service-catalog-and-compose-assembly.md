# 011 - Service Catalog and Compose Assembly

Generation: `g02`

Status: Complete
Owner: Platform
Created: 2026-04-16
Depends on: 006, 010

## Vision Alignment

The v1 container surface (`g02.006`) lets repos declare named container
environments and run them through Colima + Docker Compose. But every project
still needs a hand-written `docker-compose.yml` and Dockerfiles. For web
projects needing PHP, nginx, MariaDB, Redis, and Memcached, that's significant
boilerplate that gets duplicated and maintained in parallel.

The next product problem is eliminating that boilerplate. A repo should declare
its service stack in the manifest and let effigy assemble the compose file from
a catalog of reusable service fragments.

## Primary Tags

- `CONTRACT`
- `OPERATE`
- `MAINT`

## Target Envelope

- Effigy ships a service catalog of composable Docker Compose fragments.
- Repos declare services in the manifest instead of maintaining compose files.
- Effigy assembles a complete compose file from catalog fragments and project
  parameters.
- The catalog is embedded in the binary with layered override support.
- Projects can eject from the catalog to full compose ownership at any time.
- One real PHP project proves the loop end to end.

## Vision Target Delta

- Move from `hand-written compose files per project` toward
  `manifest-declared service stacks assembled from a reusable catalog`.

## 1) Problem

Web projects need multiple cooperating services (app runtime, web server,
database, cache, session store). Today those are configured via hand-written
docker-compose.yml and Dockerfiles that:

- get copied between projects with subtle drift
- require Docker/compose expertise to write correctly
- encode best practices (volumes, health checks, networking) inconsistently
- create a high barrier to the "host-clean dev environment" promise

## 2) Goals

- [ ] Define the service catalog format (fragment structure, parameter schema,
      capability declarations).
- [ ] Ship bundled fragments for: php-fpm, nginx, mariadb, postgres, redis,
      memcached.
- [ ] Define the manifest schema for service declarations under
      `[containers.<name>.services]`.
- [ ] Implement compose assembly from catalog fragments with parameter
      substitution.
- [ ] Support Docker Compose multi-file override for project-specific
      customization.
- [ ] Support catalog layering: project-local > user-global > bundled.
- [ ] Implement `effigy service list` and `effigy service extract`.
- [ ] Implement `effigy container eject` for full compose ownership.
- [ ] Ensure VirtioFS is configured for Colima profiles (critical for PHP
      file-serving performance on macOS).
- [ ] Prove with one real PHP + nginx + MariaDB project.

## 3) Non-Goals

- [ ] No framework detection or auto-configuration.
- [ ] No effigy Rust code that knows about PHP, nginx, or any specific service.
- [ ] No container context routing in this milestone (deferred to `g02.012`).
- [ ] No gateway or DNS in this milestone (deferred to `g02.014`).
- [ ] No attempt to replace Docker Compose semantics — compose is still the
      execution layer.

## 4) Contract Direction

### 4.1 Catalog Fragment Format

Each service fragment is a directory containing:

- `compose.fragment.yml` — templated compose service definition
- `Dockerfile` (optional) — accepts build args
- `service.toml` — parameter schema, defaults, capabilities
- `configs/` (optional) — named configuration variants

Fragment templates use simple variable substitution (not a full template
engine). Variables come from the manifest service declaration.

### 4.2 Service Declaration Schema

```toml
[containers.web.services.<name>]
catalog = "<fragment-name>"    # which catalog fragment to use
# ... fragment-specific parameters
```

`catalog` and `compose_file` remain mutually exclusive at the container level.
If a container declares services via `catalog`, compose is generated. If it
declares `compose_file`, the file is used directly.

### 4.3 Catalog Distribution

Fragments are embedded in the effigy binary at compile time using
`rust-embed` or `include_str!`. At runtime, effigy reads from its internal
catalog. No extraction step.

Override directories are optional and additive:

1. `infra/dev/catalog/` — project-local overrides
2. `~/.effigy/catalog/` — user-global overrides
3. embedded — bundled defaults (lowest priority)

### 4.4 Compose Assembly

The assembly engine:

1. Reads service declarations from manifest.
2. Loads matching fragments (respecting priority order).
3. Validates parameters against `service.toml` schema.
4. Substitutes parameters into fragment templates.
5. Assembles a complete compose file with networking, volumes, depends_on.
6. Writes to `infra/dev/.effigy-compose.generated.yml`.
7. Merges with `infra/dev/compose.override.yml` if present.
8. Caches until manifest checksum changes.

### 4.5 Nginx Configuration Flexibility

The nginx fragment ships named config variants selected by `variant =`:

- `default` — generic PHP front-controller (try_files to index.php)
- `laravel` — Laravel-specific routing
- `wordpress` — WordPress rewrite rules
- `spa` — single-page app with API proxy

Custom configs via `config = "path/to/nginx.conf"` always win over variants.

The `default` variant covers most custom PHP frameworks that use a single
front-controller entry point with URL rewriting.

### 4.6 PHP Extension Strategy

The php-fpm fragment ships a Dockerfile that:

- Starts from the official `php:<version>-fpm` image
- Accepts an `EXTENSIONS` build arg
- Installs extensions via `docker-php-ext-install` and `pecl`
- Caches the built image layer for reuse

Effigy doesn't know what PHP extensions are. It passes the list as a build
arg. The Dockerfile handles installation.

## 5) Implementation Approach

### 5.1 New Crate

`crates/effigy-catalog` — isolated library crate, no dependency on the main
runner or any other effigy domain crate.

Public API surface:

- fragment loading (from embedded + override directories)
- parameter schema parsing and validation
- template rendering (variable substitution)
- compose assembly (fragment merging, networking, volumes)
- catalog inspection (list, extract)

### 5.2 Integration Boundary

This crate is a pure library. Integration into the main runner (CLI commands,
manifest loading, container command dispatch) happens after `g02.010`
modularization completes. The crate should be fully functional and tested
independently.

### 5.3 Testing Strategy

- Unit tests for parameter validation and template rendering.
- Integration tests that assemble compose from test fragments and validate
  the output YAML.
- One end-to-end proof that generates a working compose file for a PHP +
  nginx + MariaDB stack.

## 6) Milestone Relationship

This is the critical path for the container infrastructure work. Without
catalog-based compose assembly, every project continues to need hand-written
Docker configuration.

Successor milestones `g02.012`–`g02.016` all build on this foundation.

## Next Task

This roadmap is complete. The crate foundation, product-facing catalog/eject
surface, and one real-project proof are all landed.

Move to `g02.012` for transparent execution integration, starting with `264`.

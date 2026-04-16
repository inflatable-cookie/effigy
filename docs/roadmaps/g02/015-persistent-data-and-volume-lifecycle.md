# 015 - Persistent Data and Volume Lifecycle

Generation: `g02`

Status: In Progress (volume management shipped in effigy-catalog, integration deferred)
Owner: Platform
Created: 2026-04-16
Depends on: 006, 011

## Vision Alignment

Database data and media uploads need to survive container restarts. They also
need to be exportable for machine migration and importable for team
onboarding. Complex seeding workflows (migration bundles, structured ingest
protocols) need to work as first-class tasks with Rhai integration.

## Primary Tags

- `OPERATE`
- `CONTRACT`

## Target Envelope

- Named Docker volumes for persistent state with explicit lifecycle management.
- Volume data survives `container down` and `container up` cycles.
- `container reset` wipes everything; `container reset --keep-data` preserves
  volumes.
- Volume management commands for export, import, and inspection.
- Seeding is task-based, not a special abstraction.
- Rhai hooks for production data pull and complex migration workflows.

## Vision Target Delta

- Move from `ephemeral container state that requires rebuild after every stop`
  toward `persistent data with explicit lifecycle and portable volume
  management`.

## 1) Problem

Without persistent data management:

- database contents are lost on `container reset`
- onboarding a new machine means rebuilding from scratch
- seeding is ad hoc and undocumented
- complex migration bundles (SQL + media + structured ingest) have no
  standard execution path

## 2) Goals

- [ ] Define `data.volumes` manifest field for persistent named volumes.
- [ ] Define `data.media` manifest field for media mount declarations.
- [ ] Ensure catalog fragments declare appropriate default volumes.
- [ ] Implement `effigy container reset --keep-data`.
- [ ] Implement `effigy container data list` — volume names, sizes.
- [ ] Implement `effigy container data export <volume> <path>`.
- [ ] Implement `effigy container data import <volume> <path>`.
- [ ] Define seeding as task-based (no special seed abstraction).
- [ ] Define `data.pull_production` as a Rhai or shell hook.
- [ ] Prove with a project that has complex seeding requirements.

## 3) Non-Goals

- [ ] No cross-project volume sharing.
- [ ] No automatic backup scheduling.
- [ ] No cloud storage integration.
- [ ] No database-aware dump/restore — volume-level export/import is the
      primitive; database-specific tooling runs via tasks.

## 4) Contract Direction

### 4.1 Volume Declaration

```toml
[containers.web.data]
volumes = ["mariadb-data", "redis-data"]
media = ["storage/uploads:/var/www/html/storage/uploads"]
```

Volumes are project-scoped (prefixed with compose project name). Media entries
are bind-mounted from the host.

### 4.2 Catalog Volume Defaults

Catalog fragments declare default volumes in their compose fragments:

```yaml
# mariadb/compose.fragment.yml (excerpt)
volumes:
  {{name}}-data:
    driver: local
```

These are automatically included in the generated compose file.

### 4.3 Volume Management

```bash
effigy container data list                     # names, sizes, last-modified
effigy container data export mysql-data ./backup.tar
effigy container data import mysql-data ./backup.tar
```

Export/import work at the Docker volume level — tar the volume contents.

### 4.4 Seeding as Tasks

```toml
[tasks.seed]
container_session = "web"
run = "rhai:scripts/seed.rhai"

[tasks."seed:fresh"]
container_session = "web"
run = [
    "php artisan migrate:fresh",
    "rhai:scripts/import-migration-bundle.rhai",
]
```

Complex migration bundles (SQL files, media, structured ingest) are Rhai
scripts that use effigy's exec surface.

### 4.5 Production Data Pull

```toml
[containers.web.data]
pull_production = "rhai:scripts/pull-prod.rhai"
```

The Rhai script can:

- access environment variables (including `@sensitive` from env schema)
- run exec commands inside the container
- coordinate multi-step download/import sequences
- handle errors with proper reporting

Shell scripts also supported: `pull_production = "scripts/pull-prod.sh"`.

## 5) Implementation Approach

### 5.1 Crate Impact

- Volume management logic extends `effigy-containers`.
- Data manifest fields extend `effigy-manifest`.
- CLI commands extend `effigy-cli`.
- Rhai hooks use the existing `effigy-rhai` adapter layer.

### 5.2 Testing Strategy

- Integration test for volume export/import round-trip.
- Task-based seeding test with a mock Rhai script.

## Next Task

Depends on `g02.011` for catalog fragments with volume defaults. Volume
management commands can be developed independently.

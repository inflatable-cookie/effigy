# Container Infrastructure Thread — Handoff Brief

Status: active (library crates complete, awaiting runner integration)
Updated: 2026-04-16
Thread scope: g02.011–g02.016

## What this thread built

Three isolated library crates implementing the container infrastructure
layer designed in `docs/architecture/020-container-infrastructure-design.md`.
All crates have zero dependencies on other effigy crates and are ready for
integration into the main runner after g02.010 modularization completes.

## Crates

### crates/effigy-catalog (68 tests)

Service catalog and compose assembly. Roadmap: g02.011 (complete), g02.015
(partial).

| Module | Purpose |
|--------|---------|
| `schema.rs` | `service.toml` parsing — params, types, defaults, capabilities, volumes |
| `fragment.rs` | 3-layer fragment loading (project-local > user-global > embedded), extract API |
| `template.rs` | Jinja2 rendering via minijinja, param validation, reserved-name checking |
| `assembly.rs` | Compose assembly via serde_yaml — render, parse, merge, serialize |
| `output.rs` | File writing with checksum caching, override support, eject flow |
| `volumes.rs` | Volume classification for reset, Docker CLI command specs |

10 bundled fragments: php-fpm, nginx, mariadb, postgres, redis, memcached,
mailpit, minio, elasticsearch, node.

Key design decisions:
- Config files (nginx.conf) are rendered through the same Jinja2 engine
  as compose fragments, so `{{ services["php-fpm"].name }}` resolves the
  PHP service name dynamically
- PHP Dockerfile uses `install-php-extensions` (community standard)
- Node fragment uses a named volume for node_modules to avoid platform
  conflicts with native binaries

### crates/effigy-gateway (92 tests)

DNS resolver, HTTP/HTTPS reverse proxy, and multi-project coordination.
Roadmap: g02.014 (in progress), g02.016 (partial).

| Module | Purpose |
|--------|---------|
| `dns.rs` | UDP DNS on hickory-proto, `*.test` resolution, AAAA handling, query cache |
| `proxy.rs` | Streaming HTTP/HTTPS proxy, WebSocket upgrade, body limits, response timeout, graceful drain |
| `tls.rs` | mkcert certs, rustls config, SNI resolver for per-domain HTTPS |
| `server.rs` | Coordinated DNS+HTTP+HTTPS+watcher lifecycle, PID files, signals |
| `routes.rs` | Route table with atomic JSON persistence, live reload via RwLock |
| `ports.rs` | Port allocation with gap filling, conflict detection |
| `registration.rs` | Atomic route register/deregister for container up/down |
| `resolver_setup.rs` | macOS `/etc/resolver/` file management |
| `stats.rs` | Atomic counters for all request types, JSON serialization |

Key design decisions:
- Proxy streams bodies instead of buffering — supports large uploads, SSE
- WebSocket upgrade uses bidirectional TCP pipe via hyper::upgrade
- DNS cache (2s TTL) reduces RwLock contention, invalidated on route change
- Gateway internal API at `/_effigy/{health,routes,stats}`
- Graceful drain waits 10s for in-flight connections on shutdown

### crates/effigy-exec (70 tests)

Container execution routing, CWD mapping, and readiness checking.
Roadmap: g02.012 (in progress), g02.013 (partial).

| Module | Purpose |
|--------|---------|
| `routing.rs` | Host vs container decision engine, host-native allowlist, task overrides |
| `cwd.rs` | Bidirectional host↔container path translation |
| `alias.rs` | Named exec aliases with multi-word command base |
| `detection.rs` | Container capability probing, handoff vs raw-exec strategy, cache |
| `health.rs` | Health check parsing (HTTP/TCP), polling state machine, ready display |

Key design decisions:
- Routing decision returns a struct with human-readable `reason` field
  for debugging and TUI display
- Container detection produces an ExecStrategy enum that the caller
  uses to choose between `effigy-to-effigy` handoff and raw `docker
  compose exec`
- Health poller is caller-driven (no I/O in the crate) — the module
  handles timing and state, the caller performs the actual probe

## Integration path

When g02.010 finishes:

1. **effigy-manifest**: add `services`, `context`, `exec`, `dns`, `data`
   sections to `[containers]` config schema.

2. **Container command dispatch**: when a container config has `services`
   (not `compose_file`), call `effigy-catalog::ComposeAssembler` to
   generate the compose file. Write it via `ComposeOutput`. Pass the
   `compose_file_args()` to Docker Compose invocations.

3. **Route registration**: in the container `up` handler, call
   `effigy-gateway::registration::register_route()` with the domain
   from `dns.domain`. In `down`, call `deregister_route()`.

4. **Exec routing**: before executing a task, call
   `effigy-exec::routing::route()`. If the result is
   `ExecTarget::Container`, exec the command inside the container. If
   `ExecTarget::ContainerNotRunning`, prompt the user. If
   `ExecTarget::Host`, proceed normally.

5. **CLI commands**:
   - `effigy exec <command>` — bypass task routing, exec directly
   - `effigy gateway up/down/status` — lifecycle management
   - `effigy catalog list/extract` — inspection

6. **Port allocation**: when `host.ports` are omitted in the manifest,
   call `effigy-gateway::ports::PortRegistry::allocate()` and use the
   returned ports in the generated compose file.

7. **Gateway startup**: `effigy gateway up` calls
   `effigy-gateway::server::run_gateway()` as a forked background
   process. `effigy dev` auto-starts it when DNS is configured.

## What's NOT done

- CLI integration (needs modularization complete)
- Manifest schema extensions (needs effigy-manifest crate)
- `effigy dev` TUI front door (needs managed-process runtime from g02.010)
- Rhai hooks for `pull_production` and seeding (needs effigy-rhai adapter)
- Real Docker Compose validation (testing against `docker compose config`)
- Cross-project `effigy container status --all` (needs Docker API queries)

## Files modified outside these crates

- `Cargo.toml` — workspace members list (3 crates added)
- `Cargo.lock` — dependency additions
- `docs/architecture/020-container-infrastructure-design.md` — design doc
- `docs/roadmaps/g02/README.md` — milestone status updates
- `docs/roadmaps/g02/011-016` — six roadmap files
- `docs/specs/011-*` — catalog strict lane spec (complete)
- `docs/specs/012-*` — exec strict lane spec (integration path)
- `docs/roadmaps/g04/batch-cards/200-*` — fragment format decision

## Parallel modularization work

This thread also executed g02.017 shell cleanup jobs in parallel:

| Shell | Before | After | Δ |
|-------|--------|-------|---|
| demo_command.rs | 3302 | 2819 | −483 (−14.6%) |
| docs_command.rs | 1083 | 977 | −106 (−9.8%) |
| bootstrap_command.rs | 1136 | 803 | −333 (−29.3%) |

Extractions into existing crates:
- `effigy-containers`: health probing, compose backend, Colima lifecycle commands
- `effigy-demo`: projection functions (KeyValue/TableSpec builders),
  `DemoActiveTerminalSession::from_active_attempt`, `DemoEntrypoint::from_manifest`,
  `derive_gap_class`
- `effigy-docs-policy`: JSON examples validation, next-action orchestration, domain defaults
- `effigy-core`: `KeyValue`, `TableSpec`, `NoticeLevel`, `StepState`, `MessageBlock`,
  `SummaryCounts` widget types (enables domain crates to produce display-ready projections)
- `effigy-bootstrap`: resolve/execute now delegate to crate via closure API

All four touched seams are at honest adapter/shell boundaries.

## Test counts

230 total tests across 3 infrastructure crates. All clippy clean. No cross-crate deps.

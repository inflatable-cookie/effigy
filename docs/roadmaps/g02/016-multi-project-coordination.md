# 016 - Multi-Project Coordination

Generation: `g02`

Status: In Progress (status and generated-compose auto-allocation landed; resource visibility still pending)
Owner: Platform
Created: 2026-04-16
Depends on: 011, 014

## Vision Alignment

Once multiple projects use effigy containers, port conflicts and resource
visibility become real problems. A developer running three PHP client projects
simultaneously needs automatic port allocation, a unified status view, and
awareness of resource consumption.

## Primary Tags

- `OPERATE`
- `MAINT`

## Target Envelope

- Automatic port allocation registry avoids conflicts between projects.
- Cross-project status view shows all running containers.
- Gateway route dashboard shows all registered domains.
- Resource visibility shows CPU/memory per project.
- Optional shared services for resource efficiency.

## Vision Target Delta

- Move from `manual port management and per-project visibility` toward
  `automatic coordination and unified cross-project awareness`.

## 1) Problem

With multiple projects:

- port 8080 and 3306 are declared in every project — conflicts on
  simultaneous startup
- no single view of what's running
- no awareness of total resource consumption
- each project runs its own database instance even when isolation isn't needed

## 2) Goals

- [x] Define port allocation registry at `~/.effigy/ports.json`.
- [x] Implement automatic port assignment when `host.ports` are omitted for
      Effigy-owned generated compose.
- [x] Implement `effigy container status --all` across repos.
- [x] Implement `effigy gateway status` with full route dashboard.
- [ ] Implement `effigy container stats` for resource visibility.
- [ ] Define optional `shared = true` service flag for shared instances.

## 3) Non-Goals

- [ ] No container orchestration beyond what Docker Compose provides.
- [ ] No cross-project dependency management.
- [ ] No centralized project registry — port allocation is discovered from
      running containers and manifest declarations.

## 4) Contract Direction

### 4.1 Port Registry

```json
{
  "allocations": {
    "client-x": { "base": 8100, "range": 100 },
    "client-y": { "base": 8200, "range": 100 }
  }
}
```

When `host.ports` aren't declared, effigy allocates from the pool. Explicit
ports always win.

### 4.2 Cross-Project Status

```bash
effigy container status --all
```

Discovers running containers across all repos by querying Docker. Shows
project name, container name, status, ports, domain (if registered with
gateway), uptime.

### 4.3 Resource Visibility

```bash
effigy container stats
```

Shows CPU and memory usage per container, grouped by project. Uses Docker
stats API.

### 4.4 Shared Services

```toml
[containers.web.services.db]
catalog = "mariadb"
shared = true
```

When `shared = true`, effigy uses a shared instance instead of a per-project
one. The shared instance runs as a separate compose project managed by effigy
(similar to the gateway).

This is opt-in, clearly documented as trading isolation for resource
efficiency.

## 5) Implementation Approach

### 5.1 Crate Impact

- Port allocation logic extends `effigy-containers` or lives in a thin
  coordination module.
- Cross-project discovery uses Docker API queries.
- CLI commands extend `effigy-cli`.

### 5.2 Testing Strategy

- Unit tests for port allocation logic.
- Integration test with multiple compose projects running simultaneously.

## Next Task

Execute card `276` to land bounded cross-project container resource stats.

# Feature Boundary Open Design

Status: open
Created: 2026-08-31
Owner: orchestrator
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../contracts/043-feature-placement-and-surface-migration-contract.md)

## Purpose

Preserve the unresolved design choices left after promoting the feature-
placement audit. Settled ownership and migration rules live in architecture
`026` and contract `043`; do not reopen them from this note.

## Open Questions

- What exact namespace names and grammar best express daily work, local
  runtime, repository intelligence, delivery/state, and extensions?
- Which direct aliases should remain permanent? No alias removal is approved.
- What extension transport should optional runtime/provider code use?
- What is the minimum base Rhai surface after provider-specific helpers move?
- How should the default catalog pack be installed and updated while remaining
  automatic, offline-capable, inspectable, and no harder to use than today?
- What exact consumer evidence will show the
  `bovine-accelerator-desktop` media-upload replacement is live and the
  `bovine-accelerator` Rhai storage dependency can retire?
- What evidence threshold should justify a future provider implementation in
  mandatory core?

## Settled Constraints

- semantic ownership, not universality or façade exposure, decides core;
- grouping is approved, but existing direct routes remain stable initially;
- repository intelligence remains core;
- catalog externalization cannot add operator ceremony;
- release transaction safety remains core while Effigy-specific distribution
  recipes move outward;
- S3 remains until the named consumer migration is proved.

## Decision Prototype: Help-First Grouping

Current general help is one flat table of roughly thirty command families.
Adding executable groups such as `effigy repo docs` would make the grammar
larger, reserve new top-level selector names, and produce longer forms than the
existing commands.

Recommended first migration:

- keep every executable route unchanged;
- split `effigy --help` into job-based sections;
- make the existing `help` command the discovery namespace;
- add `effigy help <group>` for one grouped inventory;
- add `effigy help <command>` as the conventional route to existing detailed
  help topics;
- do not add `effigy <group> <command>` execution aliases unless later usage
  evidence shows they solve a separate problem.

Proposed primary taxonomy:

| Help topic | Commands and shapes |
| --- | --- |
| `work` | `<task>`, `<catalog>/<task>`, `tasks`, `test`, `watch`, `doctor`, `init` |
| `local` | `container`, `system`, `workspace`, `gateway`, `service`, `exec` |
| `repo` | `graph`, `scan`, `docs`, `contracts`, `papercuts` |
| `deliver` | `artifact`, `state`, `deploy`, `release`, `bundle`, `bootstrap`, `demo` |
| `extend` | `skill`, `rhai` |
| `admin` | `config`, `deps`, `secrets`, `defer`, `uninstall`, `version`, completion and help |

Example general-help shape:

```text
Common
  effigy <task>       Run a repository task
  effigy tasks        Find tasks and inspect routing
  effigy test         Run the test orchestrator
  effigy doctor       Diagnose health and routing
  effigy init         Initialize repository configuration

Local environments
  effigy container    Operate declared containers
  effigy system       Operate the default system
  ...

Repository intelligence
  effigy graph        Navigate code structure
  effigy docs         Retrieve and validate documentation
  ...
```

Collision result: help topics live below the existing `help` command, so they
do not steal manifest selectors named `repo`, `local`, `deliver`, or similar.
Direct built-ins keep their current deferral and routing behavior.

Open naming edge: `bootstrap`, `demo`, and `secrets` cross group boundaries.
The table gives each one a primary discovery home; detailed help may cross-link
without duplicating execution routes.

## Next Task

Confirm or revise the help-first grouping and six topic names. Then prototype
catalog-pack acquisition before compiling separate migration lanes. Keep S3 out
of the implementation queue until its consumer gate is met.

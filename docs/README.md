# Effigy — current state

Effigy runs manifest tasks and built-in workflows across monorepos. Linked worktrees and clones marked `effigy.runtimeScope = ephemeral` get distinct generated-Compose identities and effective gateway hosts by default, and gateway routes are owned by their checkout path. The same checkout always replaces its own route; another checkout takes a domain automatically when the recorded checkout path is gone or its target stopped listening, and is refused by name otherwise. A deleted checkout's loopback assignment is reclaimed at allocation time rather than needing a reset. `effigy container hosts` exposes the host map; `effigy container retire` tears down one scope's owned resources. A worktree's container targets and its host-run tasks both resolve the declared backing-service alias to that scope's own service, so application configuration keeps its declared names and still reaches its own database; a host run refuses before it starts when the scope's backing service is not published. Consumer applications still need to feed the scoped names into public URLs and selectors; Queue lead `af3ab7c7-b631-4d4f-9da7-f2949ded82f3` tracks that wiring and an observed workspace recycle.

Runtime state follows [contract 052](knowledge/contracts/052-runtime-kernel-contract.md):
short foreground runs use memory, retained runs use their checkout journal, and
only gateway/port/admission facts use the machine store. The
[v0.15 migration guide](guides/085-v0.15.0-consumer-migration.md#runtime-state)
covers upgrade and deleted-worktree cleanup.

## By topic

- [Vision](knowledge/vision.md)
- [Architecture and ownership](knowledge/README.md)
- [Contracts and JSON schemas](knowledge/contracts/README.md)
- [Release procedure](knowledge/contracts/release.md)
- [Qualification evidence: merged workspace sessions on Colima, Docker Desktop and Linux](qualification/workspace-sessions-a00394012/README.md) (frozen candidate a00394012, task effigy#530; prepared, Colima entry sweep complete, positive gates blocked by the production cut)
- [Qualification evidence: ordinary-Compose on Colima](qualification/ordinary-compose-colima-5f466914/README.md) (frozen candidate 5f466914, tasks effigy#537/#542; prepared with a self-enforcing r18 entry sweep, native GO and consumer gates pending)
- [Qualification evidence: Acow ordinary-Compose consumer packets](qualification/acow-ordinary-compose-5f466914/README.md) (frozen candidate 5f466914, task effigy#540, amended by effigy#543 and effigy#545; prepared, not executed — routes armed at the selected current consumer head with the twelve-input closure, the approved bundle-sync transition, and the actual effigy#544 r18 proof receipt as the warm-up prerequisite, awaiting their GO chains)
- [Qualification evidence: task-engine signal matrix](qualification/task-engine-signal-matrix/README.md) (tasks effigy#579/#582/#585; prepared, not executed — accepted #298/#236/#325/#373 credit kept; sequence TERM/HUP host-source cells added, native Linux still owed; the UNARMED packet's registration binding and retained settlement need an owned production-seam proof; `test:qualification:task-engine-signals` is admission, labeled-double control flow and local HostCli fixtures, not native closure)
- [User and consumer guides](guides/README.md), including [managed task sessions](guides/012-dev-process-manager-tui.md)
- [Bounded QA groups workflow](guides/081-bounded-qa-groups-workflow.md) (implemented through `tasks qa-group`; `stop`/`hard_timeout_ms` wait on owned-run supervision, contract 052)
- [v0.15.0 consumer migration](guides/085-v0.15.0-consumer-migration.md) (catalog sources and pack retirement, behavior changes and DecodeLabs checklist)
- [v0.14.0 consumer migration](guides/083-v0.14.0-consumer-migration.md) (migration checklist and incremental first-parent change coverage)

## What's next

The project's plan is in Queue: its lanes, their documents and their order. Queue also holds leads, brief drafts, tasks, status and outcomes.

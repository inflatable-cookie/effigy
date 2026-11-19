# Qualification evidence

One small proof per supported backend for the v0.15 re-qualification, plus the
durable migration record. The retired packets live in git history: this index
names the exact reviewed commit that held each one and the receipts that
commit accepted, so a reviewer can retrieve the bytes without maintaining an
executable copy.

## Current proof routes

| Backend | Selector | Proves |
| --- | --- | --- |
| Colima/containerd | `effigy qualification:backend:colima -- --repo <checkout> --run <label>` | A real workspace comes up on Colima, one representative task runs in it, the stack tears down and the engine inventory returns to its starting state |
| Docker Desktop | `effigy qualification:backend:desktop -- --repo <checkout> --run <label>` | The same steps on Docker Desktop |
| Native Linux | `effigy qualification:backend:linux -- --repo <checkout> --run <label>` | The same steps on a Linux host's Docker Engine |

Each route runs `scripts/qualification_backend_proof.py`, uses ordinary
`effigy container`/`effigy exec` commands, and writes exactly one verdict file:
`<evidence-root>/<run>/<backend>.verdict.json`. The evidence root defaults to
`/Volumes/Stash/scratch/effigy/evidence`; the script refuses a root inside the
run's TMPDIR, so evidence never lands in disposable run scratch (effigy#1041).
Run instructions, the verdict schema and the refusal list are in the script's
docstring. Use a disposable checkout: teardown retires the stack's containers,
volumes and networks, and the proof asserts the engine inventory is unchanged
afterwards.

Real-engine signal settlement is proven by `tests/task_engine_kill_matrix.rs`
through `effigy test:task-engine:kill-matrix` on the host and
`effigy test:task-engine:kill-matrix:linux` on a Linux engine host: owner
INT/TERM/HUP/KILL, descendant closure, settled rows and honest owner-death
`unknown` settlement. The fake-backend signal packet and its observer feature
are gone.

Consumer continuity is not a packet. It is the owner-run checklist in
[consumer-continuity-checklist.md](consumer-continuity-checklist.md).

## Retired packets

None of these packets produced a native receipt: each was reviewed as
prepared-not-executed, and its candidate/consumer execution stayed GO-gated.
The reviewed commit is the last commit that held the packet before retirement.

| Packet | Task | Frozen subject | Reviewed commit | Accepted receipts |
| --- | --- | --- | --- | --- |
| `ordinary-compose-colima-5f466914/` (r18) | effigy#537, effigy#542 | candidate `5f4669141d8dabd91b7fbfef28d2bce21bc42ba2` | `90c6bc759` | None: prepared, never executed |
| `acow-ordinary-compose-5f466914/` (r1) | effigy#540, effigy#543, effigy#545, effigy#553 | candidate `5f4669141d8dabd91b7fbfef28d2bce21bc42ba2`; Acow selected at execution | `410361ac4` | None: prepared, never executed |
| `workspace-sessions-a00394012/` (r1) | effigy#530 | candidate `a00394012ae8dafe52981c36d459ec0491ec0552` | `68aa888f0` | None: prepared, never executed |
| `workspace-private-531/` (r1) | effigy#531, effigy#620, effigy#879, effigy#1041 | per-backend qualification candidate head | `705d08d9e` | None: source and fixture controls only |
| `task-engine-signal-matrix/` (r1) | effigy#579, effigy#582, effigy#585, effigy#739 | host task engine | `32580bcbf` | None: packet unarmed |
| `consumer-packets-r1/` (r1) | effigy#757, effigy#760, effigy#785, effigy#790 | #625 Acow/CBS continuity | `664efd00f` | None: routes unarmed |

The private workspace surface those packets qualified is retired (Tom
2026-11-19, D1); the task-engine signal packet is superseded by the real-engine
kill matrix above; the consumer packet is superseded by the owner checklist.

Retrieve a retired packet with `git show <commit>:docs/qualification/<path>`.
A historical packet, receipt or fixture is history, never a live gate.

## Retained records

- [Changelog fragment migration 804](changelog-fragment-migration-804/README.md):
  the finite mapping from released-version fragment names to the current
  layout. It has no launcher and is kept as the migration record.

# Consumer proof packets r1: Acow and CBS (task effigy#757, armed by effigy#760)

Source-only preparation for the #625 Acow/CBS continuity proof, against the
current candidate rather than the 5f466914 packet. Status: **prepared, unarmed,
not executed.** The committed manifest holds pending pins, so every route
refuses `route-pins-pending` before the request is read. Nothing here touched a
consumer checkout, engine, volume, row, secret or network.

The historical packet `acow-ordinary-compose-5f466914/` is unchanged. Its
boundary battery runs through `acow_battery.py`, which repoints only the
fixture scratch constant to a run-TMPDIR directory.

## Trust precondition

This is the same precondition task effigy#739 states for its proof entry, and
the packet relies on nothing stronger.

Nucleus launches `proof_entry.py {acow|cbs}` at the exact reviewed merged commit,
with the request file's SHA-256 on argv, under already-recorded same-sequence
authority. This program cannot authenticate that launch against another process
of the same user. A file, an environment value, a caller-set flag or this file's
existence is not that authority. Nothing here reads one as authority.

What the entry does not observe: that positive624 exists and is positive for
candidate C. The request names that receipt, and the launch precondition must
have checked it. The entry checks only that the receipt is for the same
candidate commit.

## What the entry observes before effect

Every item is checked by `consumer_core.arm`. Items that read no child come
before the evidence claim, so their refusals leave no evidence.

- Request bytes match the SHA-256 argument. Strict JSON, exact schema and keys, size cap.
- Pinned route is concrete (`route-pins-pending` otherwise).
- Every executed module matches the request's hashes: `consumer_core`,
  `proof_entry`, `acow`, `cbs`, and the shared stage primitive and resource
  monitor from `workspace-private-531`.
- Installed candidate bytes match `candidate.binary_sha256`.
- Window: monotonic bounds, route span, not expired, token digest present, id not in the consumed list.
- Budgets within route caps, with equal stream caps. Resource roles and kinds are the pinned set.
- Consumer checkout is a real directory, not a symlink.
- After the claim, and read through the supervisor: the packet checkout HEAD is
  the request's merged commit; the consumer checkout HEAD is the selected head,
  clean, and holds every reviewed closure blob; the bundle revision prefix and
  digest match.
- Engine identities, prior running state and counts, probed through the supervised engine.

## Supervision

Every child and every git read runs through `consumer_core.Supervisor`, which
drives the shipped one-use stage primitive `preparation_stage.run_stage` from
task effigy#531. The packet imports that primitive; it does not copy it. No
shared supervision file was changed.

- Each call gets a fresh 0700 directory under the evidence root. It keeps its
  own stdout and stderr files with hashes, and the stage receipt, even on refusal.
- One absolute window deadline. Steps launch before `end - 30s`. Rollback may
  use the reserve. Both use the window end as their cleanup bound.
- A live resource monitor samples the window clock on every loop turn, while a
  child is blocked.
- Only a waited zero exit with disposition `exited` passes. Every other
  disposition refuses: `child-failed`, `deadline-exceeded`, `over-cap-stream`,
  `resource-refusal`, and `child-unreaped` for unknown or local-error states.
  The window stops advancing on any of them. Rollback still runs, bounded.
- The stage primitive stops only the process group of the leader it started, by
  that leader's PID while the leader is unreaped. The packet does not treat a
  later PID as this group, and does not infer an empty group from errors.
- Known limit: a descendant that leaves the group and closes its inherited pipes
  is not seen. A descendant that keeps an inherited pipe is seen as a deadline breach.

## Files

| File | Role |
| --- | --- |
| `consumer_core.py` | Request digest and strict JSON, shape and binding checks, window, budgets, closure verification, identity probes, `Supervisor` over `run_stage`, counts and rollback rules, the `arm` sequence |
| `proof_entry.py` | The one fixed proof entry: `{acow\|cbs} --request PATH --sha256 DIGEST --evidence-dir DIR` |
| `acow.py` | Acow route: hybrid/container/hybrid phases, Level 1 forward, eight-table counts, scoped rollback, protected-identity check. `main` refuses unarmed |
| `cbs.py` | CBS route: detached app-shell, Level 1 forward, named counts, scoped rollback, volumes-kept check. `main` refuses unarmed |
| `acow_acquisition.py` | Ordinary Acow Linux-artifact acquisition check (effigy#785): legacy then modern detached `container stack up`, handoff proof by produced artifact plus running stack. `main` refuses unarmed |
| `acow_battery.py` | Test-only harness: runs the unchanged historical Acow boundary battery with only its fixture SCRATCH relocated under TMPDIR |
| `manifest-r1.json` | Source-visible pins; pending values refuse their route |
| `tests.py` | Gate controls through `arm` with fixture engine and git; supervision controls on harmless children; end-to-end fixture runs through `proof_entry.run` |

## Selectors (`config/tasks.toml`)

| Selector | Runs | Effect |
| --- | --- | --- |
| `test:qualification:consumer-packets` | `tests.py` | None: fixture engines and git, real supervisor on harmless `-I -c` children, real git in fixture checkouts |
| `check:qualification:consumer-packets:compile` | compile all packet modules in memory | None |
| `qualification:acow-consumer:proof-only` | `proof_entry.py acow {args}` | Refuses before the request is read while the route is unarmed |
| `qualification:cbs-consumer:proof-only` | `proof_entry.py cbs {args}` | Refuses before the request is read while the route is unarmed |
| `qualification:acow-acquisition:proof-only` | `proof_entry.py acow-acquisition {args}` | Refuses before the request is read while the route is unarmed |
| `test:qualification:acow` | `acow_battery.py` → historical `boundaries.py` (150 checks) | Historical bytes unchanged; fixture SCRATCH under TMPDIR, restored on exit |

## Armed sequence

**Acow.** Identity probes first: the postgres container from the project and
service labels must be exactly the request's container, and must mount the
pinned volume. Protected capture: shared MariaDB and every named volume must be
present. Prior running state of `hybrid` and `stack` must match the request, and
the intent must name exactly those two. Then `container hybrid up --detach`,
`container stack up --detach`, `container hybrid up --detach`, each observed.
Then the Level 1 forward `container stack status --json`. Then eight named-table
counts, counts only, against baseline plus declared delta. Each environment is
marked unknown before its up, so a failed or partly started up is rolled back.
Rollback stops only environments this window started and that are not observed
not running. Afterwards the protected identities must still be present.

`dev:test` is declared, not armed. Its fresh environment is not pinned in
`infra/workspace.toml`. `dev` and `dev:container` are TUI and need a PTY, which
is Level 2, so this route uses the detached container forms.

**CBS.** Identity probe for the database container and the engine volume the
request names. Prior state of the `dev` system must match the request. Then
`container up --detach --system dev`, observed. Then the forward
`container status --system dev --json`. Then counts over `cbs` and
`cbs-mortcalc` through the pinned count tail. Then `container down --system dev`
only if the window started it. Then named volumes must all still be present.
No seed, deploy, sync, schema refresh or data command is in either route.

## What the reviewed commit pins, and what remains

Bound by the request, with no source edit: candidate C and its installed bytes,
positive624 receipt, helper head and module hashes, merged commit, window,
Acow full resolved bundle revision, and the container and volume identities.

Still needs a reviewed manifest commit at freeze, because they are
consumer-config facts observed at that time:
- Both routes: the `status --json` running-field contract, verified on the frozen candidate.
- CBS: the compose project label, the mysql count tail (credential as a container-env reference, never a literal), the six table baselines for `cbs` and `cbs-mortcalc`, and the bundle prefix and digest.

Consumed windows are recorded in the reviewed manifest. A window is also one use
per evidence directory, because the directory is claimed exclusively.

## Decisions carried into the PR

- Authority is the #739 launch precondition, not an authenticate callback. The
  packet has no callback to fake, so the fixture controls never inject one.
- The supervisor is the shipped `run_stage`, not a copy of the old `Popen` runner.
  Stream caps must be equal, because `run_stage` applies one cap per stream.
- Git reads run through the same supervisor. Evidence is claimed before the
  first child, so a failed git read is retained too.
- A refusal or failure after the claim spends the occurrence. Non-refusal
  exceptions record `error`, and they spend it as well.
- Unknown rollback state counts as running. An environment never observed not
  running is brought down if this window started it.

## Not claimed

- No consumer credit, no #625 acceptance, no #624 aggregate judgment, and no
  OrdinaryAcow smoke credit. Passing these controls is source and fixture evidence only.
- No engine, VM, SSH, install, build, download, consumer command, DB access,
  network or tunnel action has run. The end-to-end controls run a harmless
  fixture engine from the run TMPDIR, against a fixture git checkout.
- The trust precondition is not verified by code. It is the external Nucleus
  launch that effigy#739 also relies on.

## Ordinary acquisition check (effigy#785)

Source and controls only. `acow_acquisition.py` is a separate route, not a mode
in `acow`. It keeps the aggregate, privacy and continuity gates of the Acow
route out of its path, because it checks one thing: the ordinary automatic
Linux-artifact producer, reached through a detached ordinary form.

**Form.** `container stack up --detach` in each checkout, cwd set to that
checkout, with no `--repo` and no `--command`. Chosen over `dev`: `dev` is a TUI
and needs a PTY (Level 2), and no reviewed PTY interface is in this packet. The
form reaches the producer by source: `container <env> up` calls
`environment::activate` (`container_command/lifecycle.rs`), which calls
`prepare_workspace_artifact` on every activation. That function acquires only
for an environment that declares an operator workspace
(`operator_workspace_for_policy`, `prepare_workspace_artifact_with`). Hybrid
never probes or acquires, so hybrid detach is refused as a check form. Stack
declares the workspace in Acow's `infra/workspace.toml`. Not traced: `dev`'s own
entry into `activate`. The 766 run shows `dev` reached the same producer, but
the equivalence is source-traced for the detached form only, not executed.

**Passes.** Legacy first, modern second, one occurrence, one window. Each pass:
identity probe (container and volume, read-only), protected capture, prior
state of `stack` must be not running (a running stack refuses before any up),
artifact stat before, `up --detach`, `status --json`, artifact stat after,
rollback (`down`) of the stack this pass started, protected check. A pass
passes only when the up exits zero, the stack is observed running, and the
producer wrote a receipted artifact in this window (new file, or mtime moved,
receipt naming `aarch64-unknown-linux-gnu`). A build exit alone refuses as
`handoff-missing`. A legacy refusal or unknown closure never starts modern.

**Closures.** Both consumer closures are read before any engine or consumer
effect, through `core.arm`'s `verify_more` hook with the same recording reader.
The request carries a second block, `modern_consumer`. Only this route allows
that key (`ROUTE_EXTRA_KEYS`).

**Budgets.** Step timeout 1680 s and 64 MiB per stream, from the reviewed
effigy#766 ordinary run (1800 s wall less a 120 s closeout; 64 MiB cap). Window
3600 s. These are not new numbers; they are the maintained route's.

**Restore.** Each pass ends with the stack in its prior state. A successful
pass ends with `down`, the same as the Acow route's rollback. The window does
not leave a stack it started running.

**Controls.** `tests.py` drives the real `arm`, `Session`, `Supervisor` and
`preparation_stage.run_stage` with a harmless fixture CLI. End-to-end controls
run real git checkouts at the reviewed closures and a fixture CLI that writes a
fixture artifact on `up`. They cover: unarmed refusal, request shape and
`modern_consumer` scope, both passes in order, build exit alone refused, legacy
failed and unknown skipping modern, modern failure, prior running refused before
any up, prior intent shape, modern closure drift, dirty modern checkout, foreign
container, receipt and stale-mtime logic, symlinked artifact path, session
allowlist, window one-use, span and expiry, unchanged acow and cbs request shape,
same-source detached copy binding the same module hashes, and the e2e run.

**Unresolved pins (block arming; read at freeze, never guessed).**
- Legacy: `effigy.toml` blob at the selected head; `includes`; service label and
  data volume and every named volume for project `acowtancy-decodelabs`; the
  bundle revision prefix (observed `8121cfc2`, 2026-11-09) and content digest.
  Also confirm the legacy `stack` environment declares an operator workspace.
  Its bundle is outside the tracked repo, so it is not read here.
- Modern: closure blobs at the selected head (main moved from `e23ed290e` to
  `16c706ba`); the bundle prefix and digest are the reviewed acow values.
- Both: whether a stopped stack keeps its postgres container and volume (the
  identity probe uses `ps -a`). If not, the pre-up identity cannot be bound and
  the pin must change before arming.
- Request resources: `legacy-postgres-container`, `legacy-postgres-volume`,
  `postgres-container`, `postgres-volume`, bound to actual identities.
- Producer host facts: the artifact paths are `.effigy/linux-release/artifacts/effigy-aarch64-unknown-linux-gnu` and its `.receipt`. The active-version sidecar is not checked here.
- Hooks: any ordinary startup hook in either consumer that writes to protected
  resources. Not inspected in this source task; the parent must bind the real
  protected window before any request.
- Candidate: product Cprime `5751ff9d`, tree `71fad36b`, the signed binary
  `efb96b23`, and its install receipt. The request binds these; the manifest
  does not.

**Not claimed.** No consumer, engine, VM, build or native run. No full625,
624, 621-623, RecipeE, Level 2 or release credit. No Linux artifact or prebuild
credit. The route is UNARMED until its pins are concrete and a Nucleus
proof-only launch at the merged commit supplies the request.

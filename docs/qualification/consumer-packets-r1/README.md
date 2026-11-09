# Consumer proof packets r1: Acow and CBS (task effigy#757, armed by effigy#760)

Source-only preparation for the #625 Acow/CBS continuity proof, against the
current candidate rather than the 5f466914 packet. Status: **prepared, unarmed,
not executed.** The committed manifest holds pending pins, so every route
refuses `route-pins-pending` before the request is read. Nothing here touched a
consumer checkout, engine, volume, row, secret or network.

Since effigy#790 the ordinary acquisition route can be armed without editing
reviewed source: the proof entry accepts a hash-pinned external runtime binding
on its argv that carries exactly the route's frozen pins. The committed pins
stay pending, so the default and every ordinary invocation still refuse. See
[the binding section](#the-external-runtime-binding-effigy790) and the
[acquisition section](#ordinary-acquisition-check-effigy785-producer-bound-per-effigy790).

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
| `consumer_core.py` | Request digest and strict JSON, shape and binding checks, window, budgets, closure verification, identity probes, `Supervisor` over `run_stage`, counts and rollback rules, the external route-binding loader, the `arm` sequence |
| `proof_entry.py` | The one fixed proof entry: `{acow\|cbs\|acow-acquisition} --request PATH --sha256 DIGEST --evidence-dir DIR [\--binding PATH \--binding-sha256 DIGEST]`; only the acquisition route offers the binding |
| `acow.py` | Acow route: hybrid/container/hybrid phases, Level 1 forward, eight-table counts, scoped rollback, protected-identity check. `main` refuses unarmed |
| `cbs.py` | CBS route: detached app-shell, Level 1 forward, named counts, scoped rollback, volumes-kept check. `main` refuses unarmed |
| `acow_acquisition.py` | Ordinary Acow Linux-artifact acquisition check (effigy#785, producer-bound per effigy#790): legacy then modern detached ordinary up, per-consumer pinned environments, host producer binding and production currency semantics. `main` refuses unarmed |
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
| `qualification:acow-acquisition:proof-only` | `proof_entry.py acow-acquisition {args}` | Refuses before the request is read while the route is unarmed; with a hash-pinned runtime binding on argv, arms the reviewed route pins without touching reviewed source |
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

## Ordinary acquisition check (effigy#785, producer-bound per effigy#790)

Source and controls only. `acow_acquisition.py` is a separate route, not a mode
in `acow`. It keeps the aggregate, privacy and continuity gates of the Acow
route out of its path, because it checks one thing: the ordinary automatic
Linux-artifact producer, reached through a detached ordinary form.

### The external runtime binding (effigy#790)

The committed manifest holds pending pins, so the real seven-argument CLI
refuses `route-pins-pending` before the request is read. Arming the shipped
entry no longer requires editing reviewed source: the proof entry accepts an
optional external runtime binding, hash-pinned on argv:

    proof_entry.py acow-acquisition --request PATH --sha256 sha256:HEX \
        --evidence-dir DIR --binding PATH --binding-sha256 sha256:HEX

The binding document is `{schema, route, pins, consumed_windows}` under
`effigy.qualification.consumer-packets.route-binding.v1`. It is schema-strict,
names exactly one route, and carries exactly the acquisition route's functional
pin set (`PIN_KEYS`); manifest-side citation fields are annotations and are not
carried. The binding supplies the route's frozen pins and its consumed-window
list; the candidate, helpers, resources and window stay bound exactly by the
hash-pinned request, and the committed manifest stays authoritative for the
routes it still pins. Malformed, duplicate, unknown-key, cross-route, altered
(digest mismatch) and still-pending forms refuse before the request is read,
before the evidence claim and before any child. `acow` and `cbs` accept no
binding (`binding-not-offered`), so their committed pending pins keep them
unarmed. The occurrence's `binding.json` records the binding digest it ran
under. Without a binding the committed defaults stay fully unarmed.

### Source call trace (candidate 5751ff9d, an ancestor of this HEAD)

The route's assumptions are traced in the candidate's own source; a fake CLI
success alone proves nothing.

| Step | Candidate source |
| --- | --- |
| `container <env> up --detach` runs the Compose apply | `src/runner/container_command/lifecycle.rs`: `environment::reconcile` (`compose up -d --remove-orphans`; recreates only services whose rendered configuration changed), then readiness, then `environment::activate` |
| Activation acquires on every activation, running or not | `src/runner/environment.rs` `activate`: `prepare_workspace_artifact(&bring_up.repo_root, policy)` before `activate_with` |
| Acquisition only for a declared operator workspace | `provisioning.rs` `prepare_workspace_artifact_with` → `operator_workspace_for_policy` → `resolve_operator_workspace_for_container`; a hybrid environment gets no guest and acquires nothing |
| The producer checkout is the host's provenance checkout, not the consumer | `provisioning.rs` `artifact_plan` (Auto + Checkout) → `host_provenance` → `checkout_of_local_install` (`<checkout>/.local-install/bin/effigy`) then `checkout_of_build_tree` (`<checkout>/target/{debug,release}/effigy`), gated by `looks_like_effigy_repo_root`; nearby checkouts are not provenance |
| Build or reuse in that checkout | `ensure_local_linux_workspace_effigy_artifact`: a current artifact is reused untouched; a missing or stale one runs `workspace:linux:artifact` once under a per-target lock and requires `linux_workspace_artifact_produced_since` plus a digest change |
| The output identities the route reads | `bind_verified_linux_workspace_effigy_artifact` writes `<artifact>.active-version` and `<artifact>.install-identity`; `workspace_artifact_receipt` reads the `key=value` `<artifact>.receipt` (`release_triple`, `sha256`); `linux_workspace_effigy_artifact_is_current` is mtime-independent |
| Publication re-verifies inside the activation | `verify_guest_artifact_until` (version sidecar = host version, bytes digest = receipt `sha256`) then `publish_workspace_guest_in` → `check_acquired_artifact` → `prepare_workspace_guest`; the `GuestPublicationReceipt` records `artifact_digest`. The route observes these as postconditions and claims no publication credit |
| The install receipt the route binds | `crates/effigy-core/src/build_info.rs`: the `<install>.active-version` stamp records `v<version>+local.<commit>[.dirty]`; `local_commit_from_identity` extracts the commit |

Tracked/bundle inputs: the modern `stack` environment is source-visible
(tracked `infra/workspace.toml` declares `[containers.stack]` and the workspace
service; `[bundle.profiles] dev = "hybrid"`), read at modern main `a58d9970`,
2026-11-09. The legacy environment names come from the decodelabs Underlay
bundle, which sits outside the tracked repo, so the legacy name is a pending
freeze pin, never guessed here.

### Form and environments

Each pass runs `container <env> up --detach` in its own checkout, cwd set to
that checkout, with no `--repo` and no `--command`, where `<env>` is that
consumer's pinned ordinary environment (`environments` route pin; legacy is
pending, modern is `stack`). Chosen over `dev`: `dev` is a TUI and needs a PTY
(Level 2), and no reviewed PTY interface is in this packet. The detached form
reaches the producer by the trace above. Neither pass assumes the other's
composition, and no fixture claims the equivalence: the exact bundled
composition of each consumer is verified at freeze against the reviewed bundle.

**Status contract.** The running state is read from `container <env> status
--json` through the route's pinned contract: field `runtime_running`, schema
`effigy.container.status.v1`. The candidate emits that field from
`status_report` (`crates/effigy-containers/src/report.rs`, candidate 5751ff9d).
The acow and cbs routes keep their own `running` contract. A report that does
not match the pinned field or schema refuses `status-unparsed` before any up. A
live read at freeze must confirm it (`status_contract.live_check`, pending).

### Passes

Legacy first, modern second, one occurrence, one window. Each pass, in order:

1. protected capture: the shared MariaDB container and every named protected
   volume must be present;
2. prior state read through the pinned status contract, which must match the
   request's recorded intent — `True` and `False` are both ordinary; drift
   refuses before any up;
3. when the stack is already running, its container and volume identity is
   bound before the up (read-only probes);
4. producer output facts before (stat, digest, receipt, sidecars — no child);
5. `up --detach`, `status --json`, then the stack container identity bound
   against the request (a cold stack's container exists only after its
   ordinary up, so the binding happens there; a foreign, replaced or
   non-mounting container refuses `resource-mismatch` after the up, with the
   rollback scoped exactly as below);
6. producer output facts after, judged by production semantics:
   - the artifact must be current for the pinned candidate version, producer
     install identity, receipt target and receipt digest
     (`artifact_current` mirrors `verify_guest_artifact_until` +
     `linux_workspace_effigy_artifact_is_current`);
   - when it was not current before, it must have been rewritten inside this
     window, as production refuses unchanged stale bytes; a stale or
     wrong-identity artifact that merely moved refuses `handoff-missing`;
   - when it was already current, the pass records the verified warm reuse
     production itself performs (`reused`), with nothing rewritten;
7. rollback: `down` only for stacks this window started (a prior-running stack
   is reconciled by the ordinary up and never stopped or recreated);
8. protected check: the same protected identities still present.

A build exit alone refuses `handoff-missing`. A legacy refusal or unknown
closure never starts modern. The route claims the host-side acquisition and the
observed running stack — not guest admission, not publication, not `dev`/TUI
credit. A naturally missing or stale artifact is left to the ordinary
maintained producer: the route never prebuilds, deletes or restamps output to
manufacture a cold pass.

**Closures.** Both consumer closures are read before any engine or consumer
effect, through `core.arm`'s `verify_more` hook with the same recording reader.
The request carries a second block, `modern_consumer`, and the producer block
`producer` (exact producer checkout, optional artifact digest, expected
install-identity sidecar value). Only this route allows those keys
(`ROUTE_EXTRA_KEYS`). Before the evidence claim the route also derives the
producer checkout from the pinned install path with the candidate's own
provenance rules, refuses a producer checkout inside either consumer, and
verifies the installed bytes carry the genuine install receipt naming the
pinned candidate version and source commit.

**Budgets.** Step timeout 1680 s and 64 MiB per stream, from the reviewed
effigy#766 ordinary run (1800 s wall less a 120 s closeout; 64 MiB cap). Window
3600 s. These are not new numbers; they are the maintained route's.

**Restore.** Each pass ends with the stack in its prior state. A pass that
started a stack ends with `down`, the same as the Acow route's rollback. A
prior-running stack is left exactly as it was found.

**Controls.** `tests.py` drives the real `arm`, `Session`, `Supervisor` and
`preparation_stage.run_stage`. The end-to-end controls run through the real
argument-parsing entry (`proof_entry.main` and the binding form of
`proof_entry.run`) with the committed pending manifest and a hash-pinned
binding — never a Python-injected manifest. The production-shaped fixture puts
the host producer output in its own producer checkout, outside both consumer
checkouts, and its producer step mirrors
`ensure_local_linux_workspace_effigy_artifact`: rebuild once when absent or not
current, reuse untouched when current. They cover: unarmed default refusal, all
binding refusal forms before any child, provenance derivation and install
receipt binding, producer-block refusals before the claim, currency matrix
(target, digest, version, identity, pinned bytes), stale-needs-rewrite and
warm-reuse logic, wrong-identity and wrong-receipt output refusing after the
up, legacy cold acquisition and modern verified reuse in order, exact
per-consumer environment mapping, prior-running reconciliation preserved and
never stopped, failed legacy skipping modern, a failed producer publishing
nothing, protected rollback, foreign and non-mounting containers, symlinked
producer paths, status contract, window one-use, span and expiry, unchanged
acow and cbs request shape, same-source detached copy binding the same module
hashes, and the subprocess main-entry e2e with supervised waited exits. On a
workspace whose committed evidence root is absent, the real claim refuses
`evidence-root` fail-closed and the control asserts exactly that refusal plus
the same occurrence through the entry's binding form; nothing is faked.

**Unresolved pins (block arming; read at freeze, never guessed).**
- Legacy: the ordinary environment name that declares the operator workspace
  (reported web/dev at 766; bundle-composed, verified against the exact
  bundled composition at freeze); `effigy.toml` blob at the selected head;
  service label, data volume and every named volume for project
  `acowtancy-decodelabs`; the bundle revision prefix (observed `8121cfc2`,
  2026-11-09) and content digest.
- Modern: closure blobs at the selected head; the bundle prefix and digest are
  the reviewed acow values.
- Status contract: one read-only `container <env> status --json` on the frozen
  candidate, for a stopped and a running stack, must show `runtime_running`
  and `effigy.container.status.v1`. The pin is `status_contract.live_check`.
- Request resources: `legacy-postgres-container`, `legacy-postgres-volume`,
  `postgres-container`, `postgres-volume`, bound to actual identities; the
  container identity is bound after the ordinary up for a cold stack, and the
  volume must exist before it (compose `down` keeps named volumes).
- Producer host facts: the producer checkout the candidate install derives to;
  the artifact at `.effigy/linux-release/artifacts/effigy-aarch64-unknown-linux-gnu`
  with its `.receipt`, `.active-version` and `.install-identity` sidecars; the
  exact install-identity sidecar value at freeze; an artifact digest pin when
  the artifact is current at freeze (null when the legacy pass is expected
  cold).
- Hooks: any ordinary startup hook in either consumer that writes to protected
  resources. Not inspected in this source task; the parent must bind the real
  protected window before any request.
- Candidate: product Cprime `5751ff9d`, tree `71fad36b`, the signed binary
  `efb96b23`, and its install receipt. The request binds these; the manifest
  does not.

**Not claimed.** No consumer, engine, VM, build or native run. No full625,
624, 621-623, RecipeE, Level 2 or release credit. No Linux artifact or prebuild
credit. No dev/TUI credit, no guest admission or publication credit. The route
is UNARMED until its pins are concrete — committed or hash-pinned on the
launch argv — and a Nucleus proof-only launch at the merged commit supplies
the request.

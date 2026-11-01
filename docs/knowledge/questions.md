# Questions

Consumer application wiring of scoped worker hosts remains on Queue lead
`af3ab7c7-b631-4d4f-9da7-f2949ded82f3`.

## Q-001 — Gateway PID identity

Status: answered
Asked: 2026-10-05
Answered: 2026-10-05 by Tom
Owner: [Gateway process identity ruling](architecture/020-container-infrastructure-design.md#gateway-process-identity-ruling)

Tom's v0.14.0 publication block remains in force until final release assurance
completes. He does not accept the PID-only risk and authorized dispatch of the
bounded correction now implemented in task 104 and merged in PR 220. The owning
architecture records the implementation, residual TOCTOU, and remaining
assurance boundary; this does not claim publication readiness.

## Q-002 — macOS gateway cross-user identity access

Status: answered
Asked: 2026-10-05
Answered: 2026-10-06 by Tom
Owner: [Gateway process identity ruling](architecture/020-container-infrastructure-design.md#gateway-process-identity-ruling)

Tom approved the narrow, bounded read-only identity command through Effigy's
existing administrator elevation. It reads only the trusted recorded gateway
target and returns bounded identity/status data. Authentication may be required;
declined or non-interactive authentication yields unknown, and lifecycle commands
refuse to signal. The reader does not signal, change records, or start anything.

No new helper, install, standing privilege, live-operations authority or change
to host-run contract 010 is approved. The larger privilege model is not approved.
Q-001's publication block remains until exact-candidate release assurance
completes; task 104's sidecar and fail-closed legacy policy are merged, with the
documented TOCTOU and cross-UID proof limitation still visible. Decision:
`634c5bd1-b3c9-4f18-88cc-4fd16d117b82`.

## Q-003 — Production deployment ownership

Status: open (partly settled)
Asked: 2026-10-18

Settled with Tom on 2026-10-18:

- Effigy's deploy commands are untested in the wild, and no consumer deploys
  through them yet. Acowtancy needs a Render deployment soon (Tom: "I need
  that sorted soon").
- v0.15 keeps `deploy model` and `deploy export`. `deploy apply` and
  `deploy redeploy` refuse cleanly rather than report success for obligations
  they don't execute.
- The direction is an end-to-end `effigy deploy <env>` that runs the same way
  from a developer machine, CI or an agent (Tom: "Probably all three?").

Settled with Tom on 2026-10-23:

- v0.15 makes `deploy model` and `deploy export` environment-aware (Tom:
  "Yes"). They take `<ENV>` like `plan`, `apply` and `status`, resolve
  `[deploy.<env>]` over the shared `[deploy.model]`, and refuse an undeclared
  environment. They never infer UAT or production values from the dev
  profile. Acowtancy's Render readiness audit found the model could only
  describe dev `.test` domains and dev task commands.
- Previews run without network or secrets (effigy#279): pinned provider
  revisions, offline plan, and the resolved provider commit in every report.

Open: the full design. The starting shape is in Queue lane `deploy`: a kernel
run with intent before each effect and explicit obligations (artifacts by
digest, release gates, state and migrations, provider apply through an
external adapter, health, history and rollback). Acowtancy on Render is the
proving case.

## Q-004 — Relationship to Compose

Status: answered
Asked: 2026-10-18
Answered: 2026-10-18 by Tom

Effigy is the environment authority. It owns identity, the desired-state
record, the render, data claims and holders, routes, and who may change an
environment. Compose is the engine that applies the render on the Colima and
Docker backends and reports what is running.

- Direct `compose_file` mode is retired, provided the migration is clean (Tom:
  "If there's a clean migration to managed catalog fragments for the two
  projects, then let's do it"). For one release a `compose_file` container is
  read as a project-layer catalog fragment, with a deprecation warning, and
  `effigy container migrate` writes the fragment out. After that release the
  key is retired, with an error naming the command.
- The generated Compose stays internal: there is no mutating pass-through
  (Tom: "Probably stay internal"). A read-only `effigy container render <env>`
  shows the exact render, path and digest for the current generation.
- The backend seam is "reconcile this environment from its render", not "run
  Compose". Apple Containers, when adopted, brings its own answer to Compose
  (Tom: "if we do Apple Containers, then we need to come with a solution for
  Compose one way or another").

## Q-005 — Service catalog strategy

Status: answered
Asked: 2026-10-18
Answered: 2026-10-18 by Tom ("Yeah that sounds spot on")

The goal stands: projects, bundles and third parties can add or change
services, workspace images and configuration without an Effigy release. Pinned
source is enough; there is no central registry.

The mechanism is catalog sources, which are directories of service fragments
resolved in this order:

1. the project's `.effigy/catalog/`;
2. a `catalog/` directory shipped inside each bundle;
3. declared extra sources (`[catalog.sources.<name>]`, from git, OCI or a path,
   pinned per checkout and updated by explicit sync like bundles);
4. the compiled baseline.

A name can be qualified (`source/service`), and `service list` and `config`
show where each service came from. Fragments declare the capabilities and
schema version they need, so an older Effigy names the version it requires.

The installed catalog-pack runtime is retired. That covers the store,
install/update/rollback/reset, the OCI channel, the workspace pack handoff and
the support policy. The pack repository is archived and its fragments fold
back into the baseline. Architecture
[026](architecture/026-feature-placement-and-command-surface.md) §Service
Catalogs is superseded and is rewritten by the implementing change.

Implementation notes (2026-10-20, #234), for Tom to confirm:

- The project source stays at the committed `infra/dev/catalog/`, not
  `.effigy/catalog/`. `.effigy/` is gitignored local state that Effigy writes
  into its own ignore block, project fragments must be committed, and #230's
  `container migrate` already writes there.
- "`config` shows where each service came from" is met by
  `effigy container render`, which lists each environment service's catalog
  and source; Effigy has no per-service `config` command.
- Declared sources resolve in handle order. Precedence between two declared
  sources is visible in `service list` (`shadows`) and can be pinned with a
  qualified name.
- Archiving the pack repository is an operator action; Effigy no longer reads
  it.

### Object storage: Silo replaces MinIO (2026-10-28)

Tom ruled MinIO unsupported. Acowtancy and the upstream Underlay bundle use
Silo (PGSTY Silo), and the one live external `catalog = "minio"` consumer found
by the root scan was Convergence, which Tom retired and shields from edits.
Effigy's baseline catalog therefore ships `silo` only: the built-in `minio`
fragment is removed (effigy#424), `effigy service list` no longer offers it, and
a manifest whose `catalog = "minio"` would fall back to it is refused with a
message naming `silo`. The retirement covers Effigy's own baseline: Q-005
precedence is unchanged, so a project, bundle or declared source that defines
its own `minio` fragment still resolves from that source, and list and render
attribute it there. The name is never remapped and no data migration is
invented.
Silo keeps the `MINIO_ROOT_*` environment names and `minioadmin` defaults as its
own compatibility surface. Existing volumes and data stay untouched.

## Q-006 — Heavy tasks

Status: answered
Asked: 2026-10-18
Answered: 2026-10-18 by Tom

Effigy offers generic heavy-task controls that any orchestrator may use, with
no commitment to a particular workflow. Tom: "We need a solution where heavy
tasks have generic controls that nucleus can choose to interact with if it
needs to, without any commitment to any particular workflow in effigy".
Encoding an orchestrator command into Effigy configuration was rejected
because it "may only exist on one machine".

- **Classification.** A task may declare `admission = "heavy"`. It changes no
  behaviour by itself and is exposed in `tasks --json` and `--plan --json`.
  Heavy is the only class for now; the schema stays open for more.
- **Gate hook.** When `EFFIGY_ADMISSION_HOOK` is set in the environment, by
  whoever launched the agent or shell, Effigy calls it before a heavy run with
  a small JSON request (run id, task, class, cwd). It waits, bounded, for admit
  or deny, and holds the admission until the run ends. Nested calls receive an
  opaque admitted marker and don't queue twice. With no hook set, heavy tasks
  run normally. An environment variable is the only mechanism, because it
  reaches agent-typed and nested invocations, which a flag cannot (Tom:
  "Whichever is cleanest").
- **Run control.** Every run, heavy or not, gets generic identity and control
  from the kernel run journal: a caller-chosen `--run-id` idempotency key,
  `runs` / `run status`, `run logs --follow` and `run stop`.

Retired with this: the Queue/Nucleus host-run client (`src/runner/host_scheduler`,
`effigy-host-run`), parent tokens, scheduler override, exit codes 75 and 77,
`--host-run-request-id` and `tasks request status|follow`, the `qa`/`ci`/`ci:fresh`
name rule, and contract
[049](contracts/049-heavy-validation-admission-contract.md)'s Effigy-side
client obligations.

Implemented by effigy#227 (2026-10-19): contract
[049](contracts/049-heavy-validation-admission-contract.md) holds the hook
protocol and run control, and `retired.toml` `effigy-host-run-client` the
retirement.

## Q-007 — Dev session shape

Status: answered
Asked: 2026-10-21
Answered: 2026-10-21 by Tom

Tom: "Python should not be needed. Minio should have been replaced with Silo.
The TUI should work effectively the same way regardless of hybrid or
container mode. We shouldn't need a `*:tui` selector at all."

- **One dev session.** The managed dev TUI looks and behaves the same whether
  the environment runs in hybrid or container mode. Placement is a property of
  the environment, not of the selector. There are no `*:tui` selector
  variants; the dev entry point itself is the session.
- **No consumer glue scripts.** Projects and bundles don't need Python (or
  other external-language) wrappers to run a dev environment. Resolved names,
  ports, URLs and trust material reach processes from Effigy, readiness waits
  and output following are Effigy's job, and bundle-specific setup steps are
  Rhai.
- **Object storage** in the Underlay bundle is Silo, not MinIO. That's a
  bundle change, owned by the Underlay side. Effigy's own baseline catalog then
  retired its built-in `minio` service in favor of `silo` (effigy#424; see
  Q-005).
- **Secrets before the TUI** (Tom, 2026-10-23, relayed by the Underlay owner
  after `effigy dev` in underlay-reference asked for the Vault passphrase
  inside the services tab): a locked vault prompts once, before the TUI
  renders. The session resolves secret access once and shares it with
  everything it starts (environment `up`, host processes, nested Effigy
  work), with no tab or child prompts and no persisted sealed credential.

Implementation is planned in the v0.15 environment lane (the
environment-owned host-process and unified-session work after effigy#235).

## Q-008 — Program fallback (for example oxide → cargo)

Status: answered
Asked: 2026-10-21 (Oxide owner, relayed via Queue decision b8c28d74)
Answered: 2026-10-21 by Tom: "Approve.. can the allow list be optional?"

Effigy offers a generic per-program fallback in configuration:
`[programs.<name>] fallback = "<program>"`, an optional `allow = [subcommands]`
and `force = false`.

- **When it applies.** If the named program can't be found on the inherited
  PATH, or `force = true`, Effigy puts a per-run shim on that run's PATH. The
  shim execs the fallback with the exact argv: no translation, no
  Oxide-specific flag handling, and no retry after an execution failure. Each
  run prints an honest notice that the original program's behaviour (for
  Oxide, managed storage) is lost.
- **What it covers.** Only name resolution through the inherited PATH. It
  doesn't apply to absolute paths, a replaced PATH, shell aliases or
  functions. A nested real `$CARGO` must never recurse into the shim.
- **Effigy owns** config precedence and trust, program resolution, the
  boundaries between global options, `--` and subcommands, the shim's lifetime
  and permissions, the refusals for config and allow-list violations, and
  focused tests.
- **The allow list is optional** (Tom). When present, unlisted subcommands are
  refused.
  - **An omitted allow list means unrestricted exact argv.** Tom, 2026-10-21, relayed by the Oxide supervisor: "I think this is fine, so long as the conditions are well documented in effigy help and the effigy skill". So `oxide clean` becomes `cargo clean` without Oxide's ownership checks, and cargo plugins and `cargo-<x>` exec run as cargo would run them.
  - **Mandatory condition:** BOTH `effigy help` and the shipped Effigy agent skill clearly document:
    - when the fallback triggers (the program is unavailable, or `force`);
    - that `force` only affects the inherited PATH;
    - the optional allow-list semantics;
    - exact argv, with no translation and no retry;
    - the loss of the original program's safeguards, including `oxide clean` → `cargo clean` and cargo plugin behaviour.
  - The supplied list (check, build, test, clippy, run, nextest, fmt, doc,
    bench) is only an example, not a guarantee.
- **Consumers** remove their `:cargo` selectors and CI switches only after the
  reviewed feature has shipped, and each owner scopes its own migration.

**Implemented** (effigy#250, 2026-10-21). `[programs.<name>]` is read from the
Effigy user config (`~/.effigy/config.toml`) and from no other layer: a project
`effigy.toml` that declares `programs` is refused as an unknown manifest field,
because program availability is a property of the machine, not the checkout.
The shim directory belongs to the run that activates it (inside the run's
`[scratch]` directory when one exists) and is removed at settle; a nested run
or step inherits it. The fallback is resolved to an absolute path on the
inherited `PATH` when the shim is written, so a shim cannot re-enter itself,
fallbacks never chain, and a nested real `$CARGO` cannot reach a shim. An
omitted `allow` is unrestricted exact argv; a present one matches only the
literal first argument and refuses, before exec, anything else including no
argument. Every condition above is stated in `effigy help`, in both shipped
Effigy skill copies, and in the user guide section
`docs/guides/069-workspace-host-integration.md`.

## Q-009 — v0.15 user-facing surface cuts

Status: answered
Asked: 2026-10-21 (planner, from the weight map and consumer usage scans)
Answered: 2026-10-21 by Tom, item by item; final "Confirm"

Breaking changes land in v0.15, the release that carries built-in migration
from v0.14.1, so that v0.16 doesn't need a second migration. Each retirement adds a
`retired.toml` entry and an actionable error.

- **Cut:**
  - `demo` and `[demos]`. The planner's usage scan only matched a literal `[demos]` header and reported one user, but Jetstream (90), Signal (8), Keepsake (6), Poodle (3) and Monkey (3) declare `[demos.<name>]` tables. Agents ran `effigy demo` 0 times in 60 days. Every demo is a command or step sequence, so Tom, 2026-10-22 ("Agreed"), kept the cut and ruled that demos convert to public tasks: `[tasks."demo:<name>"]` with the same `run`, the title and summary merged into the task `description` (added by effigy#271, Tom 2026-10-22), and `group = "Demos"` added once the project runs v0.15 (v0.14.1 rejects `group`). The retirement error and migration guide give that recipe, and each project converts before the v0.15 install;
  - the `draft`/`drafts` refusal routes;
  - the v0.14 task-status JSON fallback (migration imports old state once);
  - `[shell]`;
  - the experimental gateway listener-handoff commands (`setup-listener-handoff`, `listener-handoff-status`, `remove-listener-handoff`);
  - the `system` command;
  - `graph` and `docs context`. Tom: "graph - agree, cut". Agent transcripts over 60 days showed about 0.1% of shell commands, mostly prompted by guidance; median 10 s, frequent timeouts. `docs check` stays; scanners that need the code graph go with it unless the scanner assessment keeps them.
- **Consolidate:**
  - `system` lifecycle becomes `container [<name>] up|down|status|logs`;
  - `system repair` and `reset-runtime` move under `container profile`;
  - the `[systems]` table stays, because `workspace` and `exec` resolve through it;
  - `artifact` folds into `state`. Tom: "both surfaces were built as solutions for parts of deployment on the acowtancy project, but we've not actually made use of it yet"; the surface is revisited in v0.16;
  - `contracts check-json` takes its schema index from project config and shares one JSON Schema engine with Rhai validation, and `validate-selection` folds into it.
- **Release platform.** Tom: "I want a consolidated release platform in effigy that all projects can adhere to." One generic pipeline any project declares, run as one Run with checkpoints. Effigy-specific release machinery moves to Effigy repo tasks. `[distribution]` retires into the release platform's publish settings. v0.15 settles the shape; v0.16 adds publish providers. The release-platform contract is [054](contracts/054-release-platform-contract.md), accepted 2026-10-22 with its §10 rulings. Tom accepted the
  stage model (validate, prepare, gates/certify, publish, verify, closeout; one
  checkpointed Run; several versioned packages; build once and publish exactly
  that; tag and repo-task publish targets in v0.15) "so long as we can fit
  existing workflows to it". The contract maps every project's release workflow
  onto it, including libraries that don't use `[release]` yet, such as Poodle,
  Longhorn and Underlay.
- **Keep:**
  - `tasks qa-group`. The planner first proposed cutting it as "replaced by `runs`", but that premise was wrong. Group execution exists only through `tasks qa-group run` (contract 051: a group name isn't a task selector), Longhorn declares 18 groups, and agents ran it 383 times in 60 days. Tom, 2026-10-22, took the planner's recommendation to keep it as it is. Whether groups should become ordinary selectors is a v0.16 question;
  - `deps`: Tom: "the only way to test them in anger before release in consuming projects";
  - `workspace` and `exec`, the daily entry verbs;
  - trailing `<task> --plan` and `--json` through v0.15, with their note, and removed in v0.16. No released version has shown the note yet.
- **Keep and fix:**
  - `scan`: assess every scanner and remove or improve each. Tom, agreeing with the planner's per-scanner calls from the 2026-10 scanner assessment: keep and fix `god-files` and `duplicate-blocks`; cut `attention-markers`, `generated-in-src`, `comment-ratio`, `generated-assets` and `stale-suppressions`. The graph-backed scanners (`boundary-violations`, `dead-code`, `validation-gaps`) go with `graph`, with no graphless replacement; `effigy test --plan` answers which tests run;
  - `bootstrap`: simplify it, prove it on a selection of real projects, and align each project's `[bootstrap]`. Tom: "a single-command way to download and start another person's project".
    - **Shape:** `[bootstrap] setup = [<selectors>]` and `start = "<selector>"`, run as ordinary tasks.
    - **Cut:** `children` and its subcommands, `deps-sync`, `--db-seed` and dump prompts, `--fresh`/`teardown`, `--backend`, `--start`, and the `run` alias.
    - **Keep `submodules`.** Tom: "They would be valid in polyrepo setups".
    - **Add pinned extra sources.** Tom: "Pinned extra sources - yes please". A source is pinned to a commit and never silently advanced, and an existing working copy is never reset. This replaces `children`.
    - **First database seed.** A first seed is a setup task. Tom: CBS's seed "was commented out because of a race condition from a few versions ago - let's make sure it works now".
- **Not cut:** the internal restructuring from the weight map (extracting code from the binary crate, merging small crates). It isn't user-facing and can follow v0.15.

**Implemented** (effigy#258, 2026-10-23). `artifact` folds into `state`:
`effigy state artifact inspect|stage|capture` keeps every flag and the
`effigy.artifact.*.v1` payloads, and the retired top-level `effigy artifact`
fails with that replacement. `state capture` already owned layered captures, so
the standalone data-artifact verbs keep their own namespace rather than a second
meaning for `state capture`. `crates/effigy-artifacts` merged into `effigy-state`
as the `artifact` module (its only consumers were that crate and the binary), and
the Rhai `artifact` module became `state::artifact_inspect|artifact_stage|artifact_capture`. `container data seed` and
`container data dump` keep using the same substrate unchanged. Tom's v0.16
revisit of the deployment-data surface is untouched.

Each item lands with its implementing PR, which updates the owning knowledge.
That includes `vision.md` "Agent operations", which loses graph and docs context.

## Q-010 — Where runtime state lives (no central database)

Status: answered
Asked: 2026-10-22 (Tom, after the machine-wide `store refused: Busy` outage)
Answered: 2026-10-22 by Tom: "Yeah that's much cleaner"

Tom: "I don't know how comfortable I am having a central database for effigy..
it kinda goes against the principle of the project. I _do_ understand the
benefits, but it's already proved to be a single source of failure and is adding
_more_ churn to my drive".

On 2026-10-22 one machine-wide `~/.effigy/state/kernel.db` failed every project
at once: a close-time retention scan held the write lock for minutes. It also
grew by about 18,700 runs in four days and about 20 MB in 15 minutes. The ruling:

- **Short foreground runs write nothing durable.** A run that lives and ends with
  the invoking terminal keeps its record in memory. Its process group is the
  custody boundary, and its exclusive locks are lock files. No database write in
  the common case.
- **Runs that outlive the terminal record per checkout.** Detached runs, managed
  sessions, environments and containers keep a durable journal and custody in
  that checkout's own `.effigy/state/` (gitignored). A failure or lock affects
  one project, and removing the checkout removes its state.
- **A small machine-level store holds only genuinely shared facts:** gateway
  endpoints and routes, and host admission. It's low-traffic.
- This changes the state format, so it lands in v0.15, with built-in migration
  from the current machine store. Contract 052 is amended to match, and the
  kernel's crash-recovery guarantees still hold for every run that outlives its
  terminal.

The storage model is owned by [052 §3.1](contracts/052-runtime-kernel-contract.md#31-q-010-run-classes-and-the-durability-boundary),
with storage in §7, the v0.14.1 importers in §11 and acceptance proofs in §12.6; contracts
[053](contracts/053-environment-reconciler-contract.md) and
[049](contracts/049-heavy-validation-admission-contract.md) specify environment
and admission touch-points. Task status is owned by
[017](contracts/017-task-status-record-and-active-run-model-contract.md) and
[018](contracts/018-task-status-query-surface-and-read-model-contract.md);
[051](contracts/051-bounded-qa-groups-contract.md) preserves durable group
evidence. The runtime implements this classification, checkout routing and the
v0.14.1 importers.

Tom, 2026-10-22: "Approve" (planner relayed acceptance of the Q-010 design).
The conditions were owned by contract 052 §11.1 (busy-machine migration listed
exact blocking writers and stop/drain commands without partial work; ordinary
foreground runs continue) and §12.6 (read-only copy of the actual large store,
elapsed duration/per-checkout results, and deleted-worktree `retire --dead`
without a central index). The amendment and implementation shipped together.

2026-10-24: the current-main central-store importer itself is retired
(effigy#307). Once Tom's machine had migrated, he confirmed the schema existed
only on his machine and accepted removing the importer before v0.15. §11.1 is
now a retired note: the old store, any cutover manifest and any old
per-checkout import record are ignored, never changed and never deleted.

2026-10-24, foreground custody reports. Queue host runs whose Effigy process
died stayed `running` and held capacity, because Q-010 keeps foreground runs in
memory and losing every custodian leaves nothing to read (052 §3.1, 049 step 5).
Asked through Queue (decision b3538091), Tom chose the producer change: "the
admission hook reports the workload's real process group and start identity,
and Effigy records a terminal result for foreground runs." The resulting
exception is narrow:
- Hook-admitted heavy foreground runs report each workload group's PGID and
  leader `ProcessIdentity`, plus a terminal event, on the hook channel.
- Those runs alone write one terminal row to their checkout journal at settle.
- Reported identity is evidence for the provider's own lease, never recovery
  authority inside Effigy.
- Runs without a hook keep Q-010 unchanged.

effigy#325 implemented it: contract 049 owns the opt-in protocol
(`EFFIGY_ADMISSION_PROTOCOL=2`), the acknowledged reports and the coverage
witness, and 052 §3.1 records this note's narrow exception. The row lives in an
additive schema-1 `foreground_terminals` table in the owning checkout journal,
is retained for seven days, carries no process identity, and surfaces through
`run status` as `foreground-terminal-record.v1`.

Follow-up ruling, same day (Queue decision d3bad004 v2, Tom on the board):
"Yes: accept the escaped-process risk, and Queue releases automatically on the
evidence." A provider may therefore release its own capacity once all of these
hold:
- every reported workload group leader's `ProcessIdentity` no longer matches a
  live process;
- every reported group is empty;
- coverage is complete;
- no terminal result exists.

Tom accepted the risk that a process which escaped its group is still running.
Effigy itself still claims no closure from that evidence, and gains no
authority to signal or reclaim from it (052 §3.1).

2026-10-25 (effigy#337): every pre-reboot checkout refused every task run,
because `state/checkout.json` records the checkout's `st_dev` and macOS assigns
that number at mount, so a reboot looked like a replaced directory. Tom ruled out
hardening the comparison: "I thought we had simplified the kernel.db dependency?
This is exactly the sort of overcomplication and brittleness I wanted to avoid."
The ruling is the location-only rule, recorded in
[052 §7.1](contracts/052-runtime-kernel-contract.md#71-location-trust-and-clocks):

- A journal opens by its location, `<checkout>/.effigy/state/`. No recorded
  device, inode, checkout or Git generation can refuse it.
- No record follows: `checkout.json` and `machine-format.json` were deleted on
  2026-10-26 (effigy#372), so nothing is rewritten and a moved checkout, a
  recreated `.git` directory and a reboot all just work. Old import records are
  ignored entirely.
- Prior-boot runs are already history through the boot identity their records
  carry.
- Custody leases and socket receipts are identified by inode, never by the
  mount-time device number.

## Q-011 — v0.15 simplification before qualification

Status: answered
Asked: 2026-10-25
Answered: 2026-10-25 by Tom

A read-only audit of the rewrite (planner, HEAD fa2c39b60; full report under `~/.local/share/northstar/release-audits/effigy/complexity-audit-20261025/`) found the same pattern in kernel, gateway, containers and run engines. Effigy keeps its own parallel record of what's running, then builds guards and repair flows for when that record drifts from what the OS, Compose or the filesystem actually say. Since v0.14.1 the code grew by about 105k lines, and the week's incidents all came from that layer.

Tom: "We need to get this sorted out as a matter of urgency." His rulings:

1. **Simplify before qualifying.** "Yes. Obviously - why qualify a broken mess?" Feature work pauses until the simplification lanes land.
2. **The planner's recommendations stand:**
   - Parent-capability inheritance (`EFFIGY_PARENT_CAPABILITY` plus fd 9) is removed. This reverses that part of the Q-010 kernel design.
   - Admission protocol v2 (effigy#325) stays as shipped but is frozen: no further growth.
   - The containers generation journal (contract 053's generations, partial activations and admission context) is replaced with Compose's own state plus labels.
3. **In-flight fixes finish as they are** (effigy#336, #337, #338).

Rules for the simplification work:
- A task removes a mechanism and installs its simpler replacement.
- No new state files, records or refusal variants without a stated reason.
- Net lines go down.
- The planner proves the final candidate live on real consumer stacks (Acowtancy hybrid, inventors, a basic Effigy run, plus reboot, crash and restart, an interrupted `up`, a manifest edit and a deleted worktree) before claiming release qualification. Tom approved intermediate local installs on 2026-10-28 so he can test workflows early; those installations require a pinned binary, private-root smoke, Acow scratch-checkout compatibility check, timed rollback rehearsal and exact protected-state inventory. They are exploratory evidence and do not waive the final candidate proofs.

The effect journal around Compose apply stays deferred only if the exact
candidate's live interrupted-`up` proof passes without manual repair. A failure
reopens that mechanism before v0.15 release. Shared-service named-volume
continuity is a separate pre-release proof on Acow's existing MariaDB during
its authorized install window, with `shared-replace-proof.sh`; a matching
project-name hash alone does not prove the data survived. Tom amended the
installed-gateway observation gate on 2026-10-28: run a few hours of workflow
checks across the three Underlay bundles and consumers on the installed
candidate, with the machine gateway up and unrestarted. Run deliberate
reboot/crash journeys before this observation period. Every stop needs an
explained cause before release gates run; an unexplained stop blocks release
and gets a diagnosis task scoped to the evidence.
## Q-012: Second audit scope before v0.15 qualification

Status: answered
Asked: 2026-10-26
Answered: 2026-10-26 by Tom

A second read-only audit after Q-011 (planner, main b94b1061a; reports under `~/.local/share/northstar/release-audits/effigy/complexity-audit-20261026/`) found 1 P0, 32 P1, 54 P2 and 19 P3. Q-011 landed partly. A new gateway dead-route sweep deletes live routes. No live proof covers the Compose shapes that broke Acowtancy on 2026-10-26. The v0.15 changelog and migration guide don't describe what consumers will meet.

Tom's rulings:

1. **Before milestone QA and v0.15:** the P0/P1 fixes, the proof, selector and `qa:json` gaps, the documentation work (the `[Unreleased]` changelog rewritten as the change since v0.14.1, guide 085, stale contracts), and the low-risk deletion batches D1–D4. The effect journal around compose apply, the gateway private-fixture lifecycle (`identity.rs`) and the listener handoff chain wait until after v0.15.
2. **Admission:** the hook-liveness re-verification (`verify_hook`/`verify_held`, the inheritance capsule) is removed. The `HELD` markers and the advisory `EFFIGY_ADMITTED` stay. This is a narrow exception to Q-011's admission-v2 freeze.
3. **Upgrade path:** the v0.14.1 environment importer and the PID-only v0.14 refusals are deleted. A consumer upgrading straight from v0.14.1 gets one container recreate per stack. Condition (Tom, 2026-10-26, for Dan and other v0.14.1 users): the importer goes only after a live proof shows v0.15 reusing v0.14.1's named volumes with data intact. If any volume or project name differs, the naming compatibility (or the importer) stays. Guide 085 documents the validated upgrade steps.

4. **Isolated dirs and Oxide output links:** Effigy keeps refusing to follow a symlink on an isolated dir. The fix is on the Oxide side: projects adopt Oxide's profile member links, so `target` stays a real directory. Tom is running that adoption sweep across all projects. Effigy's refusal message names `oxide outputs adopt`.

Tom, 2026-10-27 (decision 4dd2e8b8), amended Audit2 L's live-proof gate: prove the local catalog MariaDB named volume and row survive the v0.14.1-to-v0.15 upgrade first; track shared-volume continuity separately. The fresh v0.14.1 shared MariaDB baseline failed before any row could be written: its render wrote the init SQL file at the shared render root while its Compose bind named a missing nested path, which became a directory and made MariaDB exit. The failed attempt grants no data-continuity credit. The local proof covers both down-first and running-stack recreate with the same volume and rows; it does not cover shared services. Evidence: `/Volumes/Builds/scratch/effigy/r2a8d-live/logs/cycle-ab-v6.log` (SHA-256 `ab087bbd8eacfd446e5997637d88465181e89fe7f4f204720ec5428dee1a4a20`).

The separate shared-volume proof remains a v0.15 pre-release gate: use the
existing Acow shared MariaDB, retain its named volume through the authorized
install-window replacement, and run `shared-replace-proof.sh` on the exact
candidate. The golden old-binary project-name vector is a unit compatibility
check, not live volume-continuity evidence.

## Q-013: Test-suite value before v0.15 qualification

Status: answered
Asked: 2026-10-26
Answered: 2026-10-26 by Tom

A read-only test-value audit (planner, main 9adacdd; reports under `~/.local/share/northstar/release-audits/effigy/test-audit-20261026/`) covered about 5,500 tests. It found:
- about 440 to delete, about 1,000 to merge into about 220 tables, and about 345 to rewrite. That's around 27k test LOC, plus about 3.3k LOC of dead production code that only tests keep alive;
- container fakes that never check rendered Compose, and gateway tests that only run the private-state-root path. None of the 2026-10-26 consumer breaks would have failed a test;
- 45% of the kernel tests sitting behind a `fault-injection` feature that nothing enables.

Tom asked whether everything "is actually testing something useful". His rulings:

1. **Prune before qualification.** Four prune batches run before the selector/proof work and milestone QA.
2. **Managed matrix.** The end-to-end `managed_matrix_*` tests are trimmed to one cell per front end and stop kind. The full cross product stays in the kernel model test (contract 052 §12.3).
3. **Fault-injection tests run at milestone QA.** A `fault-injection` selector joins the milestone `qa` group. Whether to cut the fault framework is decided after one milestone shows what it catches.

## Q-014: Custody anchor lifetime

Status: answered
Asked: 2026-10-27
Answered: 2026-10-27 by Tom
Owner: [contract 052 §5.7](contracts/052-runtime-kernel-contract.md#57-ownedprocess-and-signalrouter)

Tom authorized custody anchors to exit on SIGTERM or once their run settles.
The earlier rule that anchors ignore TERM indefinitely is no longer the target
behavior. Their positive group-identity evidence remains necessary until
settlement proves closure; losing an anchor earlier leaves the existing
`Unknown` outcome for still-live members. This ruling adds no machine-wide
sweep or new state. It covers direct Effigy proof invocations as well as marked
test selectors; Queue lead `d7ddb7fe-adc4-40e2-a9f7-1d5b98043830` records the
observed gap.

Implemented in effigy#388. `__kernel-anchor` now ignores only SIGINT and
SIGHUP, so TERM retires an idle anchor immediately, and settlement still stops
the anchor last after group closure. `idle_anchor_exits_on_term_within_a_bounded_grace`
proves the bounded exit,
`a_completed_invocation_leaves_no_custody_anchor` and
`a_completed_hand_run_leaves_no_custody_group_behind` prove a completed direct
invocation leaves no anchor, and
`custody_deleted_checkout_lease_survives_owner_kill_until_positive_group_closure`
still proves a deleted checkout retains custody until positive closure. The
built-in `effigy test` route now also owns the suite sweep described in
[test fixture hygiene](test-fixture-hygiene.md).

## Q-015: Argument-scoped task lock for focused tests

Status: answered, implemented (effigy#391)
Asked: 2026-10-27
Answered: 2026-10-27 by Tom
Owner: [contract 017 §Exclusion](contracts/017-task-status-record-and-active-run-model-contract.md#exclusion-k2)

Tom chose an opt-in lock key consisting of the resolved selector and normalized
task arguments. It is enabled for `test:focused` only: different test filters
may run concurrently, while identical normalized invocations serialize. The
second identical run waits about two minutes by default, with the existing
explicit wait option retaining precedence. A `test:focused` invocation with no
task arguments keeps the plain task gate, and no general shared or read-only
lock mode was added. Guide
[020](../guides/020-dag-lock-policy-baseline.md#lock-scopes) carries the
behaviour.

## Q-016: Colima visibility for Builds scratch

Status: answered, host change pending
Asked: 2026-10-27
Answered: 2026-10-27 by Tom
Owner: [guide 069 §Per-Run Scratch On An External Volume](../guides/069-workspace-host-integration.md#per-run-scratch-on-an-external-volume)

Add one writable `/Volumes/Builds/scratch` mount to the `effigy` Colima
profile during the separately authorized Effigy install window, after Acow
authorizes that window. Do not mount `/Volumes/Builds` as a whole: the VM must
not gain access to `oxide-pool`, `oxide-outputs`, `caches` or `queue`. A missing
scratch mount at VM start must fail loudly, without fallback. The install
receipt records the exact change and undo, plus before/after `colima list` and
mount evidence. Until then live Colima fixtures keep the bounded `~/.cache`
staging exception. Once the mount works, move those fixtures to Builds scratch
and close papercut `62e6b400-03df-411b-9368-846ccffbad4a`. Barge may later
own this under Barge Q-009; that does not change the interim install gate.

Assessed 2026-10-31 (effigy#515): no repository-side fix. Effigy writes no
Colima mounts, and the `effigy` profile's `colima.yaml` still has `mounts: []`.
The change stays a host edit behind a profile restart, so it waits for this gate.

## Q-017: Workspace client and operator feedback at v0.15

Status: answered
Asked: 2026-10-27
Answered: 2026-10-27 by Tom
Owners: Queue lanes `v015-workspace-client` and `operator-feedback`; [product guardrails](architecture/product-guardrails.md#human-facing-feedback)

Full container-mode workspace-client coverage remains a v0.15 release gate:
Acowtancy and CBS must pass the lane's coverage matrix, and the authenticated
host channel must be qualified on Colima, Docker Desktop and Linux Docker
Engine. Hybrid mode must provision no guest Effigy. The plan's `later` horizon
orders this work after Q-011 simplification; it does not waive the release
criterion.

Tom's 2026-10-31 creator-attribution amendment keeps the workspace client in
v0.15 with the narrow, gate-scoped ID-set rule in
[contract 052](contracts/052-runtime-kernel-contract.md). It forbids adopting a
container from another checkout, project or stale generation and requires a
complete before/after exact-ID set under the exclusive environment gate. A
same-user actor able to create an identically labelled container during that
window is explicitly outside the v0.15 threat model. This does not qualify a
backend or waive the native Colima, Docker Desktop and Linux Docker checks.

The complete human-facing feedback ruling also blocks v0.15. Every command
plane must show the live, subject-named terminal steps and actionable errors
defined in the product guardrails. Existing feedback work (#312 and #315) is a
starting point, not automatic qualification of every plane. The planner
checks the exact candidate and briefs any observed gaps before release gates;
the full spinner rollout is not deferred to a later version. State apply and
capture now emit per-layer live steps from those existing layer statuses; real
SQL-layer spinner timing still needs an exact-candidate proof and is not
qualified by the source labels alone.

Tom clarified on 2026-10-29 that a fixed spinner label is UI noise, not
progress. Every spinner must show live state as the operation advances: its
current sub-step or object, known counts, elapsed time, and the last meaningful
event or exact dependency/probe being awaited. When no further event is
observable, show elapsed time and the exact thing awaited rather than a generic
"working" label. This applies across command planes, including managed TUI
tabs and their text/JSON equivalents, and is a v0.15 gate. Managed entries now
derive each wait from the resolved ordering graph (`start` order, `after =
["environment"]`, host `depends_on`, accepted legacy `start_after_ms`), publish
it as `effigy.managed.entry-wait.v1` under `.effigy/runtime/waits/`, and project
the same state to the tab, the headless text and `status --json` `waiting_on`
(effigy#458; [product guardrails](architecture/product-guardrails.md#human-facing-feedback)).
The source-wide
audit is recorded at
`/Volumes/Builds/scratch/effigy/feedback-spinner-source-audit-20261029.md`.
effigy#467 executed it: every flagged site is a live-state step or an
immediate one-line event, `lint:feedback` has no allowlist and rejects any
fixed-label spinner entry point in production, and each site's disposition is
recorded in
[live progress sites](architecture/036-live-progress-sites.md).

On 2026-10-30 the Effigy Supervisor deferred the whole managed-pane ANSI
colour and plain-log projection task (#457, PR #616) to v0.16. Its producer
writes directly to the canonical host-process log and bypasses the new custody
sanitizer; colour must remain disabled until that writer has a safe
secret-handling boundary. PR #616 stays merge-held. This specific colour
feature is outside the v0.15 pre-QA list and is not a prerequisite of the
remaining Q-017 live-state progress gate (#467). Queue decision
`084d2652-2ada-4e28-8bcd-743d7fa85cc3` records the ruling; security lead
`a52a71e5-8f58-4c9a-a4c7-48c8166fe6aa` owns the later writer design.

Tom confirmed on 2026-10-30 that managed Cargo/Oxide colour is required for
v0.15, reversing its earlier deferral. Qualification requires a single
writer-side custody boundary for plain sanitized/redacted host logs and a
redacted styled live tap, while preserving direct path/offset followers. See
[the colour boundary ruling](architecture/product-guardrails.md#human-facing-feedback). Tom's same-day cutover ruling permits one
new-inode quarantine of an untrusted legacy prefix under the settled host
owner gate, with durable ready provenance for subsequent Effigy reads. The
Underlay follower remains a presentation reader with inode/offset gap handling;
it does not enforce ready provenance. Stop/reopen older attached followers:
POSIX cannot revoke their already-open descriptors.
The owner clarified that Underlay/direct file reads are presentation, can show
safe bytes awaiting sync and acknowledge no durability; Effigy-owned readers
must honor the durable committed extent and hide failed/torn suffixes.
The manual hold remains
until combined writer, security, style and bundled follower proof and review.
On 2026-10-31 the owner approved a bounded boolean `CLAUDE_CODE_*` launcher
switch namespace instead of expanding exact flag lists. Secret-bearing name
segments, schema-sensitive declarations and known-secret aliases override it;
non-boolean and outside unknown values stay Secret. The custody argv guard
remains unchanged; contract 052 records the complete value-domain boundary.



## Q-018: Primary Linux proof environment for v0.15

Status: answered
Asked: 2026-10-27
Answered: 2026-10-27 by Tom
Owner: [v0.15 roadmap](v015-roadmap.md#sequence-and-gates)

Use a fresh, snapshot-backed Parallels Ubuntu arm64 VM as the primary Linux
Docker Engine and service-supervision proof environment. Bazzite over SSH is an
optional second-distro check. A Docker container does not stand in for the
primary reboot, privileged-port and service-lifecycle gate. Do not reuse the
existing invalid Parallels VMs. This chooses the proof environment; it does not
authorize creating or reconfiguring a VM during planning.

Tom's Parallels automated option installed Ubuntu 26.04 arm64 instead of the
originally proposed 24.04. The VM is currently named `Ubuntu 26.04 ARM64` and
stored at `/Volumes/Vault/Parallels/Ubuntu 26.04 ARM64.pvm`; it is still being
set up. The intended `clean-base` snapshot also belongs on Vault. Confirm Vault
is attached before every Linux proof window, check its throughput before
timing-sensitive proof, and
shut the VM down cleanly before removing the drive. Do not run Linux proofs
unattended without confirming the drive is present. Rosetta may run an x86_64
smoke test inside the arm64 guest, but that is not native x86_64 qualification;
a separate x86_64 machine would be needed for that optional coverage.

## Q-019: Linux operator gateway removal and same-user interference

Status: answered
Asked: 2026-10-29
Answered: 2026-10-29 by Tom
Owner: [Gateway upgrade and recovery](architecture/034-gateway-legacy-upgrade-recovery.md)
Decision: `663f311f-4f55-4df3-96ac-af6c597f6b1c`

Tom accepted the same-UID staged-entry deletion limit found during Effigy
#448's review. A process running as the operator can already replace the
operator's unit file or installed binary. This is a same-user trust limit, not
a privilege boundary. Keep the one-administrator-action removal route and
its exact-identity protections: never unlink the shared unit name, and
preserve and report a replacement that appears there. The accepted limit
does not waive the separate daemon-reload failure handling or native Linux
qualification.

## Q-020: Release secret-audit history boundary

Status: answered
Asked: 2026-10-29
Answered: 2026-10-29 by Tom
Owner: [v0.15 roadmap](v015-roadmap.md#sequence-and-gates)
Decisions: `1b111aba-0a64-42ae-834b-02672384ae85`,
`be22a199-227a-49a4-8558-a344f2e61409`; lead
`56722ba6-ace5-4dba-8a1d-0fb8a233fea9`

Tom selected an explicit pinned lineage boundary, a scan of the current
tracked tree, and an allowlist restricted to exact known fixture paths and
content hashes before the candidate selector becomes a release gate. That
selection followed an empty `git merge-base v0.14.1 HEAD` in a shallow
checkout. After fully fetching the history on 2026-10-29, the repository is
not shallow. The tag resolves to commit
`20465f1ff690340ee9ef461915ca769da1c76fca` and is an ancestor of
`main` (`git merge-base --is-ancestor` exits 0); the first mainline commit
after it is `7b26a1579a4991248014fd3352f9ae3093ae99fa`. The actual Git
root is `9dae95a45512bf6368009f7f898c04cb07caf6c3` (2026-02-28), before
v0.14.1. The earlier empty result was a shallow-history artifact, so this
is a continuous history rather than an unrelated rebuilt lineage. Tom
confirmed `v0.14.1` at `20465f1ff690340ee9ef461915ca769da1c76fca`
as the explicit audit boundary. The release gate must pin that tag and
expected commit in its configuration, scan tag-to-candidate history and the
candidate tracked tree, and refuse a shallow checkout, missing tag or
unreachable boundary. Only exact known fixture path/content-hash matches may
be allowed; other findings and their values remain protected. This ruling
does not authorize a release run, tag or publication.

The #453 baseline scan found six distinct managed-run stop identities in
`owner.out` and `restart.out` under the two historical
`crates/effigy-parent-fixtures-gateway-*` fixture roots at `4b0aafe9f`.
These are run-scoped settle identities, not durable machine credentials: the
stop path matches a token to that run's live gate and session. On 2026-10-29
both fixture roots and all recorded session directories were absent; four
recorded PIDs were absent and the fifth live PID belonged to a different
process. Two logs explicitly record a stopped session. None of those six
identities can authorize a stop of a current run. #453 may classify only the
six historical fixture values by exact path and content hash. Their values
must never appear in audit output, review text or documentation. This does
not exempt other tokens, private keys or credential-shaped strings; every
other match needs its own fixture proof or remediation before the gate passes.

The same historical fixture roots contain two generated private gateway CAs
and their localhost leaf keys. The fixture's mkcert shim creates them under
its private `CAROOT` and explicitly rejects `-install`; on 2026-10-29 neither
fixture CA was present in this Mac's default, system or system-root trust
stores. The fixture roots are gone. The release audit may exempt only those
exact historical fixture key paths and content hashes, never a general PEM
pattern or a key from the active tree. If evidence later shows either CA was
trusted elsewhere, revoke that trust and reopen this disposition.

During #453's inspection, a raw historical grep printed six already-committed
fixture token values into the private agent tool transcript. Do not copy those
values into receipts, reviews or documentation; future inspection must emit
only redacted rule, path, commit and content-hash evidence.

For source-code matches, a literal can be classified as synthetic from its
construction and use in a named test or fake adapter; external provenance is
not required for a value written directly as a fixed test input. Record the
test/function and the use that makes it a fixture, plus the exact path and
blob hash. A test directory or a credential-shaped string alone is not that
proof. Current-tree examples include the route-token tests in
`policy/hosts.rs`, the gate-token tests in `custody.rs` and
`runtime_routing.rs`, the Rhai secret batch test, and the managed placement
argument-redaction test. Apply the same source-context check to every
historical revision; a production literal or a value of unclear origin
remains a finding. A changed blob gets scanned again.

## Q-021: Guest facts the workspace client channel still needs

Status: open (kernel owner seam implemented by #488; guest identity, transport and glue implementation and qualification outstanding)
Asked: 2026-10-29
Owners: #482 (C identity/context/publication), #483 (A transport), #452 (glue).
Authority: [contract 052 §8.11](contracts/052-runtime-kernel-contract.md#811-workspace-client-interface-v1).

Tom's 2026-10-30 ruling (decision 8c0ef812) bounds the normative seam to 250 lines,
tied to the merged #484 reservation/publication APIs. The detailed E-step,
crash/fault/retry and ownership-transfer draft is explicitly
[non-normative, unimplemented/post-v0.15 candidate material](contracts/052-runtime-kernel-contract.md#appendix-b-non-normative-post-v015-sequence-candidates),
with no acceptance claims. It does not qualify any backend or waive Q-017.

Three production facts remain to implement and prove:

- **Guest-local identity.** Observe the full container id, boot, platform and
  libc independently of the context, using qualified guest-local sources.
  Cgroup is diagnostic only under §8.2: even a matching full cgroup id cannot
  prove placement. Cgroup-only availability refuses; backend fixture/native
  qualification must distinguish it from a qualified mountinfo observation.
  The host selects the first artifact from the bound image; executing it verifies
  that selection. Prepare compares the staged binary's own observation with the
  host-bound target before publication. Unknown/conflicting facts refuse before
  credential access; no Docker socket, fabricated identity or context-as-proof.
- **Qualified TLS locator.** One session-owned key/listener; verify SPKI pin and
  key possession before Hello or any credential frame. Colima, Docker Desktop
  and Linux Docker each need bounded local and native qualification. §8.11 pins
  the typed alias/bind → session key/listener → qualified locator handoff and
  reconnect interface; only the verified locator reaches context publication.
  A fresh Linux project renders its alias before creation, applies to create the network
  and guest, then resolves/probes from that guest; it never needs the missing
  network first or guesses its IPAM gateway as the daemon's host-gateway address.
- **Actual session publication.** Use #484's reservation, frozen
  `SessionBinding {owner, run, generation}`, `admit_reserved`, typed publication
  receipt and atomic commit of active(E), mapping and Borrow. The immutable v2
  context publishes the actual session run and S, separate from environment E.
  Preserve canonical Task-before-Env order and the predecessor's live holders.
  The kernel seam for all of this landed in effigy#488
  (`WorkspaceActivationReservation::decide`, `commit_selection`,
  `Attachments::selection_switched`, `unfinished_selection`,
  `selection_recovered`/`selection_abandoned`, and `SessionLaunch`/
  `ChannelBound`/`ServingLease` custody): the atomic attach-or-reserve decision,
  the durable `SelectionReceipt`/`SelectionObligation` commit (the only commit
  that publishes a generation: it needs the session's live launch, a parsed and
  validated v2 context document, and inputs a decision froze — a fresh
  project's unselected artifact is frozen by its first publication), the
  per-selector readback switch/recovery order (the kernel reads selectors and
  objects through a `SelectorReader` under `Gate(Env)` and records only what it
  observed) and the settled-successor abandonment are implemented and
  fault-proven against the real journal, including concurrent decision callers.
  Listener/key custody is rooted in the detached session owner: a launch needs
  a mutable admitted session `Run` and its exact `SessionRef`; `SessionOwner` is
  identity evidence, not duplicate gate custody. Both `selection_switched` and
  `selection_recovered` require the retained `SessionLaunch`; recovery also takes
  target and serving probes. `SelectionReceipt::new` takes the session Run and
  an exact-target prior-selector reader, while `SelectorReader::target` binds
  readback to the committed backend/container/boot. Deserialized receipts carry
  no constructor-validation authority. What remains for #482 is the container/runner typed wrapper and
  guest selector switching, for #483 the transport, server-key generation and
  TLS qualification, and for #452 the glue and supervisor ownership transfer. The
  detailed appendix is still non-normative.

Recovery judges listener liveness independently of session and target. A bounded
pinned TLS-only probe must be available while selection is unfinished, including
Holding; it opens no channel/admission and carries no application or credential
frames. Complete only with all three facts verified. With an established dead
listener or lost target, stop/settle the exact live successor on its own private
slot before abandonment; a dead session needs no stop. Unknown liveness remains
unresolved and grants no publication/admission. Abandonment cannot refuse solely
because session and target remain live when the listener is established dead.
Never stop a predecessor to clear its successor or rebind an old endpoint/pin.

Code-first tasks own crash-family, concurrent-decision, fresh-network and
mispinned-peer zero credential/Hello-byte proofs. Proposed test names and #452's
239 fake tests supply no production evidence. Q-017's Colima, Docker Desktop and
Linux Docker qualification gate remains; safe refusal is not qualification.
Rootless Linux and Apple Containers have no qualification claim. No production
code or stop condition changes in this design-only seam.

## Q-018 — Emulated execution and default Colima gateways have no observation

Status: open
Asked: 2026-10-30
Owner: none yet; the reserved publication input builder (#506) refuses rather
than guessing

`052` §8.11 requires the host to select the guest artifact from the exact
container image's backend-reported OS, architecture and variant plus one observed
image loader family, and to claim execution only for native execution or verified
emulation. The image half now has an owner: `container_runtime_prep` reads the
platform from the backend's own `runtime image inspect` (via
`effigy_containers::exec::inspect_image_for_policy`) and the loader family from
the image root, and `#506` cross-checks the guest's diagnostic machine type
against it.

Two facts still have no observation owner, and `#506`'s builder refuses rather
than inventing them:

* **Verified emulation.** Nothing observes whether a backend can execute an image
  built for another machine type. A guest whose image platform differs from this
  host's machine type therefore refuses as unproven, so an emulated image cannot
  publish a reserved workspace. The missing signature is a backend
  emulation-qualification fact per image platform.
* **A profile's guest-side gateway when the profile configures none.** The
  renderer keeps Lima's `192.168.5.2` default for pre-start rendering;
  `runtime_gateway_observation` reports a configured address, an absent profile,
  an unreadable, oversized or non-regular one and a profile that configures no
  gateway separately, and the reserved publication refuses all but the configured
  address. Until an owner observes the
  address actually reachable from the guest for a default-configured profile,
  such a Colima workspace cannot publish through the reserved path.
* **The render preflight seal (answered).** #509 supplies
  `Objects::seal_for_preflight(cx, root, closure, deadline) -> SealedPreflight` and
  `Objects::publish_sealed_desired(run, sealed, closure)`; the reserved preflight
  seals the plan's frozen render off-gate inside the publication deadline, retains
  the receipt, hashes the sealed bytes, and the one apply promotes that exact
  receipt through `EnvRun::publish_sealed_desired` under `Gate(Env)`. Nothing
  canonical comes from the seal itself.
* **A container's creator — still blocking the fresh-target path (pending
  effigy#508).** No owner records
  "this run created container X": the engine's apply effect names the engine,
  context and project, Compose's labels name the project and service, and a
  creation stamp inside this activation's window only proves *when* the container
  appeared, not *who* created it — a competing Compose actor creates the same
  project, service and labels. The reserved publication therefore refuses every
  fresh target (`TargetEvidence::Absent`) at decision time and again at
  observation, and names the missing signature:
  `EnvironmentBackend::created_by(project, container_id) -> Observed<RunId>`, a
  backend creation receipt naming the run and the exact full container id. A
  re-observed target is exact identity matching and publishes normally, so a
  workspace can publish after its container exists; the first up of a brand-new
  environment cannot publish through the reserved path until that owner exists.
  #508 owns the narrow backend receipt; timestamps, before/after absence and
  matching Compose labels do not establish creator identity, so this refusal stays
  until that receipt is integrated.

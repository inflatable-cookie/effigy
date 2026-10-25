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
  bundle change, owned by the Underlay side.
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
  endpoints and routes, the port registry, and host admission. It's low-traffic.
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
now a retired note: a machine which committed that cutover ignores `kernel.db`
and never deletes it, and one still holding it without a manifest is refused
once with the Effigy main `7b82d26` upgrade path.

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

# Changelog

All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/).
During v0.x, MINOR bumps may include breaking changes.

## [Unreleased]

### Breaking
- Local custody receipts add a process-group anchor. Recovery can stop
  descendants that cleared their environment after their leader was reaped.
  Each group retains one extra process until settle; a killed anchor or an
  older receipt still leaves unverifiable nonce-free survivors as residue.
- Managed tasks (`mode = "tui"` with `concurrent`) run under one detached
  supervisor per run (effigy#235). The supervisor admits the run in the kernel
  journal, then owns setup, gateway, environment and workloads, and one settle
  is the only cleanup. The TUI, `--headless` and the new `--detach` are front
  ends: closing the terminal, SIGHUP, SIGINT, SIGTERM or SIGKILL to an
  attached client stops and settles the run, and `--detach` returns once every
  workload has started. `--headless` prints the run's own lines and every
  workload's output as `[name] line`, as before. `<task> stop` and an
  attached client exit non-zero when the settle leaves residue.
  `.effigy/runtime/managed/` session files, stop markers,
  lifecycle state files and the generated lifecycle shell with its EXIT trap
  are gone. `<task> status --json` prints `effigy.managed.status.v1`, and
  `<task> logs lifecycle` reads the run's own output. The environment comes up
  and passes exec readiness, observed to the readiness budget, before any
  workload starts, so `start_after_ms` no longer has to wait for it;
  `health_wait` routes are probed once the workloads that serve them have
  started. A run that holds an environment stays up when
  its workloads exit, until a stop. A live v0.14 headless session is refused
  with its stop command. `--run-id` is refused for managed tasks.
- Container tasks, sequence container steps, built-in `exec` steps and
  named-container managed setup run under kernel remote custody: the run
  journals the container workload before it starts, a stop, signal or timeout
  reaches the container session (not only the local `docker`/`colima`
  client), and the run settles only once that session is observed closed.
  A killed Effigy is settled on its next run from what the backend reports.
  A step whose exec client dies while its container session runs on is
  recorded as interrupted, not with the client's exit status, and the session
  is stopped.
  Values for the container never appear in a command line. A value that spans
  lines can no longer be passed to a container workload and is refused.
- A built-in `exec` step in a sequence reports the command's own exit code and
  streams under `--json`, instead of exit 0 with an `effigy.exec.v1` envelope.
- Managed container `setup` steps without a lifecycle entry run in the typed
  sequence engine: each step is a child run, task references resolve on the
  host, and `run` steps execute at the task root like other sequence steps.
- Rhai steps of container-routed tasks run as host steps under the run's
  custody, like those of host tasks.
- Container tasks get a terminal when Effigy runs in one, and keep the
  operator's stdin otherwise. On Colima the container gets stdin but no
  terminal.
- Container environments now have one kernel record each (contract 053).
  Every stop, restart, recreation, reset, handover and idle stop is a
  transition that refuses while any run holds the environment, except that
  run's own stop. Container tasks, `exec`, shells and data commands hold their
  environment while they run; `container down` under a live `dev` names the
  holding run and changes nothing.
- Idle stop is holder-based: a stack a task starts stops when the last run
  using it ends, not 5 minutes later. `effigy container up --detach` keeps a
  stack running; `EFFIGY_DISABLE_HOST_CONTAINER_LEASE=1` keeps task-started
  stacks running. The lease file, `EFFIGY_HOST_CONTAINER_LEASE_TIMEOUT_SECS`,
  `EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER` and the hidden
  `__container-lease-reaper` command are retired.
- Only runs that hold an environment protect it. A live host task that does
  not use a stack no longer blocks that stack's stop, recreation or mount
  reconcile; container tasks, `exec`, shells and data commands hold theirs.
- The exec-readiness restart refuses while a run holds the environment and
  reports the readiness failure instead. A changed configuration under live
  holders keeps the running generation until they end, with a warning.
- A failed managed run's environment is no longer torn down by its
  successor's start; the kernel settles the failed run and the successor's
  bring-up reconciles the stack.
- Existing v0.14.1 stacks are imported automatically on their first bring-up
  or stop: lifecycle markers and labels become the environment record, running
  containers are adopted without a restart, and the marker files are never
  changed. A stack whose ownership cannot be proven is not adopted and needs an
  explicit `container down`.
- Heavy tasks no longer go through the Queue/Nucleus host-run client
  (Q-006). `admission = "heavy"` is metadata only: with no hook set a heavy
  task runs like any other, CI runners included. When the launcher sets
  `EFFIGY_ADMISSION_HOOK`, a heavy run calls it with an
  `effigy.admission-request.v1` request in `EFFIGY_ADMISSION_REQUEST` and
  waits, bounded by `EFFIGY_ADMISSION_HOOK_TIMEOUT_MS`, for `admit` or
  `deny <reason>`. The admitted hook is held under the run's custody until
  the run settles; deny, timeout or a crash refuses the run before its
  command starts (`effigy.admission.v1` details). Nested heavy runs inherit
  `EFFIGY_ADMITTED` and never queue twice. `qa`, `ci` and `ci:fresh` are no
  longer heavy by name. Exit 75 now means only a held task lock; 77 is gone.
- `[containers.<name>].compose_file` is deprecated. For one release each
  Compose service is read as a project catalog fragment, with a warning.
  `effigy container migrate [<name>]` writes the fragments to
  `infra/dev/catalog/<container>-<service>/` and replaces the key with
  `[containers.<name>.services]`; it checks the render is unchanged. Translated
  environments behave like catalog environments: services carry checkout and
  scope labels (the first `up` recreates them once), named volumes become
  `<project>-<volume>` (old `<project>_<volume>` volumes are left alone),
  published ports get Prepare-assigned host ports on the publish address unless
  `[host].ports` fixes them, and `data`, `cache` and `reset` commands apply. Compose features a fragment cannot
  express refuse with the key to move. Effigy's own `linux-release` builder is
  migrated.
- `effigy container eject` and `container::eject` are retired: generated
  Compose stays internal. Use `effigy container render <name>`.
- The catalog-pack runtime is retired (Q-005). `effigy service pack`
  (`status`, `install`, `update`, `rollback`, `reset`) refuses with the
  replacement, the workspace pack handoff is gone, and
  `release.sync-catalog-pack-support-policy` is a manifest error. Installed
  pack state in `~/.effigy/catalog-packs/` is ignored with a notice and left
  untouched. Pack v1.2.0 was byte-identical to the compiled baseline, so
  existing projects resolve the same services.
- `effigy service list --json` is `effigy.service.list.v2`: fragments carry
  `source`, `layer`, `path`, `qualified` and `shadows`, and the pack
  `selection` object is gone. Text output labels the compiled catalog
  `baseline` instead of `bundled`.
- Each environment's backend is recorded in the kernel store when its
  generation is prepared, and every Compose, volume and data action uses it.
  Volume actions no longer infer Docker or Colima from the profile name, and
  shared services start on their environment's backend. The v0.14
  `.effigy-runtime.json` record is imported and left untouched; an unreadable
  one refuses volume and data actions until `EFFIGY_COMPOSE_BACKEND` names the
  backend. `container status` reports `backend_source`.
- Reset, cache prune and data import/export act only on volumes whose Effigy
  labels name the environment's project; a same-named lookalike is reported and
  left alone.
- Tasks now have `visibility = "public" | "internal"` and optional `group`.
  Human inventory, general/work help and completion show public tasks; `tasks --all` and
  `tasks --json` include everything. Unnamespaced entry points stay public;
  referenced namespaced helpers default to internal. Internal tasks run by name.
  The draft task namespace is removed: `[drafts]`, `draft` step keys, QA
  member `surface = "draft"` and `effigy draft`/`effigy drafts` are refused
  with their replacement. Move each definition under `[tasks.<name>]` with
  `visibility = "internal"`, replace `draft` steps with `task = "<name>"`,
  remove lifecycle metadata, and run the task by name.
- Host task and setup sequences now use one typed engine in text and JSON.
  Steps are child kernel runs; custody settles descendants on timeout, stop,
  INT, TERM, HUP and owner loss. JSON adds per-step outcomes and preserves the
  same exit code as text. Nested task cycles fail before gate acquisition;
  shared locks held by an ancestor are inherited. Nested steps inherit the
  top-level admission decision, so a direct sequence reaching `qa`, `ci`,
  `ci:fresh` or an explicitly heavy step does not queue again.
- Sequence `depends_on`, `retry`, `retry_delay_ms` and `fail_fast` are removed.
  Delete these keys and order steps explicitly; sequences stop on the first
  failure. Container task steps retain legacy execution and journal child
  outcomes pending the remote-custody cutover (#236). Container setup embedded
  in lifecycle-managed commands retains process-level outcomes until #236,
  preserving its existing readiness and startup order.
- Task runs are now kernel runs. Every invocation resolves once to a task
  identity (catalog path relative to the root, task name), so `build` from
  inside a catalog, `api/build` and `./apps/api/build` share one lock and one
  status history, and `a:b` and `a-b` never share either. The lock is the
  run's kernel gate: held exactly while the run's Effigy process lives,
  released when it exits (crash and `kill -9` included), never reclaimed from
  a live or SIGSTOPped holder. A lock conflict names the holder (pid, run id,
  age), exits 75, and carries `effigy.lock-wait.v2` details in JSON mode.
- Effigy's flags go before the task:
  `effigy [--json] [--plan] [--repo <PATH>] [--lock-wait-ms <MS>] [--env-schema <PATH>] [--verbose-root] <task> [args]`.
  Everything after the task name belongs to the task, so `effigy lint --json`
  reaches the linter. For one release `<task> --plan` and `<task> --json`
  are still read as Effigy's, with a note naming the leading spelling; other
  Effigy flags after the task name now reach the task, with a note. Built-ins
  routed as tasks (`tasks`, `test`, also with a catalog prefix such as
  `catalog_a/tasks`) keep their own flag positions.
- A plain host task runs under kernel custody: always its own process group,
  a durable record before it executes, and a settle that stops everything it
  started. INT, TERM and HUP are forwarded to the task's group and Effigy exits
  128 + n with the run recorded `cancelled`; if Effigy itself is SIGKILLed, the
  run's sentinel stops the group. Processes a task leaves running in its own
  group (`cmd &` without `setsid`) no longer survive the task.
- A failed task command now exits with the command's own code instead of 1.
- `effigy tasks status` reads the run journal and the task's gate
  (`effigy.tasks-status.v2`, `effigy.tasks-status-all.v2`). A run is `running`
  while it holds its gate, however long it takes; a run whose owner died reads
  `interrupted` with the command that settles it; the newest 20 runs are kept.
  The v0.14 read-only fallback is removed: kernel run rows are the only task
  status, so a task with no runs reads `never-run`. Old status history under
  `.effigy/runtime/tasks/` and `.effigy/reports/tasks/` is dropped, never
  imported or read; it is history, not state, so nothing restarts or unlocks
  from it, and the files are left untouched.
- Running from inside a member catalog, however deep, resolves to the root
  that declares it in `[catalog.members]`, so subdirectory and root runs see
  the same catalogs, locks and status.
- `[shell]` is removed from the manifest; it configured the interactive command
  of managed shell tabs. Tabs always run `exec ${SHELL:-/bin/zsh} -i`, and a
  manifest that still declares `[shell]` keeps its replacement.
- `effigy tasks --resolve` uses the same resolver as execution and reports
  `route`: `typed-command`, `task`, `builtin`, `exec-alias`, `defer` or
  `error`. A typed command such as `release` is reported as such when it
  shadows a member catalog's task; a root task of that name owns the word.
  `lock_scopes` names the gates a run takes: its task (one per task across
  profiles) and any shared `lock`.
- A `tasks qa-group run` is a kernel run: its run id is the kernel run id,
  and `tasks qa-group status` judges a live record by that run's gate, never
  a PID or a 20-second heartbeat. A run whose owner exited stays `unknown`,
  never a pass. Record write failures warn instead of failing the group.
- Env-schema `exec()` values run after the task is admitted, as effects of its
  run: a run refused by a live holder executes none of them. Their stdout is
  capped at 1 MiB and their stderr tail at 64 KiB.
- The vault passphrase and the local-dev secret marker no longer go into
  Effigy's process environment, so task children no longer inherit the
  passphrase; nested commands that need the vault use the local unlock
  credential or prompt.

### Removed
- Retire `effigy contracts validate-selection` and the Rhai feature
  `contracts.validate_selection`. `effigy contracts check-json
  --validate-selection` validates the selection payload of the current run
  (implied by `--print-selected=json`), and `--validate-selection --artifact
  <PATH>` validates a stored artifact. A violation is a failure row for
  `effigy.json-contract-selection.v1` inside the unchanged
  `effigy.contracts.check-json.v1` envelope. The local
  `scripts/write-json-contract-artifacts.rhai` artifact writer is gone;
  `qa:ci:json` runs `contracts check-json --full --print-selected=json`.
  Selection validation options now live on `contracts::check_json`
  (`validate_selection`, `contract`, `artifact`).
- Retire Effigy's implicit default JSON contract paths.
  `effigy contracts check-json` reads `[contracts].schema_index` and
  `[contracts].selection_contract` from the project manifest, so a project that
  declares neither is refused with "no JSON schema index configured" instead of
  being validated against Effigy's documentation tree.
- The code graph (Q-009): `effigy graph` (`index`, `status`, `search`, `node`,
  `affected`, `explore` and the earlier retired spellings), `effigy docs
  context` and its `--sources` portfolio routing, the `effigy-codegraph` crate
  and its SQLite index, `EFFIGY_GRAPH_TIMEOUT_MS`, and the `effigy.graph.*` /
  `effigy.docs.context*` JSON contracts. Use `rg` for code navigation,
  `docs/README.md` for the documentation index, and `effigy test --plan` to
  choose tests; `docs check` is unchanged. The three scanners that needed the
  graph go with it: `scan boundary-violations`, `scan dead-code` and
  `scan validation-gaps`, plus `scan --graph-context`, `scan --path` and
  `scan --stdin`. Their `[scan.boundary_violations]`, `[scan.dead_code]` and
  `[scan.validation_gaps]` tables, and `[catalog.graph]`, are manifest errors.
  The other scanners keep working without graph enrichment. The index lived in
  `.effigy/graph/` inside each checkout, which is repository-local state, not
  Effigy-owned machine state: `effigy uninstall` and state cleanup never remove
  it, and an existing `.effigy/graph/` directory is ignored. Delete it by hand
  if the disk space matters. The `[docs_policy.graph]` documentation profile
  and `[docs_policy.sources]` table are still parsed and validated so committed
  manifests keep working; nothing reads them in v0.15.
- Retire the experimental gateway listener-handoff commands:
  `effigy gateway setup-listener-handoff`, `listener-handoff-status` and
  `remove-listener-handoff`, with the launchd definition generator, its strict
  parser, and the read-only status/removal plan they used. There is no
  replacement: the gateway runs as your user, so use `effigy gateway up` and
  inspect it with `effigy gateway status`. Each retired spelling refuses with
  that guidance. The descriptor adoption boundary and the internal
  `--listener-handoff` opt-in remain.
- Retire the host-run client: `src/runner/host_scheduler`, the
  `effigy-host-run` crate, `HOST_RUN_TOKEN`/`HOST_RUN_ID` parent tokens,
  `EFFIGY_SCHEDULER_OVERRIDE`, `EFFIGY_HOST_RUN_ROOT`, the private-parent
  fixture harness and contract 049's client obligations.
  `--host-run-request-id` and `effigy tasks request status|follow` refuse with
  the replacement: `--run-id <key>` and `effigy run status|logs|stop <key>`.
- Retire `[secrets.vault].identity`, `[secrets.vault].unlock` and
  `[secrets.vault].external` after consumer cleanup. Delete these lines; they
  only restated the passphrase default. Errors name the declaring file and line.
  Secrets list/doctor JSON drops `vault.identity` and `vault.unlock` (recorded
  in the `effigy.secrets.v1` schema change log); vaults and runtime unlocks keep
  their behavior.
- Retire `effigy watch`, `effigy tasks migrate`, managed stream mode,
  transition-evidence receipts, pre-0.14 `gateway recover`, the unused
  user-runtime scaffold, and qualified private docs assurance. Removed command
  spellings give replacement instructions; unset `EFFIGY_MANAGED_STREAM` and
  use `--headless`. Existing transition files remain untouched.
- Reject the `[manifest].root` key with removal guidance. Ignore the
  user-global `~/.effigy/catalog/` override with a one-release warning; move
  custom fragments to each repository's `infra/dev/catalog/`.
- `effigy tasks unlock`. A task's lock releases itself when its run exits and
  is never taken from a live run; the command now refuses with that
  explanation. Find a live holder with `effigy tasks status <task>`.
- The task cache: `tasks.<name>.cache` and `effigy tasks cache`. A task that
  still declares `cache` is refused with the fix; `.effigy/cache` can be
  deleted.
- The demo feature: `effigy demo` (`list`, `inspect`, `history`, `run`,
  `stop`, `input`, `resize`, `rerun`, `browser`), the `[demos]` manifest
  table, `config/demos.toml` in this repository, the `effigy-demo` crate, the
  `demo.*` Rhai host features and the TUI demo browser. Each demo converts to
  an ordinary task: `[tasks."demo:<name>"]` with the same `run`, `title` and
  `summary` kept as a TOML comment, and optional `group = "Demos"`. `effigy
  demo` and a `[demos]` table both refuse with that recipe, and `effigy config
  --target demos` is gone with the section.
- v0.14 lease locks (`.effigy/locks/`), task status records
  (`.effigy/runtime/tasks/`, `.effigy/reports/tasks/`) and the 20-second
  status staleness. They are never read for exclusion or deleted; a live
  v0.14 holder still refuses its task until it exits, and `effigy doctor`
  lists the families still present.
- Rebuild `effigy bootstrap` as one command over ordinary task selectors and
  pinned sources (Q-009, effigy#262). `[bootstrap].setup` is an ordered list of
  selectors, `start` is one selector and `submodules = true` initialises
  submodules recursively at their recorded commits. Every retired spelling
  refuses with its replacement: `[bootstrap].run` and the `setup` alias for it
  became `setup`; inline run steps and `{ task = ... }`/array `start` entries
  moved into the task they name; `[bootstrap].children` and
  `bootstrap children status|sync` became pinned `[sources.<name>]` (`git`,
  `rev` or `rev_file`, `path`); `bootstrap deps sync` became a repo-owned
  selector in `setup`; `--db-seed`, the dump prompts and the
  `bootstrap:db-seed` bootstrap callback became a first-seed task in `setup`;
  `--fresh` and `bootstrap teardown` became environment lifecycle
  (`effigy container <name> down`); `--backend` became the declared environment
  plus the machine backend preference; `--start` is the default; and
  `bootstrap deps sync --refresh-lock` became a deliberate repo task. A
  non-empty foreign destination is refused instead of prompted for, and
  `effigy bootstrap` with no URL prepares the existing checkout in place.
  `effigy doctor` reports pinned-source drift; bootstrap and doctor never reset,
  fetch forward or clean a developer's working copy, and the first `dev` also
  acquires a missing pinned checkout (contract 020). Bootstrap fails once,
  early, with a collected prerequisite list — `git`, an absolute `HOME`, a
  container runtime when `[containers]` is declared, the JS manager and Node for
  a `package.json`, `cargo`/`rustc` for a Rust checkout, and a running
  user-owned gateway when routed domains are declared — and never installs a
  toolchain, starts a runtime, sets up the gateway or impersonates a login. The
  `effigy.bootstrap.deps`, `children-status`, `children-sync` and
  `bootstrap-teardown` payloads are gone; `effigy.bootstrap.v1` keeps plan and
  executed phases with `sources`, `submodules`, `setup` and `start`.

### Added
- `test:focused` runs a focused Rust test selection through Cargo with
  `--locked`, keeping `Cargo.lock` read-only while a source-linked consumer
  checkout is under observation.
- The container system guide documents Colima's VM DNS proxy timing out a
  registry pull while the host resolves normally, with a bounded two-attempt
  retry and a profile restart, so a transient VM resolver outage is not
  mistaken for a failed catalog image or wrong pull credentials.
- Generic per-program fallback (Q-008, effigy#250). An Effigy user config may
  declare `[programs.<name>]` with `fallback = "<program>"`, an optional
  `allow = ["<first argument>", ...]` and an optional `force =
  true`. When `<name>` is not an executable on the inherited `PATH`, or
  `force = true`, the run gets a shim named `<name>` first on its `PATH`, prints
  one honest notice, and settles the shim with the run. The shim execs the
  fallback with the exact argv: no flag translation, nothing added and no retry,
  and the fallback's exit status stands. An omitted allow list is unrestricted
  exact argv; a present one runs only those exact first arguments and refuses
  anything else (including no argument) before exec. The fallback resolves to an
  absolute path on the inherited `PATH`, so it never re-enters a shim and
  fallbacks never chain. Only inherited-PATH name resolution is affected;
  absolute paths, a replaced `PATH` (including a task `env` one), shell aliases
  and shell functions are not, and `force` changes nothing else. The original
  program's behaviour and safeguards are lost: with `oxide` -> `cargo`,
  `oxide clean` becomes `cargo clean` without Oxide's ownership checks or
  managed storage, and cargo plugins and cargo-subcommand exec run as plain
  cargo would. The block is user-config only; a project `effigy.toml` that
  declares `programs` is refused as an unknown field. `effigy help`, the shipped
  Effigy skill and the user guide section
  ([069](docs/guides/069-workspace-host-integration.md)) document every
  condition.
- Per-run scratch on a configured external volume (effigy#244, effigy#253).
  Setting `[scratch] root` in `~/.effigy/config.toml` to any directory on a
  drive gives every host-placed run — the built-in `effigy test` route included
  — its own `<root>/<8-character run directory>/`, named from the run's whole
  identity so same-instant and `--run-id` runs still differ, as its `TMPDIR`,
  `TMP` and `TEMP`, as a kernel run effect settled with the run: released when
  the run succeeded, kept for 24 hours from the moment it failed (the failure
  output prints the path), and swept by a bounded later run. Only the run's own
  marker, matching that run's derived directory name, authorises a verdict or a
  removal, so a directory that went away or was replaced is never relabelled or
  deleted, and a sweep never touches a directory or file Effigy did not name
  and mark itself. The built-in `effigy test` route's host suites run as child
  runs of its run, so their process groups are stopped before the directory is
  released (the multi-suite TUI is not used while such a run holds them), and
  with no configured scratch that route opens no run and needs no Effigy state
  home. The root may be any directory on the drive, not only its mount point; a
  missing root is created with mode 0700 when its nearest existing ancestor
  passes the volume proof. The gate refuses a root whose nearest existing
  ancestor is on the boot or data volume (so a stale `/Volumes/<name>`
  directory left on the internal disk after the drive goes away is refused), a
  path no mount point holds, a `volume_uuid` mismatch, a root owned by neither
  the current user nor root, and a root that is group- or world-writable or
  carries an ACL; a refusal falls back to the default temp directory and
  `effigy doctor` reports the reason. Effigy hardcodes no path, no UUID and no
  other tool's directory. An explicit or non-default `TMPDIR` wins, a nested
  run reuses its parent's directory, a nested task that declares
  `scratch = false` hides its parent's directory from its own host processes,
  container workloads never receive the host scratch, and a task opts out with
  `scratch = false` (managed sessions included) or `EFFIGY_SCRATCH=0`. Unset,
  the environment is unchanged. The user guide section is
  [069](docs/guides/069-workspace-host-integration.md).
- Catalog sources (Q-005). Services resolve from the project catalog
  (`infra/dev/catalog/`), the bundle's shipped `catalog/`, declared
  `[catalog.sources.<name>]` git, OCI or path sources, then the compiled
  baseline. Declared sources share the bundle's checkout pins and move only on
  `effigy bundle sync`; reads never fetch. Qualify a name as
  `<source>/<service>` to pick one source. `service list` and
  `container render` name each service's source.
- Fragments can declare `[requires]` (`effigy` range, `schema`,
  `capabilities`) in `service.toml`; a release that cannot meet them refuses
  the fragment and names what it needs.
- `effigy container status` and `effigy doctor` show each environment's
  backend, why it was chosen, and its record: active or stopped, persistent or
  held, the applied generation, a partial generation and its holders. Status
  JSON gains an `environment` object.
- Run control for every run (Q-006): `effigy --run-id <key> <task>` names a
  run with a caller key, and a repeat or a racing second invocation attaches
  to it and exits with its result instead of starting another. `effigy runs`
  lists runs (`effigy.runs.v1`), `effigy run status <run>` reports one
  (`effigy.run.v1`), `effigy run logs <run> [--follow]` reads its output by
  exact byte offset and ends on settle with the run's exit code
  (`effigy.run.logs.v1`; resume with `--stdout-offset`/`--stderr-offset`),
  and `effigy run stop <run>` stops it through a fenced kernel request. A
  lost follower never loses or duplicates output, a terminal run without
  logs still reports its real result, and expired log bytes are reported
  (closes papercut 910c578a). A plain run named by `--run-id` whose stdout or
  stderr is not a terminal is captured, relayed as it arrives with declared
  secrets redacted, and logged; other runs keep their descriptors untouched.
- `tasks --json` rows and `--plan --json` carry `"admission": "heavy"` for a
  classified task.
- Keep CLI presentation local and bounded: banner sizing never launches `tput`, and version metadata reads environment or installed stamps without running Git. Unstamped development binaries report the package version.
- Graph reads, documentation context and doctor never refresh or create graph
  files. Explicit `graph index` uses a kernel store gate and migrates existing
  WAL databases to rollback journaling. Until migration, reads report stale
  with an index hint. The legacy status `--refresh` flag warns and remains
  report-only for one release.
- Checkout bundle and declared catalog source pins in `.effigy/sources.lock.json`.
  Help, version, completion, inventory, plans, status, config views, doctor and
  graph read paths no longer acquire or refresh bundle sources or append Git
  excludes. Only `effigy bundle sync` updates selected path, Git and OCI bytes;
  bootstrap and first dev prepare missing pins. Existing v0.14.1 cache bytes
  import automatically, and failed acquisition keeps the previous selection.
  A locally known newer version produces a bounded sync notice without a remote
  probe. `EFFIGY_GIT_BUNDLE_REMOTE_CHECK_TTL_SECS` is retired with a Prepare warning.
- `effigy container render <env>` prints the exact render, generation path and
  digest without changing runtime state. Host renders assemble in memory and
  Prepare publishes sealed kernel objects. Admission pins generation references;
  reset preserves sibling environments. Reset and bootstrap teardown prepare
  their Compose generation before stopping or cleaning images, including fresh
  and changed environments. Existing renders migrate on Prepare.
- Container policy reads leave loopback assignments unchanged and perform no
  loopback runtime inventory. Prepare allocates or imports existing assignments,
  including for shared services and environments with explicit host ports.
- The runtime kernel exposes immutable file closures with journalled publication,
  fenced desired/applied references, per-owner reset, retention protection and
  backend metadata. Host container rendering adopts these APIs separately.
- Generated host-port inspection leaves unassigned bindings explicit and keeps
  `ports.json` unchanged. The first Prepare assigns ports through the existing
  gateway allocator; later reads preserve those assignments.
- Guest-local container shell and exec resolve manifest execution coordinates
  without rendering host policy or touching host runtime state. Missing host
  Compose files and catalogs no longer prevent local guest execution.
- A user-owned gateway daemon with kernel custody and a control socket. `gateway setup` installs platform prerequisites and migrates a v0.14.1 root gateway while preserving routes, IPs and port pins; ordinary lifecycle commands never elevate. macOS uses a peer-checked bind broker for low ports, and Linux uses a binary-local bind capability. `gateway setup --remove` removes verified platform output.
- A whole-environment handover ends the losing environment's headless
  managed owners before stopping anything. When `container up` hands over
  a live conflicting environment, it first ends every live headless
  managed owner (`effigy <task> --headless`) whose plan is bound to the
  losing environment, through the same path as `effigy <task> stop` —
  owned processes settle, status is recorded, and the owner's own exit
  releases its task lock through the lock owner-token rules — printing one
  line naming each managed task ended. Owners bound to any other
  environment are never touched. Ending an owner that fails refuses the
  handover before the environment is stopped and names what is still
  running. Headless runs also never allocate an interactive
  TTY: shell-role processes are skipped with a note instead of spawning a
  child that would block on a TTY, and SIGINT/SIGTERM to the headless owner
  settles the run through the stop path before exiting. A stale lock from
  a dead holder is still reclaimed only by the existing lock rules.
- Switching between a checkout's container and hybrid environments is one
  preflighted handover. `container up` of one environment while another
  environment of the same checkout and scope is live first evaluates
  everything that could refuse — foreign route claims, loopback allocation,
  manifest, policy, ownership and backend validation — before changing
  anything, then brings the live conflicting environment down with the
  ordinary `container down` semantics (managed host processes and listeners
  stopped, gateway routes withdrawn, containers stopped, all volumes kept),
  then starts the requested environment. This works in both directions and
  replaces both the data-services-only stop and the former live
  same-checkout listener refusal: a handover refuses only when stopping the
  other environment itself fails, names it, and leaves it recoverable by its
  own `up` or `down`. The stop is recorded, so an environment whose services
  a handover stopped restarts with a plain `up` instead of refusing an
  unknown partial state; genuinely unknown partial state still refuses.
  Environments without `data_from` and without colliding routes behave as
  before, and foreign checkouts, scopes and stacks are never touched.
- `data_from` on `[containers.<env>]` lets a container environment borrow
  another environment's service data volumes. Services present in both
  environments with the same catalog entry mount the owner environment's
  named volumes for the same checkout and worktree scope — bundle hybrid
  (`dev`) and the container profile (`dev:container`) see one database and
  one bucket per checkout, and switching between them is seamless. `up` of
  either environment hands the other live environment over whole (data kept)
  and prints one line naming the handover; a failed stop refuses instead of
  racing for the volumes. Borrowed volumes are declared `external` in the generated
  Compose, so `reset`, `reset --wipe-data`, `retire`, `retire --dead` and
  `ports reclaim` never delete or claim the owner's data — retirement records
  list them as retained, never owned. Environments without `data_from` keep
  their own fresh volumes, and existing manifests generate byte-identical
  Compose.
- Added a [v0.15.0 consumer migration guide](docs/guides/085-v0.15.0-consumer-migration.md)
  covering the catalog-pack compatibility step, container lifecycle and
  doctor behavior changes, opt-in features, and a DecodeLabs checklist.
  Container, gateway and docs help notices now lead with the user action.
- `effigy container retire --dead` retires every recorded runtime scope whose
  checkout no longer exists, such as worktrees removed without running their
  teardown, through the ordinary retirement path. It previews the scopes when
  run without a terminal or `--yes`, and is safe to schedule with `--yes`.
- Ordinary `effigy container status` output prints a one-line text note
  pointing to `effigy container ports status` when the machine-wide
  generated-container port registry is at or near exhaustion (zero or one
  free 100-port range). The note reads the registry without the lock and
  computes capacity only, with no allocation, recovery planning or probes;
  the `effigy.container.status.v1` JSON schema is unchanged.
- Optional `[containers.<name>.dns].base_domain` selects the apex for generated
  TCP service aliases and the `container hosts` cookie domain. When omitted,
  the first resolved DNS route continues to supply that apex.
- Workspace handoff verifies the host-selected installed service catalog pack
  after selected-workspace activation or recreation, for the workspace user, including when the Linux binary is already current.
  Empty guests receive validated retained content without needing its original
  source checkout or network access; matching packs avoid copy/install.
  Different or unknown guest selections and changed or unhealthy inputs refuse
  before task execution and preserve the guest store. Guarded local
  `service pack install --prepare-content` pins content without replacing an
  existing selection. Project/user override precedence and bounded handoff
  privacy, time and output limits remain unchanged.
- Secret-free versioned refusal accounting for bounded container transport
  refusals (task 159). Every bounded transport refusal payload now carries a
  `refusal_observations` block, additive in `effigy.container.bounded-exec.v1`
  and internally versioned: preparation/attempt/operation elapsed and
  remaining budget from the original operation clock at one coherent
  snapshot, a truthful `spawn_disposition` for the owned host client only
  (`not_attempted`, `attempted_not_confirmed`, `spawned`, `unknown`),
  independent stdout/stderr `state` (`pending`, `eof`, `overflow`,
  `read_error`, `unknown`) with `retained_prefix_bytes`/`read_bytes` counts
  carrying explicit `exact`/`lower_bound`/`unknown` semantics, and an explicit
  `leader_exit_observed` that stays `unknown` without a safe non-reaping
  observation. The refused attempt's retained prefix is the failed-prefix
  accounting: reported separately from completed-capture `retained` totals,
  never added to them and never charged twice against the cumulative caps, so
  deadlines, fallback eligibility and all legacy field meanings are unchanged.
  The block never carries bytes, argv, environment, raw errors or digests,
  and missing observations say unknown or lower-bound rather than zero. It
  appears only on transport refusals and true pre-step budget boundaries;
  phase failures decided about completed captures or non-transport steps
  carry no block, so spawned-and-completed work is never mislabeled
  `not_attempted`. A stream `read_error` is not inducible through real
  spawned pipes and is qualified at the production reader plus the snapshot
  mapping.
- Read-only launchd listener-handoff status and a checked administrator
  removal plan (task 157, SOURCE implementation candidate).
  `effigy gateway listener-handoff-status` opens
  `/Library/LaunchDaemons/dev.effigy.gateway.user.plist` with `O_NOFOLLOW`,
  binds it to a single regular file's kernel identity and bounded bytes, and
  reports the definition, the current canonical executable path and the
  resolved operator as distinct findings: `definition` is `verified` only for
  the complete expected generator shape with matching operator and executable,
  and otherwise names the finding (`foreign_owner`, `unsafe_mode`, `symlink`,
  `changed`, `oversized`, `malformed`, `shape_mismatch`, `operator_mismatch`,
  `program_stale`, `program_missing`, `permission_denied`, and so on). A bounded
  fixed-argv `launchctl print system/dev.effigy.gateway.user` capture is parsed
  structurally into `loaded_running`, `loaded_not_running`, `confirmed_absent`,
  `permission_denied`, `capture_timed_out`, `capture_truncated`, `nonzero_exit`,
  `malformed`, `association_mismatch`, `unavailable` and `unsupported`; label,
  PID, checksum and displayed path stay observations and never grant signal or
  removal authority. `effigy gateway remove-listener-handoff` emits only the
  two fixed literal administrator steps (`launchctl bootout` then `rm`) with a
  required fresh identity recheck and a KeepAlive warning, states that its
  effects are UNEXECUTED, and never runs `sudo`, `launchctl` or `rm`. Foreign,
  stale, changed or uncertain definitions and loaded jobs refuse before any
  usable plan is produced. Neither command installs, activates, signals,
  removes or elevates anything; the administrator execution, verification and
  cross-boot proofs remain operator-run (guide 063, architecture 034).
- Managed owned-child exit refusals now distinguish empty, invalid UTF-8,
  multiple-token, unknown-state and unsuccessful process-table captures using
  bounded structural diagnostics. They retain counts, hashes and elapsed time,
  never captured process output. Incomplete observation still refuses settlement;
  stop authority, state grammar and deadlines remain unchanged.

- Launchd loopback listener handoff for a user-owned gateway (task 156,
  SOURCE implementation candidate). `effigy gateway setup-listener-handoff`
  prints the deterministic administrator launchd definition that owns exactly
  TCP `127.0.0.1:80` (`http`) and TCP `127.0.0.1:443` (`https`) over IPv4,
  launches the daemon as the authenticated non-root operator with the
  canonical reviewed executable and pinned `HOME`, and opts it into adoption
  via `__gateway-run --listener-handoff`. The daemon adopts handed sockets
  only through launchd's documented `launch_activate_socket` ABI, validating
  each descriptor (open, non-duplicate number and kernel socket object,
  stream type, listening state via libproc on macOS and `SO_ACCEPTCONN` on
  Linux, exact endpoint, close-on-exec) before adoption; DNS, route watching,
  TCP aliases and TLS keep their ordinary unprivileged paths. Any refusal —
  root or sudo-environment identity, missing or malformed activation, wrong
  count, family, type, endpoint, or duplicate — closes every handed
  descriptor exactly once, adopts nothing, and publishes no generation
  record; unknown definition shapes refuse strict parse-and-validate
  controls. Nothing is registered, bound, installed, or migrated by any
  command: the administrator install, live activation, and cross-boot
  restore remain operator-executed (guide 063's listener-handoff section,
  architecture 034), and the task-155 direct-bind readiness gate stays
  unchanged for non-opt-in runtimes.
- `effigy --json docs qualified resolve` declares the bounded ordered host docs
  recipe and its selected installed Northstar source. The private Node bridge
  binds that selection to the reviewed v4 provider's retained inputs and uses
  live lookup before each leaf. Resolution alone grants no execution or native
  authorization; ordinary task and skill execution stay unchanged.
- `effigy container <NAME> shell --command <CMD> --bounded --deadline-ms <MS>
  --stdout-limit-bytes <N> --stderr-limit-bytes <N> [--json]` is an explicit
  opt-in finite container-command surface. One absolute deadline governs
  bounded process captures and artifact requests from runtime discovery and
  primary workspace preparation through the user command, eligible fallback
  and final draining. Independent
  stdout/stderr budgets are enforced while reading and accounted cumulatively
  across preparation and fallback; returned command bytes remain command-only.
  JSON adds preparation capture counts and retained totals to schema v1.
  Overflow, read failure,
  incomplete output and deadline refuse even when the client exits `0`, never
  report a retained prefix as success and never trigger a fallback or repeat.
  Both streams drain concurrently, and only the exact owned client process
  group is terminated and reaped on refusal. `--json` emits the versioned
  `effigy.container.bounded-exec.v1` payload with bounded bytes and counts;
  refusal payloads carry safe reason, phase, stream, limit and counts. Launch
  failures redact configured container/service names and exclude argv, command,
  output, credentials and host error payloads on human and JSON surfaces. Inner
  nonzero exit stays distinct from bounds/deadline reasons; human mode emits
  retained command output only after a successful inner exit. Remote container
  cancellation remains explicitly unverified. The default command shell keeps
  its existing unbounded behavior.
- `effigy container ports status|reclaim` reports the generated-container port
  registry: free 100-port range capacity before and after a plan, and a
  `retain`, `hold` or `reclaim` decision with a reason for each entry. Status
  and dry-run writes nothing. `reclaim --apply --expect-plan <DIGEST>` takes
  the registry lock, recomputes the plan, refuses a changed digest, and removes
  only entries whose project path is definitely missing and that no scope,
  route, container in any state, or bound host port still references. Apply
  backs up the pre-edit registry and reports its hashes. Incomplete inventory or
  unreadable ownership stores hold every absent-path entry. Reclaim never
  touches containers, volumes, data, routes or profiles.
- Managed host processes can declare a loopback listener, HTTP readiness
  check, browser route and listener dependencies. Effigy verifies the actual
  process and socket owner before route publication, reports assigned
  addresses from detached `container up --json`, propagates current URLs to
  dependent children, and withdraws exact route generations before restart or
  scoped teardown. The adapter report and versioned state-file contracts are
  documented; Linux procfs and macOS libproc provide socket ownership checks,
  while unsupported platforms fail closed.
- Managed listener supervisors write a bounded, versioned transition receipt
  beside each listener state file. It records the selected reason a generation
  exited, shut down, lost a dependency, or changed its dependency tuple, the
  reserved dependency values each generation launched with, and the
  predecessor generation. Receipts are observation only. They never authorize
  signals, routes or cleanup, and missing receipts are not proof that nothing
  changed. Listener state and the startup diagnostic keep their existing fields.
- `effigy gateway setup-tls` and `gateway up` now accept an explicit
  `--private-state-root` for disposable gateway instances. Private mode keeps
  routes, listeners, certificates, CA state, and process identity inside the
  caller-owned owner-only root; it binds only unprivileged loopback endpoints,
  returns actual listener addresses, and never installs host trust, writes
  resolver files, provisions host aliases, or requests elevation. Container
  route registration and managed startup inherit the same mode through
  `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT`. Clients explicitly trust the reported
  fixture CA and resolve route names to the reported HTTPS address. Ordinary
  operator gateway setup remains the default. Private certificate generation
  accepts an absolute `EFFIGY_GATEWAY_MKCERT_BIN` override, fails closed for an
  invalid override, and keeps its `CAROOT` inside the selected private root.
- Heavy selector callers can persist and pass `--host-run-request-id` before
  submission, then inspect or follow that exact request with `effigy tasks
  request status|follow`. Only authenticated `unknown_run` permits a same-key
  retry; ambiguous recovery remains held.
- `effigy gateway status|up|down` and managed auto-start append a bounded
  `current diagnosis: record=<kind> process=<probe> identity=<outcome>` hint
  when the gateway state is unknown. Labels only; no raw probe output, paths,
  digests, arguments, environment, or credentials. Lifecycle behavior is
  unchanged: unknown stays unknown, records are preserved, nothing is
  signalled or replaced, and a positively verified matching live gateway still
  reuses without a second daemon. Noninteractive cross-UID verification remains
  unsupported with approved interfaces; no new privilege or helper is added.
- User-owned gateway migration boundary (task 155, design and private
  qualification only; no live migration ran): a finite seven-entry privilege
  inventory, a read-only user-runtime readiness gate over injected setup
  evidence (bootstrap and route-driven `/etc/resolver` files, `lo0` aliases,
  lowport bind permission, CA trust, old-generation absence), byte-exact
  `routes.json`/`loopback-ips.json` migration preservation with a named old
  to new generation transition, and safe refusal for stale, unknown, symlink,
  UID-ambiguous, route-edited, and bind-failure cases. Native same-UID proof
  on this machine binds `127.0.0.1:180` once and reports `EACCES` (errno 13),
  so exact `127.0.0.1:80`/`443` service without privilege is an unsupported
  gate; the one missing capability (`loopback-privileged-listener-handoff`)
  is a design boundary, not a supported start path. No operator defaults,
  endpoints, or trust behavior change.

### Changed
- `effigy contracts check-json` validates with one JSON Schema engine,
  `effigy-json-schema` (Draft 2020-12 through `jsonschema` 0.50 with default
  features off), instead of hand-written per-schema key assertions. A schema
  index may declare a `payload_schemas` map from schema id to a JSON Schema
  document, relative to the index file; Effigy's own index ships documents for
  `effigy.command.v1`, `effigy.container.migrate.v1` and
  `effigy.container.render.v1`. `format` is annotation-only, `$ref` never
  leaves the document, and an invalid payload schema stops the run
  ([contract 055](docs/knowledge/contracts/055-project-json-contract-validation-contract.md)).
  The selection payload's `count` remains a producer guarantee derived from
  `selected.length`, because JSON Schema cannot compare a property with an
  array length. `--json` failures now report the structured
  `effigy.contracts.check-json.v1` payload in `error.message`.
- The contributor task that builds this checkout and installs it as the machine's `effigy` is renamed `bootstrap:local` → `install:local`, so it no longer reads as the `effigy bootstrap` feature (clone and start a project). `bootstrap:local` now fails with the new name.
- Effigy's own repository builds through Oxide. Selectors run Cargo through
  `scripts/oxide-selector.sh`, which leases Oxide's shared slot pool so Rust
  intermediates stay off the internal SSD. Every routed selector keeps a
  named `<selector>:cargo` escape with the plain-Cargo command it replaced;
  `EFFIGY_OXIDE_ESCAPE=cargo`, `effigy cargo <args>` and `cargo run --bin
  effigy --` keep the rest. The `cargo qa*` and `prepush-ci` aliases are
  removed, because a Cargo alias either needs unsupported `!sh` syntax or
  compiles Effigy's dependency graph with plain Cargo; their tasks are
  `qa:gates`, `qa:docs`, `qa:json`, `qa:json:ci`, `release gates` and
  `prepush:ci`. Selectors that pin a `target/` binary path, a non-Oxide Cargo
  profile or a scrubbed `HOME` stay on Cargo, and hosted runners keep using
  Cargo through `GITHUB_ACTIONS`.
- Gateway HTTP requests reuse upstream connections with low-latency TCP settings.
  Managed listener ownership is verified at publication and off the request path;
  stopped or replaced generations still refuse traffic, including reused connections.
  An unchanged route obtains a fresh proof when concurrent endpoint publication
  retires its connection pool, avoiding a wait on an exited worker.
- Private gateway startup waits within the 15-second gateway budget for verified
  listener publication, allowing durable endpoint import on slower filesystems.
- Concurrent cold container startup initializes endpoint authority before
  Prepare reads policy, avoiding a transient uninitialized kernel-store error.
- Detached container startup JSON identifies the preparation step and safe
  failure family without exposing backend output, argv or environment.
- Gateway routes, loopback addresses and generated host-port ranges now share
  one transactional endpoint store. Existing JSON registries import automatically
  and remain as backups, preserving URLs, IPs and pinned ports. Stopped stacks
  retain their assignments; explicit retirement releases them through quarantine.
- Container inspection and validation no longer create declared media directories or the shared Composer cache; activation prepares those bind sources.
- Graph now has six commands: `index`, `status`, `search`, `node`, `affected`
  and `explore`. Node accepts file paths and incoming/outgoing relationships;
  explore supports an excerpts-only mode. Removed command spellings and catalog
  graph settings name their replacements for one release. Catalog partitioning
  and the graph watcher are removed; all catalog members share the checkout index.
- Graph refresh commits each file's graph, search data and freshness state
  together, preserves incoming relationships, and resumes safely after a
  timeout or interruption. Concurrent queries read committed files with
  `refreshing` progress. Database snapshot copies and detached timeout workers
  are removed; `graph status` is read-only.
- A managed run's stop, a SIGINT/SIGTERM settle, and a handover-driven stop
  now report settled only after the whole stop cascade finished. `effigy
  <task> stop` and a handover wait for the supervisor's real cascade -
  including its lifecycle cleanup's `container down` - on a settle budget
  derived from the existing shutdown grace and cleanup budgets, so a real
  down is never mistaken for a hang. Every process group the run exec'd
  into its container environment is ended through the same exec surface it
  attached through (an in-container `cargo run` can no longer outlive the
  stop), a run that owns its environment brings it down as before, and the
  stop output says what settled. When a handover ends a bound managed
  owner, the owner's cascade has already brought the environment down: the
  handover verifies that instead of racing a second conflicting `down`, and
  a refusal names what is still running without ever advising the stop that
  is already in progress.
- Lock records written by the processes a managed run starts - on the host
  and inside the container - carry the run's settle token, and the run's
  settle releases records still attributed to it through the lock
  owner-token rules, so an immediate restart after a stop has no lock
  conflict. A lock holder in another PID namespace (a container-side
  Effigy process, or a host holder seen from inside a container) is judged
  by its heartbeat lease alone instead of a foreign process table; a
  genuinely live foreign holder still refuses, and lock records without
  the token keep today's behavior.
- Local workspace handoff automatically reuses a verified owned Linux artifact
  builder after an upgrade leaves a changed-input partial activation. Builds
  preserve the original admission and cache volumes; foreign or uncertain
  builders still refuse, and stale artifacts remain rejected.
- `[defer]` templates receive `{args}` and `{request}` shell-quoted, the same
  way task `{args}` are: an argument containing spaces stays one word and shell
  syntax in an argument is not interpreted. A template that relied on an
  argument being split or evaluated must now do that itself.
- Managed host listener routes publish to the shared operator gateway when no
  `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT` is set, instead of refusing. Private
  roots keep their existing validation; route ownership and generation checks
  are unchanged.
- Host-admitted standard direct-command tasks bound to a container now execute
  their resolved command through mapped container exec instead of re-entering
  Effigy inside the container. Selector resolution, parent validation, locks,
  status/cache handling, and settlement stay on the host; scheduler credentials
  remain excluded from container and `sudo` environments. Direct heavy
  container selectors without valid scheduler authority still fail closed.
- Managed host-listener routes follow the checkout host map that container
  DNS routes already use. A linked worktree or marked ephemeral clone that
  declares `[containers.<name>.host_processes.listener.route] domain` gets a
  scoped apex (`api.acme.test` becomes `api.acme-w<host-key>.test`) for its
  readiness `Host`, TLS certificate, gateway publication, `public_url` and
  dependent `EFFIGY_MANAGED_HOST_<NAME>_PUBLIC_URL`. Primary checkouts and
  `share_runtime_identity = true` keep the declared name. `container up --json`
  reports `declared_route_domain` beside the effective `route_domain`, and
  `container hosts --json` lists listener-only routes as declared-to-effective
  entries. Identical declared hosts in two checkouts no longer collide.
- Task-status records are published by replacing a complete sibling file, so a
  concurrent reader never observes a truncated or empty JSON document. A failed
  publication keeps the previous valid record and removes only the writer's own
  temporary file.
- The gateway daemon now handles Unix TERM and INT through its owned shutdown
  path and waits for server tasks before removing its exact generation record.
  Administrator identity and lifecycle handoffs provide visible operator
  guidance and bounded waits; after a timeout, check gateway status before
  retrying because privileged completion may be unknown.
- The compiled catalog baseline now tracks catalog-pack v1.2.0
  (`ghcr.io/inflatable-cookie/effigy-catalog-pack@sha256:7f34c21acce23033d5972868df13a9b061dab995b50b38adb954f7a1d1b5ef5b`)
  with compatibility `>=0.13, <0.16`, admitting Effigy 0.13 through 0.15, and
  adds the `silo` S3-compatible object storage service. Installed packs from
  1.1.2 or earlier reject 0.15: run `effigy service pack update` to move to
  1.2.0.

### Fixed
- The `[demos]` retirement error and `effigy config --target demos` now give
  the Q-009 conversion recipe instead of telling operators to delete the table
  and record sessions elsewhere: each `[demos.<name>]` becomes
  `[tasks."demo:<name>"]` with the same `run`, the `title` and `summary` kept as
  a comment, and optional `group = "Demos"`, with a before/after example.
- A retired `[secrets.vault]` key in a bundle export, such as a stale cached
  Underlay export, refuses with the export file and line and the remedy:
  `effigy bundle sync` for a pinned or cached bundle, or deleting the line when
  you own the bundle. It no longer surfaces as a bare TOML parse error carrying
  the consumer-manifest advice.
- `effigy gateway status --json` refuses unprovable state with a structured
  `effigy.gateway.status.v1` document instead of only prose: `ok: false`,
  `status: "unknown"`, the recorded `pid`, `observations.record` /
  `observations.process` / `observations.identity` labels and the
  `headless` observation precondition. A noninteractive plan can gate on
  `record=authenticated` plus `process=running` with a recorded PID rather
  than on exit 0; a denied or unknown identity stays unknown and the command
  stays non-zero. A record this caller may not open is diagnosed as
  `record=unreadable`, never as malformed.
- Gateway record readers take the shared half of the record lock, so a status
  probe that reads while the daemon publishes its identity/PID pair can no
  longer observe the identity-only first half and report "gateway identity
  exists without its PID". The interrupted-pair refusal itself is unchanged.
- Operator gateway startup bounds supervisor admission and daemon readiness
  as two phases. A slow admission no longer consumes the readiness budget, and
  each phase reports its own timeout instead of one shared startup deadline.
- Cargo local links record the committed blob of every tracked lockfile they
  resolve, and `effigy deps unlink cargo <library-path>` restores those exact
  bytes when the record still matches `HEAD` and only link-owned packages
  differ. A lockfile another link or an unrelated edit still holds is left
  untouched, and a moved `HEAD` invalidates the record rather than reverting
  committed work.
- A committed dependency advance no longer fails a valid Cargo unlink. When the
  only difference from `HEAD` outside the linked packages is a source/version
  advance of the same packages (an operator `cargo update`, or a release tag
  that moved after the consumer commit), unlink reports it as a
  `committed-transition` and completes instead of refusing and leaving stale
  machine-local link state. A split version, a path source, a changed
  repository, or a parked `[[patch.unused]]` entry is still refused, and
  link-time validation stays strict. `effigy deps status` reports a committed
  transition as a warning, not an error.
- A repo manifest that overrides a bundle-defined step table
  (`[secrets.vault].generate`, a deploy hook, or the `run = { ... }` step
  shorthand) replaces the bundle's executor instead of inheriting it. Switching
  a generator from `rhai` to `task` no longer keeps both keys and fails the
  exactly-one-executor rule. Unset step metadata (`run_in`, `env`, `env_file`,
  `id`, `timeout_ms`) still defaults from the bundle, and a manifest step that
  names two executors is still refused, never silently repaired.
- Deleting an isolated volume's host mount-point directory (`target`,
  `node_modules`) under a running container stack no longer leaves the nested
  volume without a mount point until someone recreates the container. The mount
  observation now includes those host directories, so `container status` and
  `doctor` report a missing one as `mount_drift` without creating it.
  `container up` and routed-task preparation recreate the missing host
  directory before create or reuse, and because it is already drift the
  routed-task path reconciles through the guarded bring-up instead of an
  idempotent `--no-recreate` apply. A file or symlink at that path still
  refuses, and no ownership or mode changes.
- The stopped-inclusive runtime inventory (`runtime ps --all`) used by task
  preparation and owned-service recovery retries once, after a short backoff,
  only when Effigy's own observation bound expired for one project-scoped read.
  A second timeout fails closed and names the failure as a runtime inventory
  admission failure: the task did not run, and the message points at
  `container status` and the task's host route where the repository defines
  one. Parse, refused-runtime and non-timeout failures are never retried.
- Managed host listener readiness keeps polling while another process briefly
  shares the listening socket, and fails with the exclusivity error only at
  its deadline; its diagnostic no longer mixes a stale HTTP status into a
  later observation.
- A run's control socket left unrecorded by an owner killed between bind and
  receipt is removed at settle instead of becoming residue.
- Sentinel admission after a running executable is replaced uses an APFS
  copy-on-write clone on macOS or the held executable fd on Linux, avoiding a
  full binary write. Unsupported zero-copy execution refuses admission with
  a typed helper error instead of falling back to copying.
- Deep doctor leaves identical cache generations untouched, including during
  `--refresh`, avoiding publication locks and redundant synced replacements.
- The operator gateway can start from an Effigy-managed task without inheriting
  its parent's custody or probe nonce. Startup reports refused spawns and early
  daemon exits; status retains the exit code and bounded stderr tail from the
  run journal instead of hiding the failure behind a readiness timeout.
- Reset drops only the selected environment's render references.
- Task and step admission no longer fails intermittently with `helper pipe
  closed` reported as a `kernel.db` store error when the running `effigy`
  binary's path is replaced (for example by `cargo run --bin effigy`) while a
  run starts. The refusing sentinel is retried once from the unchanged startup
  image. A kernel helper that still dies before acknowledging is reported as
  `kernel helper ... exited ... before acknowledging`, with its exit status
  and the tail of its own error output.
- Custody recovery no longer reports `group contains members without generation
  evidence` for a process that exited between the group inventory and its
  environment read. A member that is provably gone is re-inventoried; a live
  member without the run's nonce is still residue and is never signalled.
- The in-process heavy-admission marker is held per thread, like the run
  nesting it mirrors. A heavy run on one thread is no longer treated as nested
  in another thread's admission, so its own admission hook still gates it and
  host capacity control cannot be skipped by unrelated in-process work.
- `gateway setup --remove` exports current routes, loopback IPs and port pins
  for a v0.14.1 downgrade. Setup re-imports subsequent legacy edits on return.
  Interrupted exports are resumable; changed foreign files and active managed
  listener routes refuse with exact paths or domains. Teardown preserves
  existing aliases and reads resolver inventory without root opening user state.
- Gateway detach no longer races owning-client closure, and status reports
  startup and retirement coherently within its observation deadline.
- Gateway parser and descriptorless-process controls now run on Linux and
  macOS; idle shutdown proofs wait for completion instead of fixed sleeps.
- Test fixture guards remove sealed kernel object trees on success and unwind;
  ordinary fixture roots live outside the checkout, with documented workspace
  exceptions and a suite leak assertion. Shared process guards stop registered
  child groups and detached fake workloads before fixture removal. The kill-matrix
  reclaim proof holds its first run at a barrier until SIGKILL. Fixture child
  identity is registered before exec, including immediate exits. Fixture removal
  shares native child custody with live guards until their groups close.
  Detached private fixture gateways publish custody before serving, even if their
  startup client fails or is killed before returning a successful response.
- Container preparation detects missing or changed named-volume mounts before
  reusing a running stack, including worktree `isolated_dirs`. Status and doctor
  report drift, guarded `up` applies the current render, and permission repair
  refuses stale mounts instead of acting on the host bind beneath them.
  Pre-checkout-label Colima stacks use the existing checkout-bind migration
  proof. Unavailable mount observations preserve reuse while withholding
  recreation and ownership-repair authority.
- A failed Colima/nerdctl start that reports a stale health-check timer now
  fails immediately even if the container appears running. The error names
  the selected profile and exact recovery commands, instead of continuing
  readiness and gateway setup with an unresolved health check.
- Stopping a managed run now ends the process group it started through
  container exec in containers whose shell is dash or busybox. The stop
  previously reached only the group's leader there, so an Effigy task running
  inside the container kept running and kept its lock.
- `deploy apply` and `deploy redeploy` now refuse as not yet supported in v0.15,
  naming unexecuted state, hooks, health, release gates and digest pins instead
  of reporting success. Model/export and existing deployment report readers
  remain available. JSON refusals use the stable `not_supported` reason.
- Vault writes from CLI commands, local-dev upgrades and Rhai now serialize
  through the runtime kernel and atomically select complete encrypted
  generations. Existing vaults migrate on their next write, retaining the
  previous generation and preventing concurrent updates from being lost.
- Loopback allocation probes the runtime before taking its global registry lock and preserves assignments added or changed during that probe.
- A live managed headless run keeps its session record for its whole life:
  the record is written atomically before the first child starts and updated
  on every recorded state change, and it is never deleted while the owner is
  alive. When the record is missing or unreadable under a live run anyway,
  `effigy <task> status`, `logs` and `stop` still work: they find the owner
  through its task/profile locks and run token, report from the locks and log
  files, and deliver the stop through the same request the owner watches, so
  a `dev:container`-style warm-up can always be inspected and stopped. The
  controls refuse only when no live run of that task and profile exists, and
  every start hint names the actual task and profile instead of a hardcoded
  `effigy dev --headless`.
- Container lifecycle mutations respect live task/profile lock holders, even
  when a session record is absent. Lifecycle EXIT traps only record state;
  runner finish owns exactly one down per managed stop. Failed-generation
  recovery and lease expiry cannot down a live successor's environment, and
  handover must settle bound runs before stopping their environment. A live
  holder with unknown binding blocks mutation. Compose up uses `--no-recreate`
  under live holders, and exec-readiness recovery observes without restarting.
  The #634 warm-up exit-137s were caused by the proof harness's explicit
  `container stack down` cleanup after the missing-record stop failure, as
  confirmed by its command journal and Lima's ordered shutdown; the restart
  guard is separate hardening.
- Headless startup delays no longer block control requests or signal handling.
  The record is published before startup and updated as children start, and
  stop cancels children still waiting for their start delay.
- Managed headless and TUI stop lets the owner order remote and host child shutdown,
  exits successfully after earlier workload failures, and runs the bound
  environment cleanup even when the managed runtime failed. Stop checks the
  final cleanup result, and remote cleanup uses full-ID ownership labels even
  when inspect identity or exec readiness is unavailable. Detached startup
  recovers an unchanged, ownership-verified partial stack with missing or
  stopped services through ordinary Compose up, keeping its volumes.
- Headless `dev:container` app children now start and stay running on a
  healthy stack, on first start and after a handover. Three causes, found
  in the Acowtancy v0.15.0 adoption proof (#625): `container up` never ran
  workspace permission preparation, so Effigy-managed cache volumes (Cargo
  registry/git, target dirs) kept root-owned permissions from first
  creation or a reused/handed-over stack and the workspace user could not
  write — preparation now runs as an `up` integration stage after exec
  readiness is verified, before secrets, gateway exposure and readiness
  reporting. An Effigy invocation inside its own workspace container no
  longer probes host runtime commands: `container shell`/`container exec`
  and `exec` with `EFFIGY_INTERNAL_CONTAINER_HANDOFF=1` execute the
  requested command directly where the caller already is instead of
  spawning `colima`/`nerdctl`/`docker` (unreachable from inside). And
  managed children share the lifecycle's readiness budget instead of a
  fixed 30s exec-readiness timeout, wait while the lifecycle reports
  `starting`, and only then poll exec readiness with the same per-probe
  windows as `container up` — so a slow-but-healthy first start no longer
  times children out while the stack is still coming up.
- The headless managed `dev:container` lifecycle no longer refuses with
  "detached activation publication refused" while it starts an environment in
  a scoped `data_from` clone (Acowtancy adoption proof #625). The managed
  shell/exec readiness probes resolve the same container policy once per
  second while the lifecycle runs, and each resolution rewrote the admitted
  runtime Compose inputs (workspace mount rewrite and DNS override) with a
  truncating `fs::write`; a strict publication verify that read an input
  inside a rewrite window saw a torn or empty file and refused, leaving the
  stack partially activated with no routes published. Those runtime
  generated files — the per-environment generated Compose payload and
  regeneration checksum, workspace, ownership and DNS overrides — are now
  replaced atomically (temporary file plus same-directory rename), so every
  reader observes one complete generation and a killed writer leaves the
  previous complete file. Admission verification stays strict: real drift or
  a missing marker for a genuinely recorded partial activation still refuses,
  and a legacy `.effigy-compose.generated.yml` left behind by an older build
  never participates in the admitted inputs.
- Headless managed task start reuses an authenticated, running shared gateway
  whose identity the current user cannot read (permission-denied), printing
  one note that the identity could not be verified — the same way the hybrid
  workspace handoff path already reuses it — instead of refusing with
  "cannot determine gateway state". Route publication and withdrawal never
  gated on the daemon identity (they write the owner-checked route table
  directly); operations that signal, stop, replace, recover or start a second
  gateway still require a verified identity and keep their refusals. No new
  privilege, helper, or root-process read is added.
- A detached `container up` of an environment whose generated Compose input
  is shared per checkout no longer refuses with "detached activation
  publication refused" when another environment of the same checkout is
  materialized first (#196 regression from the #195 handover). Each
  environment now renders its own generated Compose payload and checksum
  (`.effigy/runtime/compose/<env>.generated.compose.yml` beside the existing
  per-environment overrides), so inspecting or handing over a sibling
  environment never rewrites the requesting environment's admitted Compose
  inputs between admission and publication. The first detached `up` of a
  listener environment works again on fresh clones, ephemeral-scoped clones,
  linked worktrees and primary checkouts, and admission verification stays
  strict: real drift or a missing marker for a genuinely recorded partial
  activation still refuses. Eject regenerates the same per-environment path.
- Shared-service project names (`effigy-shared-<catalog>-<8 hex>`, which also
  name their data volumes) now pin SipHash-1-3 with zero keys, the algorithm
  `DefaultHasher` uses today. Existing names stay byte-identical, and a later
  Rust upgrade cannot silently rename a shared service and orphan its data.
- Workspace dependency volumes (Composer `vendor`, JS `node_modules`) with one
  writable service and read-only consumers (for example an app service writing
  the volume while an Nginx service mounts it `:ro`) are classified by their
  actual writer set: the read-only consumers no longer make the volume
  "shared", so owned-volume permission preparation repairs it for the resolved
  workspace user instead of refusing. Volumes written by two or more services,
  external volumes, and ambiguous or malformed access modes keep the previous
  non-mutating behavior, and a read-only primary mount never gains mutation
  authority from a writer elsewhere. Nonrepairable dependency volumes are no
  longer silently omitted: preparation checks numeric-user access on them and
  blocks readiness on failure, and `effigy doctor` reports an actionable
  `container.workspace-ownership` finding for an unusable dependency volume
  instead of a clean bill. Same-parent dependency trees (`vendor` beside
  `node_modules` as separate volumes) each get their own recursive repair.
- A checkout can switch between its container environment and hybrid managed
  host-listener mode in either direction without errors or manual cleanup. A
  managed listener hands over a gateway route published by the same checkout
  and scope in one locked transaction, and container registration takes the
  domain back once that listener generation is no longer live. A still-live
  managed listener is resolved by the whole-environment handover, and
  another checkout or scope still refuses; container teardown no longer
  removes a handed-over route.
- Automatic local Linux artifact rebuilds now use the stable managed builder
  project `effigy-artifact-builder-v1`, preserving checkout/worktree isolation.
  Ambiguous legacy builders and their admissions/caches remain untouched; no
  manual reset is required. Future builds reuse the new strictly owned builder.
  Its separate caches may require downloads and additional disk space.
- Vault-generation regression coverage uses the production CLI boundary without
  leaking its internal generation marker into parallel Rhai unit tests.
- Container preflight and workspace catalog preparation use the existing
  transport/plan boundaries while preserving their bounds and ownership checks.
- `[secrets.vault].generate` Rhai hooks can now create `required`
  rhai-targeted secrets that are not in the vault yet. Previously the Rhai
  secret store refused the hook with `required Rhai secret(s) missing from
  the vault`, so fresh clones could not bootstrap. The skip applies only
  while the runner's generate hook runs; every other Rhai secret read still
  refuses missing required keys, and vault, passphrase, declaration and
  target checks are unchanged.
- Listener-handoff fixture closure checks retain socket identity across numeric
  descriptor reuse in parallel release tests.
- Supervision-initialization fixtures exclude parallel shared-scope installation
  through the existing signal-proof and environment lock pair.
- Managed-listener integration fixtures coordinate deliberate route edits with
  lifecycle writers and retain private gateway refusals on HTTPS failure.
- Release checks isolate baseline doctor fixtures from the developer's catalog
  store, and private catalog-pinning fixtures admit the compiled release version.
- Effigy-managed service host aliases no longer hang while their mapped
  localhost ports are ready. Compose generation and gateway registration now
  share one loopback-IP reservation covering every other project's
  container-route `dns_ip`, so a new stack never publishes on an IP a stale
  route's gateway TCP listener still binds and both paths keep the same IP
  for one identity. `effigy gateway status` and `effigy doctor` now report
  TCP alias routes with a conflicting bind tuple or a missing/invalid
  mapped-port fallback, naming the alias and the fallback; the status JSON
  gains additive `tcp_alias_unbound_count`/`tcp_alias_unbound` fields.
  Deleting stale routes for missing checkouts stays with
  `effigy container retire --dead` (#305).
- When a running Colima VM comes back from a restart with containerd and
  BuildKit stopped, `effigy container up` and container tasks now start those
  units inside the VM and continue, instead of waiting and then restarting the
  whole VM, which took down every stack on it and could itself time out.
- Container readiness, tasks and `container up` list only the selected Compose
  project's containers (runtime label filter) instead of every container in a
  shared VM, so the preflight inventory no longer times out on busy machines
  holding many stacks.
- Task locks are published atomically and carry an owner token. A concurrent
  run can no longer see a half-written lock, delete it, and start alongside the
  owner, and a finishing run never removes or refreshes a successor's lock. An
  unparsable lock is reclaimed only after a short grace period, so a lock still
  being written by an older Effigy is not stolen.
- Container exec preparation gives owned-service inventory and inspection a
  bounded 15-second window separate from the initial workload probe. Valid
  Colima stacks no longer exhaust a shared two-second window before exec;
  explicit caller deadlines and strict immutable-ID ownership checks remain.
- macOS managed-listener inspection accepts a successful empty descriptor list
  from a live same-user process, instead of failing with stale errno. Socket
  detail queries preserve native failures rather than treating every zero return
  as an absent socket. Permission/unknown observations still refuse, with the
  same ownership checks and readiness deadlines.
- A container task no longer attaches a 5-minute idle-shutdown lease to a stack
  you brought up explicitly with `effigy container up`, and a pending lease is
  dropped when you do. `effigy container down` - including `--global` and the
  cwd-scoped form - and `effigy container retire` end that persistent ownership,
  so a later task-started stack is leased and reclaimed again. Previously a task
  that reused or restarted such a stack could tear the whole stack down five
  minutes later, and a `down` outside the checkout left a stale owner marker that
  blocked later leases. The lease notice now names
  `effigy container up --detach` and `EFFIGY_DISABLE_HOST_CONTAINER_LEASE=1`.
- Redirected `effigy changelog extract` output omits the presentation banner, so
  extracted release notes contain only the selected changelog content.
- `effigy doctor` no longer reports `{ task = "defer …" }` and other builtin
  command references as unresolved tasks, and accepts every top-level manifest
  table (`isolation`, `state`, `deploy`, `data`, `env_schema`, `qa`). A manifest
  task that shares a builtin's name is still validated as a task.
- `effigy <task> --plan` describes a run-less task bound to a named container as
  `workspace shell`, matching what running it does, instead of failing with
  "missing `run` command".
- `effigy release proof` installs a `publish = false` package from its git tag
  instead of attempting a registry install that fails, or could resolve an
  unrelated crate of the same name. The Homebrew upgrade step now upgrades the
  configured formula rather than `effigy`.
- OCI artifact staging refuses pulled file paths that are absolute, climb with
  `..`, or resolve (including through a symlink) outside the pull directory.
- `effigy test --json` never opens the interactive test TUI, so its output stays
  a single JSON document on an interactive terminal.
- Task-status history records from two runs of one task that finish in the same
  second no longer overwrite each other.
- Container secret runtime files are staged on the host in an exclusively
  created, owner-only temporary file.
- `effigy container up --detach`, container tasks, `effigy exec` and gateway
  route publication no longer fail intermittently with "not exec-ready" on
  Colima. Readiness again treats its 2s/15s windows as retry windows rather than
  hard limits that killed in-flight probes, and owned-binding checks allow for
  Colima's per-call SSH hop.
- Detached `container up` applies changed manifest inputs to a stack whose last
  activation completed, and adopts running stacks started by v0.14.1 or by a
  container task, once every project container, running or stopped, is proven to
  belong to this checkout. A stopped declared sibling counts because a
  project-wide Compose up would start or recreate it. Recorded partial
  activations keep refusing changed inputs, and containers that may belong to
  another checkout are never recreated.
- `container shell --bounded` verifies, inside its budget, that the selected
  service container belongs to this checkout, then runs the command and copies
  workspace artifacts to that exact container. On Colima it previously could
  copy into another service's container.
- Workspace catalog handoff works on Colima: it identifies the workspace
  container by Compose labels (nerdctl's `compose ps -q <service>` lists the
  whole project) and stages the pack inside the checkout, which Colima mounts,
  instead of the host temp directory, which it does not. The copied pack is
  handed to the workspace user, since runtime `cp` creates it as root and its
  cleanup would otherwise fail. Copy failures now report their cause.
- A stopped service reports "container service `<name>` is not running" with
  the `container up` command again, instead of "binding unavailable".
- Workspace handoff never reuses a stale Linux workspace artifact. A prepared
  artifact is current only when its identity sidecar matches the host identity
  and its rehearsal receipt names the target. A stale, unknown, or wrong-target
  artifact is rebuilt through `workspace:linux:artifact`, or the handoff refuses
  before copy or exec with the known host/artifact versions and identities and
  the target-specific rebuild command. A build that exits successfully without
  writing fresh output is refused, and sidecars are bound only after a build or
  download justifies them. Download-mode cache entries are reused only when their
  identity names this release. Existing timestamps and file presence no longer
  qualify an artifact. The offline hint no longer recommends reusing the
  incompatible artifact. Reuse also requires the current active-version
  sidecar. A successful build that only touches unchanged old bytes is refused;
  the temporary digest comparison grants no provenance. Concurrent preparation
  checks final metadata after the build lock releases, and bounded refusals
  name the actual target-specific rebuild command.
- Direct `compose_file` containers carry the same checkout and scope identity
  labels as generated containers, rendered as a Compose override beside the
  repo-owned file. The owned-binding check is unchanged. Existing unlabelled
  direct containers are refused, not adopted, until a Compose `up` recreates them
  with the labels.
- Direct `compose_file` exec readiness now runs the owned-binding check before
  its probe and before any recovery restart. An unlabelled, foreign or wrong-checkout
  primary service is neither exec'd nor restarted, and the up reports not
  exec-ready. Readiness retains the inspected full ID through ordinary native
  Docker/nerdctl exec and registration, refusing replacement before effects.
  Builtin container suites also render verified ID-targeted exec. Separate
  inspect/exec observations do not promise an atomic remote transaction.
- A root-owned shared gateway on macOS now verifies managed listener
  exclusivity against the listener owner's processes instead of only root's,
  so user-owned managed listener routes no longer return 503.
- Detached activation errors now retain a bounded `activation_stage` alongside
  partial/unavailable status, without exposing raw backend output or arguments.
- Detached reconciliation refuses a running stack whose original admission
  marker is missing, including after a failed marker write. Unscoped binding
  verification rejects unexpected scope labels. Detached JSON retains bounded
  managed-listener readiness classifications without raw backend output.
- Recurring nerdctl health-check collisions now name the missing preservation
  interface when service-only recreation is unavailable: declaration-to-mount
  association, exclusive volume/data ownership, and a supported backend plan.
  The refusal explains that `container retire --yes` removes mutable volumes
  and is not restart recovery. A private command-adapter fixture verifies the
  recurring collision and retained file markers without a live backend.
- Detached scoped bring-up now retains an admitted context bound to checkout,
  backend/project and manifest/Compose hashes. Partial and warm reconciliation
  skip Compose up and primary restart, require fresh full-ID published bindings,
  and refuse drift, unavailable ownership or a missing primary. JSON failures
  expose bounded partial/unavailable details. Explicit down clears the context
  and keeps volumes; retirement removes owned mutable data.
- Updated the locked rustls dependency from 0.23.45 to 0.23.46 for
  RUSTSEC-2026-0346 through RUSTSEC-2026-0349, without changing TLS features,
  cipher selection or authentication policy.
- Detached route publication now checks the original admitted source and marker
  after runtime probes, under the route-table lock before preparation, and
  immediately before save. Drift or missing context preserves existing routes
  and refuses without a new source baseline, recreation or teardown. The table
  lock does not exclude source-file writers or guarantee atomic source reads.
  Both Direct and Generated preparation seal declarations before runtime
  observations and join legitimate artifact refresh through checked renderer
  receipts, refusing probe-time input drift before activation/publication.
- Detached Compose launch isolates service writers with null output sinks.
  Finite inventory capture drains nonblocking pipes without persistent reader
  threads, caps retained output at 1 MiB per stream, refuses incomplete EOF or
  overflow, and positively reaps its owned client on timeout. These mechanisms
  have private process/transport evidence, with no claim about original SIGPIPE
  provenance or a real Linux/containerd backend.
- Managed host cleanup keeps its owned TERM/KILL sequence when bounded
  completion observation is unavailable. It retains the original stop clocks,
  requires positive group settlement and actual wait before reaping, and holds
  persistent uncertainty or an actual wait failure.
- Standard container task commands now render `{repo}` and `{project}` with
  the container-mapped catalog root when invoked by a host aggregate. Task env
  aliases and selector plans use the same runtime path; nested catalogs keep
  their relative suffix. Host configuration lookup, literal paths, arguments,
  scheduler admission and existing sequence/managed routes are preserved.
- Managed host listeners preserve readiness failures and phase diagnostics
  through rollback and teardown. Supervisors settle surviving owned descendants
  before reaping an owned child leader during cleanup, and retain unavailable
  state when cleanup cannot be proved.
- Replace the archived YAML parser dependency with the maintained YAML organization compatibility backend.
- JSON contract fixtures preserve authenticated QA-parent roots for nested skill
  and group execution, keep literal path arguments intact, and clean up their
  generated repositories at the end of each checker invocation.
- Doctor health subprocess capture now supervises exit and inherited output
  against the same deadline, retaining its owned process group until final
  drain or cancellation instead of joining readers after an early reap.
- Keep JSON version identity consistent within one response while a local
  checkout changes.
- Bound final output draining after a client exits, and retain the owned
  process-group leader until cancellation finishes so inherited writers are
  terminated and the direct child is reaped.
- Optional workspace mounts are now resolved after container preparation at
  the Compose up boundary. The bounded decision report distinguishes the
  submitted Compose spec from unverified runtime mount state, and missing
  optional paths no longer produce misleading diagnostics during config loads.
- Managed listener transition receipts now expose a missing initial sequence,
  preserve readable v1 history while new writers append v2 observations, and
  sync the containing directory when creating the receipt. Dependency
  inspection failures identify the declared dependency and bounded predicate
  without recording raw errors or secrets.
- `container retire` resolves its port registry through Effigy's home resolver
  instead of the caller's `HOME`, so unit fixtures can no longer release or
  rewrite a real `~/.effigy/ports.json`.
- Managed host listener readiness no longer half-closes its one-shot HTTP
  request before reading. Ordinary HTTP/1 servers that drop a response after
  EOF are now probed correctly, and a status line split across writes is read
  to its line break. Connection failures, malformed or missing status lines,
  wrong statuses and silence remain not ready.
- Managed host listener ownership on macOS no longer fails the whole process
  inventory when `ps` reports a system daemon's UID in signed form (for
  example `-2`). The UID is mapped to its unsigned value and compared with the
  effective UID; malformed or out-of-range rows still fail closed.
- Failed managed host listener startup keeps a bounded `diagnostic` in its
  state file. It records report, ownership, HTTP probe and route observations
  for the failed generation after its report and child are cleaned up. The
  diagnostic is evidence only and does not change ownership or publication.
- The changelog parser accepts free prose paragraphs inside a release section,
  directly under a version heading or between category headings, so
  `effigy release simulate`, `release prepare`, `changelog format` and
  `changelog extract` work with prose-bearing consumer changelogs. Prose is
  preserved on format, extraction and release promotion; malformed headings and
  entries are still rejected.

## [0.14.1] - 2026-10-08

### Added
- `effigy gateway recover` restores an ordinary upgrade past a live
  pre-identity gateway. It captures the trusted legacy PID/version pair,
  holds `gateway.transition.lock` for `up`/`down`/`recover`, inspects a live
  candidate through the bounded elevated `__gateway-legacy-candidate` reader
  (canonical live path, kernel UID, boot, precise start, UDP 15353 plus TCP
  80/443 role sockets), requires interactive digest consent (`--yes` cannot
  adopt), and stops only that adopted generation through
  `__gateway-legacy-stop` with per-signal revalidation. The adoption prompt
  renders the live executable path as lossless printable-ASCII with every
  other scalar escaped (`\n`/`\t`/`\r`/`\\` and `\u{xx}`), including bidi
  and zero-width format characters; the candidate digest still binds the
  canonical path. Version-only, malformed, or symlink `gateway.version`
  records are unknown in the shared lifecycle classifier, so `status`/`up`/
  `down` and managed auto-start refuse before start can overwrite them;
  version publication refuses to follow a symlink target. Confirmed selected-
  generation absence then compare-and-removes the unchanged records and
  releases the lock before a normal `gateway up`. A missing gateway directory
  is the absent-only start path. `recover --json` refusals emit
  `effigy.gateway.recover.v1` (`ok: false`) on stdout; `status --json` emits
  `effigy.gateway.status.v1` the same way. Unlocked `up` and `status` classify
  without stopping, deleting records, or staging elevated state; mutation
  runs after the transition lock (or in the elevated child that acquires it).
  Privileged vacant first-start stages operator-owned files under that lock,
  then releases before elevation. A vanished captured PID/version pair is
  refused as a changed record. Lock acquisition, elevated-state preparation,
  and daemon spawn refuse a symlink `.effigy` ancestor before creating a
  missing gateway parent, so the symlink target is left untouched. Custom,
  partial, or ambiguous endpoints refuse with no confirmation path. Managed
  and container auto-start still refuse a legacy record and point at recover.
  The local installer stages owner-only `effigy.previous` plus `.version` as
  evidence; recover never executes it. Live role evidence is not historical
  spawn proof; the last
  check-to-signal interval remains a disclosed TOCTOU.
- Gateway lifecycle commands now identify a numeric-only pre-identity record
  as requiring operator migration, distinct from an unavailable process
  probe. They preserve its files and refuse to signal, clear, or replace it
  from `up`/`down` or managed auto-start; `effigy gateway recover` is the
  supported consumer path, including when the previous executable was already
  replaced.
- `effigy release:verify-install` is a maintained heavy-admission selector for
  Effigy's post-publication tagged-source install proof. Invoke it through
  `./target/debug/effigy`; it runs the fixed child command
  `./target/debug/effigy release verify-install {args}` and forwards
  shell-quoted task arguments without replacing the installed local Effigy.
  This source follow-up is after v0.14.0 and is not in the published binary.

### Fixed
- Managed gateway build replacement now uses a local-only read-only preflight.
  If the recorded live identity is inaccessible, one existing elevated
  `gateway up` lifecycle performs the locked replacement; its process polls
  and immediate pre-TERM/pre-KILL checks read identity afresh without launching
  another administrator reader. A noninteractive or declined handoff preserves
  records and starts no replacement. Post-signal process-probe uncertainty is
  retried only within the existing bounded wait; cleanup and restart still
  require confirmed absence or a readable different generation. A failed
  signal is reconciled only after a fresh confirmed-absence probe, and
  persistent unknown evidence remains refused.
- Managed gateway auto-start keeps the operator terminal on the child's stdin
  (`sh -lc` now inherits stdin instead of replacing it with null) so the
  bounded read-only elevated identity reader can authenticate, while
  stdout/stderr stay captured for diagnostics. A noninteractive, declined, or
  otherwise unknown identity read still returns Unknown, so the lifecycle
  refuses without signalling or launching a replacement. Startup text now
  reports the preflight state
  (stopped, replacing a different build, or unverified) instead of calling an
  unknown or mismatched live daemon simply "down".
- macOS gateway boot identity is now the stable `kern.bootsessionuuid` session
  identity with validated command status and canonical-UUID parsing; missing,
  failed, empty, or malformed output is Unknown rather than a guessed value.
  Records written before this change stored the `kern.boottime` timeval, whose
  microsecond field drifts and whose seconds component is wall-clock adjusted
  within one boot. Those records still match the live daemon they published
  through the boot-time seconds plus the still-mandatory exact process start
  identity. A changed, unreadable, or malformed legacy value is Unknown: the
  lifecycle preserves the record and refuses instead of deleting a live
  daemon, so an old sidecar is never reclassified as absent or rewritten to
  match. That ambiguous case is honestly refused rather than guessed at; the
  approved `effigy gateway recover` route currently rejects sidecar-bearing
  records, so there is no automatic supported transition for an ambiguous
  identity-bearing record yet. A bounded, implementable specification of the
  recovery input boundary and the ordered confirmed-absent cleanup (resumable
  only from the full triple or the numeric-only pair, with a version-only
  remnant kept fail-closed and actionably refused) is recorded in architecture
  034 for an operator ruling; it is not implemented and does not widen the
  numeric-only route.
- Elevated gateway lifecycle now accepts the authenticated operator-owned
  gateway directory instead of rejecting it as unsafe. An elevated root caller
  behind the existing administrator-elevation marker trusts a directory owned
  by the forwarded operator UID only when that UID is non-root, matches an
  ambient `SUDO_UID` when present, and owns the forwarded `HOME` per the
  system passwd database. Unauthenticated root ownership checks, parent
  safety, no-follow file checks, owner-only `0600` records, fail-closed
  legacy/unknown handling, and fixed HOME/path target binding are unchanged.

## [0.14.0] - 2026-10-06

### Breaking
- Retired Effigy's heavy-admission lease store, store-writing backend, and
  `effigy admission status|run|runs` commands and JSON schemas after Queue
  removed its legacy joins. `EFFIGY_ADMISSION_DIR` is no longer read;
  `EFFIGY_HOST_SCHEDULER=0` now returns an unsupported-retired diagnostic
  before effects. Existing default or custom state files are untouched and
  treated as opaque historical data; no reader or migration is added. The
  prior `48183cf` binary is local rollout evidence, not a published consumer
  rollback target.
- Removed the Rhai object-storage host surface (`storage::provider`, `status`,
  `ls`, `head`, `get`, `put`, `delete`), the `effigy-rhai` `s3` dependency, and
  the vendored `vendor/s3` library, per the independent-retirement ruling in
  contract 043. Scripts calling any removed helper now fail with an
  unknown-function error and no compatibility shim exists. Live objects,
  buckets, secrets, and assets are untouched; generic MinIO catalog services
  and independent consumer S3 adapters are unrelated and unchanged. Residual
  consumer callers (for example `bovine-accelerator` Rhai tasks) break by
  design and are retired in their own repository.
- Lean Northstar repositories no longer carry a Queue manifest. `effigy init
  northstar` omits it; Queue closeout is the default, and repository overrides
  use Queue's `repository.set` / `repository.get` settings API. Project
  membership remains on `project.get` / `project.upsert`.
- Removed the `effigy papercuts` command and JSON schemas, `effigy-papercuts`
  crate, Northstar starter papercut file, contract 036, and guide 078. Queue's
  papercuts table owns capture and inventory after the portfolio import.
- The default `effigy docs` log index path and distribution closeout output move from `docs/logs/` to `docs/notes/`. Pass explicit paths for repositories retaining the former layout.

### Added
- `effigy release:prepare` is a maintained, `admission = "heavy"` selector for
  authorized in-place release preparation. It admits the whole operation before
  any compilation or file mutation, fixes the subcommand to `release prepare`,
  rejects `--repo`, enforces `--check-gates` for a mutating run, and forwards an
  explicit target version to the Effigy built from the invocation checkout. It
  never selects `execute`, `resume`, tag or publish, and it deliberately keeps
  prepared state in the invocation `main` checkout; ordinary milestone QA stays
  on Queue `project.qa.run`. `effigy release:prepare --plan` is the outer,
  no-write task plan, while `-- --plan` runs the inner prepare plan.
- Gateway lifecycle now persists `gateway.identity` beside the compatible
  decimal PID file and verifies boot plus precise process start identity before
  reporting or signalling a gateway. Legacy PID-only or unknown records are
  preserved and refuse lifecycle actions; root publication keeps records
  readable by the gateway-directory owner. When the kernel denies a live
  identity read, the existing administrator prompt can run a bounded,
  read-only check of the trusted gateway target. Authentication failure stays
  unknown. Old binaries ignore the sidecar on rollback, and the final
  identity-check-to-signal interval remains a documented TOCTOU.
- GitHub Releases will include verified, unsigned SHA-256 sidecars for both
  macOS binaries starting with the next normally authorized release. The raw
  binaries keep their existing names.
- Added a [v0.14.0 consumer migration checklist](docs/guides/083-v0.14.0-consumer-migration.md)
  covering breaking changes and compatibility from v0.13.1, with an
  incremental first-parent supplement for the later reviewed repair wave.
- Unset or `EFFIGY_HOST_SCHEDULER=1` routes heavy tasks (`qa`, `ci`, `ci:fresh`,
  `admission = "heavy"`) and heavy QA groups through the host-run scheduler
  (Queue/Nucleus owns capacity and scheduling). `EFFIGY_HOST_SCHEDULER=0` is
  retired and rejected before task effects; malformed values also fail closed.
  The scheduler launches the same invocation with a run token; nested calls
  reuse it in place, and an invalid token exits 77. An unreachable scheduler
  fails heavy work with exit 75 (no legacy fallback);
  `EFFIGY_SCHEDULER_OVERRIDE=<reason>` records a
  durable override and runs directly. Output, exit status and JSON envelopes are
  the child's; `capacity_timeout`, prelaunch cancel, run timeout and lost
  runs stay distinct, and interrupts ask the scheduler to cancel. Owned
  container start/removal facts report `true`, `false` or `unknown` and replay
  until acknowledged. QA-group run records gain an optional `backend` object
  (schema stays `effigy.qa-group-run.v1`). Run stop/logs commands and group
  `hard_timeout_ms` remain unavailable. Run tokens are strict per contract 010
  at pin `16fcb59`: keys are 44-character RFC 4648 standard padded base64 of
  exactly 32 bytes, token parts are unpadded base64url, claims are exactly
  `runId`/`epoch`/`class`/`root`/`exp` with integer UTC millisecond expiry
  (valid strictly `now_ms < exp`), and anything else — URL-safe or unpadded
  keys, padded or standard-alphabet token parts, RFC 3339, string or fractional
  expiry, unknown or out-of-range claims — exits 77 without queueing.
- Host-run followers recover a lost attach for at most five monotonic seconds
  with capped backoff, and only for endpoint absence/refusal or transport
  closure. Recovery revalidates trusted authority, peer identity, epoch and
  token keys, checks status for the same run, and resumes stdout/stderr at their
  exact delivered byte offsets. Trusted status or attach responses without
  stream-offset progress do not reset the five-second budget; advancing output
  or `output_expired` offsets do. Interrupt cancellation retries the same run
  ID through the same bounded endpoint roll. It never resubmits; trust/protocol
  failures fail closed, and exhaustion reports exit 75 with final state unknown.
- Host-run followers now treat socket read timeouts during attach as temporary
  idleness. They keep the same connection and any partial frame bytes while
  waiting for later output or settlement; request/status deadlines and the
  bounded endpoint recovery window are unchanged.
- Bounded QA groups (contract 051) land through the existing task pipeline:
  `effigy tasks qa-groups list [FILTER] [--file PATH]` inventories maintained
  `[qa.groups]` definitions plus at most one explicit temporary definition
  file, and `effigy tasks qa-group run/status/logs` resolve a non-executing
  plan (`--plan`), run every member sequentially through the standard
  pipeline, and persist a versioned run record with run-scoped
  pipeline-redacted logs. Group selectors resolve only on the group surface
  and never fall through to a same-named task or draft; temporary
  definitions are selected only by their explicit `--file` (relative paths
  resolve inside the selected repository) and carry a content digest.
  Required/advisory scope comparison returns `needs_planner` with exact
  tokens and reasons before any run exists; a declared match still runs
  every member. Heavy groups run as one Queue/Nucleus scheduler invocation;
  nested task references reuse its token, with scheduler wait reported apart
  from execution;
  over-budget evidence never changes check outcomes. Member records carry
  the task's real exit code (including 130, classified as cancellation for
  the member and the group), never a placeholder. `tasks qa-group stop`
  and `hard_timeout_ms` parse only to be refused before any side effect:
  owned-run supervision (contract 052) has not landed, and
  interrupted runs reconcile to `unknown`, never a pass. Payload schemas:
  `effigy.qa-groups.v1`, `effigy.qa-group-plan.v1`, `effigy.qa-group-run.v1`,
  `effigy.qa-group-status.v1`.
- `[drafts]` accept the same optional `admission = "heavy"` metadata as
  `[tasks]` (contract 046 extension). A declared heavy draft opts into heavy
  admission on direct runs and is classified heavy inside QA groups; absent
  metadata keeps today's ordinary classification, and the drafts inventory,
  direct draft plan, and run JSON carry the field only when a definition
  declares it. `effigy draft <SELECTOR> --plan` now also parses: the
  documented non-executing draft inspection was rejected by the parser
  despite the contract, and a global `--repo` before `draft` was silently
  ignored; both resolve against the selected repository now.
- `effigy tasks --json` (`effigy.tasks.v1`) carries an additive `selectors`
  inventory: one deterministic array of invocation-ready names
  (`effigy <selector>`) with `kind` (`task`, `managed-profile`, `builtin`),
  `source` (`catalog`, `builtin`), and the catalog alias (`null` for
  builtins). Query `.result.selectors[].selector` instead of flattening
  `catalog_tasks`, `managed_profiles`, and `builtin_tasks`, which stay
  stable. Display-only empty-catalog rows contribute nothing and deferred
  builtins stay excluded.
- `fs::write_file_if_absent(path, contents)` publishes a complete Rhai file
  payload to an absent destination with atomic create-if-absent semantics: the
  payload is staged on the destination filesystem and linked into place, so
  exactly one concurrent publisher wins and a reader never observes a partial
  winner. A losing call returns `false` without altering the winner; an occupied
  destination, including a dangling symlink, is never replaced. Failed staged
  writes clean up their temporary file, unsupported filesystems fail clearly,
  and `fs::move_path` now documents its replacement and race limits.
- `effigy container scope --json` reports the selected checkout's absolute
  path, scope kind and full generation token before archive without loading a
  container policy or contacting a runtime backend. It creates a missing
  token only for linked worktrees and locally marked ephemeral clones.
- `effigy release prepare` can satisfy a gate marked
  `reuse-hosted-evidence = true` with a successful GitHub Actions run for this
  repository's exact `HEAD` SHA, verified through authenticated `gh`. Unlisted
  gates still run locally. Missing, pending, failed, ambiguous,
  wrong-repository, or wrong-SHA evidence fails closed. The release record
  names each reused gate and links to its run.
- Task lock waits are bounded by `--lock-wait-ms` or `EFFIGY_LOCK_WAIT_MS`.
  Independent selectors keep distinct `task:<selector>` locks. A timed-out
  waiter names the live owner and `effigy tasks status <selector>`; that
  query keeps the still-running owner, including in-process sequence tasks,
  so the caller can retry after release.
  Lock-wait JSON stays in `--json` `error.details`. `effigy tasks unlock`
  accepts a task selector such as `validate:activity-routing` as
  `task:validate:activity-routing`.
- `effigy <selector> --plan` and `effigy draft <selector> --plan` resolve the
  task, catalog, and command without starting a task process. `--plan` is
  consumed by Effigy and is not passed through to the task. Unresolved
  selectors do not run deferral, exec aliases, or other builtins;
  `effigy test --plan` keeps the test builtin. `--json` remains an output
  format: `effigy --json <selector>` still executes.
- Graph lookups name a live refresh-lock holder (pid and age) and serve the
  last complete snapshot as `stale-index` instead of reading a partial rebuild
  or spending the query budget on the lock. Without a snapshot they return
  `missing-index` and do not query the live database. The snapshot is published
  after the search index is rebuilt, via an atomic replace that keeps the prior
  file until the new copy is complete. `--stale-index` reads that snapshot
  without waiting or refreshing; a cold checkout returns `missing-index` and
  names `effigy graph index --json`. A dead lock pid is a stale holder and does
  not consume the full in-flight wait.
- Container-routed worktree tasks verify the live checkout mount and refresh
  changed Cargo inputs before execution. Built-in Bun tests report the frozen
  `bootstrap deps sync` route when a fresh worktree lacks dependencies.
- `effigy bootstrap deps sync` runs Bun installs frozen against a committed
  `bun.lock` or `bun.lockb`. Use `--refresh-lock` to explicitly generate or
  refresh `bun.lock`; successful managed Bun installs also remove Finder and
  AppleDouble metadata copied into `node_modules` without changing local
  dependency sources.
- Linked Git worktrees receive stable, distinct generated-Compose identities
  and allocated host ports by default. `share_runtime_identity = true` opts
  into a shared Compose stack.
- Gateway route claims and teardown now check worktree generation ownership
  under a route-table lock, including TLS and TCP aliases; live foreign domain
  claims fail with the owning checkout path.
- Linked worktrees expose one effective host map (`effigy container hosts`)
  that rewrites declared HTTP routes and TCP aliases onto
  `<apex>-w<scope>.test` while the primary checkout keeps declared names.
- A full clone marked `effigy.runtimeScope = ephemeral` in its local Git
  config now receives the same isolated runtime identity as a linked Git
  worktree: a token in the clone's own `.git/effigy-runtime-scope`, distinct
  generated Compose names (`-ec-` tag) and allocated ports, the same
  effective gateway hosts (`scope.kind = "ephemeral-clone"` in `container
  hosts --json`), gateway route ownership, and `container retire` with
  retry-by-token after the clone is deleted. The marker is read from the
  checkout's own `.git/config` only — no global or system Git config, no
  include expansion — and no shared-Git-directory mount is added because a
  marked clone is self-contained. Unmarked clones keep primary-checkout
  behavior; recreated clones get a new generation token.
- `effigy container retire` removes one runtime scope's owned containers,
  networks, mutable volumes, isolated routes and ports using label proofs
  and a durable record under `~/.effigy/runtime-scopes/`, so cleanup can
  retry after the checkout is gone. The record aggregates every container
  environment and runtime profile in that worktree. Shared, persistent,
  external, and shared-identity resources stay, including repo-owned
  Compose volumes unless labelled mutable. Discovery failures and
  malformed inspect output keep the record. Deletion uses the profile that
  produced each labelled observation. TLS certificates are recorded, then
  removed under the route lock after an owner check; a foreign claim keeps
  its certificate. A second retire is success
  when nothing owned remains, and a shared-only retire then drops the
  durable record.
- Generated Compose and gateway registration now reuse one checkout-qualified
  loopback assignment. Retirement removes matching qualified and legacy keys
  only when their recorded checkout scope proves ownership. Stale reclamation
  requires complete Docker and Colima inventory; listing, runtime, or parse
  failures preserve assignments and report why reclamation was skipped.
- `effigy secrets unlock` accepts one operator passphrase, repairs the local
  dev key, and allows direct vault commands and secret-backed tasks to run
  without a prompt. `effigy secrets lock` revokes both local unlock files.
- `qa:docs` now runs `effigy changelog validate CHANGELOG.md`, so docs QA
  applies the same strict changelog heading grammar as release status and an
  em-dash release heading fails before review.
- Doctor rejects empty dependency entries and package symlinks into a
  standalone child's parent before running health. Installed packages require
  readable, named package metadata within the authorized install boundary;
  exports-only and type-only shapes remain valid. Local manager store links
  and verified workspace store links remain supported, including isolated
  workspace layouts.
- `effigy doctor --deep` checks the selected catalog's declared
  `[package_manager].js` and local JS install posture before running its
  `health` task.
  A child scope with a declared JS dependency and no complete local
  `node_modules` now reports `health.task.bootstrap` and names
  `effigy bootstrap deps sync <path>` instead of running health and letting
  Bun/Node resolve packages from an ancestor checkout. The guard verifies the
  declared runtime, development, and peer dependencies are present, so an
  empty or partial child install reports the missing dependency names too. A
  `node_modules`
  symlinked into an ancestor checkout or to a file is not a local install and
  cannot bypass the guard; a complete child-local `node_modules` directory
  satisfies the precondition even without a child lock. A scope with its own
  git boundary cannot be satisfied by an ancestor install, so a standalone
  child repository with no complete local install and no in-boundary lock is
  reported as `missing-local-lock` even when an ancestor provides a lock and
  install. An ancestor lock only satisfies the scope when the
  ancestor declares it in the manager-authoritative workspace source
  (`package.json` `workspaces` for Bun/npm, `pnpm-workspace.yaml` `packages`
  for pnpm) without matching a nested path below a declared package; the
  sources are never unioned, and an unverified ancestor lock is reported as a
  gap. A verified member also shares the ancestor install only when the
  manager's effective layout hoists member dependencies: Bun uses the hoisted
  linker unless `bunfig.toml` `[install] linker = "isolated"` or a
  `configVersion = 1` workspace lockfile selects isolated installs, and under
  hoisting a self-contained member (`installConfig.hoistingLimits =
  "workspaces"` or the root `workspaces.selfContained` list of member paths
  or names) needs its own `node_modules`; pnpm's default isolated layout and
  npm's `.npmrc` `install-strategy = nested | shallow | linked` also need a
  member-local install, while npm's default and pnpm's `node-linker=hoisted`
  share the root. Non-JS catalogs and dependency-free children keep running
  health unchanged.
- Focused validation selectors cover runtime inventory participation
  (`test:containers:runtime-inventory`), loopback reclamation
  (`test:containers:loopback`, `test:containers:loopback-policy`), backend
  selection (`test:containers:backend-selection`), and gateway registration
  (`test:gateway:registration-loopback`).
- The public `HostRunRoot`, `ScriptContext`, `RouteTableLock` and
  `LiveRouteTable` types now implement `Debug` for caller diagnostics.
  `HostRunRoot` and `RouteTableLock` redact their raw descriptors, and
  `LiveRouteTable` neither locks nor traverses the live table, so formatter
  output cannot block on the route lock or expose live route contents.
  `TokenKeys` and other secret-bearing types remain without `Debug`.

### Changed
- Release preparation can opt in to syncing a catalog pack support policy with
  the selected release before gates run. The policy joins the existing
  mutation preview and rollback protections.
- Plain primary-service `effigy exec` now requires a running service and
  reports explicit `container up` guidance when it is stopped, without
  starting the VM, service, or gateway. Workspace preparation batches
  metadata and numeric access checks, repairs only dirty owned volumes, and
  rechecks mutable paths on every invocation because they have no exact cache
  invalidation signal.
- Heavy host-run submissions now send only reviewed Cargo/Rust runtime controls,
  routing settings, and process variables explicitly referenced by the selected
  task or its transitively composed published tasks and drafts. Heavy QA-group
  submissions union those names from each resolved member using its declared
  task or draft surface. Resolution follows existing catalog selectors and
  guards composition cycles. Explicitly named process profiles retain their
  existing process-environment fallback, while catalog-defined and qualified
  profiles resolve from their manifests. Scheduler-provided PATH, HOME, run ID
  and token, ambient credentials, and unrelated shell variables are excluded.
- Host-run follow errors now direct unknown results to Queue/Nucleus for
  reconciliation by run id; they no longer recommend a nonexistent
  `host-run status` command.
- Read-only doctor workspace ownership checks batch the known mount metadata
  samples and numeric uid/gid read/write tests under the shared doctor
  deadline. Overlapping Rust cache paths reuse nested samples; wrong
  permissions remain path-specific findings, while an expired or hung batch
  reports verification incomplete without partial permission samples. The
  focused proof runs through `test:doctor:ownership-latency`.
- The embedded catalog baseline now tracks published catalog-pack `v1.1.1`
  (`ghcr.io/inflatable-cookie/effigy-catalog-pack@sha256:a83b723e14a131d9bba0eaddeb931c8b881157948f5a8e47e68610e62ba91989`).
  The generated `workspace-rust-bun` and `php-fpm` entrypoints start safely
  as the non-root workspace user when the root-owned bridge log
  (`/var/log/effigy-ssh-bridge.log`) is unwritable: diagnostics, including
  unavailable-forwarding warnings, fall back to stderr, and non-root starts
  honestly refuse what needs root (the mkcert system-trust update and
  php-fpm's root-owned secrets/msmtp initialization) instead of failing
  startup. Root-mode starts keep the full bridge and trust behavior.
- Container policy reads, discovery, plans and retirement lookup no longer
  register runtime ownership. Container activation records the checkout scope
  before starting the backend or creating owned resources; a failed start
  keeps that record for retry. Retiring a configured but never activated
  checkout reports no owned scope without probing its runtime profile.
- Plain `effigy init` no longer creates, refreshes, downloads, or installs a
  project-local Effigy skill snapshot. It preserves existing local skill files
  and reports installed guidance roots, canonical symlink aliases, missing
  installs, and distinct-root ambiguity. The stable `skill.codex_project`
  action remains an explicit opt-in snapshot and warns that maintained local
  paths may be replaced. Managed `AGENTS.md` points to the installed shared
  skill; fresh agent discovery and the executable on `PATH` must be verified
  separately.
- Vitest auto-detection requires package-owned evidence (`package.json`
  dependency/script, `vitest.config.*`, or `vite.config.*` with a `test`
  block, including `.cts`). A transitive `node_modules/.bin/vitest` is not
  enough. Commented-out or string-embedded `test` / `dir` syntax is not
  active. Quoted `test` / `dir` keys and regex literals that contain `//`
  stay active. When the selected package's config sets `test.dir`, the
  detected command includes that root. Explicit `[test.suites]` and
  `[test.runners]` stay authoritative.
- `effigy container cache list` (repo and `--global`) now reports reclaimable
  size totals in text and JSON, plus in-use size on the global report.
  Unknown per-volume sizes keep `size_complete` false instead of inflating
  the total. Prune still uses the same cache inventory; it does not widen
  which volumes can be removed.
- `effigy container retire` lists Created and stopped containers with
  running ones, discovers owned resources by `com.effigy.project` as well as
  scope and Compose project labels, and removes disposable scoped caches
  while retaining persistent data. This repository's `paseo.json` worktree
  teardown now calls `effigy container retire --yes` before unlink. Archive
  callers outside Effigy must invoke that command (or `--scope` after the
  checkout is gone) and must not treat `bootstrap teardown` or a zero exit
  on residue as success.
- Container startup now waits for profile-scoped Colima containerd and
  BuildKit probes, repairs a running but incomplete profile once, and waits
  for declared Compose gateway ports to appear in the matching runtime
  project before claiming routes. Bounded failures identify the runtime stage
  or exited service while preserving project, service, port and host-listener
  checks.
- `effigy init northstar` now directs planning and leads to Queue and omits
  repository plan and triage files. Effigy's own planning records moved there too.
- Task and draft text runs send the framed CLI header to stderr so stdout is
  only the task's own bytes. Command substitution no longer needs to strip the
  banner.
- Installed skill examples now invoke `northstar/retired-concepts`. The
  temporary `northstar-lean` alias is no longer taught in help, guides,
  parse tests, or `AGENTS.md`.
- Local operations classify the minimum secret scope before any vault
  prompt. Secret-free installed skill tasks and container checks that
  do not start a runtime (`status`, `logs`, `stats`, `reset`, `down`)
  no longer unlock an unrelated required project vault. Secret-consuming
  tasks, Rhai scripts with an executable `secrets::` path, and
  `container up` still fail closed with a precise missing-secret
  diagnostic. Comments, string literals, and longer identifiers such
  as `mysecrets::` do not unlock the vault.
- Generated Nginx health checks keep the wget exit-code shell variable
  after Compose interpolation. A site whose root returns 404 can become
  healthy; transport failures stay unhealthy.
- Migrated repository planning and current technical knowledge to lean Northstar. Queue now owns task briefs, status, and outcomes; user guides retain their existing paths.

### Fixed
- QA-group heavy submission reuses the captured invocation cwd instead of
  rediscovering process cwd, so nested or already-dispatched group runs keep
  the original caller directory. Member execution discovers catalogs from the
  group's resolved root (`environment.cwd` plus a capture at that root unless
  a nested task source is present), so `tasks qa-group run` with `--repo`
  still resolves members such as `root/ok` when the process cwd is another
  checkout.
- Inline-workspace scheduler start and remove facts share the manager
  compose-plan backend id. A Colima policy reports `colima-nerdctl` for both.
- Managed TUI sessions now render startup feedback before sequential child
  startup delays finish. Waiting, starting, running, and failed tabs remain
  visible while child output and cancellation are handled; the configured
  startup order and delays are preserved, and startup failure or early exit
  restores terminal state and cleans up only recorded session children.
- Remaining Unix `pre_exec` `setpgid` error paths now convert `nix::Error` with
  the allocation-free `io::Error::from` mapping used by the earlier doctor,
  exec-transport, and container spawn callbacks. A failed group setup cannot
  allocate in the forked child. Spawn still fails closed. The old direct
  mapper (`Error::other(error.to_string())`) returned `ErrorKind::Other` with
  a formatted message and no `raw_os_error`. On rustc 1.97.1 Unix,
  `Command::spawn` then surfaced that to the parent as `EINVAL` /
  `ErrorKind::InvalidInput` through std's errno-pipe fallback
  (`raw_os_error().unwrap_or(EINVAL)` in `sys/process/unix/unix.rs`). The
  mapper now preserves the original errno, so the parent sees that errno
  rather than the `EINVAL` fallback. Prove with `test:rust:postfork-safety`.
- Gateway lifecycle now distinguishes a confirmed-absent PID from an
  unavailable or ambiguous process probe. A `ps` launch failure, a failed `ps`
  with a diagnostic, or malformed/mismatched/multiple rows is `Unknown`; status,
  up, down and daemon start refuse to report a false stop, keep the PID and
  version records byte-identical for reconciliation, and never start a
  replacement. Status leaves confirmed-absent authenticated records in place;
  locked start or stop compare-and-remove clears them. Stop stays
  idempotent. The probe result is now explicit (`GatewayProcessProbe`), so it is
  no longer collapsed to a `bool`. The task 104 identity sidecar and fail-closed
  legacy policy described above now prevent a PID-only record from authorizing
  lifecycle actions. The final identity-check-to-signal TOCTOU remains. Tom's
  Q-001 publication block remains pending exact-candidate release assurance;
  the earlier PID-only risk was not accepted
  ([Q-001](docs/knowledge/questions.md#q-001--gateway-pid-identity), answered).
  Prove the probe behavior with `test:gateway:probe-state`.
- Gateway PID-file reads and direct probe/signal paths reject PID 0, PID 1, and
  values outside the positive signed PID domain before dispatch. Unix status
  requires one exact PID row from `ps`. The numeric-domain repair does not by
  itself establish start identity or remove the final signal TOCTOU; task 104's
  sidecar policy is described above. Prove with `test:gateway:pid-domain`.
- In-process run-array shell timeouts and managed lifecycle cleanup now stop
  the owned Unix process group on expiry and reap the direct child, so
  descendants cannot leak after a sequence `124` or a cleanup timeout,
  including descendants that ignore SIGTERM (group SIGKILL after grace).
  Isolated groups acquire the existing owned-children signal scope so Ctrl+C
  still reaches them when no heavy-run scope is active; a recorded
  cancellation then reaps the group (SIGTERM, then SIGKILL) because a
  background descendant may ignore SIGINT. The caller group is never
  targeted. Non-Unix hosts keep pid-only kill. Prove with
  `test:runner:timeout-descendants`.
- Owned in-process waits now terminate and reap the already-spawned child tree
  when signal-supervision initialization fails. The wait guard takes ownership
  of the child before the fallible signal-scope install, so a `Signals::new` or
  forwarder-thread error reports the original failure (never a command exit or
  success) and cleans up exactly that child's process group. The caller group
  and unrelated siblings are never targeted, and non-Unix hosts keep pid-only
  kill. Prove with `test:runner:supervision-init`.
- `effigy container <NAME> retire` now fails before scope resolution or
  runtime inventory, even with `--scope` or `--yes`. Unnamed retirement keeps
  checkout-wide selection, and `--scope <TOKEN>` keeps exact-record selection.
- `fs::copy_if_missing(source, destination)` now publishes through the same
  atomic create-if-absent path as `fs::write_file_if_absent`: the source is
  streamed into a staged file on the destination filesystem and linked into
  place, so a concurrent winner is never overwritten and the helper returns
  `true` only when its complete independent copy is published. Occupied
  destinations, including dangling symlinks, stay in place even when the
  source is missing or unreadable; a later write to the source cannot change
  the copy; unsupported filesystems fail instead of falling back to an
  overwriting copy. `fs::copy` and `fs::move_path` keep their replacement
  behavior.
- Container exec discovery and capture now keep the caller's absolute
  deadline. Colima service-name resolution shares the remaining budget
  instead of probing unbounded on a cache miss, and a spawned capture is
  supervised against the original deadline rather than a fresh timeout
  measured after spawn. An expired deadline never spawns, a slow spawn
  cannot extend the budget, and a timed-out child is terminated and reaped
  by its own process group. Cache hits and unbounded callers are unchanged.
- Recursive workspace ownership scans now prune every declared child mount,
  including same-device nested binds and volumes. Nested host binds are checked
  as the numeric workspace user without owner-equality requirements or probe
  writes, while nested owned volumes retain separate repair scopes.
- Implicit cache, lock, and local-overlay ignore registration now writes Git's
  local `$GIT_COMMON_DIR/info/exclude` and leaves the working tree, including
  tracked or missing `.gitignore`, unchanged. Linked worktrees resolve the
  shared admin dir from the `.git` file; invalid or dangling markers, a
  gitfile, `.git` symlink, linked common dir, or real `.git` directory
  that is a bare repository (Git last-wins for duplicate `core.bare`),
  read-only metadata, and symlink escapes fail without a working-tree
  fallback. Failures whose `.git` is a file or symlink report that marker
  rather than a fabricated `.git/info/exclude` path. `effigy init` still
  generates its reviewed `.gitignore` block.
- Provisioning Effigy-owned Rust target/Cargo volumes now repairs numeric-user
  ownership in one bounded exec per volume instead of two container execs per
  unowned file. Private 44339-entry fixture across three volumes: 88991 → 28
  runtime calls (same per-exec latency model). Protected bind, shared,
  persistent, foreign and read-only scopes are unchanged; timeouts and
  permission failures report not-ready. The bulk child uses `-execdir`
  (GNU find) so a mid-run symlink swap cannot redirect ownership outside the
  volume, and is capped at 600 s. A private GNU-find container fixture (44339 entries,
  three volumes, numeric 501:20 and 1000:1000) repaired in 3 execs and 193
  batched chown calls, about 2.2 s real. Not yet accepted against an installed
  consumer stack.
- QA-group local `--json` list, plan, run, and status output now suppresses the
  CLI banner while keeping each QA-group schema as one complete stdout JSON
  document. Global `effigy --json` continues to emit the command envelope.
- `effigy doctor` preliminary container liveness probes (Colima profile
  `status`, the primary-service Colima/`compose ps` check, and the SSH-agent
  socket preflight) now share one monotonic remaining-doctor-budget deadline
  instead of each granting a fresh per-subprocess timeout. An expired deadline
  never spawns a probe, a hung probe is killed and reaped at the deadline
  (only its own recorded process group), and timeout/unavailable never report
  a running, stopped, or clean result. The bounded doctor path stays read-only:
  it does not trigger a Colima runtime repair. Prove with
  `test:doctor:workspace-liveness`.
- After a Colima/nerdctl restart, a stopped owned stack container can recover
  its own stale transient health-check timer/service collision. Effigy
  inspects the full hexadecimal container ID, requires stopped status and the
  selected project/service labels, confirms `{id}.timer` and `{id}.service`
  in the selected profile are transient, then `systemctl stop`s only
  `{id}.timer` and `reset-failed` `{id}.service`/`{id}.timer` before one
  bounded start retry and inspect readiness. A stop/reset that exits 0 while
  the transient pair stays loaded is not recovery: Effigy proves the exact
  pair actually unloaded with bounded re-probes, so a late unload recovers
  once and a never-unloaded pair fails with its still-loaded load/fragment
  state instead of a false success. A start that reports the collision while
  inspect shows the container running is recorded as an unresolved health
  check with the exact inspect/start commands, never as a clean recovery.
  Already-gone units are idempotent; persistent, foreign, one-off, running,
  undeclared, mismatched,
  or unverified units are refused with inspected identity, the reason, and
  profile-scoped operator commands. Unit files are never deleted. Docker
  start is unchanged. Prove with `test:containers:healthcheck-timer` and
  `test:containers:healthcheck-unload`.
- Workspace permission prep no longer skips nested rust build/cache repair
  when the mount root already matches `workspace_user`, and no longer
  swallows chown failures with `|| true`. Headless workspace, primary-service
  exec, and routed tasks resolve numeric uid/gid, classify named cargo/target
  volumes as owned-disposable versus bind rust paths as verify-only, then
  verify actual read/write/create-lock as that non-root user before claiming
  ready. Named volumes used by two or more compose or managed services are
  not mutated (rust caches verify-only; other shared named volumes
  forbidden). `effigy doctor` `container.workspace-ownership` stays
  read-only, covers Cargo paths and declared target mounts, probes nested
  rust lock/cache paths with per-mount inspect subprocesses and early-exit
  after the first sample, kills those inspect children at the remaining
  doctor deadline, and distinguishes not-probed/stopped/unavailable from
  clean. Bind host sources, shared volumes, and symlinks are not
  mutated. Anonymous colon-less rust target mounts stay owned-disposable.
- Generated `AGENTS.md` Effigy Agent Contract now links its reference docs as
  upstream Effigy repository URLs instead of nonexistent consumer-relative
  paths; rerunning init rewrites the managed block only.
- `container retire` probes each recorded runtime profile first. A stopped
  profile no longer aborts teardown with a raw query error: host-side and live
  resources are still retired, the stopped profile is reported as unverified
  (`unverified_profiles` in JSON), the scope record is kept, and the command
  exits non-zero until the profile is started and retire is rerun.
- Running-Compose inventory decides runtime participation before any `ps`
  probe, without launching Docker, Colima, or a container. Docker is inactive
  only when every endpoint it could be using (`DOCKER_CONTEXT`, `DOCKER_HOST`,
  the stored current context, and the platform default socket) is an absent
  local Unix socket, so a Colima-only host whose Docker Desktop is stopped no
  longer invokes the Docker CLI or reports its absent `/var/run/docker.sock`
  as a failed authoritative inventory. A remote context selected by
  `DOCKER_CONTEXT` is never omitted by an absent local `DOCKER_HOST`, the
  `DOCKER_CONFIG` path and context names are read exactly as given, and any
  remote, reachable, stale, unreadable, schema-invalid, or unresolvable
  endpoint stays fail-closed. Colima participates when its CLI exists or when
  Colima/Lima instance state remains or cannot be inspected, because a running
  VM can outlive its client CLI; only a host with neither is Colima-inactive.
  Those failures still preserve uncertain owners, and identical
  skipped-reclamation warnings print once per launch while distinct failures
  still print.
- Built-in `test` honors explicit Cargo package scope. Passthrough that
  selects packages (`-p`, `--package`, including `--package=<package>`,
  attached `-p<package>`, and repeated selections) no longer widens to the
  workspace in auto-detected Cargo test and nextest plans or executions: the
  detection-added `--workspace` flag yields, so `effigy test -p app` runs only
  `app` and `--plan` shows the scoped command. Passthrough that selects the
  workspace itself (`--workspace`, `--all`) keeps the full-workspace run
  without repeating the flag (previously a runner usage error). Combining an
  explicit workspace selection with an explicit package selection is rejected
  as ambiguous instead of silently running every workspace member.
- Reapplying an active Cargo link now recognizes its patched path resolutions
  only after the desired-state ledger and the library's managed config block
  agree on each package, workspace, source, and local path. This refreshes the
  existing link without unlink/relink churn while unrelated path dependencies
  and damaged ownership state still refuse.
- Nested catalog selections keep their declared container target. When a
  nested member declares its own `[systems]` runtime target,
  `effigy <member>/<task>` executes on that member's container — its compose
  project, activation, and exec — even when invoked from the repository root;
  root workspace ownership no longer replaces the selected catalog's declared
  target. Container policy loading, runtime activation, and container exec
  use the owning catalog root. A selected catalog without its own `[systems]`
  table still inherits the invocation-scope (ancestor) runtime, and
  `run_in = "container"` with no resolvable target fails before execution
  instead of silently running on the host.
- Builtin `effigy test` routes each target catalog's suites through that
  catalog's declared container runtime, resolved with the same execution
  binding grammar tasks use. Suites of a catalog with a named-container
  target (and their setup/teardown steps) execute inside that container;
  host-scoped catalogs keep running suites on the host; suites carrying
  nested effigy re-entries keep their own routing; and a declared but
  unusable target fails before execution instead of silently running on the
  host. `effigy <selector> --plan` exposes the resolved runtime target
  (additive `runtime` section in `effigy.task.plan.v1`), and
  `effigy test --plan` reports `runtime` per target in
  `effigy.test.plan.v1`.
- Failed task runs under `effigy --json` keep their captured output:
  `effigy --json <task>` and `effigy --json draft <selector>` already attached
  the `effigy.task.run.v1` payload — including Rhai script diagnostics written
  before the failure — to `error.details`; nested built-in surfaces such as
  `effigy --json watch --once <target>` now preserve the same payload instead
  of reporting `error.details: null`. Envelope shape is unchanged; failures
  still exit non-zero and text mode still streams script output untouched.
- The documentation graph indexes tracked Markdown files whose
  repository-relative paths contain spaces. Graph record ids percent-encode
  whitespace, control characters, and `%`, so `docs/my file.md` and a literal
  `docs/my%20file.md` get distinct identities while every previously valid
  path keeps its existing id. `docs context` output still shows original
  paths and exact source text, and declared typed-relation destinations are
  recovered byte-exact.
- After a Colima VM restart, `effigy container up` starts owned Compose
  services that stayed `Exited` or `Created` instead of treating compose
  success as readiness. Start keeps volumes, including Postgres
  crash-recovery data, and does not delete systemd units. A nerdctl stale
  health-check timer warning is recorded when the owned container is
  running; a still-stopped service fails with its status, the start
  backend text, and
  `colima nerdctl --profile <profile> -- start <container>`.
  Inspect and start are bounded so a hung nerdctl command cannot hide that
  diagnosis. Recovery starts only currently declared Compose services;
  one-off `compose run` containers and undeclared orphans stay stopped.
  `effigy container status` lists those stopped owned rows.
- Recursive `effigy docs check` walks skip `target/` and `node_modules/` at
  directory entry, so a parallel Cargo or package-manager build cannot fail
  the check when a generated path vanishes. Default scope is still
  `README.md` plus Markdown under `docs/`. An explicit file path is still
  checked; a missing or unreadable owned Markdown file still fails.
- `effigy doctor` accepts test-suite `run` commands, managed run-step arrays,
  and single run-step tables using the same command and task reference forms as
  `effigy test`; empty or malformed suite runs include their manifest paths.
- `effigy release status`, `effigy release simulate`, and `effigy changelog
  analyze` treat a `Removed` changelog entry as a breaking change instead of a
  compatible fix. Under `pre-1-0 = true` a `0.x` removal now proposes MINOR,
  matching the documented pre-1.0 compatibility policy; PATCH stays reserved
  for `Fixed` and `Security` entries.
- A release heading that separates the version and date with a Unicode em dash
  (or any non-ASCII dash) now reports that the separator must be ASCII ` - `.
  `effigy changelog validate` and `effigy release status` reject it with the
  same actionable message instead of an unexplained "invalid release header".
- `effigy release status --check-gates` reports the configured gate verdict
  independently of the optional next-version proposal. An empty
  `[Unreleased]` section keeps `ready: false`, `next_version: null`, and the
  explicit unreleased-entries blocker, but no longer turns passed gates into
  a failed gate check. The status payload adds `gates_passed` and
  `gate_check_passed`; `simulate`, `prepare`, and `execute` still require a
  derivable next version.

### Security
- Managed host task secrets now reach child processes through their
  environment, stay out of command arguments, and are redacted from managed
  stream and headless logs.

## [0.13.1] - 2026-09-25

### Added
- `workspace-rust-bun` accepts `browser_runtime = "chromium"` to install
  Debian Chromium shared libraries and a basic font set at image build.
  Default remains `"none"`. The image still does not ship a browser,
  Playwright, or Node; consumers download their own revision after rebuild.
  Unknown values fail the build.

### Changed
- The generated `workspace-rust-bun` catalog snapshot and
  `catalog-pack.lock.toml` now track published catalog-pack `v1.1.0`
  (`ghcr.io/inflatable-cookie/effigy-catalog-pack@sha256:5699fcb8641424cc6365feb2a4c4cc7f6056de385fc9dc49f771aec63f6078ba`).

## [0.13.0] - 2026-09-24

### Breaking
- Executable `local`, `repo`, `deliver`, `extend`, and `admin` namespace aliases
  and their migration warnings are removed. Use built-ins directly as
  `effigy <command> ...`; the former namespace words now follow ordinary
  repository selector routing. Help groups remain for discovery.

### Added
- Published and draft task surfaces. `[tasks]` stays the small published
  command surface, and a new lifecycle-labelled `[drafts]` table carries
  provisional proofs and environments without polluting normal discovery.
  Drafts require the full table form with a strict `created` date, a non-empty
  `purpose`, and an optional strict `expires` date; compact shorthand is
  rejected. `effigy drafts [FILTER] [--json]` inventories selector, catalog,
  purpose, lifecycle, expiry, and composed source, and `effigy draft
  <SELECTOR> [--json] [-- <ARGS>]` runs one explicitly through the ordinary
  catalog routing, request, and execution pipeline. Drafts never appear in
  `effigy tasks`, help task inventories, completion task candidates, routing,
  or default task-status queries. Published tasks cannot reference drafts;
  drafts may reference published tasks, and draft-to-draft composition is
  explicit through `{ draft = "..." }` steps. Expiry is advisory evidence in
  `effigy drafts` and `effigy doctor`; it never disables, mutates, or deletes a
  draft. New `effigy.drafts.v1` JSON and additive draft fields on the ordinary
  task-run payload leave repositories with no `[drafts]` unchanged.
- Code-graph catalog scopes. A composed catalog manifest may declare
  `[catalog.graph] segmented = true` to make that catalog an independently lazy
  graph scope (pruned from its parent's scan) and `independent = true` to give
  it a deterministic database and lock under
  `.effigy/graph/catalogs/<encoded-alias>/`. All `effigy graph` subcommands
  accept common `--catalog <ALIAS>` and `--all-catalogs` selectors; without
  `--catalog` the invocation cwd's deepest segmented catalog, or the root
  scope, is selected. Graph JSON payloads carry additive `catalog.alias`,
  `catalog.root`, `catalog.selection`, `catalog.segmented`, and
  `catalog.independent` evidence, and `--all-catalogs` renders
  `effigy.graph.fanout.v1` with one complete outcome per catalog. A repository
  with no `[catalog.graph]` keeps its existing database, paths, schemas, and
  values. `[catalog.graph] independent = true` without `segmented = true` and
  segmented catalog roots that escape the workspace are rejected before any
  index access.
- `effigy skill run` can resolve an installed agent skill by name. When
  `--path` is omitted, the qualified `<skill>/<task>` selector's first segment
  selects the skill, resolved from the invocation project's
  `.agents/skills/<skill>` first and then from the unique installed global root
  under `~/.agents/skills`, `~/.codex/skills`, `~/.claude/skills`, or
  `~/.cursor/skills`. Project-local wins, symlink aliases of one directory count
  once, an incomplete project-local copy and distinct global collisions fail
  closed before any task side effect, and `--repo` never changes discovery.
  An explicit `--path` stays authoritative and unchanged.
- `effigy skill run --stdio passthrough` runs the selected host task with raw
  stdin/stdout/stderr and its own exit status, adding no Effigy header, footer,
  spinner, envelope, or newline. Preflight and launch failures keep stdout
  empty and write one diagnostic to stderr. `--json` is rejected with
  passthrough in either global or local flag position before execution.
- `effigy docs context --sources <PATH>` can query explicitly named,
  repository-opted-in sources sequentially, returning grouped
  `effigy.docs.context.sources.v1` evidence with per-repository status,
  provenance, freshness, and content identity. No consumer repository is
  modified.
- Release gate runs now persist full per-gate output and a redacted execution
  environment under `.effigy/reports/release/gates/`, with additive JSON paths
  and failure diagnostics that include a bounded output tail.
- Consumer-contract guidance now states that `effigy doctor --deep` may execute
  eligible repo-owned health tasks and is not guaranteed read-only, and
  documents `effigy doctor <task>` (doctor explain) as the non-executing
  routing probe that reports root, catalog, and selection mode without
  running anything. Proven by the Acowtancy consumer replay (card `1111`).
- `effigy service pack update` resolves the compiled official `stable` channel
  on `ghcr.io/inflatable-cookie/effigy-catalog-pack` to an exact
  `sha256:`-plus-64-lowercase-hex digest with no surrounding whitespace, then
  reuses the existing
  acquire-validate-store-activate transaction. A pulled descriptor digest that
  is absent, malformed, or different from the requested pin is refused before
  activation. Text and JSON report the channel and digest. A verified
  already-active digest is a no-op whose decision and report snapshot are taken
  under the durable store lock. Resolution, pull, compatibility, validation,
  and activation failures leave active, previous, and channel identity
  unchanged. Ordinary catalog use stays network-silent. The support-policy
  `oldest_update_capable_release` records 0.13.0 as the first release with
  public update support.
- Effigy now owns `support/catalog-pack-update.toml`, the machine-readable
  catalog-pack update support floor. A typed `effigy-catalog` validator checks
  schema, semantic versions, current-release membership, and the
  `oldest_update_capable_release` rule locally. Pack selection, acquisition,
  and activation do not read the file. Pack repositories may consume it only
  by resolved commit and blob digest.
- `storage::put` accepts an optional boolean `create_only`. When true, Effigy
  attaches S3 `If-None-Match: *` to the same PutObject request that carries the
  bytes, so exactly one of two writers racing for an absent key succeeds and an
  occupied key is never overwritten. The loser receives one stable diagnostic
  that names the create-only collision and never carries signed URLs,
  credentials, or response bodies. Omitting the option or passing `false`
  preserves the existing unconditional write byte-for-byte.
- `effigy service pack` manages independently versioned catalog packs:
  `status`, `install oci://<REPO>@sha256:<DIGEST>`, `install --path <DIR>`,
  `update`, `rollback`, and `reset`, each with text output, a standard JSON payload, and
  standard leading `--repo` / `--json`. Catalog fragments now resolve through
  four layers — project override, user override, active installed pack, then
  the compiled baseline. The compiled baseline is permanent: a machine with no
  pack store, no `oras`, and no network resolves exactly the fragments it
  resolved before, and no ordinary catalog-backed command performs a network
  probe. Installation is explicit and transactional — acquire, validate, store,
  then activate — so a malformed, incompatible, or failed candidate leaves the
  previous selection and previously installed content untouched. An active pack
  that later becomes unreadable or incompatible falls back to the compiled
  baseline and says so on stderr — a `[warn]` line normally, one
  `effigy.catalog-pack.fallback.v1` object under `--json` — for every
  catalog-backed command including container, system, workspace, and task
  paths, leaving existing stdout contracts unchanged. `effigy doctor` adds a
  `catalog.pack-health` finding naming one repair command. Selection re-proves
  the active pack on every use: manifest and fragments revalidate, the stored
  manifest is cross-checked against the install record, and the whole tree is
  re-hashed against its recorded content identity, so edited or deleted bytes
  cannot stay selected. `rollback` re-proves its target with that same check
  before mutating state and refuses rather than activating another unhealthy
  pack, and `doctor` recommends `rollback` only when that proof passes,
  otherwise `reset`. `reset` is also the recovery path for damaged store
  metadata: an unreadable or unsupported `state.json` is preserved under a
  `state.json.unreadable-*` name rather than deleted, a valid
  baseline-selected document is written, selection pointers naming no retained
  record are dropped, and every install directory is kept. Pack content may contain only regular files and
  directories with valid UTF-8 names; symlinks — including the pack root and
  `pack.toml` — are rejected before anything is read, hashed, or copied. Durable store mutation is serialized across processes by an
  advisory lock. Every successfully installed pack is retained — `install`,
  `rollback`, and `reset` never delete installed content — and reinstalling
  content the store already holds re-verifies it, repairing rather than
  reactivating a corrupt tree. `rollback` selects the previous validated
  install and `reset` selects the baseline without touching project/user
  overrides. The official channel is compiled in, so installed content cannot
  redirect it.
- `effigy --help` and `effigy help` now group the built-in command surface by
  operator job under the topics `work`, `local`, `repo`, `deliver`, `extend`,
  and `admin`. `effigy help <group>` renders one group's inventory, and
  `effigy help <command>` renders the same panel as `effigy <command> --help`.
  Grouping is discovery only: no `effigy <group> <command>` route exists, no new
  top-level built-in name is reserved, and a manifest task named after a group
  keeps its current routing.
- The `northstar` starter now emits the full Northstar documentation ontology as
  a `[docs_policy.graph]` block: 14 document kinds with authority weights,
  `Status:`-driven currentness, and typed `contract`, `architecture`, `spec`,
  `roadmap`, `card`, `evidence`, `supersedes`, and `next-task` relations. The
  emitted block states in its own text that it is copied configuration, not an
  inherited runtime dependency: `effigy init northstar` materializes it into the
  consumer `effigy.toml`, and from then on those committed bytes are the only
  runtime authority. Effigy reads no starter, installed skill directory, or
  template cache while answering a query, so upgrading Effigy or a skill cannot
  reinterpret an existing repository; adopting a newer template is an explicit
  merge.
- `effigy perf:docs-context-benchmark` replays a predeclared documentation
  retrieval corpus over an arbitrary-vocabulary fixture and the current
  repository, and fails when a declared live authority falls outside the top
  three, a declared historical rival outranks it, a directly named historical
  source is not retrieved, an unrelated high-authority document enters a report,
  or a fixture no-match query returns anything. Empty-result cases run only
  against fixture corpora; a live-target empty case is rejected before the
  matrix executes. Reports land under `.effigy/perf/docs-context-benchmark/`.
- `docs/guides/079-documentation-graph-profiles-and-context.md` documents the
  repository-owned profile, the copied-not-inherited adoption boundary, the five
  query shapes worth memorizing, and how ranking actually behaves.
- `effigy docs context <QUERY> [--max-sections N] [--max-bytes N] [--max-hops N]`
  retrieves bounded documentation evidence from the shared graph. Results are
  exact deduplicated Markdown sections with path, heading, span, source text,
  extracted fields, typed relation path, and machine-readable match reasons. A
  traversed result names its lexical origin in `seed_path` and prefixes every
  inherited reason with that path, so it never claims its own section contains a
  seed-only term.
  Relevance gates inclusion, so currentness and authority only order results
  that already match, and they outrank heading specificity across documents; a
  query naming historical material still retrieves it. Corpus term weighting
  only reorders and never suppresses the query's only lexical evidence.
  Budgets default to 8 sections, 24000 bytes, and 1 hop, with maxima of 32,
  100000, and 3; a section that does not fit is omitted whole and reported
  rather than truncated mid-section, and `truncation.truncated` covers section,
  byte, and hop exhaustion alike. An empty query is a usage error and a
  no-match query is a successful empty report. Text and the versioned
  `effigy.docs.context.v1` JSON payload carry the same evidence, and the command
  works in baseline mode without `[docs_policy.graph]`.
- `effigy skill tasks --path <SKILL>` and `effigy skill run --path <SKILL>
  <SELECTOR>` execute one explicit installed-skill catalog against an
  independently resolved consumer repository. Versioned JSON exposes source,
  target, invocation, and execution evidence; v1 rejects members, escaping
  composition or Rhai assets, manifest runtime/secrets inheritance,
  container-bound task graphs, and managed/TUI/concurrent shapes before side
  effects.
- Optional `[docs_policy.graph]` lets a repository name documentation roots,
  field labels, currentness sets, kinds, authority weights, and typed link
  relations. Invalid roots, currentness references, overlaps, and unknown keys
  fail before indexing. A normalized profile fingerprint participates in graph
  freshness, so a profile-only edit reindexes Markdown semantics.
- Markdown heading records now span the exact hierarchical section (heading
  through the next same-or-higher heading, or EOF) instead of the whole file.
  Configured `Label: value` facts and heading/label relations are stored on the
  shared graph with exact provenance. Repositories without a profile keep
  baseline document/section/link extraction with kind `document`.
- `effigy deps status` reports committed local dependencies as observed links.
  A Cargo `path` dependency or a Bun `file:`/`link:` specifier resolving into a
  sibling checkout is grouped by library checkout and reported with
  `committed_local` (`cargo-path-dependency` / `bun-file-dependency`) and no
  `desired` entry, so a consumer that already declares a sibling library can
  see the local dependency in force instead of only the correct `deps link`
  refusal. The reports are read-only observations; `deps link` and
  `deps unlink` behavior is unchanged.
- Generated `workspace-rust-bun` images install Clippy (`rustup component add
  clippy`) so consumer validate tasks do not need an extra rustup step after
  `container up`.
- `effigy --json deps status` detects a root Bun workspace (`package.json` plus
  `bun.lock` / `bun.lockb`, Bun `packageManager`, or workspaces) and reports
  `manager: "bun"` with `detected_managers` even when no local-link record
  exists.

### Changed
- `effigy doctor` is now a bounded structural-only check by default (10
  seconds). `--deep` explicitly adds enabled content scans and selected-scope
  `health` under a 120-second overall budget, with `--catalog`,
  `--all-catalogs`, `--refresh`, and `EFFIGY_DOCTOR_TIMEOUT_MS` controls. Deep
  scans share one scope walk and an exact atomic cache under
  `.effigy/doctor/cache/v1/`; doctor text and JSON now expose run scope,
  budget, completeness, per-check state and timing, and cache counts.
- `effigy docs context` keeps a repository-owned documentation corpus: catalog
  segmentation no longer removes an explicitly configured documentation root,
  and refreshing documentation context does not refresh a source catalog. Its
  provenance stamp is now stored per scope, so it does not reuse or overwrite a
  code scope's git stamp.
- The bundled Northstar starter now uses the flattened generation-plus-task
  model: generation READMEs own the approved frontier, executable planning
  units are top-level `gNN.NNN` tasks, and the documentation graph exposes
  `task` and `archived-roadmap` kinds instead of nested ready cards.
- `effigy docs context` answers a warm query on this repository roughly three
  times faster (p50 1935 ms to 602 ms on a current index; 2045 ms to 682 ms
  with a dirty working tree), and a stale docs-only refresh of 50 Markdown
  files drops from 12.8 s to 10.2 s under the default budget. The cause was
  glob recompilation: every documentation-profile compile revalidated kind
  overlap across every in-scope document, and every scope build asked each
  document for its kind, rebuilding the same `docs_policy.graph` matchers each
  time. Each distinct pattern now compiles once per process. Ranking,
  provenance, budgets, freshness identity, and every default are unchanged; the
  frozen `perf:docs-context-benchmark` matrix stays green and its results are
  byte-identical.
- The shared `effigy.graph.timeout.v1` detail now carries an additive `phase`
  block naming what the bounded run was doing when the budget expired
  (`freshness-scan`, `refresh-lock-wait`, `index-walk`, `index-files`,
  `search-index-rebuild`, `docs-scope`, `docs-rank`, `docs-select`), with
  `items_done`/`items_total` for the file-proportional phases, plus a matching
  recovery line. The schema id, schema version, and every existing field are
  unchanged.
- `release gates` announces its configured inventory and always emits progress
  on stderr, keeping JSON stdout contract-pure; first-time `.effigy` writers
  use the existing gitignore convention.
- Retired the `docs/roadmaps/backlog/` intake layer. Roadmaps now hold only
  promoted executable tasks while `docs/triage/` temporarily holds unresolved
  or deferred candidates with no execution authority; the two surviving
  candidates (consumer cohort expansion, vendored skill portfolio sync) moved
  to timestamped triage notes. Guide `049` is self-contained for versioning,
  rollback, support-window, and v1-planning policy, live references point at
  current guides and triage notes, and both repo-owned `qa:docs` and the
  bundled Northstar starter reject a reintroduced `docs/roadmaps/backlog/`
  through a `test ! -e` absence check. The starter also emits a
  `docs/triage/README.md` intake anchor.
- `effigy help <topic>` no longer silently falls back to general help. An
  unknown topic now fails with exit code `2` and names the valid groups and
  commands. When a manifest selector or `[defer] builtins` entry owns a built-in
  name, `effigy help <command>` defers with `effigy <command> --help` instead of
  showing the built-in panel, and never executes the repository's task.
- Markdown graph documents no longer infer kinds from path prefixes such as
  `docs/knowledge/contracts/` or `docs/guides/`. Baseline kind is `document`; a
  repository profile assigns kinds.
- This repository now ships its own `[docs_policy.graph]` profile in
  `docs/effigy.docs.toml`, so `effigy docs context` runs here with Northstar
  semantics instead of baseline mode. It is the starter block adapted to
  Effigy's real docs tree, and it deliberately differs from the template:
  `status` and `owner` are multi-valued because Effigy's roadmap and
  architecture documents carry per-section metadata lines.
- The `effigy` agent skill now routes documentation-authority questions to
  `effigy docs context` and no longer claims the graph profile ships without a
  docs-context command.
- Effigy's embedded catalog is now a generated snapshot of the official
  `inflatable-cookie/effigy-catalog-pack` `v1.0.1` artifact. The checked-in
  `crates/effigy-catalog/catalog/` tree is a byte-for-byte copy of the pack
  repository's canonical `pack/` root (now including `pack.toml`) at source
  commit `5ef0ec2b64612c7803cc6105a65ea462862a0b21`, and fragment behavior is
  unchanged. A typed `catalog-pack.lock.toml` records the source
  repository/commit, pack version, OCI manifest digest, and unpacked content
  identity; offline repository tests recompute manifest facts, content
  identity, and the deterministic OCI manifest digest from the snapshot and
  reject byte, manifest, version, content-identity, or lock drift, so a hand
  edit now fails `cargo test`. No public command is added and ordinary QA and
  use stay offline.

### Fixed
- `effigy graph` selects the invocation catalog from the captured runtime
  context, including embedded calls, so a process cwd outside the selected
  catalog cannot redirect graph indexing or queries.
- `effigy doctor` now bounds its subprocess work with the selected overall
  deadline. A stalled Cargo metadata or Git identity child spawned during
  dependency health or the deep scan inventory is terminated with its owned
  process tree and reaped, and doctor reports a non-zero incomplete result
  naming the `dependency_health` or `scan_inventory` phase with the completed
  evidence instead of hanging past its budget. The `0`
  `EFFIGY_DOCTOR_TIMEOUT_MS` override stays deliberately unbounded.
- `effigy skill run --stdio passthrough` now carries raw stdio and the leaf
  exit status through nested task steps, without adding a trailing newline
  to nested output.
- Cross-repository docs sources now require sharing consent from the neighbor's
  committed root manifest, decode Git dirty paths and rename endpoints exactly,
  reject duplicate checkout handles before querying, and distinguish absent
  portfolio directories from directory read failures.
- `effigy changelog validate` now rejects duplicate category headings under
  `[Unreleased]` while retaining historical released sections, and `changelog
  analyze` counts every entry even before duplicates are consolidated.
  Previously a repeated heading could silently hide earlier entries from
  release status and bump analysis.
- Committed Cargo resolution now uses patched `rustls 0.23.45` and non-yanked
  `chacha20 0.10.2`, clearing RUSTSEC-2026-0285 and the yanked-release warning
  with no `deny.toml` exception and no manifest, requirement, or feature change.
  Cargo also moved the resolver-required `aws-lc-rs 1.18.1`, `aws-lc-sys
  0.45.0`, and `rustls-webpki 0.103.15` transitives; full `cargo deny check`
  passes.
- The attached container startup SIGINT CLI test now waits for a fixture-owned
  marker written after the delayed startup child is active, with separate
  startup-delay and marker-wait budgets, instead of racing equal three-second
  deadlines.
- A provably stale repository-local install — the checkout's own
  `.local-install/bin/effigy` whose recorded `+local.<sha>` identity resolves
  in that checkout as a strict ancestor of its `HEAD` — now fails a strict
  manifest parse with the original TOML error plus the installed and current
  revisions and the source-build refresh
  `cargo run --bin effigy -- bootstrap:local`. Current installs, unprovable or
  divergent recorded commits, release/global binaries, and consumer
  repositories keep the ordinary parse error unchanged. Proven by card `1117`.
- `effigy deps link cargo` now links across a package version transition.
  Pointing a consumer whose `Cargo.lock` pins a released Git tag at a local
  candidate carrying a different version previously left Cargo holding the
  locked packages, recorded the patch entries under `[[patch.unused]]`, and
  failed verification. Planning now detects the transition per package and
  reports it, and link refreshes exactly those packages before verifying; a
  same-version link refreshes nothing. A patch Cargo still leaves unapplied is
  a verification failure that names the package instead of a silent Git
  resolution. Every affected lockfile is also part of the transaction, so a
  failed apply, refresh, or verification restores each one byte-for-byte
  instead of leaving lock residue behind.
- `effigy docs context` now keeps identifier-shaped query tokens
  (`catalog_tasks`, `foo::bar`) whole alongside their split words and ranks
  exact whole-term containment above split-word density. `catalog_tasks`
  retrieves the guide `026` section that contains that literal; `graph` still
  does not match `graphql`. Candidate recall still uses the shared FTS index.
  Proven by card `1114`.
- Cargo lockfile release sync now authorizes version changes by actual Cargo
  workspace package identity and each member's metadata-reported package
  version, preserving independent versions in mixed-version workspaces. A
  third-party package move is rejected and the pre-prepare tree restored even
  when that package's new version matches a workspace member's target version.
- Child-catalog test-suite task references now keep the originating repository's
  loaded container registry while using the selected child task's cwd. An
  explicit child registry still wins, and direct child invocation keeps normal
  discovery semantics.
- `effigy docs context` now shares `EFFIGY_GRAPH_TIMEOUT_MS`, typed timeout
  detail, health snapshot, and recovery guidance with graph queries. Cold and
  stale rebuilds announce progress on stderr; JSON stdout stays contract-pure,
  `0` disables the bound, and usage errors are validated before the timer.
- Documentation-context selection now reserves one fitting traversed result
  when at least two section slots are available, while retaining the best
  lexical result first. One-slot, no-traversal, and oversized-result behavior
  stays unchanged.
- `perf:docs-context-benchmark` empty-result proof is fixture-owned. The live
  Effigy target keeps its authority and historical cases and no longer carries a
  query that fails when its terms appear in documentation. A live-target empty
  case is rejected before the matrix runs. Current matrix is 11 cases;
  historical freeze logs keep the older 12-case counts.
- Leading YAML frontmatter in Markdown documents is no longer indexed as one
  synthetic setext heading. A complete leading `---` … `---` block — including
  an empty body or a body that begins with blank lines — stays metadata: real
  ATX/setext headings keep their original spans, and profile-configured field
  facts and labelled relations inside the block remain available. Incomplete or
  non-leading `---` delimiters keep ordinary Markdown heading behavior. The
  Markdown extractor identity bumps to `0.2.1` so unchanged files reindex after
  upgrade instead of retaining a stale synthetic heading.
- `effigy service list` no longer advertises root bundled catalog assets such as
  `README.md` or `compose.override.example.yml` as callable fragments. Bundled
  inventory membership is the set of first-level directories that carry
  `service.toml`. Filesystem override and installed-pack listing remain
  directory-based and unchanged.
- Rhai script hosts now set explicit expression-depth limits (`64` global /
  `32` in functions) on every production engine, matching the previous release
  defaults instead of inheriting Rhai's lower `debug_assertions` parser caps.
  A checked-in script that parsed under an installed release binary no longer
  fails the documented `cargo run --bin effigy -- <task>` fallback solely
  because of build-profile drift.
- `effigy docs add-log-index` now inserts a missing log entry as the first item
  under the `## Active logs` section of `docs/logs/README.md` (newest first),
  ahead of any current entries and never below a later section such as
  `## Next Task`. A missing or duplicated `## Active logs` heading is an error
  that leaves the index file byte-identical. Path normalization, report
  schemas, and repeat-run idempotence are unchanged.
- `effigy skill run` no longer resolves the consumer secret vault. A consumer
  that declares required Rhai-target secrets could not run an unrelated
  external skill task non-interactively, because Rhai secret-store construction
  eagerly unlocked the vault before the isolated script started. The isolated
  task source now runs with an empty secret store: `secrets::get`, `has`, `set`
  and `set_many` are refused with an explicit boundary error, and consumer
  `tasks`-target secrets are no longer injected into an external skill task's
  environment. Ordinary consumer task and Rhai secret behavior is unchanged.
- Project-local skill sync now manages every file in the public Effigy skill,
  including the built-in surface reference, with a parity test guarding future
  additions.
- `effigy rhai surface` catalog signatures for `regex::is_match`,
  `regex::replace`, and `regex::captures` now match the live host argument
  order `(pattern, value[, replacement])`. The previous catalog order was
  reversed and made wrong-order calls look like silent no-ops.
- Generic config reference/schema output now uses a repository-neutral
  `[docs_policy.graph]` example, while the Northstar starter owns its profile
  and the minimal schema stays profile-free. Agent guidance now routes by job,
  and graph status documents `--refresh` as its explicit mutation boundary.
- `effigy-rhai` state/deploy host helpers read an optional thread-local env
  override map before the process environment, so in-process runtime-context
  tests no longer mutate `EFFIGY_STATE_CAPTURE_CONTEXT` (and kin) with
  `std::env::set_var`. Production hooks keep using the parent-supplied process
  env when the override map is empty.
- `deps link cargo <library>` anchors the library inventory on the library's
  root workspace and links only its members, so a package the root does not
  list — a `[workspace]`-less example under `examples/`, or a self-contained
  prototype workspace whose package name collides with a root member — no
  longer fails the whole link.
- `deps link bun <library>` resolves the consumer's Bun package root instead of
  requiring `package.json` at the Git root. Without a root manifest the
  shallowest manifests are candidate roots, and the one declaring a library
  package is selected; an ambiguous choice is refused with the candidates.
  Links are keyed by that package root while the ledger, `.gitignore`, and link
  backups stay with the enclosing checkout, so `deps status` still reports them
  from the repo root and `deps unlink` removes the entry it wrote.
  `deps status` detects Bun below the root the same way.
- Bun root discovery keeps every manifest with no package-root ancestor rather
  than only the shallowest, so independent roots at different depths
  (`harness/` and `apps/studio/`) are both candidates for selection and
  package-manager detection. Discovery stops at a vendored checkout's `.git`,
  and `deps link bun` refuses a consumer root inside one, so a parent-level
  invocation cannot run Bun or change `node_modules` in a checkout that owns
  its own link state.
- Machine-local link state resolves to one ledger per checkout for every read
  path, so `deps status`, `deps pin`, and `doctor` see a nested-root link from
  either entry point. `deps pin bun --repo <nested-root>` no longer reads an
  empty nested store and bypasses its own active-link guard, and a vendored
  checkout with its own `.git` is no longer claimed by its parent's status.
- Workspace permission prep keeps one recursive chown for the authoritative
  root `node_modules` / `vendor` tree, shallow-chowns redundant child package
  trees, skips targets nested under another recursive prep path, and reports
  prep progress counts instead of silently walking every dependency volume.
- Workspace container handoff reuses an existing linux Effigy artifact with a
  matching rehearsal receipt instead of rebuilding through Docker Hub while the
  consumer stack is already Up. Reuse preserves the artifact's install identity
  instead of stamping the current host freshness onto a stale binary. When a
  rebuild is still required and fails, the error names cached-image / offline
  reuse options.
- Agent skill task-inventory examples now query `.result.catalog_tasks[].task`
  instead of the stale `.result.payload.tasks[]` path.
- Documentation and CLI help now agree on live JSON envelope paths, graph
  refresh/timeout recovery, special help families, and the generated
  `[docs_policy.graph]` reference; the project-local and distributed Effigy
  skills remain synchronized.
- Parallel git-bundle materialization serializes clone/fetch/checkout and
  `rev-parse HEAD` behind a per-cache file lock, so overlapping
  `docs/qa:*`-style manifest loads no longer lose HEAD mid-checkout.
- `effigy <task> -- <args>` now forwards the tokens after `--` into `{args}`
  instead of dropping them, so focused `test:unit -- paths` stays focused.
- `effigy test <suite> <catalog>` treats a matching catalog alias as a target,
  not a Vitest filter copied across sibling suites.
- `deps link bun` replaces Bun registry package symlinks (targets under
  `node_modules/.bun`) instead of refusing them as conflicting.
- `[release.gates]` run in declaration order, so cheap-first is buyable
  without renaming gates.
- `container volume list` marks a declared volume in use when its service
  container is Up, even if inspect missed the mount.
- Test suite task-refs keep container routing — explicit `run_in`, inherited
  `[task_defaults]`, or a workspace binding — by dispatching through the nested
  task instead of inlining the command on the host.
- `effigy tasks` still lists managed profiles alphabetically after
  `[release.gates]` started preserving declaration order.
- `effigy scan attention-markers --warning-marker/--high-marker/--critical-marker`
  now changes the rendered pattern lists.
- Generated compose can now combine explicitly pinned host ports with
  automatically allocated ports without panicking. Container status reports a
  stopped or non-exec-ready primary service instead of failing the command.
- `--` now ends task runtime-flag parsing, so `effigy <task> -- --repo <path>`
  forwards `--repo` to the task instead of switching catalogs. Leading
  `effigy --repo <PATH> <task>` still selects the target repo. `--json` after
  `--` is a task argument, not the CLI JSON envelope.
- `effigy doctor` accepts built-in `docs` sequence references and compact
  `{ rhai = "..." }` task values that the runner already executes.
- Graph queries now run under a wall-clock budget and report an
  `effigy.graph.timeout.v1` error envelope with index and refresh-lock health
  instead of stalling. Override with `EFFIGY_GRAPH_TIMEOUT_MS` (`0` disables);
  `graph index` and `graph watch` stay unbounded.
- The graph index skips installed packages and build output at any depth
  (nested `node_modules`, `dist`, `.svelte-kit`, `.next`, `.nuxt`, `.output`,
  `.turbo`, `.parcel-cache`, `coverage`, `__pycache__`, `.venv`) and prunes
  them during the walk.
- Workspace containers started from a linked git worktree now mount the shared
  git directory, so in-container `git` works instead of failing on a host-only
  gitdir pointer.
- A missing non-catalog workspace extra mount now warns and is skipped instead
  of aborting `container status` and `doctor`. Catalog members still fail.
- Bun package discovery skips macOS Finder metadata (`__MACOSX/`,
  `.AppleDouble/`, `.DS_Store`, `._*`), and `deps status bun` reports Finder
  metadata found inside a `file:` dependency with a removal command that clears
  every reported class.
- Secret vault reads and mutations from a linked git worktree resolve to the
  primary checkout's vault, so worktrees share one machine-local vault instead
  of forking a partial local one. Vault creation still writes where it was
  asked to.

## [0.12.1] - 2026-08-25

### Added
- Release `sync-files` now accepts secondary `package.json` manifests and
  updates their root version during prepare with layout-preserving edits.

### Fixed
- `release verify-install` now rejects non-Effigy repository roots before
  network work and directs library or service releases to repo-owned consumer
  smoke validation. CLI help, release docs, and the bundled agent protocol now
  describe the command as Effigy's self-hosting binary verifier instead of a
  generic release closer.

## [0.12.0] - 2026-08-24

### Breaking
- Effigy now requires Rust 1.97.1. The Rhai and vendored S3 transport graph is
  unified on reqx 0.2.0 instead of carrying separate reqx major versions.

### Added
- `effigy doctor` now reports a named `container.workspace-ownership` finding
  when a running workspace has root-owned paths in managed workspace volumes
  or its Bun install cache.
- Managed `mode = "tui"` tasks now have a headless concurrent runtime through
  `effigy <task> --headless` or `EFFIGY_MANAGED_HEADLESS=1`, with persisted
  per-process state and logs under `.effigy/runtime/managed/` plus task-local
  `status`, `logs`, and `stop` companions. Headless failures report the failing
  process log and tail instead of reducing lifecycle failures to a bare exit.
- Managed readiness deadlines are configurable with
  `health_wait_timeout_secs` (default 60 seconds).
- `deps pin bun <LIBRARY_PATH>` and `deps unpin bun <LIBRARY_PATH>` now author
  an explicit committed root-consumer override closure without running install
  or changing Bun lockfiles. Text and `effigy.deps.pin.v1` JSON reports expose
  conflicts, writes, verification, portability warnings, and next actions.
  Disposable Soundcheck/Poodle proof covers cross-repository `file:` edges,
  one canonical local package identity, exact unpin recovery, and untouched
  intermediate repositories.
- `deps status bun` and doctor now warn when a cross-repository `file:`
  dependency exposes a package symlink from its own `node_modules` into a
  third checkout. Findings name the dependency, package, symlink, target, and
  unlink-or-override remediation.

### Changed
- CLI help, generated task configuration guidance, active reference docs, and
  the bundled agent skill now expose managed headless companions, readiness
  scope and timeout, process ordering, workspace ownership diagnosis, and
  non-console exec identity consistently.
- `effigy dev` now unlocks task, container, and Rhai secrets through a separate
  mode-`0600` local-dev key after vault setup. Direct vault commands remain
  passphrase-gated; existing vaults prompt once on the next `dev` run to add
  the local-dev channel.
- Effigy's release protocol now requires a successful manually dispatched
  `ci.yml` run for the exact candidate source commit before release prepare or
  execute. The self-hosted release configuration enforces the
  proof and rejects missing, red, pending, or different-commit CI runs.

### Fixed
- Debug builds now embed bundled catalog and starter assets, preventing
  parallel tests that change process cwd from making `effigy init` and
  container fixtures fail nondeterministically.
- Release preparation no longer fails nondeterministically when parallel
  contract-selection tests race on a timestamp-derived temporary directory.
- Updated the locked `h2` dependency to `0.4.16`, resolving
  `RUSTSEC-2026-0258`. `deps pin bun --dry-run` can now use its existing
  read-only text-lock fallback when the Bun executable is unavailable.
- `effigy exec` now works from non-console sessions without requesting a TTY.
  Primary-service execs use the same declared workspace user and `HOME` as
  host-routed tasks, while explicit non-primary service execs retain their
  service-defined user.
- Container secret forcing no longer promotes keys declared
  `required = false` into required values. Managed readiness skips gateway
  routes targeting host processes that the lifecycle entry has not started,
  concurrent `start` ranks remain the process spawn order in headless mode,
  and host-dispatched workspace tasks now run as the declared workspace user
  instead of root-poisoning shared volumes before the dev handoff.
- `effigy doctor` now accepts the supported root `[secrets]` surface and task
  `secrets = "required"` mode instead of reporting them as unsupported keys.
- `deps pin bun` now falls back to read-only package enumeration from a valid
  text `bun.lock` when `bun pm ls --all` fails. The warning-bearing fallback
  understands Bun's JSONC format and nested package keys, while malformed or
  unsafe lock data still refuses with zero writes and Bun link inventory stays
  process-authoritative.
- `deps link bun` mixed-closure refusals now explain that consumer-level Bun
  overrides are the transitive cross-repository mechanism and print a
  paste-ready block covering every matched local-library package. The command
  remains save-less and does not edit `package.json`.
- Docs index exclusions now apply to links collected from the index as well as
  files discovered under its directory. Linked child indexes in excluded
  subtrees are no longer misreported as nonexistent or subjected to the parent
  next-action policy.

## [0.11.0] - 2026-08-11

### Breaking
- `effigy test` is now always the built-in test orchestrator. Legacy
  `[tasks.test]` manifests are rejected with migration guidance; custom and
  polyglot test commands belong under named `[test.suites]` entries. Package
  migration writes package test scripts there. Configured suites now accept
  managed run-step sequences, including task references, and `test --plan`
  resolves those sequences without executing setup, suite, or teardown work.

### Changed
- Auto-detected Cargo test runners now select `--workspace` when the root
  manifest declares `[workspace]`, so `effigy test` covers member-crate tests
  without requiring a custom suite. Explicit runner and suite overrides remain
  the authority for intentionally narrower boards.
- Built-in test plans now distinguish default-board suites from focused
  on-demand suites through `test.suites.<name>.default = false`. Root test
  fanout can omit catalogs with `test.exclude_catalogs` while direct
  `catalog/test` selection remains available. Plans report exclusions and warn
  when nested Cargo targets appear to cover overlapping workspaces.
- Manually dispatched binary release runs now include the selected version tag
  in their GitHub Actions title.

### Fixed
- `effigy deps link` now resolves relative Cargo and Bun library paths from the
  selected consumer repo, including when `--repo` targets it from another
  working directory. Absolute library paths remain unchanged.
- Graph help now matches runtime behavior: data queries self-refresh stale or
  missing indexes, while `graph status` remains the report-only surface.
- `scan validation-gaps` now prepares graph traversal indexes once per scan
  instead of reopening and rebuilding the full affected-query state for every
  hotspot. Its `include_heuristic` option now also governs unresolved-symbol
  matching during nearby-test traversal.
- Cargo workspace release preparation now bumps version requirements on
  coordinated `[workspace.dependencies]` path entries alongside
  `workspace.package.version`. Plans preview the edits, failed preparation
  restores them with the version manifest, and registry, Git, external,
  excluded, or independently versioned packages remain untouched.
- `effigy deps link bun` now treats same-version peer installs across consumer
  and library trees (including Bun `.bun` copies) as shared instead of
  duplicate. Only mismatched peer versions fail verification, status, and
  doctor.
- Mutating `effigy deps` commands now exit non-zero when their operation report
  contains errors or a failed outcome, while preserving the detailed human or
  JSON report. Shell recovery such as `effigy deps link bun ... || recovery`
  now runs as expected. Cargo link failures also include their error list in
  human output.
- `effigy doctor` now warns and skips project health delegation when
  `tasks.health` reaches `qa`, built-in `test`, or a recognizable full-suite
  command. Agent guidance and the Northstar starter now enforce the intended
  cheap `health` → mid-cost `validate` → full `qa` ladder.
- Graph-context scan enrichment now refreshes a stale or missing index before
  reading it. Explicit indexing and watch refreshes also honor the same
  cross-process lock as lazy graph queries instead of allowing unlocked writers.
- The bundled Effigy agent skill now starts code-understanding work with the
  self-refreshing graph query instead of requiring a status and manual reindex
  preflight.

## [0.10.0] - 2026-08-10

### Breaking
- Catalog membership is now explicit. The root catalog is always included;
  child and sibling catalogs must be declared through `[catalog.members]`, a
  structured `member` mount, or a structured `source` mount with
  `catalog = true`. Recursive filesystem discovery, automatic membership from
  legacy/ordinary mounts, `[catalog.discovery]`, discovery caches, and
  `effigy catalog cache clear` have been removed. To migrate, remove
  `[catalog.discovery]` and cache-clear calls, then list every intended child
  or sibling directory under the root `[catalog.members]`; use typed catalog
  mounts only when the same declaration also owns runtime mounting.

### Added
- Catalog manifests can declare explicit handle-to-directory membership through
  `[catalog.members]`. System and workspace mounts now accept structured
  `member` or `source` forms with typed targets, options, and source-only
  catalog membership while preserving legacy mount strings.
- `effigy papercuts` discovers conventional root `PAPERCUTS.md` queues from
  one project or the immediate projects below a collection directory, with
  open-only/default and `--all` human output plus agent-ready
  `effigy.papercuts.v1` JSON. `effigy papercuts add` safely creates or updates
  one project queue with required canonical fields, duplicate-title refusal,
  locking, and atomic replacement.

### Changed
- Removed the repo-wide Clippy allowances for large error variants, excessive
  argument lists, and complex types. The affected APIs now use focused error,
  request, and context types, and the workspace passes Clippy with warnings
  denied.
- Graph data queries (`search`, `node`, `callers`, `callees`, `impact`, `context`,
  `explore`, `affected`, `files`) now refresh a stale or missing index on demand
  instead of returning stale results. A cross-process lock
  (`.effigy/graph/refresh.lock`) guarantees only one process re-indexes at a time;
  concurrent queries wait a short budget and then report the true trust state.
  `graph status` stays report-only, and `graph index` and `graph watch` share the
  same lock so parallel refreshes never race. On git repos whose indexed HEAD
  still matches with a clean working tree, freshness is verified via `git status`
  without a full scan; non-git repos and any git failure fall back to the
  scan-state walk. Correctness-gated scans (`dead-code`, `validation-gaps`,
  `boundary-violations`) refresh through the same gate instead of refusing on a
  stale or missing index, and only refuse when the refresh itself cannot
  complete. `effigy graph status --refresh` provides the same on-demand
  rebuild for the status surface, which stays report-only by default.
  `effigy doctor` gains a `graph.index` check that flags a stale or degraded
  index with the `graph status --refresh` remediation (a missing index is
  self-healing, so it stays silent).
- Effigy's CI and JSON-contract workflows now run for pull requests or explicit
  manual dispatch instead of every push to `main`. Binary publication is also
  explicitly dispatched with an existing annotated tag; the workflow validates
  the tag, version, and checkout identity before building immutable tagged
  source, so a dropped GitHub tag event can be recovered without re-tagging.

## [0.9.1] - 2026-08-06

### Changed
- `release execute --allow-stale` is now documented and reported as an
  age-only acknowledgement. Branch, HEAD, or prepared-file source drift still
  requires re-prepare because the recorded gates belong to the original source
  identity; JSON execute plans now include the same `suggested_actions`
  recovery guidance as resume.
- `release.sync-files` now syncs `Cargo.lock` with `cargo update --workspace` instead of `cargo generate-lockfile`, and refuses to apply if anything other than the workspace members' own versions moved. `generate-lockfile` rebuilt the lockfile from scratch and resolved every dependency to newest-compatible — on signal's `0.1.0` prepare that silently moved ~40 third-party crates including `syn 2 -> 3`, which is why signal removed `sync-files` entirely and then had to carry the lockfile in a separate hand-made commit. A tag whose manifest and lockfile disagree cannot be built with `--locked`, and every `--locked` gate refuses to run between the bump and the sync, so this is what makes a Cargo release consistent by construction.

### Fixed
- Unit tests that load generated container policies or gateway state no longer
  allocate and prune loopback identities in the user's persistent registry.
  The leak could exhaust the 50-address pool and make Effigy's own release
  `test` gate fail nondeterministically under parallel execution.
- `release execute` no longer requires the prepared-state file to appear as a working-tree change. The presence check runs through `git status`, which honours `.gitignore`, so any repository that gitignores `.release-prepared.json` — signal and swallowtail both do, correctly, since it is single-run local state — reported it permanently missing and could never execute at all. It is now tolerated whether tracked or ignored.
- `release prepare` now rolls its mutations back when it reports `Prepared: no`. It applied the version bump and changelog roll before running the gates, so a failing gate left a half-applied release in the working tree with nothing in the output saying the tree had been touched. Paths that cannot be restored are reported as blockers rather than silently left.
- `release resume`, `prepare` and `execute` no longer hang without a terminal. The review menus read from stdin, and `resume` has no `--yes` escape, so running it from a script or non-interactive shell blocked forever with no output. They now render the same read-only summary `--plan` produces.

## [0.9.0] - 2026-08-06

### Added
- `effigy deps` and `effigy deps status [cargo|bun]` expose read-only machine-local Cargo/Bun link state. `effigy deps link cargo <PATH>` plans, atomically applies, and verifies full-closure repo-local Cargo patches. `effigy deps unlink cargo <PATH>` removes only owned state, re-resolves exact committed Git sources, and rejects unrelated lockfile drift without Git restore commands. `effigy deps link bun <PATH>` registers and links the full local package closure through explicit `--no-save` operations, verifies every symlink, diagnoses duplicate peer resolution, and rejects manifest/lock churn. `effigy deps unlink bun <PATH>` removes only exact matching consumer symlinks and unregisters only provably unshared Effigy-owned packages. All operations support exact dry-run plans and versioned JSON reports.
- Release configuration can opt into `initial-tag-current-version` so a repository that already declares its intended first version can prepare that exact tag without a fake prior version. The exception is limited to changelogs with no released versions and refuses an existing matching tag.

### Changed
- Refreshed the Rust and CI dependency graph, including `base64 0.23`, `reqwest 0.13`, current compatible transitive releases, direct OS randomness through `getrandom 0.4` for vault salt and nonce generation, and `actions/checkout` plus `actions/setup-node` v7.
- `effigy deps status` now reports typed warning/error severity, exact evidence, and remediation for Cargo config/lock hygiene, Bun closure and saved-link drift, registration conflicts, and duplicate peer resolution; text and JSON use the same dependency-domain observations.
- `effigy doctor` now surfaces healthy machine-local dependency links as information, repairable Bun link loss as a warning, and closure, config, lock, registration, saved-link, and duplicate-peer failures as errors using the same evidence and remediation as `effigy deps status`.

### Fixed
- Release execution now creates and pushes an annotated Git tag whose message
  exactly equals the rendered tag, instead of flattening release identity into
  a lightweight tag.
- Cargo dependency link/unlink now recognise compatible pre-Effigy patch tables: link adopts them into managed state, unlink can remove them directly, and locked planning observes the committed graph outside repo-local Cargo config without mutating `Cargo.lock`. Mixed tables and mismatched paths remain refused.
- S3 XML responses now resolve through `quick-xml 0.41`, avoiding the CPU and memory denial-of-service vulnerabilities reported as RUSTSEC-2026-0194 and RUSTSEC-2026-0195 while upstream `s3` remains constrained to the vulnerable 0.40 line.
- Managed Bun re-link now repairs a partial local package closure left by `bun install`; the same partial shape remains rejected when no Effigy desired-state record proves ownership.
- Cargo dependency discovery, dry-run, status, and verification now ignore archived reference trees and non-member workspace manifests, keep planning/status metadata locked and read-only, scope post-link inspection to persisted consumer roots, and classify owned `patch.unused` lock entries as active-link state. Nested repositories no longer rewrite unrelated workspace lockfiles while linking or observing local dependencies.
- `effigy secrets doctor` now prompts for the vault passphrase in interactive terminals so it can validate stored values; non-interactive runs still report the vault as locked without blocking.
- `effigy graph affected` now indexes resolved graph adjacency once and evaluates unresolved references once per query, avoiding runaway CPU on large indexes with many changed symbols.
- Linux workspace artifact builds now stream in-container Cargo output while refreshing the managed container binary, instead of buffering the log until the build exits.
- Container runtime failures caused by `no space left on device` now report targeted Colima profile restart and cache-prune remediation instead of only dumping raw compose output.
- Routed container tasks on workspace containers now install the workspace effigy binary before probing container exec capabilities, so a container that has only seen `container up` and routed tasks no longer degrades to running host-rendered commands through raw exec.
- Task-time container activation now repairs workspace volume ownership (cargo/git caches, isolated dirs, workspace home) even when no workspace shell session has run yet, so fresh or recreated named volumes no longer fail with permission errors.
- Invoking a `run_in = "container"` task from a child catalog directory now falls back to the nearest ancestor catalog's `[containers]` registry instead of failing with `no container target is defined`.
- Tasks in child catalogs without their own `[env_schema]` now inherit the nearest ancestor catalog's env schema (resolved against the ancestor's catalog root), so bundle/workspace-level dev env reaches app processes in child catalogs.
- Managed dev-session `shell` and `lifecycle` role processes now receive the resolved catalog env schema (including ancestor-catalog fallback) through the same `env KEY=VALUE` command prefix used for vault secrets, so commands run manually in the dev-session shell tab see the same schema env as task-rendered processes.

## [0.8.17] - 2026-06-17

### Added
- `effigy catalog cache clear` removes the repo-local catalog discovery cache, forcing the next catalog walk to inspect previously pruned large empty subtrees.

### Changed
- Catalog discovery now caches discovered manifest paths and large no-catalog subtrees under `.effigy/cache`, avoiding repeated full-tree walks in repos with large generated data directories.

### Fixed
- Generated asset scans now ignore Effigy's internal `.effigy` state, so catalog cache files do not inflate scan counts or trip generated-assets reports.
- Container project names are normalized before backend calls, so bundle-derived names from mixed-case paths such as `r7-webCore` no longer fail Docker/nerdctl project-name validation.

## [0.8.16] - 2026-06-16

### Fixed
- Port auto-allocation now fails with an explicit registry-exhausted error when no full project range remains, instead of creating a truncated `65535..65535` allocation that later breaks container bootstrap.
- Explicit and implicit container deferrals run from inside a managed container no longer re-resolve host-side external mount paths against the container repo root, so mounts like `host = "../../libraries"` remain host-relative instead of failing under `/var/www/...`.

## [0.8.15] - 2026-06-15

### Added
- Managed Colima profile disk sizing is now configurable via `effigy config set containers.profile_disk_gib <GiB>` and `effigy container profile recreate --disk <GiB>`. Interactive `profile recreate` runs prompt for the disk size when `--disk` is omitted.
- `effigy container profile purge` deletes the managed Colima profile and its runtime data without recreating or restarting it.
- `effigy uninstall` now plans or removes initial Effigy-owned local machine state: user-global config, user-global catalog overrides, and the managed Colima profile.

## [0.8.14] - 2026-06-15

### Fixed
- Bootstrap and catalog discovery now ignore `catalog.alias` values supplied only by bundle defaults. Repos without an explicit project alias use the repo/folder name, so bootstrapping a repo like `git@github.com:acowtancy/acowtancy.git` keeps the `acowtancy` destination instead of renaming it to an inherited `root`.
- Bootstrap's built-in database seed fallback now prepares the default container runtime before importing SQL dumps, so bundle-backed repos without an explicit `bootstrap:db-seed` task do not fail with `container service ... is not running` immediately after root setup.

## [0.8.13] - 2026-06-11

### Fixed
- Workspace/container handoff no longer fails when a TCP alias domain maps to a service name that is not resolvable inside the primary container yet. Effigy now warns and skips that `/etc/hosts` patch instead of aborting `docker compose exec tcp alias hosts`, so optional sidecars like `memcached` do not block container bring-up.

## [0.8.12] - 2026-06-11

### Added
- Supply-chain policy via `cargo-deny` (`deny.toml`): enforces RUSTSEC advisories, an OSI-permissive license allowlist, registry-wildcard bans, and a crates.io-only source rule. Enforced in CI by a `cargo deny check` job and paired with Dependabot weekly cargo + GitHub-Actions update PRs. Run `cargo deny check` locally before changing dependencies.

### Added
- The gateway now verifies route-table integrity before the (possibly elevated) daemon trusts it: the route table is written owner-only (`0o600`) with an Effigy-managed provenance marker, and the daemon's read path refuses a group/other-writable or unmarked/foreign-marked table, keeping its last-known-good in-memory routes instead. `effigy gateway status` reports `route_table_trust` and `effigy doctor` warns when the table is untrusted. (Migration: a `routes.json` written by an older Effigy lacks the marker and will be treated as untrusted until the next route registration re-stamps it — re-run `effigy container up` or re-register routes once after upgrading.)

### Changed
- The repo-wide clippy allows (`result_large_err`, `too_many_arguments`, `type_complexity`) now live in `[workspace.lints.clippy]` instead of CLI `-A` flags, and the 33 redundant per-site `#[allow(clippy::too_many_arguments)]` attributes were removed. A plain `cargo clippy` now matches CI with no extra flags.
- Updated dependencies (regex, s3, reqx, tabled, sha2 0.11, rusqlite 0.40) and pinned the toolchain to Rust 1.96 via `rust-toolchain.toml`. The newer `sha2`/`rusqlite` releases require rustc ≥ 1.95; this raises the project's effective minimum supported Rust to 1.96.

### Fixed
- Container bring-up now warns pre-emptively when the colima-forwarded host SSH-agent socket is stale (a dangling `/run/host-services/ssh-auth.sock` after the host agent socket rotates on a long-running VM), naming the `colima restart <profile>` fix — instead of failing later with the cryptic nerdctl `mkdir /run/host-services/ssh-auth.sock: file exists`.
- `effigy scan dead-code` now refuses a stale graph index (not just an unusable one) and points to `effigy graph index`, instead of reporting false positives from drifted symbol positions and missing edges.
- `SecretValue` now serializes as `[REDACTED]` instead of plaintext. A secret accidentally included in a `Serialize`-derived struct (logs, JSON output, diagnostics) can no longer leak; the encrypted vault payload is the only path that serializes real secret bytes, and it now opts in explicitly.
- The local gateway daemon and the process supervisor no longer cascade a panic across every subsequent request or child-reap when a lock is poisoned by an unrelated thread panic. Poisoned route-table, TLS-cert, and child/process-map locks now recover the inner guard and keep serving instead of aborting.
- `effigy doctor` no longer reports a schema error on this repo's own clean tree. Test-fixture manifests under `tests/` are excluded from ambient catalog discovery via `[catalog.discovery] ignore`, so partial/malformed fixture `effigy.toml` files are no longer treated as live catalogs.
- The manifest schema validator now recognizes the `[catalog.discovery]` table (`enabled`, `ignore`) that catalog discovery already consumes, so configuring it no longer trips `manifest.schema.unsupported_key`.

## [0.8.11] - 2026-06-09

### Fixed
- The release-binaries workflow now pins macOS runners by CPU architecture so Apple Silicon and Intel release builds run their smoke checks on matching GitHub-hosted images.
- Gateway loopback migration now persists repo-scoped identity upgrades back to the registry file instead of only updating the in-memory assignment map.

## [0.8.10] - 2026-06-09

### Fixed
- `effigy container down` now removes derived gateway TCP service-alias routes reliably after container shutdown, and gateway TCP alias registration now rejects conflicting bind tuples while deduplicating identical shared listeners.
- `effigy gateway status` now reports duplicate TCP bind tuples, and `effigy gateway repair [--yes]` can inspect and remove stale conflicting container TCP alias routes already left behind in local gateway state.
- Gateway TCP alias loopback assignment is now scoped by repo path, so separate repos that share the same container `project_name` can coexist without collapsing onto the same `127.1.0.x` bind IP.
- When multiple repos ask a local Effigy checkout to build the shared Linux workspace artifact at the same time, later callers now wait for the in-progress `workspace:linux:artifact` task to finish and reuse its output instead of failing on the task lock.

## [0.8.9] - 2026-06-09

### Fixed
- `effigy container profile resize` now applies managed Colima sizing by stopping and restarting the profile in place, and `container profile status` now points operators at that non-destructive path instead of only the destructive recreate workflow.

## [0.8.8] - 2026-06-07

### Fixed
- `release:linux:rehearse` now streams in-container Linux build output as it
  runs instead of buffering the full build log until the container command
  exits.
- `effigy container cache list --global` now detects legacy opaque `efv-*`
  volumes that contain Rust target directories, so
  `effigy container cache prune --kind rust-target --yes` can clear old
  container build caches instead of relying on orphan volume cleanup.
- The managed default `effigy` Colima profile now targets a 300GiB disk for
  new or recreated profiles, and running-profile warnings include undersized
  disks as well as undersized memory.
- `effigy container profile status` and `effigy container profile recreate`
  now provide an explicit inspect-and-rebuild workflow for applying managed
  Colima profile sizing to existing local profiles.

## [0.8.7] - 2026-06-05

### Fixed
- `effigy scan dead-code` now recognizes Rust public/test/API roots,
  descriptor and dispatch roots, data-shape references, impl/call references,
  serde default helpers, binary entrypoints, nested crate modules, and
  cross-file references more accurately, reducing Effigy's own current
  dead-code findings to zero after removing confirmed dead artifacts.

## [0.8.6] - 2026-06-04

### Changed
- JSON Contracts CI now runs on PRs, pushes to `main`, and manual dispatch only;
  the daily scheduled run has been removed.

### Fixed
- `effigy bootstrap` no longer renames an existing reused default destination
  after reading the root catalog alias, and missing root catalog aliases now
  default to the repository directory name instead of `root`.
- Container policy port allocation now handles near-exhausted host-port ranges
  without overflowing internal `u16` arithmetic.
- Container and data command tests no longer depend on the caller's live
  `~/.effigy` port registry for auto-port allocation.
- Concurrent-runner demo fixtures now use valid shell command fragments, and
  the demo CLI tests wait for terminal-session and handoff readiness instead of
  racing the active-attempt file.

## [0.8.5] - 2026-05-28

### Added
- Rhai scripts now have an adapter-shaped `storage::*` host module, with an
  S3-compatible first provider for object-store status, listing, metadata,
  download, upload, and delete workflows against AWS S3 or compatible
  endpoints such as MinIO.

### Fixed
- `storage::head` now returns S3-compatible user metadata from
  `x-amz-meta-*` headers, and `storage::put` reports the uploaded byte count.

## [0.8.4] - 2026-05-25

### Added
- `effigy rhai surface` now prints the registered Rhai host API surface, with
  `--json` support for agents that need module/function discovery without
  leaving the runtime.
- Rhai scripts now have a first-class `git::*` host module for common local git
  operations such as status, branch inspection, changed-file lookup, switching,
  staging, committing, pulling, and pushing.
- Rhai scripts now have an adapter-shaped `forge::*` host module, with GitHub
  support through the `gh` CLI for provider/status checks and pull request
  view, list, create, and checkout workflows.
- Rhai `git::*` now includes safety and ref-inspection helpers for clean-tree
  assertions, commit existence, merge-base checks, remote URLs, and upstream
  branch lookup.
- Rhai scripts now have `prompt::*` helpers for TTY-only confirmation and
  free-text input in interactive automation.
- Rhai scripts now have `semver::*` helpers for parsing, validating,
  comparing, requirement matching, and bumping semantic versions.

## [0.8.3] - 2026-05-25

### Added
- `effigy scan` now accepts `--graph-context` and exposes graph-readiness
  metadata in JSON and text output when graph context is requested, without
  changing the underlying scan findings yet.
- `effigy scan --graph-context` now enriches `god-files` and
  `attention-markers` findings with optional file-level graph facts when a
  usable graph index exists.
- `effigy scan boundary-violations` now checks configured path-layer rules
  against graph edges and reports disallowed cross-layer dependencies with
  concrete source and target evidence.
- `effigy scan dead-code` now reports likely isolated implementation files and
  unreferenced symbols from concrete graph evidence, with confidence labels,
  reason fields, and path/symbol allowlists for intentional isolation.
- `effigy scan validation-gaps` now reports hotspot owners and changed owners
  without nearby graph-backed test targets, and surfaces likely test files and
  tasks separately when the graph can justify them.

### Changed
- **Effigy agent routing guidance is now job-based:** the bundled skill, repo
  contract, and adoption docs no longer teach `doctor -> tasks -> test --plan`
  as an automatic entry ritual. Agents are now directed to start with the
  surface that matches the work: graph for code understanding, tasks for
  selector inventory, doctor for routing or health ambiguity, and `test --plan`
  only when test execution shape matters.

### Fixed
- **Workspace-local version strings no longer leak install cache keys:** Linux
  workspace handoff now keeps the human-facing `effigy.active-version` file as
  a plain build version and stores the handoff freshness/install identity in a
  separate sidecar, so local workspace containers stop reporting long internal
  strings like `vlocal:...` from `effigy version`.

## [0.8.2] - 2026-05-21

### Fixed
- **Workspace handoff no longer fails early when Colima is stopped:** the
  gateway route readiness probe now treats “runtime not running yet” as
  `not ready` instead of aborting workspace/dev handoff before activation can
  start the container runtime.

## [0.8.1] - 2026-05-20

### Fixed
- **Release glibc guard command:** The GitHub release-binary workflow and Linux
  release rehearsal now use `effigy release check-binary --glibc-floor ...`
  instead of the removed top-level `distribution check-glibc-floor` command.

## [0.8.0] - 2026-05-20

### Breaking
- **Removed `context = "dev"` container targeting:** Manifests can no longer
  mark containers with `context = "dev"`. Default task and `effigy exec`
  container targeting now resolves through `[systems].default` and the selected
  workspace's backing `container`; legacy `context` keys are rejected.
- **Distribution commands moved under release:** The top-level
  `effigy distribution` command has been removed for v0.8.0. Use
  `effigy release validate`, `release check-binary`,
  `release preflight`, `release proof`, and
  `release evidence <validate|summary|closeout>` instead.

### Added
- **Secrets import command:** `effigy secrets import [<PATH>]` now imports
  declared secret keys from a `.env`-style file into the local Effigy vault,
  lowercasing env names to match manifest keys, defaulting to `./.env`, and
  skipping undeclared values without printing secrets.
- **Native code graph surface:** Effigy now ships `graph index`, `status`,
  `search`, `files`, `node`, `callers`, `callees`, `impact`, and bounded
  `context` commands backed by a local `.effigy/graph/graph.db` index. The
  first-party extractors cover Rust, Effigy manifests/TOML, Markdown docs,
  PHP, Python, and JavaScript/TypeScript, and `graph context` returns ranked
  items with reasons, provenance, snippet budgets, and overflow counts for
  agent-friendly repo navigation.
- **Foreground graph watch mode:** `effigy graph watch` now keeps the local
  graph warm with filesystem events, a conservative debounce, and the existing
  incremental index path, including newline-delimited JSON watch events for
  agent consumers that want direct freshness updates without polling.
- **Graph explore command:** `effigy graph explore "<question>"` now returns a
  one-call agent navigation packet with primary owners, excerpts, related
  symbols, index freshness, overflow, and exact-search fallback guidance under
  the `effigy.graph.explore.v1` JSON contract.
- **Graph affected command:** `effigy graph affected` now turns changed-file
  input from args or stdin into a bounded impact packet with affected files,
  likely test files, candidate Effigy test tasks, confidence labels, and
  traversal reasons under the `effigy.graph.affected.v1` JSON contract.
- **Rhai state capture-set helper:** Rhai scripts can now call
  `state::capture_set(options_map)` to drive the existing CLI `state capture-set`
  feature surface with stack, profile set, key, confirmation, and push options.
- **Typed Rhai deploy and distribution helpers:** Rhai now exposes the deploy
  transaction surface (`deploy::plan`, `deploy::apply`, `deploy::status`,
  `deploy::history`, `deploy::redeploy`) and the distribution command family
  (`distribution::validate_metadata`, `distribution::check_glibc_floor`,
  `distribution::preflight`, `distribution::first_publish`,
  `distribution::validate_artifacts`, `distribution::generate_closeout`,
  `distribution::write_summary`) through the same typed CLI-backed command
  contracts and confirmation semantics as the main Effigy surface.
- **Idempotent agentic init:** `effigy init` now provides a cohesive
  human-and-agent setup surface with `--check`, `--apply`, `--repair`, and
  `--json`. It plans or writes a baseline manifest and README when missing, the
  managed `AGENTS.md` contract, project-local `.agents/skills/effigy` skill
  files, and local `.effigy/` ignore policy while preserving existing project
  manifests and READMEs.
- **Interactive TTY init wizard:** plain `effigy init` now enters a bounded
  yes/no setup wizard on a real TTY, starting with baseline repo files and
  agent setup while preserving the existing deterministic non-interactive
  behavior for `--apply`, `--check`, JSON mode, and non-TTY invocation.
- **Checklist-driven init execution:** `effigy init` now exposes
  `--checklist --json` for a machine-readable setup-job inventory and
  `--apply-actions <ID>[,<ID>...]` for explicit non-interactive execution with
  per-action outcomes under the `effigy.init.checklist.v1` and
  `effigy.init.actions.v1` contracts.
- **Container exec-readiness architecture proof task:** Effigy now ships
  `qa:architecture:container-exec-readiness`, a focused regression task that
  pins the status report, runtime drift warning, and runner recovery surfaces
  behind `primary_service_exec_ready`.

### Changed
- **Catalog discovery can now be disabled at the root:** repos can now set
  `[catalog.discovery] enabled = false` to keep ambient catalog discovery
  root-only. In that mode, Effigy skips child catalog walking and mounted
  catalog expansion entirely, which keeps task routing predictable for
  single-app repos that do not need nested catalogs.
- **Container secret runtime files are now an explicit generic opt-in:**
  containers can now declare `[containers.<name>.secrets] delivery =
  "runtime-files"` with a `runtime_dir` and optional deferral sourcing. In
  that mode, Effigy writes `runtime.env` and `runtime.json` inside the running
  primary service instead of using compose env injection. Bundles can then wire
  those files into their own runtime shape without hard-coded Rust knowledge of
  the app stack. Repo `.env` files remain available for non-secret local
  overrides.
- **Vendored repo-local Effigy skills now hide from generic skills-cli
  discovery:** `effigy init` now writes the project-local
  `.agents/skills/effigy/SKILL.md` with `metadata.internal: true`, so the
  repo-authoritative vendored copy stays available to in-repo agents without
  competing with the public `skills/effigy` distribution surface.
- **Graph adoption now has a cross-repo benchmark surface:** Effigy now ships
  `perf:graph-agent-benchmark`, a fixture-backed and skip-safe benchmark that
  records graph command count, fallback search count, timing, first-hit
  correctness, and packet sufficiency across synthetic cases plus optional live
  repo targets. It writes both markdown and JSON output under
  `.effigy/perf/graph-agent-benchmark/` so closeout and follow-up work can use
  repo-owned evidence instead of ad hoc thread notes.
- **Graph guidance is sharper about when to use graph versus `rg`:** the
  project-local and distributed Effigy skills, plus the active adoption guide,
  now steer agents toward implementation-shaped graph questions, explicit
  trust-state checks, and portable query examples while keeping exact-token
  lookup and final pre-edit proof on the `rg` path.
- **Graph freshness now exposes a compact trust signal across repos:** `effigy
  graph status` and graph query JSON payloads now include a compact
  freshness/trust state with a summary, usability flag, and stale/failed path
  counts, so agents can distinguish `missing-index`, `refresh-recommended`,
  `degraded`, and `ready` without inferring trust from large path lists. The
  detailed stale/new/changed/deleted diagnostics remain available for operator
  follow-up.
- **Graph behavior-shaped queries are less phrasing-sensitive:** request
  ranking now drops repo-name tokens, adds light singular/plural normalization,
  and expands generic behavior vocabulary for prompt, shutdown, exit,
  validation, redirect, migration, cache, and index terms, so natural
  implementation questions land closer to the owner file without hard-coded
  repo-specific boosts.
- **Graph explore packets now surface edit and test targets:** `effigy graph
  explore --json` now includes bounded `edit_targets`, `likely_test_files`,
  and `likely_test_tasks` projections so agents can move from navigation into
  editing and validation with less manual inference. The packet keeps confidence
  labels explicit and still treats likely tests as bounded candidates rather
  than exhaustive proof.
- **Graph context ranking is now role-aware:** implementation-oriented context
  requests prefer implementation files over tests/docs, test and docs requests
  still promote those surfaces, and repeated same-file symbol matches are capped
  so broad terms do not drown out better owner files.
- **Graph explore ranking is now closer to agent navigation intent:** ranked
  owners now use source-body evidence, distinct request-token coverage,
  stronger stop-word filtering, and Effigy-domain query synonyms so task-shaped
  questions are less likely to land on docs, comments, or generic parser files
  before the implementation owner.
- **Graph ranking now uses indexed source evidence:** file-body token evidence
  is now stored in the local SQLite FTS graph index and reused during ranking,
  so `graph context` and `graph explore` no longer need broad candidate-file
  reads just to decide which owners matter.
- **Graph explore now walks bounded topology around primary owners:** one-hop
  traversal can now add call/import/doc neighbors from resolved edges and from
  bounded unresolved Rust/JS targets, so the explore packet can surface related
  files with explicit traversal reasons instead of only ranked owners.
- **Graph now understands first route and entrypoint facts:** the code graph
  now emits bootstrap task selectors from Effigy manifests, Python HTTP route
  symbols from FastAPI/Flask-style decorators, exact `entrypoint-task` and
  `route-handler` edges where resolution is reliable, and route-shaped queries
  now preserve literal path tokens such as `/users`.
- **Graph explore packets now label section completeness:** excerpt payloads now
  include `section_kind` and `completeness`, same-path excerpts are deduplicated,
  and supported Python/Markdown sections expand to fuller local blocks so
  agents can trust when a packet is complete versus when a file still needs to
  be opened.
- **Graph storage now upgrades more predictably:** the local graph DB now uses
  storage schema `2`, backfills indexed source-search rows when opening older
  graph databases that predate file-body FTS evidence, rejects newer unknown
  storage schemas instead of guessing, and opens SQLite with a steadier local
  read/write posture for watch-mode and query overlap.
- **Graph search and context output are more actionable:** graph search now
  includes snippets for file and symbol matches, and file-level context items
  prefer snippets near matched symbol evidence instead of always starting at the
  top of the file.
- **Init now routes follow-up setup through a shared adapter inventory:** the
  TTY init wizard now builds a repo-context inventory for task migration,
  health checks, graph setup, bundle/secrets/runtime follow-up, validation,
  and read-only advanced surfaces, and only recommends commands that actually
  exist while keeping release/deploy/state/distribution mutation out of init.
- **TTY init now executes runnable setup jobs:** when plain `effigy init` runs
  on a real TTY, it now continues past satisfied baseline setup and asks yes/no
  questions for runnable setup jobs such as task migration, graph status/index,
  bundle inspection, and secrets inspection instead of only listing those
  commands as follow-up text. Health checks such as `doctor`, `tasks`, and
  `test --plan` remain end-of-wizard next-step guidance.
- **Interactive workspace and container shell exits now ask before shutdown:**
  leaving `effigy workspace`, `effigy dev`, and direct `effigy container <NAME>
  shell` sessions now prompts whether to bring the runtime down, with Enter
  accepting the default `yes`, instead of tying shell-exit cleanup to whether
  the runtime was session-owned or adopted.
- **Container shell perf matrix now records runtime readiness too:** the
  `perf:container-shell-matrix` task now captures each target repo's
  `container status` output and calls out `primary_service_exec_ready` beside
  the shell timing report, so decodelabs, underlay, and other workspace-style
  stacks can be compared on both latency and actual exec readiness.
- **Container shell perf matrix now covers decodelabs library and app fixtures
  plus underlay:** the maintained live matrix now exercises a decodelabs
  library archetype, a decodelabs site, and the underlay workspace reference

### Fixed
- **Runtime-files container secret delivery now writes through exec:** the
  generic `secrets.delivery = "runtime-files"` path now streams `runtime.env`
  and `runtime.json` into the running primary service over `exec` instead of
  relying on backend `cp` behavior against runtime-mounted paths like tmpfs.
- **Ambient task discovery now respects nested Effigy roots:** `effigy tasks`
  and other catalog discovery surfaces no longer walk into nested child
  projects whose local `effigy.toml` declares `[manifest].root = true`, so
  repo-root task listings do not leak example or fixture catalogs from
  self-contained subprojects.
  so shell-path changes are checked across more than one container shape.
- **Container shell perf matrix now emits a compact JSON summary:** alongside
  the markdown reports, `perf:container-shell-matrix` now writes
  `.effigy/perf/container-shell-matrix/summary.json` with per-target readiness
  and timing fields so regressions can be compared without scraping text.
- **Effigy's own task catalog is leaner:** the repo-local
  `config/tasks.toml` now groups related tasks, removes stale one-off aliases,
  and inlines private implementation steps so `effigy tasks` is easier to scan
  without changing the main QA, release, bootstrap, or container proof
  workflows.

### Fixed
- **PHP workspace catalogs now pin pnpm 11 to the dedicated store volume:**
  the `php-fpm` generated-compose surface now exports
  `pnpm_config_store_dir` as well as the older npm-compatible store env, so
  modern pnpm releases stop creating repo-local `.pnpm-store` directories in
  bind-mounted project roots. The shipped PHP workspace image also now keeps
  Corepack's cache tree writable for the `dev` user, so `pnpm` can actually
  run inside those containers instead of failing on a root-owned
  `/home/dev/.cache`.
- **Workspace container handoff no longer runs full Linux release rehearsal:**
  when Effigy needs a Linux binary for `workspace`, `dev`, or `container shell`
  handoff into a workspace container, it now runs a dedicated artifact-build
  path instead of invoking `release:linux:rehearse`, so normal container access
  avoids release smoke checks and GLIBC validation noise. The shared
  `linux-release` builder now also keeps Cargo registry and git caches on named
  volumes so cold-path rebuilds do not redownload dependencies every time the
  container is recreated. Repeated handoffs now also skip the workspace
  container copy/install step when the target already has the same install
  identity, so unchanged local builds now bypass both the architecture probe
  and the copy/install path on steady-state entry. Workspace home permission
  prep now fixes targeted subtrees instead of recursively chowning the whole
  home directory on every entry. Direct `container shell --command` handoff now
  also reuses the runner's already-resolved container session instead of
  resolving and validating the same shell session twice, and Colima-backed
  direct exec now reuses the resolved running service container name across the
  process so the same shell handoff stops re-running identical `ps` lookups
  before every internal `exec`. Command-mode shell handoff now also tries the
  configured workspace user on the real exec path first and only falls back to
  root if the runtime reports that the user is missing, so the steady-state
  path no longer pays a separate `id -u <workspace_user>` probe exec.
- **Container status now reports primary-service exec readiness:** `effigy
  container status` now distinguishes “runtime backend is up” from “the
  primary service can actually exec in its working dir” through a
  `primary_service_exec_ready` field and warning output, so drifted workspace
  stacks stop looking healthy when the service runtime is unusable.
- **Container-local deferral avoids host runtime probes:** `effigy defer` now
  treats Effigy workspace containers as local execution contexts even when
  containerd/cgroup-v2 does not expose `/.dockerenv`, `/run/.containerenv`, or
  useful cgroup names, so external bundle deferrals do not stall while trying
  to launch host runtime tools from inside the container.
- **Workspace containers trust materialized git bundles on normal loads:**
  cached git-backed bundles no longer run remote freshness probes during
  ordinary manifest loading inside Effigy workspace containers, preventing
  slow repeated SSH timeouts for commands such as `effigy tasks` and
  `effigy defer`.
- **Ambient catalog discovery can be scoped per repo:** root manifests can now
  declare `[catalog.discovery].ignore` to keep task-catalog discovery out of
  repo-specific generated trees such as media exports or state snapshots
  without growing Effigy's hard-coded internal skip list.
- **Workspace shell tool baseline is now explicit across workspace catalogs:**
  the `node` catalog now builds through a real workspace Dockerfile instead of
  using a bare Alpine image, and the workspace-flavored catalogs now ship a
  shared baseline of shell and agent tools including `bash`, `git`, `jq`,
  `ripgrep`, `fd`, `less`, `curl`, and `wget` so bundles and repos can rely on
  `rg`-style tooling being present in workspace containers.

## [0.7.1] - 2026-05-15

### Fixed
- **External PHP bundle nginx ownership restored:** generated compose now lets
  container service config paths resolve under `{{ bundle.root }}` for
  materialized external bundles, nginx mirrors sibling php-fpm isolated dirs
  like `vendor` and `node_modules` read-only into web services, shared named
  volume identity stays stable across sibling service rewrite/compaction, and
  bundled nginx PHP configs now pass `SERVER_PROTOCOL` through to php-fpm so
  Decodelabs-style external bundles work again without core-owned provider
  variants.

## [0.7.0] - 2026-05-14

### Breaking
- **Maintainer dev wrapper removed:** the repo no longer ships `scripts/effigy-dev`
  or links `~/.local/bin/effigy-dev`. Current-checkout validation should use
  `cargo run --bin effigy -- ...`, while `effigy ...` remains the installed or
  locally linked binary path.
- **No-op secrets session commands removed:** `effigy secrets unlock` and
  `effigy secrets lock` are gone. Effigy does not keep a cross-command unlock
  session, so these commands only added misleading ceremony around the vault
  flow.

### Added
- **Manifest minimum Effigy version gate:** any manifest fragment can now set
  `[manifest].minimum_effigy_version = "X.Y.Z"` so older Effigy binaries fail
  early, including when the requirement comes from an included partial
  manifest.
- **Vault-backed task secret injection:** declared `[secrets.keys.*]` entries
  with `targets = ["tasks"]` are now loaded from the local Effigy vault and
  injected into referenced task process environments, with missing required
  values blocking before spawn only when the selected shell task actually
  references that secret env name, and captured JSON output redacted.
- **Invocation-local vault passphrase reuse** is now generic across the
  `secrets` command family, task/container secret resolution, and Rhai secret
  helpers, so one top-level Effigy run reuses the first entered passphrase
  instead of prompting again at each secret-aware sub-step.
- **Local vault generator hook:** `[secrets.vault].generate` can now point at
  an inline task such as a host-side Rhai script. `effigy secrets init` and
  `secrets = "required"` task startup use that hook to create or fill the
  built-in local vault before launch instead of only failing on a missing or
  incomplete vault.
- **Scoped Rhai secret API:** Rhai scripts can now call `secrets::get(name)`,
  `secrets::has(name)`, `secrets::set(name, value)`, and
  `secrets::set_many(map)` for declared `targets = ["rhai"]` vault secrets,
  with undeclared or wrong-target access rejected and known values redacted
  from Rhai errors and host output maps.
- **Structured Rhai data helpers:** Rhai scripts can now write and read JSON
  and TOML files directly through `json::write_file`, `json::read_file`,
  `json::stringify_compact`, `toml::write_file`, and `toml::read_file`, and
  can extract regex groups through `regex::captures(...)`.
- **YAML Rhai helpers:** Rhai scripts can now parse, stringify, read, and
  write YAML through `yaml::parse`, `yaml::stringify`, `yaml::read_file`, and
  `yaml::write_file`, giving provider packages enough structured file support
  to own deployment exports outside Effigy core.
- **Rhai URL and DSN helpers:** Rhai scripts can now parse generic URLs with
  `url::parse(...)` and MySQL DSNs with `url::parse_mysql_dsn(...)` instead of
  hand-rolling regex extraction for host, port, database, and query params.
  The same surface now includes `url::query_get(...)` plus
  `url::parse_pg_dsn(...)` for Postgres DSNs.
- **Secret-scoped deployment and state hooks:** internal Rhai execution can now
  opt into `deploy`, `state`, and `artifacts` secret targets. Deploy provider
  packages run with `deploy` secret access, and state apply hook tasks receive
  declared `state` secrets through process environment injection.
- **Provider-package deployment exports:** `effigy deploy export <PROVIDER>`
  now dispatches to the configured `[deploy.providers.<provider>]` package
  export capability instead of a fixed built-in provider list, and
  `deploy::emit(...)` can target any configured provider id.
- **Container startup secret injection:** `effigy container up` now resolves
  declared `targets = ["containers"]` vault secrets before startup, blocks
  missing required values before compose mutation, and passes resolved values
  through the compose process environment instead of writing repo-root
  plaintext files.
- **Directory artifact capture:** `effigy artifact capture` and staging now
  accept local directories as artifact payloads, preserving relative file paths
  for object-store/media-library bundles and OCI push inputs.
- **State capture sets:** `effigy state capture-set <STACK> <PROFILE>...` can
  run multiple named capture profiles with one shared key, reducing app-local
  orchestration glue for DB/media legacy snapshot exports.
- **State apply layer skipping:** `effigy state apply <STACK> --skip-layer
  <KEY>` can now leave named layers as `skipped` in the apply report, letting
  wrapper workflows pre-run prerequisite layers without duplicating work.
- **Rhai state apply context helpers:** state apply hooks can now call
  `state::apply_context()` and `state::apply_context_path()` to consume the
  structured layer handoff without reading `EFFIGY_STATE_APPLY_CONTEXT`
  manually.
- **Rhai path parent helper:** Rhai scripts can now call `path::parent(...)`
  instead of carrying app-local dirname helpers.
- **Compact task `run_in`:** shorthand task objects under `[tasks]` can now set
  task-level execution context directly, for example
  `{ rhai = "scripts/capture.rhai", run_in = "host" }`, instead of wrapping the
  step in `{ run = { ... }, run_in = "host" }`.
- **Compact state inline tasks:** state capture `task` definitions and state
  layer `hook` definitions can now use the same compact single-step inline task
  shape with `run_in`.
- **Compact bootstrap inline tasks:** root `[bootstrap].run` and child
  `bootstrap.children[].run` definitions can now use compact single-step inline
  task syntax with `run_in`.

### Changed
- **Rhai task steps** in fully in-process sequences now execute directly inside
  the runner during normal task execution, and managed-process setup sequences
  do the same when they can stay on the local route and every setup step is
  in-process-capable, instead of shelling out through a nested internal script
  command path. Container-routed managed setup keeps the routed fallback so
  container-local script behavior does not drift.
- **Managed setup execution** no longer glues named-container setup onto
  `process.run`; Effigy now executes those setup sequences through a dedicated
  routed prelaunch step before the managed process starts.
- **Remote Rhai setup invocations** now render through the internal
  `script run --file ...` surface instead of the older raw hidden Rhai
  pseudo-command.
- **Manifest version floors** now treat repo-local `+local` development builds
  as ahead of tagged releases, so local feature work is not blocked by
  unreleased `minimum_effigy_version` bumps.
- **State apply hooks** now run during `effigy state apply --yes` after a
  layer is successfully executed, staged, or imported. Apply reports now carry
  hook status/output/error plus a structured
  `EFFIGY_STATE_APPLY_CONTEXT` handoff file for repo-owned finalize work.
- **Container data seed** now prepares the selected container runtime before
  import execution, so host-run seed commands can stage local file paths
  without requiring a manual `effigy container up` first.
- **State capture profile tasks** can now be declared inline with normal task
  run syntax, such as `task = [{ rhai = "capture.rhai" }]`, instead of forcing
  a named `[tasks.*]` indirection.
- **State apply hooks** in composed manifests can now be declared inline with
  normal task run syntax, such as `hook = [{ rhai = "apply-media.rhai" }]`,
  while standalone state manifests keep selector-string hooks.
- **Single-step task refs** can now use `run = { task = "..." }` without a
  one-element array wrapper, and the Decodelabs bundle now uses that native
  shape for its `release` deferral wrapper. Shorthand task definitions under
  `[tasks]` now accept the same single-object form.
- **Built-in DB seed fallback** now removes its staged seed copies and
  metadata from `.effigy/local/db-seeds/` after a successful import instead of
  leaving consumed hidden files behind.
- **Decodelabs bundle seeding** now relies on Effigy's built-in
  `container data seed` and bootstrap DB-seed fallback instead of shipping a
  custom `bootstrap:db-seed` override and helper script.
- **Git-backed bundle sources** now reuse a short shared remote-check freshness
  window during manifest load, so repeated runs across many repos stop paying
  for a fresh `git ls-remote` probe on every invocation while still picking up
  remote bundle updates automatically and letting `effigy bundle sync` force a
  refresh immediately.
- **Repo-local JSON contract artifact capture** now runs through a native Rhai
  task helper instead of bash-plus-Python glue, and the old
  `scripts/lib/check-json-contracts/*` shell library is gone.
- **Runtime/container drift guarding** now runs through a native Rhai script
  with in-process file search and regex filtering instead of the old shell
  `rg`/`awk` guard wrapper.
- **Linux GLIBC floor validation** now runs through the native
  `release check-binary` command in release workflow/test surfaces,
  and the old `scripts/check-linux-glibc-floor.sh` wrapper is gone.

### Fixed
- **Bundle manifest version floors:** remote and path bundle defaults now
  accept and validate `[manifest].minimum_effigy_version` instead of rejecting
  the field as an unknown bundle manifest key during defaults composition.
- **Leading global CLI flags** now work consistently across built-ins and task
  selectors: top-level `--repo <PATH>` and `--json` apply before built-in
  commands like `doctor`, `tasks`, and deferred built-ins like `test`, while
  task-only globals `--verbose-root` and `--env-schema <PATH>` now work before
  the selector instead of only after it.
- **Linux rehearsal version floors** now carry the invoking binary's active
  version into the container-built Linux binary, so repo-local `+local`
  development builds still satisfy `[manifest].minimum_effigy_version` during
  in-container smoke checks.
- **Rhai state capture context paths** now resolve relative
  `EFFIGY_STATE_CAPTURE_CONTEXT` values against the task runtime CWD, so
  `state::capture_context()` works reliably with `--repo` execution.
- **State capture task source paths** are now passed to repo tasks as absolute
  paths when the manifest source is relative, preventing capture scripts from
  writing into the caller's current directory.
- **Rhai container exec cwd mapping** now translates explicit repo-host `cwd`
  paths into the matching in-container working directory before `docker
  compose exec -w ...`, while still preserving true container-native absolute
  paths.
- **Database seed/dump service resolution** now uses one shared resolver and
  treats `catalog = "mysql"` consistently with `mariadb` as a MariaDB/MySQL
  service.
- **Builtin task refs** now resolve against the full shared root-command
  catalog inside `task = "..."` run steps, fixing missing builtins like
  `defer` and `docs` in wrapper-task/task-ref paths.
- **Managed task secret injection** now reaches managed child processes when a
  task sets `secrets = "required"`, so container-backed `dev`/TUI tasks can
  pass declared task-target auth/runtime env values into their API/front/jobs
  processes instead of only unlocking the vault for lifecycle/container
  startup.
- **Vault passphrase reuse within one Effigy invocation** now covers container
  startup, managed task secret injection, and nested internal Linux workspace
  rehearsal calls, so `effigy dev` does not need to prompt again just because
  workspace handoff prepares a Linux Effigy artifact or task/container secret
  resolution hits a second internal path.

## [0.6.1] - 2026-05-12

### Fixed
- **Git-tag installs with external submodules** now use absolute
  `ssh://git@github.com/...` URLs in `.gitmodules`, so `cargo install` from
  Effigy git tags can initialize external bundle, provider, and setup
  submodules instead of failing on scp-style `git@github.com:...` URLs.

## [0.6.0] - 2026-05-12

### Breaking
- **Built-in shipped bundle catalog removed:** `effigy bundle` now supports only
  active-source `inspect` and `sync`, `[bundle].base` must use typed
  `path|git|oci` sources, legacy string `base = "name"` and `[bundle].name`
  are removed, and first-party starter/config/help surfaces now assume
  self-owned or remote bundle repos instead of compiled-in bundle presets.
- **Helper-style root built-ins** have been tightened into clearer nested
  homes: use **`effigy tasks migrate`**, **`effigy tasks unlock`**,
  **`effigy tasks cache`**, and **`effigy config completion`** instead of the
  old root forms. The legacy **`effigy catalogs`** alias is gone; use
  **`effigy tasks`** for catalog and task discovery.
- **Container crate sprawl** has been collapsed into **`effigy-containers`**:
  the old **`effigy-container-manager`** and **`effigy-container-ops`** crates
  are gone, and backend selection plus typed container operation planning now
  live in one canonical container-domain crate.
- **Bundle base config** no longer accepts **`[bundle].base_path`**. Local
  bundle directories must now use **`base = { type = "path", dir = "..." }`**,
  and the `[bundle].base` surface is widened for later git/OCI source forms.
- **Docs check commands** are now consolidated under **`effigy docs check
  <KIND>`**. Old flat forms such as **`docs check-links`** and
  **`docs check-paths`** are removed and fail with migration guidance.

### Added
- Added `[manifest].root = true` so nested Effigy manifests can opt out of
  parent workspace root promotion.
- Added table-form `[deploy.<env>.provider]` config for external
  deploy-provider packages, with `adapter` selecting the package.
- Added an HTTPS `s3.<host>` route and MinIO bucket/CORS bootstrap helper to
  the Underlay starter bundle, with generated S3-style local blob environment
  values for API workspaces.
- Added state-stack parser support for `role = "media-library"` and
  `artifact_kind = "object-store"` so media/object-store bundles can be modeled
  as first-class state layers before generic object-store apply lands.
- **Git bundle sources:** `[bundle].base = { type = "git", ... }` now resolves
  local or remote git-backed bundle sources into the shared Effigy bundle cache
  and records a commit-sha version hint on the materialized bundle root.
- **OCI bundle sources:** `[bundle].base = { type = "oci", ... }` now resolves
  registry-backed bundle sources through the shared artifact/ORAS transport,
  materializes them into the shared Effigy bundle cache, records digest-backed
  version hints, and marks cached bundle roots stale when the remote digest
  drifts.
- **`effigy bundle sync`:** the bundle surface now adds an explicit repo-local
  refresh command for git and OCI bundle sources, reports whether the cached
  bundle root changed, and returns `effigy.bundle.sync.v1` in JSON mode.
- **`effigy bundle inspect`:** bare `effigy bundle inspect` now reports the
  active repo bundle source, including source type, local path, version hint,
  stale state, and manifest path.
- **Decodelabs bundle git hosting:** the `decodelabs` bundle (and
  `decodelabs-library`) now works when loaded from a git bundle source. The
  `host_dir_name` derivation and `zest_port` validation that were previously
  shipped-only now run for all bundle source types, and the PHP extensions list
  is baked into the template so no shipped-only placeholder replacement is
  required.
- **Task status read surface:** `effigy tasks status <selector>` now resolves
  one task selector through normal routing and reports live-or-last-known task
  status in text or JSON using the persisted active/latest task-status record
  model, and `effigy tasks status --all` now inventories the current repo plus
  descendants, including declared unknown rows and stale no-longer-declared
  rows.
- **Deployment transaction system:** `effigy deploy` now has
  `plan/apply/status/history/redeploy` surfaces for provider-neutral
  UAT/production deployment transactions across code refs, state stacks, OCI
  artifact policy, release evidence, provider reports, hooks, health checks,
  durable report history, and evidence-backed redeploy. Railway and Render are
  supported through the shared deployment transaction report boundary, while
  provider setup creation, secret creation, release execution, and
  database/media rollback remain out of scope.
- **Render deployment transaction planning:** Render provider configs now get
  explicit transaction preflight checks for adapter boundary, required variable
  names, and domains, and unknown deploy providers block at plan time.
- **Deploy provider packages:** `[deploy.providers.<name>]` can now resolve
  path and git provider packages with `provider.toml` descriptors during
  deploy planning, validate declared Rhai capability scripts, and block unsafe
  provider package policies before any live provider mutation is attempted.
  Provider package `preflight.rhai` scripts now run during `deploy plan` through
  `deploy::provider_context()` and `deploy::provider_report(...)`, and their
  reported checks, warnings, files, and blockers are merged into provider
  preflight output. `deploy apply` and `deploy status` now dispatch to provider
  package `apply.rhai` and `status.rhai`, and deploy environments without a
  configured provider package block instead of falling back to built-in
  Railway/Render stubs.
- **External package workspace:** Effigy-adjacent action, provider, and bundle
  repos now live under `external/` as Git submodules, and repo discovery skips
  `external/` so provider/bundle manifests do not become ambient task catalogs.
- **State stack planning foundation:** `effigy state plan [<STACK>]` now
  validates `effigy.state-stack.v1` manifests and reports ordered lineage in
  text or JSON without executing app hooks. When no standalone manifest is
  supplied, it reads `[state]` from the composed Effigy manifest and supports
  positional stack selection, `state.default`, and `--stack <NAME>`.
  `--write-report` persists the plan to
  `.effigy/reports/state/<stack>/plan.json`. `effigy state apply` now adds a
  guarded apply surface that executes `apply_mode = "task"` layers and stages
  `apply_mode = "artifact"` layers only when `--yes` is supplied. SQL layers can
  now declare `target = "<data-target>"` and import through the existing
  database seed/import path after target preflight. `effigy state capture` now
  emits plan-only `effigy.state-stack.capture.v1` reports for future capture
  layers and can stage an already-produced local payload with
  `--yes --source <PATH> --ref oci://...`; adding `--push` explicitly publishes
  the staged capture artifact and reports the digest. `--task <TASK>` now runs
  one repo-owned capture task before staging and records task output or failure,
  while built-in app payload semantics remain unsupported. `effigy state history`
  now provides a read-only JSON/text lookup over existing state report files.
  Plan, apply, and capture reports now write latest pointers and timestamped
  history entries; the legacy `plan.json` write is preserved for plan reports.
  Capture tasks now receive a versioned JSON context file through
  `EFFIGY_STATE_CAPTURE_CONTEXT` and report it as `tasks[].context_path`.
  Named capture profiles under `[state.<stack>.captures.<profile>]` let
  operators run concise captures such as `effigy state capture uat new-content`
  while preserving flag overrides for one-off captures.
  Rhai tasks can now use the `state` module to read capture context through
  `state::capture_context()`, `state::capture_context_path()`,
  `state::capture_source()`, and `state::capture_destination_ref()` without
  reaching directly into environment variables.
  Task env overrides now propagate through routed workspace-container handoff,
  so state capture context reaches app-owned tasks that execute inside the
  workspace container.
- **Rhai typed surface closeout:** `.rhai` scripts now expose typed helpers for
  state orchestration (`state::plan`, `state::apply`, `state::capture`,
  `state::history`), artifacts (`artifact::inspect`, `artifact::stage`,
  `artifact::capture`), container cleanup and logical DB flows
  (`container::cache_*`, `container::volume_*`, `container::data_dump`,
  `container::data_seed`, `container::data_pull_production`), and user-global
  config mutation (`config::user_path`, `config::user_get`,
  `config::user_set`, `config::user_unset`) so first-party scripts no longer
  need `effigy::run_json(...)` escape hatches for those shipped operator
  surfaces.
- **Rhai catalog context:** `.rhai` steps now expose `catalog_root` and
  `invocation_cwd`, and file imports resolve from the selected catalog root so
  cross-catalog routed tasks can import their own local Rhai modules.
- **Rhai shell-replacement helpers:** `.rhai` scripts now expose
  `fs::make_temp_file`, `fs::list_recursive`, `fs::env_file_map`,
  `str::parse_int`, and `http::capture(...)` so repo automation can replace
  common `mktemp`, `find`, dotenv-map, integer parsing, and `curl -o/-w`
  shell script patterns.
- **Interactive completion setup:** `effigy config completion` now supports
  `--install` and `--export`, prompts for shell and action on a real TTY when
  omitted, installs user-local completion files for bash/zsh/fish, wires
  bash/zsh startup automatically when `--install` is selected, and upgrades the
  JSON surface to `effigy.completion.v2`.

### Changed
- **Repo-local bundle and changelog commands** now accept `--repo <PATH>` on
  the bounded surfaces from `g04.024`: `bundle inspect|sync` and
  `changelog validate|format|analyze|extract`. Relative changelog file paths
  and active bundle-source resolution now anchor to the selected repo root
  when `--repo` is supplied.
- **Documentation:** quick-start, command matrix, glossary, docs front door,
  and crate-level rustdoc now spell out task-runtime prefix flags
  (`--repo`, `--verbose-root`, `--env-schema`), JSON placement, scan deep-help,
  and which built-ins reject the prefix flags on the built-in invocation.
- **`deploy model` bundle source handling** now derives the active bundle name
  from the materialized bundle descriptor for path, git, and OCI bundle
  sources, so deployment planning works with remote bundle-backed repos.
- **Bundle cache location** moved from `~/.effigy/cache/bundles/` to
  `<project>/.effigy/cache/bundles/` so cached bundles are available inside
  workspace containers that mount the project root.

### Fixed
- **Submodule install verification** now uses absolute `ssh://git@github.com/...`
  URLs in `.gitmodules`, so `cargo install` from Effigy git tags can
  initialize external bundle/provider/setup submodules during release
  verification instead of failing on scp-style `git@github.com:...` URLs.
- **Global `--json` on `config completion`** now preserves the nested
  `completion` subcommand position, so commands like
  `effigy --json config completion bash --export` and
  `effigy --json config completion candidates --prefix ...` route through the
  completion built-in instead of falling back to plain `config` argument
  validation.
- **Nested Effigy repos without Cargo/npm workspace markers** now promote to a
  parent `effigy.toml` root during repo resolution, so sibling catalog
  discovery and relative task prefixes keep working inside repos that use
  parent/child Effigy manifests without an extra package-manager workspace.
- **Standalone nested Effigy repos** no longer get incorrectly promoted back
  to a parent `effigy.toml` root just because they live under another Effigy
  repo, so `--repo child` and in-repo commands keep using the child repo's
  own docs policy, tasks, and manifest authority unless the child manifest is
  explicitly acting as a catalog/workspace child.
- **Git bundle refresh visibility** now prints a short `[bundle] ...` status
  line on real TTY runs when Effigy clones, refreshes, or updates a git-backed
  bundle cache during manifest load, so operators can see when a newer bundle
  revision is being pulled.
- **Git bundle loads** now refresh stale cached clones during manifest load
  when the remote ref has advanced, so repos do not get stranded on outdated
  bundle templates until someone runs `effigy bundle sync` by hand.
- **Git bundle stale detection** now compares the cached local `HEAD` to
  `git ls-remote` for the configured ref, so `bundle inspect` correctly reports
  when a git-backed bundle source has drifted, and `bundle sync` no longer
  unconditionally fetches on every manifest load.
- **Bundle-owned route labels** now render through a generic template helper
  and generic optional-string input normalization, so external bundle repos can
  add or rename route labels like Underlay `routes.s3` without adding new
  Rust-side `*_route_domain` wiring inside Effigy.
- **Colima/containerd runtime row recovery** now repairs transient
  `error retrieving current runtime: empty value` state-loss failures on the
  runtime `ps` path used by gateway registration and container discovery, so
  detached bring-up and managed handoff no longer fail after the runtime has
  already been started.
- **State capture standalone manifest parsing** now treats a positional
  `*.toml` argument the same way across `state plan`, `state apply`, and
  `state capture`, so JSON contract and operator flows like
  `effigy state capture path/to/state-stack.toml ...` no longer misroute the
  manifest path into the stack selector. Ambiguous manifest-stack errors also
  again point users at `--stack <NAME>`.
- **Scoped runtime backend persistence** now stores container backend overrides
  per container policy instead of one repo-wide backend value, so helper
  containers like Effigy's `linux-release` rehearsal container no longer drift
  onto stale Docker metadata from unrelated repo-local runtime sessions.

## [0.5.0] - 2026-05-08

### Breaking
- **Container machine-scope flags** now use **`--global`** instead of
  **`--all`** on container status, stats, down, cache, and volume surfaces.
- **Volume cleanup semantics** are now split by scope:
  repo inventory/prune uses **`--dormant`**, while machine-wide ownerless
  cleanup uses **`--global --orphans`**.

### Added
- **First-class Docker/Desktop backend control:** `effigy config` now manages
  machine-local container defaults with **`config path|get|set|unset`**,
  `~/.effigy/config.toml` accepts `[containers] backend/profile`, and
  **`effigy bootstrap --backend containerd|docker`** adds a one-shot override
  with real-TTY backend prompting when both Docker and Colima are available.
- **Container cleanup surfaces:** **`effigy container cache list/prune`**
  inventories and removes purge-safe build caches such as Rust **`target`**,
  **`node_modules`**, **`pnpm-store`**, and shared Cargo caches, while
  **`effigy container volume list/prune`** adds repo-scoped **`--dormant`**
  cleanup for superseded named volumes and machine-scoped
  **`--global --orphans`** cleanup for ownerless ones.
- **`effigy container data dump --push`** publishes explicit **`oci://`** dump
  destinations after the local SQL dump is staged, using the artifact capture
  push path and returning the pushed digest in JSON reports.
- **Logical database targets:** optional **`[data.targets.<name>]`** entries let
  bootstrap DB seed, **`effigy container data seed`**, and
  **`effigy container data dump`** address sidecar databases without overloading
  **`[bundle].databases`**.

### Changed
- **Container backend selection** is now stable across Docker Desktop and
  Colima: repo-bound operations keep their declared runtime, repo-local runtime
  choices persist after bootstrap, unscoped runtime/cache flows honor the new
  machine preference, and **`effigy doctor`** / **`effigy container status`**
  now report the selected backend more clearly.
- **Decodelabs bundles** now default their PHP-FPM workspace service to
  **Node.js 24** instead of **20**, so current pnpm and newer Node built-ins
  like **`node:sqlite`** work out of the box in bundle-based sites while
  staying on the current LTS line.

### Fixed
- **Release prepare gate reruns** now build the current Effigy binary and run
  it directly for self-hosted `qa`, `smoke`, and metadata checks, avoiding the
  nested `cargo run --bin effigy` wrapper stalls that could leave
  `prepare --yes --check-gates` half-applied without writing prepared release
  state.
- **OCI artifact failures** now add operator-facing remediation for common
  `oras` failure classes such as missing auth, denied push access, malformed
  refs, and registry reachability, so inspect/pull/push errors say what to do
  next instead of only echoing raw registry stderr.
- **Bundled nginx healthchecks** now treat any HTTP response as runtime-ready,
  so Decodelabs-style PHP stacks no longer hang bootstrap just because `/`
  returns a bootstrap-time **`404`**.
- **Container volume ownership and cleanup** are now much more accurate:
  path-sensitive isolated volumes get fresh names when their container mount
  path changes, generated named volumes carry explicit ownership labels,
  Redis now uses an explicit named `/data` volume, cache-like mounts are marked
  ephemeral correctly, repo-scoped dormant detection catches stale legacy
  `efv-*` generations, and cache/volume prune reports now include reclaimed and
  skipped byte totals.
- **Shipped PHP workspaces** now install a sendmail-compatible `msmtp` shim and
  point PHP `mail()` at the local Mailpit SMTP service by default, so legacy
  mail code is captured in development instead of disappearing into an
  unconfigured container mail path.
- **Shipped PHP workspaces** now keep pnpm's content-addressable store on a
  dedicated named volume at **`/home/dev/.local/share/pnpm/store`** instead of
  under the repo bind mount, so local projects stop accumulating
  **`.pnpm-store`** and repo-local store state. The cache inventory/prune
  surface now reports that volume as **`pnpm-store`**.

## [0.4.0] - 2026-05-06

### Breaking
- **Rhai script steps:** host helpers are grouped into namespaces (`scan::…`,
  `fs::…`, `deploy::…`, and so on). Only `log`, `log_warn`, and `env` stay at the
  top level. Remove any reliance on the old `module.func(...)` rewrite; use
  `module::func(...)` (or the real module path) instead.

### Added
- **`effigy bootstrap`:** repeatable **`--db-seed`** (`<file>` or
  `<database>=<file>`). Single-database bundles can omit the target; multi-database
  bundles must name each target. Staged dumps are available to your bootstrap
  tasks via the documented env vars (see bootstrap guide).
- **`effigy bootstrap`:** on a real TTY, optional prompts for missing DB seed
  paths and before reusing a non-empty clone destination. No prompts for
  **`--json`**, **`--plan`**, non-interactive I/O, or **`--no-prompt`**.
- **`effigy bootstrap --fresh`** isolates generated-compose runtime state with
  a session-scoped project-name suffix, and **`effigy bootstrap teardown`**
  tears that recorded fresh session back down afterward.
- **`effigy container data pull-production`**, **`data import`**, and broad
  **`effigy unlock`** use the same confirmation pattern: TTY asks (default no);
  JSON or non-interactive runs need **`--yes`** where applicable or they fail
  clearly.
- **`effigy container data seed`** matches the bootstrap DB-seed contract after
  bring-up (repeatable **`--db-seed`**, optional TTY prompts, staging under
  **`.effigy/local/db-seeds/`**, then **`bootstrap:db-seed`**).
- **`effigy container data dump`** exports logical SQL dumps from generated-compose
  Postgres or MariaDB services using repeatable **`--db-dump <FILE>|<TARGET>=<FILE>`**
  inputs. Single-database bundles can omit the target; multi-database bundles
  must name one, mirroring the `data seed` target contract.
- **`effigy bootstrap children sync`** refreshes the composed
  **`bootstrap.children`** checkouts for the active repo, with safe
  fast-forward defaults plus **`--fetch-only`** and **`--checkout`** modes.
- **`effigy bootstrap children status`** reports the composed child checkout
  state without network calls.
- **`effigy bootstrap`** now has explicit **`--reuse-path`** for reusing a
  non-empty destination without an interactive confirmation.
- **`effigy init`** with the **`minimal`** starter (the default) now also emits a
  root **`README.md`** next to **`effigy.toml`**: first commands, tasks vs
  built-ins, and stable links into the Effigy docs on GitHub.

### Changed
- **`effigy init`** leaves an existing root **`README.md`** untouched by default
  (JSON: per-file **`skipped: true`** on that entry); use **`--force`** to replace
  it like any other starter target. Documented in **`019`**, **`021`**, **`025`**,
  **`effigy init --help`**, and the **`minimal`** starter README.
- **Shipped catalog Postgres/MariaDB** services use **named volumes** for DB
  data. **`effigy container reset`** keeps that data unless you pass
  **`--wipe-data`**.
- **`effigy bootstrap`** picks the default clone directory from **`[catalog].alias`**
  when the repo defines it and you did not pass **`--path`**.
- **`effigy release verify-install`** installs with **`cargo install --locked`**
  so release verification matches the locked dependency graph.
- **`effigy release verify-install`** now checks the installed binary's reported
  version and command output contract instead of trusting zero exit codes alone.
- **`effigy container status`** and **`effigy container down`** can discover Effigy
  repos under the current directory when you are not at a repo root, so you
  often do not need **`--global`** for subtree checks.
- **`effigy container data seed`** is documented and implemented only for the
  repo default container on the **generated-compose** path.
- **DecodeLabs starter bundle:** default **`[bootstrap].start`** is **`dev`**;
  bootstrap may copy **`infra/dev/bootstrap/app.env`** to **`.env`** when that
  template exists (otherwise it logs a skip).
- Fewer unused dependencies on the main **`effigy`** crate (no expected impact
  for normal installs).
- **First-read docs:** root **`README`**, **`docs/README`**, **`docs/guides/README`**,
  **`021`**, and **`055`** now spell out manifest-driven behavior, **`dev`** as a
  normal task name, built-in **`test`** vs overrides, contributor-only **`qa:`**
  tasks, and the split between the short docs front door and the full guides map.
- **Agent surfaces:** **`AGENTS.md`**, **`skills/README.md`**, **`skills/effigy/`**
  (including **`references/workflow-shortcuts.md`** link fixes to **`063`**/**`064`**
  guides), and **`references/config-shapes.md`** carry the same mental model for
  cross-repo use.
- **Starters:** **`underlay`** and **`northstar`** starter readmes and
  **`northstar/AGENTS.md`** clarify selectors vs built-ins;
  **`crates/effigy-catalog/catalog/README.md`** points consumers at **`067`**.

### Fixed
- **Docs and bundled agent skill:** bootstrap default **`[bootstrap].start`**
  behavior matches the CLI; command matrix includes **`effigy defer`**; skill uses
  **`effigy doctor <selector> <args...>`** for routing explain.
- **`effigy bootstrap`** no longer treats **`--no-prompt`** as implicit approval
  to reuse a non-empty destination; use **`--reuse-path`** for that explicit
  destructive choice.
- **`effigy qa`** exits cleanly with a clear message when **`cargo`** is not on
  **`PATH`** (instead of panicking).
- **Fresh clones** of this repository are much smaller after a one-time history
  cleanup.
- **Install examples** in docs now reference **`v0.3.3`** where they were stale.
- **DecodeLabs / DB seeding:** MySQL imports run through the container (no host
  **`mysql`** client required); seeding is more reliable on **Colima** (stdin from
  files, working directory, staged dumps, **`PATH`** for host tools).
- **DecodeLabs `bootstrap:db-seed`:** MySQL import no longer nests container exec
  in a way that broke from inside the workspace container.
- **Linux** installs that ship **`effigy.active-version`** next to the binary keep
  the full local version string visible inside dev containers.
- **Generated compose:** when readiness recovery restarts the **primary** service,
  **dependent** services in the same project are refreshed so they do not keep a
  stale upstream address (avoids **502**-class failures on multi-service stacks).

## [0.3.3] - 2026-05-03

### Fixed
- DecodeLabs deferral CI fixtures now use realistic published-port metadata
  and fully sandbox gateway/TLS state. The macOS and Linux release gates no
  longer fail on fake runtime rows, ambient `HOME` gateway state, or trusted
  host `mkcert` discovery leaking into the test harness.

## [0.3.2] - 2026-05-03

### Added
- `effigy deploy export railway` now generates the first bounded Railway
  deployment bundle for Underlay repos: service-local `railway.toml` files
  plus a machine-facing `report.json` that leaves domains, Postgres creation,
  and secret wiring explicit instead of guessed.
- The `decodelabs` bundle now includes Mailpit as a first-class service and
  default route. DecodeLabs stacks now publish `mailpit.<host>` out of the box,
  and apps inside the stack can send local SMTP traffic to `mail:1025`.
- Rhai scripts now have `run_process_tee(...)` for commands that should stream
  live output and still return captured `stdout` / `stderr` to the script.
- Rhai subprocess helpers now share optional `cwd` and `env` overrides across
  `run_process(...)`, `run_process_stream(...)`, and `run_process_tee(...)`,
  so scripts can change working directory or inject scoped environment without
  dropping to shell wrappers.
- Rhai subprocess helpers now also accept `stdin_file`, so scripts can feed
  file-backed stdin to buffered, streaming, or teeing commands without
  shell redirection. The shipped DecodeLabs seed helper now uses that path
  instead of `sh -lc 'mysql ... < dump.sql'`.

### Added
- `[bootstrap].start` now accepts an array of selectors in addition to the
  original single-string form, so `effigy bootstrap --start` can run a
  short chain (e.g. `start = ["container:up", "dev"]`) without needing a
  dedicated aggregator task. Selectors run sequentially in declaration
  order and the first failure aborts the chain. The `effigy.bootstrap.v1`
  envelope now carries both `start.task` (first selector, for back-compat)
  and `start.tasks` (full array). Array entries can be bare selector
  strings or table form (`{ task = "..." }`) — mixed arrays allowed,
  mirroring the shape of `[bootstrap].run`.
- `effigy deploy model --json` now has a first Underlay-only foundation.
  It emits the new `deploy.model.v1` envelope from the effective bundle-backed
  manifest, deriving `front`, `admin`, `api`, optional `jobs`, primary
  Postgres, routed domains, secret references, and promotion warnings without
  depending on runtime state or provider-specific export logic yet.
- `effigy deploy export render` now exists as the first provider adapter
  foundation. It consumes the Underlay-derived deployment model, writes a
  first `render.yaml`, supports `--plan`, and maps static sites, Rust web and
  worker services, managed Postgres, and operator-owned secrets onto the
  bounded Render Blueprint surface.
- The first Underlay deploy-model derivation now promotes three more pieces of
  production metadata directly into `deploy.model.v1`: static-service output
  directories, static fallback files for SPA rewrites, the shared API health
  probe (`/v1/health`), and `db:migrate` as the release hook when the API
  package exposes it.
- The Rhai host API now exposes a fuller low-level automation layer for
  file-oriented scripts: direct `copy_file`, `move_path`, `read_lines`,
  `is_dir`, string predicates/transforms such as `string_contains` and
  `replace_string`, plus `http_download` for writing HTTP responses
  straight to disk without round-tripping large files through script
  memory.
- Rhai file-oriented bootstrap helpers now also include `copy_if_missing`
  and `replace_in_file`, so common template-copy plus local-substitution
  flows no longer need manual `path_exists` guards or full read/modify/write
  plumbing in script code.
- Rhai now exposes envfile-aware helpers `env_file_get` and `env_file_set`
  for common `.env` mutation flows, so bootstrap scripts that mean “set this
  key” no longer have to treat dotenv files as unstructured text.
- The envfile Rhai surface now also includes `env_file_entries` and
  `env_file_remove`, rounding it out for simple inspect/set/remove flows
  without forcing scripts back into raw text handling.

### Fixed
- `effigy exec` container-surface resolution now uses typed `RunnerError`
  families for missing `[containers]`, missing default workspace targets,
  missing named containers, not-running container operators, and one
  container-policy translation seam. Those failures no longer flatten into
  generic `task_invocation` strings before they reach the runner surface.
- Public workspace shell handoff and host-container lease translation now use
  typed `RunnerError` families for combined shell-plus-cleanup failure
  reporting and lease encode/reaper bootstrap failures instead of flattening
  those session and lease seams into generic `task_invocation` strings.
- Gateway reconciliation now uses typed `RunnerError` families for route-table
  load/save, route register/deregister, and the first route-shape validation
  seams in `gateway_registration.rs` instead of flattening those failures into
  generic `task_invocation` strings.
- Gateway reconciliation closeout now covers loopback registry load/save/allocation,
  runtime-row discovery, service-alias lookup, raw port-binding translation,
  and remaining route-target validation in `gateway_registration.rs`. The
  runtime/container gateway path no longer relies on generic
  `task_invocation` strings as its dominant failure shape.
- Container runtime prep now uses typed `RunnerError` families for
  policy-validation and exec-readiness failures instead of collapsing those
  high-signal seams into generic `task_invocation` strings. The runtime core
  now preserves failure category while keeping the operator-facing messages
  intact.
- Public workspace entry and bootstrap start handoff now delegate through a
  shared workspace-session orchestrator instead of keeping the full session
  lifecycle inline in `system_command/workspace.rs`. The public shell
  ownership and cleanup boundary now has one explicit owner.
- Generated compose assembly now has a first typed internal policy layer in
  `effigy-containers` for shared-service env injection and generated port
  publication. Those main generated-compose seams no longer each reparse the
  compose YAML string as their working data model before writeout.
- Bootstrap setup work, public workspace handoff, seeded shells, routed
  container activation, deferred container activation, and `effigy exec` now
  share a typed runtime/session context for lease refresh and bootstrap
  stop-on-exit behavior. The runtime core no longer depends on bootstrap-only
  ambient env flags as the primary control path for those ownership seams.
- `effigy bootstrap` root-run and task phases no longer create temporary
  host-container leases for container-backed setup work. Bootstrap can now
  hand off into a DecodeLabs `dev` shell without an earlier five-minute
  lease reaper killing the interactive session underneath it.
- `effigy bootstrap --start` now treats the final public workspace handoff as
  session-owned even when earlier bootstrap setup already started and readied
  the container. Exiting the handed-off DecodeLabs `dev` shell once again
  stops the stack instead of preserving it as an adopted runtime.
- Bootstrap-start cleanup overrides now flow explicitly through both the
  public workspace handoff and the interactive seeded-task shell paths. That
  closes the split where DecodeLabs bootstrap shells stopped the runtime on
  exit but Underlay/TUI bootstrap shells still preserved it.
- `effigy gateway up` and managed gateway auto-start now compare the running
  daemon's recorded build identity against the current Effigy binary. Rebuilt
  local binaries now restart stale gateway daemons instead of silently
  reusing an older process after upgrade.
- `effigy bootstrap:local` no longer kills the running installed binary when
  invoked through the local wrapper. The local build script now stages the new
  binary into `effigy.new` and atomically renames it into place instead of
  copying over `.local-install/bin/effigy` in place.
- Local `bootstrap:local` installs now write a sibling active-version stamp
  for the installed binary, and the CLI header, `effigy version`, and TUI
  version surfaces now prefer that stamp when present. Local installs can
  now show distinct build identities such as
  `v0.3.1+local.abc123` instead of always rendering the last
  released semver with no visible distinction between builds.
- Bootstrap task dispatch, Rhai `run_effigy_command(...)`, and run-array
  builtin replay now share one embedded-runner foundation for nested command
  parsing, repo targeting, JSON propagation, and dispatch instead of keeping
  separate local replay paths.
- Interactive workspace shells and seeded task shells now share one ownership
  classifier for adopted versus session-owned runtimes. Direct
  `effigy workspace` entry and overlapping `stay_in_shell` / managed seeded
  shell paths now derive cleanup from the same readiness and cleanup policy
  model instead of separate local booleans.
- Inline workspace container capability checks now go through one shared
  binding-layer helper across standard task routing, managed attached
  session fallback, and `effigy workspace` / `effigy system`. Unsupported
  surfaces now fail through one explicit message family instead of keeping
  separate caller-local rejection branches.
- `effigy exec` and exec aliases now use the shared non-shell runtime
  activation contract. Stopped container runtimes now get the same startup,
  exec-readiness, gateway/route reconciliation, and temporary host-container
  lease behavior as container-backed task execution instead of acting like a
  separate already-running-only path.
- Nested Effigy command re-entry now uses one shared embedded repo-targeting
  helper instead of separate per-surface allowlists. Run-array builtin
  dispatch still force-pins nested builtins to the parent repo, while Rhai
  `run_effigy_command(...)` now uses the same shared helper in
  default-if-missing mode so explicit nested `--repo` still wins. This
  also widens the default nested coverage to `system`, `workspace`, and
  `defer`, removing another common-path split where embedded commands
  behaved differently depending on whether they came from run-array or
  Rhai.
- Public workspace handoff now reconciles container gateway routes before
  entering the shell, and treats an adopted-but-route-incomplete stack as
  session-owned for shutdown. This fixes `effigy bootstrap` landing in a
  shell with a running DecodeLabs stack but missing `*.legacy.test` route
  registration, and restores the expected "stop on shell exit" behavior
  for that path without changing healthy already-running `effigy dev`
  sessions.
- Non-shell container task activation now uses one shared runtime-prep
  contract across explicit `run_in = "container"` tasks and deferred
  container requests. Auto-started runtimes now get the same temporary
  host-container lease and gateway/route reconciliation either way,
  instead of keeping deferred DecodeLabs/DecodeLabs-library tasks warm
  for five minutes while explicit container tasks used a separate
  one-shot startup path.
- Workspace-seeded container tasks now share one handoff implementation
  across managed and standard execution, so `stay_in_shell` semantics no
  longer drift based on whether a task ran under the TUI.
- Standard container bring-up now repairs primary-service TCP alias host
  entries before workspace handoff and routed exec. That restores
  reliable in-container resolution for aliases such as
  `mysql.<site>.legacy.test` during `effigy bootstrap` and other
  non-managed flows that previously skipped the alias hydration path.
- `CwdMapper::host_to_container` no longer emits a trailing path
  separator when the host CWD equals the repo root. `PathBuf::join("")`
  was producing `/var/www/html/`, which nerdctl/runc rejects as
  `current working directory is outside of container mount namespace
  root` because the literal `-w` argument has to match the container's
  WORKDIR exactly. The mapper now returns the container working dir
  unchanged when the relative path from repo root is empty, and a
  regression test covers the case.
- Container exec under Colima/nerdctl now recovers from the persistent
  "current working directory is outside of container mount namespace root"
  failure that runc surfaces after a transitional `compose up
  --force-recreate` cycle. The auto-up path now probes `-w <working_dir>`
  exec readiness (matching the condition real exec uses, not a generic
  `true` exec) and, on failure, runs `compose restart <primary_service>`
  once and re-probes. The same recovery also runs when entering a routed
  task exec while the container is reported running, so a broken
  mount-namespace state inherited from a previous run is repaired before
  dispatch instead of failing the user's task. This unblocks `effigy
  bootstrap` of consumer repos whose `[bootstrap].run` chains include
  `run_in = "container"` tasks (e.g. `seed` running mysql commands).
- `effigy` now pre-creates host bind-mount directories declared in the
  generated compose before `compose up` runs. `nerdctl-compose` (unlike
  `docker-compose`) does not auto-create missing host paths, so catalog
  fragments such as `mariadb` that declare `<repo>/.effigy/runtime/data/db/mysql:/var/lib/mysql`
  previously caused runc to abort with `failed to fulfil mount request:
  open <path>: no such file or directory` when the project state directory
  had been cleaned. We now walk the generated compose YAML, find bind
  mounts under `repo_root`, and `mkdir -p` the host side. File-typed
  mounts (`*.conf`, `*.sql`, `*.yml`, etc.) are left to the catalog writer
  that produces them. The same pre-create + idempotent `compose up -d`
  also runs on the routed-exec-ready path, so sibling services (e.g.
  `db`) that were left in `Created` state by a previous failed `compose
  up` are brought online before a routed task dispatches its exec —
  fixing `effigy bootstrap` re-runs where the primary service is
  reported running but a dependent service never started, surfacing as
  `mysql -h db` returning `Unknown server host 'db' (-2)` from inside
  the workspace container.
- Tasks that route into a container via the standard pipeline (e.g.
  `[bootstrap].run` entries declared with `run_in = "container"`, or any
  manifest-resolved task whose binding lands in a container) now auto-up
  the compose stack when it is not running, instead of failing with
  `container service \`<name>\` is not running`. The standard pipeline
  now uses `is_primary_service_running` (compose-stack check) for routing
  rather than the looser colima-profile check, and brings the stack up via
  `compose up` + `ensure_colima_running` before re-routing — mirroring the
  behaviour previously only available on the deferral path. This unblocks
  `[bootstrap]` shapes like `run = [..., { task = "seed" }] start = "dev"`
  where seed is declared `run_in = "container"`.
- Running-container discovery across the runner now walks up from the Docker
  compose `working_dir` label to find the nearest `effigy.toml` (up to six
  levels) instead of requiring the label to equal the repo root, and treats
  the absence of the label as a match (rather than a reject) when
  `project_name` and `service` already agree. This fixes generated-compose
  stacks (decodelabs and any other bundle that emits compose into
  `<repo>/.effigy/runtime/compose/`) and Colima-backed stacks (where
  `nerdctl-compose` does not emit the
  `com.docker.compose.project.working_dir` label at all). The same logic
  is now used by the routing decision that decides whether a container
  task can execute in-container, the exec transport that picks which
  compose service container to attach to, the runtime-mismatch filter,
  and the gateway registration project filter. A shared
  `working_dir_belongs_to_repo` helper in `effigy-runtime` is used
  everywhere these comparisons happen. Previously, container tasks
  dispatched via the standard pipeline (e.g. `[bootstrap].run = [{ task =
  "seed" }]` with `seed` declared `run_in = "container"`) would auto-up
  the compose stack and then immediately fall through to host execution
  because the just-started Colima stack carried no `working_dir` label,
  so `mysql` ran on the host with `command not found`. Note: `effigy
  container status --all` and `stats --all` still need the
  `working_dir` label to attribute a stack back to its source repo, so
  Colima-backed stacks remain absent from those views; that gap is
  separate from per-repo container task execution.
- `effigy bootstrap` start tasks that open a workspace shell (decodelabs-style
  `[tasks.dev]` with no `run`, only a workspace binding) now use the cloned
  repo as the resolved root instead of re-resolving from the parent invocation
  cwd. Previously, running `effigy bootstrap <repo>` from a parent directory
  could fail with `could not resolve a project root from cwd <parent>` even
  though the clone destination was a valid project root.

## [0.3.1] - 2026-04-29

### Added
- Cross-repo agent skill at `skills/effigy/` teaching AI coding assistants
  (Claude Code, OpenAI Codex, Cursor, etc.) how to discover tasks, run
  common workflows, parse `--json` envelopes, and avoid release/CI footguns
  in any repo that uses Effigy. Installs via
  `npx skills add inflatable-cookie/effigy` and follows the open
  [Agent Skills](https://agentskills.io/specification) standard.
- `php-fpm` now accepts explicit `host_ports`, and the shipped `decodelabs`
  bundle now supports optional `zest_port` / `zest_domain` inputs so legacy
  DecodeLabs sites can route a temporary Zest/Vite dev server through the
  local gateway instead of leaving it stranded on an internal container port.

### Changed
- The shipped `decodelabs` bundle now derives its container working directory
  from the first label of `[bundle].host`, so DecodeLabs repos mount at paths
  like `/var/www/cbs` or `/var/www/contact-patch` instead of the generic
  `/var/www/html`. The bundled seed helper now follows the repo-root-relative
  dump path instead of hardcoding the old container root.
- JSON command envelopes plus help/version payloads now expose shared binary
  metadata, including the active local build version when present.
- Shipped DecodeLabs, DecodeLabs-library, and Underlay bundles now use a single
  canonical `export.toml` template source for both bundled defaults and
  exported local bundle generation.
- PHP workspaces now share one Effigy-managed Composer-home volume by default
  instead of only sharing auth/config files, so Composer tokens and global
  state persist across repos unless the host Composer home is mounted
  explicitly.
- Workspace SSH integration is simpler and more explicit now: host
  `~/.ssh/config` is no longer mounted or rewritten by default, container-safe
  config mounting is opt-in via `ssh_config_path`, and trusted local-dev cases
  can mount a full read-only SSH directory via `mount_host_ssh_dir` /
  `ssh_dir_path`.
- The docs front doors and core guides were heavily tightened for new users:
  onboarding, install, local-dev, release, and catalog-authoring paths are now
  clearer, less repetitive, and less maintainers-only in tone.

### Fixed
- Task catalog discovery now skips `vendor/` as well as `node_modules/`,
  `target/`, and `.effigy/`, avoiding slow scans and accidental task discovery
  from dependency and build trees.
- `php-fpm` catalog builds no longer fetch `install-php-extensions` from a live GitHub release URL during image build, avoiding transient 5xx failures on fresh PHP workspace and `decodelabs-library` builds.

## [0.3.0] - 2026-04-28

### Breaking
- `concurrent = [...]` now requires `mode = "tui"`. Sidecars that should follow container lifecycle now belong on `[[containers.<name>.host_processes]]`.
- Legacy root deferral is no longer inferred from `composer.json` or `effigy.json`. Repos that relied on it now need an explicit `[defer]` block or a shipped bundle that provides one.
- The workspace/container config contract has moved: default workspace settings now live under `[systems.<name>]`, managed task settings live directly on `[tasks.<name>]`, generated compose output lives under `.effigy/runtime/compose`, and task-level `host = true` is replaced by `run_in = "host" | "container" | "either"`.
- Compatibility-only release wrapper scripts are gone. Call the native Effigy release surfaces directly instead of relying on the old shell entrypoints.

### Added
- A full container and system surface: `effigy container ...`, `effigy system ...`, `effigy workspace`, generated compose ownership, data lifecycle commands, cross-project `status --global` / `stats --global`, shared backing services, and bounded reset/eject/import/export flows.
- A native local gateway surface: `effigy gateway up/down/status/setup-tls`, HTTP reverse proxying, DNS routing, TCP aliases, auto port allocation, TLS management, and support for non-`.test` local domains via macOS `/etc/resolver` integration.
- A shipped service catalog with extract/list tooling plus new built-in fragments such as `workspace-rust-bun`, `php-fpm`, `phpmyadmin`, `dbgate`, `mailpit`, and `minio`.
- A first-class demo system: manifest-owned `[demos.*]`, `effigy demo list/inspect/run/stop/rerun/history`, receipts and artifact tracking, and the interactive `effigy demo browser`.
- A native distribution and release-support surface: `effigy release preflight`, `validate`, `check-glibc-floor`, `first-publish`, `validate-artifacts`, `generate-closeout`, and `write-summary`.
- File-backed Rhai task steps plus a substantially wider host API, allowing repo automation to use typed Effigy helpers instead of shell-script glue.
- Structured host mounts with `external = true`, `${VAR}` / `~` expansion, target-host DNS routes, host-side companion processes, and `run_in = "host"` concurrent entries for sidecars such as SSH tunnels.
- `effigy.local.toml` auto-discovery, optional includes, fragment-owned `[manifest].extend`, and `effigy bundle export ...` for local bundle customization without forking shipped bundles.
- A shipped `decodelabs-library` bundle, a shipped `decodelabs` `seed` task, and broader underlay bundle defaults for bootstrap, root `dev`, `health`, `validate`, and `qa`.
- Repo-owned `qa:ci:fast` and `qa:ci:local` tasks that reproduce the current GitHub Actions CI and JSON-contract lanes locally before push.

### Changed
- Existing local manifest composition now uses fragment-owned `[manifest].extend`; the old include-side `extend = [...]` form is gone.
- Existing workspace-backed local dev now stages generated compose and other runtime state under `.effigy/runtime`, with clearer ownership boundaries between repo source, generated runtime artifacts, and container-local writable state.
- Existing underlay consumers now resolve sibling task catalogs from declared mounts, so selectors like `underlay/*` no longer depend on symlinked sibling repos.
- The pre-release bundle/config contract was tightened before the first `v0.3` cut: shipped `decodelabs` and `underlay` bundles now take `databases = ["app"]` as the single database input shape, and `workspace-rust-bun` now uses one `isolated_dirs` list instead of split `cargo_target_dirs` / `node_modules_dirs` knobs.

### Fixed
- Generated compose stacks now honor declared host mounts on catalog-backed repo-root services, and direct-compose workspace runtimes now auto-adopt mounted sibling isolation contracts without requiring duplicate config.
- Workspace and deferred container execution now handle isolated writable dirs correctly, fixing permission failures, accidental cross-repo `vendor` reuse, read-only bind edge cases, and a broad class of handoff/startup regressions.
- The gateway/runtime path is more resilient: stale loopback assignments are reclaimed, unreachable host runtimes no longer crash registration, health waits now probe real route readiness, and route registration rejects invalid or conflicting upstreams more honestly.
- Gateway elevation and TLS helper lookup are now tighter: elevated runs no longer inherit a caller-controlled `PATH`, and mkcert lookup is resolved through an explicit or bounded trusted path instead of ambient shell state.
- Gateway-managed DNS suffixes are now validated before resolver-file writes, and manifest DNS route domains reject invalid label shapes instead of letting unsafe path-like values drift into runtime routing.
- Env-schema `exec('...')` resolution now drains stderr safely, fixing a false-timeout / apparent hang class when commands produce large stderr output.
- Task catalog discovery now skips `.effigy` runtime artifacts instead of walking generated runtime trees during selector resolution.
- Doctor and catalog discovery now correctly accept `[bundle]`, skip shipped starter assets, avoid treating starter templates as live project catalogs, and surface strict-parse failures as findings instead of crashing out of the sweep.
- DecodeLabs PHP workspaces now correctly use the shared Composer cache mount, and the shipped `php-fpm` image now carries the broader extension set needed by current DecodeLabs consumers.
- The built-in release flow now resolves workspace-inherited Cargo versions correctly, so self-hosted `release status` and `release simulate` work against repos using `workspace.package.version` plus `version.workspace = true`.
- Container/dev regressions were fixed across SSH agent forwarding, mkcert trust propagation, git safe-directory handling, bootstrap start behavior, VT/TUI rendering, bundle default drift, and manifest-driven MariaDB/Postgres database selection.

## [0.2.13] - 2026-04-13

### Added
- Demo registry entries now accept task-style `run` sequences directly under
  `[demos.*]`, so repos can inline small proof chains without separate
  `demo:*` wrapper tasks.

### Added
- Make `effigy demo browser` launch browser-owned live attached terminal
  sessions for browser-launched run-backed interactive demos, so the `Terminal`
  tab can host the actual running demo with direct input while runner-owned
  logs, receipts, and history still populate behind it
- Extend browser-owned live attached terminal sessions to browser-launched
  single-process concurrent-runner-backed interactive demos, while keeping
  multi-process concurrent demos on the existing projected terminal/session
  path instead of launching nested TUIs
- Add general manifest composition through `[manifest].include`, including
  nested partial-fragment loading, path-scoped override enforcement, and
  deterministic conflict failures so features like `tasks`, `docs_policy`, and
  `release` can share one config-splitting model instead of growing
  feature-local file-loading semantics
- Add `effigy config --inspect` so operators can inspect the effective composed
  manifest, include graph, evaluation order, overridden paths, effective value
  sources, and rendered merged TOML in both text and JSON mode
- Add focused manifest inspection via `effigy config --inspect --path <dotted.path>`
  so operators can inspect one effective value, its source file, and any
  matching override history without scanning the full manifest dump
- Add first-class demo registry loading through `[demos.<id>]` plus `effigy demo list`
  and `effigy demo inspect <id>` so repos can expose proof/demo inventory,
  coverage, source provenance, and the latest known receipt/artifact state
  without depending on project-local script catalogs
- Add `effigy demo run <id>` so task-backed and run-backed demos can execute
  through a first-class CLI surface, write normalized latest-attempt receipts,
  and immediately refresh the state reported by `demo inspect`
- Add runner-owned demo active-attempt state plus `effigy demo stop <id>` and
  `effigy demo rerun <id>` so run-backed demos can expose honest lifecycle
  control without pretending generic task cancellation already exists
- Add focused browser-facing demo discovery via `effigy demo list` filters and
  grouping so operators and future TUI clients can browse proof inventory by
  owner, tag, mode, coverage, status, gap, and stale state without project-
  local script glue
- Add self-hosted demo proofs in the Effigy repo itself, including a
  task-backed browser proof report and a run-backed lifecycle window, so the
  shipped demo registry, receipts, artifacts, and stop/rerun lifecycle can be
  exercised against a real repo-local proof surface before browser/TUI work
  hardens around them
- Add `effigy demo browser` as the first interactive demo browser/TUI
  foundation, with grouped list/detail browsing plus bounded in-browser
  `run`, `stop`, `rerun`, and refresh actions on top of the shipped demo
  runner surface
- Add bounded artifact-opening support inside `effigy demo browser`, so
  operators can select a recorded artifact reference and open it without
  leaving the browser or depending on project-local glue
- Add bounded recent-output visibility inside `effigy demo browser`, so
  operators can inspect active or latest runner-owned stdout/stderr without
  leaving the browser or dropping into a second terminal
- Add bounded in-browser query controls to `effigy demo browser`, so operators
  can narrow proof inventory by search, owner, status, gap, and stale state
  without leaving the browser for `demo list`
- Add bounded detail-pane navigation to `effigy demo browser`, so longer
  selected-demo records with receipts, artifacts, and recent output remain
  reachable from one interactive surface
- Add metadata-query parity to `effigy demo browser`, so operators can filter
  by tag, mode, and cover and cycle through the full shipped grouping
  contract without dropping back to `demo list`
- Add bounded persisted attempt history for `effigy demo inspect <id>`, so
  repos can retain and review recent terminal demo outcomes beyond the single
  latest-attempt summary while keeping the first history slice runner-side
- Add `effigy demo history <id>` so one demo's retained terminal-attempt
  history can be queried directly, with optional `--limit <N>` trimming,
  without widening `demo list` or the browser
- Add `effigy demo history <id> --attempt <ATTEMPT_ID>` so operators can
  select one retained historical attempt and inspect its receipt, artifacts,
  and log references directly from the dedicated history surface
- Add bounded history-query controls to `effigy demo history <id>`, including
  `--outcome <OUTCOME>` filtering and `--ordinal <N>` selection, so operators
  can narrow one demo's retained results and select a displayed attempt
  without copying long attempt ids for common review flows
- Add an integrated retained-history view inside `effigy demo browser`, so
  operators can open one demo's settled `demo history` result set from the
  action menu and review retained attempts without leaving the browser
- Add a runner-owned active demo terminal/session handoff to `effigy demo inspect`,
  `demo run`, and `demo stop`, including transport metadata, bounded recent
  output snapshots, and explicit no-nested-TUI signaling for later browser
  terminal views
- Add a bounded live terminal view inside `effigy demo browser`, so operators
  can inspect one selected demo's active terminal/session metadata and a live
  tail from runner-owned logs in-place without launching nested TUIs
- Add bounded demo-scoped tabs to `effigy demo browser`, so one selected demo
  can switch between `Overview`, `History`, `Terminal`, and `Artifacts`
  without leaving the detail surface
- Add `effigy demo input <DEMO_ID> --text <TEXT> [--append-newline]` plus a
  bounded active-terminal input-forwarding contract shape, so later browser
  terminal work can target one demo-scoped forwarding surface without
  inventing client-side transport semantics
- Add embedded terminal emulation to `effigy demo browser`, plus browser-side
  terminal input capture and a real runner-owned input handoff for active
  run-backed demo sessions, so the terminal tab can act like a live terminal
  surface instead of a plain log page without launching nested TUIs
- Add `effigy demo resize <DEMO_ID> --cols <COLS> --rows <ROWS>` plus
  runner-owned terminal size/resize contract fields, so active demo sessions
  can report terminal geometry and browser-consumed demo terminals can hand
  resize intent back through the runner instead of inventing browser-local
  session semantics

### Changed
- Keep `effigy demo browser` on one stable `28/72` list/detail split so the
  terminal detail pane no longer expands only after a demo starts and the TUI
  layout stays visually steady while demos launch
- Force `effigy demo browser` live terminal sessions to launch demo children
  with `EFFIGY_COLOR=always` and no `NO_COLOR`, so ANSI-colored demo output
  can render inside the browser terminal tab instead of being downgraded to
  plain text by piped stdout auto-detection
- Tighten `effigy demo browser` live terminal rendering so browser-launched
  attached demo sessions seed their initial terminal geometry from the detail
  pane and strip the stray literal `^D` wrapper noise that could leak into the
  terminal tab for interactive demos like `lifecycle-window`
- Make the Effigy CLI header width-aware in browser-owned live demo terminal
  sessions so narrow terminal tabs keep the header but truncate long repo paths
  instead of wrapping stray path fragments like `/effigy` onto a second line
- Deepen concurrent-runner-backed projected demo runtime reporting with
  runner-owned `projected_output_provenance` facts so inspect and active
  terminal/session payloads now say whether flattened concurrent output is
  `single-source` or `flattened-unlabeled` instead of making browser
  consumers guess how source attribution survives projection
- Deepen concurrent-runner-backed projected demo runtime reporting with
  runner-owned `projected_process_summary` facts so inspect and active
  terminal/session payloads now expose the managed process names behind one
  flattened demo-owned terminal/session and whether that projected surface is
  merging output from multiple named managed processes
- Deepen concurrent-runner-backed demo runtime reporting with runner-owned
  `projection_shape` facts so inspect and active terminal/session payloads now
  say whether a demo is `single-terminal` or `projected-multi-process`,
  whether one live browser terminal is eligible, and how many managed
  processes sit behind the projection when that count is known
- Deepen demo inspect/active-session contracts with runner-owned
  `runtime_backend` identity and capability facts so task-backed, run-backed,
  and future richer demo runtimes can stay demo-scoped without forcing meaning
  through browser-only semantics or nested TUI launch
- Project concurrent-runner-backed demos through the shipped demo session
  contract so `demo inspect`, active terminal/session reporting, and `demo stop`
  expose honest flattened concurrent-runner facts without launching a nested
  TUI
- Deepen concurrent-runner-backed demo sessions with bounded input-forwarding
  and resize projection so detached browser and CLI consumers can use the same
  demo-owned interaction contract already exposed by run-backed demos without
  nested TUI launch
- Deepen active demo terminal/session reporting with explicit terminal size,
  resize posture, and detached-session resize handoff metadata, and have
  `effigy demo browser` auto-sync the terminal tab viewport through that
  runner-owned resize surface when the active session supports it
- Trim `effigy demo browser` history, terminal, and artifacts tabs down to
  their core content, strengthen the right-panel title/tab chrome, and let the
  terminal tab fall back to latest-attempt logs when no active session exists
- Tighten `effigy demo browser` right-panel chrome so the selected demo title
  owns the panel frame, the tab strip drops the redundant `tabs:` label, and
  in-body duplicate tab titles are removed
- Swap `effigy demo browser` to a panel-first control model so `Tab` and
  `Shift+Tab` switch between the demo list and detail pane while `←` and `→`
  switch the selected detail view and `↑` and `↓` stay inside the focused
  panel
- Make text-mode `effigy demo run <DEMO_ID>` attach directly for interactive
  and hybrid run-backed demos while still teeing live stdout/stderr into the
  runner-owned log, receipt, and active-session surfaces so human terminal use
  stays first-class without dropping the shipped machine/client contract
- Deepen text-mode interactive and hybrid run-backed `effigy demo run <DEMO_ID>`
  into a PTY-backed terminal session on macOS, reporting honest `pty`
  transport metadata and merged terminal transcript capture instead of
  pretending PTY demos still have a split stdout/stderr stream
- Rework `effigy demo browser` around the same magenta-framed visual language
  as the concurrent TUI and collapse the interaction model down to arrow-key
  browsing, enter-led action dispatch, escape-to-close, direct search, and one
  bounded filter sheet instead of a large exposed hotkey surface
- Simplify the `effigy demo browser` detail pane into a shorter high-signal
  structure, grouping run state and metadata more tightly and replacing the
  old artifact dump with a selected-artifact summary
- Reshape `effigy demo browser` around explicit list/detail panel focus so
  `←`/`→` switches panes, `↑`/`↓` acts inside the focused pane, and the detail
  side now opens directly into the selected artifact instead of acting like a
  scrolling receipt wall
- Route catalog discovery, doctor strict-parse checks, docs-policy loading, and
  scan manifest options through the same composed-manifest loader so split
  config behaves consistently across runtime, health, and validation surfaces
- Make composition conflict errors and inspect output more actionable by naming
  both source fragments in conflict/override reporting and grouping effective
  value sources by fragment in text-mode inspection
- Make `effigy demo inspect <id>` report active in-flight attempt state
  separately from the latest terminal receipt, so operators can distinguish
  `running now` from `last known proof result`
- Make `effigy demo inspect <id>` and `demo list` expose browser-facing
  freshness, receipt presence, grouped discovery output, and action
  availability so the next browser/TUI slice can build on honest runner state
- Remove artifact opening from the `effigy demo browser` action sheet so
  retained artifacts are activated only through detail-pane navigation instead
  of a redundant action that bypassed the current selection flow
- Make `effigy demo browser` hide the `Result` section until the current
  session runs or reruns a demo, move `covers:` directly under `tags:`, and
  keep the result summary at the bottom of the detail pane so fresh-session
  overview layouts stay focused on metadata, actions, and artifacts first
- Remove the redundant bracketed action summary from `effigy demo browser`
  list rows now that run/history/artifact affordances are owned by the detail
  pane instead of the left-hand list
- Make `Esc` in `effigy demo browser` return from nested detail-pane history
  back to the demo overview before quitting, so `Esc` only exits the browser
  from the root overview surface
- Replace the `effigy demo browser` terminal tab's text/log summary with a
  demo-scoped terminal screen, letting `↑` and `↓` scroll terminal output,
  `Enter` toggle terminal input capture when the active session supports it,
  and `Esc` leave terminal input mode before it navigates the browser

### Fixed
- Make stop-requested run-backed demos persist `stop-requested` state before
  sending the termination signal, so fast-exiting demo processes are recorded
  as `terminated` instead of racing into `failed` receipts on CI or other
  fast hosts

## [0.2.12] - 2026-03-24

### Added
- Managed concurrent tasks now support `shutdown_on_exit = true` on
  individual `concurrent` entries, allowing one process such as an Electron
  main window to shut down the whole stack when it exits.

### Fixed
- Switch `release.sync-files = ["Cargo.lock"]` preparation from
  `cargo check --quiet` to `cargo generate-lockfile --quiet`, so
  `effigy release prepare --yes` can refresh the lockfile without stalling in a
  build-oriented Cargo path.

## [0.2.11] - 2026-03-18

### Fixed
- Make text-mode release orchestration report live phase, mutation, and gate
  progress on TTYs across `status`, `simulate`, `prepare`, `gates`, and
  `execute`, so long-running steps like `cargo check`, `cargo test`, and
  `cargo build` no longer appear to hang while the release pipeline is still
  making forward progress

## [0.2.10] - 2026-03-18

### Added
- Start `g02.001` with a first-class `effigy bootstrap` command surface and
  initial runtime: parser/help/JSON-envelope wiring, root clone-or-update,
  repo-owned `[bootstrap]` manifest loading, optional submodule sync, child
  repo checkout, declared setup tasks, explicit `--start`, and preview support
  through `--plan`

### Fixed
- Harden `effigy bootstrap` around real bring-up edge cases by failing cleanly
  on dirty or remote-mismatched existing checkouts, preserving optional-child
  failures as warnings instead of hard errors, and making text-mode execution
  summaries report missing setup/child state more explicitly
- Expand `effigy.bootstrap.v1` and text-mode output to report root checkout
  decisions, per-child destination/branch outcome details, and the difference
  between “no manifest file” and “manifest exists but has no `[bootstrap]`
  contract”
- Pin Linux release artifacts to an Ubuntu 22.04 glibc baseline and add a
  release-time GLIBC floor check so GNU binaries do not regress to newer libc
  requirements like `GLIBC_2.39` without the workflow failing first
- Make `effigy docs check-json-examples` resilient to numbered section drift by
  matching semantic H2 titles like `Completion Candidates` even when guide
  insertions change the visible ordinal, and fix the bootstrap no-manifest test
  expectation that was still asserting an `effigy.toml` existed in the plain
  fixture repo

## [0.2.9] - 2026-03-16

### Added
- Add explicit built-in deferral through `[defer].builtins = ["release", ...]`
  so legacy repos can route selected parser-level command families back through
  defer instead of Effigy's native built-ins
- Defer `release` by default in automatic PHP-legacy mode (`composer.json` +
  `effigy.json`), so legacy repos do not need an explicit
  `builtins = ["release"]` entry just to preserve their existing release flow

### Changed
- Hide explicitly deferred built-ins from general help and the built-in section
  of `effigy tasks`, so legacy-command ownership is reflected consistently in
  both routing and discovery surfaces
- Run-array `task = "..."` steps now support managed/concurrent task targets by
  delegating through a nested Effigy invocation when the referenced task has no
  inline `run = ...`, instead of failing with a misleading missing-run error

## [0.2.8] - 2026-03-15

### Changed
- Refresh Effigy's own install and `setup-effigy` examples to `0.2.7` after the
  patch release, while leaving consumer repos alone because no active CI/docs
  pins below `0.2.7` were found in the rollout audit
- Change default task locking from workspace-wide to per-task scopes, and add
  per-task shared lock names via `tasks.<name>.lock = "<shared-name>"` for the
  cases that still need explicit cross-task serialization

### Fixed
- Keep `effigy doctor` aligned with the live manifest surface by accepting
  `[docs_policy]` and `[release]`, add coverage against the current repo
  manifest so new top-level sections do not silently drift out of the schema,
  and tighten attention-marker matching so category words in changelog/docs
  prose no longer report as real attention markers
- Stop `qa:docs:agent-defaults` from failing in CI when the `setup-effigy`
  submodule is not checked out, and calibrate this repo's `scan.god_files`
  baseline so doctor focuses on unexpected new mega-hubs instead of known
  intentional command/test aggregation points

## [0.2.7] - 2026-03-12

### Added
- Add built-in `effigy docs check-paths` so repos can validate required
  contract files/directories such as `README.md`, `AGENTS.md`, and the minimum
  Northstar docs spine without bespoke shell checks

### Changed
- Define the reusable `qa:northstar` starter bundle around existing native docs
  validators (`check-index`, `check-next-action`, `check-headings`,
  `check-forbidden`) and document the product boundary between Effigy-native
  validation engines and the `northstar-effigy` skill/template layer
- Package the starter native consumer `[docs_policy]` bundle for vision index
  and next-action validation, fix the contract docs to use the real
  `check-headings --require-heading` flag, and prove the reusable `qa:docs` /
  `qa:northstar` shape against a neutral fixture instead of only migrated repos
- Extend the starter `qa:northstar` bundle with root/front-door/docs-spine
  drift checks using `check-paths` plus generic `check-contains` rules for
  agent-loop and discoverability surfaces
- Prove the completed starter bundle on both single-repo and workspace-root +
  nested-docs-authority fixtures, and keep bootstrap scaffolding in the
  `northstar-effigy` skill/templates layer instead of productizing an Effigy
  `init` surface prematurely
- Align the README, docs landing pages, agent-adoption guide, and roadmap
  indexes with the finished product boundary so the source-of-truth docs no
  longer describe `g01.029` as a future migration milestone

### Fixed
- Normalize scp-style SSH remotes such as `git@github.com:owner/repo.git`
  during `effigy release verify-install`, so tagged install verification works
  from auto-detected `origin` remotes without requiring manual `--repo-url`
  rewriting

## [0.2.6] - 2026-03-12

### Added
- Add top-level `effigy --version` and `effigy version`, with matching JSON
  envelope output so operators and automation can inspect the current binary
  version without parsing the general help banner
- Add built-in `effigy docs` validation commands for markdown link checks, JSON
  example section checks, and docs-index consistency checks, with the matching
  `scripts/check-doc-*.sh` entrypoints reduced to thin compatibility wrappers
- Add built-in `effigy contracts check-json` to run schema-index-driven JSON
  contract validation, selected-schema reporting, and changed-only checks
  without the previous shell-library implementation, with
  `scripts/check-json-contracts*.sh` reduced to thin wrappers over the built-in
- Add built-in `effigy contracts validate-selection` for JSON selection
  artifact validation, replacing the prior jq-heavy shell implementation behind
  `scripts/validate-json-contract-selection-artifact.sh`
- Add built-in `effigy release validate`,
  `validate-artifacts`, `generate-closeout`, and `write-summary` so
  distribution metadata checks, first-publish summary contracts,
  artifact-bundle validation, and acceptance-closeout log generation no longer
  depend on shell as the primary implementation language

### Changed
- Route `qa:json` and `qa:json:ci` directly through `effigy contracts
  check-json`, and reduce `scripts/check-selection-artifact-validator-smoke.sh`
  to a compatibility wrapper over targeted Rust coverage instead of owning its
  own validator fixture logic in shell
- Route `dist:metadata` and the release `metadata` gate through `effigy
  release validate`, and reduce
  `scripts/check-distribution-artifact-pipeline-smoke.sh` to a compatibility
  wrapper over targeted CLI coverage for the built-in distribution flow
- Route `qa:docs` and release-gate `qa` orchestration through native
  `effigy.toml` task composition, broaden `effigy docs check-links` default
  scope to the full `docs/` tree, and reduce `scripts/check-quality-gates.sh`
  plus `src/bin/effigy-qa.rs` to compatibility delegation over those task
  surfaces
- Reduce `scripts/check-distribution-first-publish.sh` so the
  `distribution-summary.env` contract is written by `effigy distribution
  write-summary`; the wrapper now retains only publish/install side effects,
  step-log capture, and final built-in artifact validation
- Add built-in `effigy release preflight` with summary-file output for
  the non-publish distribution gate path, and reduce
  `scripts/check-distribution-preflight.sh` to a compatibility wrapper over
  the native preflight surface
- Replace `scripts/check-prepush-ci.sh` with a thin wrapper over native
  `prepush:ci` task aliases, and update active operator docs to lead with
  `effigy` task/command entrypoints instead of the old shell-first QA wording
- Reduce `scripts/check-distribution-first-publish.sh` to the intentional
  external side-effect boundary by delegating tag verification, summary
  writing, and artifact validation to native Effigy commands instead of
  shell-wrapper entrypoints
- Retire redundant docs/knowledge/contracts/distribution wrapper scripts and update
  active workflows, tasks, and operator guides to call native `effigy`
  commands or targeted Rust tests directly where no external script boundary
  is needed
- Remove the unused `scripts/check-json-contracts-ci.sh` wrapper and keep
  pull-request versus mainline JSON-contract policy in workflow YAML instead of
  duplicating it in shell
- Add built-in `effigy docs add-log-index` and retire the old
  `scripts/add-log-index-entry.sh` helper so docs/log index maintenance stays
  inside the native docs command surface
- Add built-in `effigy docs check-workflow-paths` and retire the old
  `docs/scripts/check-doc-workflow-paths.sh` helper so docs workflow-reference
  validation no longer depends on a standalone shell script
- Add optional `[docs_policy.indexes]` manifest support and
  `effigy docs check-index --policy-index <NAME>` so repo-specific markdown
  index rules can be supplied declaratively instead of hardcoded into generic
  built-ins
- Add optional `[docs_policy.next_actions]` manifest support and
  `effigy docs check-next-action --policy <NAME>` so repo-specific heading and
  actionable-verb rules can be enforced by a reusable built-in engine instead
  of a standalone shell checker
- Move the active vision index validation path onto
  `effigy docs check-index --policy-index vision`, so
  `docs/scripts/check-vision-index.sh` is no longer needed as a standalone
  entrypoint in the repo's active docs QA flow
- Move the active vision next-action validation path onto
  `effigy docs check-next-action --policy vision`, so
  `docs/scripts/check-vision-next-task.sh` is no longer needed as a standalone
  entrypoint in the repo's active docs QA flow
- Move next-action negative-path coverage out of a shell regression harness and
  into Rust CLI tests, so docs QA validates the live repo state without mixing
  in fixture-only shell checks
- Replace the last docs-policy shell bundle with visible `qa:docs:vision` task
  composition plus generic `effigy docs check-headings` /
  `effigy docs check-contains` validators, so repo policy stays in the
  manifest/task graph instead of a dedicated bash entrypoint
- Remove the redundant `effigy-release-qa` helper binary and point
  `cargo qa-release` directly at `effigy release gates`, reducing one
  more compatibility layer without changing the operator-facing release gate
  path
- Stop advertising the broken `qa:release` task alias and lead with
  `effigy release gates` plus `cargo qa-release`, because
  manifest-task wrapping around release gates still self-nests under the
  workspace lock when release gates invoke nested Effigy commands
- Classify the remaining top-level release/bootstrap scripts into durable
  external boundaries versus timed compatibility backups, and document explicit
  retirement criteria for the three release wrapper scripts instead of keeping
  their lifespan open-ended
- Add a dedicated release-wrapper retirement record template guide so the next
  real release cycle can capture a concrete keep/retire decision in one
  reusable checkpoint artifact
- Add a dedicated release checkpoint log template guide so real release evidence,
  distribution evidence, and wrapper-retirement decisions can be captured in a
  single dated maintainer artifact
- Rewrite the top-level README, docs landing pages, and the first layer of
  workflow guides around newcomer, day-to-day, troubleshooting, automation,
  release, contribution, and support-doc user flows, add an
  everyday-workflows guide, refresh the IA snapshot, and move repo-specific
  self-hosting detail plus redundant `--repo .` examples out of the primary
  docs paths so the main Effigy feature set is easier to discover without
  teaching unnecessary flags before readers drop into the deeper reference
  material
- Add built-in `effigy docs check-forbidden`, wire `qa:docs` through a
  self-hosted agent-defaults guard, and update active adoption/setup/help
  surfaces plus workflow examples, and remove the same bad default from live
  release remediation hints and completion help examples so copied `--repo .`
  usage fails validation instead of spreading into downstream agent instructions
- Add a concrete Northstar + Effigy consumer repo contract guide, a cross-repo
  adoption landscape scan, and a first `monkey` pilot gap assessment so the
  new consumer-adoption roadmap has an explicit Wave 1 source of truth instead
  of only roadmap intent
- Refine the Northstar + Effigy consumer-adoption contract after the
  `compli-me` pilot so the active guidance now models both single-repo
  adoption and thin-workspace-root plus nested docs-authority adoption instead
  of assuming every consumer project should carry one root-level docs/release
  surface
- Prove the same contract on `underlay` as a shared foundation repo, showing
  the native single-repo path works outside app repos while also surfacing the
  remaining question of when changelog/release posture becomes mandatory for
  adoption
- Prove the same contract on `acowtancy` as a thin workspace container with
  `ledger` as the docs authority, and tighten the portable `northstar-effigy`
  skill plus templates around the explicit split between orchestration roots,
  docs-authority repos, and releasable repos

## [0.2.5] - 2026-03-11

### Added
- Add changelog library implementing the Northstar Changelog Profile — parse,
  format, validate, analyze, and extract changelogs with `effigy changelog`
  subcommands (`validate`, `format`, `analyze`, `extract`)
- Add `effigy release status` and non-destructive `effigy release prepare --plan`
  with `[release]` manifest config, version-file autodetection (`Cargo.toml`,
  `package.json`, `pyproject.toml`, `VERSION`), changelog readiness checks,
  version/changelog mutation previews, optional gate execution, and JSON
  payloads
- Add non-interactive `effigy release prepare --yes` to apply supported
  version/changelog updates and write `.release-prepared.json` state without
  committing, tagging, or pushing
- Add `effigy release execute --plan` as a preflight that loads
  `.release-prepared.json`, warns on stale prepared state, and verifies the git
  working tree matches the prepared file set before any commit/tag/push work
- Add non-interactive `effigy release execute --yes` to create the release
  commit and tag, push branch and tag to `origin`, print post-release checks,
  remove `.release-prepared.json` only after full success, and refuse to re-tag
  after a failed push
- Add standalone `effigy release gates` with sequential timed gate execution,
  fail-fast behavior, JSON output, and captured failed-gate output for release
  readiness checks outside the full prepare flow
- Add `effigy release simulate` as a full dry-run that runs release gates,
  previews version/changelog mutations plus commit/tag creation, reports
  fail-fast gate metadata, and guarantees no files or `.release-prepared.json`
  state are written
- Add `--dry-run` as a non-destructive alias for `effigy release prepare --plan`
  and `effigy release execute --plan`, so preview-first release flows can use
  either spelling while still producing the same plan payloads
- Preserve existing layout for release version-file updates in `Cargo.toml`,
  `pyproject.toml`, and `package.json` by mutating only the targeted version
  field instead of reformatting the whole file during `effigy release prepare`
  flows
- Add a self-hosted `[release]` section to this repo’s `effigy.toml` and route
  `qa:release` through `effigy release gates`, with contract tests that keep
  the configured baseline gate set aligned with `scripts/check-release-gates.sh`
- Add real `release.sync-files = ["Cargo.lock"]` support for Cargo-based
  release preparation, including prepare-plan/prepare-apply coverage and a
  Cargo-fixture parity test against `scripts/prepare-release.sh --apply`
- Add built-in `effigy release verify-install` for tag-based install
  validation, with the legacy `scripts/check-release-install-from-tag.sh`
  helper now delegating to the built-in command
- Turn `scripts/check-release-gates.sh` into a compatibility wrapper over
  `effigy release gates` plus optional `effigy release verify-install`, and add
  self-hosting contract checks that keep both legacy wrapper entrypoints aligned
  with the built-in release surfaces
- Add end-to-end migration parity tests showing the legacy release wrappers
  execute the same built-in `release gates` and `release verify-install` paths
  on Effigy-shaped fixtures, closing the remaining section-8 parallel-validation
  proof for shipped release surfaces
- Update the release checklist template and maintainer/operator docs to prefer
  the built-in `effigy release simulate/status/prepare/execute/verify-install`
  flow while keeping legacy release scripts documented as backup channels
- Tighten release protocol/checklist wording so built-in commands are clearly
  the primary operator path, legacy shell wrappers remain explicit backup
  channels until the first successful live built-in release, and workflow
  cutover tasks stay clearly human-gated
- Add end-to-end CLI coverage and maintainer guidance for
  `effigy changelog extract` as the preferred release-note baseline generator
  ahead of any approved workflow migration
- Add cross-project release orchestration coverage for `package.json`,
  `pyproject.toml`, and plain `VERSION` repos, plus agent-adoption examples for
  Node.js, Python, and multi-language release configs
- Add dedicated release orchestration guide `051`, update the command matrix to
  include `release` and `changelog` surfaces, and align `CLAUDE.md` with the
  built-in release workflow reference
- Add text-mode interactive confirmation flows for `effigy release prepare` and
  `effigy release execute`, while keeping `--plan` as preview-only and `--yes`
  as the explicit non-interactive path
- Expand text-mode `effigy release prepare` and `effigy release execute` into
  staged review flows with separate version/state, mutation/working-tree, gate,
  and final approval prompts before any release changes are applied
- Require explicit stale-state acknowledgement for `effigy release execute`:
  text-mode execute now inserts a stale-state approval step, while `--plan` and
  `--yes` require `--allow-stale` to proceed when `.release-prepared.json` is
  older than the default threshold
- Allow text-mode `effigy release prepare` to accept a deliberate custom semver
  override during version review, and carry suggested-versus-selected version
  metadata through prepare output and `.release-prepared.json`
- Add `--version <SEMVER>` override support to `effigy release prepare --plan`
  and `effigy release prepare --yes`, so non-interactive preview/apply flows
  can use the same deliberate version-selection contract as interactive prepare
- Tighten `release prepare --version` validation and surface
  suggested-versus-selected version metadata consistently in `release simulate`
- Add `effigy release simulate --version <SEMVER>` so full dry-run previews can
  exercise the same deliberate selected-version contract as non-interactive
  `release prepare` without writing files or state
- Upgrade `effigy release simulate` and `effigy release prepare --plan` with
  richer per-file mutation details and concise inline diff previews for
  supported write mutations
- Add interactive mutation drill-down to plain `effigy release prepare`, so
  Step 2 review can inspect one planned file mutation in detail before apply
- Add interactive drill-down to plain `effigy release execute`, so stale
  warnings and working-tree items can be inspected in detail before approval or
  before a blocked preflight returns failure
- Replace the fixed linear interactive release review flow with compact prepare
  and execute review menus, so operators can jump directly between review
  sections before apply/execute
- Keep interactive release review menus self-describing with a compact command
  legend plus persistent selected-version or stale-acknowledgement summaries,
  so operators can see the active state without re-reading prompt footers
- Mark reviewed sections directly inside interactive release menus and append
  suggested remediation actions to blocked prepare/execute output, so operators
  can track review progress and see the likely next fix path
- Add `effigy release resume` as a dedicated prepared-state recovery command
  that summarizes `.release-prepared.json`, highlights drift since prepare
  time, and can hand operators directly back into interactive execute review
- Add prepared-state source fingerprints to `.release-prepared.json`, so
  `effigy release resume` and `effigy release execute --plan/--yes` can detect
  branch drift, HEAD movement, and changed prepared-file contents since
  prepare time
- Add direct interactive recovery shortcuts to `effigy release resume` and
  `effigy release execute`: operators can now run `gates`, `reprepare`, or
  `discard` from the review flow, and blocked execute preflight exposes the
  same shortcuts before failing
- Add `@env-spec` integration: declarative `.env.schema` files with annotation
  DSL (`@type`, `@required`, `@sensitive`), value expressions (`exec()`,
  `env()`, `${VAR}` templates), type validation, topological dependency
  resolution, and dual environment injection (plain values via shell wrapping,
  secrets via `Command::env()` to avoid `ps` exposure)
- Add `[env_schema]` configuration section in `effigy.toml` with `enabled`,
  `schema` path override, and `exec_timeout` options
- Add `--env-schema <PATH>` task-runtime override so one-off task invocations
  can select a non-default `.env.schema` file without editing `effigy.toml`
- Allow run-array env directives, task-ref expansions, and configured built-in
  test suite env resolution to consume resolved `.env.schema` values for
  internal Effigy planning/runtime behavior
- Add env-schema string constraint annotations (`@min`, `@max`) and regex
  validation via `@pattern`, with task execution now failing before launch when
  resolved values violate those schema rules
- Redact sensitive env-schema validation values from task/runtime error output
  and back `SecretString` with `zeroize::Zeroizing<String>` for stronger
  in-memory secret handling
- Round out the public env-schema library surface with autodetection helpers,
  explicit `resolve_env` / `validate_env` entry points, and `ResolvedEnv`
  export helpers that return `HashMap<String, EnvValue>`
- Validate `[env_schema]` manifest configuration more strictly and add runtime
  coverage for `enabled`, `schema`, and `exec_timeout` behavior
- Extend env-schema secret redaction coverage across JSON-mode runner failures
  and resolved-env debug output so sensitive values stay masked across normal
  Effigy reporting surfaces
- Add roadmaps for Varlock @env-spec integration (025), changelog library and
  Northstar Profile (026), and release orchestration system (027)

### Changed
- Cut over `.github/workflows/release-binaries.yml` to use built-in
  `effigy changelog extract` for GitHub Release notes with the existing
  generated-notes fallback preserved, and refresh the touched GitHub-managed
  action versions used by release/CI/JSON-contract workflows ahead of the
  Node 24 runtime transition

### Fixed
- Align `scripts/check-distribution-metadata.sh` with the actual
  `release-binaries.yml` workflow and current distribution helper scripts, so
  release metadata validation no longer fails on obsolete workflow file names
  during built-in release rehearsals

## [0.2.4] - 2026-03-10

### Added
- Publish `inflatable-cookie/setup-effigy@v1` GitHub Action for CI binary
  installation with caching
- Add ARM Linux (`aarch64-unknown-linux-gnu`) binary to release pipeline and
  Homebrew formula (for AWS Graviton, Docker on Apple Silicon)

## [0.2.3] - 2026-03-10

### Added
- Homebrew formula now supports Linux (Linuxbrew) via `on_linux` block for
  x86_64 binaries

## [0.2.2] - 2026-03-10

### Fixed
- Fix release pipeline failure caused by using `secrets` context in job-level
  `if` condition — `secrets` is only available at step level in GitHub Actions

## [0.2.1] - 2026-03-10

### Added
- Homebrew tap auto-update in release pipeline — formula in
  `inflatable-cookie/homebrew-tap` is updated automatically on each tagged
  release
- JSON contracts CI workflow (`.github/workflows/json-contracts.yml`) now
  active on PRs, pushes to main, and daily schedule
- CHANGELOG-based release notes — GitHub Releases now use entries from
  CHANGELOG.md instead of auto-generated notes
- Install section in README with three channels: Homebrew, prebuilt binary,
  and cargo install from source

### Changed
- Replace ripgrep (`rg`) with POSIX `grep` in all QA and docs-check scripts
  so CI runners work without ripgrep installed
- Remove `.github-bak/` staging directory — all workflows are now active or
  superseded
- Update doc 042 (Homebrew Tap) to reflect prebuilt binary formula approach
- Update doc 049 (Release Protocol) to reflect current active workflow state

### Fixed
- Fix JSON contracts CI failure caused by `rg` not being available on
  ubuntu-latest runners
- Fix stale `.github-bak/` workflow references across documentation

## [0.2.0] - 2026-03-09

### Breaking
- Change process spawn from login shell (`sh -lc`) to non-login shell (`sh -c`)
  across all execution paths. Fixes PATH clobbering on Linux where `/etc/profile`
  unconditionally resets PATH in login shells. Parent process environment is now
  inherited correctly on all platforms.

### Added
- CI workflow (`.github/workflows/ci.yml`) with format, clippy, and test jobs
  on Linux and macOS
- Release binaries workflow (`.github/workflows/release-binaries.yml`) for
  cross-platform binary distribution via GitHub Releases
- Changelog and automated release preparation (`scripts/prepare-release.sh`)

### Changed
- Rename test fixture catalogs from project-specific names (`farmyard`, `dairy`,
  `cream`) to generic names (`catalog_a`, `catalog_b`, `catalog_c`) across all
  test suites, scripts, and documentation
- Use `frontend` in user-facing help text examples instead of project-specific names

### Fixed
- Resolve 5 pre-existing clippy warnings (needless return, vec init-then-push,
  field reassign with default, manual contains)
- fix Rhai `exec::run(..., #{ run_in: "host" })` inside workspace container handoff so host-context `cwd` and `stdin_file` paths are remapped onto the local workspace repo before execution

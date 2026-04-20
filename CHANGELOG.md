# Changelog

All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/).
During v0.x, MINOR bumps may include breaking changes.

## [Unreleased]

### Breaking
- Move default workspace container config onto `[systems.<name>]` itself,
  replacing the extra `[systems.<name>.workspace_defaults]` layer, and route
  workspace `workdir`, `user`, `home`, and `mounts` resolution through
  that system-level config plus per-workspace overrides.

### Added
- Add the first composed `system`/`workspace` task-runtime contract, including
  manifest-owned `[systems]` definitions, per-system default workspaces,
  task-level `system` and `workspace` binding, named workspace-to-container
  resolution, and inline workspace container sugar that normalizes onto the
  same execution model.
- Auto-provision a Linux `effigy` binary into workspace containers on
  `effigy workspace`, reusing or rebuilding the cached local Linux rehearsal
  artifact from the current Effigy checkout before installing it into the
  running workspace service.
- Add the first bounded `g02.013` managed dev-task foundation, including
  `[tasks.<name>.managed].container_lifecycle`, `concurrent` lifecycle roles,
  plan/schema/docs support, and managed runtime ownership for starting and
  stopping a repo-owned workspace-backed container through one task-owned
  lifecycle process.
- Add bounded `g02.013` managed shell-role support, so `concurrent` entries
  can declare `role = "shell"` and open the task-owned primary-service
  container shell through the shipped `effigy container shell` path.
- Add bounded `g02.013` managed readiness UX support, so repo-owned managed dev
  tasks can declare `managed.health_wait` plus `managed.ready_message`, render
  that contract in plan/docs/schema output, and project one honest ready
  message through the lifecycle-owned runtime path after detached container
  startup reaches ready state.
- Add bounded `g02.013` managed gateway auto-start support, so repo-owned
  managed dev tasks can declare `managed.gateway = true`, validate that
  contract against lifecycle-owned workspace containers, render it in
  plan/docs output, and trigger the shipped `effigy gateway up` path before
  the managed runtime starts.
### Fixed
- Let single-entry container/system/workspace registries resolve implied
  defaults, so repos can omit `[containers].default`, `[systems].default`,
  `[systems.<name>].default_workspace`, and workspace `container = "..."`
  when there is only one valid choice.
- Let empty systems expose an implied `default` workspace when an eligible
  default dev container exists, so repos can keep `[systems.<name>]`
  without declaring a placeholder `[systems.<name>.workspaces.default]`.
- Remove the unused `[containers.<name>.ui]` config surface and its
  `ui_tabs` status/report output; attached container sessions now always
  render the built-in `overview` plus `primary_service` tabs instead of
  pretending repos can shape that dead UI path.
- Start the gateway for workspace-seeded managed dev tasks too, so
  `managed.gateway = true` now applies on the TUI workspace handoff path
  instead of only on the later in-process managed runtime branch.
- Stop the gateway daemon automatically when container route deregistration
  leaves the shared route table empty, keeping the resolver setup in place
  while restoring the intended zero-route idle shutdown behavior.
- Make managed lifecycle owner shells self-terminate when their parent runtime
  disappears, so interrupted or abandoned dev sessions do not leave idle
  `managed-lifecycle/*.state` owners behind on the host.
- Make gateway route registration reject host ports already held by unrelated
  listeners, so container DNS routes no longer silently proxy to whichever
  process happens to own the declared host port.
- Roll back detached container bring-up automatically when gateway
  registration fails, so rejected DNS-route registration does not leave a
  partial stack running behind.
- Allow `[containers.<name>.dns]` routes to declare an optional `service`,
  and when present require gateway registration to prove the published host
  port belongs to that runtime service instead of only the same compose
  project.
- Make the gateway daemon arm a 5 minute idle shutdown when the watched
  route table transitions from non-empty to empty, and cancel that timer
  if routes return, so workspace/container restarts do not immediately tear
  down the shared gateway after the last route is removed.
- Make `effigy workspace` verify that the bound system's primary service is
  actually running before it skips bring-up, so stale compose state no longer
  drops straight into `no running containers from service workspace`.
- Route `effigy workspace` / `effigy container shell` through the normal exec
  transport with the container exec working dir and an interactive shell
  launcher, so the workspace shell opens in the expected directory and no
  longer wedges on exit behind the isolated lifecycle spawn path.
- Stop managed shell panes inside workspace-backed `effigy dev` runs from
  assuming `/bin/zsh` exists in the container; the default handoff shell now
  falls back through `$SHELL`, `bash`, and `sh`.
- Stop plain `exit` from an interactive `effigy workspace` shell from
  surfacing as a transport failure when the shell inherits a non-zero status
  from the last command; only command-mode container execs now treat non-zero
  shell exit as an Effigy error.
- Route host-side workspace-backed managed TUI tasks like `effigy dev`
  through the same `effigy workspace` shell session path, so the task now
  starts inside the container and drops back to the workspace shell when the
  managed session exits instead of maintaining a separate host orchestration
  stack.
- Add manifest-owned workspace `mounts` support for direct-compose workspace
  containers, and rewrite the runtime workspace
  service bind mounts onto `repo root + explicit extras` instead of requiring
  one broad parent-directory mount in the checked-in compose file.
- Route managed standard container panes through `effigy container shell
  --command` instead of the ad hoc `effigy exec` surface so managed dev
  sessions use the same transport as the stable shell pane on Colima.
- Run routed workspace-backed task execs through `compose exec -T` so
  non-interactive managed/dev task panes do not allocate TTYs on the
  Colima/containerd path, reducing long-running task dropouts under the
  `colima nerdctl` fallback surface.
- Time out managed container shutdowns inside the container exec layer and
  terminate the whole shutdown process group, so hung `container down`
  cleanup stops failing as an outer 15s wrapper timeout with orphaned child
  processes left behind.
### Changed
- Show a spinner for non-streaming `effigy system` calls while they are
  waiting on container runtime output, so long-running `system up/down/logs`
  paths do not sit silent in interactive terminals.
- Render Rhai task status prefixes like `[check]`, `[ok]`, `[error]`, and
  `[next]` with native colour at the task log source, and switch owned
  `effigy workspace` shutdown onto a transient spinner so exit waits stay
  visible without leaving an extra notice line behind.
- Shorten the Linux release rehearsal closeout so it ends on a compact
  success status instead of a long `[next] inspect the artifact ...` line.
- Add explicit `effigy system repair` support for bounded Colima profile
  recovery, so operators can restart one broken workspace runtime through
  Effigy instead of working through manual `colima` / `limactl` cleanup.
- Remove the unpublished redundant task field `container_session` and route
  task-owned container execution entirely through the released `system` /
  `workspace` contract.
- Add transparent container exec integration for `g02.012`, including
  manifest-owned `[containers.<name>.exec]` aliases and working-dir config,
  explicit `effigy exec ...`, bare alias fallback like `effigy mysql`, and
  task-routing handoff/CWD mapping for dev-context container execution.
- Add manifest-owned `[containers.<name>.dns]` gateway registration on top of
  the container surface, so `effigy container up/down/reset` and attached
  owner-exit shutdown now write and remove route-table entries for declared
  project hostnames.
- Add optional `[containers.<name>.dns].port` so multi-port container stacks
  can choose which declared host HTTP port the gateway should proxy during
  route registration instead of always taking the first host-port mapping.
- Add the first `effigy gateway` product surface with `up`, `down`, and
  `status` commands, a root-owned hidden gateway daemon entrypoint, JSON/plain
  lifecycle output, and macOS resolver setup/teardown hooks around the
  host-native DNS/proxy gateway.
- Add gateway TLS closeout on top of that surface with `effigy gateway
  setup-tls`, route-owned mkcert certificate generation/cleanup for
  `[containers.<name>.dns].tls = true`, HTTPS bind/status projection, and
  live cert reload so TLS routes can come and go without restarting the
  gateway.
- Add `effigy container status --all` plus a fuller `effigy gateway status`
  route dashboard so operators can inspect running Effigy-managed environments
  and shared registered domains across repos from one product-owned status
  surface.
- Add stable generated-compose host-port auto-allocation when
  `[containers.<name>.host].ports` is omitted, wiring the shared
  `~/.effigy/ports.json` registry through compose generation, container
  policy/status, and gateway route registration while keeping direct
  `compose_file` ownership explicit-ports-only.
- Add `effigy container stats --all` for one bounded cross-project resource
  view, collecting live CPU and memory usage for running Effigy-managed
  containers across repos while degrading honestly when runtime stats are
  partial or unavailable.
- Add bounded generated-compose shared services through `shared = true` for
  standalone backing-service catalogs, so `container up` can start and reuse
  shared MariaDB, Postgres, Redis, and Memcached instances while generated
  consumer compose rewrites local stacks to point at those shared targets.
- Add generated-compose `effigy container reset --keep-data`, so persistent
  named volumes declared through shipped catalog metadata survive reset while
  ephemeral volumes are still removed and direct `compose_file` ownership is
  rejected honestly on this bounded path.
- Add bounded `effigy container data list` on the generated-compose path so
  operators can inspect Effigy-managed named volumes with
  persistent-vs-ephemeral classification and best-effort runtime size metadata
  before wider data lifecycle commands land.
- Add bounded generated-compose `effigy container data export` and
  `effigy container data import`, so operators can move Effigy-managed named
  volumes through the product surface without widening into hook orchestration
  or direct-`compose_file` ownership semantics yet.
- Add bounded generated-compose `[containers.<name>.data].media` support, so
  repo-owned media directories can be declared through the container data
  surface, prepared automatically, and mounted onto generated services that
  already bind the repo root without widening direct `compose_file` ownership.
- Add bounded generated-compose `effigy container data pull-production` plus
  manifest-owned `[containers.<name>.data].pull_production`, so one product
  entrypoint can bring the environment to ready state and run a repo-relative
  Rhai or shell production-pull hook without widening direct `compose_file`
  ownership.
- Add Rhai-backed manifest run steps through `rhai = "path/to/script.rhai"`,
  with a first Effigy-native host API for args, env,
  path/file helpers, JSON/TOML helpers, structured subprocess execution, and
  task invocation so Rust-first repos can start replacing shell glue without
  taking a Bun dependency just for task scripting.
- Add the first bounded `effigy container` surface with manifest-defined
  `[containers]` registries, named/default container resolution, Colima-backed
  compose bring-up/down/status/logs/shell/reset commands, explicit
  host-port/mount policy, attached owner-exit shutdown, and a no-Docker host
  fallback through `colima nerdctl` for machines that only have Colima
  installed.
- Add attached container session widening on top of that foundation, including
  multi-tab container TUI sessions for `effigy container up`, stream-mode
  overview fallback for non-interactive runs, and repo-owned task aliases via
  `container_session = "..."`.
- Extend the first Rhai host API with `now_utc()` and `make_temp_dir(prefix)`,
  so first-party repo automation can stamp artifacts and build short-lived
  working directories without shelling out to `date` or `mktemp`.
- Extend the Rhai host API with `stop_requested()`, `process_id()`,
  `sleep_ms(...)`, and `append_file(...)` so long-running repo-local scripts
  can react cleanly to stop requests, emit heartbeats, and append lifecycle
  evidence without falling back to shell loops.
- Add an optional `[distribution]` manifest section covering package identity,
  preflight task selection, and metadata requirements so native distribution
  commands can start serving other repos without hardcoding Effigy's exact
  release policy.
- Add an Effigy-owned `linux-release` container plus `release:linux:rehearse`
  and `release:linux:env` tasks, so pre-release prep can build, smoke-test,
  and GLIBC-check the Linux binary locally through the shipped container
  surface before relying on CI.
- Add in-process Rhai host helpers `run_effigy(...)`, `run_effigy_json(...)`,
  and first typed container helpers so repo-local scripts can call Effigy
  built-ins without shelling back through `cargo run --bin effigy`.

### Changed
- Make `effigy gateway up`, `gateway down`, and `gateway setup-tls` request
  admin approval on demand for host setup instead of requiring operators to
  rerun the whole command under `sudo`, while keeping gateway state rooted in
  the calling user's `~/.effigy/gateway`.
- Rename the service-catalog CLI surface from `effigy catalog ...` to
  `effigy service ...` so the container fragment feature reads more clearly at
  the command line.
- Migrate Effigy's `link:local` task from a shell script to a file-backed Rhai
  step so the new embedded scripting path is exercised by a real repo-local
  operator task instead of a synthetic fixture.
- Migrate Effigy's `smoke:release` task and `browser-proof-report` demo off
  shell entrypoints and onto file-backed Rhai scripts so the first dogfooding
  cluster exercises both operator tasks and demo runners.
- Migrate Effigy's `lifecycle-window` demo from its shell loop onto a Rhai
  script and prefer attached-stream transport for interactive Rhai-backed runs,
  so stop-aware long-running demos can finish their cleanup path and persist
  terminated lifecycle artifacts without relying on the macOS PTY wrapper.
- Migrate Effigy's compatibility release wrappers
  `scripts/check-release-gates.sh` and
  `scripts/check-release-install-from-tag.sh` so their real logic now lives in
  file-backed Rhai scripts, while the executable `.sh` entrypoints remain as
  minimal launchers for CI/docs compatibility.
- Migrate Effigy's `scripts/install-local-bin-links.sh` and
  `scripts/check-release-smoke.sh` onto the same Rhai-backed compatibility
  launcher pattern, leaving only the genuinely shell-bound operator surfaces as
  explicit permanent boundaries.
- Retire Effigy's compatibility-only shell entrypoints
  `scripts/install-local-bin-links.sh`, `scripts/check-release-smoke.sh`,
  `scripts/check-release-gates.sh`, `scripts/check-release-install-from-tag.sh`,
  and `scripts/prepare-release.sh`, so the repo now points directly at native
  Effigy tasks and built-in release commands instead of preserving legacy shell
  entrypoints for migration safety.
- Replace Effigy's remaining first-publish release wrapper with native
  `effigy distribution first-publish` and `effigy distribution check-glibc-floor`
  built-ins, so distribution orchestration now lives inside Effigy's defined
  command surface instead of depending on shell-script entrypoints for publish
  and validation flow.
- Make `effigy distribution validate-metadata` and
  `effigy distribution preflight` read optional manifest-driven distribution
  policy, so cross-repo adoption can override package identity, required
  evidence files, and preflight task names without forking Effigy's
  self-hosting defaults.
- Widen the optional `[distribution]` contract with publish identity and
  closeout defaults, so `distribution first-publish`,
  `distribution write-summary`, and `distribution generate-closeout` can serve
  other repos with manifest-driven package/binary/registry naming and generic
  closeout text instead of hardcoded Effigy-shaped defaults.
- Harden attached container stop and closeout behavior so startup-phase stop
  requests and nested log-follow subprocess trees now route through one
  reliable shutdown path on real Colima-backed consumer sessions.
- Make `effigy container shell --command <CMD>` run a real shell command string
  via `sh -lc` instead of treating the whole command as one argv token.
- Move `release:linux:rehearse` off `cargo run --bin effigy` subprocess
  re-entry and onto the running Effigy process through the new Rhai host API.

### Fixed
- Stop `effigy doctor` from flagging valid task-level `host = true` bindings as
  unsupported manifest keys after the `system` / `workspace` task-routing
  migration.
- Stop forcing macOS PTY transport onto every managed TUI tab; lifecycle and
  standard app tabs now use plain supervised pipes while the shell tab keeps
  PTY transport, which lets startup failures surface and terminate honestly
  instead of getting stranded behind `script`-wrapped waiting tabs.
- Stop container-backed managed shell and exec tabs from waiting forever when
  the lifecycle owner fails during startup, by projecting lifecycle failure to
  dependent processes so they exit with a real error instead of hanging on the
  first-output spinner.
- Stop container-backed managed child task refs from re-invoking the host
  Effigy binary path inside the workspace container; they now resolve to the
  referenced task command and working directory before the container exec
  wrapper is applied.
- Stop VT-backed managed tabs from clamping scrollback to roughly one screen,
  and allow output scrolling from insert mode as well as command mode.
- Pin explicit runtime `name` values on generated top-level named volumes so
  managed-volume reporting, reset retention, and data export/import stay
  aligned on the Colima/nerdctl path instead of drifting into double-prefixed
  runtime volume names.
- Stop managed dev tabs from colliding on one default task lock or racing the
  container exec surface at startup; prefixed child tasks now scope locks by
  rendered selector, stale reused-PID locks are reclaimed honestly, and
  container-backed child commands wait until the workspace service is ready.
- Hydrate empty container-local JS dependency volumes during managed container
  startup when the repo declares `[package_manager].js`, so container-backed
  `vite`/`svelte-kit` tabs do not fail immediately on first launch just
  because named `node_modules` mounts start empty.
- Auto-repair broken Colima profile DNS during compose/build bring-up by
  restarting the selected profile with fallback public resolvers and retrying
  once when Docker Hub metadata fetches fail with the known
  `registry-1.docker.io` lookup outage shape.
- Stop managed TUI and other supervised multiprocess runs from leaking nested
  child workloads when an immediate task runner exits or is terminated;
  shutdown now walks descendant process trees instead of only signaling the
  first child process group.
- Regenerate generated compose output when the rendered compose content changes
  even if the manifest checksum is unchanged, preventing stale compose
  artifacts after assembly-layer fixes.
- Stop `effigy --help` and other builtin-deferral probes from recursively
  scanning arbitrary unanchored directories like `~/`; deferred builtin
  discovery now only walks child catalogs after a root `effigy.toml` anchor
  exists.

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
- Add built-in `effigy distribution validate-metadata`,
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
  distribution validate-metadata`, and reduce
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
- Add built-in `effigy distribution preflight` with summary-file output for
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
- Retire redundant docs/contracts/distribution wrapper scripts and update
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

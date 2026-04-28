# Changelog

All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/).
During v0.x, MINOR bumps may include breaking changes.

## [Unreleased]

### Changed

- Explicitly enabled and tuned `opcache` in the `php-fpm` catalog dev image instead of relying on base-image defaults.
- DecodeLabs PHP workspaces now isolate hot dirs through a single `php-fpm` `isolated_dirs` list; the shipped Decodelabs app bundle uses `["vendor", "node_modules"]` and `decodelabs-library` uses just its repo-local `vendor/`.
- `php-fpm` now explicitly activates `pnpm` when Node.js is enabled and pins the pnpm store under `.effigy/runtime/pnpm/store` instead of letting `.pnpm-store` appear at the project root.
- `php-fpm` now exports `COMPOSER_CACHE_DIR=/home/dev/.cache/composer` so the existing shared Composer cache mount is the cache Composer actually uses.
- Imported manifest layers can now declare `[manifest].extend = ["..."]` so specific array paths append instead of replacing. That now covers normal includes, auto-discovered local overlays, and bundle-provided arrays like DNS routes.

### Fixed

- Gateway registration no longer crashes when the host container runtime is unreachable (e.g. colima/docker not installed in CI sandboxes or the daemon transiently down). Stale loopback-IP pruning becomes best-effort: when listing running containers fails, the prune is skipped that round and runs again next time.
- Linux clippy: `crate::resolver_setup` import in `effigy-gateway::server` is now gated behind `cfg(target_os = "macos")` to match the only call site, fixing an `unused-imports` failure on Linux builds.
- Generated compose containers now actually apply declared `[containers.<name>.host].mounts` onto generated services that already bind the repo root, so catalog-backed stacks can mount sibling checkouts and other external paths without ejecting to a hand-owned compose file.
- Deferred container tasks now prepare isolated workspace dirs before exec, fixing first-run permission failures on `decodelabs-library` `vendor/` volumes. The `decodelabs-library` bundle also now derives a repo-specific default `project_name` from `workspace_subdir`, so sibling library repos no longer share the same isolated `vendor` volume unless they opt into a shared project explicitly.
- DecodeLabs PHP workspace defaults now include `sockets`, `bz2`, `curl`, `gmp`, `imagick`, `mbstring`, `readline`, `sqlite3`, and `xml` in addition to the existing `mysqli` / `pdo_mysql` MySQL support.
- `php-fpm` now explicitly sets `short_open_tag = Off` in its dev ini instead of inheriting the base-image default implicitly.

### Breaking
- Reject `concurrent = [...]` on a task that does not declare
  `mode = "tui"`. The old behaviour silently ignored concurrent
  entries on non-TUI tasks, so sidecars declared on `tasks.dev`
  without an explicit mode never ran. The error message points
  consumers at either setting `mode = "tui"` or moving the entries
  onto `[[containers.<name>.host_processes]]` if the sidecar should
  follow the container's lifecycle instead of a TUI session.
- Remove legacy `[bundle].name` support. The only built-in bundle selector
  keys at manifest level are now `base` for shipped bundles and `base_path`
  for local bundle directories.
- Remove implicit root deferral based on `composer.json` + `effigy.json`.
  Legacy PHP repos now need an explicit `[defer]` block or a shipped bundle
  that provides one; legacy markers alone no longer hide `release` or route
  missing selectors through Composer-global Effigy automatically.
- Reshape the `effigy.init.v1` JSON payload to carry a `files[]` array
  (with per-file `target` / `path` / `contents` / `existed` / `written`) plus
  a top-level `guidance` string, replacing the single-file `path`/`content`
  fields. Adopters consuming `--json` output need to read from `files[0]`
  for the minimal starter.
- Move default workspace container config onto `[systems.<name>]` itself,
  replacing the extra `[systems.<name>.workspace_defaults]` layer, and route
  workspace `working_dir`, `user`, `home`, and `mounts` resolution through
  that system-level config plus per-workspace overrides.
- Flatten managed dev-task settings onto `[tasks.<name>]` itself and remove
  the nested `[tasks.<name>.managed]` table; `container_lifecycle`, `gateway`,
  `health_wait`, and `ready_message` now live directly on the task.
- Move generated compose runtime artifacts out of `infra/dev` and into
  `.effigy/runtime/compose` by default; only user-owned direct-compose output
  remains under `infra/dev` after an explicit eject.
- Replace task-level `host = true` with `run_in = "host" | "container" |
  "either"`. `host` is no longer accepted in `[tasks.<name>]`; `either` is the
  default transparent routing mode.

### Added
- Resolve registered gateway routes whose domain falls outside the
  managed TLD (e.g. `dev.cumberland.co.uk`). The DNS resolver now
  consults the route table for any A query and answers when a route
  exists — not only `.test`-style queries — and returns NoError on
  matching AAAA queries so browsers don't retry through upstream
  DNS and bypass the local override. The gateway daemon (which runs
  as root through the existing elevation flow) reconciles macOS
  `/etc/resolver/<domain>` files on startup and after every
  `routes.json` change, writing one per non-managed-TLD route domain
  and removing files when a route disappears. Files carry the
  `Managed by Effigy gateway` header so the gateway-down cleanup
  can sweep them safely without touching unrelated user-authored
  resolver entries. Lets a manifest declare a `domains = [...]`
  list under a public DNS suffix and have those names resolve
  locally with no per-machine `/etc/hosts` edits.
- Auto-mount the host's mkcert root CA into workspace catalog
  containers and install it into the system trust store on container
  start, so HTTPS calls from inside the container back through the
  host gateway (e.g. a PHP migration sync task hitting
  `https://dev.cumberland.co.uk/...`) trust the gateway's
  mkcert-issued certs without per-project glue. Effigy locates the
  cert via `mkcert -CAROOT`, mounts `rootCA.pem` read-only at
  `/usr/local/share/ca-certificates/effigy-mkcert.crt`, and the
  catalog's `effigy-entrypoint` wrapper runs
  `update-ca-certificates` on startup. Active for the `php-fpm` and
  `workspace-rust-bun` catalogs (whose entrypoints wire the install
  step). Silently skipped when mkcert is not installed or has not
  generated a root CA. Per-service opt-out via
  `params.mount_host_mkcert_ca = false`.
- Add `[[containers.<name>.host_processes]]` for declaring host-side
  sidecar processes whose lifecycle follows the container. Each entry
  has a `name`, a `run` shell command, and optional `restart`
  (`on-failure` default, `always`, `never`), `restart_delay_ms`,
  `shutdown_signal` (`SIGTERM` default, plus `SIGINT` / `SIGHUP` /
  `SIGKILL`), and `shutdown_grace_secs` knobs. Effigy spawns a
  detached supervisor per entry after `compose up` succeeds and tears
  it down before `compose down`/`reset`/attached-session shutdown,
  with PID and combined stdout/stderr written to
  `.effigy/runtime/host-processes/<container>/<name>.{pid,log}`.
  Replaces the `concurrent`-on-dev-task pattern for sidecars (e.g.
  `autossh` tunnels backing `target_host` gateway routes) that need
  to be tied to the container rather than to a managed dev TUI
  session.
- Accept a structured table form on `[containers.<name>.host].mounts`
  alongside the legacy `"host:container[:options]"` string form, with an
  `external = true` opt-in for sourcing mounts from outside the repo
  root. The structured `host` field also supports `${VAR}` (process env)
  and `~` expansion. Without `external = true`, the existing
  repo-relative containment policy still applies; with it, the source
  may live anywhere on disk but must canonicalise to an absolute path
  (use `~/...`, `${VAR}`, or a literal absolute path). Host-mount
  validation now happens at policy-load time instead of later
  validation, so misconfigurations surface earlier.
- Add `domains = [...]` and `domain_defaults = { ... }` sugar on
  `[containers.<name>.dns]`. Each domain in `domains` expands into a
  route inheriting `tls`, `port`, and `service` from `domain_defaults`,
  letting overlays grow a flat list of public-facing names without
  restating the per-route shape. A literal entry in `routes` with the
  same domain wins over its sugar form, so power users can still
  override individual entries.
- Add `target_host = "host:port"` to `[containers.<name>.dns]` routes
  and `domain_defaults`. When set, the gateway registers the route
  directly against the named host listener instead of resolving against
  a container service, so domains can be fronted by sidecar processes
  (e.g. an `autossh` tunnel running as a managed concurrent task) without
  requiring a compose service to back them. `target_host` is mutually
  exclusive with `service` on the same scope, and the value must parse as
  `host:port` with a u16 port; both rules surface as manifest validation
  errors rather than runtime failures.
- Accept `run_in = "host"` on `[[tasks.<name>.concurrent]]` entries.
  When set, the entry runs its `run` command on the host shell rather
  than inheriting the parent task's container wrap (compose exec /
  workspace handoff prefix). Pairs with the `target_host` route directive
  to let a host-side sidecar (e.g. autossh) back gateway routes for a
  container project. Rejected on `role = "lifecycle"` and `role =
  "shell"` entries, which by definition own the container handoff.
- Auto-discover `effigy.local.toml` alongside the root manifest. When
  present, it loads as if the root declared
  `{ path = "effigy.local.toml", optional = true }` at the end of its
  include list — the local file always wins over committed layers, can
  carry its own `[manifest].include` block, and is detected by canonical
  path so explicit declarations don't double-merge. Set
  `EFFIGY_NO_LOCAL_OVERLAY=1` to skip auto-discovery for CI determinism.
  The first time auto-discovery activates against a `.git` repo, Effigy
  idempotently amends the repo's `.gitignore` so the local fragment is
  never committed accidentally.
- Add `optional = true` on manifest include specs. When the flag is set
  and the include file does not exist, the include is silently skipped
  rather than raising a manifest error. Present optional files load and
  merge normally.
- Add `extend = [...]` array merge directive to manifest include specs.
  Listed paths in the included file get appended onto the parent's array
  instead of being rejected as a conflict, so layered overlays can grow a
  shared list without restating the existing entries. Validation rejects
  extending a non-array path and declaring the same path under both
  `override` and `extend`. Extending a path that didn't conflict (because
  the parent didn't have it yet) is treated as a successful no-op rather
  than an error, so `extend` can be declared prophylactically to guarantee
  append semantics even if a future root manifest grows the same path.

### Changed
- Move internal workspace crates onto shared workspace metadata so their crate
  versions now track the main Effigy release version instead of all reporting
  `0.1.0`, and mark the internal crates `publish = false`.
- Make the shared host gateway return `308` redirects from HTTP to HTTPS when
  a registered route sets `tls = true` and the HTTPS listener is available, so
  HTTP-oriented bundles pick up consistent TLS redirects without per-bundle
  config.
- Move the default `underlay` bootstrap contract into the shipped bundle
  and collapse `effigy init underlay` back to a single root `effigy.toml`.
  The bundle now owns the default env generation, sibling children, and
  `bootstrap deps sync ...` command built from the Underlay directory inputs,
  while split-out `effigy.bootstrap.toml` / `effigy.tasks.toml` becomes an
  opt-in decomposition pattern instead of the starter default.
- Add `EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE=auto|local|download` so
  workspace-container handoff can force Linux Effigy install artifacts to
  come from the local repo build or the published GitHub release cache
  without changing machine layout.
- Add `effigy container down --all` as the first cross-project container
  control surface, reusing the same running-environment discovery model as
  `status --all` and `stats --all`.
- Add a public `effigy defer <request> [args...]` builtin that runs the same
  explicit deferral surface automatic fallback uses, and make container-side
  deferral short-circuit to local execution when already inside an Effigy
  container handoff instead of trying to open a nested container session.
  The builtin now also has a dedicated `effigy defer --help` panel and is
  listed in general command help.
- Let tasks opt into staying in the workspace shell after a host-triggered
  container run with `stay_in_shell = true`. The bundled `decodelabs` `seed`
  task now uses this and explicitly declares `run_in = "container"`.
- Add a shipped `decodelabs-library` bundle for shared DecodeLabs library
  repos: one php-fpm workspace container, no default web/db/gateway services,
  and the same container-side Composer-global Effigy deferral contract as
  `decodelabs`. Its shared-root mount now defaults to `../` relative to the
  consuming repo, and it carries the same Node toolchain defaults as
  `decodelabs` (`node_version = "20"`, `node_global_packages = ["eclint"]`).
- Move host-triggered container leases onto a shared runtime identity keyed by
  Colima profile + compose project + container name, so compatible repos can
  refresh and observe the same temporary lease instead of each repo tracking
  its own private timeout state.
- Show a temporary spinner/progress line when a host-triggered deferred task
  has to start a stopped container environment before running the deferred
  command.
- Add an explicit container-side `[defer]` block to the shipped `decodelabs`
  bundle and its exported `base_path` template, routing unresolved selectors
  through `composer global exec effigy -- {request} {args}` inside the
  workspace container instead of relying on host-side legacy fallback.
- Let explicit `[defer]` blocks declare `run_in = "host" | "container" | "either"`.
  Omitted `run_in` stays host-only for backward compatibility. `either`
  reuses normal workspace-container binding when a default target exists and
  otherwise falls back to the current host deferral path.
- Add a bundled `release` task to the shipped `decodelabs` bundle and its
  exported `base_path` template, delegating to the legacy Composer-global
  `effigy` binary with `composer global exec effigy -- release`.
- Raise the shared `php-fpm` dev defaults to `upload_max_filesize = 256M`,
  `post_max_size = 256M`, and `date.timezone = UTC`, and let the catalog mount
  a caller-specified host source instead of always binding the consuming repo
  root directly.
- Add an `EFFIGY_COMPOSE_BACKEND` env override so operators can force
  Effigy onto `docker` or the Colima `nerdctl` / containerd path without
  changing what binaries happen to be on `PATH`.
- Add macOS-only `EFFIGY_COLIMA_ARCH` and `EFFIGY_COLIMA_VM_TYPE` overrides
  so Effigy can force Colima startup/profile settings like `aarch64` + `vz`
  without affecting Linux hosts.
- Raise the Colima startup/status command time budgets so first-run VM boot
  and provisioning on slower machines do not fail after the old 15s cap.
- Make `effigy bootstrap <git-url>` run the repo's configured
  `[bootstrap].start` task by default after bootstrap setup completes.
  Pass `--no-start` to skip that final launch step.
- Bump the default `workspace-rust-bun` catalog toolchain from Rust 1.88 to
  Rust 1.91 so underlay-style workspace containers can build current AWS SDK
  crate releases without per-repo toolchain overrides.
- Move `bootstrap:local` onto a dedicated `target/bootstrap-local` Cargo
  target dir so local binary refreshes stop fighting the shared workspace
  `target/` cache and remain fast/predictable even when the main build tree
  is noisy or wedged.
- Let the shipped `decodelabs` and `underlay` bundles accept
  `databases = ["main", "test", ...]` alongside the legacy singular
  `database = "main"` input, hydrating the singular primary database from the
  first list entry and carrying the full list through to MariaDB/Postgres
  init-time database creation.
- Let the shipped `underlay` bundle carry explicit front/admin UI package-dir
  mapping for the bundled `ui-setup.rhai` helper and optional per-role route
  labels, so polyrepo consumers can stop relying on the default
  `app-*` / `acme-*` package-name guesses.
- Add a shipped `seed` task to the `decodelabs` bundle, backed by a bundled
  `seed-latest-db-dump.rhai` helper that imports the newest
  `.effigy/local/db-seeds/<database>-*.sql` dump into the primary MariaDB
  database from bundle config.
- Add a `decodelabs` nginx config variant and point the `decodelabs` bundle at
  it. The rendered config now only rewrites every request to
  `/vendor/genesis.php` and hands off to php-fpm — no try_files, asset
  caching, or security locations, since DecodeLabs apps handle routing,
  static assets, and error pages in PHP. The `rewrite_all_to`,
  `asset_fallback`, and `error_page_404` nginx params are no longer set by
  the `decodelabs` bundle (consumers of the default config can still set
  them).
- Add manifest-scoped `[task_defaults].run_in` so one imported/catalog
  `effigy.toml` can set the default execution context for its own tasks
  without repeating `run_in` per task. Task-level `run_in` still overrides
  the manifest default.

### Fixed
- `effigy distribution` metadata validation now resolves
  workspace-inherited Cargo fields (`version.workspace = true`,
  `license.workspace = true`, etc.) against `[workspace.package]`
  before checking semver / license / tag-vs-version. Previously the
  validator treated the inheritance marker as the literal value, so
  the `effigy` root crate (which adopted shared workspace metadata
  recently) failed validation with `package version is not
  semver-like:` / `package license is empty` / `tag version
  0.2.13 does not match Cargo version` on every release-gate
  preflight run.
- Honour user-supplied `databases = [...]` on the `mariadb` and `postgres`
  catalogs when no explicit `database` was set. The compose-fragment
  `MYSQL_DATABASE` / `POSTGRES_DB` env var now reflects `databases[0]`
  rather than staying pinned to the schema default `"app"`. The bug was
  in param-resolution ordering: `normalize_database_params` ran after
  schema defaults were merged, so `database` was always present (as
  `"app"`) and the "derive from `databases[0]`" branch never fired.
  Normalisation now runs against raw user-and-variant params before
  schema defaults backfill.
- Reclaim stale gateway loopback-IP assignments from the persisted registry
  before allocating new DNS-only service aliases, using the live compose
  project set as the primary signal. This stops old temp repos and dead
  projects from exhausting the bounded `127.1.0.1–127.1.0.50` pool and
  blocking new underlay-style containers like `contact-patch`.
- Make managed `health_wait = true` gate lifecycle readiness on the task's
  declared DNS routes instead of just printing a label. Underlay-style `dev`
  stacks now wait until the gateway stops returning upstream startup errors
  like `502` before the lifecycle pane flips to ready.
- Stop the workspace permission prep from failing wholesale when the host
  gitconfig / SSH known_hosts read-only bind mounts land under
  `/home/dev`. The chown step now uses `chown -fR ... || true` so the
  per-entry "Read-only file system" failures on the bind-mounted files
  are silently tolerated. Real misconfigurations (missing target,
  unwritable parent) still fail via `mkdir -p` and the loop
  scaffolding.
- Emit the host SSH agent forward unconditionally when
  `forward_host_ssh_agent = true` (the default). The previous
  implementation stat'd `/run/host-services/ssh-auth.sock` from the
  Effigy process on macOS, but that path only exists inside the Colima
  VM where compose runs — so the host-side check always failed and the
  mount was silently skipped, leaving `git push` over SSH to die with
  `Permission denied (publickey)`. Now we trust Colima to honour the
  mount; if the agent isn't actually forwarded, compose-up fails loudly
  instead of leaving ssh keyless at runtime. Opt-out via the catalog
  param.
- Bridge the forwarded host SSH agent socket through `socat` inside the
  `php-fpm` and `workspace-rust-bun` catalog containers so the non-root
  workspace user can actually use it. Colima creates the forwarded
  socket as `root:root` mode 0600 inside its VM, which left ssh-add
  reporting `Error connecting to agent: Permission denied` even after a
  plain `chmod 0666` from the container's entrypoint (Colima may harden
  the socket in ways that defeat in-container chmod). The catalog
  images now ship `socat` plus an `effigy-entrypoint` wrapper that
  starts a bridge from `/run/host-services/ssh-auth.sock` to a
  workspace-user-owned `/tmp/effigy-ssh-auth.sock` on container startup.
  `SSH_AUTH_SOCK` is injected pointing at the bridge. The wrapper also
  writes a startup log to `/var/log/effigy-ssh-bridge.log` for
  diagnosability.
- Set `git config --system --add safe.directory '*'` in the `php-fpm`
  and `workspace-rust-bun` catalog images so root-side tooling
  (`effigy prep`, composer-invoked git, etc.) stops hitting git's
  "dubious ownership" guard on bind-mounted workspace and library
  paths owned by the host UID. Trust is implicit in a dev container,
  so the system-wide allowlist is the right blanket fix.
- Pass `--ssh-agent` to `colima start` and persist `sshAgent: true` in
  the managed Colima profile YAML so the host SSH agent socket is
  actually forwarded into the VM at `/run/host-services/ssh-auth.sock`.
  Without this, the workspace agent-socket bind mount landed on a
  non-existent source path; Docker autocreated an empty *directory* in
  its place, the in-container bridge never started, and `ssh-add -l`
  inside workspace shells reported `Error connecting to agent: No such
  file or directory`. Existing running Colima profiles need a one-time
  `colima stop --profile <profile>` for the new flag to take effect.
- Disable `health_wait` in the
  `managed_stream_derives_ready_message_from_dns_routes` test fixture.
  Commit 247198fb added a real curl-based readiness probe loop to
  `managed_lifecycle_command`, which spins forever against the test's
  fake docker runtime (no HTTP server is listening), so the lifecycle
  process gets torn down by `window`'s `shutdown_on_exit` before the
  `managed ready: routes:` and `dns_routes:` banners ever print. Probe
  behaviour has its own unit-level coverage in
  `managed_lifecycle_command_waits_for_probe_urls_before_ready`; this
  test only exercises ready-message + dns_routes banner derivation,
  which is orthogonal to whether `health_wait` is enabled.
- Realign the deferral loop-guard test with the implicit-fallback policy
  added in 0.2.13. When `EFFIGY_DEFER_DEPTH` is set we're inside a
  deferred subprocess, so `should_attempt_deferral` correctly refuses to
  attempt another deferral and lets the original `TaskNotFoundAny`
  surface — that's a cleaner diagnostic for the common typo'd-task case
  than `DeferLoopDetected`. The stale test was still asserting the
  pre-policy `DeferLoopDetected` outcome on the implicit path; rename
  and update it to match the new contract. Defense-in-depth
  `DeferLoopDetected` still fires for the explicit `effigy defer ...`
  path, which bypasses the policy and goes straight to
  `run_deferred_request`'s depth check.
- Stop the bootstrap deps-sync runner-shell tests from racing with
  neighbouring tests on the process-wide `PATH` env var. The three
  `run_bootstrap_with_cwd_*` tests prepend a fake `bin_dir` so they can
  observe `bun`/`cargo` invocations, but `bootstrap_deps_json_contract_*`
  also mutates `PATH` (and `cwd`) and was not on the same lock, which
  occasionally let real `cargo` slip in mid-test and trip "no targets in
  manifest" errors. The bootstrap shell tests now acquire the shared
  `contract_test_support::lock_test()` mutex before mutating `PATH`, and
  use a small `PathPrepend` RAII guard so the prior value is restored
  even on panic.
- Anchor the `preflight_recommends_native_first_publish_command`
  distribution test on `CARGO_MANIFEST_DIR` rather than `Path::new(".")`,
  so the test no longer races against `set_current_dir` in
  `defer_command`, `builtin_contract_tests`, or other neighbours under
  cargo's parallel execution.
- Serialize the two `should_stay_in_workspace_shell` standard-pipeline
  tests on a shared `OnceLock<Mutex<()>>` and wrap the `EFFIGY_INTERNAL_
  CONTAINER_HANDOFF` env mutation in an `EnvRestore` RAII guard, so the
  pair stops corrupting each other when run in parallel.
- Let VT-backed managed TUI panes wrap rendered rows to the tab width again
  while still keeping the wider PTY buffer intact, so long real log lines
  stop being clipped without bringing back Bun-style progress row history
  smearing.
- Align the shipped underlay bootstrap env helper with the actual postgres
  catalog password default (`secret`) so generated `DATABASE_URL` values stop
  drifting from the running stack contract.
- Make sibling service params in catalog assembly include schema defaults, so
  generated dependents like phpMyAdmin can inherit defaulted values such as
  the bundled MariaDB `password` instead of seeing them as unset.
- Run bundled Rhai setup hooks through a PTY in interactive terminals so
  tools like `bun install` keep their normal progress output in managed dev
  tabs without needing extra verbosity flags.
- Derive the managed lifecycle ready banner from configured container DNS
  routes when a task does not set `ready_message`, so bundle-backed dev tasks
  surface their gateway URLs without hand-maintained manifest strings. The
  lifecycle tab also lists the effective DNS routes and service aliases.
- Add `effigy bundle export <name> --path <dir>` so teams can write a
  compiled-in shipped bundle to a local `base_path` bundle directory and then
  own local modifications explicitly.
- Automatically add `.effigy` to `.gitignore` when Effigy creates project-local
  runtime state inside a Git root.
- Add a first-class Rhai `container_exec(...)` host helper that runs commands
  through Effigy's container transport and returns structured status/stdout/
  stderr without recursively invoking the Effigy CLI from scripts.
- Expand the Rhai host API with typed helpers for task discovery, container
  status/logs/data/reset/eject/stats, docs checks, bundle/service/catalog
  inspection, gateway status/setup, doctor, scan, and cache operations, plus

### Fixed
- Give `php-fpm` workspace containers an Effigy-managed shared Composer auth
  / config / cache layer by default instead of mounting the host Composer
  home. This keeps GitHub tokens and cache reusable across containers without
  tying container tools to host-side Composer installs.
- Stop mounting the host Composer home into `php-fpm` workspace containers
  unless `mount_host_composer_home = true` is set explicitly. This keeps
  container-side global Composer tools available even after legacy host-side
  Composer installs are removed.
- Stop prefixing managed TUI stderr lines with `[stderr]`, so package-manager
  and build-process output that legitimately streams on stderr renders plainly
  in tabs, failure tails, and saved transcripts.
- Render VT-backed managed TUI panes from the PTY screen width instead of the
  narrow tab width, so in-place ANSI progress rows from tools like Bun stop
  being split into appended wrapped fragments.
- Render bootstrap progress lines with the normal colored status prefixes and
  clearer phase spacing, so checkout/setup/start output no longer mixes muted
  spinner completion lines with plain unthemed follow-up logs.
- Prefer live local Effigy checkouts over the persisted source pointer when
  choosing the Linux workspace binary in `auto` mode, so switching back from a
  downloaded release artifact stops leaving stale cached binaries in workspace
  containers.
- Refresh the workspace Effigy binary from the local Linux rehearsal path
  before `effigy container shell` and `effigy container shell --command` enter
  workspace-style containers, so managed `dev` tabs stop reusing stale
  in-container binaries after switching back from downloaded artifacts.
- Add `bcmath`, `mysqli`, and `event` to the shared DecodeLabs PHP extension
  defaults, so both `decodelabs` and `decodelabs-library` containers match
  the current app/runtime requirements without per-repo overrides.
- Add a first-pass cross-repo isolation contract: producer repos can declare
  `[isolation].paths`, and consumer systems can adopt those paths with
  `systems.<name>.isolation = [{ repo = "../library" }]`. Direct-compose
  workspace runtimes now overlay those adopted paths from
  `.effigy/runtime/isolation/...`, keeping source shared while isolating
  install/build directories inside the container.
- Auto-adopt mounted sibling repo isolation contracts in direct-compose
  workspace runtimes, so underlay-style consumers do not need to repeat the
  same repo list under both `systems.<name>.mounts` and
  `systems.<name>.isolation`. The isolation overlays now render as named
  volumes instead of long host bind mounts, which keeps the Colima
  `nerdctl/mounts` label under containerd's 4096-byte limit on repos like
  `underlay-reference`.
- Restore per-subproject `target/` and `node_modules/` named-volume injection
  on workspace-rust-bun containers via two new catalog params
  (`cargo_target_dirs`, `node_modules_dirs`) populated by the shipped
  `underlay` bundle from `[bundle.dirs]`. This keeps cargo build output and
  Bun-installed deps on Linux-native ext4 storage, working around the Colima
  virtiofs file-locking limitation that broke `cargo build` on
  acowtancy-style underlay sites with `Permission denied` on `.rcgu.o`
  removal. The catalog assembler now also auto-discovers named-volume
  references from rendered service definitions and emits matching top-level
  declarations.
- Consolidate database superuser credential defaults across the shipped
  catalogs: rename the `mariadb` catalog param `root_password` to `password`
  so it matches `postgres`, collapse the dbgate/phpmyadmin sibling-password
  resolvers onto a single `params.password` lookup, and drop the underlay
  bundle's hardcoded `password = "postgres"` override so postgres falls
  through to the catalog's `secret` default. MariaDB and Postgres now both
  default to `secret` for the engine-canonical superuser (`root` and
  `postgres` respectively); the upstream image conventions for usernames are
  preserved. Minio keeps `root_password` since `MINIO_ROOT_PASSWORD` is the
  canonical upstream env var name.
- Stop non-managed container-backed tasks with a real `run` command from
  being intercepted into the workspace shell handoff path. Bundle tasks like
  `decodelabs` `seed` now execute their configured command inside the
  container instead of just opening the container shell from the host.
- Make the bundled `decodelabs` seed helper choose the latest `.sql` dump in
  `.effigy/local/db-seeds` by filename sort order instead of requiring dump
  names to start with the local project or database name.
- Prefer Composer's global `vendor/bin` ahead of the workspace-installed
  Effigy binary when running handoff-local deferred commands like
  `composer global exec effigy -- ...` inside legacy container shells, so
  DecodeLabs deferral targets resolve to the intended legacy handler instead
  of looping back into the real Effigy binary.
- Refuse automatic nested deferral whenever `EFFIGY_DEFER_DEPTH` is already
  set, so handoff-local deferred commands that accidentally resolve back to
  the real Effigy binary fail as plain missing-task lookups instead of
  tripping the deferral loop error again.
- Switch the shipped `decodelabs` and `decodelabs-library` legacy handoff
  commands from `composer global exec effigy -- ...` to the direct Composer
  global bin path `${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy`
  so in-container `prep`/`release` style deferrals bypass Composer's own
  command resolution layer and hit the intended legacy Effigy handler.
- Add a Rhai `trim_string(...)` host helper and switch shipped starter
  scripts onto it so bundle helpers stop breaking on Rhai's in-place
  `trim()` semantics, which return `()` instead of a trimmed string.
- Restore the resolve-only `tasks --resolve <selector>` text shortcut so that
  running `effigy tasks --resolve` without a filter renders just the
  `Resolution:` block instead of the full Catalogs / Tasks / Built-in Tasks
  listing. The pre-refactor shortcut was lost when tasks listing moved into
  the `effigy-tasks` crate.
- Keep seeded bootstrap start tasks and workspace handoff internals pinned to
  the cloned repo root even when the original `effigy bootstrap ...` command
  was launched from outside that repo, so the final `dev` start step no longer
  falls back to resolving container commands from the outer shell cwd.
- Resolve the underlay bundled `ui-setup.rhai` package-dir probes from the
  owning repo root instead of the launched child process cwd, so explicit
  `[bundle.dirs]` mappings like Acowtancy's `cream` / `dairy` / `froyo`
  hydrate correctly inside workspace containers.
- Stop in-process builtin task references from reconstructing CLI argv with a
  stale `--repo` global flag path, so bootstrap run steps like
  `container up --detach` execute against the target repo instead of failing
  with `unknown argument: --repo`.
- Fail container-policy validation early when the Colima nerdctl compose
  fallback targets a repo under a temp directory like `/tmp` or
  `/private/tmp`, so bootstrap explains the unsupported host path clearly
  instead of surfacing an opaque compose-file ENOENT from inside Colima.
  The check now runs as a separate `validate_compose_backend_runtime`
  preflight from the runtime call sites that actually drive `docker
  compose`, leaving file-only operations like `container eject` free to
  rewrite manifests under temp directories during tests.
- Restore explicit empty-catalog rows in the `effigy tasks --json`
  `catalog_tasks` array (with `task` / `run` set to `null`) and emit
  manifest paths as absolute paths so JSON consumers can match catalogs by
  on-disk location, fixing the regression introduced by the recent tasks
  listing projection refactor.
- Force workspace handoff shells and in-container Effigy handoff exec to use
  the installed `/usr/local/bin/effigy` path ahead of Composer global bins, so
  legacy Decodelabs sites can still defer missing tasks to the old Composer
  package without that package shadowing the real Effigy binary completely.
- Run bootstrap builtin steps like `bootstrap deps sync ../underlay` against the
  cloned repo root instead of the outer invocation cwd, so sibling bootstrap
  paths resolve relative to the bootstrapped repo rather than the parent
  directory you launched `effigy bootstrap` from.
- Mount MariaDB and Postgres extra-database init scripts from the generated
  `.effigy/runtime/compose/*.conf` path instead of the stale per-project
  runtime path, so bundle-generated database services no longer bind a
  directory over `/docker-entrypoint-initdb.d/10-extra-databases.sql` and fail
  at startup.
- Treat `container_lifecycle = true` as an implicit container run target when
  a task does not set `run_in`, so manifest-wide `[task_defaults].run_in =
  "host"` no longer breaks managed dev tasks unless they explicitly opt back to
  host execution.
- Preserve default workspace `working_dir` inference for container exec/CWD
  mapping even when repos set `[task_defaults].run_in = "host"`, so
  bundle-backed stacks like `underlay-reference` still inherit the generated
  underlay workspace path.
- Update `bootstrap:local` installs with a temp file plus atomic rename instead
  of copying over the live `.local-install/bin/effigy` path in place, which
  avoids corrupting the running local binary during self-hosted refreshes.
- Finish the postgres/mysql service-alias rename on the runtime and gateway
  paths so generated stacks register `postgres.<domain>` and `mysql.<domain>`
  consistently instead of still leaking the old `db.<domain>` alias.
- Define the forwarded FastCGI variables in the `decodelabs` nginx variant so
  generated `web` containers start cleanly instead of crashing on unknown
  `$fastcgi_*` variables during nginx config load.
  a maintained audit matrix and static guard against recursive Effigy calls in
  first-party Rhai scripts.
- Add Rhai `http_get(...)`, `http_post(...)`, and `http_request(...)` helpers
  for portable smoke probes without depending on host `curl`.
- Add Rhai `search_files(...)` for portable source audits without depending on
  host `rg`, shell, or `awk`.
- Reject recursive `run_process("effigy", ...)` and
  `run_process_stream("effigy", ...)` calls from Rhai scripts so missing
  runtime surface is handled by adding typed host helpers.
- Add a static first-party Rhai process-call allowlist so new
  `run_process(...)` dependencies are reviewed instead of added accidentally.
- Add Rhai `config_raw()`, `config_effective()`, and `config_get(path)` helpers
  so scripts can read Effigy's raw and composed/bundle-expanded manifest view
  without parsing `effigy.toml` themselves.
- Move Underlay error-reporting smoke, metrics, and validation helpers into
  the shipped `underlay` bundle as bundled Rhai tasks, so consumer repos do not
  need to carry their own `scripts/error-reporting.rhai` copy.
- Route Decodelabs bundle asset requests through `/vendor/genesis.php` instead
  of leaving `asset_fallback` empty, so browser subresource loads do not get a
  framework body wrapped in an nginx 404 status.
- Let `[bootstrap]` and `[[bootstrap.children]]` declare bootstrap-local
  `run = ...` definitions using the normal managed run grammar, with
  `setup = ...` retained as a compatibility alias. Bootstrap setup no longer
  needs to be modeled as task selectors in the task catalog.
- Allow `[[bootstrap.children]].path` to target sibling repos such as
  `../underlay`, as long as the resolved child stays within the root repo's
  parent directory.
- Add `bootstrap deps sync` for bootstrap-local dependency hydration,
  covering `package.json` installs through the nearest manifest's
  `[package_manager].js` and `Cargo.toml` fetches without shelling out to
  ad hoc `bun install && cargo fetch ...` chains.
- Refresh the guides hub (`docs/guides/README.md`) and primary surface-area
  guides (012, 017, 022, 025, 026, 062, 063, 064, 065) to document the
  shipped v0.3+ surface: flattened `[systems.<name>]` substrate config,
  flattened managed dev-task fields (`container_lifecycle`, `gateway`,
  `health_wait`, `ready_message`), the generated-compose runtime location
  (`.effigy/runtime/compose/`), TCP DNS aliases + loopback IP allocation,
  the reshaped `effigy.init.v1` / `effigy.init.list.v1` JSON payloads,
  and the full `effigy system`, `effigy workspace`, and `effigy bundle`
  command surfaces.
- Archive five superseded guides under `docs/guides/archive/` with
  deprecation markers pointing to their replacements: `028-docs-flow-map`,
  `031-docs-navigation-cleanup`, `032-docs-consistency-sweep-and-changelog`,
  `043-wrapper-channel-evaluation-and-policy`, and
  `053-release-wrapper-retirement-record-template`. Inbound references
  from active guides have been redirected through `archive/`.

### Added
- Install `openssh-client` in the `php-fpm` and `workspace-rust-bun` catalog
  base images so `git push` over SSH actually works inside the container.
  Without this git fails with `error: cannot run ssh: No such file or
  directory` even when the host SSH agent socket is forwarded.
- Mount the host developer's git identity and SSH access into workspace
  containers so in-container release tasks (e.g. `git push`) work without
  hand-configuring credentials per container. Adds three default-on params on
  the `php-fpm`, `workspace-rust-bun`, and `node` catalogs:
  `mount_host_git_config` (binds `~/.gitconfig` read-only at
  `/home/dev/.gitconfig`), `mount_host_ssh_known_hosts` (binds
  `~/.ssh/known_hosts` read-only at `/home/dev/.ssh/known_hosts`), and
  `forward_host_ssh_agent` (forwards Colima's
  `/run/host-services/ssh-auth.sock` and sets `SSH_AUTH_SOCK` accordingly).
  All three skip silently when the host source is absent; private keys
  themselves are deliberately not copied — the agent socket is the auth path.
  User-set `SSH_AUTH_SOCK` env values in the manifest take precedence over
  Effigy's runtime default.
- Add a user-global `~/.effigy/config.toml` configuration file with bundle-keyed
  `library_mounts` entries. When a project's manifest declares `[bundle].base
  = "<name>"` matching a `[bundle.<name>]` block in the user config, each
  listed parent directory is bind-mounted into the workspace container under
  `/workspace-libraries/<basename>`. Lets per-developer library checkouts
  (e.g. `~/Dev/legacy/libraries/decodelabs`) stay reachable from the legacy
  `effigy mount` command without committing host paths into the project's
  checked-in `effigy.toml`. Missing host paths are skipped silently;
  basename collisions across two declared parents are rejected with an
  error.
- Let the shipped `underlay` bundle infer sibling `underlay` / `poodle`
  sources from `systems.<name>.mounts`, with optional `[bundle.sources]`
  overrides only for ambiguous layouts, so nested consumer repos can drop
  repo-local `[bootstrap]` overrides without duplicating the same paths
  twice.
- Add a bundled underlay `bootstrap-env.rhai` helper and wire it into the
  starter bootstrap run so fresh underlay repos can create missing
  app-local `.env` files before the first container bring-up. The helper
  derives local route URLs from `[bundle]` / `[bundle.routes]`, preserves
  existing files, and generates local-only API auth/encryption secrets.
- Add configurability knobs for the shipped `underlay` and `decodelabs`
  bundles: `system_name`, `container_name`, `workspace_service_name`, and
  `default_workspace` are now optional `[bundle]` inputs (defaulting to
  the existing hardcoded values — `dev`/`stack`/`workspace`/`app` for
  underlay and `dev`/`web`/`app`/`app` for decodelabs) so consumer repos
  can rename the rendered system, container, workspace service, and
  default workspace without forking the bundle or switching to a local
  `base_path` copy.
- Add `dbgate` catalog fragment (`dbgate/dbgate:latest`) with row editing,
  foreign-key lookups, and spreadsheet-style grid view, and promote it as the
  default database UI in the shipped `underlay` bundle (published at
  `dbgate.<host>`). The existing `pgweb` fragment stays in the catalog as a
  lean read-oriented alternative.
- Add `docs/guides/067-catalog-services-reference.md` covering the full
  shipped catalog service surface (postgres, dbgate, pgweb, mariadb, redis,
  memcached, mailpit, minio, elasticsearch, phpmyadmin, nginx, php-fpm,
  workspace-rust-bun) with default images, configurable inputs, exposed
  ports, volumes, healthchecks, and gateway eligibility per service.
- Extend `docs/guides/065-underlay-starter.md` with a full Decodelabs
  bundle reference (PHP-FPM + nginx + MariaDB + phpMyAdmin + Memcached +
  Redis stack) so consumer PHP-native repos can adopt `base = "decodelabs"`
  from one guide without inspecting the bundle source.
- Add a bounded host-side TCP alias fallback for DNS-only service routes on
  the macOS Colima/nerdctl path: generated compose now keeps a dynamic runtime
  host binding for shipped TCP alias services, container route registration
  persists `tcp_port`/`tcp_target` metadata for DNS-only aliases, and the
  host gateway daemon now owns `127.1.x.x:<service-port>` listeners that
  forward to the runtime-discovered host port. This makes aliases such as
  `db.<app>.test:5432`, `smtp.<app>.test:1025`, and `s3.<app>.test:9000`
  reachable on the real bounded runtime path instead of only resolving in DNS.
  Multi-label hosts now keep their full alias domain shape
  (`db.contact-patch.legacy.test`, not `db.legacy.test`), and gateway
  registration prunes stale container routes for the same project before
  writing the current route set.
- Acknowledge Ctrl+C during `effigy container up` across both attached and
  detached bring-up: SIGINT/SIGTERM now surfaces a visible
  `[info] shutdown requested; stopping container cleanly...` line, and the
  bring-up unwinds through `compose down` plus gateway deregistration before
  returning control to the terminal. The detached path used to ignore the
  stop flag entirely while compose was still pulling images.
- Persist bounded gateway loopback-IP assignments in
  `~/.effigy/gateway/loopback-ips.json` and provision the macOS
  `127.1.0.1`–`127.1.0.50` alias range during the existing elevated
  `effigy gateway up` setup path, so later TCP-service DNS work can bind onto
  stable loopback addresses without requesting extra privilege during normal
  runtime.
- Change HTTP gateway registration to prefer live runtime published-port
  discovery over manifest-time host-port assumptions, so generated-compose
  services with ephemeral host ports can still register `.test` routes
  honestly after startup.
- Add bounded project-owned TCP service DNS alias registration on top of the
  loopback-IP gateway state, so shipped catalogs like Postgres, MariaDB,
  Redis, Memcached, MinIO, Elasticsearch, and SMTP-capable mail services can
  resolve stable DNS-only aliases such as `db.<app>.test` without pretending
  to be HTTP proxy routes.
- Add bounded shared-service DNS alias reuse on the same loopback-IP gateway
  substrate, so several consuming projects can resolve the same shipped alias
  shape like `db.<app>.test` onto one shared backing-service identity instead
  of allocating duplicate per-project loopback IPs for the same shared
  service.
- Add a first-class top-level `[bundle]` manifest surface with a bundled
  `decodelabs` resolver. Bundle defaults are applied after manifest
  composition as lowest-precedence config, so repo-owned `effigy.toml`
  blocks can override the resolved bundle shape directly without a second
  indirection layer. The canonical selector key is `[bundle].base`; legacy
  `[bundle].name` remains accepted as a compatibility alias.
- Add `[bundle].base_path` for repo-local bundle directories. A local bundle
  carries `bundle.toml` metadata plus a templated `effigy.toml` defaults file,
  validates declared inputs, and merges rendered defaults with the same
  lowest-precedence behavior as shipped bundles. Local bundle templates can
  reference bundled scripts and assets with `{{ bundle.root }}` so those files
  stay in the bundle source instead of being copied into each consumer repo.
  Shipped bundles materialize embedded assets under
  `.effigy/runtime/bundles/<bundle>/<hash>/`, and repo-owned run steps can
  reference the active bundle root with the same `{{ bundle.root }}` token.
- Add `effigy bundle list` and `effigy bundle inspect <name>` so users can
  discover shipped bundles and inspect both the accepted `[bundle]` input
  schema and the manifest paths each bundle injects by default, and expose the
  same bundle schema surface through `effigy config --schema --target bundle`
  with optional `--bundle <name>` detail.
- Add a bundled `underlay` manifest preset beside `decodelabs`. It owns the
  stable Underlay system/container shape: `package_manager.js = "bun"`,
  `systems.dev`, the `workspace-rust-bun` workspace service, bundled
  postgres/pgweb/mailpit/minio services, gateway app routes, and the current
  loopback-alias surface (`db.<host>`, `smtp.<host>`, `s3.<host>`). The
  `underlay` starter now emits a bundle-backed root manifest instead of a
  separate `effigy.system.toml`, so the starter, the config docs, and the
  `underlay-reference` example all point at one canonical shape.
- Register the `northstar` starter with `effigy init`: `effigy init northstar`
  now emits the single-repo Northstar + Effigy consumer contract (root
  `effigy.toml` with a starter `[docs_policy]` block and `qa` / `qa:docs` /
  `qa:northstar` task bundle, `README.md`, `AGENTS.md`, `CHANGELOG.md`, the
  four-file docs spine under `docs/{README, vision/README, roadmaps/README,
  logs/README}.md`, a first `docs/vision/001-product-vision.md`, and a
  starter `docs/policy/vision-next-task-verbs.txt`) plus a post-emission
  edit checklist. Lands as a pure content starter on top of the unified
  `effigy init <name>` loader — no command work was required.
- Register the `underlay` starter with `effigy init`: `effigy init underlay`
  now emits the three-file Underlay manifest shape (root `effigy.toml`,
  `effigy.bootstrap.toml`, and `effigy.tasks.toml`) and prints a
  post-emission edit checklist. The default UI setup helper stays in the
  shipped bundle and is referenced with `{{ bundle.root }}` instead of being
  copied into each consumer repo.
  Multi-file starters share one pre-scan: any existing target refuses the
  run without `--force`, and every conflict is listed.
  `--dry-run` emits each file under a `=== <target> ===` header; `--json`
  surfaces per-file records plus the starter's guidance string.
- Extend `effigy init` into a named-starter surface: accept a positional
  `<name>` (defaulting to `minimal`) and a `--list [--json]` mode that reports
  registered starters. The emit payload now includes a `starter` field and a
  new list contract ships as `effigy.init.list.v1`. The `minimal` starter is
  now sourced from the embedded catalog under
  `crates/effigy-catalog/starters/minimal/` via a `starter.toml` descriptor
  rather than a hardcoded inline scaffold.
- Add a bundled `workspace-rust-bun` service catalog fragment: a long-running
  Rust + Bun dev workspace container (Rust toolchain, Bun, non-root `dev` user
  aligned with host UID/GID, persistent cargo caches, `sleep infinity` shell
  target, parameterised host port bindings) so Underlay-style repos can drop
  their hand-written `workspace.Dockerfile` and `infra/dev/docker-compose.yml`
  and declare the workspace via `[containers.<name>.services.workspace] catalog
  = "workspace-rust-bun"` instead.
- Ship an Underlay starter fragment set under
  `crates/effigy-catalog/starters/underlay/` (root `effigy.toml`,
  `effigy.bootstrap.toml`, `effigy.tasks.toml`, plus a README) packaging the reusable
  Underlay shape on top of `systems` / `workspaces` / generated services /
  managed `dev`, with a composition proof test under
  `crates/effigy-manifest/tests/underlay_starter.rs` and an adoption guide at
  `docs/guides/065-underlay-starter.md`. Emission is now wired into
  `effigy init underlay` (see above).
- Add a bundled `phpmyadmin` service catalog fragment so repos can expose a
  local phpMyAdmin UI for MariaDB/MySQL stacks without carrying a project-local
  override.
- Add `list_dir(path)` plus `path_file_name(path)` to the Rhai host helper
  surface so repo-owned task scripts can inspect local drop folders like
  timestamped SQL seed bundles without shelling out just to enumerate files.
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
  task-level `container_lifecycle`, `concurrent` lifecycle roles,
  plan/schema/docs support, and managed runtime ownership for starting and
  stopping a repo-owned workspace-backed container through one task-owned
  lifecycle process.
- Add bounded `g02.013` managed shell-role support, so `concurrent` entries
  can declare `role = "shell"` and open the task-owned primary-service
  container shell through the shipped `effigy container shell` path.
- Add bounded `g02.013` managed readiness UX support, so repo-owned managed dev
  tasks can declare `health_wait` plus `ready_message`, render
  that contract in plan/docs/schema output, and project one honest ready
  message through the lifecycle-owned runtime path after detached container
  startup reaches ready state.
- Add bounded `g02.013` managed gateway auto-start support, so repo-owned
  managed dev tasks can declare `gateway = true`, validate that
  contract against lifecycle-owned workspace containers, render it in
  plan/docs output, and trigger the shipped `effigy gateway up` path before
  the managed runtime starts.
- Add explicit `setup = [...]` support on standard managed `concurrent` panes,
  including `run`, `task`, and `rhai` step sequences, so repos can own
  transparent per-pane hydration and other pre-run logic directly in
  manifest config instead of relying on hidden runner heuristics.
### Fixed
- Probe the running workspace service architecture before injecting the
  container-side `effigy` binary, and select the matching Linux GNU artifact
  (`x86_64` or `aarch64`) instead of always copying the x86_64 build into ARM
  workspace containers; workspace handoff now also prefers a persisted host
  source pointer at `~/.effigy/source.toml` before falling back to local repo
  heuristics or release downloads.
- Reclaim stale underlay UI setup locks, verify package.json dependencies
  before treating isolated Bun `node_modules` trees as hydrated, and run
  `svelte-kit sync` through Bun itself so front/admin panes do not hang
  forever on a dead lock, skip installs against empty isolation volumes, or
  fail on workspaces without `node`.
- Stop non-managed `workspace = ...` tasks from reopening the workspace shell
  when they are invoked from inside an active workspace handoff, so commands
  like `effigy seed` run in place instead of trying to call host-only Colima
  tooling from inside the container.
- Preserve HTTPS awareness for PHP apps behind the local TLS gateway by
  forwarding the correct `X-Forwarded-Proto` value from the HTTP vs HTTPS
  listener and translating forwarded HTTPS into FastCGI `HTTPS`,
  `REQUEST_SCHEME`, and `SERVER_PORT` values in the bundled nginx configs.
- Change bundled MariaDB and Postgres storage from hidden Docker named volumes
  to repo-local bind mounts under `.effigy/runtime/data/<service>/...`, so DB
  state stays visible and project-scoped by default instead of living in
  runtime-owned volume state.
- Let the bundled `phpmyadmin` service inherit a sibling MariaDB/MySQL root
  password by default, with an explicit override still available when needed,
  so local DB admin UI access stays aligned with the actual database service
  config instead of duplicating passwords in two service blocks.
- Let container exec alias commands render against declared sibling service
  config, so repo manifests can derive commands like `mysql` from the actual
  DB service database/password params instead of copying credentials into a
  second static alias string.
- Add layered Composer-home support for bundled `php-fpm` workspaces: repos
  can opt PHP services into host Composer-home mounting when available, while
  the image still carries a fallback internal Composer home plus configurable
  global packages; fallback global installs now treat those packages as
  trusted and enable Composer plugins automatically inside the container-owned
  Composer home during image build.
- Extend bundled `php-fpm` Node support so enabling Node also enables
  Corepack-backed `pnpm`, and let repos request npm global tools like
  `eclint` through `node_global_packages`.
- Let catalog services use the existing `variant` key for parameter presets as
  well as config-file variants, and ship a bundled `php-fpm` `decodelabs`
  preset covering the standard DecodeLabs PHP workspace defaults.
- Add the same preset handling to bundled `nginx` `decodelabs`, so the
  variant also applies the repo-root document root and `/var/www/html`
  working-dir defaults instead of repeating them in each manifest.
- Infer generated-service `working_dir` from `[containers.<name>].working_dir`
  whenever the target catalog service supports that param, so repos only need
  to declare the path once instead of repeating it across service and
  workspace config.
- Point generated service Dockerfile paths at the actual generated catalog
  artifact directory under `.effigy/runtime/compose/.effigy-catalog` instead of the stale
  repo-root `.effigy-catalog` compatibility path, so rebuilt workspace images
  use the current catalog Dockerfiles.
- Regenerate catalog-managed Dockerfiles and config files when their rendered
  contents change even if the manifest checksum and compose YAML stay the same,
  so bundled service image updates like the PHP workspace `dev` user actually
  reach generated stacks on the next `up --build`.
- Force generated compose stacks to recreate services on `up --build`, because
  some backends keep existing containers pinned to the previous image even
  after rebuilding the same local tag.
- Default catalog-backed workspace systems onto a non-root shell identity when
  the repo does not declare one, using `dev` for bundled `php-fpm` and `node`
  for bundled `node`, so workspace/dev handoff no longer drops into root on
  those shipped container shapes by default.
- Run bundled `php-fpm` HTTP workers as the same `dev` workspace user used for
  shell handoff, so bind-mounted project runtime paths stay writable from web
  requests instead of only from the interactive container shell.
- Remove the `workdir` spelling from `[systems.<name>]` and
  `[systems.<name>.workspaces.<name>]`; workspace execution config now uses
  `working_dir` consistently with container and catalog service config.
- Keep the bundled nginx `default` variant generic and ship the
  Genesis-style repo-root rewrite routing through a dedicated
  `decodelabs` config variant instead.
- Move the implicit Colima profile from `default` to `effigy`, auto-size that
  Effigy-owned profile with a host-aware memory+swap plan, and warn when a
  running workspace profile is still undersized or intentionally sharing
  Colima's global `default` profile name.
- Clear the host terminal immediately before `effigy workspace` hands off into
  the interactive container shell, so cold-start and prep logs do not stay on
  screen above the final workspace prompt.
- Remove the hidden managed-dev JS auto-hydration wrapper from container-backed
  standard panes; repos should now declare explicit per-pane `setup = [...]`
  when they need `bun install`, workspace hydration, or other startup prep.
- Keep `effigy workspace` handoff notices on explicit `[info]` labels and cap
  managed TUI per-frame event draining so heavy compile output does not stall
  redraws for multiple seconds during `effigy dev` startup.
- Fail fast when a repo changes `[containers.<name>].project_name` while the
  old Compose project is still running, so `effigy dev`, `effigy exec`, and
  container shell entrypoints now report the stale/new project mismatch
  directly instead of hanging or crashing later in the managed runtime path.
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
  `gateway = true` now applies on the TUI workspace handoff path
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
- Make workspace-seeded managed dev sessions shut their cold-started system
  back down after a successful `effigy dev` handoff, while still leaving it
  running when the handoff itself fails so debugging does not lose the live
  workspace.
- Preserve seeded workspace handoff transport failures as the primary error
  instead of masking them behind a later shutdown failure when the container
  exec session has already died.
- Add `effigy system reset-runtime` as an explicit hard reset path for wedged
  Colima-backed systems, so Effigy can kill stale profile processes, clear
  control sockets, and leave the profile fully stopped without waiting on the
  softer repair flow.
- Give `colima stop` a longer shutdown timeout than startup/status probes, so
  ordinary macOS VZ teardown does not get misreported as a fatal workspace
  shutdown failure after 15 seconds.
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
- Default a sole container's effective `project_name` to the catalog alias
  when present, and reject multi-container manifests whose effective
  `project_name` values collide unless they declare unique overrides.
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

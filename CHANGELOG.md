# Changelog

All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/).
During v0.x, MINOR bumps may include breaking changes.

## [Unreleased]

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
- fix Rhai `exec::run(..., #{ run_in: "host" })` inside workspace container handoff so host-context `cwd` and `stdin_file` paths are remapped onto the local workspace repo before execution

# Papercuts

Small, actionable friction found during agent work. Agents append entries when
they hit a solvable hurdle; they do not stop the current task to fix one.

## Open

<!-- Keep entries short. Append newest entries at the top. Do not include secrets. -->

### [ ] `docs add-log-index` appends after `## Next Task` instead of under Active logs — 2026-09-02
- Friction: `effigy docs add-log-index docs/logs/2026-09/02-155016-official-catalog-pack-update-1107.md` reported success and inserted the bullet after the logs README `## Next Task` paragraph, not at the top of `## Active logs`.
- Impact: the unique index entry is present for `docs check index` but the front-door list is wrong until a human moves the bullet; workers can ship a drifted Next Task block.
- Possible fix: insert immediately after the `## Active logs` heading (newest first), and never write below the README `## Next Task` section.
- Surface: `effigy docs add-log-index`; card closeout log-index step.

### [ ] `cli_container_attached_session_handles_sigint_during_startup` is timing-flaky — 2026-09-02
- Friction: `effigy::cli_output_tests` `cli_container_attached_session_handles_sigint_during_startup` failed under `effigy qa` and in isolation (twice) while passing under `cargo test --workspace`; it also fails on the clean base with this lane's changes stashed, so it is a pre-existing environment/timing race, not a regression.
- Impact: full `effigy qa` rounds fail intermittently on a container-attach SIGINT startup race, blocking worker required-validation runs.
- Possible fix: make the SIGINT-during-startup assertion race-free (wait for the attach/startup handshake before signalling) or mark it for container-availability/timing tolerance.
- Surface: workspace `cli_output_tests` container attach tests; any worker running `effigy qa`.

### [ ] `graph explore` can hang with no output on a cold worktree — 2026-09-01
- Friction: `effigy graph explore "<question>" --json` produced no stdout for
  more than 100s during worker startup on a fresh worktree; the process had to
  be killed.
- Impact: the documented code-understanding first command is not fail-fast, so
  workers fall back to direct search and lose the promised time bound.
- Possible fix: share the graph time-budget and stderr progress seam with
  `explore`, or fail quickly when the index is missing instead of blocking
  silently.
- Surface: `effigy graph explore`; worker-mode code-understanding routing.

### [ ] Repository task shadowing makes `docs context` unreachable — 2026-09-01
- Friction: this repository declares `[tasks.docs]`, so `effigy docs context`
  follows manifest-selector precedence and passes `context` to the task instead
  of reaching the built-in documentation query.
- Impact: the project-local Effigy agent route cannot use its documented
  authority lookup inside Effigy itself; operators need an undocumented escape
  or must fall back to direct file search.
- Possible fix: provide an explicit built-in escape that preserves normal task
  precedence, or move the query behind a non-shadowed command shape.
- Surface: deferred built-in routing, repository-intelligence discovery, and
  the project-local Effigy skill.

### [ ] Vendored Effigy skills need portfolio-level status and sync — 2026-08-30
- Friction: 15 consumer repos under one projects directory had stale copies of
  all 10 managed Effigy skill files. The supported updater works one repo at a
  time, and ignored skill trees are easy to miss with ordinary file discovery.
- Impact: agent routing and safety guidance drift across repos; maintaining the
  portfolio requires an ad hoc shell loop and manual dirty-tree checks.
- Possible fix: add a JSON-first scoped status/sync surface that inventories
  repo-local installs, fingerprints the bundled skill version, refuses dirty
  skill trees, and updates only the managed files.
- Surface: cross-repo skill distribution; `init` / agent adoption maintenance.

## Closed

### [x] Child-catalog suite task refs lose ancestor `[containers]` registry — 2026-09-01
- Friction: suite task-ref expansion changed cwd to the child catalog and then
  rediscovered the repository there, losing the loaded ancestor container
  registry.
- Impact: child suites could not inherit the workspace container default.
- Fix (2026-09-01): card `1100` pins nested host-launched refs to the originating
  repository while preserving the child cwd and child-explicit precedence.
  Evidence: `docs/logs/2026-09/01-173500-child-catalog-suite-registry-1100.md`.
- Surface: test suite task-ref expansion and container registry lookup.

### [x] `docs context` traversal is unreachable on a large corpus — 2026-09-01
- Friction: lexical results exhausted the section budget before any typed
  relation result could be selected.
- Impact: relation evidence disappeared on large repositories.
- Fix (2026-09-01): card `1102` keeps the best lexical result and, when at
  least two slots exist, reserves one for the best whole traversed result that
  fits. Evidence:
  `docs/logs/2026-09/01-172541-docs-context-traversal-budget-1102.md`.
- Surface: documentation-context selection and contract `041`.

### [x] `docs context` has no wall-clock bound on a cold graph — 2026-09-01
- Friction: lazy graph refresh could look wedged on a fresh checkout.
- Impact: callers had neither progress nor a bounded typed failure.
- Fix (2026-09-01): card `1101` shares the graph time-budget seam, timeout
  payload, health snapshot, and recovery guidance. Cold/stale rebuilds announce
  on stderr; usage validation remains outside the timer. Evidence:
  `docs/logs/2026-09/01-184159-docs-context-time-budget-1101.md`.
- Surface: graph time budget, lazy refresh, and docs-context shell.

### [x] A no-match benchmark case cannot name itself in its own corpus — 2026-08-31
- Friction: the `effigy-no-match` case in `perf:docs-context-benchmark` asserts
  an empty report. Documenting its query inside `docs/`, `README.md`,
  `AGENTS.md`, `CHANGELOG.md`, or `PAPERCUTS.md` - all profile roots - gives its
  terms a non-zero document frequency and turns the case red. It broke once
  exactly this way while writing the card `1090` evidence log.
- Impact: a durable, self-hosted no-match assertion is one careless sentence
  away from failing, and the failure looks like a retrieval regression.
- Fix (2026-09-01): card `1098` / roadmap `g08.043` keep empty-result proof on
  the fixture corpus and reject a live-target empty case before the matrix
  runs. Evidence:
  `docs/logs/2026-09/01-150452-no-match-benchmark-isolation-1098.md`.
- Surface: `scripts/benchmark-docs-context.rhai`; `docs/effigy.docs.toml` roots.

### [x] YAML frontmatter is indexed as one setext heading — 2026-08-31
- Friction: `effigy docs context` results for `docs/handoffs/*.md` show the whole
  frontmatter block as a single heading string, because the closing `---` reads
  as a setext underline for the preceding line. The result is a heading value
  hundreds of characters long that is useless in an agent context window.
- Impact: any repository whose Markdown carries frontmatter gets one unusable
  section per such document, and it competes for the section budget.
- Fix (2026-09-01): card `1097` / roadmap `g08.042` skip headings that start
  inside a complete leading `---` … `---` block; facts, relations, and exact
  spans remain. Evidence:
  `docs/logs/2026-09/01-135932-markdown-frontmatter-1097.md`.
- Surface: `crates/effigy-codegraph/src/language/markdown/extract.rs`.

### [x] `service list` reports non-fragment bundled files as fragments — 2026-09-01
- Friction: `CatalogResolver::list_bundled_fragments` took the first path
  component of every embedded asset, so root `README.md` and
  `compose.override.example.yml` appeared as fragments (16 names for 14 real
  services).
- Fix (2026-09-01): bundled membership requires a first-level
  `<name>/service.toml`. Root docs/examples and directories without a service
  manifest are ignored. Filesystem override and installed-pack listing remain
  directory-based; sorting and layering are unchanged.
- Recurrence proof: unit, integration, and CLI text/JSON proofs in
  `docs/logs/2026-09/01-133154-catalog-fragment-listing-1096.md`.
- Surface: `crates/effigy-catalog/src/fragment.rs` `list_bundled_fragments`.

### [x] `effigy-containers` tests read process-global env without the env lock — 2026-09-01
- Friction: `crate::test_env_lock()` guarded process-global env mutations, but
  direct and helper-hidden reads of `HOME`, `PATH`, and
  `EFFIGY_COMPOSE_BACKEND` in the same test binary could still overlap them.
- Cause: the first audit covered direct reads, backend/Colima construction,
  bundle user-config resolution, and direct `HOME` assertions, but missed the
  helper-hidden runtime-DNS read reached by policy loads with services.
- Fix (2026-09-01): added the existing lock as the precondition for every
  confirmed affected policy caller, including the two direct/generated runtime
  DNS tests named in PR 69. Production env semantics are unchanged; empty and
  error-before-read tests keep their parallel execution.
- Recurrence proof: the corrected inventory and repeated focused validation are
  recorded in `docs/logs/2026-09/01-123424-papercuts-env-lock-audit.md`.
- Surface: `crates/effigy-containers/src/{colima/tests.rs,compose/tests.rs,mount_spec.rs,policy_support/generated_compose.rs,runtime/dns.rs}`;
  `crates/effigy-containers/src/tests/{compose,policies,volumes_reports}.rs`;
  shared lock in `crates/effigy-containers/src/lib.rs`.

### [x] Rhai scripts can parse in release and fail in debug — 2026-08-31
- Friction: Rhai caps in-function expression nesting at 16 in debug builds and 32
  in release. `scripts/benchmark-docs-context.rhai` ran fine under
  `target/release/effigy` and failed under the `AGENTS.md`-documented
  `cargo run --bin effigy -- <task>` fallback with
  `Expression exceeds maximum complexity`. A reviewer found it, not the author.
- Impact: a first-party script can ship green from one build profile and be
  unrunnable from the other, including for any contributor without an installed
  binary. The error names a line and column but not the cap or the profile.
- Fix (2026-09-01): `configured_rhai_engine()` sets explicit expression depths
  `64` / `32` on every production host, matching release defaults. Card `1094`
  closed under archived strict spec `112`.
- Surface: `crates/effigy-rhai` engine construction; `scripts/*.rhai`.


### [x] `git fetch origin` can hang indefinitely waiting on SSH — 2026-08-30
- Friction: worker preflight `git fetch origin` sat silent for minutes until
  killed; a retry with
  `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes"` returned
  immediately.
- Impact: handoff/startup probes look wedged and waste a long command timeout.
- Fix (2026-08-30): `AGENTS.md` documents the BatchMode + ConnectTimeout wrap
  for worker-mode `git fetch origin`. No Git wrapper binary.
- Surface: `AGENTS.md`; worker handoff preflight; GitHub SSH remotes.

### [x] `effigy rhai surface` listed regex helpers with reversed args — 2026-08-30
- Friction: catalog advertised `regex::replace(value, pattern, replacement)`
  (and the same swap for `is_match` / `captures`) while the live host and guide
  use `(pattern, value[, replacement])`. Callers who trust the catalog get a
  silent no-op that returns the pattern.
- Fix (2026-08-30): catalog strings follow the live pattern-first order via
  `REGEX_PATTERN_FIRST_SIGNATURES`; a host self-check asserts catalog matches
  and that value-first calls do not silently rewrite.
- Surface: `crates/effigy-rhai/src/surface.rs`;
  `crates/effigy-rhai/src/tests/utility.rs`.

### [x] `effigy-rhai` runtime-context tests race on process-wide env vars — 2026-08-29
- Friction: `cargo test --workspace` intermittently fails
  `runtime::execute_rhai_script_exposes_state_capture_context_helpers` or
  `..._state_capture_set_in_capture_hook_context` with
  `Runtime error: missing EFFIGY_STATE_CAPTURE_CONTEXT`. Each passes in
  isolation. Recurring noise when validating unrelated changes.
- Cause: the scoped-env helper uses `std::env::set_var` / `remove_var`, which
  are process-wide. Both tests set `EFFIGY_STATE_CAPTURE_CONTEXT`, so one
  test's teardown clears it while the other is mid-run.
- Fix (2026-08-29): state/deploy host helpers consult a thread-local override
  map before the process env; runtime-context tests inject capture/apply/
  deploy keys there instead of `std::env::set_var`. Production capture
  semantics stay parent-process env when the map is empty.
- Surface: `lookup_host_env` / `ScopedHostEnvOverrides`;
  `crates/effigy-rhai/src/tests/runtime.rs`.

### [x] `deps link` cannot adopt committed path / `file:` local dependencies — 2026-08-29
- Friction: once root selection was fixed, `deps link cargo ../longhorn` from
  Figmatic refuses with `pre-migration path dependency` and `deps link bun`
  reports `committed-pin-active`, because Figmatic already declares Longhorn
  through Cargo `path` deps and Bun `file:` overrides. Both refusals are
  correct — a Cargo `[patch]` cannot redirect a path dep and a committed
  override outranks an ephemeral link — but `deps status` still cannot report
  the local dependency that is already in force.
- Fix (2026-08-29): `deps status` reads committed manifests and reports each
  cross-checkout Cargo `path` dependency and Bun `file:`/`link:` specifier as
  an observed link, grouped by library checkout, with `committed_local` set and
  `desired` absent. Read-only: the link refusals are unchanged, and the
  informational reason says so.
- Surface: `inventory_cargo_committed_path_locals`,
  `inventory_bun_committed_file_locals`, `deps status`.

### [x] `deps link` assumed the Git root was the manifest root — 2026-08-28
- Friction: `effigy deps link cargo ../longhorn --dry-run` walked into
  Longhorn's non-member `examples/command-system-proof/rust/jetstream` and died
  on `cargo metadata`; the Bun form looked for `package.json` at the Figmatic
  repo root while Figmatic keeps Bun in `studio/`.
- Fix (2026-08-28): Cargo library inventory anchors on the library root
  workspace and keeps only its members; Bun resolves the consumer package root,
  using the library to pick between sibling roots and refusing a genuinely
  ambiguous choice. `deps status` detects Bun below the repo root too.
- Surface: `inventory_cargo_library`, `inventory_bun_consumer`,
  `plan_bun_link`, `detect_repo_package_managers`.

### [x] Ship Clippy in the workspace container image — 2026-08-28
- Friction: generated `workspace-rust-bun` images shipped rustc without Clippy,
  so consumer validate tasks needed an undocumented `rustup component add clippy`.
- Fix (2026-08-28): Dockerfile runs `rustup component add clippy` and puts
  `cargo-clippy` / `clippy-driver` on PATH. Catalog contract test asserts the
  fragment.
- Surface: `crates/effigy-catalog/catalog/workspace-rust-bun/Dockerfile`.

### [x] Avoid recursive ownership prep across every workspace dependency tree — 2026-08-28
- Friction: `effigy health` recursively chowned every child `node_modules` /
  `vendor` volume after root Bun consolidation, spending minutes before checks.
- Fix (2026-08-28): prep the authoritative root dependency tree recursively,
  shallow-chown redundant child package trees, skip targets nested under another
  recursive prep path, and report prep counts / per-path progress.
- Surface: workspace permission prep (`plan_workspace_permission_prep`).

### [x] Workspace container exec rebuilds a linux artifact via Docker Hub — 2026-08-28
- Friction: `run_in = "container"` selectors rebuilt `effigy-linux-release-builder`
  from `ubuntu:22.04` even when the consumer workspace was already Up; Hub DNS
  timeouts aborted Contact Patch / Composer package selectors.
- Fix (2026-08-28): workspace handoff reuses a reusable on-disk linux artifact
  (file + matching rehearsal receipt) instead of rebuilding; reuse keeps the
  artifact's install identity honest (does not stamp current host freshness onto
  a stale binary); failed rebuilds name cached-image / offline /
  `EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE=download` options.
- Surface: `ensure_local_linux_workspace_effigy_artifact`, linux artifact build.

### [x] Detect the root Bun workspace in Effigy dependency status — 2026-08-28
- Friction: `effigy --json deps status` reported `manager: null` on a root
  `package.json` + Bun workspaces + `bun.lock` with no local-link record.
- Fix (2026-08-28): status detects root Bun (and Cargo) managers and reports
  `manager: "bun"` plus `detected_managers` when Bun is the sole root manager.
- Surface: `detect_repo_package_managers`, `effigy deps status`.

### [x] Parallel docs QA can fail `git rev-parse HEAD` — 2026-08-28
- Friction: overlapping `effigy docs/qa:*` loads raced on a shared git-bundle
  cache; one process `rev-parse HEAD` while another was mid-checkout and hit
  `ambiguous argument 'HEAD'`.
- Fix (2026-08-28): per-cache exclusive file lock around git-bundle
  materialization and HEAD reads. Concurrent refresh contract test covers it.
- Surface: `[bundle]` git source / manifest parse.

### [x] Skill task inventory jq path is stale — 2026-08-28
- Friction: skill examples queried `.result.payload.tasks[]` while live
  `effigy --json tasks` exposes `.result.catalog_tasks[]`.
- Fix (2026-08-28): retarget `skills/effigy` and `.agents/skills/effigy`
  examples to `.result.catalog_tasks[].task`.
- Surface: agent skill JSON envelope examples.

### [x] Attention-marker CLI overrides are ignored — 2026-08-28
- Friction: `effigy scan attention-markers --warning-marker ...` accepted the
  flags but still used the stock marker lists.
- Fix (2026-08-28): apply warning/high/critical marker request overrides in
  the attention-marker path. CLI contract test asserts the JSON `patterns`
  lists change.
- Surface: `effigy scan attention-markers`.

### [x] Root test suite task-refs drop `run_in = "container"` — 2026-08-28
- Friction: a suite `{ task = "cp-api/test:unit" }` inlined the cargo command
  on the host and dropped `run_in = "container"`.
- Fix (2026-08-28): container-bound task-refs expand to a nested `effigy`
  invocation so the referenced task's run_in is honored.
- Surface: `[test.suites]` task-refs.

### [x] Volume list reports a running postgres volume as not in use — 2026-08-28
- Friction: `container volume list` reported `in_use: false` while the
  postgres service was Up and compose mounts the volume.
- Fix (2026-08-28): a declared volume is in use when inspect lists the mount
  or the volume's service is among the running compose services.
- Surface: `effigy container volume list`.

### [x] Release gates execute in name order, so cheap-first is unbuyable — 2026-08-28
- Friction: `[release.gates]` sorted by name, so an expensive MSRV floor ran
  before a cheap candidate check.
- Fix (2026-08-28): gates keep TOML declaration order. No rename required.
- Surface: `[release.gates]`, `effigy release gates`.

### [x] `deps link bun` refuses Bun registry package symlinks — 2026-08-28
- Friction: after `bun install`, `deps link bun` treated Bun's
  `node_modules/.bun/...` package symlinks as conflicting targets.
- Fix (2026-08-28): registry store symlinks classify as replaceable Registry
  links, same as non-symlink installs.
- Surface: `effigy deps link bun`.

### [x] `effigy test <target>` passes the target as a suite filter — 2026-08-28
- Friction: `effigy test vitest stem --plan` scheduled every catalog and
  forwarded `stem` to each Vitest command.
- Fix (2026-08-28): a passthrough token that matches a catalog alias selects
  that catalog; it is not copied as a filter across siblings.
- Surface: `effigy test`.

### [x] Effigy task arguments silently widen when preceded by `--` — 2026-08-28
- Friction: `effigy test:unit -- <paths>` dropped the paths, while
  `effigy test:unit <paths>` forwarded them.
- Fix (2026-08-28): `{args}` strips a leading `--` delimiter and keeps the
  tokens after it.
- Surface: task argument forwarding / `{args}`.


### [x] Graph failures are unbounded and unexplained — 2026-08-27
- Friction: a `graph explore`/`context` that triggered a lazy re-index could
  sit indefinitely; the caller could not tell a slow first build from a wedged
  one, and no output said which.
- Fix (2026-08-27): graph *queries* run under a wall-clock budget
  (`EFFIGY_GRAPH_TIMEOUT_MS`, default 120000, `0` disables). Blowing it returns
  an `effigy.graph.timeout.v1` error envelope carrying index presence, index
  size, and refresh-lock state. `graph index` and `graph watch` stay unbounded:
  the caller explicitly asked for the long build.
- Surface: `run_graph`, `effigy_codegraph::health`.

### [x] Graph indexes installed and built frontend output — 2026-08-27
- Friction: the skip list only matched at the repo root, so every nested
  `node_modules` in a monorepo was indexed, and framework output
  (`dist`, `.svelte-kit`, `.next`, `coverage`) was never skipped at all. That
  bloated the index and dominated every freshness walk.
- Fix (2026-08-27): the skip set matches any path segment and covers installed
  packages plus build/framework output. Skipped directories are pruned during
  the walk instead of filtered per file. Ambiguous source names (`build`,
  `out`, `lib`) stay indexable.
- Surface: `crates/effigy-codegraph/src/walk.rs`.

### [x] Container git is broken inside a linked worktree — 2026-08-27
- Friction: a linked worktree's `.git` is a file holding a host-absolute path
  into the primary checkout. Mounting only the repo root carried that pointer
  into the container with nothing behind it, so every in-container `git` call
  failed.
- Fix (2026-08-27): workspace containers bind-mount the shared git directory at
  its own absolute path, so the recorded pointer resolves without rewriting
  repository state. Ordinary checkouts are unaffected.
- Surface: `build_worktree_git_mounts`, `effigy_core::git_worktree`.

### [x] A missing sibling mount aborts doctor and container status — 2026-08-27
- Friction: a `../book`-style workspace extra mount that a teammate has and you
  do not took down `container status` and `doctor` for the whole repo.
- Fix (2026-08-27): an absent non-catalog workspace extra mount warns and is
  skipped, matching how user-global library mounts already behave. Catalog
  members still hard-fail — a missing declared member is broken state, not
  machine variance.
- Surface: `parse_workspace_extra_mount`.

### [x] macOS Finder metadata breaks Bun `file:` dependency work — 2026-08-27
- Friction: Finder droppings inside a `file:` dependency tree. `__MACOSX/` and
  `.AppleDouble/` hold an unparseable copy of `package.json`, which turned a
  Bun inventory into a hard parse failure; `.DS_Store` and `._*` sidecars ride
  along into a container install.
- Fix (2026-08-27): Effigy's Bun package walk skips Finder metadata
  directories and sidecar files, and `deps status bun` reports the exact
  offending paths with a removal command built from the same constants as the
  detector, so it clears every class the diagnostic reports. Bun's own `file:`
  copy is not Effigy's to change; naming the files is.
- Surface: `crates/effigy-deps/src/bun.rs`, `deps status bun`.

### [x] Launcher worktrees miss the Effigy local vault — 2026-08-27
- Friction: the local vault is machine-local state outside version control, so
  a fresh `git worktree` starts without one and every secret-backed task fails,
  even though the same machine already holds an unlocked vault in the primary
  checkout.
- Fix (2026-08-27): vault reads *and* mutations in a linked worktree resolve
  through one shared path that falls back to the primary checkout's vault, so
  `secrets set` cannot fork a partial local vault that shadows primary-only
  records. When neither vault exists the warning names the primary checkout and
  the `secrets init` to run there. Vault *creation* still writes where it was
  asked to. No new secrets backend.
- Surface: `resolve_shared_effigy_vault_path`, `resolve_shared_rhai_secret_vault_path`.


### [x] Doctor schema rejects inline `{ rhai = ... }` task values — 2026-08-27
- Friction: consumer manifests such as Longhorn use compact
  `task = { rhai = "scripts/..." }` values the runner already executes; Doctor
  treated `rhai` as an unsupported task-table key.
- Fix (2026-08-27): Doctor schema accepts compact inline task tables with
  `run`/`task`/`rhai`/`run_in`.
- Surface: `effigy doctor` manifest schema for `[tasks]`.

### [x] Doctor rejects built-in `docs` steps as unresolved task references — 2026-08-27
- Friction: `{ task = "docs check" }` sequence steps resolved as missing
  tasks because Doctor's builtin skip-list omitted `docs`.
- Fix (2026-08-27): treat `docs` as a built-in selector in task-reference
  checks.
- Surface: `effigy doctor` `tasks.references.resolve`.

### [x] Global `--` is consumed after `--`, so a task cannot take `--repo` — 2026-08-27
- Friction: `effigy <task> -- --repo <path>` still switched catalogs, so
  consumer tasks could not receive `--repo` as their own argument.
- Fix (2026-08-27): `--` ends task runtime-flag parsing and builtin-deferral
  repo scans; remaining args, including `--repo`, reach the task. Leading
  `--repo` before the task name still switches catalogs.
- Surface: `parse_task_runtime_args`, global CLI flag application.

### [x] Parallel contracts tests can share a timestamp temp directory — 2026-08-21
- Friction: full `cargo nextest` intermittently failed
  `validate_selection_accepts_valid_payload`; its isolated rerun passed. The
  valid and wrong-count tests both use a nanosecond timestamp as the complete
  temporary-directory identity and can overwrite each other's artifact.
- Fix (2026-08-24 / closed 2026-08-27): already replaced `tempfile_root`
  nanosecond paths with `tempfile::tempdir()` in `086adc7ae`. No remaining
  collision; no further test rewrite.
- Surface: `crates/effigy-contracts/src/tests.rs`.


### [x] Bun pin depends on fallible lockfile enumeration — 2026-08-12
- Friction: `deps pin bun` aborted when `bun pm ls --all` returned
  `Error loading lockfile: InvalidPackageInfo`, even after regeneration.
- Fix (2026-08-12): pin-only fallback reads valid text `bun.lock` package
  records as JSONC, warns with the original Bun failure, and fails closed on
  unsafe lock data. Bun links remain process-authoritative.
- Surface: `effigy deps pin bun` package-closure inventory.

### [x] Bun status hides linked packages exposed through `file:` dependencies — 2026-08-11
- Friction: a repository consumed through `file:` can expose linked packages
  from its own `node_modules` without identifying the cross-repository source.
- Fix (2026-08-11): Bun status and doctor warn with the dependency, package,
  symlink, external target, and unlink-or-override remediation.
- Surface: `effigy deps status bun` and doctor dependency findings.

### [x] Docs index check required `./`-prefixed links — 2026-08-09
- Friction: `collect_index_markdown_links` only matched `(./x.md)`; plain
  `(x.md)` and backtick mentions looked present but counted as missing.
- Fix (2026-08-11): accept `(path.md)` and `(./path.md)`; missing-entry
  failures hint that backtick-only mentions do not count.
- Surface: `crates/effigy-docs-policy` index check.

### [ ] Stale local-install binary fails `qa:docs` after a manifest grammar change — 2026-09-05
- Friction: PR `93` added `[docs_policy.sources]` to `effigy.toml`; the
  `.local-install/bin/effigy` on PATH (built before it) then fails every
  task with `unknown field sources` before docs QA can start. Nothing says
  the binary is behind main.
- Impact: any agent validating docs on a fresh `main` sees a parse error
  unrelated to its change until it thinks to use `cargo run --bin effigy`.
- Fix: `effigy doctor` (or the manifest parse error itself) should say when
  the running binary's build SHA is older than the repository's own
  manifest-grammar requirement, and point at the local-install refresh task.
- Surface: local-install route, manifest parsing error text, `doctor`.

# Papercuts

Small, actionable friction found during agent work. Agents append entries when
they hit a solvable hurdle; they do not stop the current task to fix one.

## Open

<!-- Keep entries short. Append newest entries at the top. Do not include secrets. -->

### [ ] Colima VM DNS proxy can timeout Docker Hub blob pulls — 2026-09-25
- Friction: `nerdctl build` of `rust:1.91-bookworm` failed after ~3 minutes with `lookup registry-1.docker.io on 192.168.5.3:53: read udp ... i/o timeout` while the Mac host resolved the same name via LAN DNS in 19ms. A retry minutes later succeeded.
- Impact: linux-arm64 catalog image smokes on Colima look wedged or blocked on a host DNS proxy that is not the worker's registry credentials.
- Possible fix: make Colima's `192.168.5.3` DNS proxy fail over to the host resolver, or document a bounded retry for worker image pulls.
- Surface: Colima default VM DNS; `nerdctl build` of `workspace-rust-bun`.

### [ ] `workspace-rust-bun` entrypoint cannot run as `dev` — 2026-09-25
- Friction: `nerdctl run --user 1000:1000` dies before the command because `/usr/local/bin/effigy-entrypoint` writes `/var/log/effigy-ssh-bridge.log` as root (`Permission denied`). The non-root Playwright smoke had to use `--entrypoint bash`.
- Impact: any compose `user: "1000:1000"` / `user: "dev"` launch of the catalog image fails at start instead of reaching the shell or task.
- Possible fix: write the bridge log under `/tmp` (dev-owned) or create the log path as root then drop privileges, matching the existing socat bridge model.
- Surface: `crates/effigy-catalog/catalog/workspace-rust-bun/Dockerfile` `effigy-entrypoint`.

### [ ] `cargo update -p X --precise` silently re-resolves unrelated lockfile edges — 2026-09-24
- Friction: updating only `hickory-proto`/`hickory-server` to 0.26.3 with per-package `--precise` updates re-selected unrelated edges that were already validly locked: `tempfile 3.27.0` dropped from `getrandom 0.4.3` to `0.3.4` (splitting getrandom into an extra version) and several `windows-sys` consumers re-pointed from 0.61.2 to existing 0.60.2/0.59.0 entries. The drift reproduced deterministically regardless of update order.
- Impact: bounded dependency tasks get unexplainable lockfile deltas exactly when review oracles demand none, and the extra getrandom version ships unless caught.
- Possible fix: when the target releases keep dependency lists identical, apply the version/checksum entry update surgically to `Cargo.lock` and validate with `cargo fetch --locked` plus `--locked` builds (what Dependabot itself writes); longer term, cargo could offer a truly edge-stable precise update mode.
- Surface: `Cargo.lock` maintenance workflow; `cargo update -p ... --precise` behavior.

### [ ] Doctor-bounded subprocesses are opt-in per call site — 2026-09-24
- Friction: `ReadOnlyProcess::run` carries no deadline, so a new dependency-inspection subprocess silently bypasses the doctor budget, and `inspect_cargo_link` mapped an inventory timeout to `cargo-resolution-inspection-failed` instead of propagating it until g10.012 added the `is_process_timeout` early return.
- Impact: future subprocess call sites can reintroduce unbounded doctor hangs or mistimed phases by default.
- Possible fix: thread the deadline through the `ReadOnlyProcess` contract (or a doctor-scoped wrapper type) so unbounded execution requires an explicit opt-out, and keep one timeout-propagation helper next to the status inspection arms.
- Surface: `crates/effigy-deps/src/process.rs`, `crates/effigy-deps/src/status.rs`; `crates/effigy-doctor/src/dependency_health.rs`.

### [ ] Cold graph indexing exceeds the default agent lookup budget — 2026-09-24
- Friction: `effigy graph explore` timed out after 120 seconds while indexing this fresh worker checkout (1712 of 2766 files). Exact-symbol `rg` navigation was needed to continue the task.
- Impact: the documented graph-first code navigation route can fail at worker startup on a cold checkout.
- Possible fix: make initial indexing incremental or set a cold-start budget that covers this repository, while retaining the bounded failure.
- Surface: code graph lazy refresh and agent navigation.

### [ ] Documentation graph rejects Markdown paths containing spaces — 2026-09-24
- Friction: a real Git CLI fixture for docs-source provenance failed during graph indexing with `graph id must not contain whitespace` when a tracked Markdown filename contained a space. Git status parsed the path correctly.
- Impact: `docs context --sources` cannot return source evidence from such a file.
- Possible fix: encode path components safely in graph IDs while retaining the original repository-relative path in results.
- Surface: documentation graph record IDs and Markdown path indexing.

### [ ] Installed Northstar skill task leaves `{skill}` unexpanded through `--repo` — 2026-09-24
- Friction: `effigy --repo <installed-northstar-skill> northstar/language:route ...` passed the literal `{skill}/scripts/language-package-lifecycle.ts` to Bun and failed before the Rust audit route. Calling the same local script by absolute path succeeded.
- Impact: the documented installed-skill language route is unusable through this Effigy invocation.
- Possible fix: make the task placeholder resolve for `--repo` execution or update the Northstar task definition and route to a supported path form.
- Surface: installed Northstar `effigy.toml` task interpolation; Effigy task runner `--repo` behavior.

### [ ] `effigy-core` build-info env test races under full-workspace parallelism — 2026-09-15
- Friction: the exact-head review of g10.008 observed `build_info::tests::read_active_version_env_trims_and_ignores_empty_values` fail once during a full parallel `cargo test --workspace` run (process-global `EFFIGY_*` version env vars mutated concurrently by `EnvGuard`), while it passes in isolation and under `effigy qa`. `effigy-core` only gained `task_selection.rs` in that lane.
- Impact: full workspace rounds can fail intermittently for reasons unrelated to the change under test.
- Possible fix: serialize env-mutating build-info tests behind the shared test lock or inject env per test instead of using process-global `std::env::set_var`.
- Surface: `crates/effigy-core/src/build_info.rs` tests; any `cargo test --workspace` round.

### [ ] Draft task bodies duplicate every `[tasks]` field in two places — 2026-09-15
- Friction: `ManifestDraftTable` in `crates/effigy-manifest/src/draft_defs.rs` re-lists every `ManifestTask` field because serde `flatten` is incompatible with `deny_unknown_fields`, which the strict draft grammar needs. Adding a task runtime field now requires editing both structs plus the `into_manifest_draft` conversion.
- Impact: a future task field can silently be accepted in `[tasks]` but rejected or dropped in `[drafts]`.
- Possible fix: derive the draft table from `ManifestTask` with a shared field list, or split lifecycle metadata into a nested table so the body can reuse `ManifestTask` directly.
- Surface: `crates/effigy-manifest/src/task_runtime.rs`, `crates/effigy-manifest/src/draft_defs.rs`.

### [ ] Adding a `LoadedCatalog` field edits many test fixtures — 2026-09-15
- Friction: `LoadedCatalog` is built with struct literals in seven test helpers across `src/runner/execute/**`, `crates/effigy-managed/**`, `crates/effigy-doctor/**`, and `crates/effigy-codegraph/**`; adding `draft_sources` meant touching every one.
- Impact: catalog-model changes pay a mechanical fixture tax and are easy to leave half-updated.
- Possible fix: add a `LoadedCatalog::for_test(...)` constructor or a `Default`-based fixture helper and migrate the literals.
- Surface: `crates/effigy-manifest/src/loaded_catalog.rs`, test helpers across the workspace.

### [ ] Graph skill guidance must be edited in two parity-locked copies — 2026-09-15
- Friction: `.agents/skills/effigy/**` is the owned/authoritative skill surface, but `documentation_coverage_tests::project_local_and_distributed_effigy_skills_have_semantic_parity` requires `skills/effigy/**` to be byte-identical (apart from the local internal metadata block). Documenting a new graph flag means editing an unowned path or failing the parity test.
- Impact: a worker following owned-path discipline cannot update the authoritative skill doc without a one-sided edit that fails CI.
- Possible fix: generate the distributed copy from the local copy in one task, or list both copies as owned together in dispatch manifests.
- Surface: `.agents/skills/effigy/**`, `skills/effigy/**`, `tests/documentation_coverage_tests.rs`.

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
- Triage: `docs/triage/20260909-152107-vendored-effigy-skill-portfolio-sync.md` (open candidate; keep this entry open until promoted or deliberately declined).

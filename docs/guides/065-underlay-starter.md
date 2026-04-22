# Underlay starter

Reusable Effigy manifest shape for Underlay-style consumer repos — a
long-running Rust + Bun workspace container, bundled dev services
(postgres, mailpit, minio), a gateway-fronted domain set, and a managed
`dev` TUI task.

Use this when a repo wants the Underlay local-dev shape without copying
`docker-compose.yml`, `workspace.Dockerfile`, and large system/container
overrides from an existing Underlay repo.

## Emit the starter

```
effigy init underlay
```

Emits five files into the current repo:

| File                        | Purpose                                                                                      |
|-----------------------------|----------------------------------------------------------------------------------------------|
| `effigy.toml`               | Root manifest. `[catalog]` alias, `[package_manager]`, `[manifest].include` for the shape.   |
| `effigy.system.toml`        | `systems.dev` + `containers.stack` + bundled service declarations + gateway DNS routes.      |
| `effigy.bootstrap.toml`     | `[bootstrap]` entry points and `bootstrap:deps` task.                                        |
| `effigy.tasks.toml`         | Managed `tasks.dev` concurrent shape + `health` / `validate` / `qa` aggregators.             |
| `scripts/dev/ui-setup.rhai` | Starter-seeded frontend hydration helper, consumer-owned thereafter.                         |

Nested parent directories (e.g. `scripts/dev/`) are created as needed.

Supported flags mirror the rest of `effigy init`:

- `--dry-run` — print each file's content under a `=== <target> ===`
  header without touching disk.
- `--force` — overwrite existing targets. Without `--force`, `init`
  refuses to clobber any target that already exists and lists every
  conflicting path.
- `--json` — emit the `effigy.init.v1` contract (see
  [guide 017](./017-json-output-contracts.md)) with a per-file
  `files[]` array and a top-level `guidance` string.

After emission the text output prints the post-emission guidance block
embedded in the starter descriptor (edit checklist below).

## Post-emission edit checklist

The `effigy init underlay` output already prints this list; keeping a
copy here so users can refer back without re-emitting.

1. In `effigy.system.toml`:
   - set `systems.dev.working_dir` to `/workspace-root/<repo-name>`
   - adjust `systems.dev.mounts` for any sibling checkouts
   - rename `containers.stack.project_name`
   - rename the DNS `domain` values to the project's real domains
   - set `containers.stack.services.workspace.working_subdir` to the
     repo's directory name under `workspace_mount`
   - align `host_ports` with the repo's app dev-server ports

2. In `effigy.tasks.toml`:
   - replace the `app-api/dev`, `app-admin/dev`, `app-front/dev`
     entries with the repo's real child apps
   - update the `ready_message`, tab ordering, and aggregator task
     lists to match

3. In `effigy.bootstrap.toml`, rewrite the `bootstrap:deps` command to
   match the repo's dependency-fetch sequence.

4. In `scripts/dev/ui-setup.rhai`, edit the `shell_targets` block so
   it hydrates the repo's real frontend packages.

After the edit pass, the consumer repo carries **no** `docker-compose.yml`
and **no** workspace Dockerfile — the compose output is generated from
bundled catalog fragments each run.

## What the starter is built on

The starter uses only the stable Effigy model:

- [`064-system-workspace-and-dev-contract.md`](./064-system-workspace-and-dev-contract.md)
  for `systems` / `workspaces` / managed `dev`.
- [`063-container-system-guide.md`](./063-container-system-guide.md) for
  the generated service catalog and `[containers.<name>.services.*]`.
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
  for `[manifest].include` composition.

No new runtime concept is introduced.

## The `workspace-rust-bun` bundled service

The starter's `workspace` service resolves to a bundled catalog fragment
named **`workspace-rust-bun`**. It ships:

- `rust:${RUST_VERSION}-bookworm` base image
- Bun (latest by default, pinnable via `bun_version`)
- a non-root `dev` user aligned with host UID/GID so bind-mounted files
  round-trip cleanly
- `command: sleep infinity` — the container runs as a shell target and
  command runner, not a service
- persistent named volumes for cargo registry + cargo git
- an opt-in `host_ports` parameter for repo-owned dev-server bindings
- a healthcheck that verifies cargo and bun are on `PATH`

Parameters (see `service.toml` in the fragment directory for full
details):

| Param            | Default             | Purpose                                                            |
|------------------|---------------------|--------------------------------------------------------------------|
| `rust_version`   | `"1.88"`            | Rust base image tag.                                               |
| `bun_version`    | `""` (latest)       | Pin a specific Bun release.                                        |
| `workspace_mount`| `"/workspace-root"` | Mount point for the workspace root inside the container.           |
| `working_subdir` | `""` (= mount root) | Subdirectory of `workspace_mount` to set as the compose `working_dir`. |
| `host_ports`     | `[]`                | Host-side port bindings, e.g. `["41001:41001", "41002:41002"]`.    |

System-layer overrides still apply on top:

- `systems.<name>.user` and `systems.<name>.working_dir` win at task
  time via `docker compose exec -u <user> -w <working_dir>` — the
  Dockerfile's `USER`/`WORKDIR` are defaults only.
- `systems.<name>.mounts` are injected into the workspace service's
  compose `volumes` at runtime.

## What stays consumer-owned

- Per-app `effigy.toml` in each child app (cargo/bun build commands,
  PORT and env strings, vite flags, migrations, jobs runner, etc.)
- Tab ordering and concurrent makeup in `tasks.dev`
- Project domains, project name, host port numbers
- Sibling-checkout layout in `systems.dev.mounts`
- The `ui-setup.rhai` helper (starter-seeded, consumer-owned thereafter)

## Proof

The integration suite covers both the fragment and the starter:

- `crates/effigy-catalog/tests/integration.rs`
  - `resolve_workspace_rust_bun_fragment`
  - `workspace_rust_bun_assembles_with_defaults`
  - `workspace_rust_bun_publishes_host_ports_when_requested`
  - `underlay_style_stack_assembles_with_bundled_fragments_only`
- `crates/effigy-catalog/src/starter.rs`
  - `underlay_starter_resolves_with_all_declared_files_and_guidance`
- `crates/effigy-manifest/tests/underlay_starter.rs`
  - verifies the starter composes into a single manifest via
    `[manifest].include`
  - verifies `systems.dev` binds the `stack` container
  - verifies the four expected services resolve to their bundled
    catalog fragments
  - verifies `tasks.dev` wires both `role = "lifecycle"` and
    `role = "shell" service = "workspace"` runtime-contract entries
  - verifies the `bootstrap:deps` / aggregator tasks are present
- `src/tests/runner_tests/runner_core_tests/init_migrate_tests/init_tests.rs`
  - `run_manifest_task_builtin_init_underlay_emits_all_declared_files_and_guidance`
  - `run_manifest_task_builtin_init_underlay_refuses_overwrite_without_force`
  - `run_manifest_task_builtin_init_underlay_force_overwrites_all_targets`
  - `run_manifest_task_builtin_init_underlay_dry_run_prints_fenced_sections_without_writing`
  - `run_manifest_task_builtin_init_underlay_json_reports_files_array_and_guidance`

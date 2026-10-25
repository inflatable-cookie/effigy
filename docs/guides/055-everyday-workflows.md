# 055 - Everyday Workflows

Use this guide when you want Effigy to feel simple in day-to-day work.

This page focuses on the common human workflows first, then links to the deeper
reference pages behind them.

Use this after the quick start. If you still need the first ten minutes, go
back to [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md).

## 1) Start By Asking The Repo What Exists

Once the CLI is installed and the repo is open, the first useful question is:

- what can this repo already do?

```sh
effigy tasks
effigy tasks --resolve test
effigy tasks --resolve app/build
```

Use `effigy tasks` for discovery. Use `--resolve` when you want to understand
which catalog owns a selector before you run it. Use `effigy <selector> --plan`
when you need the command that would run, without running it.

Deep dive:
- [`016-task-routing-precedence.md`](016-task-routing-precedence.md)
- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)

## 2) Run Work By Intent, Not By Directory

Once tasks are named in `effigy.toml`, the daily path should be direct:

```sh
effigy dev
effigy test
effigy app/db:reset
```

Names like **`dev`** and **`app/db:reset`** come from **`effigy.toml`** (or a
nested catalog). **`test`** is always the **built-in** test orchestrator. Named
**`[test.suites]`** entries configure what it runs.

The shift is simple:

- stop teaching people where a script lives
- start teaching them the task name that expresses the intent

Deep dive:
- [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`059-manifest-composition-guide.md`](059-manifest-composition-guide.md)

## 3) Standardize The Workflows Teams Repeat

### Health and diagnosis

```sh
effigy doctor
effigy doctor --deep
effigy doctor --repo /path/to/workspace api/test --watch
```

Use default `doctor` for bounded structural evidence. Use `--deep` when you
explicitly want enabled content scans and the selected catalog's `health`
task. Use the selector form when you want explain-mode evidence for one
specific request.

Deep dive:
- [`018-doctor-explain-mode.md`](018-doctor-explain-mode.md)
- [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)

### Tests

```sh
effigy test --plan
effigy test vitest
effigy test cargo-nextest -- --test-threads=1
```

Use built-in test planning when you want one entrypoint for mixed stacks and
predictable suite selection.

Keep focused Rust runs read-only. A focused run against a checkout whose
sibling crates are source-linked resolves through `Cargo.lock`, and a plain
Cargo invocation may rewrite that file as a side effect of collecting
evidence:

```sh
effigy test:focused -p effigy-deps --lib -E 'test(cargo_unlink::tests::)'
effigy test -p app-api --locked
```

`test:focused` passes `--locked` for you in this repository. In another repo,
add `--locked` to the built-in test passthrough. A lockfile that genuinely
needs an update is updated deliberately, never as a side effect of a test run;
when `--locked` refuses, the error names what the resolver wanted to change.

Deep dive:
- [`013-testing-orchestration.md`](013-testing-orchestration.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](048-built-in-test-suite-lifecycle-and-env.md)
- [`077-local-dependency-linking.md`](077-local-dependency-linking.md)

### Repo scans

```sh
effigy scan god-files
effigy scan duplicate-blocks
```

Use scanners when you want concrete findings about codebase drift instead of
manual grep or one-off scripts.

Deep dive:
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)

### Agent code- and documentation-understanding context

```sh
rg -n "<symbol>" crates src
rg -n "<authority-shaped question>" docs
```

Effigy has no code graph in v0.15: `rg` finds code, `docs/README.md` indexes the
knowledge, and `effigy test --plan` narrows validation after edits. For
discovery, health, and test routing, start with `effigy doctor`, `effigy tasks`,
and `effigy test --plan`.

## 4) Use One Local-Dev Chain

For web or service-heavy repos, the clean daily path should read as one chain
instead of a pile of unrelated commands:

```sh
effigy service list
effigy container up
effigy gateway status
effigy exec composer install
effigy dev
```

Use the commands in this order:

- `service` when you need to inspect or extract bundled service fragments
- `container` when you need to bring the local environment up, inspect it, or
  manage its data
- `gateway` when the repo declares local domains or TLS-backed routes
- `exec` when you need one ad-hoc command in the dev container without opening
  a long-lived shell
- a managed task such as `effigy dev` when the repo wants one named
  front door for the whole local session

The point is that local bring-up should feel like one built-in path, not
one-off compose commands plus wrapper scripts plus tribal knowledge.

Deep dive:
- [`063-container-system-guide.md`](063-container-system-guide.md)
- [`012-dev-process-manager-tui.md`](012-dev-process-manager-tui.md)
- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)

### Runtime cleanup

Once a repo has been around for a while, two cleanup questions show up:

- what disposable build caches can I drop safely?
- which old named volumes are no longer part of the current repo shape?

Use the built-in inventory surfaces instead of raw runtime commands:

```sh
effigy container cache list --global
effigy container cache prune --project my-app-dev --yes
effigy container volume list --dormant
effigy container volume prune --dormant --yes
```

Use `cache` for safe disposable build artifacts such as `target`,
`node_modules`, `pnpm-store`, and Cargo caches. Use `volume` when you need to
inspect or remove stale repo-owned named volumes.

## 5) Use Artifacts for Data Lifecycle

When a repo needs versioned database seeds, snapshots, or UAT content, use the
artifact substrate instead of ad-hoc file copies:

```sh
# Inspect an OCI artifact before using it
effigy state artifact inspect oci://ghcr.io/acme/uat-content:2026-05-06 --json

# Stage a local dump for seeding
effigy state artifact stage ./data/legacy.sql.gz --json

# Capture and push a UAT snapshot
effigy state artifact capture ./dumps/uat.sql.gz \
  --ref oci://ghcr.io/acme/uat-content:2026-05-06 \
  --environment uat --push --json

# Seed a container from an OCI artifact
effigy container data seed --db-seed app=oci://ghcr.io/acme/private-data:uat

# Dump and push to OCI in one command
effigy container data dump app=oci://ghcr.io/acme/uat-content:2026-05-07 --push --json

# Seed a checkout during bootstrap through the repo's own setup task
effigy bootstrap git@github.com:acme/app.git
```

Workflow rules:

- OCI refs must use the explicit `oci://` prefix
- `--push` is required for live registry writes; planned captures are the default
- digest-pinned refs are preferred for audited apply operations
- use `oras login` beforehand for registry auth
- artifact kinds (`sql-dump`, `uat-content-snapshot`, etc.) help automation route
  payloads correctly

Deep dive:
- [`072-artifact-commands-guide.md`](072-artifact-commands-guide.md)
- [`014-artifact-substrate-contract.md`](../knowledge/contracts/014-artifact-substrate-contract.md)
- [`057-bootstrap-repo-bringup.md`](057-bootstrap-repo-bringup.md)

## 6) Move Friction Into The Manifest

When a workflow still depends on memory, shell aliases, or repo-specific setup,
move that detail into `effigy.toml`.

```sh
effigy init
effigy init --check --json
effigy init --checklist --json
effigy init --apply-actions manifest.effigy_toml,secrets_surface.inspect --json
effigy config --schema --minimal
```

This is usually where Effigy starts to feel easier: env handling, test
selection, chains, caches, and task naming become explicit instead of implied.
Plain `effigy init` now fills in the missing deterministic repo setup surface:
baseline manifest and README, managed `AGENTS.md`, project-local skill files,
and local `.effigy/` ignore rules. Use `--check --json` for the baseline
managed setup report, `--checklist --json` for the wider setup inventory, and
`--apply-actions` when a script or agent needs explicit selected-action
execution instead of prompt-driven TTY flow.

Deep dive:
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`028-migration-quick-paths.md`](028-migration-quick-paths.md)
- [`050-env-schema-integration.md`](050-env-schema-integration.md)

### Keep temporary proofs internal

Declare proof tasks under `[tasks]` with `visibility = "internal"`. They run
by name and share ordinary routing, kernel gates and status:

```sh
effigy tasks --all
effigy --json tasks
effigy provider-smoke --verbose
```

Publish a useful command with `visibility = "public"` and add a `group` for
human discovery. Remove temporary definitions and their explicit includes
when finished. See the [manifest cookbook](022-manifest-cookbook.md#public-and-internal-tasks).

## 7) Make Automation Boring

When humans and tools use the same commands, the machine-facing path should
stay just as clear:

```sh
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

Use JSON mode when CI, bots, or agents should consume stable payloads instead
of scraping terminal text.

Deep dive:
- [`017-json-output-contracts.md`](017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](026-json-payload-examples.md)

## 9) Use Native Release And Distribution Surfaces

When release or packaging work still depends on wrapper-script memory, the
default path should be native. Before these commands, prove the clean pushed
candidate SHA with the repository's full hosted CI board; for Effigy, manually
dispatch `ci.yml` and watch the exact-SHA run to success.

```sh
effigy release status --check-gates
effigy release run --to certify
effigy release:preflight --tag vX.Y.Z
effigy release:evidence:validate --artifacts-dir ./artifacts/distribution-vX.Y.Z
effigy release receipt <run>
```

Use `release` for release readiness, the checkpointed mutation flow, the
certified artifact digests and the receipt. The `release:` repo tasks hold
Effigy's own preflight, metadata, artifact-proof, GLIBC, install and evidence
checks; a project declares its own as `[release.gates]` or a target's `verify`.

Deep dive:
- [`051-release-orchestration.md`](051-release-orchestration.md)
- [`062-distribution-system-guide.md`](062-distribution-system-guide.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](049-ci-binary-distribution-and-release-protocol.md)

## 10) Bridge Legacy Commands With Deferral

When a repo still has legacy commands that are not yet migrated to `effigy.toml`
tasks, use deferral as a migration bridge instead of maintaining two separate
runners:

```sh
effigy defer prep
effigy defer release -- --dry-run
```

Configure the fallback in `effigy.toml`:

```toml
[defer]
run = "composer global exec effigy -- {request} {args}"
builtins = ["release"]
```

Use deferral when:
- the project does not yet have full `effigy.toml` task coverage
- you want unresolved selectors to hand off to a legacy runner automatically
- you need to bypass a built-in temporarily so a legacy command can own that name

Stop using deferral once all critical paths are native Effigy tasks.

Deep dive:
- [`015-deferral-fallback-migration.md`](015-deferral-fallback-migration.md)
- [`028-migration-quick-paths.md`](028-migration-quick-paths.md)

## 11) When Effigy Still Feels Hard

That usually means the built-in path or manifest still needs work.

Common signals:

- new contributors need to know the repo layout before they can run the basics
- test setup lives in wrapper scripts instead of `effigy test` or manifest data
- env rules live in shell docs instead of `effigy.toml` or `.env.schema`
- CI depends on ad-hoc parsing instead of `effigy --json <command>`
- release steps are explained as a script bundle instead of one built-in flow

Prefer fixing those by improving the manifest or built-in usage path, not by
adding more onboarding prose around the same friction.

Deep dive:
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](048-built-in-test-suite-lifecycle-and-env.md)
- [`050-env-schema-integration.md`](050-env-schema-integration.md)
- [`051-release-orchestration.md`](051-release-orchestration.md)
- [`062-distribution-system-guide.md`](062-distribution-system-guide.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](049-ci-binary-distribution-and-release-protocol.md)

## 9) Bridge Legacy Commands With Deferral

When a repo still has legacy commands that are not yet migrated to `effigy.toml`
tasks, use deferral as a migration bridge instead of maintaining two separate
runners:

```sh
effigy defer prep
effigy defer release -- --dry-run
```

Configure the fallback in `effigy.toml`:

```toml
[defer]
run = "composer global exec effigy -- {request} {args}"
builtins = ["release"]
```

Use deferral when:
- the project does not yet have full `effigy.toml` task coverage
- you want unresolved selectors to hand off to a legacy runner automatically
- you need to bypass a built-in temporarily so a legacy command can own that name

Stop using deferral once all critical paths are native Effigy tasks.

Deep dive:
- [`015-deferral-fallback-migration.md`](015-deferral-fallback-migration.md)
- [`028-migration-quick-paths.md`](028-migration-quick-paths.md)

## 10) When Effigy Still Feels Hard

That usually means the built-in path or manifest still needs work.

Common signals:

- new contributors need to know the repo layout before they can run the basics
- test setup lives in wrapper scripts instead of `effigy test` or manifest data
- env rules live in shell docs instead of `effigy.toml` or `.env.schema`
- CI depends on ad-hoc parsing instead of `effigy --json <command>`
- release steps are explained as a script bundle instead of one built-in flow

Prefer fixing those by improving the manifest or built-in usage path, not by
adding more onboarding prose around the same friction.

Deep dive:
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](048-built-in-test-suite-lifecycle-and-env.md)
- [`050-env-schema-integration.md`](050-env-schema-integration.md)
- [`051-release-orchestration.md`](051-release-orchestration.md)

## 12) Keep Dependency Updates Edge-Stable

`cargo update -p <crate> --precise <version>` is bounded in intent but not in
effect: Cargo re-resolves the edges around the named package, so a two-crate
bump can re-point unrelated `windows-sys` consumers or split `getrandom` into
an extra version. Review then sees deltas nobody can explain, and the extra
version ships unless someone catches it.

When the target releases keep their dependency lists identical, keep the graph
still:

1. Name the exact crates and versions the change is for.
2. Run the precise update once, by hand:
   `cargo update -p <crate> --precise <version>`.
3. Copy the new `version` and `checksum` values for those entries out of the
   resulting `Cargo.lock`, then restore the lockfile from your last commit.
4. Apply the update surgically: edit only those `[[package]]` entries, changing
   `version` and the registry `checksum`. Leave every `source` and
   `dependencies` list untouched. This is the shape Dependabot commits.
5. Validate with locked commands, which fail instead of re-resolving:
   `cargo fetch --locked`, then the covering test selector such as
   `effigy test:focused -p <crate> --lib`.
6. If `--locked` refuses because the graph genuinely needs a new edge, the
   update is not edge-stable: land the full resolution instead and call out the
   extra deltas in review. Never accept a split version as review noise.

A `Cargo.lock` diff for a bounded dependency task should be explainable line by
line: the named crates' version and checksum, and nothing else.

Deep dive:
- [`077-local-dependency-linking.md`](077-local-dependency-linking.md)
- [`013-testing-orchestration.md`](013-testing-orchestration.md)

## Expected Outcome

After this guide, you should have a clearer default path for:

- discovering work
- running tasks and tests
- using one coherent local-dev path for services, containers, gateway, exec,
  and repo-owned dev sessions
- using artifacts for versioned data seeds, snapshots, and OCI workflows
- using built-ins for health, tests, and scans
- using native release and distribution commands instead of wrapper-script
  bundles
- spotting the next piece of repo friction that should move into Effigy

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)
- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](048-built-in-test-suite-lifecycle-and-env.md)
- [`050-env-schema-integration.md`](050-env-schema-integration.md)
- [`051-release-orchestration.md`](051-release-orchestration.md)
- [`062-distribution-system-guide.md`](062-distribution-system-guide.md)
- [`063-container-system-guide.md`](063-container-system-guide.md)
- [`059-manifest-composition-guide.md`](059-manifest-composition-guide.md)
- [`072-artifact-commands-guide.md`](072-artifact-commands-guide.md)

## Next Step

After this workflow pass, open
[`022-manifest-cookbook.md`](022-manifest-cookbook.md) and convert the next
repeated manual step in your repo into an explicit task, env rule, or built-in
test/release configuration.

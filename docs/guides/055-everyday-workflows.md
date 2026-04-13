# 055 - Everyday Workflows

Use this guide when you want Effigy to feel simple in day-to-day work.

This page focuses on the common human workflows first, then links to the deeper
reference pages behind them.

## Vision Alignment

- Primary tags: `OPERATE`, `ROUTE`, `ADOPT`
- Target movement: the most common repo workflows are obvious enough that
  people reach for Effigy first instead of wrapper scripts or tribal knowledge.

## 1) Start By Asking The Repo What Exists

The first question should be "what can this repo already do?"

```sh
effigy tasks
effigy tasks --resolve test
effigy tasks --resolve app/build
```

Use `effigy tasks` for discovery. Use `--resolve` when you want to understand
which catalog owns a selector before you run it.

Deep dive:
- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)

## 2) Run Work By Intent, Not By Directory

Once tasks are named in `effigy.toml`, the daily path should be direct:

```sh
effigy dev
effigy test
effigy app/db:reset
```

The important shift is that you stop teaching people where a script lives and
start teaching them the task name that expresses the intent.

Deep dive:
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)

## 3) Standardize The Workflows Teams Repeat

### Health and diagnosis

```sh
effigy doctor --verbose
effigy doctor --repo /path/to/workspace api/test -- --watch
```

Use `doctor` when a repo feels ambiguous, broken, or inconsistent. Use the
selector form when you want explain-mode evidence for one specific request.

Deep dive:
- [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)

### Tests

```sh
effigy test --plan
effigy test vitest
effigy test cargo-nextest -- --test-threads=1
```

Use built-in test planning when you want one entrypoint for mixed stacks and
predictable suite selection.

Deep dive:
- [`013-testing-orchestration.md`](./013-testing-orchestration.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)

### Watch mode

```sh
effigy watch --owner effigy --once test
effigy watch --owner effigy --max-runs 2 test
```

Use watch mode when you want bounded reruns with explicit ownership rules
instead of nested watcher loops.

Deep dive:
- [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md)

### Repo scans

```sh
effigy scan god-files
effigy scan comment-ratio
effigy scan generated-in-src
```

Use scanners when you want concrete findings about codebase drift instead of
manual grep or one-off scripts.

Deep dive:
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)

## 4) Move Friction Into The Manifest

When a workflow still depends on memory, shell aliases, or repo-specific setup,
move that detail into `effigy.toml`.

```sh
effigy init
effigy migrate --from package.json
effigy config --schema --minimal
```

This is usually where Effigy starts to feel easier: env handling, test
selection, chains, caches, and task naming become explicit instead of implied.

Deep dive:
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)
- [`050-env-schema-integration.md`](./050-env-schema-integration.md)

## 5) Make Automation Boring

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
- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)

## 6) Treat Proof Demos As A First-Class Operator Surface

When a repo has demo or proof scripts that people actually need to discover,
run, inspect, and review, move them into `[demos.<id>]` instead of keeping
them as an ad hoc script pile.

```sh
effigy demo list
effigy demo browser
effigy demo inspect login-smoke
effigy demo history login-smoke --limit 5
effigy demo run login-smoke
```

Use demos when the repo should name what the proof covers and keep operator
review inside one surface.

Deep dive:
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)

## 7) When Effigy Still Feels Hard

That usually means the product surface or manifest still needs work.

Common signals:

- new contributors need to know the repo layout before they can run the basics
- test setup lives in wrapper scripts instead of `effigy test` or manifest data
- env rules live in shell docs instead of `effigy.toml` or `.env.schema`
- CI depends on ad-hoc parsing instead of `effigy --json <command>`
- release steps are explained as a script bundle instead of one built-in flow

Prefer fixing those by improving the manifest or built-in usage path, not by
adding more onboarding prose around the same friction.

Deep dive:
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)
- [`050-env-schema-integration.md`](./050-env-schema-integration.md)
- [`051-release-orchestration.md`](./051-release-orchestration.md)

## Expected Outcome

After this guide, you should have a clearer default path for:

- discovering work
- running tasks and tests
- using built-ins for health, watch, and scans
- using demos as an explicit proof surface instead of script sprawl
- spotting the next piece of repo friction that should move into Effigy

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md)
- [`050-env-schema-integration.md`](./050-env-schema-integration.md)
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`059-manifest-composition-guide.md`](./059-manifest-composition-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)

## Next Step

After this workflow pass, open
[`022-manifest-cookbook.md`](./022-manifest-cookbook.md) and convert the next
repeated manual step in your repo into an explicit task, env rule, or built-in
test/release configuration.

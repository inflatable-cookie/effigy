# Documentation Coverage Parity Closeout

Status: complete
Created: 2026-08-21
Roadmap: g08.034
Cards: 1086, 1087

## Summary

- Audited current public Effigy behavior by command and manifest family against
  active user, agent, built-in, generated, reference, and troubleshooting
  surfaces.
- Fixed verified discovery gaps around managed headless sessions, readiness,
  process order, optional secret unlock, workspace ownership diagnosis, and
  non-console exec identity.
- Restored the repo-local skill's missing `built-in-surfaces.md` reference and
  kept both skill trees semantically aligned.
- Added proportional guards for skill parity, built-in-to-command-reference
  routing, managed-runtime discovery tokens, help output, and generated config
  output.
- The documentation coverage batch changed no public runtime, manifest, JSON,
  release, dependency, or workflow behavior. Hosted-check follow-up later
  updated one vulnerable lockfile entry and made Bun pin planning's existing
  text-lock fallback work when Bun is unavailable.

## Evidence Matrix

| Behavior family | Implementation owner | Active documentation surfaces | Finding | Disposition |
| --- | --- | --- | --- | --- |
| Global CLI, help topics, global flags, JSON envelope, completion | `effigy-cli` command descriptors/parsers; `effigy-core::builtin_tasks`; CLI output tests | root README; guides `017`, `021`, `025`, `026`; built-in help/completion | Command families, global `--json` / `--repo`, help routing, schemas, and completion already covered | Already covered; registry-to-matrix guard added |
| Task selectors, catalogs, routing, status, deferral, locks, cache | `effigy-manifest`, `effigy-routing`, `effigy-execution`, task/defer built-ins | guides `015`, `016`, `020`, `022`, `025`; config reference; help; skill | Selector and JSON affordances routed correctly | Already covered |
| Test, watch, init, migrate | `effigy-builtin`, `effigy-cli`, manifest test owners | guides `013`, `019`, `021`, `025`, `048`; generated config; help | Current built-in test authority, planning, watch owner policy, init, and migration covered | Already covered |
| Graph, scan, doctor, docs, contracts, papercuts | codegraph, scan, doctor, docs-policy, contracts, and papercuts crates | guides `018`, `022`, `023`, `025`, `029`, `076`, `078`; help; skill | Doctor help and agent lookup omitted workspace ownership finding | Fixed; help/skill/troubleshooting routes added |
| Cargo/Bun dependency links and committed Bun pins | `effigy-deps`; deps CLI/help/tests | guides `025`, `077`; help; skill | Link, unlink, pin, unpin, status, doctor, and safety boundaries covered | Already covered |
| Container, system, workspace, gateway, service, exec | manifest container/system sections; `effigy-containers`; runner runtime/exec/workspace modules | guides `023`, `025`, `063`, `064`, `067`, `069`; help; skill | Deep guide and exec help were correct; command lookup, troubleshooting, and skill omitted part of workspace identity/non-console behavior | Fixed |
| Managed TUI/headless sessions | manifest task config; `effigy-managed`; runner managed pipeline and behavior tests | root/docs front doors; guides `012`, `022`, `025`; generated config; help; skill | Deep guides covered behavior; front doors, skill, general help, generated comments, and troubleshooting were incomplete | Fixed |
| Secrets and typed env | manifest secret/env sections; `effigy-secrets`, `effigy-env`; runner injection tests | guides `022`, `050`, `075`; config reference; help; skill | Deep secret guide covered optional keys under forced unlock; agent surface did not | Fixed |
| Demo and Rhai scripting | manifest demo/script sections; `effigy-demo`, `effigy-rhai`; CLI/help tests | guides `025`, `058`, `061`, `068`; generated config; help | Discovery, execution, host API, and proof routes covered | Already covered |
| State, deploy, artifacts, container data | manifest state/deploy/data sections; state, deploy runner, artifact/data crates | guides `025`, `072`, `073`, `074`; generated config; help | Plan/apply/capture/history, provider, OCI, seed, and dump surfaces covered | Already covered |
| Bundle, bootstrap, manifest composition, service catalogs | manifest composition/bundle/bootstrap owners; bootstrap, bundle, catalog crates | guides `022`, `025`, `057`, `059`, `065`, `066`, `067`; generated config; help | Current source, bring-up, include/override, and catalog routes covered | Already covered |
| Release, distribution, changelog, uninstall | release/distribution/changelog crates; CLI descriptors and tests | guides `025`, `036`, `049`, `051`, `052`, `062`; help; skill | Current release routes covered; `distribution` registry exception needed explicit no-top-level wording in the command lookup | Fixed |
| Generated manifest reference families | manifest config-section types; builtin config docs for manifest, tasks, test, package manager, defer, shell, scan, distribution, containers, demos, secrets, state, deploy | `effigy config`; guides `022`, `027` and family deep dives | Generated tasks output named readiness timeout/order but not headless flag/env/companions or readiness scope | Fixed; config render assertions added |
| Root/docs front doors and agent skill trees | README/index owners; bundled skill source and project-local installed copy | `README.md`, `docs/README.md`, guide index, both skill trees | Root entry omitted headless usage; docs entry omitted guide `012`; repo-local skill linked a missing reference | Fixed; semantic tree parity guard added |

Blocked findings: none.

## Managed-Runtime Seed Proof

- Headless mode and companions: guide `012`, cookbook `022`, command matrix,
  root/docs front doors, general help, generated task comments, troubleshooting,
  and both skills expose `--headless`, `EFFIGY_MANAGED_HEADLESS=1`, `status`,
  `logs [process] [--follow]`, and `stop`.
- Readiness: guide `012`, cookbook `022`, config reference, command matrix, and
  skill explain container-owned lifecycle route scope and
  `health_wait_timeout_secs`.
- Concurrent order: guide `012`, cookbook `022`, generated config, command
  matrix, and skill state that `start` controls spawn order, including headless
  mode.
- Optional forced secrets: guide `075`, cookbook `022`, config-shapes skill
  reference, and skill state that missing `required = false` container keys do
  not become required during local-dev unlock.
- Workspace ownership: doctor implementation/tests, guide `063`, command
  matrix, doctor help, troubleshooting, and skill expose
  `container.workspace-ownership` and its read-only remediation path.
- Non-console exec identity: exec implementation/tests, guide `063`, exec help,
  command matrix, troubleshooting, and skill expose declared workspace
  user/home for the primary service and no TTY request for non-console callers.

## Changed Surfaces

- front doors: `README.md`, `docs/README.md`
- active guides: `023-troubleshooting-and-failure-recipes.md`,
  `025-command-reference-matrix.md`
- skills: both `SKILL.md` files, both `config-shapes.md` references,
  both `built-in-surfaces.md` references
- built-in/generated docs: general help, doctor help, task config docs
- recurrence: help/config assertions and `tests/documentation_coverage_tests.rs`
- release note: `CHANGELOG.md` under `[Unreleased]`
- closeout: cards `1086`/`1087`, roadmap `g08.034`, strict spec `107`, and
  planning/log/roadmap front doors

## Recurrence Protection

- Project-local and distributed skill file inventories must match. Reference
  files must match byte-for-byte; `SKILL.md` may differ only by the local
  `internal: true` metadata block.
- Every public builtin registry family must route through guide `025`; the
  deliberate no-top-level `distribution` boundary remains explicit.
- Stable managed-runtime seed tokens must remain present across the command
  reference, skill, troubleshooting, and front doors.
- Help and generated config tests assert the built-in discovery text at the
  renderer boundary.

These guards check stable relationships and identifiers. They do not claim to
prove prose completeness.

## Orientation And Validation

- startup worktree preflight: clean registered worktree
  `/Users/tom/Dev/worktrees/effigy-g08-034-docs-coverage` on
  `worker/g08-034-docs-coverage`, initially equal to `origin/main`
- `effigy tasks`: passed; selectors recorded
- `effigy doctor`: `ok:19`, `warn:1`, `err:0`; existing god-file warning only
- `effigy graph explore "public built-in commands CLI flags help descriptors and documentation owners" --json`: passed
- focused help render test: passed
- focused generated config render test: passed
- `effigy test --test documentation_coverage_tests`: passed, 3 tests
  - initial guard runs exposed missing literal `workspace_user` and
    `non-console` lookup terms; surfaces were corrected and the final rerun
    passed
- `effigy qa:docs`: passed
- `effigy docs check workflow-paths`: passed
- `effigy qa:docs:agent-defaults`: passed
- `effigy qa`: passed
- `cargo fmt --all -- --check`: passed
- `cargo clippy --all-targets -- -D warnings`: passed
- `git diff --check`: passed

## Hosted PR Follow-Up

- Hosted cargo-deny found `RUSTSEC-2026-0258` in locked `h2 0.4.15`.
  `Cargo.lock` now resolves `h2 0.4.16`, the advisory's patched release.
- Hosted full JSON validation had no Bun installation and failed while creating
  dependency fixtures, before the indexed commands ran. The fixture now writes
  its small deterministic text `bun.lock` directly.
- `deps pin bun --dry-run` still needed package enumeration after fixture
  creation. Its existing safe text-lock fallback now covers both a failed Bun
  process and a missing Bun executable; a focused unit test protects the
  missing-executable case.
- No workflow file changed.

Follow-up validation:

- `cargo tree -i h2@0.4.16`: passed; `h2 0.4.15` absent
- `cargo deny check`: passed; advisories, bans, licenses, and sources green
- focused Bun fallback tests: passed
- `cargo test -p effigy-contracts`: passed
- full `contracts check-json` with Bun deliberately absent from `PATH`: passed
- `effigy qa`: passed
- `cargo fmt --all -- --check`: passed
- `cargo clippy --all-targets -- -D warnings`: passed
- `git diff --check`: passed

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Movement: baseline feature-local documentation coverage -> current
  evidence-backed whole-surface audit with stable recurrence guards
- Remaining gap: none in this lane; prose review remains a human judgment and
  is not overstated as mechanically complete

## Residual Risks

- Prose quality and sufficiency cannot be fully automated. The evidence matrix
  and routed deep guides remain the review surface.
- New subcommands and config families still require the contribution playbook;
  the guards catch registry/route and seeded-behavior drift, not every possible
  explanation gap.

## Next Task

Review PR `main <- worker/g08-034-docs-coverage`. Do not merge without operator
authorization. After this lane, run the second governance review by 2026-09-17
and await operator intent for the next Horizon theme.

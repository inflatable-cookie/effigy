---
name: effigy
description: >
  Effigy task-runner skill for agents: route by job instead of ritual —
  use rg and the docs index for code-navigation questions, tasks for selector
  inventory, doctor for routing or health ambiguity, test --plan when test shape matters,
  run work through effigy selectors, parse --json envelopes, and avoid
  release/CI footguns. Use when the user mentions effigy, effigy.toml, or
  needs tests, dev, QA, repo navigation, deployment, or validation in an
  Effigy-adopting repo.
---

# Effigy Skill

## What Effigy is

One CLI (`effigy`) + **`effigy.toml`** (often split across includes) = the
repo's task surface. **Selectors** (`dev`, `api/test`, `qa:ci:fast`) route to
manifest tasks or **built-ins** (`test`, `doctor`, `scan`, …).

**`dev`** is almost always a **repo task name**, not a built-in verb.
**`test`** is always the built-in test orchestrator. Repos configure explicit
test routes under `[test.suites]`. Prefer `effigy <selector>` over raw `cargo` / `npm` /
`docker compose` when Effigy covers the path.

Use `test.suites.<name>.default = false` for focused suites that should run only
when named. Use root `test.exclude_catalogs` when a parent workspace already
owns a child catalog's tests; direct `catalog/test` selection still works.

## Shared guidance source

Effigy maintains this skill in `skills/effigy/` and distributes it as an
installed Agent Skill. Supported global roots are `~/.agents/skills/effigy`,
`~/.codex/skills/effigy`, `~/.claude/skills/effigy`, and
`~/.cursor/skills/effigy`. Resolve symlinks before comparing them: aliases to
the same canonical directory count once. Distinct matches are ambiguous and
named Effigy task lookup fails closed.

Plain `effigy init` does not copy or refresh a project-local skill. Preserve
existing `.agents/skills/effigy` content; named skill-task lookup still gives
an invocation project's local task source precedence by contract 042. A
filesystem lookup does not prove what a running agent loaded; use a fresh
agent context after an install or refresh.

Agent Skill guidance and the `effigy` executable are separate. Check the
resolved executable with `command -v effigy` and `effigy --version`. Heavy
selectors wait only for an `EFFIGY_ADMISSION_HOOK` the launcher sets. Name a
run with `effigy --run-id <key> <task>` and inspect, follow or stop it with
`effigy run status|logs|stop <key>`. See
`docs/guides/080-host-wide-validation-admission.md`.

## Footguns (read first)

- **Never modify `.github/workflows/`** without explicit human approval.
- **Never bypass release gates.** Fix the underlying issue.
- **Never re-tag a failed release.** Fix lands in next PATCH.
- **Never run release mutations** (`release prepare/execute`) without explicit human ask.
- **Never use `release verify-install` for a library or service repo.** It is
  Effigy's fixed self-hosting binary verifier; other repos need a repo-owned
  consumer smoke.
- **Don't add `package.json` scripts** that re-export Effigy tasks.
- **Don't add a current-directory repo override** when already inside the
  target repo.
- **Never map `health` to `qa`.** Keep `health` seconds-scale, `validate`
  mid-cost, and `qa` the full board.
- **Never define `tasks.test`.** It was removed in v0.11. Use named
  `[test.suites]` entries for Rust, TypeScript, and other repo test routes.

Full rationale: `references/footguns.md`.

## Agent routing

Do not run `doctor`, `tasks`, and `test --plan` as an automatic entry ritual.
Pick the first Effigy command that matches the job.

| Job | Use when | First command |
|-----|----------|---------------|
| Understand code | Ownership, behavior, implementation, impact | `rg` for the symbol, then read `docs/README.md` for the owning knowledge |
| Find documentation authority | Which contract, decision, lane, or prior decision governs the work | `rg` the docs tree, then `docs/README.md` for the index |
| Find runnable selectors | You need repo tasks or QA surfaces | `effigy tasks` |
| Inspect test shape | You need to know what `effigy test` will actually do | `effigy test --plan` |
| Diagnose routing or repo health | Selector resolution is unclear, or health/drift is the task | `effigy doctor` |
| Inspect local dependency links | Cargo/Bun desired state, drift, or lock/peer hygiene is the task | `effigy --json deps status` |
| Run installed skill tasks | A skill owns task code but the current repo owns runtime effects | `effigy skill tasks --path <SKILL>` |
| Inspect a selector without running it | You need resolved task, catalog, and command shape | `effigy --plan <selector>` or `effigy --json --plan <selector>` |
| Execute work | A task or built-in already covers the operation | `effigy <selector>` |
| Narrow validation | You changed code and want likely tests/files first | `effigy test --plan` |
| Parse results | Another tool/agent will consume the output | `effigy --json <command>` |

Effigy has no code graph in v0.15: `rg` finds code, `docs/README.md` indexes the
knowledge, and `effigy test --plan` answers which tests run. Details:
`references/agent-operating-loop.md`.

## Routing rules

- use `rg` and the docs index for code- and documentation-understanding questions
- use `tasks` when you need selector inventory, not when you already know the task
- use `doctor` when routing is unclear or repo health is itself the task
- use `test --plan` when test execution shape matters, not as a greeting
- use selectors and built-ins for real execution work
- use `--json` whenever another agent step will consume the result; it is an
  output format, never a planning or safety flag
- use `effigy --plan <selector>` to inspect command shape without executing; Effigy flags go before the task, everything after it is the task's

Good navigation question shapes:

- `rg -n "redirect" crates src`
- `rg -n "migrat.*valid" crates`
- `rg -n "exit cleanup" crates`

Full built-in lookup: `references/built-in-surfaces.md`.

## Common workflows

| Goal | Command |
|------|---------|
| Orient in unfamiliar code | `rg` the symbol, then read `docs/README.md` |
| Find the governing document | `rg` the docs tree; start from `docs/README.md` |
| Run focused tests | `effigy test <suite-or-runner-filter>` |
| Inspect test plan | `effigy test --plan` |
| Bring local dev up | `effigy container up` then `effigy dev` |
| Inspect work validation selectors | `effigy tasks` or `effigy test --plan` |
| Repo health scan | `effigy doctor --verbose` |
| Local dependency link health | `effigy --json deps status` |
| Run an installed skill task | `effigy skill run [--path <SKILL>] <SKILL>/<TASK>` |
| Scaffold manifest | `effigy init`, then define explicit manifest tasks |
| Check repo setup | `effigy init --check --json` or `effigy init --checklist --json` |
| Apply repo setup | `effigy init` or `effigy init --apply --json` |
| Read-only release check | `effigy release gates` |

Details: `references/workflow-shortcuts.md`.

## Bounded QA groups

A QA group names a deliberate set of existing selectors for one work context,
with declared targets, proof limits, and an expected runtime. It is
composition and evidence, not another runner, and a passing group proves only
its members — never that the map is true or your scope list complete.

| Goal | Command |
|------|---------|
| Inventory maintained groups | `effigy tasks qa-groups list` |
| Include one temporary definition | append `--file <path>` (relative paths resolve inside the repo; no directory scan exists) |
| Inspect before running | `effigy tasks qa-group run <GROUP> --scope <TOKEN> --plan` |
| Run the declared members | `effigy tasks qa-group run <GROUP> --scope <TOKEN>...` |
| Inspect a run | `effigy tasks qa-group status <RUN_ID> --json` / `logs <RUN_ID>` |

Rules that keep group use honest:

- Supply scope as repeatable typed tokens (`path:`, `cargo-package:`,
  `bun-package:`, `workspace:`, `input:`, `external:`), including unchanged
  inputs you know matter (lockfiles, generator dependencies). Effigy never
  infers scope from a diff, status, or graph output.
- `needs_planner` means stop and ask the planner with the printed token and
  reason; it never means "run the full board". A `declared_match` runs every
  member — scope never filters checks.
- A group selector never falls through to a same-named task.
- `stop` and `hard_timeout_ms` are unavailable (owned-run supervision, contract 052); an
  interrupted run reconciles to `unknown`, never a pass.

## Selector routing (60 seconds)

1. **Alias prefix** (`qa:` → `qa:ci:fast`)
2. **Path prefix** (`api/test` → api catalog)
3. **CWD-nearest** workspace
4. **Shallowest** match if still ambiguous

Ambiguous? `effigy doctor <selector> <args...>` — then **stop and ask** if still unclear.

Details: `references/selector-routing.md`.

## JSON envelope

```bash
effigy --json <command>
```

Returns `effigy.command.v1` with command-specific data in `result` (or
`error.details` for some failures).

```bash
effigy --json tasks | jq -r '.result.catalog_tasks[].task'
```

Details: `references/json-envelope.md`.

## When to stop and ask

- Selector still ambiguous after `effigy doctor <selector> <args...>`
- `effigy doctor` structural FAIL on a task where repo health or routing matters
- User did not explicitly request release prepare/execute
- Change needs `.github/workflows/` edits
- Someone suggests skipping a release gate

## Specialized surfaces

Read these only when the repo or task needs them.

**Managed task sessions** — a repo task with `mode = "tui"` can run without a
terminal UI through `effigy <task> --headless` or
`EFFIGY_MANAGED_HEADLESS=1`. Inspect it from another shell with task-local
`status`, `logs [process] [--follow]`, and `stop`. Start ranks remain spawn
order; readiness covers container-owned routes started by the lifecycle entry
and uses `health_wait_timeout_secs`. Optional container secrets stay optional
even during forced local-dev unlock. Guide:
`docs/guides/012-dev-process-manager-tui.md`.

**Workspace identity** — primary-service `effigy exec` and host-routed
workspace tasks use the declared `workspace_user` and `workspace_home`.
Those paths, plus headless workspace handoff, prepare owned disposable
rust/cargo/target named volumes exclusive to the primary service for the
resolved uid/gid and verify read/write/lock before children launch. Bind
rust paths and named volumes shared with another compose or managed
service are probed only.
`effigy exec` from a non-console caller does not request a TTY. Use
`effigy doctor --verbose` to find
read-only `container.workspace-ownership` findings for cargo/target,
managed volume, or Bun-cache paths the resolved user cannot use. A
deadline-expired inspect is unavailable, never clean. Guide:
`docs/guides/063-container-system-guide.md`.

**Secrets** — `effigy secrets init`, `set`, `import`, `list`, `doctor` when
`[secrets]` is declared. The vault uses a passphrase; delete retired
`identity`, `unlock` and `external` keys under `[secrets.vault]`. An operator can
run `effigy secrets unlock` to enable
prompt-free local commands and secret-backed tasks; `secrets lock` revokes that
access. Guide: `docs/guides/075-secrets-and-vault-guide.md`.

**Bundles** — `[bundle].base` with `path` / `git` / `oci`; `effigy bundle inspect`,
`effigy bundle sync`. Guide: `docs/guides/065-external-bundle-adoption.md`.

**Installed skill tasks** — use
`effigy skill tasks --path <SKILL_DIR|EFFIGY_TOML>` to inventory one explicit
skill catalog, then
`effigy skill run [--path <SKILL_DIR|EFFIGY_TOML>] <SELECTOR> [--repo <CONSUMER>] [--json] [--stdio passthrough] [-- <ARGS>]`.
An explicit `--path` is the authoritative source. Without it, a qualified
`<skill>/<task>` selector derives the skill name and resolves the invocation
project's `.agents/skills/<skill>` first, then the unique installed global root
under `~/.agents/skills`, `~/.codex/skills`, `~/.claude/skills`, or
`~/.cursor/skills`; `--repo` never changes discovery, and two distinct global
matches fail closed. The skill path supplies task code; invocation CWD or
`--repo` supplies the consumer target. Do not use `--repo <SKILL>` as a
substitute. V1 is host-only, does not merge consumer
selectors/defaults/config, and rejects members or escaping composition/assets,
container inheritance, and managed/TUI/concurrent shapes before side effects.
Add `--json` when the agent needs to verify canonical source, target,
invocation, and execution paths. Use `--stdio passthrough` to let the selected
task own raw stdin/stdout/stderr and exit status for machine-to-machine calls;
it cannot combine with `--json`. Contract:
`docs/knowledge/contracts/042-external-skill-task-runner-contract.md`.

**Documentation navigation** — v0.15 removed `effigy docs context`; use
`docs/README.md` as the docs index and `rg` for exact text. The optional
committed `[docs_policy.graph]` profile is still validated config, but it has no
v0.15 reader. `effigy docs check` is unchanged.

**Config shapes** — `[tasks]`, `[systems]`, `[containers]`, `[bootstrap]`,
`[release]`, `[bundle]`, `[secrets]`, `[state]`, `[deploy]`, and the optional
`[docs_policy.graph]` repository-defined Markdown graph profile.
Details: `references/config-shapes.md`.

**Program fallbacks** — `[programs.<name>]` is user-config only
(`~/.effigy/config.toml`), never a project `effigy.toml`:
`fallback = "<program>"`, optional `allow = ["<first argument>", ...]`, optional
`force = true`. The fallback is active when `<name>` is not an executable on the
inherited PATH, or when `force = true`; Effigy then puts a per-run shim named
`<name>` first on that run's PATH and prints an honest notice. `force` changes
only inherited-PATH name resolution: absolute paths, a replaced PATH (including
a task `env` PATH), shell aliases and shell functions are untouched. An omitted
`allow` means unrestricted exact argv; a present `allow` runs only those exact
first arguments and refuses anything else (including no argument) before exec.
The shim execs the fallback with the exact argv — no translation, nothing added,
no retry — and the fallback's exit status is the shim's. The original program's
behaviour and safeguards are lost: for `oxide` -> `cargo`, `oxide clean` becomes
`cargo clean` without Oxide's ownership checks or managed storage, and cargo
plugins and cargo-subcommand exec run as plain cargo would. The fallback is
resolved to an absolute path on the inherited PATH, so it never re-enters a shim
and fallbacks never chain. Guide:
`docs/guides/069-workspace-host-integration.md`.

**State stacks** — `effigy state plan`, `apply`, `capture`.
Guide: `docs/guides/073-state-stack-guide.md`.

**Deployment** — `effigy deploy model --json` and `deploy export` remain supported.
`deploy apply` and `deploy redeploy` are not yet supported in v0.15, even with
`--yes`; plan/status/history remain available. Full execution design is in the
Queue deploy lane (Q-003). Guide: `docs/guides/074-deployment-guide.md`.

**Local dependencies** — use `effigy deps link <cargo|bun> <PATH>` for
ephemeral machine-local state. Use `effigy deps pin bun <PATH>` for a committed
root-consumer override, then run `bun install` separately; reverse it with
`unpin` plus another operator-run install. Status and doctor are read-only;
never restore linked locks through Git.
Guide: `docs/guides/077-local-dependency-linking.md`.

**Release** — never mutate without explicit human instruction.
Sequence: `references/release-protocol.md`.

## Deeper docs

| Topic | Guide |
|-------|-------|
| Documentation profile | `docs/knowledge/architecture/024-repository-defined-documentation-graph.md`, `docs/knowledge/contracts/041-documentation-graph-profile-contract.md` |
| Agent adoption | `docs/guides/047-agent-and-cross-repo-adoption.md` |
| Installed skill task execution | `docs/knowledge/contracts/042-external-skill-task-runner-contract.md`, `docs/knowledge/architecture/025-external-skill-task-execution.md` |
| Rhai script steps | `docs/guides/061-rhai-script-steps-guide.md` |
| Rhai host surface audit | `docs/guides/068-rhai-host-surface-audit.md` |
| Task routing | `docs/guides/016-task-routing-precedence.md` |
| JSON contracts | `docs/guides/017-json-output-contracts.md` |
| Quick start | `docs/guides/021-quick-start-and-command-cookbook.md` |
| Command reference | `docs/guides/025-command-reference-matrix.md` |
| Managed task sessions | `docs/guides/012-dev-process-manager-tui.md` |
| Local dependency linking | `docs/guides/077-local-dependency-linking.md` |
| Distribution evidence | `docs/guides/062-distribution-system-guide.md` |
| Containers / dev | `docs/guides/063-container-system-guide.md` |
| Release | `docs/guides/051-release-orchestration.md` |
| Built-in lookup | `references/built-in-surfaces.md` |

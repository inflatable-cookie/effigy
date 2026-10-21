# Effigy

Effigy is a Rust task runner for monorepos. Its CLI and `effigy.toml` give agents and humans one predictable surface for tasks, tests, local systems, diagnostics, and release checks. Keep routing explicit and JSON output stable.

## Where things live

- Current state: `docs/README.md`
- Current knowledge: `docs/knowledge/README.md`
- Retired concepts: `docs/knowledge/retired.toml`
- Open questions: `docs/knowledge/questions.md`
- User documentation: `docs/guides/README.md`

The plan (lanes, their documents and their order), leads, papercuts, brief drafts, tasks and status live in Queue. Read what's next with `plan.get` (see the `northstar` skill). The repository holds product knowledge and code.

## Commands

- `effigy graph` for code ownership and changed-file impact.
- `effigy docs context "<question>"` for documentation authority.
- `effigy tasks` for selector inventory; `effigy doctor` for routing or health ambiguity.
- `effigy test --plan` when test selection matters; narrow Effigy selectors for task validation.
- Without an installed binary, use `sh scripts/effigy-selfbuild.sh <command>`. It builds this checkout through Oxide and runs the built `target/oxide-quick/effigy`. `cargo run --bin effigy -- <command>` is the plain-Cargo escape.

## Guardrails

- Use the current checkout. Do not add a current-directory `--repo` override.
- Do not change `.github/workflows/` or start a release without explicit human instruction. Never bypass release gates or re-tag a failed release.
- Do not add package scripts that re-export Effigy tasks.
- Update the owning knowledge file when product truth changes. Record operator rulings there before handing over.
- Dispatch product implementation and retirement work as Queue tasks with approved briefs. Chatterbox owns planning and knowledge maintenance.
- Tom's standing authority (2026-09-30): "You have blanket approval from me, you don't need to keep asking." Chatterbox may brief, approve and dispatch bounded work within the agreed Effigy plan without asking per task. Preserve independent review, CI, prerequisites and closeout. Escalate material direction changes or actions outside the plan; the explicit workflow, release and destructive host-cleanup boundaries still apply.
- File small recurring friction with Queue `papercut.add`; record unplanned ideas and observations with `lead.add`.
- Add user-facing changes under `CHANGELOG.md` `[Unreleased]`.

## Validate

Workers run the changed-code tests, compile the touched targets, and run docs checks when docs changed, once. Briefs name those targeted Effigy selectors. Then open the PR and report; no whole suites or repeat passes. Reviewers read the diff, run the same targeted checks, and exercise the behavior. A re-review checks only the change since the last reviewed head, plus the targeted checks for that change. A base that has moved on is not a content finding. Anyone who starts fixture processes stops them by recorded PID, and removes the fixture, before reporting. Hosted CI is not a task gate (Tom, 2026-10-20). Workers and reviewers don't wait for it or hold a PR for it. A task merges once the targeted checks for its change pass. Hosted failures outside the change are noted in the PR and fixed holistically at milestone QA.

The planner runs milestone full QA on `main` through Queue `project.qa.run` and reads its result with `project.qa.get`, then briefs fixes for failures. Do not configure a per-task Queue validation command. Run `effigy skill run northstar/retired-concepts` when retiring concepts.

Run validation through Effigy selectors, not raw Cargo, Bun, or Vitest commands. During the v0.15 rebuild, this checkout's `effigy.toml` uses keys the installed v0.14.1 binary rejects. Run selectors with the worktree build, `sh scripts/effigy-selfbuild.sh <selector>`, not the installed `effigy`. Selectors run Cargo through `scripts/oxide-selector.sh`, so Rust intermediates land in Oxide's shared slot pool instead of `target/`; selectors that pin a `target/` binary path, a non-Oxide Cargo profile or a scrubbed `HOME` stay on Cargo. Escapes when Oxide is broken: `EFFIGY_OXIDE_ESCAPE=cargo effigy <selector>` for one selector, `effigy cargo <cargo args>` for one command, `cargo run --bin effigy -- <selector>` for the whole self-build. Record background exit codes. Stop only processes you started, using their recorded PIDs or process groups; never use pattern kills (`pkill -f`, `killall`, or `pgrep` piped into `kill`).

<!-- northstar:rust-quality:start -->
## Northstar Rust Quality

Scope: Rust source, Cargo manifests, build files, tests, and directly related
documentation under this directory.

Use Northstar's strict everyday-authoring route for ordinary Rust work. Resolve
the repository-owned profile and deviations under `docs/knowledge/contracts/`; never
assume a universal MSRV. Re-enter at task start and coherent batch closeout.
Preserve unrelated work. A quality audit, no-slop pass, or audit-and-fix request
is explicit audit intent; never route it through everyday authoring.
<!-- northstar:rust-quality:end -->

<!-- oxide:begin -->
## Rust commands: use `oxide`

- Write `oxide` wherever you would write `cargo`: `oxide check`, `oxide test`, `oxide run -- <args>`, `oxide add <crate>`.
- Profiles: `quick` (default), `--profile debug` for a debugger, `--release`. Oxide places build output, so it refuses `--target-dir`.
- A `waiting for …` line means another build holds a slot; let it wait. `oxide status` shows the pool, slots and waits.
- Use `oxide clean`, not `cargo clean`.

Managed by `oxide init`; edits between these markers are replaced.
<!-- oxide:end -->

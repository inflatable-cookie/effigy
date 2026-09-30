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
- Without an installed binary, use `cargo run --bin effigy -- <command>`.

## Guardrails

- Use the current checkout. Do not add a current-directory `--repo` override.
- Do not change `.github/workflows/` or start a release without explicit human instruction. Never bypass release gates or re-tag a failed release.
- Do not add package scripts that re-export Effigy tasks.
- Update the owning knowledge file when product truth changes. Record operator rulings there before handing over.
- Dispatch product implementation and retirement work as Queue tasks with approved briefs. Chatterbox owns planning and knowledge maintenance.
- File small recurring friction with Queue `papercut.add`; record unplanned ideas and observations with `lead.add`.
- Add user-facing changes under `CHANGELOG.md` `[Unreleased]`.

## Validate

Workers run the changed-code tests, compile the touched targets, and run docs checks when docs changed, once. Briefs name those targeted Effigy selectors. Then open the PR and report; no whole suites or repeat passes. Reviewers read the diff, run the same targeted checks, and exercise the behavior.

The planner runs milestone full QA on `main` through Queue `project.qa.run` and reads its result with `project.qa.get`, then briefs fixes for failures. Do not configure a per-task Queue validation command. Run `effigy skill run northstar/retired-concepts` when retiring concepts.

Run validation through Effigy selectors, not raw Cargo, Bun, or Vitest commands. Record background exit codes. Stop only processes you started, using their recorded PIDs or process groups; never use pattern kills (`pkill -f`, `killall`, or `pgrep` piped into `kill`).

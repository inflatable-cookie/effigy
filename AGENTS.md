# Effigy

Effigy is a Rust task runner for monorepos. Its CLI and `effigy.toml` give agents and humans one predictable surface for tasks, tests, local systems, diagnostics, and release checks. Keep routing explicit and JSON output stable.

## Where things live

- Current state: `docs/README.md`
- Current knowledge: `docs/knowledge/README.md`
- Retired concepts: `docs/knowledge/retired.toml`
- Open questions: `docs/knowledge/questions.md`
- Intent: `docs/plan.md`
- Unresolved leads: `docs/triage/`
- User documentation: `docs/guides/README.md`

Tasks, briefs, status, and outcomes live in Queue. The repository holds product knowledge and code.

## Commands

- `effigy graph` for code ownership and changed-file impact.
- `effigy docs context "<question>"` for documentation authority.
- `effigy tasks` for selector inventory; `effigy doctor` for routing or health ambiguity.
- `effigy test --plan` when test selection matters; `effigy qa` for PR validation.
- Without an installed binary, use `cargo run --bin effigy -- <command>`.

## Guardrails

- Use the current checkout. Do not add a current-directory `--repo` override.
- Do not change `.github/workflows/` or start a release without explicit human instruction. Never bypass release gates or re-tag a failed release.
- Do not add package scripts that re-export Effigy tasks.
- Update the owning knowledge file when product truth changes. Record operator rulings there before handing over.
- Append solvable friction to `PAPERCUTS.md`; leave unresolved leads in `docs/triage/` until planned.
- Add user-facing changes under `CHANGELOG.md` `[Unreleased]`.

## Validate

Run `effigy qa` before opening a PR. The command builds from this checkout with Cargo and does not require prepared local dependencies. Also run `effigy skill run northstar-lean/retired-concepts` when retiring concepts.

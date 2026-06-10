# Monkey Consumer Contract Gap Assessment

Date: 2026-03-12
Owner: Platform
Target repo: `~/Dev/projects/pilot-repo-a`
Reference contract:
[`../guides/056-northstar-effigy-consumer-repo-contract.md`](../guides/056-northstar-effigy-consumer-repo-contract.md)

## Summary

Assessed `pilot-repo-a` against the minimum Northstar + Effigy consumer repo
contract.

`pilot-repo-a` is a strong first pilot because it already has:

- `docs/README.md`
- `docs/vision/`
- `docs/roadmaps/`
- `docs/logs/`
- `docs/guides/`
- a simple root `effigy.toml`

It is missing the parts that make the contract end-to-end and enforceable:

- an Effigy-first `AGENTS.md` contract
- changelog baseline
- release baseline
- docs and Northstar validation bundles

That makes it a good proving ground: enough structure to avoid scaffolding
noise, but still missing the exact adoption-kit surfaces this roadmap needs to
standardize.

## Current State Snapshot

### Present

- root `effigy.toml`
- root `AGENTS.md`
- `docs/README.md`
- `docs/vision/README.md`
- `docs/roadmaps/README.md`
- `docs/logs/README.md`
- active roadmap queue
- meaningful guides and contracts

### Missing

- `CHANGELOG.md`
- root `[release]` config in `effigy.toml`
- `qa:docs`
- `qa:northstar`
- explicit docs-policy or equivalent validation layer
- Effigy-first agent loop in `AGENTS.md`

## Contract Checklist Assessment

- [x] Root `effigy.toml` exists
- [ ] `AGENTS.md` teaches the Effigy-first loop
- [ ] `--repo .` is not taught as a default
- [x] `docs/README.md` exists and names the docs authority
- [x] `docs/vision/README.md` exists
- [x] `docs/roadmaps/README.md` exists
- [x] `docs/logs/README.md` exists
- [x] vision document defines long-term outcome and constraints
- [x] roadmap queue exists and has a clear next milestone
- [ ] `CHANGELOG.md` exists
- [ ] release baseline is documented or intentionally deferred
- [ ] repo has a contract validation path beyond raw human review

## Detailed Gaps

### 1. AGENTS contract is Northstar-aware but not Effigy-first

Current `AGENTS.md` documents project context, architecture, and references,
but it does not yet provide the reusable Effigy-first loop:

- no `effigy tasks`
- no `effigy doctor` or `effigy health` default contract
- no `effigy test --plan`
- no explicit machine-readable guidance
- no policy on `--repo <PATH>` versus local-root execution

### 2. Test semantics are still task-owned, not clearly framed for adoption

`pilot-repo-a` defines:

- `check = "cargo check --workspace"`
- `test = "cargo test --workspace"`
- `validate = [ ..., { task = "test" } ]`

That is workable, but the repo does not yet explain whether:

- `tasks.test` is the intentional source of truth, or
- built-in `effigy test` should become the default contract

### 3. Changelog and release are missing

`pilot-repo-a` is ready to be prepared for an initial release, but the repo does not
yet have:

- `CHANGELOG.md`
- `[release]` config
- documented release gates
- a release validation path

This is the largest missing contract area and the main reason `pilot-repo-a` is a
good pilot.

### 4. Docs structure exists, but validation does not

The docs tree is already strong, but there is no visible repo-owned validation
bundle for:

- docs links
- docs indexes
- required headings/metadata
- agent/default drift
- roadmap/log structure drift

### 5. Docs authority is broad, but consumer semantics are still implicit

`pilot-repo-a` clearly uses Northstar structure, but it does not yet explicitly say
what the phrase "use Northstar and Effigy" means in operator terms. The shape
exists; the reusable contract does not.

## Recommended First Normalization Batch

1. Rewrite `AGENTS.md` around the Effigy-first contract.
2. Decide and document whether `tasks.test` remains the default test source of
   truth or whether built-in `effigy test` should take over.
3. Add `CHANGELOG.md` using the Northstar profile starter shape.
4. Add minimal `[release]` config to `effigy.toml`.
5. Add `qa:docs` and `qa:northstar` starter task bundles.
6. Add at least one forbidden-text drift guard for active agent/setup/workflow
   surfaces.

## Validation Evidence

- read `AGENTS.md`
- read `effigy.toml`
- read `docs/README.md`
- read `docs/vision/001-pilot-repo-a-vision.md`
- read `docs/roadmaps/g01/README.md`
- read `docs/logs/README.md`

## Conclusion

`pilot-repo-a` is a better first pilot than `pilot-repo-b` for Wave 1 because it is
structurally mature enough to test the contract seriously, but still simple
enough that the first adoption batch will not be dominated by workspace
orchestration complexity.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `RELEASE`, `MAINT`
- Moved from: `pilot choice based on repo familiarity and rough intuition`
  to `explicit contract-based assessment showing why pilot-repo-a is a cleaner first
  proving ground than a larger workspace repo`
- Remaining open:
  - normalize `pilot-repo-a` against the contract
  - capture which gaps belong in the skill versus Effigy product surface
  - use the result to harden the reusable starter file set

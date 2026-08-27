---
title: Northstar AGENTS and Rust audit worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / Northstar orchestrator
created: 2026-08-27
updated: 2026-08-27
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260827-171858-northstar-agents-rust-audit-worker.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, agents-audit, rust-audit]
---

## What This Thread Was Doing

The operator asked Northstar to audit Effigy's always-loaded agent instructions,
then run the strict repository-scope Rust quality audit. These are one serial
maintenance runway: the instruction audit establishes whether unfamiliar agents
receive the right project context and boundaries; the Rust audit then applies
Northstar's finding-first recorder and repairs only what its checked authority
allows.

This is the only handoff from the orchestrator to the worker. Start from this
file without a copied transcript or a second prompt.

## Why It Matters

Effigy is both a Rust CLI and an agent-facing repo runtime. Its always-loaded
instructions need to preserve project intent without wasting context, and its
Rust implementation needs an evidence-backed quality pass that cannot turn into
blanket cleanup. Running the audits in this order separates instruction-surface
judgment from code repair authority.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `9f895a29b2a2bc694acedc358129e1b0e701a7b0`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  the planning base before this handoff was created.
- **Planning checkout:** clean before this handoff was added.
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates the worker-only worktree preflight.
- **Planning artifacts included at the base:** this committed worker handoff.
- **Worker branch:** `worker/northstar-agents-rust-audit`
- **Worker worktree:** `/Users/tom/Dev/worktrees/effigy-northstar-agents-rust-audit`
- **Worktree creation command:** `git worktree add /Users/tom/Dev/worktrees/effigy-northstar-agents-rust-audit -b worker/northstar-agents-rust-audit origin/main`
- **Worker worktree policy:** first use a clean, dedicated, non-`main`
  registered worktree supplied by the launcher. If the dispatch starts in the
  orchestrator's `main` checkout rather than a provider-owned worker worktree,
  use the named pre-created worktree above. Record the selected path and branch;
  do not create a second worktree. Only if neither is usable may you read
  `.agents.local.env`, require `AGENTS_WORKTREE_CONTAINER_DIR`, and create a
  unique fallback under that configured container. Never use `/tmp`, `TMPDIR`,
  or a guessed path.
- **Active spec lane:** none; the repository is `strict-paused` and this is an
  operator-authorized maintenance audit, not a product roadmap lane.
- **Roadmap milestone:** none; do not open a generation or product milestone.
- **Ready cards, in order:** (1) Northstar AGENTS instruction audit, read-only;
  (2) Northstar strict repository-scope Rust audit and recorder-authorized
  repairs.
- **Allowed runway:** those two ordered audit phases, generated Rust audit setup
  required by the canonical mode, one consolidated audit evidence log, and a
  reviewable PR.
- **Remaining card budget:** two phases, serial.
- **Dispatch topology:** one worker; phase two begins only after phase one is
  completely assessed and reported.
- **Parallel safety check:** serial because Rust setup may add a marked block to
  `AGENTS.md`; parallel execution would overlap the instruction surface being
  audited.
- **Canonical refs:** `README.md`; `AGENTS.md`; `CLAUDE.md`; `docs/README.md`;
  `docs/architecture/000-overview.md`; `docs/architecture/product-guardrails.md`;
  `docs/contracts/001-working-rules.md`; `docs/policy/internal-writing-style.md`;
  `PAPERCUTS.md`; the installed Northstar `northstar-agents-review` and
  `northstar-rust-audit` skills and every required reference they route to.
- **Model capability profile:** capable coding model, medium reasoning.
- **Tool/runtime restrictions:** use Effigy for repo-owned work; use the
  verified absolute Northstar Rust audit payload and pinned `stopslop 0.5.1`;
  never edit `.github/workflows/`; never run release mutations; never add
  package-manager wrappers; do not run blanket formatting or blanket lint fixes.
- **Required validation:** the installed Northstar consumer-safe AGENTS audit;
  Rust recorder `inspect`, `plan`, `init`, complete unit assessments, checked
  evidence collection for applied repairs, and `finalize`; focused tests for
  each repair; `effigy health`; `cargo fmt --all -- --check` without broad
  auto-formatting; `cargo clippy --all-targets -- -D warnings`; `effigy qa` at
  closeout when the preceding gates are green.
- **PR base/head:** current pushed `main` / selected worker branch.
- **PR URL:** pending.
- **Review state:** awaiting orchestrator review after worker completion.
- **Merge authorisation:** absent; do not merge.

## Boundaries

Please keep this run inside the two ordered audits:

- **In scope, phase one:** review the whole applicable `AGENTS.md`/`CLAUDE.md`
  chain, run the consumer-safe mechanical audit, build a section-intent map,
  assess the reader journey and bridge, and report dispositions and ambiguity.
- **In scope, phase two:** resolve repository scope, install the canonical Rust
  activation/profile setup if missing, run the strict finding-first audit across
  the discovered Cargo repository, and apply only `review_required` repair plans
  accepted by the recorder.
- **In scope, closeout:** one consolidated evidence log under
  `docs/logs/2026-08/` and its log-index entry, plus changelog text only when a
  user-facing Rust repair warrants it.
- **Out of scope:** applying phase-one AGENTS optimization recommendations;
  product feature work; architecture replacement; breaking API or compatibility
  decisions; foreign error-policy changes; MSRV changes; unsafe-boundary repair;
  release work; CI workflow edits; unrelated cleanup; the existing stale graph
  index and god-file warnings.
- Phase one is advisory and read-only. Do not edit `AGENTS.md`, `CLAUDE.md`, or
  any other file from its recommendations. Phase two may later change
  `AGENTS.md` only through the canonical generated Rust activation block; do not
  conflate that setup mutation with an instruction-review recommendation.
- Record `operator_decision`, `report_only`, unsafe, compatibility, public-API,
  or architecture findings without repairing them. Stop and report when the
  canonical Rust mode requires operator direction.
- Units with no authorized repair must remain byte-for-byte unchanged. Preserve
  excluded, read-only, user-owned, and unrelated files exactly.
- Do not invent architecture, change contracts, widen the roadmap, or turn an
  evaluation-only scanner candidate into repair authority.
- Work only in the selected clean worker worktree. Never edit the orchestrator's
  planning checkout or clean/reset/stash over unrelated state.
- Do not merge the PR. Merge remains a separate operator-authorized action.

## Important Context

- **Planning lineage:** `g08` remains the current generation, but all advertised
  milestones are complete and no strict lane is active. The roadmap and working
  rules await operator intent; this explicit request supplies a bounded audit
  runway without authorizing `g09`, release work, or a new product milestone.
- **Repository posture:** `strict-paused`, with clean pushed `main`. `effigy
  doctor` reports only two pre-existing warnings: a stale graph index and four
  warning-level god-file findings.
- **AGENTS audit route:** no target-local `check:agent-instructions` task is
  defined. Use the consumer-safe installed catalog:
  `effigy --repo /Users/tom/.agents/skills/northstar northstar/check:agent-instructions /absolute/path/to/selected/worker/repo`.
- **Rust setup state:** the marked Northstar Rust activation block,
  `docs/contracts/rust-quality-profile.json`, and
  `docs/contracts/rust-quality-deviations.json` were absent at dispatch. Follow
  the canonical setup route; do not hand-author those surfaces.
- **Audit scope:** `repository`, covering every Cargo workspace/package/target
  discovered by the checked tool. Full coverage is assessment coverage, not
  blanket rewrite authority.
- **Decisions and preferences:** keep the AGENTS audit read-only; prefer
  surgical Rust repair waves; preserve Effigy's manifest-driven routing,
  machine-readable contracts, explainability, operator safety, and modular
  ownership boundaries.
- **Open tensions:** a whole-repository Rust audit can surface operator-decision
  or report-only findings. Record those honestly and stop where the mode says to
  stop; do not force a green/no-finding narrative.
- **Report after:** phase-one instruction audit; Rust scope initialization;
  every coherent Rust repair wave; finalization and PR creation.
- **Report to:** the operator/orchestrator through the collaboration channel.

## Suggested Next Move

Read this file from the top. Its metadata activates worker mode. Before any
broad repository read, run the startup worktree-safety preflight below. This
dispatch uses the named pre-created worktree when no provider-owned worker
worktree is supplied; select it, record it, and do not create another.

Once the worktree is settled, run phase one completely and report its findings
without editing the instruction surfaces. Then load the Rust audit contract,
perform its generated setup if required, and proceed through repository-scope
discovery and finding-first assessment. Do not edit Rust before the recorder has
the complete unit assessment and an authorized repair plan.

## Completion Protocol

### Before you start

1. Read this handoff path and confirm `handoff_mode: worker-pr-loop`,
   `worker_mode: implementation`, and `dispatch_authority: orchestrator`.
2. Before broad reads run: `git rev-parse --show-toplevel`, `git branch
   --show-current`, `git status --porcelain`, and `git worktree list
   --porcelain`.
3. If the current context is a clean, dedicated, non-`main` registered worker
   worktree, use it. If the collaboration dispatch begins in the orchestrator's
   `main` checkout, switch all subsequent commands to the already-created named
   worktree `/Users/tom/Dev/worktrees/effigy-northstar-agents-rust-audit` and
   branch `worker/northstar-agents-rust-audit`. Do not create another worktree.
   If neither context is usable, use the `.agents.local.env` fallback rules and
   stop for the operator if they cannot be satisfied. Never use `/tmp`.
4. From the selected worker worktree run `git fetch origin`; confirm `HEAD ==
   origin/main`; confirm `git merge-base --is-ancestor
   9f895a29b2a2bc694acedc358129e1b0e701a7b0 HEAD`; confirm this handoff exists in
   `HEAD`; and confirm the worktree is clean.
5. Read `AGENTS.md`, `CLAUDE.md`, `README.md`, `docs/README.md`,
   `docs/contracts/001-working-rules.md`, the named architecture/guardrail
   references, `PAPERCUTS.md`, and the installed Northstar mode contracts.
6. Run `effigy tasks` and `effigy doctor`; record the actual orientation state.

### Phase one: AGENTS instruction audit

1. Follow `northstar-agents-review` and its canonical
   `agent-instruction-review.md` route exactly.
2. Run the installed consumer-safe audit against the selected worker repo.
3. Build the section-intent map and reader-journey assessment. Verify
   `CLAUDE.md` contains the exact `@AGENTS.md` bridge.
4. Report findings, dispositions, measurements, preserved boundaries, and
   ambiguity through the collaboration channel. Do not mutate any file in this
   phase and do not wait for permission to continue unless a stop condition is
   hit.

### Phase two: strict Rust repository audit

1. Follow `northstar-rust-audit`, `rust-quality-audit.md`, the strict projection,
   recorder contract, tool bootstrap, and evidence collection contract exactly.
2. Because activation/profile surfaces were absent at dispatch, use the
   canonical installed-catalog setup command for the narrowest Rust-owning root;
   do not hand-write setup files.
3. Resolve `repository` scope. Use the checked tool for `inspect`, `plan`,
   `init`, three-pass unit assessment, `extend` when required before mutation,
   checked evidence collection, unit completion, and `finalize`.
4. Record a complete verdict for every normative rule and every assessed unit.
   Build the total exact-forwarder ledger for `RUST-SLOP-001`; it remains
   evaluation-only/report-only. Treat `RUST-UNSAFE-001` as report-only.
5. Apply only coherent recorder-authorized `review_required` repair waves.
   Report each wave and its actual validation before moving on.

### When the assigned runway is complete

1. Finalize the Rust audit and retain its deterministic report/evidence in the
   repository Git metadata. Write one consolidated tracked evidence log under
   `docs/logs/2026-08/` with the AGENTS audit outcome, Rust audit ID/catalogue
   hash, findings/dispositions, repair waves, setup changes, preservation proof,
   validation, limitations, and `## Vision Target Delta`; add it to
   `docs/logs/README.md`.
2. Run the required final validation named in `## Current State`. Do not run
   `effigy qa` until the focused and cheap gates are green.
3. Inspect the final diff, verify excluded/read-only/no-repair units stayed
   unchanged, and ensure no phase-one recommendation leaked into `AGENTS.md`.
4. Commit meaningful chunks, push `worker/northstar-agents-rust-audit`, and open
   a reviewable PR against the current pushed `main` tip.
5. The PR body must link this handoff and consolidated log; name changed
   surfaces, audit ID, findings and dispositions, validation, limitations,
   unresolved operator decisions, and any setup-generated instruction change.
6. Report the PR URL and evidence through the collaboration channel. Do not
   merge.

### Review and merge path

The orchestrator will review the PR independently against this handoff, the
canonical audit contracts, the diff, and checks. Because the worker and
orchestrator share a GitHub identity, the orchestrator will post its verdict as
a PR comment when formal self-approval is unavailable. Current review state:
awaiting orchestrator review. Requested changes: none yet.

Merge requires a separate explicit operator instruction after the review/check
gate passes.

- **Closeout refs:** this handoff; the consolidated audit log;
  `docs/logs/README.md`; the Rust audit record under repository Git metadata;
  PR URL pending.

### Handoff closeout

Leave the audit record, tracked evidence, and next-task state honest. Do not
change the roadmap's existing governance `Next Task`; this maintenance runway
does not replace it. If the audit is blocked or produces an operator-decision
stop, record and report that limitation instead of presenting the runway as
complete.

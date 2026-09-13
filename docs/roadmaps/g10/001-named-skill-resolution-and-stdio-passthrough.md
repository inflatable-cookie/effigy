# 001 - Named Skill Resolution and Stdio Passthrough

Status: complete
Owner: task routing and execution
Created: 2026-09-13
Governing refs: `docs/contracts/042-external-skill-task-runner-contract.md`, `docs/architecture/025-external-skill-task-execution.md`
Depends on: none

## Outcome

`effigy skill run northstar/queue:hook` can resolve the `northstar` agent skill
from the invocation project or installed user roots when `--path` is absent.
An explicit `--stdio passthrough` mode can then carry arbitrary child bytes and
exit status without Effigy-owned output.

## Ready-State Rubric

- [x] Objective is bounded enough to finish without fresh planning decisions.
- [x] Governing refs point at current canonical surfaces.
- [x] Scope, acceptance, validation, evidence, and stop conditions are explicit.
- [x] The review oracle covers exact byte transport, source precedence, and
  no-execution failures.
- [x] Continuation returns to Chatterbox; no speculative sibling is ready.
- [x] Operator selected this direction and supplied the passthrough acceptance
  contract on 2026-09-13.

## Decisions

- Keep `--path` unchanged and authoritative when present. It remains required
  for `skill tasks`; only `skill run` gains selector-derived lookup.
- Derive the skill name from the first segment of a qualified selector. Keep
  the full selector unchanged for task selection.
- Resolve from the invocation project, then unique-global across
  `~/.agents/skills`, `~/.codex/skills`, `~/.claude/skills`, and
  `~/.cursor/skills`. Project-local wins. Distinct global collisions fail.
- Use `--stdio passthrough`. Do not overload `--json`: JSON means Effigy owns a
  versioned envelope; passthrough means the child owns raw stdio.
- Keep preflight and every existing isolation rejection before process launch.

## Dispatch manifest

- **State:** complete; no parallel siblings
- **Completion:** implementation and docs merged, raw-boundary and regression
  proofs green, canonical closeout complete
- **Owned mutable paths:** `crates/effigy-cli/src/lib.rs`,
  `crates/effigy-cli/src/command_parsing.rs`,
  `crates/effigy-cli/src/global_json.rs`,
  `crates/effigy-cli/src/help/topics/skill.rs`,
  `crates/effigy-execution/src/lib.rs`, `src/cli/entrypoint.rs`,
  `src/cli/output/**`, `src/runner/skill_command.rs`, `src/runner/execute/**`,
  `src/tests/lib_tests_parse_tests/skill_option_tests.rs`,
  `tests/cli_output_tests/skill_command_tests.rs`, relevant skill-run fixtures,
  `docs/contracts/042-external-skill-task-runner-contract.md`,
  `docs/architecture/025-external-skill-task-execution.md`, current skill-run
  guides/help references, `.agents/skills/effigy/**`, `skills/effigy/**`,
  `CHANGELOG.md`, `PAPERCUTS.md`, and this task's closeout evidence
- **Reserved closeout surfaces:** `docs/roadmaps/g10/README.md`,
  `docs/roadmaps/README.md`, `docs/roadmaps/generation-index.md`,
  `docs/contracts/001-working-rules.md`, `docs/logs/README.md`, and the dispatch
  handoff may change only during canonical closeout
- **Worker:** automatic general pool; no frontier profile required
- **Excluded:** named lookup for `skill tasks`, remote acquisition, registry
  behavior, consumer-catalog merging, schema-specific hook logic, release or
  workflow changes, and widening rejected managed/TUI/concurrent/container or
  escaping task shapes
- **Escalation:** operator owns any discovery-root, compatibility, or public
  syntax change beyond the decisions above

## Work

1. Extend the CLI AST, parser, validation, and help for optional run paths and
   `--stdio passthrough`; reject passthrough plus any Effigy `--json` before
   execution.
2. Implement deterministic project/global skill-name resolution from the
   invocation context while preserving explicit-path and consumer-target rules.
3. Add a raw execution/output path that inherits stdin/stdout/stderr, bypasses
   Effigy rendering, and preserves child status without weakening preflight.
4. Add subprocess fixtures and adversarial tests for raw bytes, failures,
   discovery precedence/ambiguity, and unchanged default/JSON behavior.
5. Update user guidance, synchronized Effigy skills, changelog, and closeout
   evidence.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Explicit source behavior is unchanged | `--path` is reinterpreted or named discovery participates | Existing and new explicit-path parser/integration tests |
| Named lookup is deterministic and invocation-scoped | `--repo` changes the source, global shadows project, or two distinct globals run arbitrarily | Project/global fixtures, alternate consumer, collision failure, canonical symlink-dedup proof |
| Transport is byte-exact | JSON stdin lacks a final newline or child emits non-UTF-8/unterminated output and Effigy rewrites it | Capture the Effigy subprocess as raw bytes and compare stdin, stdout, and stderr exactly |
| Child owns both output streams | Effigy adds its header/footer/newline or redirects child stderr | Success and failure fixtures with distinct raw stdout/stderr assertions |
| Status is transparent | a child exit `23` becomes Effigy exit `1` | Assertions for exit `0` and a non-zero child status |
| Effigy failures are transport-safe | missing skill, rejected task, or launch failure writes a synthetic stdout payload | Empty stdout, useful stderr, non-zero status, and no side-effect marker |
| JSON and passthrough cannot conflict | global or local `--json` reaches task execution with passthrough | Both flag placements rejected before a marker-writing fixture runs |
| Existing modes remain compatible | normal text loses its resolution report or JSON loses `effigy.command.v1` | Current human/JSON snapshots and contract tests remain unchanged |
| Named and explicit sources keep isolation | consumer task/default/container/secret config leaks into a named skill | Existing isolation suite replayed through named passthrough where applicable |
| Generic JSON hook shape works | Effigy parses, wraps, or appends to the task's one JSON object | Generic fixture reads raw stdin, emits one JSON object, and boundary stdout parses as exactly that object |

## Validation

- targeted CLI parser/help and skill-command unit tests
- raw subprocess integration tests for all acceptance cases
- `cargo test --test cli_output_tests skill_ -- --nocapture`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `effigy qa:docs`
- `effigy qa:json` when JSON fixtures or schema evidence changes
- `git diff --check`

## Stop conditions

- Stop if raw transport requires skipping source/task-graph preflight or
  weakening current isolation.
- Stop if exact exit propagation requires a broad CLI error-contract rewrite;
  return with the smallest architectural decision needed.
- Stop on a new discovery root, fallback rule, or compatibility choice not
  settled above.

## Evidence

On completion, record outcome, validation run, PR link, reviewed exact head,
merge commit, and material limits or blockers.

### Implementation (worker PR, pre-merge) — 2026-09-13

- **Outcome reached:** `effigy skill run <skill>/<task>` resolves an installed
  agent skill without `--path` (invocation project first, then unique global
  under `~/.agents/skills`, `~/.codex/skills`, `~/.claude/skills`,
  `~/.cursor/skills`), and `--stdio passthrough` hands the selected host task
  raw stdin/stdout/stderr plus its exit status with no Effigy framing.
  `--path` remains authoritative and `skill tasks` still requires it.
- **Code:** `crates/effigy-cli` grammar/help/global-flag conflict checks,
  `crates/effigy-execution` typed passthrough output mode,
  `src/runner/skill_command.rs` named resolution and the passthrough run path,
  `src/runner/entrypoints*` passthrough dispatch, and `src/cli/entrypoint.rs`
  renderer/envelope bypass.
- **Validation run:** `cargo test` (1499 lib tests plus 323 `cli_output_tests`,
  all green), `cargo test --test cli_output_tests skill_`,
  `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `effigy qa:docs`,
  `effigy qa:json`, and `git diff --check`.
- **Adversarial proof added:** non-UTF-8 unterminated stdin round-trips
  byte-for-byte; distinct raw stdout/stderr; child exit `23`; `--json` plus
  passthrough rejected in both global and local flag positions before a
  marker fixture runs; project-local source wins over a global copy; unique
  global resolves; two distinct globals fail closed with no side effect;
  symlink aliases collapse to one candidate; an incomplete project-local copy
  fails closed instead of falling through; `--repo` does not change discovery;
  named sources keep host-only isolation before side effects.
- **Friction fixed in lane:** the planning commit's vision `020` `## Next Task`
  lead verb failed the repository's own `docs check next-action --policy
  vision`; the sentence now leads with an allowlisted verb and the friction is
  recorded in `PAPERCUTS.md`.
- **Limits:** child status crosses exactly for direct and fail-fast sequential
  host shell steps; a non-fail-fast sequence still reports `1` by existing
  behavior. No broad CLI error/output rewrite was required. Merge commit and
  reviewed head are recorded at canonical closeout.

### Canonical closeout — 2026-09-13

- **Merged outcome:** PR [#104](https://github.com/inflatable-cookie/effigy/pull/104)
  merged into `main` as `189c0a71cf5772dfd3e788f6ab650eadd816466b`. The
  integration checkout and `origin/main` are synchronized at that commit.
- **Review:** independent review accepted the exact implementation head
  `26cf5350676e6d3aa15a3fdc862db222630fa9f5` in PR comment
  [5652458474](https://github.com/inflatable-cookie/effigy/pull/104#issuecomment-5652458474).
  All ten acceptance rows were evidenced, with no blocking findings or
  unresolved review threads.
- **Validation:** worker and reviewer records report green full and targeted
  Rust validation, formatting, clippy, documentation, JSON-contract, and diff
  checks. Closeout reran the focused documentation check and diff check on the
  synchronized integration checkout; both passed.
- **Recovered friction:** the first post-merge synchronization attempt stopped
  on `git ls-tree: fatal: not a tree object`. The recorded merge object was
  valid and the plugin's single closeout retry synchronized `main`; no failure
  remains deferred from that incident.
- **Deferred limits:** non-fail-fast sequential host steps retain their
  existing exit-status flattening to `1`; the unused passthrough enum variant
  remains a non-blocking implementation note. Neither changes the accepted
  task outcome.

## Next task

Return to Chatterbox for planning direction. Do not compile `g10.002` without
operator direction.

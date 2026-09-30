# Bounded QA Groups Workflow

Status: proposed. QA-group commands and grammar are not available yet. The
current selector workflow below remains the executable path until the runtime
and agent-skill adoption land.

Contract: [051](../knowledge/contracts/051-bounded-qa-groups-contract.md)
Architecture: [031](../knowledge/architecture/031-bounded-qa-groups-runtime.md)

## Current workflow

Use the repo's existing Effigy tasks. Inspect a known selector before running
it when the command shape matters:

```sh
effigy tasks
effigy <selector> --plan
effigy <selector>
```

`effigy drafts` and `effigy draft <name>` remain for temporary ordinary tasks
and environments. They are not QA groups. `effigy qa`, CI, and release gates
keep their current owners and meanings. A worker with uncertain coverage asks
the planner which proof is needed; uncertainty does not mean “run the full
board.”

## Proposed group workflow

After implementation, inventory maintained groups and run a selected group
through the `tasks` dispatcher:

```sh
effigy tasks qa-groups list
effigy tasks qa-group run rust-cli --plan
effigy tasks qa-group run rust-cli
effigy tasks qa-group status <run-id>
effigy tasks qa-group logs <run-id>
```

A one-off group requires an explicit file every time:

```sh
effigy tasks qa-groups list --file config/qa-groups/2026-10-02-binding-check.toml
effigy tasks qa-group run binding-check --file config/qa-groups/2026-10-02-binding-check.toml --plan
effigy tasks qa-group run binding-check --file config/qa-groups/2026-10-02-binding-check.toml
```

The file path is part of definition identity. Effigy does not search its
directory. A temporary group expires for inventory and cleanup purposes; an
expired definition remains runnable when selected explicitly. Expected time
and expiry answer different questions.

Read the plan for the exact selectors, fixed arguments, targets, companions,
limits, admission class, and definition source. If a task is broad, its
declared target is uncertain, or an input has no mapped selector, stop at
`needs_planner`. Do not call an aggregate a bounded check because its name
looks narrow.

## Longhorn pilot examples

The examples below use the read-only Longhorn checkout at `77e0d9873ac278c83729606e0aef9916b36950d0`.
Its `docs/knowledge/validation-input-map.md` is unchanged from reviewed PR61
commit `58baccf8ec630557d51096241c85d9c8c211d46b`. The map is Longhorn's gate
evidence, not an Effigy selector contract or proof of complete coverage.

| Work context | Existing runnable selectors | Proposed bounded group or next selector | Boundary |
| --- | --- | --- | --- |
| Docs-only, general knowledge/index prose | `qa:docs` (which contains link, agent-default, path, and catalog-link checks). | A `longhorn-docs` group can run `qa:docs`, with those four checks shown in the plan if task expansion supports it. | `qa:docs` is an aggregate. It proves its declared docs checks, not prose correctness or all docs consumers. |
| Edit `docs/guides/getting-started.md` | `qa:docs` plus `proof:artifacts`. | Add a standalone `proof:guides-card126` selector and include it in `longhorn-getting-started-docs`. | The map's guide-content verifier is one of fourteen `proof:artifacts` members, not a runnable selector today. Current `proof:artifacts` is heavy and broad, so this is not yet a bounded docs group. |
| Rust package `longhorn-agent-tool-dispatch` | `check:agent-tool-dispatch`. Its aggregate runs `lint:agent-tool-dispatch`, `test:agent-tool-dispatch`, `docs:agent-tool-dispatch`, `check:agent-tool-dispatch-release-absence`, and `check:api-reference`. | A `agent-tool-dispatch` group can select the current package aggregate and declare the crate plus dependency closure. | This is the only per-crate Rust aggregate in the map. It does not establish package-filter support for other crates. |
| Another Rust crate, such as `longhorn-credential-keyring` | `fmt:rust`, `lint:rust`, `lint:rust:features`, `test:rust`. | After lead `035b121a` or an equivalent scoped-selector change, add package-filtered test and compile selectors, then group them. | Current selectors span the workspace. They are runnable but do not prove bounded package scope. |
| TypeScript package `longhorn-tauri` | `check:ts` and `test:ts`; `check:packages` if package assembly changed. | Add `check:ts:longhorn-tauri` and `test:ts:longhorn-tauri`, then define `longhorn-tauri-ts`. | `check:ts` loops all three packages; `test:ts` combines `longhorn` and `longhorn-tauri`. There is no per-package selector. `test:vitest` covers the Svelte package, not this one. |
| Rust behavior that changes generated bindings, such as `longhorn-licence/src/key.rs` | `test:rust`, `check:bindings`, `check:ts`, and `test:ts`. | Once package/domain selectors exist, define a `licence-bindings` group with the Rust behavior test, licence generation/drift, TypeScript check, and consuming test. | The current selectors are workspace/domain aggregates. The generator executes behavior and writes generated JSON; it is not only a type export. `check:bindings` also has unmapped transitive compile dependencies. |
| Agent-control release-absence proof | `check:agent-control-release-absence`; add `check:agent-control-shim` for the committed shim asset. | Keep both compile and marker-scan obligations explicit in a future `agent-control-absence` group after the input split is reviewed. | The map separates compile inputs (`longhorn-core`, `longhorn-config`, `longhorn-tauri-config`, Tauri shim asset) from byte-scan marker sources, but leaves exact marker coverage unresolved. A change whose role is uncertain returns `needs_planner`. |

Do not put names of `proof:artifacts` scripts into a group as if they were
selectors. The map lists fourteen ordered members, but only the aggregate
`proof:artifacts` and explicitly standalone proof selectors can be invoked.
An aggregate's member names are implementation details until Longhorn adds
real selectors.

### Opaque and changing configuration

The map treats `Cargo.lock` conservatively: it affects the Rust lane,
`check:bindings`, `check:api-reference`, both agent absence proofs,
`proof:artifacts`, prototype checks, and release-only proof. That selection is
too broad to call a bounded package group. Ask the planner which affected
scope and milestone policy apply. Do not convert uncertainty into a whole
board instruction.

For `effigy.toml` task or include changes, the map selects the board plus
configuration/tooling checks because the selector definitions themselves are
changing. That is `needs_planner` for worker group selection. For the narrower
`config/release.toml` case, the map names `check:release-gates`,
`test:release-tooling`, the private-candidate proof, and `check:runner-tools`;
the owner still decides whether release-only evidence is in scope. A group
does not change `effigy qa` or release-gate policy.

The bindings generator's transitive compile dependency closure is not
enumerated. The absence proof's compile inputs and byte-scan marker inputs are
not fully split. Until the map and selectors close those gaps, do not claim a
narrow result from `graph affected`, a generated path, or a matching group
name. Record the concrete uncertainty and request planner judgment.

## Reading a run

The proposed run report keeps these measures separate:

- selected group, definition digest, repository head/worktree state, and
  declared targets;
- capacity queue position and admission wait;
- setup/compile/member/cleanup execution time and per-member outcomes;
- expected wall time and `within_budget`, `over_budget`, or `unknown` evidence;
- cold/warm phase timing only when measured.

For example, a group can pass its checks and exceed its expected time. That is
`outcome=passed`, `budget_state=over_budget`; it is neither a timeout nor a
failed check. A cancelled or interrupted run cannot be presented as a pass. If
the owner was killed before writing a final record, status stays incomplete or
unknown.

Success evidence for the Longhorn pilot requires more than a hand-written
input map or a named group. The group must run the exact selected members,
record target scope, queue wait, execution duration, and over-budget evidence,
and injected failures must prove that each member catches its declared
behavior. The report still makes no claim about unmapped inputs.

## Canonical skill adoption

The future QA guidance belongs in the maintained distributed Effigy skill at
`skills/effigy/`. Consumer repository facts stay in that repository's
`AGENTS.md` and knowledge files. This design PR does not edit the skill,
`effigy init`, installed copies, containers, or consumer repositories.

An adoption brief should audit all supported discovery locations before
removing any copy:

- project roots: `.agents/skills`, `.codex/skills`, `.claude/skills`, and
  `.cursor/skills`;
- user roots: `~/.agents/skills`, `~/.codex/skills`, `~/.claude/skills`, and
  `~/.cursor/skills`;
- container or remote-agent roots configured by that environment.

Compare each `effigy` skill to the canonical source and inspect differences.
Move project-specific commands, policies, and examples into `AGENTS.md`,
knowledge docs, or a separate repository skill before deleting a duplicate.
Do not overwrite a divergent global copy or silently choose the first of two
conflicting global matches; fail closed and resolve the conflict. If no global
skill is installed, use repository instructions and current CLI capability,
not a newly vendored copy.

After the runtime ships, the canonical skill must check the actual Effigy
capability before suggesting a group command. `effigy --version` is useful
context, but the authoritative check is a successful group inventory payload
with the expected schema ID (`effigy.qa-groups.v1`). If unavailable, use
existing task selectors and make no QA-group claim. `effigy init` and its
checklist must stop re-vendoring the old project skill copy; update them in a
separate implementation/adoption cut. Containers must install or mount the
same canonical skill version and verify capability against the Effigy binary
inside that container.

The internal `skills/effigy/` source, init parity fixtures, and installer
fixtures are maintenance assets. They are not consumer copies to delete as
part of fleet cleanup. Every consumer migration needs its own approved Queue
brief and must preserve local additions.

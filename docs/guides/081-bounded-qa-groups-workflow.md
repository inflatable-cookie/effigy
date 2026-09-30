# Bounded QA Groups Workflow

Status: active. QA-group commands are implemented through the
`tasks qa-group` surfaces. `stop` and `hard_timeout_ms` stay unavailable:
they are refused with the owned-run supervision prerequisite
([052](../knowledge/contracts/052-owned-run-supervision-contract.md),
proposed and unavailable until implementation) before any side effect.
Installed-skill adoption/distribution is a separate approved cut; until it
reaches your environment, verify capability with
`effigy tasks qa-groups list --json` and fall back to plain selectors.

Contract: [051](../knowledge/contracts/051-bounded-qa-groups-contract.md)
Architecture: [031](../knowledge/architecture/031-bounded-qa-groups-runtime.md)

## Group workflow

Inventory maintained groups and run a selected group through the `tasks`
dispatcher. Repositories (or environments) without groups keep the plain
selector workflow — `effigy tasks`, `effigy <selector> --plan`,
`effigy <selector>` — and `effigy drafts`/`effigy draft <name>` remain for
temporary ordinary tasks and environments, not QA groups. `effigy qa`, CI,
and release gates keep their current owners and meanings: uncertain coverage
means ask the planner, never "run the full board."

```sh
effigy tasks qa-groups list
effigy tasks qa-group run agent-cli --scope cargo-package:effigy-cli --scope path:crates/effigy-cli/src/main.rs --plan
effigy tasks qa-group run agent-cli --scope cargo-package:effigy-cli --scope path:crates/effigy-cli/src/main.rs
effigy tasks qa-group status <run-id>
effigy tasks qa-group logs <run-id>
```

A one-off group requires an explicit file every time:

```sh
effigy tasks qa-groups list --file config/qa-groups/2026-10-02-binding-check.toml
effigy tasks qa-group run binding-check --file config/qa-groups/2026-10-02-binding-check.toml --scope path:crates/longhorn-bindings/src/lib.rs --plan
```

The file path is part of definition identity; a relative path resolves inside
the selected repository. Effigy does not search its directory. A temporary
group expires for inventory and cleanup purposes; an expired definition
remains runnable when selected explicitly. Expected time and expiry answer
different questions. The example's known bindings compile gap makes this plan
return `needs_planner`; do not start a run until that gap has a reviewed
proof mapping or the planner chooses the required evidence. The temporary
definition also names its owning catalog alias explicitly; its file location
never selects a catalog implicitly.

Read the plan for the exact selectors, fixed arguments, targets, companions,
limits, admission class, and definition source. If a task is broad, its
declared target is uncertain, or an input has no mapped selector, stop at
`needs_planner`. Do not call an aggregate a bounded check because its name
looks narrow.

### Scope input and planner boundary

Supply the scope you can name explicitly with repeatable `--scope` tokens:
repository-relative `path:` inputs and relevant typed package, workspace,
external, or opaque `input:` identities. Include relevant unchanged inputs
such as a lockfile or generator/toolchain dependency when they affect the
proof. Do not assume Effigy will extract this list from a Git diff, status, or
graph. If the input cannot be classified, name it as `input:<identity>`; an
unmapped token or a token matching a group's declared `coverage_gaps` returns
`needs_planner` before execution and identifies the token and reason.

`scope_policy = "required"` with no tokens also returns `needs_planner`.
Advisory groups with no tokens report `not_requested` and make no coverage
claim. A `declared_match` means every supplied token matches at least one
member coverage declaration and no known gap; it does not prove the input
list is complete or validate the map. In all cases an accepted group runs
every member in declaration order. Scope matching never filters out checks.
Ask the planner when the declared map cannot resolve the explicit scope or
when you cannot state the scope set confidently. This decision does not direct
you to run the full board.

### Declare ordinary path and package gaps

Use `coverage_gaps` for unresolved ordinary inputs as well as opaque inputs.
For example, a member may claim a path pattern and its exact Cargo package
while both remain unresolved:

```toml
covers = ["path:crates/example/**", "cargo-package:example", "path:crates/mapped/**"]
coverage_gaps = [
  { input = "path:crates/example/**", reason = "The example crate's dependency closure is not mapped" },
  { input = "cargo-package:example", reason = "The example package's dependency closure is not mapped" },
  { input = "input:generator-closure", reason = "The generator's transitive compile inputs are not enumerated" },
]
```

Coverage and gaps are compared independently. A supplied
`path:crates/example/src/lib.rs` matches both the member and the path gap, and
`cargo-package:example` matches both the member and the exact package gap;
each plan returns `needs_planner`. The gap wins even though a member claims
the same token. A supplied `path:crates/mapped/src/lib.rs` matches the member
without a gap and can return `declared_match`. An unmapped input also returns
`needs_planner`.

Choose a broad gap pattern only when every path it can match remains
unresolved. A successful check does not remove a gap: the declaration records
uncertainty in the coverage map, and passing one member run does not establish
that the dependency closure or the caller's scope list is complete. Path gaps
match paths; package gaps match only the exact package token. Keep opaque gaps
for inputs callers can name only by identity: `input:generator-closure` catches
that explicitly supplied token, not an ordinary path or package that may
relate to it. See the [coverage contract](../knowledge/contracts/051-bounded-qa-groups-contract.md)
and [runtime model](../knowledge/architecture/031-bounded-qa-groups-runtime.md)
for the shared grammar and matching rules.

Group names are catalog-scoped. Use `catalog-alias/name` (or the documented
catalog path prefix) when multiple groups could match; an unresolved tie is
an error, not a task/draft fallback. Member selectors resolve in the owning
catalog unless their definition explicitly pins another catalog alias, and
their `published`/`draft` surface is explicit. A QA group may share a name
with a published task or a draft because the group has its own command
surface. Contract [046](../knowledge/contracts/046-published-and-draft-task-surface-contract.md)
still rejects a published task and draft with the same name in one effective
catalog; adding a same-named group does not change that rule. Duplicate group
definitions within one catalog are invalid; a temporary file is selected only
by its explicit `--file` and cannot shadow a maintained group.

## Longhorn pilot examples

The examples below use the read-only Longhorn checkout at `77e0d9873ac278c83729606e0aef9916b36950d0`.
Its `docs/knowledge/validation-input-map.md` is unchanged from reviewed PR61
commit `58baccf8ec630557d51096241c85d9c8c211d46b`. The map is Longhorn's gate
evidence, not an Effigy selector contract or proof of complete coverage.

| Work context | Existing runnable selectors | Candidate bounded group or next selector | Boundary |
| --- | --- | --- | --- |
| Docs-only, general knowledge/index prose | `qa:docs` currently runs all six checks: links, agent-defaults, paths, catalog-links, held-surface, and host-protocol. | Define `longhorn-docs` with exactly four individual members: `qa:docs:links`, `qa:docs:agent-defaults`, `qa:docs:paths`, and `qa:docs:catalog-links`. Do not include `qa:docs`, held-surface, or host-protocol. | The group proves only those four declared checks; held-surface and host-protocol remain outside it. The existing `qa:docs` aggregate is unchanged and still includes all six. Group expected runtime is unknown until these four members are measured; do not reuse the six-check aggregate's timing. |
| Edit `docs/guides/getting-started.md` | `qa:docs` plus `proof:artifacts`. | Add a standalone `proof:guides-card126` selector and include it in `longhorn-getting-started-docs`. | The map's guide-content verifier is one of fourteen `proof:artifacts` members, not a runnable selector today. Current `proof:artifacts` is heavy and broad, so this is not yet a bounded docs group. |
| Rust package `longhorn-agent-tool-dispatch` | `check:agent-tool-dispatch`. Its aggregate runs `lint:agent-tool-dispatch`, `test:agent-tool-dispatch`, `docs:agent-tool-dispatch`, `check:agent-tool-dispatch-release-absence`, and `check:api-reference`. | A `agent-tool-dispatch` group can select the current package aggregate and declare the crate plus dependency closure. | This is the only per-crate Rust aggregate in the map. It does not establish package-filter support for other crates. |
| Another Rust crate, such as `longhorn-credential-keyring` | `fmt:rust`, `lint:rust`, `lint:rust:features`, `test:rust`. | After lead `035b121a` or an equivalent scoped-selector change, add package-filtered test and compile selectors, then group them. | Current selectors span the workspace. They are runnable but do not prove bounded package scope. |
| TypeScript package `longhorn-tauri` | `check:ts` and `test:ts`; `check:packages` if package assembly changed. | Add `check:ts:longhorn-tauri` and `test:ts:longhorn-tauri`, then define `longhorn-tauri-ts`. | `check:ts` loops all three packages; `test:ts` combines `longhorn` and `longhorn-tauri`. There is no per-package selector. `test:vitest` covers the Svelte package, not this one. |
| Rust behavior that changes generated bindings, such as `longhorn-licence/src/key.rs` | `test:rust`, `check:bindings`, `check:ts`, and `test:ts`. | Once package/domain selectors exist, define a `licence-bindings` group with the Rust behavior test, licence generation/drift, TypeScript check, and consuming test. | The current selectors are workspace/domain aggregates. The generator executes behavior and writes generated JSON; it is not only a type export. `check:bindings` also has unmapped transitive compile dependencies. |
| Agent-control release-absence proof | `check:agent-control-release-absence`; add `check:agent-control-shim` for the committed shim asset. | Keep both compile and marker-scan obligations explicit in a future `agent-control-absence` group after the input split is reviewed. | The map separates compile inputs (`longhorn-core`, `longhorn-config`, `longhorn-tauri-config`, Tauri shim asset) from byte-scan marker sources, but leaves exact marker coverage unresolved. A change whose role is uncertain returns `needs_planner`. |

For the bindings group, declare
`input:bindings-generator-transitive-compile-dependencies` as a known coverage
gap until the generator's compile closure is mapped. For the absence proof,
declare `input:agent-control-marker-source-coverage` as a gap until the
compile and marker source sets are independently reviewed. If either input is
in the caller's scope, or a submitted input has no member mapping, the plan
returns `needs_planner` with that input and reason. The planner can resolve
the proof obligation or request a map/selector update; a matching group name
does not clear either gap.

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
scope and milestone policy apply. A caller can name the input as
`--scope input:cargo-lock`, but that makes it visible to coverage matching; it
does not narrow these broad gates or settle the required milestone policy. Do
not convert uncertainty into a whole-board instruction.

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

The run report keeps these measures separate:

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

QA guidance now lives in the maintained distributed Effigy skill source at
`skills/effigy/`. Consumer repository facts stay in that repository's
`AGENTS.md` and knowledge files. Distributing it to installed copies,
auditing discovery locations, and removing consumer duplicates remain the
separately approved adoption cut's work; no consumer repository changes as
part of the runtime implementation.

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

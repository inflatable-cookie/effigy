# g09.006 Cross-Repository Source Routing

Status: Complete
Created: 2026-09-05
Frozen: 2026-09-05 (operator confirmed)
Spec: [`122`](../../specs/122-cross-repository-source-routing-strict-lane.md)
Card: [`1115`](./batch-cards/1115-cross-repository-source-routing.md)
Depends on: [`g09.005`](./005-docs-context-latency-and-freshness.md) (complete),
[`g09.007`](./007-docs-context-exact-identifier-retrieval.md) (serial edge)
Contracts: [`041`](../../contracts/041-documentation-graph-profile-contract.md),
[`037`](../../contracts/037-explicit-catalog-membership-contract.md) (membership posture)
Architecture: [`024`](../../architecture/024-repository-defined-documentation-graph.md)
Origin: Northstar shared-knowledge retrieval pilot,
`northstar/docs/triage/20260905-093742-shared-knowledge-retrieval-pilot.md`;
operator-confirmed direction relayed by the Northstar Chatterbox, 2026-09-05;
membership model revised by the operator on 2026-09-05

## Purpose

Let an agent ask one question across the local repositories that have opted
in, and get back exact sections grouped by repository with per-repository
provenance and identity, using Effigy's existing repository-local retrieval
underneath. Centralise discovery, not copies of decisions.

## Gate

`g09.005` measured warm retrieval at about 600 ms per repository and stale
refresh at about 10 s, which clears the latency gate. The lane starts after
`g09.007` merges so identifier queries work before any recall is measured.

## Frozen Decisions (operator confirmed 2026-09-05)

- **Caller surface:** `effigy docs context <QUERY> --sources <PATH>` with the
  standard leading `--repo` and `--json`, the existing budget flags, and an
  optional repeatable `--only <HANDLE>`. `--sources` names a portfolio file
  or a directory. JSON uses a distinct `effigy.docs.context.sources.v1`
  payload; the single-repository `effigy.docs.context.v1` shape is untouched.
- **Two-sided membership, no crawling.** The portfolio file names where to
  look; each repository declares that it wants to be found:

  ```toml
  # portfolio file (Northstar may commit one; anyone may keep a local one)
  [portfolio]
  directories = ["."]      # relative to this file; immediate children only
  ```

  ```toml
  # each project's effigy.toml (portable, committed)
  [docs_policy.sources]
  share = true
  front_doors = ["docs/README.md", "AGENTS.md"]
  skill_roots = [".agents/skills"]
  ```

  Roots come from the repository's own `[docs_policy.graph].roots`, else
  baseline Markdown. Enumeration is one level deep per named directory. A
  child joins only if it is a git checkout, has `effigy.toml`, and declares
  `share = true`. The handle is the directory name. Hidden directories and
  worktree containers are never considered. Passing a directory to
  `--sources` is equivalent to a file naming that one directory.
- **Execution:** sequential per repository, each with its own graph, lock,
  freshness, and `EFFIGY_GRAPH_TIMEOUT_MS` budget; each repository receives
  the full requested section and byte budget. Parallel execution is deferred
  until a measurement says sequential is too slow.
- **Output:** results grouped per repository in directory order, never
  merged into one ranked list; authority and currentness stay
  repository-declared and are not compared across repositories. Each block
  carries the handle, status, freshness, front doors, and results.
- **Per-repository status vocabulary:** `ok`, `empty`, `stale` (results may
  be behind), `timeout`, `not-shared` (present, no opt-in), `missing`
  (directory or checkout absent), `invalid` (bad manifest or entry),
  `disallowed` (`--only` handle resolves to nothing). The call exits 0 when
  at least one repository is `ok` or `empty`; it fails only when none is.
  Every non-ok repository carries a next step.
- **Source identity** per result, additive: handle, current HEAD, indexed
  HEAD, and whether the file matches HEAD or is working-tree content. A
  working-tree excerpt is never labelled as committed bytes.
- **Replay protocol:** automated proof on two fixture repositories plus a
  fixture portfolio file covering every status and every negative control
  (missing source, retired versus current, same term in two repositories,
  dirty checkout, not-shared, disallowed, no answer), joined to the benchmark.
  The pilot's questions K1–K4, K5a, and K5b are replayed by hand against
  Northstar, Effigy, and Underlay and compared with plain `rg` on time to
  usable evidence, source correctness, and bytes returned; that table lives
  in the evidence log. No speedup or recall claim before it exists.
  K5 is replayed as two evidence-shaped questions settled with the Northstar
  Chatterbox on 2026-09-05: **K5a** (tool behaviour) "What does release execute
  commit, tag, and push?" expecting Effigy guide `051`'s execute section; **K5b**
  (consumer obligation) "What publication step does Underlay require after
  release execute?" expecting Underlay's own release guidance, or a reported
  no-match if absent. Returning the relevant sections satisfies retrieval; no
  synthesised yes/no answer is required, Effigy's binary-publication workflow
  is never treated as Underlay's obligation, K5b stays inside the two-sided
  membership (no bypass of an absent opt-in), and an unavailable or excluded
  source is reported as such, not as a semantic no-match.
- **Latency acceptance:** three shared repositories warm inside 5 s total on
  the reference machine.

## Cards

- [x] [`1115`](./batch-cards/1115-cross-repository-source-routing.md) —
  complete; PR `93` merged

## Non-Goals

- central authoritative memory copies, embeddings, MCP server, hosted
  knowledge service, separate index or daemon, artifact caching
- recursive discovery, globs, or scanning anything not named and opted in
- merged cross-repository ranking or a global authority scale
- agent notification or coordination changes
- release or workflow mutation; writes to any consumer repository
- Northstar documentation authority or lifecycle doctrine (Northstar owns it)
- autonomous promotion of retrieved content into any project's authority

## Dispatch Manifest

Published for the coordinator at the promoting commit on `main`.

- **Lane:** card `1115`, roadmap `g09.006`, strict spec `122`. State:
  complete; PR `93` merged at `931c1b68`.
  **Serial edge:** card `1114` (`g09.007`) has
  merged to `main`. Not approved for parallel execution with `1114`.
- **Prerequisites:** `1114` merged; clean `main`; no other active strict
  lane. The K5 rephrasing (K5a/K5b) is settled in the Frozen Decisions.
  **Completion:** PR merged with evidence log, card, roadmap, spec, guides,
  benchmark freeze, starter/init profile addition, and changelog closed out.
- **Owned mutable paths:** `crates/effigy-codegraph/src/docs_context/**`,
  `crates/effigy-manifest/src/**` (`[docs_policy.sources]` grammar),
  `crates/effigy-cli/src/**` (flag parsing and help for `docs context`),
  `src/runner/docs_command/**`, `scripts/benchmark-docs-context.rhai`,
  `tests/fixtures/docs-context-benchmark/**` (two fixture repositories and a
  portfolio file), tests under those crates and `src/tests/**`,
  `docs/guides/079-documentation-graph-profiles-and-context.md`,
  `docs/guides/017-json-output-contracts.md`,
  `docs/guides/026-json-payload-examples.md`, the Northstar init/starter
  profile assets that emit `[docs_policy.graph]` (add the `sources` block),
  and this repository's own `effigy.toml` (opt in).
  **Reserved shared closeout surfaces:** `CHANGELOG.md` `[Unreleased]`,
  `docs/logs/2026-09/`, `docs/logs/README.md`, this roadmap, card `1115`,
  spec `122`, contract `041` (command contract and drift trigger for the new
  flag and grammar), `docs/specs/README.md`, `docs/roadmaps/README.md`,
  `docs/roadmaps/g09/README.md`.
- **Concurrency:** no approved siblings. Serial after `1114`.
- **Worker capability class:** frontier-capable implementation worker; the
  lane spans manifest grammar, a new payload, fixture design, and a measured
  replay.
- **Acceptance evidence and review oracle:** card `1115` acceptance and spec
  `122` whole-lane oracle; fixture benchmark with the new freeze, focused
  tests, the three-repository warm timing, `effigy qa`, fmt, clippy,
  `git diff --check`; one dated evidence log with the replay table.
- **Stop conditions and escalation owner:** spec `122` stop conditions.
  Planning questions escalate to the coordinator, then Chatterbox. Any
  change to contract `041` semantics beyond the frozen grammar and flag
  escalates to Chatterbox before code.

## Next Task

Card `1115` is complete. Await the next Chatterbox-promoted direction.

# Vision

Effigy gives a monorepo one predictable command surface. A manifest owns task
selection and execution; built-ins cover common operational jobs without
hiding repository-specific tasks. The same selector and context should resolve
the same way, and ambiguity should fail with usable candidate evidence.

Text, JSON, and managed task interfaces should express the same decision.
Versioned JSON contracts support automation. Discovery and diagnosis need to
be fast and bounded because agents and people both depend on them.

Keep the runner reusable. Provider-specific deployment behavior and consumer
application policy belong outside the core. New capability needs explicit
ownership, compatibility, and release proof. Product work should reduce or at
least contain the cost of parsing, routing, runtime, and command-surface
complexity.

## Decision Principles

1. Reliability takes precedence over speed for default behavior. Narrow scope
   when a deadline would otherwise change a contract without proof.
2. Machine-readable compatibility takes precedence over convenience. A
   breaking JSON change needs a versioned contract and migration decision.
3. Routing and deferral stay explicit. Ambiguity fails with candidates and a
   useful next action; it does not silently choose a fallback.
4. Release-critical changes remain reversible and gated. A failed gate is
   evidence to fix the candidate, not a reason to bypass it.
5. Operator-facing failures should explain the next action. More text is not
   the same as better diagnosis.
6. A structural refactor preserves public behavior unless a separate product
   decision changes it. Pure plans and focused parity tests are its proof.

## Product Direction

- **Deterministic spine:** catalog membership, task routing, tests, JSON
  envelopes, and release gates remain inspectable.
- **Agent operations:** graph, docs context, scan, and doctor should
  reduce reconstruction work without making generated summaries authoritative.
- **Portfolio proof:** bootstrap, bundles, dependency links, and consumer
  guidance need evidence from real sibling repositories, not fixtures alone.
- **Sustainable internals:** semantic crate boundaries and clear command
  groups matter more than accumulating convenience aliases.

Remote execution, plugins, telemetry, and new distribution channels need
specific contracts and consumer demand before they become product promises.
A required MCP server, a separate graph daemon, or a marketplace is not a
precondition for the current product. A local development service catalog is
not a production deployment topology.

## Decision Records Carried into Current Contracts

- Root-owned [explicit catalog membership](contracts/037-explicit-catalog-membership-contract.md)
  replaced ambient descendant discovery. Undeclared nested catalogs do not
  join task routing automatically.
- Built-in [test orchestration](contracts/038-unified-test-orchestration-contract.md)
  uses `[test]`; `tasks.test` is removed and `test --plan` does not execute.
- The [documentation graph](architecture/024-repository-defined-documentation-graph.md)
  takes its semantic profile from the selected repository, not an installed
  skill, a hard-coded Northstar layout, or a second index.

Those rules outlive their former vision decision records. Planning selections
about past governance and consumer cohorts remain in Git history; current
intent lives in [plan.md](../plan.md).

## Quality Knowledge

- [Quality targets](vision/quality-targets.md) define the signals and target
  envelopes; they are not claims of measured SLO attainment.
- [Risk model](vision/risks.md) names threats to routing, contracts,
  operability, maintainability, and release safety.
- [Exception policy](vision/exceptions.md) defines how a temporary deviation
  is approved, expires, and is retired without hiding a contract break.

The [architecture index](architecture/README.md) and
[contract index](contracts/README.md) own technical detail.

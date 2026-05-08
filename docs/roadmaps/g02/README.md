# Roadmap g02

`g02` is the previous Effigy roadmap generation.

Generation theme:

- start the next product-shaping cycle from a clean sequence instead of
  extending `g01` indefinitely
- use `g02` for the release and local-runtime expansion wave that followed the
  original implementation and consolidation generation

Current milestones:

- [`001-bootstrap-command-and-clone-contract.md`](./001-bootstrap-command-and-clone-contract.md) (complete; built-in released and live-pilot validated on `loophole` and `songsprout`)
- [`002-manifest-composition-and-override-contract.md`](./002-manifest-composition-and-override-contract.md) (complete; the general split-manifest contract, override model, and inspectability surface are now shipped strongly enough to close the lane)
- [`003-demo-harness-model-and-runner-contract.md`](./003-demo-harness-model-and-runner-contract.md) (complete; shipped and released in `v0.2.13`, including the demo registry, browser, live terminal, query/history surfaces, concurrent-runner projection, and real-project consumer proof)
- [`004-rust-native-scripting-surface-contract.md`](./004-rust-native-scripting-surface-contract.md) (complete; the scripting policy split, Rhai v1 boundary, script-step foundation, long-running lifecycle support, release-wrapper convergence, and native distribution cutover are all shipped on the intended boundary)
- [`005-optional-distribution-surface-contract.md`](./005-optional-distribution-surface-contract.md) (complete; the optional manifest-driven distribution surface is consumer-proven for metadata validation, artifact validation, and closeout evidence reuse, while the fuller published-consumer `first-publish` question is explicitly deferred)
- [`006-colima-container-environment-contract.md`](./006-colima-container-environment-contract.md) (complete; the v1 container foundation, attached-session widening, repo-owned task composition, and real-machine live-stop hardening are all shipped on the intended boundary)
- [`007-distribution-release-and-consumer-rollout.md`](./007-distribution-release-and-consumer-rollout.md) (complete; the `v0.3.x` release work shipped, and any broader rollout follow-up must now be re-sequenced into the live generation)
- [`008-demo-and-manifest-import-rollout.md`](./008-demo-and-manifest-import-rollout.md) (complete; the rollout itself was intentionally not run in `g02`, so any future adoption push must be rehomed into the live generation or backlog)
- [`009-vault-backed-varlock-rollout.md`](./009-vault-backed-varlock-rollout.md) (complete; the rollout itself was intentionally not run in `g02`, so any future vault-backed adoption push must be rehomed into the live generation or backlog)
- [`010-effigy-modularization-and-crate-boundaries.md`](./010-effigy-modularization-and-crate-boundaries.md) (complete; the backbone plus domain crates are real, the recent `/src` cleanup chain is fully landed, and the lane is closed on a trustworthy boundary)
- [`011-service-catalog-and-compose-assembly.md`](./011-service-catalog-and-compose-assembly.md) (complete; the crate foundation, runner integration, operator-facing catalog/eject surface, and real-project proof are now all landed)
- [`012-container-context-and-transparent-execution.md`](./012-container-context-and-transparent-execution.md) (complete; transparent task routing, explicit exec, alias fallback, CWD mapping, handoff strategy, and one real consumer proof are now all landed)
- [`013-dev-front-door-and-managed-lifecycle.md`](./013-dev-front-door-and-managed-lifecycle.md) (complete; lifecycle ownership, shell embedding, readiness UX, gateway auto-start, and the final real-project proof are now all landed)
- [`014-rust-native-gateway.md`](./014-rust-native-gateway.md) (complete; bounded gateway integration is shipped through command ownership, route lifecycle, and real plain HTTP/HTTPS consumer proofs, with broader dashboard residue deferred to `g02.016`)
- [`015-persistent-data-and-volume-lifecycle.md`](./015-persistent-data-and-volume-lifecycle.md) (complete; reset retention, inventory, transfer, media and pull hooks, real-project proof, and proof-exposed volume/caching fixes are all shipped)
- [`016-multi-project-coordination.md`](./016-multi-project-coordination.md) (complete; cross-project status, route dashboard, generated-compose auto-allocation, resource stats, and bounded shared services are all shipped)
- [`017-remaining-shell-cleanup-and-crate-extraction-program.md`](./017-remaining-shell-cleanup-and-crate-extraction-program.md) (closed; the queued shell-cleanup and extraction jobs that were worth doing are now landed, and the remaining root-crate untidiness is acceptable rather than roadmap-worthy)
- `g02.018` is retired and must not be reused; older logs may still mention it,
  but the number is intentionally left vacant for traceability
- [`019-v0-3-surface-audit-and-ux-simplification.md`](./019-v0-3-surface-audit-and-ux-simplification.md) (complete; the shipped `v0.3` surface, docs front doors, and local-dev UX were tightened on the intended boundary)
- [`020-multi-project-gateway-expansion-and-service-dns.md`](./020-multi-project-gateway-expansion-and-service-dns.md) (complete; the post-`v0.3` networking follow-up landed with per-route DNS targets, project/shared loopback-IP service routing, and HTTP post-start port discovery)
- [`021-unified-init-and-starter-emission.md`](./021-unified-init-and-starter-emission.md) (complete; `effigy init [<name>]` is the single scaffolding surface, `minimal` + `underlay` ship as embedded starters with multi-file emission and post-emission guidance, and the `effigy.init.v1` / `effigy.init.list.v1` contracts are live — resolves the `g01.029` Wave 5 `effigy init --northstar`-shaped candidate as `effigy init <name>`)
- [`022-v0-3-pre-release-hardening-and-contract-cleanup.md`](./022-v0-3-pre-release-hardening-and-contract-cleanup.md) (complete; the bounded pre-cut hardening and first-contract cleanup pass landed across gateway privilege flow, resolver validation, env execution reliability, discovery hygiene, and the last cheap `v0.3` contract tidy-up worth taking before release)

Container infrastructure design document:

- [`../architecture/020-container-infrastructure-design.md`](../architecture/020-container-infrastructure-design.md)

Closeout note:

- `g02` carried the repo through `v0.3.0` and `v0.3.1`
- `g04` now owns the next live roadmap queue
- no `g02` strict lane remains active

Historical strict planning lanes:

- the `g02.020` strict lane was the last active `g02` lane
- the `g02.013` dev front door strict lane is complete through card `300`
- the `g02.015` persistent data strict lane is now complete through card `290`
- the `g02.014` gateway strict lane is now complete through card `270`
- the `g02.016` multi-project coordination strict lane is now complete through
  card `278`

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items now start in `g04`
- `g02` numbering is closed
- treat rollover as full closeout, not a convenience reset: `g02` does not end
  until every `g02` roadmap is closed, paused, superseded, or rehomed and the
  stale `g02` strict-lane artifacts have been purged from the active
  `docs/specs/` tree
- as a healthy default, expect a generation to carry roughly 20 to 40 roadmap
  files before rollover is even worth discussing

## Next Task

`g02` is closed as the live roadmap queue.

Move to [`../g04/README.md`](../g04/README.md) and continue the live `g04`
queue instead of reopening `g02`.


Batch cards live in `g02/batch-cards/` when strict posture uses them.

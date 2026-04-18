# Roadmap g02

`g02` is the current Effigy roadmap generation.

Generation theme:

- start the next product-shaping cycle from a clean sequence instead of
  extending `g01` indefinitely
- use `g02` for new command surfaces and architectural direction that are
  meaningfully beyond the original implementation and consolidation waves

Current milestones:

- [`001-bootstrap-command-and-clone-contract.md`](./001-bootstrap-command-and-clone-contract.md) (complete; built-in released and live-pilot validated on `loophole` and `songsprout`)
- [`002-manifest-composition-and-override-contract.md`](./002-manifest-composition-and-override-contract.md) (in progress; composition foundation and inspectability are now real product surface and no longer block downstream planning)
- [`003-demo-harness-model-and-runner-contract.md`](./003-demo-harness-model-and-runner-contract.md) (complete; shipped and released in `v0.2.13`, including the demo registry, browser, live terminal, query/history surfaces, concurrent-runner projection, and real-project consumer proof)
- [`004-rust-native-scripting-surface-contract.md`](./004-rust-native-scripting-surface-contract.md) (complete; the scripting policy split, Rhai v1 boundary, script-step foundation, long-running lifecycle support, release-wrapper convergence, and native distribution cutover are all shipped on the intended boundary)
- [`005-optional-distribution-surface-contract.md`](./005-optional-distribution-surface-contract.md) (complete; the optional manifest-driven distribution surface is consumer-proven for metadata validation, artifact validation, and closeout evidence reuse, while the fuller published-consumer `first-publish` question is explicitly deferred)
- [`006-colima-container-environment-contract.md`](./006-colima-container-environment-contract.md) (complete; the v1 container foundation, attached-session widening, repo-owned task composition, and real-machine live-stop hardening are all shipped on the intended boundary)
- [`007-distribution-release-and-consumer-rollout.md`](./007-distribution-release-and-consumer-rollout.md) (paused; release closure is complete, but the repo will now finish the remaining `g02` feature/integration spine before one explicit `v0.3` release cut)
- [`008-demo-and-manifest-import-rollout.md`](./008-demo-and-manifest-import-rollout.md) (planned; complete manifest-import adoption and demo rollout across the intended repo cohort)
- [`009-vault-backed-varlock-rollout.md`](./009-vault-backed-varlock-rollout.md) (planned; turn the shipped env-schema/varlock foundation into a vault-backed consumer rollout program)
- [`010-effigy-modularization-and-crate-boundaries.md`](./010-effigy-modularization-and-crate-boundaries.md) (complete; the backbone plus domain crates are real, the recent `/src` cleanup chain is fully landed, and the lane is closed on a trustworthy boundary)
- [`011-service-catalog-and-compose-assembly.md`](./011-service-catalog-and-compose-assembly.md) (complete; the crate foundation, runner integration, operator-facing catalog/eject surface, and real-project proof are now all landed)
- [`012-container-context-and-transparent-execution.md`](./012-container-context-and-transparent-execution.md) (complete; transparent task routing, explicit exec, alias fallback, CWD mapping, handoff strategy, and one real consumer proof are now all landed)
- [`013-dev-front-door-and-managed-lifecycle.md`](./013-dev-front-door-and-managed-lifecycle.md) (planned; single-command `effigy dev` front door using the managed-process concurrent runtime with embedded terminal and health gate)
- [`014-rust-native-gateway.md`](./014-rust-native-gateway.md) (complete; bounded gateway integration is shipped through command ownership, route lifecycle, and real plain HTTP/HTTPS consumer proofs, with broader dashboard residue deferred to `g02.016`)
- [`015-persistent-data-and-volume-lifecycle.md`](./015-persistent-data-and-volume-lifecycle.md) (in progress; volume management shipped in `effigy-catalog::volumes`, seeding and Rhai hooks deferred to integration phase)
- [`016-multi-project-coordination.md`](./016-multi-project-coordination.md) (in progress; port allocation registry shipped in `effigy-gateway::ports`, cross-project status deferred to integration phase)
- [`017-remaining-shell-cleanup-and-crate-extraction-program.md`](./017-remaining-shell-cleanup-and-crate-extraction-program.md) (closed; the queued shell-cleanup and extraction jobs that were worth doing are now landed, and the remaining root-crate untidiness is acceptable rather than roadmap-worthy)
- [`018-research-promotion-and-carry-forward.md`](./018-research-promotion-and-carry-forward.md) (planned; carries the unfinished promotion and future-facing residue from the closed `g01` research phases)

Container infrastructure design document:

- [`../architecture/020-container-infrastructure-design.md`](../architecture/020-container-infrastructure-design.md)

Active strict planning lanes:

- `g02.007` is paused until the remaining `g02` feature/integration spine is complete
- the `g02.014` gateway strict lane is now complete through card `270`
- the `g02.016` multi-project coordination strict lane is now active with
  `272` landed as the first execution batch

Rules:

- `g01` remains the historical implementation and consolidation generation
- new roadmap items that represent a fresh product cycle should start in `g02`
- continue numbering in `g02/` until another manual rollover is justified
- treat rollover as full closeout, not a convenience reset: `g02` does not end
  until every `g02` roadmap is closed, paused, superseded, or rehomed and the
  stale `g02` strict-lane artifacts have been purged from the active
  `docs/specs/` tree
- as a healthy default, expect a generation to carry roughly 20 to 40 roadmap
  files before rollover is even worth discussing

## Next Task

`g02.016` now has its first landed status/dashboard batch. Stop in planning
and choose the next bounded follow-up.

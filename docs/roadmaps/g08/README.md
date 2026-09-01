# g08 Roadmaps

Status: Active
Theme: Graph-aware scan intelligence and code quality boundary follow-through,
extended with the 2026-06-10 security and posture hardening tranche
(g08.010–g08.015, complete), the machine-local dependency management suite
(g08.018–g08.023), and bounded release-orchestration follow-through in
`g08.024` through `g08.026`, papercut portfolio discovery in `g08.027`,
explicit catalog membership in `g08.028`, unified v0.11 test orchestration in
`g08.029`, exact-candidate pre-release CI proof in `g08.030`, committed Bun
dependency pinning and its bounded enumeration fallback in `g08.031`, and vision
governance operationalization in `g08.032`, doctor secret-schema parity in
`g08.033`, documentation coverage parity in `g08.034`, the completed
documentation, instruction, and help parity refresh in `g08.036`, the completed
repository-defined documentation graph in `g08.035`, the completed external
skill-task runner in `g08.037`, and the completed help-first command discovery
in `g08.038`.

## Purpose

`g08` connects Effigy's scan surface to the code graph without making the
existing scans slower, fuzzy, or index-dependent. The follow-up tranche uses
that scan evidence plus a manual code-quality sweep to reduce drift-prone
declarations and mixed ownership boundaries.

The goal is not to replace deterministic filesystem scans. The goal is to add
relation-aware findings where graph data is the missing signal, and to enrich
current scan output when a ready index already exists.

The completed dependency tranche added a package-manager-aware `effigy deps`
domain for reversible machine-local Cargo and Bun links while committed
dependency manifests remain authoritative. Later completed tranches replaced
ambient catalog discovery with explicit root-owned membership and unified the
v0.11 test authority under `[test]`.

The completed dependency follow-up adds an explicit committed Bun override
mode for cross-repository graphs. It remains separate from machine-local links
and never mutates intermediate repositories. Card `1081` handles consumers
where Bun cannot enumerate its own text lockfile.

The completed help-first lane groups the public command inventory by operator
job without changing executable routes or reserving new top-level names.

This generation should help agents and maintainers answer questions like:

- which oversized files are also high-blast-radius owners?
- which TODOs sit on central or boundary-crossing code?
- which symbols or files look unused, isolated, or under-tested?
- which imports/calls violate declared architecture boundaries?
- which changed areas need validation attention before merge?

## Roadmap Sequence

- [`001-graph-aware-scan-intelligence-suite.md`](./001-graph-aware-scan-intelligence-suite.md)
- [`002-scan-graph-contract-and-readiness-model.md`](./002-scan-graph-contract-and-readiness-model.md)
- [`003-existing-scan-graph-enrichment.md`](./003-existing-scan-graph-enrichment.md)
- [`004-boundary-and-layer-violation-scans.md`](./004-boundary-and-layer-violation-scans.md)
- [`005-dead-and-isolated-code-scans.md`](./005-dead-and-isolated-code-scans.md)
- [`006-validation-gap-and-hotspot-scans.md`](./006-validation-gap-and-hotspot-scans.md)
- [`007-agent-docs-json-and-benchmark-proof.md`](./007-agent-docs-json-and-benchmark-proof.md)
- [`008-graph-aware-scan-closeout.md`](./008-graph-aware-scan-closeout.md)
- [`009-code-quality-boundary-sweep-suite.md`](./009-code-quality-boundary-sweep-suite.md)
- [`010-security-and-posture-hardening-suite.md`](./010-security-and-posture-hardening-suite.md)
- [`011-discovery-and-doctor-correctness.md`](./011-discovery-and-doctor-correctness.md)
- [`012-supply-chain-and-ci-security-gates.md`](./012-supply-chain-and-ci-security-gates.md)
- [`013-daemon-panic-safety-and-secret-egress-hardening.md`](./013-daemon-panic-safety-and-secret-egress-hardening.md)
- [`014-gateway-route-table-trust-model.md`](./014-gateway-route-table-trust-model.md)
- [`015-docs-spine-compaction.md`](./015-docs-spine-compaction.md)
- [`016-suppression-hygiene-and-dead-code-precision.md`](./016-suppression-hygiene-and-dead-code-precision.md)
- [`017-workspace-ssh-agent-mount-resilience.md`](./017-workspace-ssh-agent-mount-resilience.md)
- [`018-local-dependency-management-suite.md`](./018-local-dependency-management-suite.md)
- [`019-dependency-inventory-and-command-foundation.md`](./019-dependency-inventory-and-command-foundation.md)
- [`020-cargo-local-dependency-linking.md`](./020-cargo-local-dependency-linking.md)
- [`021-bun-local-dependency-linking.md`](./021-bun-local-dependency-linking.md)
- [`022-dependency-link-doctor-and-hygiene.md`](./022-dependency-link-doctor-and-hygiene.md)
- [`023-dependency-link-portfolio-proof-and-closeout.md`](./023-dependency-link-portfolio-proof-and-closeout.md)
- [`024-initial-current-version-release-tag.md`](./024-initial-current-version-release-tag.md)
- [`025-annotated-release-tag-integrity.md`](./025-annotated-release-tag-integrity.md)
- [`026-patch-release-lane-hardening.md`](./026-patch-release-lane-hardening.md)
- [`027-papercuts-discovery-and-capture.md`](./027-papercuts-discovery-and-capture.md)
- [`028-explicit-catalog-membership.md`](./028-explicit-catalog-membership.md)
- [`029-unified-test-orchestration-v011.md`](./029-unified-test-orchestration-v011.md)
- [`030-pre-release-ci-proof.md`](./030-pre-release-ci-proof.md)
- [`031-bun-committed-dependency-pinning.md`](./031-bun-committed-dependency-pinning.md)
- [`032-vision-governance-operationalization.md`](./032-vision-governance-operationalization.md)
- [`033-doctor-secrets-schema-parity.md`](./033-doctor-secrets-schema-parity.md)
- [`034-documentation-coverage-parity.md`](./034-documentation-coverage-parity.md)
- [`035-repository-defined-documentation-graph.md`](./035-repository-defined-documentation-graph.md)
- [`036-documentation-instruction-and-help-parity-refresh.md`](./036-documentation-instruction-and-help-parity-refresh.md)
- [`037-external-skill-task-runner.md`](./037-external-skill-task-runner.md)
- [`038-help-first-command-discovery.md`](./038-help-first-command-discovery.md)
- [`039-rhai-profile-independent-limits-papercut.md`](./039-rhai-profile-independent-limits-papercut.md)
- [`040-catalog-pack-acquisition-prototype.md`](./040-catalog-pack-acquisition-prototype.md)
- [`041-catalog-fragment-listing-papercut.md`](./041-catalog-fragment-listing-papercut.md)
- [`042-markdown-frontmatter-extraction-papercut.md`](./042-markdown-frontmatter-extraction-papercut.md)
- [`043-docs-context-no-match-benchmark-isolation-papercut.md`](./043-docs-context-no-match-benchmark-isolation-papercut.md)
- [`044-rhai-storage-create-only.md`](./044-rhai-storage-create-only.md)
- [`045-child-catalog-suite-registry-papercut.md`](./045-child-catalog-suite-registry-papercut.md)
- [`046-docs-context-time-budget-papercut.md`](./046-docs-context-time-budget-papercut.md)
- [`047-docs-context-traversal-budget-papercut.md`](./047-docs-context-traversal-budget-papercut.md)

## Design Posture

- keep existing scan commands deterministic and useful without a graph index
- make graph-backed behavior explicit in JSON and human output
- use graph data for relationships, not vague scoring
- preserve source paths, ranges, and reasons for every finding
- prefer repo-agnostic rules, fixtures, and configuration over Effigy-specific
  assumptions
- keep exact-token proof and final code inspection outside graph claims
- keep committed dependency manifests authoritative while local links remain
  machine-local, inspectable, and reversible
- keep committed Bun pins explicit, root-consumer-owned, and separate from
  machine-local link ownership
- select local-link mechanisms by package manager, not source language

## Non-Goals

- no hidden auto-indexing from `scan`
- no LLM-generated scan findings
- no MCP server or graph daemon
- no Effigy-only architecture rule hard-coding
- no breaking rewrite of existing `scan` JSON contracts
- no claim that graph-backed scans prove semantic dead code with compiler
  precision
- no public command behavior changes while reducing declaration drift
- no speculative macro framework or generated command surface
- no broad code deletion from advisory scan findings
- no dependency manifest edits or Bun `--save` behavior from local linking
- no package managers beyond Cargo and Bun in the first dependency tranche
- no ambient catalog fallback, recursive member expansion, or migration scan
  in the explicit-membership tranche
- no Northstar-only documentation ontology, generated graph summaries, second
  graph store, or required daemon/MCP surface in `g08.035`

## Generation Runway

- Runway goal: make cross-repository Bun local development actionable without
  hidden mutation or mixed local/registry package identity.
- Completed pin foundation: card `1078` owns full-closure planning and the safe
  root-manifest transaction under contract `040`.
- Completed command surface: card `1079` owns CLI, JSON, and link/pin
  interlocks.
- Completed proof and closeout: card `1080` owns disposable Soundcheck/Poodle
  proof, public guidance, full validation, and lane archival.
- Completed pin resilience follow-up: card `1081` owns a warning-bearing,
  pin-only text-lockfile fallback and six-consumer proof.
- Foundation complete: `g08.019` established shared inventory, desired state,
  status, and JSON foundations through cards `1051` to `1053`.
- Cargo planning complete: `1054` established the pure full-closure and safety
  plan.
- Cargo milestone complete: cards `1054` through `1056` plan, apply, verify,
  and safely unlink full Cargo closures.
- Bun planning complete: `1057` established full-closure, immutable-file,
  explicit `--no-save`, and registration-ownership plans without mutation.
- Bun link apply complete: `1058` added exact-precondition application,
  rollback, immutable-file guards, full symlink verification, CLI/JSON, and
  real Bun `1.3.14` proof.
- Bun milestone complete: `1059` added exact unlink, ownership-safe
  registration release, peer diagnostics, rollback, and real round-trip proof.
- Status health complete: `1060` added manager-neutral Cargo/Bun findings,
  exact evidence, remediation, peer diagnostics, and text/JSON parity.
- Doctor/hygiene complete: `1061` adapted the shared report into doctor
  information/warning/error findings with text/JSON parity and closed
  `g08.022`.
- Cargo portfolio proof complete: `1062` proved flat Soundcheck and nested
  Loophole closure, edit propagation, lock-neutral observation, and exact
  unlink recovery in disposable clones.
- Bun proof complete: `1063` proved save-less full closure, real install drift,
  managed repair, exact peer paths, and owned-registration cleanup.
- Dependency suite complete: `1064` published operator/agent guidance, kept
  command/JSON references current, passed full QA, and closed `g08.018`,
  `g08.023`, and strict lane `099`.
- Initial-tag follow-through complete: `g08.024` added an explicit,
  first-release-only path for tagging the version already declared by a new
  repository without weakening normal monotonic release planning.
- Annotated-tag integrity complete: `g08.025` makes the irreversible release path
  preserve the approved Git tag object type and deterministic message.
- Patch-release hardening complete: `g08.026` removes persistent loopback test
  leakage, settles prepared-source drift policy, and proves the `0.9.1`
  candidate without release mutation.
- Explicit membership complete: `g08.028` delivered under contract `037` and
  strict spec `101`; cards `1072` through `1075` are complete.
- Visible cards: `1072` typed schema, `1073` routing cutover, `1074` deletion
  and diagnostics, `1075` migration proof and closeout.
- Deletion checkpoint: no undeclared consumer, ambient walker, or cache surface
  remains. No generation rollover is implied.
- Documentation/help parity complete: archived strict spec `109` and card
  `1091` cover the scan evidence, Northstar AGENTS review, active docs,
  generated reference, and shipped CLI help.
- Documentation graph complete: archived strict spec `108` and contract `041`
  govern cards `1088` through `1090`; all three are complete.
- External skill runner complete: archived strict spec `110`, architecture
  `025`, and contract `042` govern completed card `1092`.
- Help-first command discovery complete: archived strict spec `111`,
  architecture `026`, and contract `043` govern completed card `1093`.
- Rhai profile-independent limits complete: archived strict spec `112` and
  completed card `1094` govern the expression-depth papercut repair.
- Catalog-pack acquisition prototype complete: archived strict spec `113`,
  architecture `026`, and contract `043` govern completed card `1095`.

## Execution Rule

The dependency suite completed under strict spec `099`. Cards `1065` and
`1066` completed the bounded release-tag integrity lane. Cards `1067` through
`1069` completed the `g08.026` patch-release hardening lane. Strict spec `100`
completed `g08.027` through cards `1070` and `1071`.
Strict spec `101` and `g08.028` are complete through cards `1072` to `1075`.
Strict spec `102` and `g08.029` are complete through card `1076`.
Strict spec `103` and `g08.030` are complete through card `1077`.
Strict spec `104` and `g08.031` are complete through card `1081`.
Strict spec `105` and `g08.032` are complete through card `1084`.
Strict spec `107` and `g08.034` are complete through cards `1086` and `1087`.
Strict spec `110` and `g08.037` are complete through card `1092`. Strict spec
`108` and `g08.035` are complete through cards `1088` to `1090`. Strict spec
`109` and `g08.036` are complete. Strict spec `111` and `g08.038` are complete
through card `1093`. Strict spec `112` and `g08.039` are complete through card
`1094`. Strict spec `113` and `g08.040` are complete through card `1095`.

## Batch Cards

- [`1029-open-graph-aware-scan-lane.md`](./batch-cards/1029-open-graph-aware-scan-lane.md)
- [`1030-define-scan-graph-readiness-contract.md`](./batch-cards/1030-define-scan-graph-readiness-contract.md)
- [`1031-enrich-existing-scans-with-graph-context.md`](./batch-cards/1031-enrich-existing-scans-with-graph-context.md)
- [`1032-add-boundary-and-layer-violation-scans.md`](./batch-cards/1032-add-boundary-and-layer-violation-scans.md)
- [`1033-add-dead-and-isolated-code-scans.md`](./batch-cards/1033-add-dead-and-isolated-code-scans.md)
- [`1034-add-validation-gap-and-hotspot-scans.md`](./batch-cards/1034-add-validation-gap-and-hotspot-scans.md)
- [`1035-update-agent-docs-json-and-benchmark-proof.md`](./batch-cards/1035-update-agent-docs-json-and-benchmark-proof.md)
- [`1036-close-graph-aware-scan-lane.md`](./batch-cards/1036-close-graph-aware-scan-lane.md)
- [`1037-open-code-quality-boundary-sweep-lane.md`](./batch-cards/1037-open-code-quality-boundary-sweep-lane.md)
- [`1038-define-command-surface-descriptor-seam.md`](./batch-cards/1038-define-command-surface-descriptor-seam.md)
- [`1039-define-rhai-feature-descriptor-seam.md`](./batch-cards/1039-define-rhai-feature-descriptor-seam.md)
- [`1040-split-container-up-phase-helpers.md`](./batch-cards/1040-split-container-up-phase-helpers.md)
- [`1041-converge-repo-marker-rules.md`](./batch-cards/1041-converge-repo-marker-rules.md)
- [`1042-reduce-selected-duplicate-blocks.md`](./batch-cards/1042-reduce-selected-duplicate-blocks.md)
- [`1043-tune-boundary-and-dead-code-scans-for-effigy.md`](./batch-cards/1043-tune-boundary-and-dead-code-scans-for-effigy.md)
- [`1044-fix-dead-code-scan-rust-signal.md`](./batch-cards/1044-fix-dead-code-scan-rust-signal.md)
- [`1045-classify-and-reduce-dead-code-residuals.md`](./batch-cards/1045-classify-and-reduce-dead-code-residuals.md)
- [`1046-classify-trait-and-api-surface-dead-code.md`](./batch-cards/1046-classify-trait-and-api-surface-dead-code.md)
- [`1047-classify-descriptor-and-dispatch-dead-code-roots.md`](./batch-cards/1047-classify-descriptor-and-dispatch-dead-code-roots.md)
- [`1048-classify-dto-render-config-dead-code-roots.md`](./batch-cards/1048-classify-dto-render-config-dead-code-roots.md)
- [`1049-classify-rust-impl-and-associated-call-dead-code.md`](./batch-cards/1049-classify-rust-impl-and-associated-call-dead-code.md)
- [`1050-complete-dead-code-false-positive-burn-down.md`](./batch-cards/1050-complete-dead-code-false-positive-burn-down.md)
- [`1051-establish-dependency-domain-and-state-foundation.md`](./batch-cards/1051-establish-dependency-domain-and-state-foundation.md)
- [`1052-add-read-only-dependency-inventory-and-status.md`](./batch-cards/1052-add-read-only-dependency-inventory-and-status.md)
- [`1053-wire-deps-cli-json-and-foundation-closeout.md`](./batch-cards/1053-wire-deps-cli-json-and-foundation-closeout.md)
- [`1054-plan-cargo-full-closure-and-managed-config.md`](./batch-cards/1054-plan-cargo-full-closure-and-managed-config.md)
- [`1055-apply-and-verify-cargo-links.md`](./batch-cards/1055-apply-and-verify-cargo-links.md)
- [`1056-apply-cargo-unlink-and-closeout.md`](./batch-cards/1056-apply-cargo-unlink-and-closeout.md)
- [`1057-plan-bun-full-closure-and-registration-ownership.md`](./batch-cards/1057-plan-bun-full-closure-and-registration-ownership.md)
- [`1058-apply-and-verify-bun-links.md`](./batch-cards/1058-apply-and-verify-bun-links.md)
- [`1059-apply-bun-unlink-peer-diagnostics-and-closeout.md`](./batch-cards/1059-apply-bun-unlink-peer-diagnostics-and-closeout.md)
- [`1060-observe-dependency-hygiene-and-status-parity.md`](./batch-cards/1060-observe-dependency-hygiene-and-status-parity.md)
- [`1061-integrate-dependency-health-with-doctor-and-closeout.md`](./batch-cards/1061-integrate-dependency-health-with-doctor-and-closeout.md)
- [`1062-prove-signal-links-across-flat-and-nested-consumers.md`](./batch-cards/1062-prove-signal-links-across-flat-and-nested-consumers.md)
- [`1063-prove-bun-closure-drift-and-repair.md`](./batch-cards/1063-prove-bun-closure-drift-and-repair.md)
- [`1064-publish-dependency-link-guidance-and-close-suite.md`](./batch-cards/1064-publish-dependency-link-guidance-and-close-suite.md)
- [`1065-create-annotated-release-tags.md`](./batch-cards/1065-create-annotated-release-tags.md)
- [`1066-prove-annotated-release-execution.md`](./batch-cards/1066-prove-annotated-release-execution.md)
- [`1067-remove-loopback-test-state-leakage.md`](./batch-cards/1067-remove-loopback-test-state-leakage.md)
- [`1068-settle-prepared-source-drift-policy.md`](./batch-cards/1068-settle-prepared-source-drift-policy.md)
- [`1069-prove-patch-release-candidate.md`](./batch-cards/1069-prove-patch-release-candidate.md)
- [`1070-add-papercuts-discovery-foundation.md`](./batch-cards/1070-add-papercuts-discovery-foundation.md)
- [`1071-add-papercuts-capture-and-closeout.md`](./batch-cards/1071-add-papercuts-capture-and-closeout.md)
- [`1072-add-explicit-member-and-typed-mount-schema.md`](./batch-cards/1072-add-explicit-member-and-typed-mount-schema.md)
- [`1073-cut-routing-over-to-explicit-membership.md`](./batch-cards/1073-cut-routing-over-to-explicit-membership.md)
- [`1074-delete-discovery-and-align-diagnostics.md`](./batch-cards/1074-delete-discovery-and-align-diagnostics.md)
- [`1075-prove-migration-and-close-explicit-membership-lane.md`](./batch-cards/1075-prove-migration-and-close-explicit-membership-lane.md)
- [`1076-unify-test-orchestration-for-v011.md`](./batch-cards/1076-unify-test-orchestration-for-v011.md)
- [`1077-enforce-pre-release-ci-proof.md`](./batch-cards/1077-enforce-pre-release-ci-proof.md)
- [`1078-build-bun-pin-planner-and-manifest-transaction.md`](./batch-cards/1078-build-bun-pin-planner-and-manifest-transaction.md)
- [`1079-wire-bun-pin-cli-json-and-link-interlocks.md`](./batch-cards/1079-wire-bun-pin-cli-json-and-link-interlocks.md)
- [`1080-prove-bun-pin-consumer-workflow-and-closeout.md`](./batch-cards/1080-prove-bun-pin-consumer-workflow-and-closeout.md)
- [`1081-decouple-bun-pin-from-pm-ls-lockfile-failures.md`](./batch-cards/1081-decouple-bun-pin-from-pm-ls-lockfile-failures.md)
- [`1082-populate-artifact-status-register.md`](./batch-cards/1082-populate-artifact-status-register.md)
- [`1083-create-decision-index-and-seeded-records.md`](./batch-cards/1083-create-decision-index-and-seeded-records.md)
- [`1084-run-first-governance-review-and-closeout.md`](./batch-cards/1084-run-first-governance-review-and-closeout.md)
- [`1085-align-doctor-with-secrets-schema.md`](./batch-cards/1085-align-doctor-with-secrets-schema.md)
- [`1086-audit-and-align-documentation-coverage.md`](./batch-cards/1086-audit-and-align-documentation-coverage.md)
- [`1087-guard-and-close-documentation-coverage.md`](./batch-cards/1087-guard-and-close-documentation-coverage.md)
- [`1088-build-documentation-profile-and-structural-index.md`](./batch-cards/1088-build-documentation-profile-and-structural-index.md)
- [`1089-add-bounded-documentation-context-query.md`](./batch-cards/1089-add-bounded-documentation-context-query.md)
- [`1090-prove-generic-and-northstar-profiles.md`](./batch-cards/1090-prove-generic-and-northstar-profiles.md)
- [`1091-audit-and-refresh-documentation-instructions-and-help.md`](./batch-cards/1091-audit-and-refresh-documentation-instructions-and-help.md)
- [`1092-add-external-skill-task-runner.md`](./batch-cards/1092-add-external-skill-task-runner.md)
- [`1093-add-help-first-command-discovery.md`](./batch-cards/1093-add-help-first-command-discovery.md)
- [`1094-fix-rhai-profile-dependent-expression-limits.md`](./batch-cards/1094-fix-rhai-profile-dependent-expression-limits.md)
- [`1095-prototype-catalog-pack-acquisition.md`](./batch-cards/1095-prototype-catalog-pack-acquisition.md)
- [`1099-add-rhai-storage-create-only.md`](./batch-cards/1099-add-rhai-storage-create-only.md)
- [`1100-preserve-ancestor-container-registry.md`](./batch-cards/1100-preserve-ancestor-container-registry.md)
- [`1101-bound-docs-context-cold-refresh.md`](./batch-cards/1101-bound-docs-context-cold-refresh.md)
- [`1102-reserve-docs-context-traversal-slot.md`](./batch-cards/1102-reserve-docs-context-traversal-slot.md)

## Current State

`g07` is closed through `g07.078`.

`g08` is complete through `g08.017`.

`g08.018` through `g08.023` are complete.

Cards `1051` through `1075` and strict specs `100` and `101` are complete.

`g08.029` is complete under contract `038`; card `1076` and strict spec `102`
are complete.

`g08.030` is complete under contract `039`; card `1077` and strict spec `103`
make hosted CI evidence for the exact candidate SHA release-blocking.

`g08.031` is complete under contract `040`; strict spec `104` is archived and
cards `1078` through `1081` are complete.

`g08.032` is complete under archived strict spec `105`. Cards `1082` through
`1084` delivered governance registers, seeded decisions, and the first review
cycle.

`g08.033` is complete under archived strict spec `106`. Card `1085` restored
doctor parity with the supported secret manifest surface and proved the
corrected installed CLI against Bovine.

`g08.034` is complete under archived strict spec `107`. Cards `1086` and
`1087` delivered the whole-repository documentation coverage audit, all
verified gap repairs, proportional recurrence guards, validation, and
closeout.

`g08.035` is complete under contract `041` and archived strict spec `108`.
Card `1089` shipped bounded `effigy docs context` retrieval with
`effigy.docs.context.v1` JSON on 2026-08-31, and card `1090` closed the lane the
same day: the Northstar ontology is now committed starter configuration copied
into this repository's own manifest, repository neutrality and installed-skill
independence are proved end to end, and `perf:docs-context-benchmark` replays a
predeclared retrieval corpus. Evidence:
[`31-213000-northstar-profile-proof-1090.md`](../../logs/2026-08/31-213000-northstar-profile-proof-1090.md).

`g08.036` is complete under archived strict spec `109`. Card `1091` delivered
the serial documentation, instruction, help, scan-evidence, and closeout batch.

`g08.037` is complete under architecture `025`, contract `042`, and archived
strict spec `110`. Card `1092` delivered the external skill-task runner and
returned the queue to card `1089`, which has since closed.

`g08.038` is complete under architecture `026`, contract `043`, and archived
strict spec `111`. Card `1093` added help-first grouping with no executable
group aliases and no selector-routing changes, and returned the
feature-placement queue to planning.

`g08.039` is complete under archived strict spec `112`. Card `1094` made Rhai
expression-depth parsing profile-independent while preserving release limits
and every unrelated runtime boundary. Evidence:
[`01-080923-rhai-profile-independent-limits-1094.md`](../../logs/2026-09/01-080923-rhai-profile-independent-limits-1094.md).

`g08.040` is complete under architecture `026`, contract `043`, and archived
strict spec `113`. Card `1095` landed the in-repository catalog-pack acquisition
prototype: four-layer resolution with a permanent compiled baseline, explicit
digest-addressed OCI and local installs through one validated transaction,
visible fallback with a `catalog.pack-health` doctor repair, and deterministic
rollback/reset. Official publication and concrete-asset cutover remain planning.
Evidence:
[`01-095641-catalog-pack-acquisition-prototype-1095.md`](../../logs/2026-09/01-095641-catalog-pack-acquisition-prototype-1095.md).

`g08.041` is complete. Card `1096` made bundled fragment inventory require a
first-level `service.toml` without changing filesystem/pack directory listing,
catalog packs, layers, schemas, or command contracts. Evidence:
[`01-133154-catalog-fragment-listing-1096.md`](../../logs/2026-09/01-133154-catalog-fragment-listing-1096.md).

`g08.042` is complete. Card `1097` removed the synthetic heading caused by
leading YAML frontmatter while preserving profiled metadata, relations, and
exact source spans. Evidence:
[`01-135932-markdown-frontmatter-1097.md`](../../logs/2026-09/01-135932-markdown-frontmatter-1097.md).

`g08.043` is complete. Card `1098` moved the no-match benchmark proof off
Effigy's live documentation corpus and guards the matrix against reintroducing
that vocabulary dependency. Evidence:
[`01-150452-no-match-benchmark-isolation-1098.md`](../../logs/2026-09/01-150452-no-match-benchmark-isolation-1098.md).

`g08.044` is complete under contract `044`. Card `1099` adds atomic
create-if-absent behavior to the retained Rhai storage PUT surface after Bovine
PR 32 proved HEAD then PUT cannot close the collision race. Evidence:
[`01-182838-rhai-storage-create-only-1099.md`](../../logs/2026-09/01-182838-rhai-storage-create-only-1099.md).

`g08.045` through `g08.047` are ready as three independent papercut lanes.
Cards `1100` through `1102` own child-catalog registry preservation, cold
docs-context time bounds/progress, and traversal-budget reachability. Their
runtime write sets are partitioned; shared front doors remain orchestrator-owned.

## Next Task

Pass card `1099`'s PR through exact-head orchestrator review, resume Bovine PR
32 after merge. Dispatch cards `1100`, `1101`, and `1102` in parallel, and keep
publication planning in its existing delegate workspace. Merge Effigy PRs one
at a time.

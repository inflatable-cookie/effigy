# State Domain Ownership Contract

`effigy-state` owns pure state-stack model, lineage, path, history, and
apply/capture planning behavior. The runner owns command dispatch and side
effects. The boundary keeps state-stack changes from growing a second domain
model inside `src/runner/state_command.rs`.

## Domain-Owned Behavior

`effigy-state` owns:

- `StateStackManifest`, layers, environments, roles, apply modes, and
  environment policy;
- layer ordering, validation, and lineage planning;
- report path conventions under `.effigy/reports/state/<stack>/`;
- history scanning, filtering, classification, latest-report selection, and
  summaries;
- apply report planning from lineage and layer/hook status models;
- capture mode derivation and produced-layer planning.

These operations can be computed or validated without writing files, running
tasks, contacting a provider, or invoking a container. Future media and
object-store state work should consume this domain owner rather than importing
runner internals.

## Runner-Owned Behavior

`src/runner/state_command.rs` and its modules own:

- CLI flags, dispatch, output mode, and text rendering;
- loading state configuration from the composed Effigy manifest;
- report and context file writes;
- task execution and apply/capture hooks;
- artifact staging, capture, and transport;
- SQL import and other impure adapters;
- provider/deploy composition at the command edge.

The runner may invoke a domain plan, but it must not duplicate path, history,
lineage, or report status rules locally.

## Compatibility

Extraction is structural. It must not silently change manifest grammar,
command behavior, report paths, JSON schema, or hook ordering. If a report
shape is externally consumed, preserve it or make an explicit compatibility
decision before changing it. State domain types stay in `effigy-state`; a
new generic utility crate would obscure the owner.

Focused pure-domain tests prove path, history, lineage, apply, and capture
rules. Command-level tests prove that the runner still performs side effects
and renders the same public result.

[Contract 016](016-state-stack-and-layered-seed-framework-contract.md)
owns the user-facing state-stack guarantees. This document owns the code
boundary that implements them.

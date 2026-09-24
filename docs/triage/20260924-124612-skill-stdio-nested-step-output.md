# Raw skill stdio across nested steps

Raised: 2026-09-24. Release-readiness audit requested by the operator.
Status: open; not execution authority.
Owner: Effigy Chatterbox for planning; skill execution maintainers for repair.

## Evidence

Contract `042` promises `effigy skill run --stdio passthrough` adds no stdout or
stderr bytes. The skill route selects `ExecutionOutputMode::Passthrough`, but
the runner's nested in-process task, draft, and built-in steps call
`render_nested_output`, which appends a newline when captured output lacks
one. The existing raw-stdio fixture exercises direct shell output; nested
skill tests cover normal JSON mode instead.

## Known direction and open checks

- Preserve accepted host-only skill shapes, source/consumer isolation,
  ordinary text/JSON rendering, and failure diagnostics.
- Prove exact stdin, stdout, stderr, and exit status for a valid nested step
  under passthrough, including output without a terminal newline and binary
  bytes if the nested command surface permits them.
- Determine whether every nested in-process step can meet the raw transport
  contract. If one cannot, resolve its behavior before dispatch rather than
  silently narrowing the documented promise.

## Promotion condition

Confirm the accepted nested-step shape and transport path, then obtain
operator confirmation for a dedicated implementation task under contract
`042`.

## Next check

Review the evidence with the operator and decide whether this is part of the
pre-release repair frontier.

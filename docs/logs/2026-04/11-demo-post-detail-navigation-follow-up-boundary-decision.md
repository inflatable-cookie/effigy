# Demo Post-Detail-Navigation Follow-Up Boundary Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.23`

## Summary

Chose browser metadata-query parity as the next bounded follow-up after
detail-pane navigation.

What this settled:

- the next honest operator-visible gap is not richer rendering or deeper
  runtime control
- the browser already renders `tag`, `mode`, and `cover` metadata from the
  demo registry, but operators still cannot filter or group by the full
  shipped metadata contract from inside the browser
- the existing self-hosted demos already provide enough variation across
  `mode`, `covers`, and `tags` to justify this slice without waiting for more
  demo fixtures

## Vision Target Delta

- Primary tags: `OPERATE`, `ROUTE`, `CONTRACT`
- Moved from `the browser can inspect long selected-demo records but still
  cannot use the full shipped metadata query model without dropping back to
  demo list` to `the next browser slice is explicitly narrowed to metadata-
  query parity rather than inferred from generic UI polish`
- Remaining open:
  - implement bounded browser `tag`, `mode`, and `cover` filters
  - extend browser grouping controls to the full shipped `group-by` contract
  - keep richer rendering and deeper runtime questions deferred until after
    that parity slice is shipped

## Validation

- `git diff --check`
- `effigy qa:docs`

## Outcome

The lane stays honest. The next batch reuses shipped runner semantics instead
of inventing new browser-only behavior, and it remains strictly inside browser
ergonomics rather than widening into runtime control, terminal behavior, or
desktop-client work.

## Next Task

Use the next `g02.003` ready card to implement bounded browser metadata-query
parity on top of the shipped demo browser.

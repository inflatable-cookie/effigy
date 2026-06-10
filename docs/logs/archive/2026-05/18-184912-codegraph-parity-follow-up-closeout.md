# CodeGraph Parity Follow Up Closeout

Date: 2026-05-18  
Roadmap: [`g07.049`](../../../roadmaps/g07/049-codegraph-parity-follow-up-closeout.md)  
Batch card: [`999`](../../../roadmaps/g07/batch-cards/999-close-codegraph-parity-follow-up.md)  
Strict lane: [`092`](../../../specs/092-codegraph-parity-follow-up-strict-lane.md)

## Decision

Do **not** claim "as good as or better than CodeGraph" yet.

This follow-up lane fixed the severe warm-query regression and made the last
deferred parity cases executable. That closes the bounded scope honestly. It
does not yet justify a parity claim because warm live-repo queries are still
multi-second, one active case now lands on an acceptable alternate owner, and
the PHP fixture case still prefers an implementation neighbor over the front
controller entrypoint.

## Warm Live-Repo Posture

Fresh warm-index status after reindex:

- graph ready: `true`
- stale paths: `0`
- failed paths: `0`
- indexed files: `3319`
- symbols: `32167`
- edges: `141752`
- references: `64615`

## Active Corpus Results

| Case | Expected primary | Current top owner | Time | Result |
| --- | --- | --- | ---: | --- |
| deploy provider export | `src/runner/deploy_command/provider_package.rs` | `src/runner/deploy_command/provider_package.rs` | `3.96s` | exact owner |
| graph watch implementation | `crates/effigy-codegraph/src/watch.rs` | `crates/effigy-codegraph/src/watch.rs` | `3.37s` | exact owner |
| release orchestration | `crates/effigy-release/src/lib.rs` | `crates/effigy-release/src/lib.rs` | `3.62s` | exact owner |
| graph status stale detection | `crates/effigy-codegraph/src/index.rs` | `crates/effigy-codegraph/src/query/mod.rs` | `6.43s` | acceptable alternate |
| task route parsing | `src/runner/execute/routing.rs` | `src/runner/execute/routing.rs` | `6.71s` | exact owner |
| bundle source git | `crates/effigy-manifest/src/bundles/source.rs` | `crates/effigy-manifest/src/bundles/source.rs` | `7.25s` | exact owner |
| graph agent docs | `docs/guides/076-code-graph-and-agent-workflows.md` | `docs/guides/076-code-graph-and-agent-workflows.md` | `4.17s` | exact owner |

Active-corpus score:

- `6/7` exact expected primaries
- `1/7` acceptable alternate
- `0/7` hidden owner failures
- warm live-repo queries now sit in a `3.37s` to `7.25s` range instead of the
  earlier `6.83s` to `137.84s` regression window

## Fixture-Backed Cases

From `998`:

| Case | Result | Top owner |
| --- | --- | --- |
| `affected-test-proxy` | exact | `tests/graph_watch_tests.rs` |
| `cross-language-php-front-controller` | acceptable alternate | `legacy/App/Controller.php` |

## Comparison To Prior Closeout

Compared with `g07.045`:

- the severe warm-query regression is fixed
- release-orchestration ranking is fixed
- the deferred fixture cases are now executable
- parity is still not ready to claim because:
  - warm query time is still materially slower than the CodeGraph-style story
    we want to match
  - one live active case and one fixture case still land on acceptable
    alternates rather than the preferred entry owner

## Interpretation

- `effigy graph` is now a credible day-to-day navigation tool for agents
- this bounded follow-up lane is complete
- the repo should not keep an active ready card for graph parity work
- any later push toward CodeGraph-level claims needs a fresh bounded lane with
  a tighter target, likely around sub-second or low-single-digit warm query
  behavior and stronger entrypoint ownership for cross-language cases

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: the follow-up lane removed the extreme latency regression and converted
  the last deferred parity placeholders into executable fixture-backed proof
- remains open: any future attempt to claim CodeGraph-level parity or better

## Next Task

No active ready card.

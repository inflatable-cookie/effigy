# Architecture

Effigy's root manifest owns catalog membership. The CLI resolves the repository and selector, then runs either a built-in or a manifest task. Execution and diagnostics share routing facts; machine output uses versioned contracts.

The [architecture directory](architecture/000-overview.md) holds the detailed ownership map. Start with [package ownership](architecture/010-package-map.md), [feature placement](architecture/026-feature-placement-and-command-surface.md), and [catalog graph scope](architecture/027-catalog-scoped-code-graph.md) for changes across boundaries. The [contract index](contracts/README.md) owns exact interfaces.


## External orchestration boundary

Tom ruled on 2026-10-13: "We don't control Paseo - we cannot rely on adding
anything to Paseo for anything in effigy." Effigy and its owned validation and
review workflows must use existing supported external interfaces. A required
Paseo daemon or SDK extension is not an Effigy prerequisite or delivery route.

Queue owns its task, run and review coordination. It must describe the guarantees
its existing interfaces actually provide: dated observations and immutable
source identities do not prove writer exclusion or a continuing idle reservation.
A Queue-owned revised-head route must retain independent exact-head review and
fresh admission/premerge revalidation, refuse unresolved active or unknown work,
and invalidate drift without carrying old approval. If a guarantee cannot be
provided within owned scope, record that exact boundary instead of assuming an
upstream change. This ruling grants no consumer retry, runtime installation,
release, settings change or historical settlement.

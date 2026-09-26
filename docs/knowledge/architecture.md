# Architecture

Effigy's root manifest owns catalog membership. The CLI resolves the repository and selector, then runs either a built-in or a manifest task. Execution and diagnostics share routing facts; machine output uses versioned contracts.

The [architecture directory](architecture/000-overview.md) holds the detailed ownership map. Start with [package ownership](architecture/010-package-map.md), [feature placement](architecture/026-feature-placement-and-command-surface.md), and [catalog graph scope](architecture/027-catalog-scoped-code-graph.md) for changes across boundaries. The [contract index](contracts/README.md) owns exact interfaces.

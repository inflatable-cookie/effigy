# &lt;PROJECT_NAME&gt;

Replace this paragraph with a one-sentence product description.

## Operating model

This repo uses the Northstar + Effigy consumer contract:

- Northstar owns the docs and planning shape (vision / roadmaps / logs)
- Effigy owns the executable validation and operator surface

**Selectors:** names like **`qa`** or **`validate`** are **tasks** (or task
chains) defined in **`effigy.toml`** for this repo. **`effigy test`** is always
the built-in test orchestrator; configure deterministic repo-owned suites under
**`[test.suites]`**. See Effigy’s quick start:
[`021-quick-start-and-command-cookbook.md`](https://github.com/inflatable-cookie/effigy/blob/main/docs/guides/021-quick-start-and-command-cookbook.md).

Start here:

```sh
effigy tasks            # discover repo work
effigy doctor           # health / routing surface
effigy test --plan      # inspect the test plan
effigy qa               # full validation bundle (repo-defined aggregator)
```

## Docs

- [`docs/README.md`](docs/README.md) — docs index and authority
- [`docs/vision/README.md`](docs/vision/README.md) — current product vision
- [`docs/roadmaps/README.md`](docs/roadmaps/README.md) — active milestone queue
- [`docs/logs/README.md`](docs/logs/README.md) — evidence and decisions

## Agent contract

See [`AGENTS.md`](AGENTS.md) for the full agent operating contract.
New agents should read that file first.

## Changelog

User-facing changes are tracked in [`CHANGELOG.md`](CHANGELOG.md) under
the `[Unreleased]` section until a release is cut.

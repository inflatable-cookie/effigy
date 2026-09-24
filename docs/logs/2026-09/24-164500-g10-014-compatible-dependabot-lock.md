# g10.014 Compatible Dependabot Lock Refresh

Status: implementation complete; awaiting independent review, current-base CI, merge, and source-PR disposition
Created: 2026-09-24
Roadmap: g10.014
Batch: post-0.13.0-compatible-dependabot-lock

## Summary

- Started from pushed `main` at `d12f114f2251f890c40aa9282d5b359f05939fa9` after published `v0.13.0`. Workspace version remains `0.13.0`.
- Applied six precise lockfile updates covering Dependabot PRs #79, #80, #96, #97, #99, and #100. Manifests, the vendored `s3` patch, and unrelated packages were left unchanged.
- Resolver-required transitives: `hickory-net` stayed at `0.26.1`; `smartstring` and `thin-vec` dropped unused optional `serde` features after `rhai 1.26.1`. No other package versions moved. Package count stayed 511.

## Source PRs

| PR | Title | Bot head | Baseline -> target |
| --- | --- | --- | --- |
| [#79](https://github.com/inflatable-cookie/effigy/pull/79) | bump hyper 1.11.0 to 1.11.1 | `54400e8615d41a1489a3c3f9b520bda8c087cfce` | hyper 1.11.0 -> 1.11.1 |
| [#80](https://github.com/inflatable-cookie/effigy/pull/80) | bump rhai 1.25.1 to 1.26.1 | `5dabb73ae23d430df7b95e0c7fd23bb2cfeaece0` | rhai 1.25.1 -> 1.26.1 |
| [#96](https://github.com/inflatable-cookie/effigy/pull/96) | bump toml 1.1.4+spec-1.1.0 to 1.1.6+spec-1.1.0 | `6fcd669ce854d694a9e97f54e85f16a91ab700ce` | toml 1.1.4+spec-1.1.0 -> 1.1.6+spec-1.1.0 |
| [#97](https://github.com/inflatable-cookie/effigy/pull/97) | bump hickory-proto 0.26.1 to 0.26.2 | `abed6c891fd263d592e5d4317bc086f058bbc9da` | hickory-proto 0.26.1 -> 0.26.2 |
| [#99](https://github.com/inflatable-cookie/effigy/pull/99) | bump indexmap 2.14.0 to 2.14.2 | `d0ee0c95eebb29d6c570f6e7f96c02793cd657ec` | indexmap 2.14.0 -> 2.14.2 |
| [#100](https://github.com/inflatable-cookie/effigy/pull/100) | bump hickory-server 0.26.1 to 0.26.2 | `132f83807129d4f93a2009c5911233e239c3b9be` | hickory-server 0.26.1 -> 0.26.2 |

Those bot heads were lockfile-only against older bases. This replacement was resolved on current `main` with `cargo update -p <crate> --precise <version>` for each target. Aggregate PR: [#117](https://github.com/inflatable-cookie/effigy/pull/117). Do not close the bot PRs until that PR merges.

## Lockfile delta

- `Cargo.lock`: 12 insertions, 16 deletions. Only the six named packages changed versions and checksums.
- `hickory-net` remained `0.26.1`.
- `rhai 1.26.1` no longer enables `serde` on `smartstring` or `thin-vec`; those crate versions are unchanged.
- `[patch.crates-io] s3 = { path = "vendor/s3" }` is unchanged.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`.
- Movement: six compatible Dependabot updates absent from the 0.13.0 lockfile on stale overlapping heads -> one current-base lockfile refresh with the six target versions and no unrelated package-version moves.
- Remaining gap: independent exact-head review, hosted CI, merge, and superseded close of the six named bot PRs. `g10.015` stays blocked until this task's terminal closeout.

## Validation Performed

- `cargo tree -i` for hyper, rhai, toml, hickory-proto, indexmap, and hickory-server: each reports the target version on the locked graph.
- Before/after package inventory: 511 packages; exactly six version replacements plus the two optional-feature drops above.
- `cargo test --locked -p effigy-gateway -p effigy-rhai -p effigy-manifest -p effigy-catalog`: passed (653 lib/integration tests, 0 failed).
- `cargo test --workspace --locked`: passed, 0 failed.
- `cargo deny check`: advisories, bans, licenses, and sources ok.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets --locked -- -D warnings`: passed.
- `effigy qa:docs`: passed.
- `git diff --check`: passed.

No user-visible CLI or JSON behavior changed, so `CHANGELOG.md` was not updated.

## Risks

- Source bot PRs remain open until Queue closes them after merge. Closing them now would fail the disposition oracle.
- `g10.015` must not write `Cargo.lock` until this task reaches terminal closeout.

## Next Task

Independent exact-head review and current-base CI on the aggregate PR. After merge, close #79, #80, #96, #97, #99, and #100 as superseded with the merge link, then dispatch `g10.015`.

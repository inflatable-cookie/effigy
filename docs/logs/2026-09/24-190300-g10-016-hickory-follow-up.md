# g10.016 Hickory 0.26.3 Follow-Up

Status: implementation complete; awaiting independent review, current-base CI, merge, and source-PR disposition
Created: 2026-09-24
Roadmap: g10.016
Baseline: `f586103f6` (`g10.015` terminal record, `main`)

## Resolution

| Source PR | Dependency | Baseline | Target |
| --- | --- | --- | --- |
| [#97](https://github.com/inflatable-cookie/effigy/pull/97) (closed, branch deleted) | `hickory-proto` | 0.26.2 | 0.26.3 |
| [#100](https://github.com/inflatable-cookie/effigy/pull/100) (open) | `hickory-server` | 0.26.2 | 0.26.3 |

The manifests already require `hickory-proto = "0.26"` and `hickory-server = "0.26"`
(`crates/effigy-gateway/Cargo.toml`); no manifest change was needed or made.
The aggregate replacement PR covers both retargeted bot targets explicitly, as
required because #97 cannot reopen (GitHub REST/GraphQL reopen failed: its
`dependabot/cargo/hickory-proto-0.26.2` branch was deleted).

Resolver-required transitive: `hickory-net` 0.26.1 -> 0.26.3. `hickory-server`
depends on the matching `hickory-net` release, so it moved with the server bump.
The complete lockfile delta is three `[[package]]` entries — version and
checksum for `hickory-net`, `hickory-proto`, and `hickory-server`. No other
package version or dependency edge changed. All three 0.26.3 releases carry
dependency lists identical to their 0.26.2/0.26.1 predecessors; upstream
v0.26.3 fixes DNSSEC verification regressions, QUIC/HTTP/3 server timeouts, and
insecure ancestor-delegation handling.

## Bounded-resolution note

`cargo update -p hickory-proto@0.26.2 --precise 0.26.3` and the follow-up
`cargo update -p hickory-server@0.26.2 --precise 0.26.3` each re-resolved
unrelated edges while producing the required entries: `tempfile 3.27.0`'s
`getrandom >=0.3.0, <0.5` requirement dropped from the locked `getrandom 0.4.3`
selection to `0.3.4` (splitting the graph into an extra getrandom version), and
several `windows-sys` dependency edges re-pointed from 0.61.2 to existing
0.60.2/0.59.0 entries. That drift is exactly the "Cargo silently changes
unrelated package versions" counterexample in the g10.016 review oracle, and it
reproduced deterministically regardless of update order. Because the 0.26.3
dependency lists are identical to their predecessors, the bounded resolution was
produced by applying only the three version/checksum entry updates to the
pristine `g10.015` lockfile — the same surgical form Dependabot itself writes.
`cargo fetch --locked` then accepted the resolution and verified the new
checksums against the registry, and every `--locked` validation below ran
against it.

## Compatibility evidence

- The focused `effigy-gateway` suite covers DNS end to end on the upgraded
  stack: `dns_resolves_registered_domain_end_to_end` and
  `dns_resolves_registered_domain_to_route_specific_ip_end_to_end` exercise the
  hickory-server listener plus resolver path, and the 15-test integration suite
  (proxy routing, stats, health, timeouts) passed unchanged.
- `hickory-net` 0.26.3's dependency list is identical to 0.26.1's; the gateway
  surface (`crates/effigy-gateway/src/dns.rs`) uses only `hickory_proto` op/rr
  APIs that are unchanged between 0.26.2 and 0.26.3, and no new compiler
  warning or behavioral test gap appeared, so no new compatibility assertion
  was required.

## Validation

- `cargo test -p effigy-gateway --locked`: passed, including the DNS
  end-to-end tests and the full gateway integration suite.
- `cargo test --workspace --locked`: 97 test suites, 3,922 passed, 0 failed.
- `cargo deny check`: passed; advisories, bans, licenses, and sources all ok.
  Existing policy warnings remain.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets --locked -- -D warnings`: passed.
- `effigy qa:docs`: passed after registering this evidence log in the active
  log index.
- `git diff --check`: passed.
- Hosted CI and exact-head review: pending.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`.
- Movement: the two Dependabot 0.26.3 retargets left false-superseded (#97) and
  dangling (#100) after the 0.26.2-only #117 merge -> one current-base bounded
  lockfile replacement covering both targets explicitly, with gateway DNS
  behavior validated on the upgraded stack.
- Remaining gap: independent exact-head review, hosted CI, merge, and
  source-PR disposition.

No user-visible CLI or JSON behavior changed; `CHANGELOG.md` is unchanged,
matching the `g10.015` lockfile-only precedent. Queue owns merge, closing #100
as superseded with the merge link, and adding that link to #97's correction
thread after the aggregate PR merges.

## Next Task

Independent exact-head review and current-base CI on the aggregate PR. After
merge, Queue closes #100 as superseded with the merge link and links the
replacement in #97's correction thread.

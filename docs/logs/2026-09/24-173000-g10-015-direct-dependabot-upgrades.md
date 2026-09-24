# g10.015 Direct Dependabot Upgrades

Status: implementation complete; awaiting independent review, current-base CI, merge, and source-PR disposition
Created: 2026-09-24
Roadmap: g10.015
Baseline: `03a4b44e56e63b7902680b3401f1752b14202819` (`g10.014` terminal record, `main`)

## Resolution

| Source PR | Dependency | Baseline | Target |
| --- | --- | --- | --- |
| [#81](https://github.com/inflatable-cookie/effigy/pull/81) | `argon2` | 0.5.3 | 0.6.0 |
| [#98](https://github.com/inflatable-cookie/effigy/pull/98) | `tree-sitter` | 0.26.13 | 0.27.0 |
| [#101](https://github.com/inflatable-cookie/effigy/pull/101) | `tree-sitter-language` | 0.1.7 | 0.1.8 |
| [#102](https://github.com/inflatable-cookie/effigy/pull/102) | `tabled` | 0.21.0 | 0.22.0 |

The three direct manifests changed. `cargo update -p argon2 --precise 0.6.0` resolved all four targets against current `main`. Required transitives: `blake2` 0.10.6 -> 0.11.0, `password-hash` 0.5.0 -> 0.6.1, new `phc` 0.6.1, `tabled_derive` 0.11.0 -> 0.12.0. `proc-macro-error2` and `proc-macro-error-attr2` dropped. The lockfile also adjusted dependency edges on existing `digest` and `tempfile` versions. No unrelated package version changed.

Aggregate PR: [#118](https://github.com/inflatable-cookie/effigy/pull/118). The four source PRs remain open until it merges.

## Compatibility evidence

- Before the upgrade, the v0.13.0 build with `argon2 0.5.3` derived this Argon2id key from passphrase `correct horse battery staple`, 32 bytes of salt `0x2a`, and parameters m=19456 KiB, t=2, p=1: `9ae366cc91b47ba2598b19140035cde0e86a49dc719811965943b16eb2c8598d`. A focused test pins that key and the old XChaCha20Poly1305 ciphertext generated with a 24-byte `0x07` nonce, then decrypts those old ciphertext bytes after the upgrade.
- The pre-upgrade language indexer suite passed 13 tests. The upgraded suite passed the same Rust, JavaScript/TypeScript, Python, and PHP symbol/edge and parse-diagnostic assertions. It also retained the mixed-repository and graph JSON tests in the full `effigy-codegraph` suite.
- The v0.13.0 plain renderer produced exact bytes `Name    Value  \nalpha   1      \nbeta    two    ` for a two-column fixture. A focused post-upgrade assertion pins that string, including spaces and newlines.

## Validation

- `cargo test -p effigy-secrets -p effigy-codegraph -p effigy-ui --locked`: passed, 178 unit tests, 0 failed.
- `cargo test --workspace --locked`: full log completed 97 test suites with 3,922 passed, 0 failed, including CLI fixtures and doc tests. The shell wrapper itself then failed because it assigned zsh's read-only `status` variable; the test log has no failure or error entries.
- `cargo deny check`: passed; advisories, bans, licenses, and sources all ok. Existing policy warnings remain.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets --locked -- -D warnings`: passed.
- `effigy qa:docs`: passed after registering this evidence log in the active log index.
- `git diff --check`: passed.
- Hosted CI and exact-head review: pending.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`.
- Movement: four stale-base direct/transitive bot upgrades -> one current-base replacement with fixed vault and table vectors plus graph fixture parity.
- Remaining gap: independent exact-head review, hosted CI, merge, and source-PR disposition.

No user-visible CLI or JSON behavior changed; `CHANGELOG.md` is unchanged. Queue owns merge and closing the four source PRs after the aggregate PR merges.

## Next Task

Independent exact-head review and current-base CI on the aggregate PR. After merge, Queue closes #81, #98, #101, and #102 as superseded with the merge link.

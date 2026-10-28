# 059 - Manifest Composition Guide

Use this guide when one `effigy.toml` is starting to carry too many unrelated
concerns and the right fix is composition, not more comments or wrapper files.

This is the practical front door for `[manifest].include`, partial manifests,
override boundaries, and fragment layout.

## Vision Alignment

- Primary tags: `MAINT`, `CONTRACT`, `ADOPT`
- Target movement: manifest structure stays explicit and composable as repos
  grow, without hiding ownership in ad hoc script files.

## 1) Keep One Canonical Entry Point

Effigy still starts at root `effigy.toml`.

Use includes to split concerns, not to create multiple competing roots.

Root example:

```toml
[manifest]
include = [
  "config/tasks.toml",
  "docs/effigy.docs.toml",
]
```

Rule:

- `effigy.toml` is the canonical entrypoint
- included files are partial manifests
- included paths resolve relative to the file that declares them
- any fragment can declare `[manifest].minimum_effigy_version = "X.Y.Z"` to
  fail early when the active Effigy binary is too old for that manifest shape
- repo-local development builds like `v0.6.1+local.<hash>` are treated as
  ahead of tagged releases, so they still pass newer manifest floors during
  local feature work

Nested fixtures, examples, or child projects can opt into being their own
Effigy root:

```toml
[manifest]
root = true
```

Use this when a nested `effigy.toml` should not be promoted to a parent
Cargo/npm workspace during automatic root resolution. This is for real nested
repo boundaries or smoke fixtures, not for ordinary include fragments.

## 2) Split By Concern

Good fragment shapes:

- `config/tasks.toml` for broad repo tasks
- `docs/effigy.docs.toml` for docs policy and docs helper tasks
- `config/containers.toml`, `config/release.toml`, or `config/scan.toml` for
  other singleton repo-local manifest fragments
- `scripts/effigy.scripting.toml` for Rhai-backed task clusters in Rust-first
  repos
- `effigy.local.toml` for local-only overrides when the repo explicitly wants
  them

Bad split:

- one fragment per tiny feature
- multiple fragments competing to define the same keys without a clear override
  policy
- fragments that only exist to hide confusing task ownership

Aim for “one concern per fragment,” not “maximum fragmentation.”

## 3) Know How Merge Works

Distinct keys merge additively by default.

Conflicting values fail unless the include entry explicitly allows that path to
be replaced.

Example:

```toml
[manifest]
include = [
  "effigy.tasks.toml",
  { path = "effigy.local.toml", override = ["tasks.dev", "release.sync-files"] },
]
```

Meaning:

- `config/tasks.toml` contributes normal values
- `effigy.local.toml` may replace only `tasks.dev` and `release.sync-files`
- other conflicting keys still fail

Override is path-scoped and replaces the whole addressed value.

For arrays specifically, `[manifest].extend = [...]` appends instead of
replacing:

```toml
[manifest]
extend = ["containers.web.dns.domains"]
include = ["envs/cumberland/effigy.env.toml"]
```

This is the right tool when an imported fragment needs to grow a shared list
(DNS routes, env-schema paths, scan globs) or merge entries into a shared
table without restating every existing entry. `extend` needs arrays or
tables on both sides and conflicts with `override` on
the same path — pick one per path.

Includes can also opt out of the "missing file is an error" default:

```toml
[manifest]
include = [
  { path = "effigy.local.toml", optional = true },
]
```

A missing optional file is silently skipped; a present one loads and
merges normally. Auto-discovery uses different conflict rules (see §4b).

## 4b) Auto-Discovered `effigy.local.toml`

If a file named `effigy.local.toml` sits next to the root manifest,
Effigy auto-discovers it after the committed layers. Conflicting leaves
are replaced by the local value; inspection reports their sources and
replacement history. The local file may declare `[manifest].extend` to
append at selected paths, or include further fragments under ordinary
composition rules.

Add `effigy.local.toml` to `.gitignore` to keep it machine-local.
Discovery does not change Git ignore files.

If the committed manifest explicitly includes the same file, Effigy detects
it by canonical path and does not auto-discover it again. That explicit
include follows ordinary conflict and override rules, even when
`EFFIGY_NO_LOCAL_OVERLAY=1` is set.

For CI determinism, set `EFFIGY_NO_LOCAL_OVERLAY=1` to skip
auto-discovery entirely.

## 4) Inspect The Effective Result

Use inspection every time composition gets non-trivial:

```sh
effigy config --inspect
effigy config --inspect --path tasks.dev
```

Use full inspect when the question is “what is the merged manifest?”

Use `--path` when the question is narrower:

- where did this one value come from?
- which include replaced it?
- which file currently owns this task?

Do this before relying on composition in CI, docs policy, or consumer-repo
adoption.

## 5) Put Rhai Script Clusters In Their Own Fragment

A Rust-first repo can use a focused scripting fragment too:

```toml
[manifest]
include = ["scripts/effigy.scripting.toml"]
```

Then in `scripts/effigy.scripting.toml`:

```toml
[tasks.report:stamp]
run = { rhai = "scripts/report-stamp.rhai" }
```

This keeps Rhai-backed automation visible without bloating the root manifest or
reintroducing shell wrappers.

Use this when:

- the repo is Rust-first
- the scripts are really Effigy-native task glue
- you want scripting ownership to live under `scripts/`

## 6) Prefer One `config/` Root For Singleton Fragments

When a fragment is just one repo-local TOML file with no strong neighboring
assets, prefer collecting it under `config/`:

- `config/tasks.toml`
- `config/containers.toml`
- `config/release.toml`
- `config/scan.toml`

Keep a fragment outside `config/` when it is tightly coupled to local assets:

- `docs/effigy.docs.toml` beside docs policy files
- `scripts/effigy.scripting.toml` beside Rhai helpers
- env-specific overlays under their own env folder

That keeps the repo root from turning into a row of one-file directories while
still letting contextual fragments live next to the files they actually govern.

For the Rhai host API and v1 limits, use
[`061-rhai-script-steps-guide.md`](061-rhai-script-steps-guide.md).

## 7) Keep Shared Helpers Shared

Composition is not just about moving things out. It is also about keeping
shared pieces obvious.

Examples of good shared roots:

- top-level `[env]` profiles used by many tasks
- broad repo tasks like `dev`, `build`, `lint`, `docs:qa`

Examples of good fragment-local entries:

- one docs-policy cluster
- one local override file

## 8) Avoid Drift

Composition starts to hurt when:

- the root file no longer tells you what fragments exist
- one fragment silently overrides another without an explicit path
- docs explain a value but `config --inspect` shows it comes from somewhere else
- task ownership is split so widely that people stop knowing where to edit

Use composition to reduce cognitive load, not to move it around.

## Expected Outcome

After this guide, you should be able to:

- split one manifest into a few focused fragments
- use `[manifest].include` safely
- inspect the effective merged result

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- [`050-env-schema-integration.md`](050-env-schema-integration.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](056-northstar-effigy-consumer-repo-contract.md)
- [`061-rhai-script-steps-guide.md`](061-rhai-script-steps-guide.md)
- [`070-per-machine-overlays-and-external-mounts.md`](070-per-machine-overlays-and-external-mounts.md)
  for a worked end-to-end pattern that combines auto-discovery,
  `extend`/`optional` includes, domain sugar, and external host
  mounts into one developer-onboarding workflow.

## Next Step

After this guide, use
[`022-manifest-cookbook.md`](022-manifest-cookbook.md) when you want more
copy-paste manifest patterns instead of composition rules.

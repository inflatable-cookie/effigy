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
  "effigy.tasks.toml",
  "effigy.docs.toml",
  "demos/effigy.demos.toml",
]
```

Rule:

- `effigy.toml` is the canonical entrypoint
- included files are partial manifests
- included paths resolve relative to the file that declares them

## 2) Split By Concern

Good fragment shapes:

- `effigy.tasks.toml` for broad repo tasks
- `effigy.docs.toml` for docs policy and docs helper tasks
- `demos/effigy.demos.toml` for `[demos.*]` entries and tightly related demo
  helpers
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

- `effigy.tasks.toml` contributes normal values
- `effigy.local.toml` may replace only `tasks.dev` and `release.sync-files`
- other conflicting keys still fail

Override is path-scoped and replaces the whole addressed value.

## 4) Inspect The Effective Result

Use inspection every time composition gets non-trivial:

```sh
effigy config --inspect
effigy config --inspect --path tasks.dev
effigy config --inspect --path demos.login-smoke
```

Use full inspect when the question is “what is the merged manifest?”

Use `--path` when the question is narrower:

- where did this one value come from?
- which include replaced it?
- which file currently owns this demo or task?

Do this before relying on composition in CI, docs policy, or consumer-repo
adoption.

## 5) Put Demo Registry In Its Own Fragment

A common practical split is:

```toml
[manifest]
include = ["demos/effigy.demos.toml"]
```

Then in `demos/effigy.demos.toml`:

```toml
[demos.login-smoke]
title = "Login smoke"
summary = "Checks that login still works."
proof = "Operator-visible smoke proof for login."
owner = "platform"
mode = "interactive"
status = "ready"
covers = ["auth.login"]
run = [
  { task = "demo:conventions" },
  { run = "python3 demos/run_login_smoke.py" },
]
```

This keeps demo proof inventory together and stops the root manifest from
turning into a mixed grab-bag of runtime, docs, release, and proof entries.

Use inline `run = [ ... ]` when the proof sequence belongs to the demo itself.

Keep a separate task only when it is genuinely reusable outside that one demo.

## 5b) Put Rhai Script Clusters In Their Own Fragment

A Rust-first repo can use a focused scripting fragment too:

```toml
[manifest]
include = ["scripts/effigy.scripting.toml"]
```

Then in `scripts/effigy.scripting.toml`:

```toml
[tasks.link:local]
run = [{ rhai = "scripts/rhai/install-local-bin-links.rhai" }]
```

This keeps Rhai-backed automation visible without bloating the root manifest or
reintroducing shell wrappers.

Use this when:

- the repo is Rust-first
- the scripts are really Effigy-native task glue
- you want scripting ownership to live under `scripts/`

For the Rhai host API and v1 limits, use
[`061-rhai-script-steps-guide.md`](./061-rhai-script-steps-guide.md).

## 6) Keep Shared Helpers Shared

Composition is not just about moving things out. It is also about keeping
shared pieces obvious.

Examples of good shared roots:

- top-level `[env]` profiles used by many tasks or demos
- broad repo tasks like `dev`, `build`, `lint`, `docs:qa`
- shared demo helper tasks such as `demo:conventions`

Examples of good fragment-local entries:

- one demo registry cluster
- one docs-policy cluster
- one local override file

If a task is only there to wrap one demo’s private run steps, consider moving
those steps inline into the demo instead.

## 7) Avoid Drift

Composition starts to hurt when:

- the root file no longer tells you what fragments exist
- one fragment silently overrides another without an explicit path
- docs explain a value but `config --inspect` shows it comes from somewhere else
- demo and task ownership are split so widely that people stop knowing where to
  edit

Use composition to reduce cognitive load, not to move it around.

## Expected Outcome

After this guide, you should be able to:

- split one manifest into a few focused fragments
- use `[manifest].include` safely
- inspect the effective merged result
- keep demo registry and helper task ownership coherent

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`050-env-schema-integration.md`](./050-env-schema-integration.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)
- [`058-demo-system-guide.md`](./058-demo-system-guide.md)
- [`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
- [`061-rhai-script-steps-guide.md`](./061-rhai-script-steps-guide.md)

## Next Step

After this guide, use
[`058-demo-system-guide.md`](./058-demo-system-guide.md) if the next split is a
demo registry fragment, use
[`060-consumer-demo-migration-guide.md`](./060-consumer-demo-migration-guide.md)
if the next job is migrating an existing demo-script surface, or use
[`022-manifest-cookbook.md`](./022-manifest-cookbook.md) when you want more
copy-paste manifest patterns instead of composition rules.

> Status: Deprecated
> Superseded by: [`062-distribution-system-guide.md`](./062-distribution-system-guide.md) and [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
> Kept for: historical CI pinning and wrapper-migration detail

# 041 - Distribution CI Pinning and Wrapper Migration

This guide standardizes deterministic CI installs and migration from legacy `bun effigy` wrappers to direct binary usage.

## 1) Recommended Channel Policy

- CI and release automation: pinned git tag install.
- Daily local development: `cargo run` (source of truth while iterating).
- Operator usage in stable repos: PATH-installed `effigy` binary.
- Legacy wrapper (`bun effigy`): temporary compatibility fallback only.

## 2) CI Pinned Install Recipes

Use explicit versions in CI for repeatable behavior.

### Git tag install (primary source-build path)

```bash
cargo install \
  --locked \
  --git https://github.com/inflatable-cookie/effigy.git \
  --tag v0.__.__ \
  effigy \
  --force
```

Note: Effigy does not publish to crates.io because its workspace contains
app-specific internal crates not intended as reusable library dependencies.

### Existing repo clone + local path install

```bash
cargo install --path . --root ./.local-install --force
export PATH="$PWD/.local-install/bin:$PATH"
```

### Cache-friendly GitHub Actions step

```yaml
- name: Install pinned effigy
  run: |
    cargo install \
      --locked \
      --git https://github.com/inflatable-cookie/effigy.git \
      --tag v0.__.__ \
      effigy \
      --force
    effigy --help
```

## 3) Bootstrap Script Pattern (Team Repos)

Use a single bootstrap script so local and CI setup stay aligned:

```bash
#!/usr/bin/env bash
set -euo pipefail
TAG="${EFFIGY_VERSION:-v0.__.__}"
cargo install \
  --locked \
  --git https://github.com/inflatable-cookie/effigy.git \
  --tag "$TAG" \
  effigy \
  --force
effigy --version
```

For local self-built installs, `effigy version` may show a stamped active build
string such as `v0.5.0+local.abc123`. Treat `binary.version` / release semver
as the pinning source of truth; the local suffix is there to distinguish
working builds while iterating.

Policy:
- Pin `EFFIGY_VERSION` in CI variables.
- Bump only via reviewed release PRs.
- Validate with `effigy release verify-install --tag <tag>` before changing tag pins.

## 4) Wrapper Migration (`bun effigy` -> `effigy`)

### Before (legacy wrapper)

```json
{
  "scripts": {
    "effigy": "cargo run --manifest-path ../effigy/Cargo.toml --bin effigy --",
    "tasks:list": "bun effigy tasks",
    "doctor": "bun effigy doctor"
  }
}
```

### After (direct binary, no package script re-export)

```sh
effigy tasks
effigy doctor
```

Migration steps:
1. Install pinned Effigy binary in CI and developer setup.
2. Replace `bun effigy ...` calls with direct `effigy ...`.
3. Keep a short rollback window with wrapper scripts on a separate branch/tag.
4. Remove wrapper script entries once CI and local smoke checks are stable.
5. Do not add `package.json` scripts that re-export Effigy tasks.

## 5) Rollback and Fallback

- Fast rollback: revert the pinned tag to previous known-good release.
- Fallback command for temporary incidents:
  - `cargo run --manifest-path /abs/path/to/effigy/Cargo.toml --bin effigy -- <args...>`
- Do not run mixed long-term channels in one repo (wrapper + pinned binary) after migration cutover.

## 6) Validation Checklist

- `effigy --help` runs in CI after install.
- One core command suite runs via installed binary (`tasks`, `doctor`, `test --plan`).
- Release-gate install validation passes for the pinned tag.
- Wrapper and Effigy task re-export references are removed from `package.json`
  scripts in migrated repos.

## Related Guides

- [`010-path-installation-and-release.md`](./010-path-installation-and-release.md)
- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)

## Next Step

After CI pinning and wrapper migration are stable, complete Homebrew channel workflow work in backlog phase C and reassess optional wrapper need in phase E.

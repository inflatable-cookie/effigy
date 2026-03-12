# 010 - PATH Installation and Release Workflow

This guide defines the recommended local install flow and release checklist for Effigy.

## 1) Local Development Invocation

Use this mode when you want changes to propagate immediately from source:

```bash
cargo run --manifest-path /abs/path/to/effigy/Cargo.toml --bin effigy -- <args...>
```

First-class local dev wrapper in the Effigy repo:

```bash
/abs/path/to/effigy/scripts/effigy-dev <args...>
```

Common wrapper in consumer repos:

```json
{
  "scripts": {
    "effigy": "cargo run --manifest-path ../effigy/Cargo.toml --bin effigy --"
  }
}
```

## 2) PATH-First Invocation (Recommended Daily Use)

Install locally to a controlled root:

```bash
cd /abs/path/to/effigy
cargo install --path . --root ./.local-install --force
```

Link stable/dev entrypoints into `~/.local/bin`:

```bash
./scripts/install-local-bin-links.sh
```

Then run directly:

```bash
effigy tasks --repo /abs/path/to/workspace
effigy-dev tasks --repo /abs/path/to/workspace
effigy doctor --repo /abs/path/to/workspace
```

Shell profile requirement:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Recommended local channel contract:
- `effigy`: stable installed binary, symlinked from `~/.local/bin/effigy`
- `effigy-dev`: source/dev wrapper, symlinked from `~/.local/bin/effigy-dev`

If an old shell alias still defines `effigy`, remove it from your shell rc after
linking the real commands.

## 3) Fallback Strategy

Keep `bun effigy ...` wrapper scripts as compatibility fallback while teams migrate to PATH-first usage.

Recommended policy:
- primary: direct `effigy ...`
- fallback: `bun effigy ...` wrapper (cargo-run)
- migration runbook: [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)

## 4) Versioning

Effigy uses semantic versioning:
- patch: bug fixes and non-breaking behavior improvements,
- minor: backward-compatible feature additions,
- major: breaking command/cfg behavior.

For now, version is controlled in `Cargo.toml`.

## 5) Release Checklist

1. Run release gates in one pass: `effigy release gates`.
   - compatibility fallback: `cargo qa-release`
2. Validate install from the release tag:
   - `./scripts/check-release-install-from-tag.sh --tag v0.__.__`
3. CLI help and core commands run from installed PATH binary.
4. Wrapper fallback still operational in at least one consumer repo.
5. Update roadmap/log docs with validation evidence.
6. Bump `Cargo.toml` version if required.
7. Commit, tag, and push release branch.

## 6) Smoke Matrix

| Mode | Command | Expected |
|---|---|---|
| Source run | `cargo run --manifest-path ../effigy/Cargo.toml --bin effigy -- doctor` | Doctor report rendered, exit 0 |
| Dev wrapper | `effigy-dev doctor --repo <workspace>` | Doctor report rendered from current checkout, exit 0 |
| PATH binary | `effigy --help` | Usage shown, exit 0 |
| PATH binary | `effigy doctor --repo <workspace>` | Doctor report rendered, exit 0 |
| Wrapper fallback | `bun effigy tasks` | Catalogs listed, exit 0 |

## 7) Notes

If cargo lock contention causes delayed startup for wrapper mode, direct PATH invocation avoids the cargo-run lock path.

## Related Guides

- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)

## Next Step

After confirming install mode and smoke matrix, run the release checklist in [`014-release-checklist-template.md`](./014-release-checklist-template.md) for your target version.

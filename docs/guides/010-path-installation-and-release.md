# 010 - PATH Installation and Release Workflow

This guide defines the recommended local install flow and release checklist for Effigy.

## 1) Local Development Invocation

Use this mode when you want changes to propagate immediately from source:

```bash
cargo run --manifest-path /abs/path/to/effigy/Cargo.toml --bin effigy -- <args...>
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

Add to PATH (shell profile):

```bash
export PATH="/abs/path/to/effigy/.local-install/bin:$PATH"
```

Then run directly:

```bash
effigy tasks --repo /abs/path/to/workspace
effigy doctor --repo /abs/path/to/workspace
```

## 3) Fallback Strategy

Keep `bun effigy ...` wrapper scripts as compatibility fallback while teams migrate to PATH-first usage.

Recommended policy:
- primary: direct `effigy ...`
- fallback: `bun effigy ...` wrapper (cargo-run)

## 4) Versioning

Effigy uses semantic versioning:
- patch: bug fixes and non-breaking behavior improvements,
- minor: backward-compatible feature additions,
- major: breaking command/cfg behavior.

For now, version is controlled in `Cargo.toml`.

## 5) Release Checklist

1. `cargo test` passes on default profile.
2. CLI help and core commands run from installed PATH binary.
3. Wrapper fallback still operational in at least one consumer repo.
4. Update roadmap/report docs with validation evidence.
5. Bump `Cargo.toml` version if required.
6. Commit, tag, and push release branch.

## 6) Smoke Matrix

| Mode | Command | Expected |
|---|---|---|
| Source run | `cargo run --manifest-path ../effigy/Cargo.toml --bin effigy -- doctor --repo .` | Doctor report rendered, exit 0 |
| PATH binary | `effigy --help` | Usage shown, exit 0 |
| PATH binary | `effigy doctor --repo <workspace>` | Doctor report rendered, exit 0 |
| Wrapper fallback | `bun effigy tasks` | Catalogs listed, exit 0 |

## 7) Notes

If cargo lock contention causes delayed startup for wrapper mode, direct PATH invocation avoids the cargo-run lock path.

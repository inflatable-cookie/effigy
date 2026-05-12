# 010 - Local PATH Install for Effigy Maintainers

This guide is for people working on Effigy itself.

Use it when you want:

- a stable local `effigy` binary on `PATH`
- a clean daily workflow while iterating on the Effigy repo

This is not the general user install page. For normal user installation, start
at the repo [`README.md`](../../README.md).

## 1) Source-Run Invocation

Use this mode when you want changes to propagate immediately from source:

```bash
cargo run --manifest-path /abs/path/to/effigy/Cargo.toml --bin effigy -- <args...>
```

## 2) PATH-First Invocation (Recommended Daily Use)

Install locally to a controlled root:

```bash
cd /abs/path/to/effigy
cargo install --path . --root ./.local-install --force
```

Link the installed binary into `~/.local/bin`:

```bash
effigy link:local
```

Then run directly:

```bash
effigy tasks --repo /abs/path/to/workspace
effigy doctor --repo /abs/path/to/workspace
```

Shell profile requirement:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Recommended local channel contract:
- `effigy`: locally installed binary, wrapped from `~/.local/bin/effigy`

If an old shell alias still defines `effigy`, remove it from your shell rc after
linking the real commands.

## 3) Daily Rule

Prefer:

- `effigy ...` for the installed binary
- `cargo run --bin effigy -- ...` when you explicitly want the live checkout

Do not treat package-manager wrappers as the normal interface when working on
Effigy itself.

## 4) Smoke Matrix

| Mode | Command | Expected |
|---|---|---|
| Source run | `cargo run --manifest-path ../effigy/Cargo.toml --bin effigy -- doctor` | Doctor report rendered, exit 0 |
| PATH binary | `effigy --help` | Usage shown, exit 0 |
| PATH binary | `effigy doctor --repo <workspace>` | Doctor report rendered, exit 0 |
## 5) Notes

If cargo lock contention causes delayed startup for source-run mode, direct PATH invocation avoids the cargo-run lock path.

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`051-release-orchestration.md`](./051-release-orchestration.md)

## Next Step

After confirming local PATH install works, use the release guides only when you
are actually cutting a release.

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

Build the optimized local binary, install it under the checkout, and maintain
the launcher on `PATH`:

```bash
cd /abs/path/to/effigy
effigy install:local
```

`install:local` builds the **release** profile once through Oxide
(`oxide build --release --bin effigy`) and activates the exact release binary
Oxide uplifts to `target/release/effigy`. It never falls back to another profile
or to plain Cargo: a missing release output fails and names the escape. On macOS
the activated copy is also signed with Effigy's one stable local identity (see
[Signing on macOS](#signing-on-macos)). Without Oxide on `PATH`, run the one
escape, which produces the same path:

```bash
EFFIGY_OXIDE_ESCAPE=cargo effigy install:local
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

### What gets written

In the checkout:

- `.local-install/bin/effigy` — the active optimized binary
- `.local-install/bin/effigy.active-version` — its `v<version>+local.<sha>`
  build identity (`.dirty` when the worktree has changes)
- `.local-install/bin/effigy.previous` and `.previous.version` — the
  owner-only rollback channel, refreshed on each activation

The active path is staged (`effigy.new`), byte-compared with the release
artifact, signed with the local identity on macOS, marked executable, and only
then renamed over the active binary. The version receipt is published by rename
too, so a failed build, copy, signature or rename leaves the previously active
binary and receipt intact instead of truncating them. A non-regular active path
(for example a symlink) is refused before anything is copied.

### Signing on macOS

`install:local` signs the binary it activates with one local code-signing
identity, `Effigy Local Signing`, so consecutive rebuilds keep the same code
identity. Without it macOS treats each rebuild as a new program, and a
Gatekeeper verification panel comes back with every build.

The identity is Effigy's own, kept in your default (login) keychain and shared
by every checkout on the machine:

- an identity named `Effigy Local Signing` that the keychain already offers for
  code signing is reused;
- otherwise the first `install:local` on the machine provisions it once: a
  self-signed code-signing certificate and its private key made with
  `/usr/bin/openssl`, imported with
  `security import -T /usr/bin/codesign` so codesign reads the key without a
  keychain prompt. The certificate and key are generated in an owner-only
  temporary directory that is removed straight after the import; only the two
  keychain items stay, and the certificate is valid for ten years.

No step uses `sudo`, changes system trust settings or asks Apple for anything.
This identity makes a local build recognisable as the same local build; it is
not a distribution identity, it is not notarized, and release artifacts are
signed by the release pipeline instead.

Two variables override the policy:

| Variable | Effect |
|---|---|
| `EFFIGY_LOCAL_CODESIGN_IDENTITY` | Sign with this identity (common name or SHA-1 fingerprint); nothing is discovered or provisioned. |
| `EFFIGY_LOCAL_CODESIGN=off` | Skip signing for this build. |

A refused signature fails `install:local` before activation: the active binary,
its version receipt, the previous-binary rollback copy and your launcher stay
exactly as they were, and the staged copy is removed.

If the automatic provisioning cannot run on your machine, create the identity by
hand once: Keychain Access → Certificate Assistant → Create a Certificate, name
`Effigy Local Signing`, Identity Type `Self Signed Root`, Certificate Type
`Code Signing`. The next `install:local` reuses it. Trusting that certificate
(Keychain Access → the certificate → Trust → Always Trust) changes how macOS
verifies a signature, not which identity signs it.

A non-macOS host is unchanged: nothing is selected, provisioned or signed.

### What happens to your launcher

A fresh install writes the maintained wrapper at `~/.local/bin/effigy` (or
`$EFFIGY_LOCAL_BIN_DIR/effigy`) pointing at this checkout's stable binary.

If that path already holds something `install:local` did not generate, it is
**preserved byte-for-byte** and reported: a customized launcher script and a
custom symlink both stay exactly as they are, and the task never executes them.
To adopt the maintained wrapper, move your launcher aside and re-run
`effigy install:local`.

The only file that is refreshed is a regular file that positively matches the
maintained wrapper format for a recognised stable target
(`<checkout>/.local-install/bin/effigy`); that refresh writes a staged file and
renames it over the old one. Unsupported entries (a directory, for example) are
refused without being touched.

If an old shell alias still defines `effigy`, remove it from your shell rc after
linking the real commands.

## 3) Daily Rule

Prefer:

- `effigy ...` for the installed binary
- `effigy install:local` after source changes you want on `PATH`
- `cargo run --bin effigy -- ...` when you explicitly want the live checkout

Do not treat package-manager wrappers as the normal interface when working on
Effigy itself.

## 4) Smoke Matrix

| Mode | Command | Expected |
|---|---|---|
| Source run | `cargo run --manifest-path ../effigy/Cargo.toml --bin effigy -- doctor` | Doctor report rendered, exit 0 |
| Local install | `effigy install:local` | Optimized release binary activated, version receipt written, local code signature applied on macOS |
| PATH binary | `effigy --help` | Usage shown, exit 0 |
| PATH binary | `effigy doctor --repo <workspace>` | Doctor report rendered, exit 0 |

## 5) Notes

If cargo lock contention causes delayed startup for source-run mode, direct PATH
invocation avoids the cargo-run lock path.

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](049-ci-binary-distribution-and-release-protocol.md)
- [`051-release-orchestration.md`](051-release-orchestration.md)

## Next Step

After confirming local PATH install works, use the release guides only when you
are actually cutting a release.

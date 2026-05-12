# g05.003 - Local Encrypted Vault

Status: Complete
Depends on: `g05.002`

## Goal

Add Effigy's built-in local encrypted vault for developer secrets, with an
explicit human-gated unlock model.

## Scope

- Add:

```sh
effigy secrets init
effigy secrets set <name>
effigy secrets unset <name>
effigy secrets unlock
effigy secrets lock
```

- Store vault files under the configured `[secrets.vault].path`.
- Use standard authenticated encryption and memory-hard passphrase derivation.
- Support the first unlock policies:
  - `passphrase`
  - `key-and-passphrase`
- Require interactive operator input for unlock and set operations.
- Store only declared secret keys by default.
- Reject setting undeclared keys unless an explicit future policy allows it.
- Keep an in-process unlock cache only for the current Effigy invocation unless
  a later roadmap deliberately adds a local agent.
- Redact all secret values in command output and JSON reports.

## Safety Boundary

The MVP must not rely on SSH-agent access alone.

`key-and-passphrase` may use an SSH key identity as one factor, but a human
passphrase is still required so an agent with filesystem and SSH-agent access
cannot silently decrypt the vault.

## Non-Goals

- No long-running secret daemon.
- No team recipient management.
- No cloud secret sync.
- No provider-hosted secret provisioning.
- No automatic production secret migration.
- No `.env` export except where added by a later compatibility roadmap.

## Acceptance Criteria

- [x] A developer can initialize a vault and set declared secrets.
- [x] `secrets doctor` can distinguish:
  - no vault
  - locked vault
  - unlocked vault
  - missing required values
  - undeclared stored values
- [x] Unlock requires explicit operator participation.
- [x] Secret values never appear in normal output, JSON output, debug formatting,
  or error messages.
- [x] Corrupt vault files fail closed with clear diagnostics.
- [x] File permissions are checked and warned or blocked when unsafe.

## Outcome

Effigy now has a built-in local encrypted vault backed by `effigy-secrets`.
The vault uses Argon2id key derivation, XChaCha20-Poly1305 authenticated
encryption, OS-random salt/nonce material, declared-key enforcement, hidden
operator input, safe Unix file permissions, and value-free text/JSON output.

No runtime injection, container startup injection, provider provisioning,
`.env` export, daemon, or cross-invocation unlock cache was added in this
roadmap.

## Test Strategy

- Vault round-trip tests with test passphrases.
- Wrong-passphrase tests.
- Corrupt-file tests.
- Redaction tests for text, JSON, and debug output.
- Manifest-declared key enforcement tests.
- File permission diagnostics tests where platform support allows.

## Next Task

Wire unlocked secrets into task, Rhai, and deploy execution in `g05.004`.

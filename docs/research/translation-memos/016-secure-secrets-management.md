# Translation Memo 016: Secure Secrets Management

**Status:** Draft  
**Track:** 16 - Secure Secrets Management  
**Tools:** Varlock, Mozilla SOPS, git-crypt, age, Doppler  
**Date:** 2026-03-07  
**Related:** Translation Memo 010 (Environment Management)

## Executive Summary

This memo translates Track 16 research into implementation guidance for Effigy's secrets management. The key insight: **Effigy should provide secure, multi-backend secrets management with a default of age-based encryption for simplicity, while supporting external providers (1Password, Bitwarden) for teams already using them.**

## Research Summary

### Varlock
- **Strengths**: Schema validation, external provider resolution, redaction
- **Weaknesses**: JS-centric, new/unproven, doesn't encrypt (yet)
- **Pattern**: .env.schema with @decorators

### Mozilla SOPS
- **Strengths**: Industry standard, encrypts values not keys, multiple backends
- **Weaknesses**: Complex key management, cloud KMS dependency for teams
- **Pattern**: Encrypted YAML/JSON with Git-friendly diffs

### git-crypt
- **Strengths**: Transparent git workflow, GPG support
- **Weaknesses**: GPG complexity, maintenance mode, poor user management
- **Pattern**: Git filters for transparent encryption

### age
- **Strengths**: Modern crypto (X25519), simple, no configuration, SSH-compatible
- **Weaknesses**: No signing, manual key distribution
- **Pattern**: File encryption with minimal UX

### Doppler
- **Strengths**: Centralized, good UX, real-time sync, access control
- **Weaknesses**: Cloud dependency, subscription cost, vendor lock-in
- **Pattern**: Cloud vault with CLI injection

## Core Principles

### 1. Security Without Complexity

Good security should be easy. age demonstrates this: one command to encrypt, one to decrypt. No keyrings, no trust models, no configuration.

### 2. Multi-Backend Flexibility

Different teams have different needs:
- Solo developers: Symmetric age key
- Small teams: age with key sharing
- Enterprises: SOPS with KMS
- Existing password manager users: 1Password/Bitwarden integration

### 3. Offline-First

Development should work without internet. Cloud-only solutions (Doppler) break this.

### 4. Git-Friendly

Secrets in git should:
- Show what's changed (SOPS-style value encryption)
- Work with normal git workflows
- Not require special server-side hooks

## Proposed Implementation

### Phase 1: age-Based Encryption (Core)

**Default approach for most users.**

```bash
# Initialize project secrets
effigy secrets init
# Creates:
#   .effigy-key (git-ignored) - private key
#   .effigy-key.pub (committed) - public key for team
```

```toml
# effigy.toml
[secrets]
backend = "age"
# Auto-detects .effigy-key.pub

[[task]]
name = "start"
env = { from = "secrets.env.age" }
```

```bash
# Encrypt existing .env file
effigy secrets encrypt .env
# Creates secrets.env.age
# Original .env added to .gitignore

# Run task with decrypted secrets
effigy run start
# Auto-decrypts secrets.env.age
```

**Team sharing:**
```bash
# Add team member's public key
effigy secrets add-key age1ql3z7hjy54pw3... alice@example.com
# Re-encrypts secrets for all keys

# List authorized keys
effigy secrets list-keys
```

### Phase 2: Schema Validation (Optional)

Inspired by Varlock, but in TOML:

```toml
# effigy.toml
[secrets]
backend = "age"

[secrets.validation]
enabled = true
# Validates on load, warns if invalid

[secrets.validation.DATABASE_URL]
required = true
type = "url"
schemes = ["postgresql", "mysql"]
sensitive = true  # Redact in output

[secrets.validation.LOG_LEVEL]
type = "enum"
values = ["debug", "info", "warn", "error"]
default = "info"

[secrets.validation.PORT]
type = "number"
min = 1
max = 65535
default = 3000

[secrets.validation.API_KEY]
required = true
type = "string"
pattern = "^sk-[a-zA-Z0-9]{48}$"
sensitive = true
```

**Benefits:**
- Catch configuration errors early
- Type safety for environment variables
- Clear error messages

### Phase 3: External Provider Integration

For teams using password managers:

```toml
# effigy.toml
[secrets]
backend = "external"
provider = "1password"  # or "bitwarden", "doppler"

[env]
# Resolved at runtime from external provider
STRIPE_KEY = { from = "1password", vault = "Production", item = "Stripe", field = "live_key" }
DATABASE_URL = { from = "bitwarden", project = "api", secret = "database-url" }
```

```bash
# Requires user to be logged into 1Password
effigy run start
# Resolves secrets from 1Password, injects into task
```

**Benefits:**
- No secrets in repository at all
- Centralized rotation in password manager
- Works with existing team workflows

### Phase 4: SOPS Integration (Enterprise)

For teams already using SOPS:

```toml
# effigy.toml
[secrets]
backend = "sops"
config = ".sops.yaml"  # Standard SOPS config

[[task]]
name = "deploy"
env = { from_sops = "secrets.prod.yaml" }
```

```bash
# Use existing SOPS setup
effigy run deploy
# Decrypts with SOPS, runs task
```

**Benefits:**
- Migration path for SOPS users
- Enterprise KMS integration
- No change to existing workflows

### Phase 5: Transparent Mode (Advanced)

Optional git-crypt style transparency:

```bash
# Initialize transparent encryption
effigy secrets transparent init

# Mark files for encryption
echo "*.secret.env filter=effigy-crypt" >> .gitattributes
echo "secrets/** filter=effigy-crypt" >> .gitattributes

# Install git hook
effigy secrets transparent install-hook
```

```bash
# Normal git workflow
git add config.secret.env
git commit -m "Update config"
# File automatically encrypted
```

**Benefits:**
- Invisible to users
- Works with any git tool

**Downsides:**
- Requires git hooks
- Harder to debug

## Implementation Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Effigy Secrets                            │
├─────────────────────────────────────────────────────────────┤
│  Interface Layer                                            │
│  ├── effigy secrets init                                    │
│  ├── effigy secrets encrypt <file>                          │
│  ├── effigy secrets decrypt <file>                          │
│  ├── effigy secrets add-key <key>                           │
│  └── effigy secrets validate                                │
├─────────────────────────────────────────────────────────────┤
│  Backend Layer (pluggable)                                  │
│  ├── age (default) - Modern encryption                      │
│  ├── sops - Enterprise/KMS                                  │
│  ├── external - 1Password, Bitwarden, Doppler              │
│  └── none - Validation only                                 │
├─────────────────────────────────────────────────────────────┤
│  Features                                                   │
│  ├── Schema validation                                      │
│  ├── Secret redaction in output                             │
│  ├── Team key management                                    │
│  └── CI/CD integration                                      │
└─────────────────────────────────────────────────────────────┘
```

## Migration Paths

### From .env files

```bash
# One-time migration
effigy secrets migrate --from .env --to .env.age
# Encrypts .env → .env.age
# Adds .env to .gitignore
# Commits .env.age
```

### From SOPS

```bash
# Use existing SOPS files
effigy secrets backend sops
# Or gradually migrate
effigy secrets migrate --from sops --to age
```

### From Doppler/1Password

```toml
# Keep using external provider
[secrets]
backend = "external"
provider = "doppler"  # or "1password"

# Or import to local encryption
effigy secrets import --from doppler
```

## Security Considerations

### Key Storage

| Location | Use Case | Risk |
|----------|----------|------|
| `.effigy-key` (git-ignored) | Local dev | Low (file perms) |
| Password manager | Backup/sharing | Low (encrypted vault) |
| CI environment variable | CI/CD | Medium (env exposure) |
| Hardware token | High security | Low (physical access) |

### CI/CD Integration

```bash
# GitHub Actions example
- name: Run task
  env:
    EFFIGY_AGE_KEY: ${{ secrets.AGE_PRIVATE_KEY }}
  run: effigy run deploy
```

Best practices:
- Rotate CI keys regularly
- Use separate keys per environment
- Limit key access in CI

### Secret Rotation

```bash
# Rotate a secret
effigy secrets rotate DATABASE_URL
# 1. Decrypt secrets file
# 2. Prompt for new value
# 3. Re-encrypt with new value
# 4. Commit

# Or bulk rotate
effigy secrets rotate --all
```

## Comparison: Effigy vs. Alternatives

| Feature | Effigy (proposed) | SOPS | git-crypt | Doppler |
|---------|-------------------|------|-----------|---------|
| Encryption | ✅ (age) | ✅ | ✅ | ✅ (cloud) |
| Validation | ✅ | ❌ | ❌ | ❌ |
| Self-hosted | ✅ | ✅ | ✅ | ❌ |
| Offline | ✅ | ✅ | ✅ | ❌ |
| Team sharing | ✅ | ⚠️ (complex) | ⚠️ (GPG) | ✅ |
| CI/CD | ✅ | ⚠️ | ⚠️ | ✅ |
| Cost | Free | Free | Free | $7/user/mo |
| Learning curve | Low | Medium | High | Low |

## Implementation Phases

| Phase | Features | Timeline |
|-------|----------|----------|
| 1 | age encryption, basic CLI | MVP |
| 2 | Schema validation | +2 weeks |
| 3 | External providers | +2 weeks |
| 4 | SOPS integration | +1 week |
| 5 | Transparent mode | +2 weeks |
| 6 | Advanced key management | +3 weeks |

## Open Questions

1. Should we bundle age binary or require external install?
2. How to handle key recovery when team member leaves?
3. Should we support hardware tokens (YubiKey) directly?
4. What's the story for secret versioning/history?

## Success Criteria

- Encrypt/decrypt in < 100ms for typical .env files
- One-command setup (`effigy secrets init`)
- Clear error messages for validation failures
- Works offline
- No vendor lock-in (exportable)

## Related Concepts

- Concept: Multi-Backend Secrets
- Concept: Schema Validation
- Concept: Transparent Encryption
- Concept: External Provider Integration

---

**Track 16 research and recommendations complete.**

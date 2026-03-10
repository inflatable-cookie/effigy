# Track 16: Secure Secrets Management

Status: Draft
Value track: Secure Secrets Management (Varlock, SOPS, git-crypt, age, Doppler)
Created: 2026-03-07
Tools covered: Varlock, Mozilla SOPS, git-crypt, age, Doppler, Bitwarden Secrets Manager

## 1) Synthesis

### Common Patterns

| Pattern | Varlock | SOPS | git-crypt | age | Doppler | Best For |
|---------|---------|------|-----------|-----|---------|----------|
| Storage | .env.schema files | Encrypted YAML/JSON | Git filters | Encrypted files | Cloud vault | Different workflows |
| Encryption | Coming soon | Values only | Whole file | Whole file | At-rest in cloud | Value visibility vs security |
| Key mgmt | External providers | KMS/PGP/age | GPG/Symmetric | Age keys | Doppler-hosted | Team size/existing infra |
| Transparency | Load-time | Manual decrypt | Git transparent | Manual | CLI injection | Dev experience |
| Offline | Yes | Yes | Yes | Yes | No | Connectivity requirements |
| Cost | Free | Free | Free | Free | $7/user/mo | Budget constraints |

### Key Insights

**Three architectural approaches:**

| Approach | Examples | Pros | Cons | Best For |
|----------|----------|------|------|----------|
| **File-based encryption** | SOPS, git-crypt, age | Git-friendly, offline, no vendor | Key management overhead | Teams comfortable with crypto |
| **Cloud secret managers** | Doppler, Bitwarden | Centralized, access control, audit | Subscription cost, offline issues | Teams needing enterprise features |
| **Schema + resolution** | Varlock | Validation, IDE support, extensible | New, JS-centric | Modern JS/TS projects |

**The .env problem:**

Plain .env files have issues:
- No validation (typos caught at runtime)
- No type safety
- Secrets in plain text
- No audit trail
- Sprawl across environments

Solutions address different aspects:
- **Validation**: Varlock schema (catches errors early)
- **Encryption**: SOPS, git-crypt, age (protects secrets at rest)
- **Centralization**: Doppler (single source of truth)
- **Transparency**: git-crypt (invisible to users)

**Key management is the hard part:**

Every tool struggles with key distribution:
- GPG: Complex web of trust
- KMS: Cloud IAM complexity
- Age: Manual key sharing
- Symmetric: Password distribution

No solution is perfect; tradeoffs exist.

### What Works

**SOPS patterns:**
- Encrypt values, keep keys readable (git diffs)
- Multiple backends (flexibility)
- Cloud KMS integration (enterprise)

**git-crypt patterns:**
- Transparent git workflow
- GPG for multi-user
- Symmetric for simple cases

**age patterns:**
- Minimalist design
- Modern cryptography
- SSH key compatibility

**Doppler patterns:**
- CLI injection (no files on disk)
- Environment separation
- Real-time sync

**Varlock patterns:**
- Schema validation
- External provider resolution
- Redaction of sensitive output

### What Doesn't

**Anti-patterns:**
- Requiring GPG (too complex for many)
- Cloud-only (offline development breaks)
- File-only (no direct env var support)
- No rotation story (key rotation is painful)

**Pain points:**
- Key distribution across team
- Secret rotation workflows
- CI/CD integration complexity
- Migration from existing .env files

## 2) Cross-Tool Capabilities Matrix

| Capability | Varlock | SOPS | git-crypt | age | Doppler | Effigy Should |
|------------|---------|------|-----------|-----|---------|---------------|
| **Encryption** | Planned | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Validation** | ✅ | ❌ | ❌ | ❌ | ❌ | Optional |
| **Git-friendly** | ✅ | ✅ (diffs) | ✅ (transparent) | ❌ | N/A | ✅ |
| **Offline** | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| **Self-hosted** | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| **No vendor lock** | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| **Team sharing** | External | KMS/GPG | GPG/Symmetric | Key share | ✅ Built-in | Flexible |
| **Cost** | Free | Free | Free | Free | $ | Free |

## 3) Secrets Management Patterns

### Pattern 1: Local file encryption (SOPS-style)

```yaml
# secrets.env.yaml (encrypted)
DATABASE_URL: ENC[AES256_GCM,data:...,iv:...]
API_KEY: ENC[AES256_GCM,data:...,iv:...]
```

```bash
# Decrypt on load
effigy run --secrets secrets.env.yaml -- task
```

Pros: Git-friendly, offline, no vendor
Cons: Key management

### Pattern 2: Transparent git encryption (git-crypt style)

```bash
# .gitattributes
*.secret.env filter=effigy-crypt

# Transparent encrypt/decrypt
git add config.secret.env  # Auto-encrypted
```

Pros: Invisible to users
Cons: Complex key management

### Pattern 3: Age-based encryption

```toml
# effigy.toml
[secrets]
backend = "age"
key = ".effigy-key"
```

```bash
# Encrypt
effigy secrets encrypt secrets.env

# Decrypt on use
effigy run -- task
```

Pros: Modern crypto, simple
Cons: Manual key sharing

### Pattern 4: Schema validation (Varlock-style)

```toml
# effigy.toml
[env.schema.DATABASE_URL]
required = true
type = "url"
scheme = "postgresql"
sensitive = true

[env.schema.API_KEY]
required = true
type = "string"
pattern = "^sk-[a-zA-Z0-9]+$"
```

Pros: Catch errors early
Cons: Doesnt encrypt

### Pattern 5: External provider resolution

```toml
# effigy.toml
[env]
DATABASE_URL = { from = "1password", path = "prod/db/url" }
API_KEY = { from = "bitwarden", path = "api/key" }
```

Pros: No secrets in repo
Cons: Requires external tool

## 4) Decision Framework

| If you need... | Consider... |
|----------------|-------------|
| Enterprise audit/compliance | SOPS + KMS, Doppler |
| Simple team sharing | age, SOPS + age |
| Existing GPG infrastructure | git-crypt, SOPS + PGP |
| Validation without encryption | Varlock pattern |
| Zero secrets in repo | Doppler, external providers |
| Offline-first | SOPS, age, git-crypt |
| Free, open source | SOPS, age, git-crypt |
| Minimal setup | age (symmetric), Varlock |

## 5: Gaps and Opportunities

### Gaps in current tools

1. **No integrated validation+encryption**: Varlock validates but doesn't encrypt (yet); SOPS encrypts but doesn't validate
2. **Key management is hard**: All tools struggle with secure key distribution
3. **CI/CD friction**: Decrypting in CI requires keys, creating chicken-and-egg
4. **Migration from .env**: No smooth path from existing .env files
5. **No secret rotation**: Tools don't help with rotation workflows

### Opportunities for Effigy

1. **Hybrid approach**: Validation + encryption together
2. **Simplified key management**: Project keys, team sharing via HTTPS/TLS
3. **Built-in support**: First-class secrets, not plugin
4. **Multiple backends**: age, SOPS, external providers
5. **Smooth migration**: Import from .env, export to other tools

## 6: Recommendations for Effigy

### Core Principle

> Effigy should provide secure secrets management that's both easy to use and doesn't lock users into a specific vendor. Support multiple backends and allow migration.

### Specific Recommendations

**1. Multi-Backend Architecture**

```toml
# effigy.toml - choose your backend
[secrets]
backend = "age"  # or "sops", "external", "none"

# Backend-specific config
[secrets.age]
public_key = "age1..."

[secrets.sops]
config = ".sops.yaml"

[secrets.external]
provider = "1password"  # or "bitwarden", "doppler"
```

**2. Default: age-based encryption**

```bash
# Initialize project secrets
effigy secrets init
# Creates .effigy-key (git-ignored)
# Creates .effigy-key.pub (committed for team)

# Encrypt file
effigy secrets encrypt .env
# Creates .env.age

# Use in task
effigy run --secrets .env.age -- task
```

Why age?
- Modern cryptography
- Simple, no configuration
- Good UX
- Fast

**3. Schema validation (optional)**

```toml
# effigy.toml
[secrets.validation]
enabled = true

[secrets.validation.DATABASE_URL]
required = true
type = "url"
sensitive = true

[secrets.validation.LOG_LEVEL]
type = "enum"
values = ["debug", "info", "warn", "error"]
default = "info"
```

**4. External provider integration**

```toml
# For teams already using password managers
[env]
STRIPE_KEY = { from = "1password", path = "api/stripe/live" }
```

**5. Transparent mode (optional)**

```bash
# Similar to git-crypt
effigy secrets transparent init
effigy secrets transparent add *.env
# Auto-encrypt on commit via git hook
```

**6. CI/CD integration**

```bash
# Pass key via environment (CI secret)
export EFFIGY_AGE_KEY="AGE-SECRET-KEY-1..."
effigy run -- task
```

## 7: Open Questions

- Should Effigy bundle age or use it as external dependency?
- How to handle key backup and recovery?
- Should we support SOPS for existing users?
- What's the story for secret rotation?
- How to integrate with existing password managers?

## 8: Next Steps

1. Prototype age-based encryption
2. Design schema validation system
3. Implement external provider interface
4. Create migration tools from .env
5. Document key management best practices

---

**Research for Track 16 complete.**

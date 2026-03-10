# Translation Memo 016b: External Provider Secrets Integration

**Status:** Draft  
**Track:** 16 - Secure Secrets Management (Refined)  
**Focus:** External Provider Integration (1Password, Bitwarden, Infisical)  
**Date:** 2026-03-07  
**Related:** Translation Memo 016 (Original secrets research)

## Executive Summary

This memo refines the Track 16 recommendations to focus on **external provider integration** rather than git-committable encryption. The rationale:

1. **Most developers already use password managers** (1Password, Bitwarden)
2. **Key management is eliminated** - leverage existing infrastructure
3. **Secret rotation is centralized** in the provider
4. **Team onboarding is simpler** - grant vault access, not encryption keys
5. **No key copies needed** - reference secrets directly by URI/path

## Research Summary: External Providers

### 1Password CLI

**Strengths:**
- Mature, widely adopted
- `op read` for direct secret retrieval
- `op run` for environment injection
- Secret references: `op://vault/item/field`
- Biometric unlock ( Touch ID, Face ID)
- Service accounts for CI/CD

**Usage pattern:**
```bash
# Direct read
export DB_URL=$(op read op://prod/db/connection-string)

# Environment injection
op run --env-file=.env.op -- npm start

# Service account for CI
export OP_SERVICE_ACCOUNT_TOKEN="..."
op read op://prod/api/key
```

**Pricing:** Paid (subscription)

### Bitwarden Secrets Manager

**Strengths:**
- Separate from password manager (machine-focused)
- `bws run` for environment injection
- Machine accounts with granular access
- Open source (GitHub)
- Free tier available

**Usage pattern:**
```bash
# Set access token
export BWS_ACCESS_TOKEN="..."

# Run with secrets
bws run -- npm start

# Get specific secret
export API_KEY=$(bws secret get $SECRET_ID | jq -r '.value')
```

**Pricing:** Free tier, paid plans for teams

### Infisical

**Strengths:**
- Open source (MIT license)
- Self-hostable
- Developer-focused UI
- `infisical run` command
- Secret scanning/rotation
- Kubernetes operator

**Usage pattern:**
```bash
# Login
infisical login

# Run with secrets from project
infisical run --projectId=xxx --env=dev -- npm start
```

**Pricing:** Open source free, paid cloud option

## Common Patterns Across Providers

| Feature | 1Password | Bitwarden SM | Infisical |
|---------|-----------|--------------|-----------|
| **CLI command** | `op run` | `bws run` | `infisical run` |
| **Direct read** | `op read` | `bws secret get` | API/CLI |
| **Auth method** | Biometric/token | Access token | Token/login |
| **Secret path** | `op://v/i/f` | Project + key | Project + path |
| **Self-hosted** | ❌ | ❌ | ✅ |
| **Open source** | ❌ | Partial | ✅ |
| **Pricing** | $ | Free tier | Free/oss |

## Recommended Implementation for Effigy

### Core Principle

> **Reference secrets, don't store them.** Use the provider's CLI/SDK to fetch secrets at runtime by URI/path. No keys, no encryption, no copies.

### Proposed Configuration

```toml
# effigy.toml - External provider secrets

[secrets]
provider = "1password"  # or "bitwarden", "infisical", "env"

# Optional: Default vault/project
[secrets.defaults]
1password_vault = "Development"
bitwarden_project = "api-project"
infisical_project = "my-app"

# Environment variable definitions
[env]
# 1Password style
DATABASE_URL = { from = "1password", path = "op://Development/Database/connection" }
STRIPE_KEY = { from = "1password", path = "op://Production/Stripe/live_key" }

# Bitwarden style
API_SECRET = { from = "bitwarden", key = "api-secret-key" }

# Infisical style
REDIS_URL = { from = "infisical", path = "/dev/redis/url" }

# Fallback to regular value (for non-sensitive)
LOG_LEVEL = "debug"
```

### CLI Integration

```bash
# Check provider status
effigy secrets status
# Provider: 1password
# Status: ✓ Authenticated (user@example.com)
# Default vault: Development

# Verify secrets are accessible
effigy secrets verify
# ✓ DATABASE_URL - readable
# ✓ STRIPE_KEY - readable
# ✗ API_SECRET - not found (check Bitwarden)

# Run task with secrets
effigy run start
# Fetches secrets from provider, injects, runs task

# Export secrets for external use (with warning)
effigy secrets export --format env
# WARNING: Exporting secrets to stdout
# DATABASE_URL=postgresql://...
```

### Provider Abstraction Layer

```rust
// Internal Effigy trait for secret providers

pub trait SecretProvider {
    fn name(&self) -> &str;
    
    // Check if authenticated
    fn is_authenticated(&self) -> bool;
    
    // Fetch single secret
    fn get(&self, path: &str) -> Result<String>;
    
    // Fetch multiple secrets (batch for performance)
    fn get_many(&self, paths: &[&str]) -> Result<HashMap<String, String>>;
    
    // Run command with env vars
    fn run(&self, env_vars: &HashMap<String, String>, cmd: &[&str]) -> Result<()>;
}

// Implementations
pub struct OnePasswordProvider;
pub struct BitwardenProvider;
pub struct InfisicalProvider;
pub struct EnvProvider; // Fallback to .env
```

### Graceful Degradation

```toml
# effigy.toml - Multiple provider fallback

[env.DATABASE_URL]
primary = { from = "1password", path = "op://Dev/Postgres/url" }
fallback = { from = "env", var = "DATABASE_URL_FALLBACK" }
local = { from = "file", path = ".env.local" }
```

Order of resolution:
1. Try primary provider
2. Try fallback provider
3. Try local file
4. Error with helpful message

### Local Development Workflow

```bash
# 1. Developer has 1Password installed and unlocked
op signin

# 2. effigy.toml references secrets by path
# No local .env file needed!

# 3. Run task - secrets fetched on demand
effigy run dev
# [effigy] Resolving 3 secrets from 1Password...
# [effigy] ✓ All secrets resolved
# [dev] Starting server...

# 4. Secrets never touch disk
```

### CI/CD Integration

```yaml
# GitHub Actions with 1Password
- name: Run tests
  env:
    OP_SERVICE_ACCOUNT_TOKEN: ${{ secrets.OP_SERVICE_ACCOUNT_TOKEN }}
  run: effigy run test
```

```yaml
# GitHub Actions with Bitwarden
- name: Run tests
  env:
    BWS_ACCESS_TOKEN: ${{ secrets.BWS_ACCESS_TOKEN }}
  run: effigy run test
```

```yaml
# Self-hosted with Infisical
- name: Run tests
  env:
    INFISICAL_TOKEN: ${{ secrets.INFISICAL_TOKEN }}
  run: effigy run test
```

## Comparison: External vs. File Encryption

| Aspect | External Provider | File Encryption (SOPS/age) |
|--------|-------------------|---------------------------|
| **Setup** | Install provider CLI | Generate keys, distribute |
| **Key management** | Provider handles | You handle |
| **Team onboarding** | Grant vault access | Share encryption keys |
| **Secret rotation** | Centralized in provider | Re-encrypt files |
| **Offline use** | ❌ Requires connectivity | ✅ Works offline |
| **Audit trail** | ✅ Built-in | ❌ Git history only |
| **Cost** | Subscription (mostly) | Free |
| **Lock-in** | Provider-dependent | Open standards |
| **CI/CD** | Token-based | Key-based |

## Recommendation

### Primary: External Provider Integration

**Default to 1Password** (most popular, best UX) with support for:
- 1Password (primary recommendation)
- Bitwarden Secrets Manager (free option)
- Infisical (open source/self-hosted)
- Environment fallback (.env for local overrides)

### Secondary: Optional File Encryption

For teams that need offline capability or want to avoid subscriptions:
- Age-based encryption as optional add-on
- Not the primary path

## Implementation Phases

| Phase | Work | Timeline |
|-------|------|----------|
| 1 | 1Password integration | 1 week |
| 2 | Bitwarden SM integration | 3 days |
| 3 | Infisical integration | 3 days |
| 4 | Multi-provider config | 3 days |
| 5 | Documentation & examples | 2 days |

## Configuration Examples

### 1Password Team

```toml
# effigy.toml
[secrets]
provider = "1password"

[env]
# References secrets in 1Password vaults
DATABASE_URL = { from = "1password", path = "op://Development/Postgres/connection" }
REDIS_URL = { from = "1password", path = "op://Development/Redis/url" }
STRIPE_KEY = { from = "1password", path = "op://Production/Stripe/live_key" }
```

### Bitwarden (Free Option)

```toml
# effigy.toml
[secrets]
provider = "bitwarden"
project = "my-project-id"

[env]
DATABASE_URL = { from = "bitwarden", key = "db-url" }
API_KEY = { from = "bitwarden", key = "api-key" }
```

### Infisical (Self-Hosted)

```toml
# effigy.toml
[secrets]
provider = "infisical"
project = "my-app"
environment = "dev"

[env]
DATABASE_URL = { from = "infisical", path = "/db/url" }
```

### Mixed (Migration Scenario)

```toml
# effigy.toml - Gradual migration
[secrets]
provider = "1password"  # Primary

[env]
# Using 1Password
DATABASE_URL = { from = "1password", path = "op://Dev/Database/url" }

# Still using .env during migration
LEGACY_TOKEN = { from = "env", file = ".env.legacy" }
```

## Security Considerations

### Provider Token Storage

| Environment | Storage | Risk |
|-------------|---------|------|
| Local dev | Provider CLI session (keychain) | Low |
| CI/CD | Repository secret | Medium (scope carefully) |
| Production | Provider SDK with IAM | Low |

### Secret Access Patterns

```bash
# Good: Fetch on demand, don't persist
effigy run start

# Bad: Export to file
effigy secrets export > .env  # Don't do this

# Good: Verify access without exposing
effigy secrets verify
```

### Audit and Compliance

- All access logged by provider
- No copies of secrets in Effigy
- Clear audit trail in 1Password/Bitwarden/Infisical

## Open Questions

1. Should Effigy cache secrets briefly (e.g., 1 minute) to avoid API rate limits?
2. How to handle provider outages - fail fast or use stale cache?
3. Should we support secret versioning (fetch specific version)?
4. What's the UX for switching providers mid-project?

## Success Criteria

- [ ] 1Password integration works with `op run`-like UX
- [ ] Bitwarden SM integration works
- [ ] Infisical integration works
- [ ] Clear error messages when secrets not found
- [ ] No secrets written to disk by default
- [ ] CI/CD integration documented
- [ ] Migration guide from .env files

## Related Concepts

- Concept: External Secret Provider
- Concept: Secret References (op://, bws://, inf://)
- Concept: Graceful Degradation
- Concept: Provider Abstraction

---

**Refined recommendation: External provider integration as primary, file encryption as optional fallback.**

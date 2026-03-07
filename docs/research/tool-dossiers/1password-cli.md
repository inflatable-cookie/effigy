# 1Password CLI (op)

Status: Draft
Tool name: 1Password CLI (op)
Category: secret management (secure credential injection)
Owner:
Last updated: 2026-03-07
Scope: 1Password CLI 2.x documentation, secret injection patterns, service accounts

## 1) Why this tool matters

1Password CLI (the `op` command) is the command-line interface to 1Password. It enables:
- Secure secret injection into scripts
- Service account automation
- Biometric unlock
- No plaintext secrets in files

For Effigy, 1Password CLI represents:
- Modern secret management patterns
- Secure credential injection
- Developer-friendly security
- Secret rotation strategies

## 2) Product and era context

### Timeline

- **2019**: 1Password CLI 1.0 released
- **2020-2022**: Feature expansion, biometric unlock
- **2023-2024**: CLI 2.0, service accounts, improved UX

### Design Philosophy

From 1Password documentation:

> "Securely automate workflows"
> "Biometric unlock"
> "Service accounts for CI/CD"
> "Secrets where you need them"

### Target Audience

- Developers managing application secrets
- DevOps engineers automating deployments
- Teams wanting secure secret workflows
- Security-conscious organizations

### Evolution

1Password CLI evolved from manual copy-paste to automation:
- v1: Basic secret retrieval
- v2: Service accounts, biometrics, better scripting

## 3) Defining architectural bets

### Biometric unlock

1Password CLI uses biometrics when available:
```bash
$ op signin
# Prompts for fingerprint/face unlock
```

No password typing for daily use.

### Secret references

Secrets referenced by path, not value:
```bash
# Reference, not value
op read "op://Production/API/credential"
```

This enables:
- Secret rotation without code changes
- No secrets in shell history
- No secrets in environment variables

### Service accounts

For automation (CI/CD), 1Password offers service accounts:
```bash
# Non-interactive, limited scope
op signin --service-account <token>
```

- Token-based authentication
- Fine-grained access control
- Audit logging

### Environment injection

1Password can inject secrets as environment variables:
```bash
op run --env-file=.env -- npm start
```

`.env` file contains references:
```
API_KEY=op://Production/API/key
DATABASE_URL=op://Production/Database/url
```

Secrets are injected at runtime, not stored.

## 4) Standout strengths

- **Biometric unlock**: Convenient and secure
- **Secret references**: No secrets in code
- **Service accounts**: Automation-friendly
- **Audit logging**: Track secret access
- **Multi-platform**: macOS, Linux, Windows
- **Enterprise features**: Groups, permissions

## 5) Chronic weaknesses and recurring costs

### 1Password dependency

Requires 1Password subscription:
- Individual: ~$36/year
- Families: ~$60/year
- Business: ~$96/user/year

### Service account complexity

Setting up service accounts:
- Token management
- Permission configuration
- Rotation procedures

More complex than environment variables.

### Network dependency

1Password CLI requires internet:
- Can't access secrets offline
- API rate limits
- Latency for secret retrieval

### Lock-in

Deep integration creates lock-in:
- Secrets stored in 1Password
- Migration is work
- Vendor-specific format

## 6) Between-release corrections

### CLI 1.x (2019-2022)
- Basic secret retrieval
- Sign-in required frequently

### CLI 2.0+ (2023+)
- Biometric unlock
- Service accounts
- Better scripting support
- Improved performance

The pattern: Maturing from manual tool to automation platform.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Secret references**: Store paths, not values
- **Environment injection**: Runtime secret loading
- **Service account patterns**: For CI/CD
- **Audit awareness**: Log secret access

### Reject early

- **Vendor lock-in**: Support multiple secret providers
- **Network requirement**: Allow offline operation
- **Subscription requirement**: Support free alternatives

### Prototype before deciding

- 1Password integration for Effigy
- Generic secret provider interface
- Local secret fallback

## 8) Comparison: 1Password vs. other approaches

| Approach | Security | Convenience | Cost |
|----------|----------|-------------|------|
| .env files | Low | High | Free |
| direnv + .env | Low | High | Free |
| 1Password CLI | High | Medium | Subscription |
| HashiCorp Vault | High | Low | Self-hosted cost |

**For Effigy**: Support multiple, recommend best practices.

## 9) Effigy Integration Ideas

### Option 1: Native integration

```toml
[env]
API_KEY = { op = "op://Production/API/key" }
```

Effigy calls `op read` at runtime.

### Option 2: Generic secret provider

```toml
[env]
API_KEY = { secret = "1password://Production/API/key" }
# Or
API_KEY = { secret = "vault://secret/data/api" }
# Or
API_KEY = { secret = "env://LOCAL_API_KEY" }
```

### Option 3: Use op run

```bash
# Wrap effigy with op run
op run --env-file=.env -- effigy deploy
```

No Effigy changes needed.

### Option 4: Environment templates

```toml
[env]
# Loaded from local .env
# Can be overridden by op run
API_KEY = "${API_KEY}"
```

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [1Password CLI docs](https://developer.1password.com/docs/cli/) | official docs | current | high | Primary reference |
| [1Password CLI reference](https://developer.1password.com/docs/cli/reference/) | official docs | current | high | Commands |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |

## 11) Open questions

- What's the latency impact of secret retrieval?
- How do teams handle secret rotation?
- What's the adoption barrier (cost, complexity)?

## Next Task

Compare against direnv and other tools in Track 10 synthesis on environment management.


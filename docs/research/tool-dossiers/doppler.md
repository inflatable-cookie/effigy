# Doppler

Status: Draft
Tool name: Doppler
Category: Cloud-based secrets manager
Owner:
Last updated: 2026-03-07
Scope: Doppler secrets management platform, CLI, developer workflow

## 1) Why this tool matters

Doppler is a cloud-based secrets management platform designed for developer workflows. It's notable for:
- Centralized secrets across environments
- Real-time sync across team
- CLI for local development
- CI/CD integrations

For Effigy, Doppler represents:
- Cloud-based secret management pattern
- Developer-first UX for secrets
- Integration model with task runners

## 2) Product and era context

### Timeline

- **2018**: Doppler founded
- **2020-2022**: Rapid feature development
- **2023-2024**: Enterprise features, broader integrations
- **Present**: Established player in secrets management

### Design Philosophy

From Doppler documentation:

> "SecretOps platform"
> "Stop wasting time on secrets management"
> "Single source of truth for secrets"

### Target Audience

- Development teams
- DevOps engineers
- Startups to enterprises
- Multi-environment projects

### Ecosystem

- **Integrations**: GitHub Actions, AWS, Vercel, etc.
- **SDKs**: Multiple languages
- **CLI**: Primary interface for developers
- **VS Code**: Extension available

## 3) Defining architectural bets

### Centralized cloud vault

Secrets stored in Doppler cloud:
```bash
# Login
doppler login

# Setup project
doppler setup

# Run with secrets
doppler run -- npm start
```

Benefits:
- Single source of truth
- Access control and audit logs
- No secrets in git

### Environment model

Projects → Configs (environments):
```
my-app/
  ├── dev
  ├── staging
  └── prod
```

Each config contains secrets for that environment.

### CLI injection

```bash
# Inject secrets as env vars
doppler run -- ./start.sh

# Specific config
doppler run -c staging -- ./start.sh
```

No .env files written to disk (by default).

### Real-time sync

Changes propagate immediately:
- Web dashboard edits
- CLI updates
- API changes

All clients receive updates.

## 4) Standout strengths

- **Centralized**: Single source of truth
- **Developer UX**: Simple CLI, good documentation
- **Integrations**: Wide ecosystem support
- **Access control**: Granular permissions
- **Audit logs**: Track access and changes
- **No secret sprawl**: Not stored in git or local files

## 5) Chronic weaknesses and recurring costs

### Cloud dependency

Requires internet and Doppler service:
- No offline development
- Service availability concerns
- Network latency

### Subscription cost

Pricing tiers:
- Free tier: 3 users, limited features
- Teams: $7/user/month
- Enterprise: Custom pricing

### Vendor lock-in

Migration challenges:
- Export possible but workflow changes
- Integration-specific features
- Team workflow tied to platform

### Trust model

Cloud service has secrets:
- Must trust Doppler security
- Data on their servers (encrypted)
- Compliance considerations

## 6) Between-release corrections

### Early Doppler (2018-2020)
- Basic secret storage
- Limited integrations

### Modern Doppler (2021-)
- Expanded integrations
- Enterprise features
- Self-hosting option (Enterprise)

The pattern: Added enterprise features while keeping developer UX simple.

## 7) Effigy-relevant lessons

### Adopt carefully

- **CLI injection**: Run commands with secrets
- **Environment separation**: Dev/staging/prod configs
- **No local files**: Avoid .env file sprawl
- **Access control**: Granular permissions

### Reject early

- **Cloud dependency**: Require offline capability
- **Subscription model**: Avoid mandatory costs
- **Vendor lock-in**: Keep migration path open
- **External service**: Self-hosted preferred

### Prototype before deciding

- Doppler integration as optional backend
- Similar UX without cloud dependency
- Hybrid: local encryption + optional cloud

## 8: Effigy Integration Options

### Option 1: Doppler as backend

```toml
# effigy.toml
[secrets]
provider = "doppler"
project = "my-app"
config = "dev"

[[task]]
name = "start"
env = { from_doppler = true }
```

### Option 2: Doppler-compatible interface

```bash
# Similar to doppler run
effigy secrets run -- ./start.sh

# Uses local encrypted storage, not cloud
```

### Option 3: Migration path

```bash
# Export from Doppler
doppler secrets download --format json > secrets.json

# Import to Effigy
effigy secrets import --from doppler secrets.json
```

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [doppler.com](https://www.doppler.com) | official docs | current | high | Primary reference |
| [Doppler CLI docs](https://docs.doppler.com/docs/cli) | docs | current | high | CLI reference |
| [GitHub dopplerhq/cli](https://github.com/DopplerHQ/cli) | source | latest | high | Implementation |
| Blog posts | various | ongoing | medium | Use cases |

## 10: Open questions

- How does Doppler handle secret rotation?
- What happens during outages?
- How does pricing scale for large teams?

## Next Task

Compare against Bitwarden, 1Password, and self-hosted solutions in Track 16 synthesis.


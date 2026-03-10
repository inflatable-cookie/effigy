# Mozilla SOPS

Status: Draft
Tool name: SOPS (Secrets OPerationS)
Category: File encryption for secrets
Owner:
Last updated: 2026-03-07
Scope: SOPS encryption for YAML, JSON, ENV, binary files

## 1) Why this tool matters

SOPS is a widely-adopted tool for encrypting secrets in structured files. It's notable for:
- Encrypting values while keeping keys readable
- Multiple key backends (KMS, PGP, age)
- Git-friendly diffs (shows which keys changed)
- Editing encrypted files in-place

For Effigy, SOPS represents:
- Industry-standard approach to encrypted secrets in repos
- Multiple encryption backend options
- Integration patterns with cloud providers

## 2) Product and era context

### Timeline

- **2015**: Initial release by Mozilla
- **2017-2020**: Adoption by Kubernetes/GitOps community
- **2021-2024**: Added age support, cloud KMS improvements
- **Present**: Active maintenance, industry standard

### Design Philosophy

From SOPS documentation:

> "SOPS is an editor of encrypted files that supports YAML, JSON, ENV, INI and BINARY formats"
> "Encrypts values in structured files, not keys"

### Target Audience

- DevOps and platform engineers
- Kubernetes users
- Infrastructure-as-code practitioners
- Security-conscious teams

### Ecosystem

- **Helm Secrets**: Helm plugin for SOPS
- **Kustomize**: SOPS integration
- **Flux CD**: Native SOPS support
- **ArgoCD**: SOPS integration available

## 3) Defining architectural bets

### Structured file encryption

Encrypt values, not keys:
```yaml
# Encrypted YAML
apiVersion: v1
kind: Secret
metadata:
    name: my-secret
type: Opaque
stringData:
    password: ENC[AES256_GCM,data:...,iv:...,type:str]
```

Benefits:
- Git diffs show which keys changed
- Code review without exposing values
- Structure remains visible

### Multiple key backends

```bash
# AWS KMS
sops --kms arn:aws:kms:us-east-1:... encrypt secret.yaml

# GCP KMS
sops --gcp-kms projects/... encrypt secret.yaml

# Azure Key Vault
sops --azure-kv https://... encrypt secret.yaml

# PGP
sops --pgp 12345678 encrypt secret.yaml

# age (modern)
sops --age age1... encrypt secret.yaml
```

Flexibility to match existing infrastructure.

### Config file (.sops.yaml)

Per-directory encryption rules:
```yaml
# .sops.yaml
creation_rules:
  - path_regex: \.prod\.yaml$
    kms: arn:aws:kms:us-east-1:...:key/prod
  
  - path_regex: \.dev\.yaml$
    kms: arn:aws:kms:us-east-1:...:key/dev
  
  - path_regex: \.yaml$
    age: age1...
```

Automatic key selection based on file path.

### In-place editing

```bash
# Edit encrypted file (decrypts, opens editor, re-encrypts)
sops secret.yaml

# Rotate data key
sops rotate secret.yaml
```

No manual decrypt/edit/encrypt cycle.

## 4) Standout strengths

- **Git-friendly**: Diffs show key changes, not encrypted blobs
- **Multiple backends**: KMS, PGP, age support
- **Auditing**: Track access with cloud KMS
- **File-level permissions**: Different keys for different files
- **CI/CD integration**: Decrypt at deploy time
- **Mature ecosystem**: Wide tool support

## 5) Chronic weaknesses and recurring costs

### Key management complexity

Managing encryption keys:
- PGP key distribution
- KMS IAM policies
- Key rotation procedures
- Backup/restore concerns

### Cloud dependency

Cloud KMS options tie to providers:
- AWS account required for AWS KMS
- Cross-cloud usage is complex
- Cost for API calls

### No native env var support

SOPS works with files, not directly with environment:
```bash
# Requires intermediate step
sops -d secret.env > .env
source .env
```

### Complex for simple use cases

Overkill for single-developer projects:
- Multiple tools to install
- Key setup overhead
- Learning curve

## 6) Between-release corrections

### Early SOPS (2015-2018)
- PGP primarily
- YAML/JSON focus

### Modern SOPS (2019-)
- Added age support (modern crypto)
- Cloud KMS improvements
- Binary file support
- Better CI/CD integration

The pattern: Added modern crypto options while maintaining compatibility.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Encrypt values, not keys**: Git-friendly approach
- **Multiple backends**: Flexibility for different teams
- **File-based**: Natural for configuration
- **In-place editing**: Good developer experience

### Reject early

- **Complexity for simple cases**: Too heavy for basic needs
- **Cloud dependency**: Require offline capability
- **File-only**: Need direct env var support

### Prototype before deciding

- SOPS integration for effigy secrets
- Decrypt-and-load workflow
- Key management simplification

## 8: Effigy + SOPS Integration

### Option 1: Native SOPS support

```toml
# effigy.toml
[secrets]
backend = "sops"
config = ".sops.yaml"

[[task]]
name = "deploy"
env = { from_sops = "secrets.prod.yaml" }
```

### Option 2: Decrypt on load

```bash
# SOPS-encrypted .env file
effigy run --env secrets.env.yaml -- task

# Effigy decrypts SOPS file before loading
```

### Option 3: SOPS-inspired encryption

```toml
# Effigy-native encryption inspired by SOPS
[secrets]
type = "age"
public_key = "age1..."

[[task]]
name = "build"
env = { encrypted_file = "secrets.env" }
```

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [SOPS GitHub](https://github.com/mozilla/sops) | source | current | high | Primary reference |
| [SOPS README](https://github.com/mozilla/sops/blob/main/README.rst) | docs | current | high | Usage guide |
| [GitGuardian SOPS guide](https://blog.gitguardian.com/a-comprehensive-guide-to-sops/) | tutorial | 2024 | medium | Best practices |
| Community tutorials | various | ongoing | medium | Use cases |

## 10: Open questions

- How to simplify key management for small teams?
- Can SOPS be integrated without the full tool chain?
- What's the performance impact of decrypting on every run?

## Next Task

Compare against git-crypt, age, and other encryption tools in Track 16 synthesis.


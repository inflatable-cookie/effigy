# age encryption

Status: Draft
Tool name: age
Category: Modern file encryption
Owner:
Last updated: 2026-03-07
Scope: age file encryption tool, modern alternative to PGP

## 1) Why this tool matters

age is a modern, simple file encryption tool designed as a better alternative to PGP/GPG. It's notable for:
- Small, explicit keys (no keyring management)
- No configuration options (secure defaults)
- Modern cryptography (X25519, ChaCha20-Poly1305)
- SSH key compatibility
- Post-quantum hybrid support

For Effigy, age represents:
- Modern encryption for secrets
- Simpler alternative to PGP
- Good UX patterns for crypto tools
- Plugin architecture for extensibility

## 2) Product and era context

### Timeline

- **2019**: Initial release by Filippo Valsorda
- **2020-2022**: Rust implementation (rage), wide adoption
- **2023-2024**: SSH key support, plugin system
- **2025**: Post-quantum hybrid support (ML-KEM-768)

### Design Philosophy

From age documentation:

> "A simple, modern and secure file encryption tool"
> "Have one joint and keep it well oiled"
> "No configuration or (much) algorithm agility"

### Target Audience

- Developers wanting simple file encryption
- Security-conscious users avoiding PGP complexity
- Infrastructure/tooling authors
- Anyone encrypting files

### Ecosystem

- **rage**: Rust implementation (fully compatible)
- **SOPS**: age backend support
- **Plugins**: YubiKey, hardware tokens
- **Language bindings**: Go, Rust, Python

## 3) Defining architectural bets

### Minimalism

No options, secure defaults only:
```bash
# Generate key
age-keygen -o key.txt

# Encrypt
age -r age1ql3z7hjy54pw3... file.txt > file.txt.age

# Decrypt
age -d -i key.txt file.txt.age
```

No:
- Keyring management
- Algorithm selection
- Trust models
- Configuration files

### Modern cryptography

- X25519 for key exchange
- ChaCha20-Poly1305 for encryption
- HKDF-SHA256 for key derivation
- STREAM for large file chunking

### SSH compatibility

Use existing SSH keys:
```bash
# Encrypt to SSH public key
age -r ~/.ssh/id_ed25519.pub file.txt > file.txt.age

# Decrypt with SSH private key
age -d -i ~/.ssh/id_ed25519 file.txt.age
```

No separate key management needed.

### Bech32 encoding

Human-friendly key format:
```
# Public key
age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j...

# Private key
AGE-SECRET-KEY-1... (starts with clear prefix)
```

Easy to recognize, type, share.

### Plugin system

Extensible via subprocesses:
```bash
# YubiKey plugin
age -r age1yubikey1... file.txt > file.txt.age

# Custom HSM
age -r age1hsm1... file.txt > file.txt.age
```

Core stays small, extensions provide extra features.

## 4) Standout strengths

- **Simple**: No configuration, easy to learn
- **Modern crypto**: X25519, ChaCha20-Poly1305
- **Small keys**: Easy to share, backup
- **SSH support**: Use existing keys
- **Multiple implementations**: Go (reference), Rust (rage)
- **Post-quantum**: Hybrid ML-KEM-768 + X25519

## 5) Chronic weaknesses and recurring costs

### No signing

age encrypts, doesn't sign:
- For signing, use minisign or ssh
- Single-purpose tools philosophy

### Newer/less adopted than PGP

Ecosystem gaps:
- Not as widely integrated as PGP
- Some tools still require GPG
- Email encryption not supported

### Key distribution

Still need to share public keys:
- No keyserver infrastructure
- Manual distribution
- Documentation encourages out-of-band sharing

## 6) Between-release corrections

### Early age (2019-2020)
- Basic file encryption
- X25519 only

### Modern age (2021-)
- SSH key support
- Plugin system
- Post-quantum hybrid
- rage (Rust) implementation

The pattern: Core stays simple, extensibility via plugins.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Modern crypto**: X25519, ChaCha20-Poly1305
- **Simplicity**: No configuration needed
- **SSH compatibility**: Leverage existing keys
- **Plugin architecture**: Extensible without bloat

### Reject early

- **No signing**: Need separate solution for signatures
- **Key distribution**: Still manual
- **Tool chain requirement**: Additional dependency

### Prototype before deciding

- Age integration for secret files
- Hybrid approach with SOPS
- Custom age-based encryption

## 8: Effigy Integration Options

### Option 1: Native age support

```toml
# effigy.toml
[secrets]
backend = "age"
public_key = "age1ql3z7hjy54pw3..."

[[task]]
name = "deploy"
env = { encrypted_file = "secrets.env.age" }
```

### Option 2: Age key generation

```bash
# Generate age key for project
effigy secrets init --backend age
# Creates .effigy-key (git-ignored)
# Creates .effigy-key.pub (committed)
```

### Option 3: Transparent age encryption

```bash
# Encrypt file
effigy secrets encrypt secrets.env
# Creates secrets.env.age

# Auto-decrypt on task run
effigy run --secrets secrets.env.age -- task
```

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [age website](https://age-encryption.org) | official docs | current | high | Primary reference |
| [GitHub FiloSottile/age](https://github.com/FiloSottile/age) | source | latest | high | Implementation |
| [age design doc](https://docs.google.com/document/d/...) | design doc | 2019 | high | Rationale |
| [nixFAQ age article](https://nixfaq.org/2021/01/age-the-modern-alternative-to-gpg.html) | tutorial | 2021 | medium | Comparison |

## 10: Open questions

- Should Effigy bundle age or require it as dependency?
- How to handle key backup/recovery?
- What's the migration path from .env files?

## Next Task

Compare against SOPS, git-crypt, and cloud secret managers in Track 16 synthesis.


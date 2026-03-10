# git-crypt

Status: Draft
Tool name: git-crypt
Category: Transparent git encryption
Owner:
Last updated: 2026-03-07
Scope: git-crypt transparent encryption for git repositories

## 1) Why this tool matters

git-crypt enables transparent encryption of files in git repositories. It's notable for:
- Automatic encrypt on commit, decrypt on checkout
- GPG-based multi-user support
- Symmetric key option for simple cases
- No changes to git workflow once set up

For Effigy, git-crypt represents:
- Transparent encryption model (users don't think about it)
- Git integration patterns
- Key management approaches

## 2) Product and era context

### Timeline

- **2014**: Initial release by Andrew Ayer
- **2017-2022**: Stable releases, feature complete
- **2022-04**: Version 0.7.0 (latest)
- **Present**: Maintenance mode, stable

### Design Philosophy

From git-crypt documentation:

> "Transparent file encryption in git"
> "git-crypt lets you freely share a repository containing a mix of public and private content"

### Target Audience

- Developers wanting to store secrets in git
- Small to medium teams
- Projects with mix of public/private content
- Self-hosters

### Ecosystem

- **GitHub**: 3k+ stars, widely used
- **Package managers**: brew, apt, etc.
- **Integrations**: yadm (dotfiles manager)

## 3) Defining architectural bets

### Transparent encryption

Files encrypted automatically via git filters:
```
# .gitattributes
secretfile filter=git-crypt diff=git-crypt
*.key filter=git-crypt diff=git-crypt
secrets/** filter=git-crypt diff=git-crypt
```

Once set up, users use git normally:
```bash
git add secrets/config.env
git commit -m "Update config"
git push
# File is encrypted automatically
```

### GPG integration

Multi-user support via GPG:
```bash
# Add user with GPG key
git-crypt add-gpg-user user@example.com

# File encrypted for all users
# Any user can decrypt with their GPG key
git-crypt unlock
```

### Symmetric key option

Simple password-based encryption:
```bash
# Export key for CI/storage
git-crypt export-key ./git-crypt-key

# Unlock with key
git-crypt unlock ./git-crypt-key
```

### Graceful degradation

Users without key can still:
- Clone the repository
- View non-encrypted files
- Commit to non-encrypted files
- Cannot view encrypted content

## 4) Standout strengths

- **Transparent**: Works with normal git workflow
- **GPG support**: Multi-user with existing keys
- **Symmetric option**: Simple password mode
- **File-level**: Selective encryption via .gitattributes
- **No server**: Fully self-hosted
- **Mature**: Stable, well-tested

## 5) Chronic weaknesses and recurring costs

### User management complexity

Adding/removing users requires re-encryption:
```bash
# Add user
git-crypt add-gpg-user new@example.com

# Remove user - must re-encrypt all files
# No native command for removal
```

### GPG friction

GPG complexity:
- Key generation complexity
- Key distribution challenges
- Trust model confusion

### No native rotation

Key rotation is painful:
- Export/decrypt/re-encrypt cycle
- Coordination across team
- No automated rotation

### Limited metadata

Encrypted files are opaque:
- Can't see who encrypted
- No audit trail
- No versioning of keys

## 6) Between-release corrections

### Early git-crypt (2014-2017)
- Basic GPG support
- Filter-based encryption

### Modern git-crypt (2017-)
- Symmetric key option added
- Multiple key support
- Status command improvements

Now in maintenance mode - stable but not actively developed.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Transparency**: Encryption should be invisible to users
- **Git integration**: Work with existing workflows
- **Flexible key options**: Support both GPG and symmetric
- **File-level control**: Let users choose what to encrypt

### Reject early

- **GPG dependency**: Too complex for many users
- **User management**: Difficult to add/remove users
- **No rotation**: Key rotation is painful
- **Opaque encryption**: No visibility into encrypted state

### Prototype before deciding

- Transparent encryption for effigy secrets
- Simplified key management
- Integration with age (modern crypto)

## 8: Effigy Integration Options

### Option 1: Git-crypt style transparency

```bash
# Effigy manages transparent encryption
# .gitattributes marks files for encryption
effigy secrets init  # Initialize encryption
effigy secrets add-user user@example.com  # Add GPG user
```

### Option 2: Simplified symmetric encryption

```toml
# effigy.toml
[secrets]
encryption = "symmetric"
key_file = ".effigy-key"  # Git-ignored

[[secrets.files]]
pattern = "*.secret.env"
encrypt = true
```

### Option 3: Age-based modern alternative

```toml
# effigy.toml - inspired by git-crypt but with age
[secrets]
backend = "age"
public_key = "age1..."

[secrets.files]
include = ["secrets/**", "*.env.secret"]
```

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [git-crypt](https://www.agwa.name/projects/git-crypt/) | official docs | current | high | Primary reference |
| [GitHub AGWA/git-crypt](https://github.com/AGWA/git-crypt) | source | v0.7.0 | high | Implementation |
| [LinuxLinks review](https://www.linuxlinks.com/git-crypt-transparent-file-encryption/) | review | 2023 | medium | Overview |
| Community tutorials | various | ongoing | medium | Usage patterns |

## 10: Open questions

- Why is git-crypt in maintenance mode?
- What are the alternatives for new projects?
- How does it compare to transcrypt (similar tool)?

## Next Task

Compare against age, SOPS, and other encryption tools in Track 16 synthesis.


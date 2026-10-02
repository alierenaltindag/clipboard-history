# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take the security and privacy of user clipboard data extremely seriously. If you discover a security vulnerability, please report it privately:

1. **Do NOT report security issues via public GitHub issues.**
2. Send an email with a detailed description, reproduction steps, and proof-of-concept to `security@example.com` (or submit a Private Vulnerability Advisory on GitHub).
3. You will receive an initial response acknowledging receipt within 48 hours.
4. We will coordinate a fix, release a patch version, and publish a security advisory giving appropriate credit.

## Security Guarantees & Safeguards
- **Sensitive Data Isolation**: Database and blobs are stored with POSIX `0600` permissions.
- **Credential Protection**: Known password manager targets (`x-kde-passwordManagerHint`, KeePassXC, 1Password, Bitwarden) are suppressed from being stored.
- **Regex Secret Masking**: Credit cards, private keys, and high-entropy API tokens are blocked or masked before writing to persistent storage.

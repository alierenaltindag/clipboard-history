# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

Please report security vulnerabilities privately using [GitHub Private Vulnerability Reporting](https://github.com/alierenaltindag/clipboard-history/security/advisories/new). This ensures that the issue can be discussed, triaged, and resolved before any public disclosure.

1. **Do NOT report security issues via public GitHub issues.**
2. Submitting through the private advisory link allows us to safely collaborate on a fix, or email alierenaltindaag@gmail.com.
3. You will receive an initial response acknowledging receipt within 48 hours.
4. We will coordinate a fix, release a patch version, and publish a security advisory giving appropriate credit.

## Security Guarantees & Safeguards
- **Sensitive Data Isolation**: Database and blobs are stored with POSIX `0600` permissions.
- **Credential Protection**: Known password manager targets (`x-kde-passwordManagerHint`, KeePassXC, 1Password, Bitwarden) are suppressed from being stored.
- **Regex Secret Masking**: Credit cards, private keys, and high-entropy API tokens are blocked or masked before writing to persistent storage.

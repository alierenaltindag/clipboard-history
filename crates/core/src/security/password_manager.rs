pub struct PasswordManagerGuard;

impl PasswordManagerGuard {
    pub const SENSITIVE_MIME_PATTERNS: &'static [&'static str] = &[
        "x-kde-passwordManagerHint",
        "application/x-keepassxc-selection",
        "application/x-1password-secret",
        "application/x-bitwarden-selection",
        "secret",
        "password",
        "private",
    ];

    /// Returns true if any MIME type or target indicates the clipboard payload contains sensitive credential data
    pub fn is_sensitive_mime(mime_types: &[String]) -> bool {
        for mime in mime_types {
            let lower = mime.to_lowercase();
            for pattern in Self::SENSITIVE_MIME_PATTERNS {
                if lower.contains(pattern) {
                    return true;
                }
            }
        }
        false
    }
}

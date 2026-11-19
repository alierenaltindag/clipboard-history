pub struct PasswordManagerGuard;

impl PasswordManagerGuard {
    pub const SENSITIVE_MIME_PATTERNS: &'static [&'static str] = &[
        "x-kde-passwordmanagerhint",
        "application/x-keepassxc-selection",
        "application/x-1password-secret",
        "application/x-bitwarden-selection",
        "x-private-clipboard",
        "private-data",
        "application/x-password",
        "application/x-gnome-password",
        "secret",
        "password",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixed_case_kde_password_hint_detected() {
        assert!(PasswordManagerGuard::is_sensitive_mime(&[
            "x-kde-passwordManagerHint".to_string()
        ]));
        assert!(PasswordManagerGuard::is_sensitive_mime(&[
            "X-KDE-PASSWORDMANAGERHINT".to_string()
        ]));
        assert!(PasswordManagerGuard::is_sensitive_mime(&[
            "x-private-clipboard".to_string()
        ]));
        assert!(PasswordManagerGuard::is_sensitive_mime(&[
            "application/x-private-data".to_string()
        ]));
        assert!(PasswordManagerGuard::is_sensitive_mime(&[
            "application/x-password".to_string()
        ]));
        assert!(PasswordManagerGuard::is_sensitive_mime(&[
            "application/x-gnome-password".to_string()
        ]));
    }

    #[test]
    fn test_innocent_desktop_mimes_not_rejected() {
        assert!(!PasswordManagerGuard::is_sensitive_mime(&[
            "application/x-private-notes".to_string()
        ]));
        assert!(!PasswordManagerGuard::is_sensitive_mime(&[
            "chromium/x-web-custom-data".to_string()
        ]));
        assert!(!PasswordManagerGuard::is_sensitive_mime(&[
            "application/x-qt-windows-mime".to_string()
        ]));
    }
}

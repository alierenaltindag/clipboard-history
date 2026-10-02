use std::process::Command;
use tracing::debug;

pub struct WindowFocusDetector {
    is_x11: bool,
}

impl WindowFocusDetector {
    pub fn new() -> Self {
        let is_x11 = std::env::var("DISPLAY").is_ok()
            && std::env::var("WAYLAND_DISPLAY").is_err()
            && std::env::var("XDG_SESSION_TYPE").unwrap_or_default() != "wayland";
        Self { is_x11 }
    }

    /// Queries the currently active window title and class if accessible.
    pub fn get_active_window_info(&self) -> Option<(String, String)> {
        if self.is_x11 {
            self.get_x11_active_window()
        } else {
            self.get_wayland_active_window()
        }
    }

    /// Checks if the active window matches any configured incognito/secret pattern.
    pub fn is_incognito_active(&self, patterns: &[String]) -> bool {
        if let Some((title, class)) = self.get_active_window_info() {
            Self::matches_incognito_pattern(&title, &class, patterns)
        } else {
            false
        }
    }

    /// Evaluates whether a given window title or class matches configured incognito patterns.
    pub fn matches_incognito_pattern(title: &str, class: &str, patterns: &[String]) -> bool {
        if patterns.is_empty() {
            return false;
        }

        let title_lower = title.to_lowercase();
        let class_lower = class.to_lowercase();

        for pat in patterns {
            let clean_pat = pat.trim().trim_matches('*').to_lowercase();
            if !clean_pat.is_empty()
                && (title_lower.contains(&clean_pat) || class_lower.contains(&clean_pat))
            {
                debug!(
                    "Active window (title='{}', class='{}') matched incognito pattern '{}'",
                    title, class, pat
                );
                return true;
            }
        }

        false
    }

    fn get_x11_active_window(&self) -> Option<(String, String)> {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

        if let Ok((conn, screen_num)) = x11rb::connect(None) {
            if let Some(screen) = conn.setup().roots.get(screen_num) {
                let root = screen.root;

                if let (Ok(active_reply), Ok(wm_reply), Ok(utf8_reply)) = (
                    conn.intern_atom(false, b"_NET_ACTIVE_WINDOW"),
                    conn.intern_atom(false, b"_NET_WM_NAME"),
                    conn.intern_atom(false, b"UTF8_STRING"),
                ) {
                    if let (Ok(active_atom), Ok(wm_atom), Ok(utf8_atom)) = (
                        active_reply.reply().map(|r| r.atom),
                        wm_reply.reply().map(|r| r.atom),
                        utf8_reply.reply().map(|r| r.atom),
                    ) {
                        if let Ok(active_win_reply) =
                            conn.get_property(false, root, active_atom, AtomEnum::WINDOW, 0, 1)
                        {
                            if let Ok(rep) = active_win_reply.reply() {
                                if let Some(active_win) =
                                    rep.value32().and_then(|mut iter| iter.next())
                                {
                                    if active_win != 0 {
                                        let title = if let Ok(reply) = conn.get_property(
                                            false, active_win, wm_atom, utf8_atom, 0, 1024,
                                        ) {
                                            if let Ok(r) = reply.reply() {
                                                String::from_utf8_lossy(&r.value).to_string()
                                            } else {
                                                String::new()
                                            }
                                        } else {
                                            String::new()
                                        };

                                        let class = if let Ok(reply) = conn.get_property(
                                            false,
                                            active_win,
                                            AtomEnum::WM_CLASS,
                                            AtomEnum::STRING,
                                            0,
                                            1024,
                                        ) {
                                            if let Ok(r) = reply.reply() {
                                                String::from_utf8_lossy(&r.value).replace('\0', " ")
                                            } else {
                                                String::new()
                                            }
                                        } else {
                                            String::new()
                                        };

                                        return Some((title, class));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback using xdotool if available
        if let Ok(output) = Command::new("xdotool").args(["getactivewindow"]).output() {
            if output.status.success() {
                let win_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let title = Command::new("xdotool")
                    .args(["getwindowname", &win_id])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();
                let class = Command::new("xdotool")
                    .args(["getwindowclassname", &win_id])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();
                return Some((title, class));
            }
        }
        None
    }

    fn get_wayland_active_window(&self) -> Option<(String, String)> {
        // 1. Try hyprctl (Hyprland)
        if which::which("hyprctl").is_ok() {
            if let Ok(output) = Command::new("hyprctl")
                .args(["activewindow", "-j"])
                .output()
            {
                if output.status.success() {
                    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                        let title = val["title"].as_str().unwrap_or_default().to_string();
                        let class = val["class"].as_str().unwrap_or_default().to_string();
                        if !title.is_empty() || !class.is_empty() {
                            return Some((title, class));
                        }
                    }
                }
            }
        }

        // 2. Try swaymsg (Sway)
        if which::which("swaymsg").is_ok() {
            if let Ok(output) = Command::new("swaymsg").args(["-t", "get_tree"]).output() {
                if output.status.success() {
                    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                        if let Some((title, app_id)) = find_focused_sway_node(&val) {
                            return Some((title, app_id));
                        }
                    }
                }
            }
        }

        None
    }
}

fn find_focused_sway_node(val: &serde_json::Value) -> Option<(String, String)> {
    if val["focused"].as_bool().unwrap_or(false) {
        let name = val["name"].as_str().unwrap_or_default().to_string();
        let app_id = val["app_id"]
            .as_str()
            .or_else(|| val["window_properties"]["class"].as_str())
            .unwrap_or_default()
            .to_string();
        return Some((name, app_id));
    }

    if let Some(nodes) = val["nodes"].as_array() {
        for node in nodes {
            if let Some(res) = find_focused_sway_node(node) {
                return Some(res);
            }
        }
    }

    if let Some(floating) = val["floating_nodes"].as_array() {
        for node in floating {
            if let Some(res) = find_focused_sway_node(node) {
                return Some(res);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_incognito_pattern() {
        let patterns = vec![
            "*incognito*".to_string(),
            "*private browsing*".to_string(),
            "*tor browser*".to_string(),
            "*keepass*".to_string(),
        ];

        // Should match incognito title in Chrome/Chromium
        assert!(WindowFocusDetector::matches_incognito_pattern(
            "New Tab - Google Chrome (Incognito)",
            "google-chrome",
            &patterns
        ));

        // Should match Firefox private browsing
        assert!(WindowFocusDetector::matches_incognito_pattern(
            "Mozilla Firefox (Private Browsing)",
            "firefox",
            &patterns
        ));

        // Should match by window class
        assert!(WindowFocusDetector::matches_incognito_pattern(
            "Passwords Database",
            "org.keepassxc.KeePassXC",
            &patterns
        ));

        // Should not match regular window
        assert!(!WindowFocusDetector::matches_incognito_pattern(
            "Cargo.toml - ClipboardHistory - Visual Studio Code",
            "code",
            &patterns
        ));

        // Empty patterns should never match
        assert!(!WindowFocusDetector::matches_incognito_pattern(
            "Incognito Window",
            "chrome",
            &[]
        ));
    }

    #[test]
    fn test_find_focused_sway_node() {
        let tree_json = serde_json::json!({
            "nodes": [
                {
                    "nodes": [
                        {
                            "focused": false,
                            "name": "Terminal",
                            "app_id": "alacritty"
                        },
                        {
                            "focused": true,
                            "name": "Mozilla Firefox Private Browsing",
                            "app_id": "firefox"
                        }
                    ]
                }
            ]
        });

        let focused = find_focused_sway_node(&tree_json);
        assert_eq!(
            focused,
            Some((
                "Mozilla Firefox Private Browsing".to_string(),
                "firefox".to_string()
            ))
        );
    }
}

#![cfg(feature = "gtk")]

use gtk4::gdk::Display;
use gtk4::CssProvider;

pub const APPLICATION_CSS: &str = r#"
/* Modern Polished Theme for Clipboard History */

/* Window Container */
window.clipboard-window,
window {
    border-radius: 16px;
    background-color: alpha(@window_bg_color, 0.96);
    border: 1px solid alpha(@borders, 0.35);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.4);
}

/* HeaderBar */
headerbar {
    border-bottom: 1px solid alpha(@borders, 0.2);
    background-color: transparent;
    padding: 2px 6px;
}
headerbar .heading {
    font-weight: 700;
    font-size: 14px;
}
headerbar button {
    border-radius: 8px;
    padding: 6px 8px;
    margin: 2px;
    transition: all 180ms cubic-bezier(0.2, 0, 0, 1);
}
headerbar button:hover {
    background-color: alpha(@window_fg_color, 0.1);
    transform: translateY(-1px);
}
headerbar button:active {
    transform: translateY(1px) scale(0.96);
}

/* Search Entry */
.search-bar {
    border-radius: 20px;
    padding: 7px 14px;
    margin: 8px 12px 4px 12px;
    background-color: alpha(@card_bg_color, 0.85);
    border: 1px solid alpha(@borders, 0.3);
    font-size: 13.5px;
    transition: all 200ms cubic-bezier(0.2, 0, 0, 1);
}
.search-bar:focus-within {
    border-color: @accent_color;
    box-shadow: 0 0 0 3px alpha(@accent_color, 0.25);
    background-color: @card_bg_color;
}

/* Category Filter Bar */
.filter-bar {
    padding: 4px 8px;
    margin: 2px 8px 4px 8px;
}
.filter-pill {
    border-radius: 16px;
    padding: 4px 12px;
    font-size: 12px;
    font-weight: 500;
    margin: 2px 2px;
    background: transparent;
    border: 1px solid transparent;
    color: alpha(@window_fg_color, 0.75);
    transition: all 180ms cubic-bezier(0.2, 0, 0, 1);
}
.filter-pill:hover {
    background-color: alpha(@window_fg_color, 0.08);
    color: @window_fg_color;
}
.filter-pill.filter-active {
    background-color: @accent_bg_color;
    color: @accent_fg_color;
    font-weight: 600;
    border: 1px solid alpha(@accent_color, 0.3);
    box-shadow: 0 2px 6px alpha(@accent_color, 0.3);
    transform: scale(1.02);
}

/* ListBox & History Cards */
listview, listbox {
    background: transparent;
}
.history-card {
    border-radius: 10px;
    padding: 10px 14px;
    margin: 3px 10px;
    background-color: alpha(@card_bg_color, 0.65);
    border: 1px solid alpha(@borders, 0.2);
    transition: background-color 180ms cubic-bezier(0.2, 0, 0, 1),
                border-color 180ms cubic-bezier(0.2, 0, 0, 1),
                box-shadow 180ms cubic-bezier(0.2, 0, 0, 1),
                transform 150ms cubic-bezier(0.2, 0, 0, 1);
}
.history-card:hover {
    background-color: alpha(@card_bg_color, 0.95);
    border-color: alpha(@accent_color, 0.4);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.18);
    transform: translateY(-1px);
}
.history-card:selected {
    background-color: alpha(@accent_color, 0.22);
    border-color: @accent_color;
    box-shadow: 0 0 0 1px @accent_color;
}

/* Keyboard Keycap Badge */
.keycap {
    border-radius: 4px;
    background-color: alpha(@window_fg_color, 0.08);
    border: 1px solid alpha(@window_fg_color, 0.18);
    border-bottom: 2px solid alpha(@window_fg_color, 0.28);
    color: alpha(@window_fg_color, 0.85);
    font-size: 10.5px;
    font-weight: 600;
    padding: 1px 5px;
    font-family: monospace;
}

/* Type Pill Badges */
.badge-type {
    border-radius: 5px;
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    letter-spacing: 0.3px;
}
.badge-text {
    background-color: alpha(#3584e4, 0.18);
    color: #62a0ea;
    border: 1px solid alpha(#3584e4, 0.3);
}
.badge-code {
    background-color: alpha(#9141ac, 0.18);
    color: #c061cb;
    border: 1px solid alpha(#9141ac, 0.3);
    font-family: monospace;
}
.badge-image {
    background-color: alpha(#26a269, 0.18);
    color: #33d17a;
    border: 1px solid alpha(#26a269, 0.3);
}
.badge-files {
    background-color: alpha(#e66100, 0.18);
    color: #ff7800;
    border: 1px solid alpha(#e66100, 0.3);
}
.badge-snippet {
    background-color: alpha(#e5a50a, 0.18);
    color: #f6d32d;
    border: 1px solid alpha(#e5a50a, 0.3);
}

/* Metadata & Timestamp Tags */
.meta-tag {
    font-size: 11px;
    color: alpha(@window_fg_color, 0.55);
}
.app-tag {
    font-size: 11px;
    font-weight: 500;
    color: alpha(@window_fg_color, 0.7);
    background-color: alpha(@window_fg_color, 0.06);
    border-radius: 4px;
    padding: 1px 5px;
}

/* Card Action Buttons */
.card-action-btn {
    border-radius: 6px;
    padding: 4px;
    min-width: 26px;
    min-height: 26px;
    opacity: 0.6;
    transition: all 150ms ease;
}
.card-action-btn:hover {
    opacity: 1.0;
    background-color: alpha(@window_fg_color, 0.12);
    transform: scale(1.1);
}
.card-action-btn:active {
    transform: scale(0.95);
}
.pin-active {
    color: #f6d32d;
    opacity: 1.0;
}

/* Code Preview Block */
.code-preview-box {
    font-family: 'JetBrains Mono', 'Fira Code', 'Monospace', monospace;
    font-size: 12px;
    background-color: alpha(@window_fg_color, 0.05);
    border-radius: 6px;
    padding: 6px 10px;
    border: 1px solid alpha(@borders, 0.15);
    margin-top: 3px;
}

/* Image Thumbnail Preview */
.thumbnail-preview {
    border-radius: 6px;
    border: 1px solid alpha(@borders, 0.25);
    margin-top: 4px;
    margin-bottom: 2px;
    background-color: alpha(black, 0.2);
}

/* Multi-Selection Action Bar */
actionbar {
    border-top: 1px solid alpha(@borders, 0.25);
    background-color: alpha(@headerbar_bg_color, 0.95);
    padding: 8px 12px;
    transition: all 250ms cubic-bezier(0.2, 0, 0, 1);
}
actionbar button {
    border-radius: 8px;
    margin: 0 3px;
    font-weight: 500;
    transition: all 150ms ease;
}

/* Modern Overlay Scrollbars */
scrollbar {
    background: transparent;
}
scrollbar slider {
    min-width: 6px;
    min-height: 6px;
    border-radius: 3px;
    background-color: alpha(@window_fg_color, 0.2);
    transition: all 180ms ease;
}
scrollbar slider:hover {
    background-color: alpha(@window_fg_color, 0.45);
    min-width: 8px;
}

/* StatusPage (Empty State) */
statuspage {
    padding: 24px 16px;
}
statuspage .title {
    font-weight: 700;
    font-size: 16px;
}
statuspage .description {
    font-size: 13px;
    color: alpha(@window_fg_color, 0.7);
}
"#;

pub fn apply_application_styles() {
    let provider = CssProvider::new();
    provider.load_from_string(APPLICATION_CSS);
    if let Some(display) = Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

#![cfg(feature = "gtk")]

use gtk4::gdk::Display;
use gtk4::CssProvider;

pub const APPLICATION_CSS: &str = r#"
window {
    border-radius: 14px;
    background-color: alpha(@window_bg_color, 0.98);
    border: 1px solid alpha(@borders, 0.3);
}
.history-card {
    border-radius: 8px;
    padding: 10px 12px;
    margin: 3px 6px;
    background-color: alpha(@card_bg_color, 0.6);
}
.history-card:hover {
    background-color: alpha(@accent_color, 0.12);
}
.history-card:selected {
    background-color: alpha(@accent_color, 0.22);
}
.filter-active {
    font-weight: bold;
    background-color: alpha(@accent_color, 0.2);
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

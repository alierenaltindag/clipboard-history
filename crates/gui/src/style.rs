#![cfg(feature = "gtk")]

use gtk4::gdk::Display;
use gtk4::CssProvider;

pub const APPLICATION_CSS: &str = r#"
/* Modern Obsidian/Mica Theme for Clipboard History (Raycast / Windows 11 Fluent inspired) */

@define-color window_bg_color #141416;
@define-color window_fg_color #f4f4f5;
@define-color view_bg_color #141416;
@define-color view_fg_color #f4f4f5;
@define-color headerbar_bg_color #141416;
@define-color headerbar_fg_color #f4f4f5;
@define-color card_bg_color rgba(255, 255, 255, 0.035);
@define-color card_fg_color #f4f4f5;
@define-color popover_bg_color #1a1a1e;
@define-color popover_fg_color #f4f4f5;
@define-color dialog_bg_color #1a1a1e;
@define-color dialog_fg_color #f4f4f5;

/* Global Resets: Kill all Ubuntu/Yaru orange outlines and defaults */
* {
    outline: none;
}

*:focus {
    outline: none;
    box-shadow: none;
}

/* Window Container */
window.clipboard-window,
window {
    border-radius: 14px;
    background-color: #141416;
    color: #f4f4f5;
    border: 1px solid rgba(255, 255, 255, 0.09);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.8);
}

.main-box {
    background-color: #141416;
    border-radius: 14px;
    border: none;
    box-shadow: none;
}

box {
    border: none;
    box-shadow: none;
    background: transparent;
}

/* Integrated Top Header */
.integrated-header {
    padding: 10px 14px 6px 14px;
    min-height: 36px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.header-icon {
    color: #60cdff;
    opacity: 0.9;
    margin-right: 2px;
}

.header-title {
    font-weight: 600;
    font-size: 13.5px;
    color: #f4f4f5;
    letter-spacing: -0.1px;
}

.header-btn {
    border-radius: 6px;
    padding: 4px 6px;
    margin-left: 2px;
    color: rgba(255, 255, 255, 0.55);
    background: transparent;
    border: none;
    box-shadow: none;
    transition: all 120ms ease;
}

.header-btn:hover {
    background-color: rgba(255, 255, 255, 0.08);
    color: #ffffff;
}

.header-btn:active {
    background-color: rgba(255, 255, 255, 0.14);
}

/* Search Entry: Ultra-modern pill/rounded with sleek border, NO orange outline */
.compact-search-bar,
searchentry,
entry {
    border-radius: 9px;
    padding: 7px 11px;
    margin: 8px 12px 6px 12px;
    background-color: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: #ffffff;
    font-size: 13px;
    outline: none;
    box-shadow: none;
    transition: all 140ms ease;
}

.compact-search-bar:focus-within,
.compact-search-bar:focus,
searchentry:focus-within,
entry:focus-within,
searchentry:focus,
entry:focus {
    outline: none;
    border-color: #60cdff;
    background-color: rgba(255, 255, 255, 0.08);
    box-shadow: 0 0 0 2px rgba(96, 205, 255, 0.25);
}

.compact-search-bar text,
searchentry text,
entry text {
    color: #ffffff;
    outline: none;
    box-shadow: none;
}

.compact-search-bar image {
    color: rgba(255, 255, 255, 0.45);
}

/* Category Tab Bar (Pills) */
.category-tabs-bar {
    padding: 2px 12px 6px 12px;
}

.category-tab {
    border-radius: 6px;
    padding: 3px 8px;
    font-size: 13.5px;
    min-width: 32px;
    min-height: 28px;
    color: rgba(255, 255, 255, 0.65);
    background-color: transparent;
    border: 1px solid transparent;
    box-shadow: none;
    transition: all 120ms ease;
}

.category-tab:hover {
    background-color: rgba(255, 255, 255, 0.08);
    color: #ffffff;
}

.category-tab.tab-active {
    background-color: rgba(255, 255, 255, 0.1);
    color: #ffffff;
    border-bottom: 2px solid #60cdff;
    border-radius: 6px 6px 0 0;
}

/* Toast Notifications */
toast {
    background-color: #1f1f23;
    color: #f4f4f5;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.7);
    font-size: 12px;
    font-weight: 500;
    padding: 6px 12px;
    margin-bottom: 8px;
}

toast button {
    background-color: rgba(96, 205, 255, 0.18);
    color: #60cdff;
    border: 1px solid rgba(96, 205, 255, 0.35);
    border-radius: 5px;
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    margin-left: 8px;
    transition: all 120ms ease;
}

toast button:hover {
    background-color: rgba(96, 205, 255, 0.3);
    color: #ffffff;
}

/* ScrolledWindow, Viewport & View Stack */
scrolledwindow,
scrolledwindow viewport,
scrolledwindow > viewport,
viewport,
stack,
.view,
view,
.history-stack,
.history-scrolled,
.history-scrolled viewport,
.history-scrolled > viewport,
.history-listbox {
    background-color: transparent !important;
    background: transparent !important;
    color: #f4f4f5;
    border: none;
    outline: none;
    box-shadow: none;
}

/* ListBox & History Cards */
listview, listbox, .history-listbox {
    background: transparent !important;
    background-color: transparent !important;
    padding: 4px 0;
    border: none;
    outline: none;
}

listbox row {
    border: none;
    outline: none;
    box-shadow: none;
    background: transparent;
    background-color: transparent;
    padding: 0;
    margin: 0;
}

listbox row:selected,
listbox row:focus {
    outline: none;
    background: transparent;
    background-color: transparent;
}

.history-card {
    border-radius: 8px;
    padding: 6px 8px 6px 12px;
    margin: 2px 8px;
    background-color: rgba(255, 255, 255, 0.035);
    border: 1px solid rgba(255, 255, 255, 0.05);
    transition: all 120ms ease;
}

.history-card:hover {
    background-color: rgba(255, 255, 255, 0.07);
    border-color: rgba(255, 255, 255, 0.1);
}

.history-card:selected,
.history-card:focus,
listbox row:selected .history-card {
    background-color: rgba(96, 205, 255, 0.13);
    border-color: #60cdff;
    box-shadow: 0 0 0 1px rgba(96, 205, 255, 0.35);
}

/* Card Content Typography */
.card-content-text {
    font-size: 12.5px;
    color: #f4f4f5;
    line-height: 1.35;
    font-weight: 400;
}

.card-meta-text {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.38);
    font-weight: 500;
}

/* Vertical Actions Column on the far right */
.card-vertical-actions {
    margin-left: 6px;
    opacity: 0.45;
    transition: opacity 120ms ease;
}

.history-card:hover .card-vertical-actions,
listbox row:selected .card-vertical-actions {
    opacity: 1.0;
}

/* Card Action Buttons */
.card-action-btn {
    border-radius: 4px;
    padding: 1px 2px;
    min-width: 18px;
    min-height: 18px;
    color: rgba(255, 255, 255, 0.75);
    background: transparent;
    border: none;
    box-shadow: none;
    transition: all 120ms ease;
}

.card-action-btn image {
    -gtk-icon-size: 13px;
}

.card-action-btn:hover {
    opacity: 1.0;
    color: #ffffff;
    background-color: rgba(255, 255, 255, 0.15);
}

.card-action-btn.pin-active,
button.pin-active,
.pin-active {
    color: #ffd60a;
    opacity: 1.0;
    background-color: rgba(255, 214, 10, 0.16);
}

.card-action-btn.pin-active image,
button.pin-active image,
.pin-active image {
    color: #ffd60a;
    opacity: 1.0;
}

.pinned-tag {
    font-size: 10px;
    font-weight: 600;
    color: #ffd60a;
    background-color: rgba(255, 214, 10, 0.15);
    border: 1px solid rgba(255, 214, 10, 0.3);
    border-radius: 4px;
    padding: 1px 6px;
    margin-right: 4px;
}

.url-badge {
    font-size: 9.5px;
    font-weight: 600;
    color: #4cc2ff;
    background-color: rgba(76, 194, 255, 0.12);
    border: 1px solid rgba(76, 194, 255, 0.25);
    border-radius: 4px;
    padding: 0 5px;
    margin-right: 4px;
}

.code-badge {
    font-size: 9.5px;
    font-weight: 600;
    color: #68d391;
    background-color: rgba(104, 211, 145, 0.12);
    border: 1px solid rgba(104, 211, 145, 0.25);
    border-radius: 4px;
    padding: 0 5px;
    margin-right: 4px;
}

.color-badge {
    font-size: 9.5px;
    font-weight: 600;
    color: #f6ad55;
    background-color: rgba(246, 173, 85, 0.12);
    border: 1px solid rgba(246, 173, 85, 0.25);
    border-radius: 4px;
    padding: 0 5px;
    margin-right: 4px;
}

.snippet-badge {
    font-size: 9.5px;
    font-weight: 600;
    color: #b794f4;
    background-color: rgba(183, 148, 244, 0.12);
    border: 1px solid rgba(183, 148, 244, 0.25);
    border-radius: 4px;
    padding: 0 5px;
    margin-right: 4px;
}

/* Section Header (📌 Pinned (N) / History) */
.section-header {
    font-size: 11px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.45);
    letter-spacing: 0.3px;
    padding: 6px 14px 3px 14px;
}

.section-header-btn {
    background: transparent;
    border: none;
    box-shadow: none;
    font-size: 11px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.55);
    letter-spacing: 0.3px;
    padding: 3px 8px;
    margin: 2px 6px;
    border-radius: 6px;
    transition: all 120ms ease;
}

.section-header-btn:hover {
    color: #ffd60a;
    background-color: rgba(255, 255, 255, 0.07);
}

/* Windows 11 Subhead (Clipboard Title + Clear all Button) */
.win11-subhead {
    padding: 4px 14px 4px 14px;
}

.win11-title {
    font-size: 13.5px;
    font-weight: 600;
    color: #f4f4f5;
}

.win11-clear-btn {
    border-radius: 6px;
    padding: 3px 8px;
    font-size: 11.5px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.65);
    background: transparent;
    border: 1px solid transparent;
    transition: all 120ms ease;
}

.win11-clear-btn:hover {
    background-color: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.12);
    color: #ffffff;
}

.win11-clear-btn:active {
    background-color: rgba(255, 255, 255, 0.14);
}



/* Image Thumbnail Preview */
.thumbnail-preview {
    border-radius: 7px;
    border: 1px solid rgba(255, 255, 255, 0.09);
    margin-bottom: 4px;
    background-color: rgba(0, 0, 0, 0.3);
}

/* Modern Minimalist Scrollbar */
scrollbar {
    background: transparent;
    border: none;
}

scrollbar slider {
    min-width: 4px;
    min-height: 4px;
    border-radius: 2px;
    background-color: rgba(255, 255, 255, 0.14);
    transition: all 140ms ease;
}

scrollbar slider:hover {
    background-color: rgba(255, 255, 255, 0.35);
    min-width: 6px;
}

/* Modern Shortcut Footer (HUD Style) */
.modern-footer {
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    background-color: rgba(0, 0, 0, 0.3);
    padding: 6px 14px;
}

.footer-key-badge {
    background-color: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 10px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.7);
    margin-right: 3px;
}

.footer-text {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    margin-right: 12px;
}

/* StatusPage (Empty State) */
statuspage {
    padding: 24px 16px;
}

statuspage .title {
    font-weight: 600;
    font-size: 15px;
    color: #ffffff;
}

statuspage .description {
    font-size: 12.5px;
    color: rgba(255, 255, 255, 0.55);
}

/* Modern Quick Transforms & Actions Popover */
popover.transforms-popover,
.transforms-popover {
    background-color: #1a1a1e;
    color: #f4f4f5;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.85);
}

popover.transforms-popover > contents {
    background-color: #1a1a1e;
    border-radius: 12px;
    padding: 6px;
    border: 1px solid rgba(255, 255, 255, 0.12);
}

/* Popover Meta Card (Source App, Time, Details) */
.popover-meta-card {
    background-color: rgba(255, 255, 255, 0.045);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    padding: 8px 12px;
    margin-bottom: 4px;
}

.meta-app-name {
    font-size: 12.5px;
    font-weight: 600;
    color: #60cdff;
}

.meta-time-text {
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.45);
}

.meta-details-text {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.6);
    font-family: monospace;
    margin-top: 2px;
}

.popover-section-title {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: rgba(255, 255, 255, 0.4);
    margin-top: 6px;
    margin-bottom: 2px;
    text-transform: uppercase;
}

.popover-action-btn {
    border-radius: 6px;
    padding: 5px 10px;
    font-size: 11.5px;
    font-weight: 500;
    color: #f4f4f5;
    background-color: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: none;
    transition: all 120ms ease;
}

.popover-action-btn:hover {
    background-color: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
    color: #ffffff;
}

.popover-action-btn.suggested {
    background-color: rgba(96, 205, 255, 0.14);
    border-color: rgba(96, 205, 255, 0.35);
    color: #60cdff;
}

.popover-action-btn.suggested:hover {
    background-color: rgba(96, 205, 255, 0.26);
    border-color: #60cdff;
    color: #ffffff;
}

.popover-transform-btn {
    border-radius: 5px;
    padding: 4px 6px;
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.82);
    background-color: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.06);
    box-shadow: none;
    transition: all 120ms ease;
}

.popover-transform-btn:hover {
    background-color: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.18);
    color: #ffffff;
}

/* Clear Confirmation Popover */
.clear-confirm-popover > contents {
    background-color: #1e1e20;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.55);
    padding: 2px;
}

.confirm-title {
    font-size: 13.5px;
    font-weight: 600;
    color: #ffffff;
}

.confirm-desc {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.65);
    line-height: 1.35;
}

.confirm-checkbox {
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.88);
    margin-top: 2px;
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

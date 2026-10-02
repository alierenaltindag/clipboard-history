#[cfg(feature = "gtk")]
use crate::injector::InjectorCascade;
#[cfg(feature = "gtk")]
use crate::list_row::EntryRow;
#[cfg(feature = "gtk")]
use crate::search::{CategoryFilter, FuzzySearchEngine};
#[cfg(feature = "gtk")]
use clipboard_history_core::config::AppConfig;
#[cfg(feature = "gtk")]
use clipboard_history_core::domain::ClipboardEntry;
#[cfg(feature = "gtk")]
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
#[cfg(feature = "gtk")]
use gdk4::Key;
#[cfg(feature = "gtk")]
use gtk4::prelude::*;
#[cfg(feature = "gtk")]
use gtk4::{
    Align, Box as GtkBox, Button, CssProvider, EventControllerKey, HeaderBar, ListBox, Orientation,
    ScrolledWindow, SearchEntry, Window,
};
#[cfg(feature = "gtk")]
use std::rc::Rc;
#[cfg(feature = "gtk")]
use std::sync::Arc;
#[cfg(feature = "gtk")]
use tokio::runtime::Handle;
#[cfg(feature = "gtk")]
use tracing::info;

#[cfg(feature = "gtk")]
pub struct ClipboardWindow {
    pub window: Window,
}

#[cfg(feature = "gtk")]
impl ClipboardWindow {
    pub fn new(app: &gtk4::Application, config: AppConfig) -> Self {
        let window = Window::builder()
            .application(app)
            .title("Clipboard History")
            .icon_name("clipboard-history")
            .default_width(config.ui.window_width as i32)
            .default_height(config.ui.window_height as i32)
            .hide_on_close(true)
            .build();

        // Load modern CSS styles
        let provider = CssProvider::new();
        provider.load_from_data(
            r#"
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
            "#,
        );
        gtk4::style_context_add_provider_for_display(
            &gdk4::Display::default().unwrap(),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let main_box = GtkBox::new(Orientation::Vertical, 6);

        // Header Bar
        let header = HeaderBar::new();
        header.set_show_title_buttons(false);

        let title_box = GtkBox::new(Orientation::Horizontal, 8);
        let app_icon = gtk4::Image::from_icon_name("clipboard-history");
        app_icon.set_pixel_size(20);
        let title_label = gtk4::Label::new(Some("Clipboard History"));
        title_label.add_css_class("heading");
        title_box.append(&app_icon);
        title_box.append(&title_label);
        header.set_title_widget(Some(&title_box));

        let clear_btn = Button::from_icon_name("edit-clear-all-symbolic");
        clear_btn.set_tooltip_text(Some("Clear unpinned history"));
        header.pack_end(&clear_btn);

        main_box.append(&header);

        // Search Entry
        let search_entry = SearchEntry::new();
        search_entry.set_margin_start(10);
        search_entry.set_margin_end(10);
        search_entry.set_placeholder_text(Some("Type to search history (Win+V)..."));
        main_box.append(&search_entry);

        // Filter Bar (Categories)
        let filter_box = GtkBox::new(Orientation::Horizontal, 4);
        filter_box.set_halign(Align::Center);
        filter_box.set_margin_top(4);
        filter_box.set_margin_bottom(4);

        let all_btn = Button::with_label("All");
        let text_btn = Button::with_label("Text");
        let img_btn = Button::with_label("Images");
        let files_btn = Button::with_label("Files");
        let code_btn = Button::with_label("Code");
        let pinned_btn = Button::with_label("📌 Pinned");

        all_btn.add_css_class("flat");
        text_btn.add_css_class("flat");
        img_btn.add_css_class("flat");
        files_btn.add_css_class("flat");
        code_btn.add_css_class("flat");
        pinned_btn.add_css_class("flat");

        filter_box.append(&all_btn);
        filter_box.append(&text_btn);
        filter_box.append(&img_btn);
        filter_box.append(&files_btn);
        filter_box.append(&code_btn);
        filter_box.append(&pinned_btn);

        main_box.append(&filter_box);

        // Scrolled List
        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);

        let list_box = ListBox::new();
        list_box.set_selection_mode(gtk4::SelectionMode::Single);
        list_box.add_css_class("navigation-sidebar");
        scrolled.set_child(Some(&list_box));

        main_box.append(&scrolled);
        window.set_child(Some(&main_box));

        // Keyboard Navigation Controller
        let key_controller = EventControllerKey::new();
        let win_clone = window.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == Key::Escape {
                win_clone.set_visible(false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(key_controller);

        Self { window }
    }

    pub fn present_near_cursor(&self) {
        // Position window cleanly on screen
        self.window.present();
    }
}

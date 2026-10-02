#![cfg(feature = "gtk")]

use crate::injector::InjectorCascade;
use crate::list_row::EntryRow;
use crate::search::{CategoryFilter, FuzzySearchEngine};
use crate::transforms_dialog::show_transforms_popover;
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use clipboard_history_core::transforms::TextTransforms;
use gdk4::Key;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, CssProvider, EventControllerKey, HeaderBar, ListBox, Orientation,
    ScrolledWindow, SearchEntry, Window,
};
use std::cell::RefCell;
use std::rc::Rc;
use tracing::info;

#[cfg(feature = "gtk")]
pub struct ClipboardWindow {
    pub window: Window,
    config: AppConfig,
    list_box: ListBox,
    entries: Rc<RefCell<Vec<ClipboardEntry>>>,
    filtered_entries: Rc<RefCell<Vec<ClipboardEntry>>>,
    current_filter: Rc<RefCell<CategoryFilter>>,
    current_query: Rc<RefCell<String>>,
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

        // Modern acrylic card CSS styles
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

        let settings_btn = Button::from_icon_name("preferences-system-symbolic");
        settings_btn.set_tooltip_text(Some("Preferences"));
        header.pack_end(&settings_btn);

        let clear_btn = Button::from_icon_name("edit-clear-all-symbolic");
        clear_btn.set_tooltip_text(Some("Clear unpinned history"));
        header.pack_end(&clear_btn);

        let win_for_settings = window.clone();
        let cfg_for_settings = config.clone();
        settings_btn.connect_clicked(move |_| {
            crate::settings_dialog::SettingsDialog::show(
                &win_for_settings,
                cfg_for_settings.clone(),
            );
        });

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

        all_btn.add_css_class("filter-active");
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

        let entries: Rc<RefCell<Vec<ClipboardEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let filtered_entries: Rc<RefCell<Vec<ClipboardEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let current_filter = Rc::new(RefCell::new(CategoryFilter::All));
        let current_query = Rc::new(RefCell::new(String::new()));

        // Helper to update list rows
        let populate_rows = {
            let list_box_clone = list_box.clone();
            let filtered_clone = Rc::clone(&filtered_entries);
            let win_clone = window.clone();
            let config_clone = config.clone();

            Rc::new(move || {
                // Clear existing
                while let Some(child) = list_box_clone.first_child() {
                    list_box_clone.remove(&child);
                }

                let items = filtered_clone.borrow();
                for (idx, entry) in items.iter().enumerate() {
                    let entry_row = EntryRow::new(entry, idx);

                    // Actions / Transforms button
                    let entry_for_popover = entry.clone();
                    let win_for_pop = win_clone.clone();
                    let cfg_for_pop = config_clone.clone();
                    entry_row.actions_btn.connect_clicked(move |btn| {
                        show_transforms_popover(
                            btn,
                            &entry_for_popover,
                            &win_for_pop,
                            &cfg_for_pop,
                        );
                    });

                    // Pin button
                    let id_for_pin = entry.id.clone();
                    let is_pinned = entry.is_pinned;
                    entry_row.pin_btn.connect_clicked(move |_| {
                        let id = id_for_pin.clone();
                        glib::MainContext::default().spawn_local(async move {
                            if let Ok(mut client) = IpcClient::connect().await {
                                let req = if is_pinned {
                                    IpcRequest::UnpinEntry { id }
                                } else {
                                    IpcRequest::PinEntry { id }
                                };
                                let _ = client.send(&req).await;
                            }
                        });
                    });

                    // Delete button
                    let id_for_del = entry.id.clone();
                    entry_row.delete_btn.connect_clicked(move |_| {
                        let id = id_for_del.clone();
                        glib::MainContext::default().spawn_local(async move {
                            if let Ok(mut client) = IpcClient::connect().await {
                                let _ = client.send(&IpcRequest::DeleteEntry { id }).await;
                            }
                        });
                    });

                    list_box_clone.append(&entry_row.row);
                }
            })
        };

        // Helper to re-filter and populate
        let filter_and_render = {
            let entries_clone = Rc::clone(&entries);
            let filtered_clone = Rc::clone(&filtered_entries);
            let current_filter_clone = Rc::clone(&current_filter);
            let current_query_clone = Rc::clone(&current_query);
            let pop_fn = Rc::clone(&populate_rows);

            Rc::new(move || {
                let searcher = FuzzySearchEngine::new();
                let all_items = entries_clone.borrow();
                let q = current_query_clone.borrow();
                let cat = *current_filter_clone.borrow();
                let results = searcher.filter_entries(&all_items, &q, cat);
                *filtered_clone.borrow_mut() = results;
                pop_fn();
            })
        };

        // Filter button clicks
        let make_filter_handler =
            |cat: CategoryFilter, btn: Button, filter_box_ref: GtkBox, fn_ref: Rc<dyn Fn()>| {
                let cur_filt = Rc::clone(&current_filter);
                move |_| {
                    *cur_filt.borrow_mut() = cat;
                    // update CSS classes
                    let mut child = filter_box_ref.first_child();
                    while let Some(c) = child {
                        c.remove_css_class("filter-active");
                        c.add_css_class("flat");
                        child = c.next_sibling();
                    }
                    btn.remove_css_class("flat");
                    btn.add_css_class("filter-active");
                    fn_ref();
                }
            };

        all_btn.connect_clicked(make_filter_handler(
            CategoryFilter::All,
            all_btn.clone(),
            filter_box.clone(),
            Rc::clone(&filter_and_render),
        ));
        text_btn.connect_clicked(make_filter_handler(
            CategoryFilter::Text,
            text_btn.clone(),
            filter_box.clone(),
            Rc::clone(&filter_and_render),
        ));
        img_btn.connect_clicked(make_filter_handler(
            CategoryFilter::Images,
            img_btn.clone(),
            filter_box.clone(),
            Rc::clone(&filter_and_render),
        ));
        files_btn.connect_clicked(make_filter_handler(
            CategoryFilter::Files,
            files_btn.clone(),
            filter_box.clone(),
            Rc::clone(&filter_and_render),
        ));
        code_btn.connect_clicked(make_filter_handler(
            CategoryFilter::Code,
            code_btn.clone(),
            filter_box.clone(),
            Rc::clone(&filter_and_render),
        ));
        pinned_btn.connect_clicked(make_filter_handler(
            CategoryFilter::Pinned,
            pinned_btn.clone(),
            filter_box.clone(),
            Rc::clone(&filter_and_render),
        ));

        // Search entry live typing
        let fn_for_search = Rc::clone(&filter_and_render);
        let q_ref = Rc::clone(&current_query);
        search_entry.connect_search_changed(move |entry| {
            *q_ref.borrow_mut() = entry.text().to_string();
            fn_for_search();
        });

        // Clear button
        let fn_for_clear = Rc::clone(&filter_and_render);
        clear_btn.connect_clicked(move |_| {
            let fn_call = Rc::clone(&fn_for_clear);
            glib::MainContext::default().spawn_local(async move {
                if let Ok(mut client) = IpcClient::connect().await {
                    let _ = client
                        .send(&IpcRequest::ClearHistory {
                            delete_pinned: false,
                        })
                        .await;
                    fn_call();
                }
            });
        });

        // Row Activated: Paste item
        let filtered_for_act = Rc::clone(&filtered_entries);
        let win_for_act = window.clone();
        let cfg_for_act = config.clone();
        list_box.connect_row_activated(move |_, row| {
            let idx = row.index() as usize;
            let items = filtered_for_act.borrow();
            if let Some(entry) = items.get(idx) {
                let text = entry.preview.clone();
                if let Some(display) = gdk4::Display::default() {
                    display.clipboard().set_text(&text);
                }
                win_for_act.set_visible(false);
                if cfg_for_act.paste.auto_paste {
                    glib::MainContext::default().spawn_local(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        let cascade = InjectorCascade::new();
                        let (name, success) = cascade.execute_paste().await;
                        info!("Paste executed: injector={}, success={}", name, success);
                    });
                }
            }
        });

        // Keyboard Navigation Controller (Shift+Enter, Ctrl+T, Escape, Del, P, 1..9)
        let key_controller = EventControllerKey::new();
        let win_key = window.clone();
        let list_box_key = list_box.clone();
        let filtered_key = Rc::clone(&filtered_entries);
        let cfg_key = config.clone();

        key_controller.connect_key_pressed(move |_, key, _, state| {
            // Dismiss
            if key == Key::Escape {
                win_key.set_visible(false);
                return glib::Propagation::Stop;
            }

            // Quick Transforms Popover (Ctrl+T)
            if (key == Key::t || key == Key::T) && state.contains(gdk4::ModifierType::CONTROL_MASK)
            {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let idx = selected_row.index() as usize;
                    let items = filtered_key.borrow();
                    if let Some(entry) = items.get(idx) {
                        show_transforms_popover(&selected_row, entry, &win_key, &cfg_key);
                        return glib::Propagation::Stop;
                    }
                }
            }

            // Paste as Plain Text (Shift + Return / Shift + KP_Enter)
            if (key == Key::Return || key == Key::KP_Enter)
                && state.contains(gdk4::ModifierType::SHIFT_MASK)
            {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let idx = selected_row.index() as usize;
                    let items = filtered_key.borrow();
                    if let Some(entry) = items.get(idx) {
                        let plain_text = TextTransforms::strip_formatting(&entry.preview);
                        if let Some(display) = gdk4::Display::default() {
                            display.clipboard().set_text(&plain_text);
                        }
                        win_key.set_visible(false);
                        let auto_paste = cfg_key.paste.auto_paste;
                        glib::MainContext::default().spawn_local(async move {
                            if auto_paste {
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                let cascade = InjectorCascade::new();
                                let (name, success) = cascade.execute_paste().await;
                                info!(
                                    "Plain text paste executed: injector={}, success={}",
                                    name, success
                                );
                            }
                        });
                        return glib::Propagation::Stop;
                    }
                }
            }

            // Quick 1..9 Pasting
            if !state.contains(gdk4::ModifierType::CONTROL_MASK)
                && !state.contains(gdk4::ModifierType::ALT_MASK)
            {
                let digit_opt = match key {
                    Key::_1 => Some(0),
                    Key::_2 => Some(1),
                    Key::_3 => Some(2),
                    Key::_4 => Some(3),
                    Key::_5 => Some(4),
                    Key::_6 => Some(5),
                    Key::_7 => Some(6),
                    Key::_8 => Some(7),
                    Key::_9 => Some(8),
                    _ => None,
                };
                if let Some(digit) = digit_opt {
                    let items = filtered_key.borrow();
                    if let Some(entry) = items.get(digit) {
                        let text = entry.preview.clone();
                        if let Some(display) = gdk4::Display::default() {
                            display.clipboard().set_text(&text);
                        }
                        win_key.set_visible(false);
                        let auto_paste = cfg_key.paste.auto_paste;
                        glib::MainContext::default().spawn_local(async move {
                            if auto_paste {
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                let cascade = InjectorCascade::new();
                                let _ = cascade.execute_paste().await;
                            }
                        });
                        return glib::Propagation::Stop;
                    }
                }
            }

            glib::Propagation::Proceed
        });
        window.add_controller(key_controller);

        Self {
            window,
            config,
            list_box,
            entries,
            filtered_entries,
            current_filter,
            current_query,
        }
    }

    /// Fetches the latest clipboard entries from daemon via IPC and clamps window within active monitor workarea
    pub fn present_near_cursor(&self) {
        // Multi-Monitor Geometry Clamping
        if let Some(display) = gdk4::Display::default() {
            let monitors = display.monitors();
            if let Some(mon_obj) = monitors.item(0) {
                if let Ok(mon) = mon_obj.downcast::<gdk4::Monitor>() {
                    let geom = mon.geometry();
                    let max_width = (geom.width() - 40).max(320);
                    let max_height = (geom.height() - 80).max(400);

                    let target_w = (self.config.ui.window_width as i32).min(max_width);
                    let target_h = (self.config.ui.window_height as i32).min(max_height);
                    self.window.set_default_size(target_w, target_h);
                }
            }
        }

        // Fetch latest entries via IPC
        let entries_cell = Rc::clone(&self.entries);
        let filtered_cell = Rc::clone(&self.filtered_entries);
        let list_box = self.list_box.clone();
        let win = self.window.clone();
        let cfg = self.config.clone();

        glib::MainContext::default().spawn_local(async move {
            if let Ok(mut client) = IpcClient::connect().await {
                let req = IpcRequest::ListEntries {
                    limit: 100,
                    offset: 0,
                    entry_type: None,
                    only_pinned: false,
                };
                if let Ok(IpcResponse::EntryList(items)) = client.send(&req).await {
                    *entries_cell.borrow_mut() = items.clone();
                    *filtered_cell.borrow_mut() = items.clone();

                    while let Some(child) = list_box.first_child() {
                        list_box.remove(&child);
                    }

                    for (idx, entry) in items.iter().enumerate() {
                        let entry_row = EntryRow::new(entry, idx);

                        let e_pop = entry.clone();
                        let w_pop = win.clone();
                        let c_pop = cfg.clone();
                        entry_row.actions_btn.connect_clicked(move |btn| {
                            show_transforms_popover(btn, &e_pop, &w_pop, &c_pop);
                        });

                        list_box.append(&entry_row.row);
                    }
                }
            }
        });

        self.window.present();
    }
}

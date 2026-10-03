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
    Align, Box as GtkBox, Button, EventControllerKey, HeaderBar, Label, ListBox,
    Orientation, ScrolledWindow, SearchEntry, Window,
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
    #[allow(dead_code)]
    current_filter: Rc<RefCell<CategoryFilter>>,
    #[allow(dead_code)]
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

        // Apply modern acrylic card CSS styles from style module
        crate::style::apply_application_styles();

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

        let select_mode_btn = Button::from_icon_name("selection-mode-symbolic");
        select_mode_btn.set_tooltip_text(Some("Toggle Multi-Selection Mode (Ctrl+M)"));
        header.pack_start(&select_mode_btn);

        let add_snippet_btn = Button::from_icon_name("list-add-symbolic");
        add_snippet_btn.set_tooltip_text(Some("Add Canned Snippet"));
        header.pack_end(&add_snippet_btn);

        let queue_hud_btn = Button::from_icon_name("media-playlist-consecutive-symbolic");
        queue_hud_btn.set_tooltip_text(Some("Paste Next Item from Queue"));
        header.pack_end(&queue_hud_btn);

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
        let snippets_btn = Button::with_label("📝 Snippets");

        all_btn.add_css_class("filter-active");
        text_btn.add_css_class("flat");
        img_btn.add_css_class("flat");
        files_btn.add_css_class("flat");
        code_btn.add_css_class("flat");
        pinned_btn.add_css_class("flat");
        snippets_btn.add_css_class("flat");

        filter_box.append(&all_btn);
        filter_box.append(&text_btn);
        filter_box.append(&img_btn);
        filter_box.append(&files_btn);
        filter_box.append(&code_btn);
        filter_box.append(&pinned_btn);
        filter_box.append(&snippets_btn);

        main_box.append(&filter_box);

        // Scrolled List
        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);

        let list_box = ListBox::new();
        list_box.set_selection_mode(gtk4::SelectionMode::Single);
        list_box.add_css_class("navigation-sidebar");
        scrolled.set_child(Some(&list_box));

        // Multi-Selection Action Bar
        let action_bar = gtk4::ActionBar::new();
        action_bar.set_revealed(false);

        let selected_count_label = Label::new(Some("0 selected"));
        selected_count_label.add_css_class("heading");
        selected_count_label.set_margin_start(8);
        action_bar.pack_start(&selected_count_label);

        let join_paste_btn = Button::with_label("📋 Join & Paste");
        join_paste_btn.add_css_class("suggested-action");
        action_bar.pack_end(&join_paste_btn);

        let diff_btn = Button::with_label("🔀 Compare Diff");
        diff_btn.set_sensitive(false);
        diff_btn.set_visible(false);
        action_bar.pack_end(&diff_btn);

        let enqueue_all_btn = Button::with_label("🔁 Enqueue All");
        action_bar.pack_end(&enqueue_all_btn);

        let pin_all_btn = Button::with_label("📌 Pin All");
        action_bar.pack_end(&pin_all_btn);

        let delete_selected_btn = Button::with_label("🗑️ Delete");
        delete_selected_btn.add_css_class("destructive-action");
        action_bar.pack_end(&delete_selected_btn);

        main_box.append(&scrolled);
        main_box.append(&action_bar);
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
                    let entry_row = create_configured_entry_row(entry, idx, &win_clone, &config_clone);
                    list_box_clone.append(&entry_row.row);
                }
            })
        };

        // Helper to re-filter and populate
        let filter_and_render: Rc<dyn Fn()> = {
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
                move |_: &gtk4::Button| {
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
        snippets_btn.connect_clicked(make_filter_handler(
            CategoryFilter::Snippets,
            snippets_btn.clone(),
            filter_box.clone(),
            Rc::clone(&filter_and_render),
        ));

        let win_for_snip = window.clone();
        let fn_for_snip = Rc::clone(&filter_and_render);
        add_snippet_btn.connect_clicked(move |_| {
            let tr = Rc::clone(&fn_for_snip);
            crate::snippet_dialog::SnippetDialog::show(&win_for_snip, move || {
                tr();
            });
        });

        let win_for_q = window.clone();
        queue_hud_btn.connect_clicked(move |_| {
            let win = win_for_q.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = IpcClient::connect().await {
                    if let Ok(IpcResponse::QueuePopped {
                        remaining_count,
                        pasted,
                        text,
                    }) = client.send(&IpcRequest::PopAndPasteQueue).await
                    {
                        if pasted {
                            if let Some(popped_text) = text {
                                if let Some(display) = gdk4::Display::default() {
                                    display.clipboard().set_text(&popped_text);
                                }
                            }
                            win.set_visible(false);
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            let cascade = InjectorCascade::new();
                            let _ = cascade.execute_paste().await;
                            info!("Pasted sequential item, {} remaining", remaining_count);
                        }
                    }
                }
            });
        });

        // Search entry live typing with 150ms debounce (HIGH-10)
        let fn_for_search = Rc::clone(&filter_and_render);
        let q_ref = Rc::clone(&current_query);
        let debounce_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let debounce_clone = Rc::clone(&debounce_source);
        search_entry.connect_search_changed(move |entry| {
            *q_ref.borrow_mut() = entry.text().to_string();
            if let Some(source) = debounce_clone.borrow_mut().take() {
                source.remove();
            }
            let fn_call = Rc::clone(&fn_for_search);
            let deb_ref = Rc::clone(&debounce_clone);
            let source_id = glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                deb_ref.borrow_mut().take();
                fn_call();
            });
            *debounce_clone.borrow_mut() = Some(source_id);
        });

        // Clear button
        let fn_for_clear = Rc::clone(&filter_and_render);
        clear_btn.connect_clicked(move |_| {
            let fn_call = Rc::clone(&fn_for_clear);
            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = IpcClient::connect().await {
                    let _ = client
                        .send(&IpcRequest::ClearHistory {
                            include_pinned: false,
                        })
                        .await;
                    fn_call();
                }
            });
        });

        // Multi-Selection State & Handlers
        let is_multi_select = Rc::new(RefCell::new(false));

        let is_multi_for_toggle = is_multi_select.clone();
        let list_box_for_toggle = list_box.clone();
        let select_btn_for_toggle = select_mode_btn.clone();
        let action_bar_for_toggle = action_bar.clone();
        let selected_count_for_toggle = selected_count_label.clone();
        let join_btn_for_toggle = join_paste_btn.clone();
        let enq_btn_for_toggle = enqueue_all_btn.clone();
        let pin_btn_for_toggle = pin_all_btn.clone();
        let del_btn_for_toggle = delete_selected_btn.clone();
        let diff_btn_for_toggle = diff_btn.clone();

        let toggle_selection_mode = Rc::new(move || {
            let mut active = is_multi_for_toggle.borrow_mut();
            *active = !*active;
            if *active {
                list_box_for_toggle.set_selection_mode(gtk4::SelectionMode::Multiple);
                select_btn_for_toggle.add_css_class("suggested-action");
                action_bar_for_toggle.set_revealed(true);
                selected_count_for_toggle.set_text("0 selected");
                join_btn_for_toggle.set_sensitive(false);
                enq_btn_for_toggle.set_sensitive(false);
                pin_btn_for_toggle.set_sensitive(false);
                del_btn_for_toggle.set_sensitive(false);
                diff_btn_for_toggle.set_sensitive(false);
                diff_btn_for_toggle.set_visible(false);
            } else {
                list_box_for_toggle.unselect_all();
                list_box_for_toggle.set_selection_mode(gtk4::SelectionMode::Single);
                select_btn_for_toggle.remove_css_class("suggested-action");
                action_bar_for_toggle.set_revealed(false);
            }
        });

        let toggle_for_click = toggle_selection_mode.clone();
        select_mode_btn.connect_clicked(move |_| {
            toggle_for_click();
        });

        // Update action bar buttons when rows are selected
        let is_multi_for_sel = is_multi_select.clone();
        let list_box_for_sel = list_box.clone();
        let count_lbl_for_sel = selected_count_label.clone();
        let join_btn_for_sel = join_paste_btn.clone();
        let diff_btn_for_sel = diff_btn.clone();
        let enq_btn_for_sel = enqueue_all_btn.clone();
        let pin_btn_for_sel = pin_all_btn.clone();
        let del_btn_for_sel = delete_selected_btn.clone();

        list_box.connect_selected_rows_changed(move |_| {
            if !*is_multi_for_sel.borrow() {
                return;
            }
            let rows = list_box_for_sel.selected_rows();
            let count = rows.len();
            count_lbl_for_sel.set_text(&format!("{} selected", count));
            let has_sel = count > 0;
            join_btn_for_sel.set_sensitive(has_sel);
            enq_btn_for_sel.set_sensitive(has_sel);
            pin_btn_for_sel.set_sensitive(has_sel);
            del_btn_for_sel.set_sensitive(has_sel);
            diff_btn_for_sel.set_sensitive(count == 2);
            diff_btn_for_sel.set_visible(count == 2);
        });

        // 📋 Join & Paste Popover
        let popover = gtk4::Popover::new();
        let pop_box = GtkBox::new(Orientation::Vertical, 4);
        pop_box.set_margin_start(8);
        pop_box.set_margin_end(8);
        pop_box.set_margin_top(8);
        pop_box.set_margin_bottom(8);

        let pop_header = Label::new(Some("Join with delimiter:"));
        pop_header.add_css_class("heading");
        pop_header.set_halign(Align::Start);
        pop_box.append(&pop_header);

        let delimiters = [
            (
                clipboard_history_core::transforms::ConcatDelimiter::Newline,
                "Newlines (\\n)",
            ),
            (
                clipboard_history_core::transforms::ConcatDelimiter::DoubleNewline,
                "Paragraphs (\\n\\n)",
            ),
            (
                clipboard_history_core::transforms::ConcatDelimiter::Comma,
                "Comma (, )",
            ),
            (
                clipboard_history_core::transforms::ConcatDelimiter::Space,
                "Space ( )",
            ),
            (
                clipboard_history_core::transforms::ConcatDelimiter::Semicolon,
                "Semicolon (; )",
            ),
            (
                clipboard_history_core::transforms::ConcatDelimiter::NumberedList,
                "Numbered List (1. ...)",
            ),
            (
                clipboard_history_core::transforms::ConcatDelimiter::BulletList,
                "Bulleted List (- ...)",
            ),
        ];

        for (delim, label_str) in delimiters {
            let item_btn = Button::with_label(label_str);
            item_btn.add_css_class("flat");
            item_btn.set_halign(Align::Fill);

            let pop_close = popover.clone();
            let win_join = window.clone();
            let list_for_join = list_box.clone();
            let filtered_for_join = filtered_entries.clone();
            let cfg_for_join = config.clone();
            let toggle_exit = toggle_selection_mode.clone();

            item_btn.connect_clicked(move |_| {
                pop_close.popdown();
                let mut rows = list_for_join.selected_rows();
                rows.sort_by_key(|r| r.index());
                let items = filtered_for_join.borrow();
                let mut texts = Vec::new();
                for r in rows {
                    let idx = r.index() as usize;
                    if let Some(e) = items.get(idx) {
                        texts.push(e.text_content.clone().unwrap_or_else(|| e.preview.clone()));
                    }
                }
                if !texts.is_empty() {
                    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
                    let joined = clipboard_history_core::transforms::TextTransforms::concatenate(
                        &text_refs, &delim,
                    );
                    if let Some(display) = gdk4::Display::default() {
                        display.clipboard().set_text(&joined);
                    }
                    win_join.set_visible(false);
                    toggle_exit();
                    if cfg_for_join.paste.auto_paste {
                        glib::MainContext::default().spawn_local(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            let cascade = InjectorCascade::new();
                            let (name, success) = cascade.execute_paste().await;
                            info!(
                                "Concatenated paste executed: injector={}, success={}",
                                name, success
                            );
                        });
                    }
                }
            });
            pop_box.append(&item_btn);
        }

        popover.set_child(Some(&pop_box));
        popover.set_parent(&join_paste_btn);

        let pop_trigger = popover.clone();
        join_paste_btn.connect_clicked(move |_| {
            pop_trigger.popup();
        });

        // 🔀 Compare Diff Button
        let list_for_diff = list_box.clone();
        let filtered_for_diff = filtered_entries.clone();
        let win_diff = window.clone();
        diff_btn.connect_clicked(move |_| {
            let mut rows = list_for_diff.selected_rows();
            if rows.len() == 2 {
                rows.sort_by_key(|r| r.index());
                let idx_a = rows[0].index() as usize;
                let idx_b = rows[1].index() as usize;
                let items = filtered_for_diff.borrow();
                if let (Some(ea), Some(eb)) = (items.get(idx_a), items.get(idx_b)) {
                    let text_a = ea
                        .text_content
                        .clone()
                        .unwrap_or_else(|| ea.preview.clone());
                    let text_b = eb
                        .text_content
                        .clone()
                        .unwrap_or_else(|| eb.preview.clone());
                    let label_a = ea.preview.clone();
                    let label_b = eb.preview.clone();
                    crate::diff_dialog::DiffDialog::show(
                        &win_diff, text_a, text_b, label_a, label_b,
                    );
                }
            }
        });

        // 🔁 Enqueue All Button
        let list_for_enq = list_box.clone();
        let filtered_for_enq = filtered_entries.clone();
        let toggle_for_enq = toggle_selection_mode.clone();
        enqueue_all_btn.connect_clicked(move |_| {
            let rows = list_for_enq.selected_rows();
            let items = filtered_for_enq.borrow();
            let ids: Vec<String> = rows
                .iter()
                .filter_map(|r| items.get(r.index() as usize).map(|e| e.id.clone()))
                .collect();
            if !ids.is_empty() {
                let count = ids.len();
                glib::MainContext::default().spawn_local(async move {
                    if let Ok(client) = IpcClient::connect().await {
                        let _ = client.send(&IpcRequest::EnqueueItems { ids }).await;
                        info!("Enqueued {} batch items into paste queue", count);
                    }
                });
                toggle_for_enq();
            }
        });

        // 📌 Pin / Unpin All Button
        let list_for_pin = list_box.clone();
        let filtered_for_pin = filtered_entries.clone();
        let fn_refresh_for_pin = Rc::clone(&filter_and_render);
        let toggle_for_pin = toggle_selection_mode.clone();
        pin_all_btn.connect_clicked(move |_| {
            let rows = list_for_pin.selected_rows();
            let items = filtered_for_pin.borrow();
            let mut any_unpinned = false;
            let mut ids = Vec::new();
            for r in rows {
                if let Some(e) = items.get(r.index() as usize) {
                    ids.push(e.id.clone());
                    if !e.is_pinned {
                        any_unpinned = true;
                    }
                }
            }
            if !ids.is_empty() {
                let refresh = fn_refresh_for_pin.clone();
                let pin_flag = any_unpinned;
                let toggle_exit = toggle_for_pin.clone();
                glib::MainContext::default().spawn_local(async move {
                    if let Ok(client) = IpcClient::connect().await {
                        let _ = client
                            .send(&IpcRequest::BatchPin {
                                ids,
                                pinned: pin_flag,
                            })
                            .await;
                        refresh();
                    }
                });
                toggle_exit();
            }
        });

        // 🗑️ Delete Selected Button
        let list_for_del = list_box.clone();
        let filtered_for_del = filtered_entries.clone();
        let fn_refresh_for_del = Rc::clone(&filter_and_render);
        let toggle_for_del = toggle_selection_mode.clone();
        delete_selected_btn.connect_clicked(move |_| {
            let rows = list_for_del.selected_rows();
            let items = filtered_for_del.borrow();
            let ids: Vec<String> = rows
                .iter()
                .filter_map(|r| items.get(r.index() as usize).map(|e| e.id.clone()))
                .collect();
            if !ids.is_empty() {
                let refresh = fn_refresh_for_del.clone();
                let toggle_exit = toggle_for_del.clone();
                glib::MainContext::default().spawn_local(async move {
                    if let Ok(client) = IpcClient::connect().await {
                        let _ = client.send(&IpcRequest::BatchDelete { ids }).await;
                        refresh();
                    }
                });
                toggle_exit();
            }
        });

        // Row Activated: Paste item
        let filtered_for_act = Rc::clone(&filtered_entries);
        let win_for_act = window.clone();
        let cfg_for_act = config.clone();
        let is_multi_for_act = is_multi_select.clone();
        list_box.connect_row_activated(move |_, row| {
            if *is_multi_for_act.borrow() {
                return;
            }
            let idx = row.index() as usize;
            let items = filtered_for_act.borrow();
            if let Some(entry) = items.get(idx) {
                let mut text = entry.text_content.clone().unwrap_or_else(|| entry.preview.clone());
                if entry.source_app.as_deref() == Some("Snippet") {
                    let current_clip = items
                        .iter()
                        .find(|e| e.source_app.as_deref() != Some("Snippet"))
                        .and_then(|e| e.text_content.as_deref().or(Some(&e.preview)));
                    text = clipboard_history_core::transforms::SnippetExpander::expand(
                        &text,
                        current_clip,
                    );
                }
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

        // Keyboard Navigation Controller (Shift+Enter, Ctrl+T, Ctrl+M, Escape, Del, P, 1..9)
        let key_controller = EventControllerKey::new();
        let win_key = window.clone();
        let list_box_key = list_box.clone();
        let filtered_key = Rc::clone(&filtered_entries);
        let cfg_key = config.clone();
        let is_multi_key = is_multi_select.clone();
        let toggle_key = toggle_selection_mode.clone();

        key_controller.connect_key_pressed(move |_, key, _, state| {
            // Dismiss or Exit Selection Mode
            if key == Key::Escape {
                if *is_multi_key.borrow() {
                    toggle_key();
                    return glib::Propagation::Stop;
                }
                win_key.set_visible(false);
                return glib::Propagation::Stop;
            }

            // Toggle Multi-Selection Mode (Ctrl+M)
            if (key == Key::m || key == Key::M) && state.contains(gdk4::ModifierType::CONTROL_MASK)
            {
                toggle_key();
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
                        let raw_text = entry.text_content.as_deref().unwrap_or(&entry.preview);
                        let plain_text = TextTransforms::strip_formatting(raw_text);
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

            // Enqueue into Paste Queue (Q)
            if key == Key::q || key == Key::Q {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let idx = selected_row.index() as usize;
                    let items = filtered_key.borrow();
                    if let Some(entry) = items.get(idx) {
                        let id = entry.id.clone();
                        glib::MainContext::default().spawn_local(async move {
                            if let Ok(client) = IpcClient::connect().await {
                                let _ = client
                                    .send(&IpcRequest::EnqueueItems { ids: vec![id] })
                                    .await;
                                info!("Enqueued item into paste queue via 'Q' key");
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
                        let text = entry.text_content.clone().unwrap_or_else(|| entry.preview.clone());
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
            if let Ok(client) = IpcClient::connect().await {
                let req = IpcRequest::GetEntries {
                    limit: 100,
                    offset: 0,
                    filter: None,
                    pinned_only: false,
                };
                if let Ok(IpcResponse::Entries(mut items)) = client.send(&req).await {
                    if let Ok(IpcResponse::Snippets(snippets)) = client
                        .send(&IpcRequest::ListSnippets { category: None })
                        .await
                    {
                        for s in snippets {
                            items.push(ClipboardEntry {
                                id: s.id,
                                content_hash: format!("snippet_{}", s.label),
                                entry_type: clipboard_history_core::domain::EntryType::Text,
                                preview: format!("📝 {} — {}", s.label, s.content),
                                text_content: Some(s.content),
                                html_content: None,
                                blob_hash: None,
                                thumbnail_blob_hash: None,
                                mime_types: vec!["text/plain".to_string()],
                                size_bytes: s.label.len(),
                                created_at: s.created_at,
                                last_used_at: s.last_used_at,
                                is_pinned: true,
                                source_app: Some("Snippet".to_string()),
                            });
                        }
                    }

                    *entries_cell.borrow_mut() = items.clone();
                    *filtered_cell.borrow_mut() = items.clone();

                    while let Some(child) = list_box.first_child() {
                        list_box.remove(&child);
                    }

                    for (idx, entry) in items.iter().enumerate() {
                        let entry_row = create_configured_entry_row(entry, idx, &win, &cfg);
                        list_box.append(&entry_row.row);
                    }
                }
            }
        });

        self.window.present();
    }
}

/// Helper to construct and configure an EntryRow with full actions, pin, and delete button handlers
fn create_configured_entry_row(
    entry: &ClipboardEntry,
    index: usize,
    window: &Window,
    config: &AppConfig,
) -> EntryRow {
    let entry_row = EntryRow::new(entry, index);

    // Actions / Transforms button
    let entry_for_popover = entry.clone();
    let win_for_pop = window.clone();
    let cfg_for_pop = config.clone();
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
            if let Ok(client) = IpcClient::connect().await {
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
            if let Ok(client) = IpcClient::connect().await {
                let _ = client.send(&IpcRequest::DeleteEntry { id }).await;
            }
        });
    });

    entry_row
}

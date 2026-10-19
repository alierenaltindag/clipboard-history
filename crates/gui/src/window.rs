#![cfg(feature = "gtk")]

use crate::injector::InjectorCascade;
use crate::list_row::EntryRow;
use crate::search::{CategoryFilter, FuzzySearchEngine};
use crate::transforms_dialog::{
    show_transforms_popover, ActionCallback, EntryCallback, TransformsOptions,
};
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use clipboard_history_core::transforms::TextTransforms;
use gdk4::Key;
use gio::prelude::ActionMapExt;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, CheckButton, EventControllerKey, EventControllerMotion, GestureClick,
    GestureDrag, Label, ListBox, ListBoxRow, Orientation, Popover, PropagationPhase,
    ScrolledWindow, SearchEntry, Window, WindowHandle,
};
use std::cell::RefCell;
use std::rc::Rc;
use tracing::info;

#[cfg(feature = "gtk")]
pub struct ClipboardWindow {
    pub window: Window,
    config: AppConfig,
    #[allow(dead_code)]
    list_box: ListBox,
    entries: Rc<RefCell<Vec<ClipboardEntry>>>,
    #[allow(dead_code)]
    filtered_entries: Rc<RefCell<Vec<ClipboardEntry>>>,
    #[allow(dead_code)]
    current_filter: Rc<RefCell<CategoryFilter>>,
    #[allow(dead_code)]
    current_query: Rc<RefCell<String>>,
    #[allow(dead_code)]
    search_seq: Rc<RefCell<u64>>,
    filter_and_render: Rc<dyn Fn()>,
    daemon_connected: Rc<RefCell<bool>>,
    #[allow(dead_code)]
    toast_overlay: libadwaita::ToastOverlay,
}

#[cfg(feature = "gtk")]
impl ClipboardWindow {
    #[allow(clippy::type_complexity)]
    pub fn new(app: &impl IsA<gtk4::Application>, config: AppConfig) -> Self {
        let window = Window::builder()
            .application(app)
            .title("Clipboard History")
            .icon_name("clipboard-history")
            .default_width(config.ui.window_width as i32)
            .default_height(config.ui.window_height as i32)
            .hide_on_close(true)
            .decorated(false)
            .build();
        window.add_css_class("clipboard-window");

        // Apply modern CSS styles
        crate::style::apply_application_styles();

        let main_box = GtkBox::new(Orientation::Vertical, 0);
        main_box.add_css_class("main-box");

        // Header container wrapped in WindowHandle for native compositor window dragging
        let window_handle = WindowHandle::new();
        let header_box = GtkBox::new(Orientation::Vertical, 0);

        // 1. Windows 11 Top Category Tabs Bar with Settings Button
        let top_tab_container = GtkBox::new(Orientation::Horizontal, 4);
        top_tab_container.add_css_class("category-tabs-bar");
        top_tab_container.set_valign(gtk4::Align::Center);

        let tab_defs: [(CategoryFilter, &str, &str); 3] = [
            (CategoryFilter::All, "📋", "Clipboard (All)"),
            (CategoryFilter::Images, "🖼️", "Images"),
            (CategoryFilter::Code, "💻", "Code"),
        ];

        let mut tab_buttons: Vec<Button> = Vec::new();
        for (cat, icon, tooltip) in tab_defs.iter() {
            let btn = Button::with_label(icon);
            btn.set_tooltip_text(Some(tooltip));
            btn.add_css_class("category-tab");
            if *cat == CategoryFilter::All {
                btn.add_css_class("tab-active");
            }
            top_tab_container.append(&btn);
            tab_buttons.push(btn);
        }

        let top_spacer = GtkBox::new(Orientation::Horizontal, 0);
        top_spacer.set_hexpand(true);
        top_tab_container.append(&top_spacer);

        let settings_btn = Button::from_icon_name("preferences-system-symbolic");
        settings_btn.add_css_class("header-btn");
        settings_btn.set_tooltip_text(Some("Preferences"));
        top_tab_container.append(&settings_btn);

        header_box.append(&top_tab_container);

        let win_for_settings = window.clone();
        let cfg_for_settings = config.clone();
        settings_btn.connect_clicked(move |_| {
            win_for_settings.set_visible(false);
            crate::settings_dialog::SettingsDialog::show(None, cfg_for_settings.clone());
        });

        // 2. Windows 11 Subhead: Left "Clipboard" title, Right "Clear all" button
        let subhead_box = GtkBox::new(Orientation::Horizontal, 6);
        subhead_box.add_css_class("win11-subhead");
        subhead_box.set_valign(gtk4::Align::Center);

        let subhead_title = Label::new(Some("Clipboard"));
        subhead_title.add_css_class("win11-title");
        subhead_box.append(&subhead_title);

        let subhead_spacer = GtkBox::new(Orientation::Horizontal, 0);
        subhead_spacer.set_hexpand(true);
        subhead_box.append(&subhead_spacer);

        let clear_all_btn = Button::with_label("Clear all");
        clear_all_btn.add_css_class("win11-clear-btn");
        clear_all_btn.set_tooltip_text(Some("Clear history (Pinned items are preserved)"));
        subhead_box.append(&clear_all_btn);

        header_box.append(&subhead_box);

        window_handle.set_child(Some(&header_box));
        main_box.append(&window_handle);

        // 3. Update notification banner (shown when a newer version is available)
        let update_banner = GtkBox::new(Orientation::Horizontal, 8);
        update_banner.add_css_class("update-banner");
        update_banner.set_margin_start(10);
        update_banner.set_margin_end(10);
        update_banner.set_margin_top(4);
        update_banner.set_margin_bottom(4);
        update_banner.set_valign(gtk4::Align::Center);
        update_banner.set_visible(false);

        let update_icon = gtk4::Image::from_icon_name("system-software-update-symbolic");
        update_icon.set_pixel_size(16);
        update_banner.append(&update_icon);

        let update_label = Label::new(Some("Yeni sürüm mevcut!"));
        update_label.set_hexpand(true);
        update_label.set_xalign(0.0);
        update_banner.append(&update_label);

        let update_btn = Button::with_label("Güncelle");
        update_btn.add_css_class("suggested-action");
        update_btn.add_css_class("small-button");
        update_banner.append(&update_btn);

        let close_banner_btn = Button::from_icon_name("window-close-symbolic");
        close_banner_btn.add_css_class("flat");
        close_banner_btn.set_tooltip_text(Some("Kapat"));
        update_banner.append(&close_banner_btn);

        let banner_to_hide = update_banner.clone();
        close_banner_btn.connect_clicked(move |_| {
            banner_to_hide.set_visible(false);
        });

        let u_label = update_label.clone();
        let u_btn = update_btn.clone();
        update_btn.connect_clicked(move |_| {
            u_btn.set_sensitive(false);
            u_label.set_text("Güncelleniyor...");
            let l_ref = u_label.clone();
            let b_ref = u_btn.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = IpcClient::connect().await {
                    match client.send(&IpcRequest::ApplyUpdate).await {
                        Ok(IpcResponse::UpdateApplyResult { success, message }) => {
                            if success {
                                l_ref.set_text(&format!("Başarılı: {}", message));
                            } else {
                                l_ref.set_text(&format!("Hata: {}", message));
                                b_ref.set_sensitive(true);
                            }
                        }
                        _ => {
                            l_ref.set_text("Güncelleme başarısız oldu.");
                            b_ref.set_sensitive(true);
                        }
                    }
                }
            });
        });

        main_box.append(&update_banner);

        // Check if an update is available in background when auto-check is enabled
        let banner_show = update_banner.clone();
        let label_show = update_label.clone();
        if config.updates.auto_check {
            glib::MainContext::default().spawn_local(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if let Ok(client) = IpcClient::connect().await {
                    if let Ok(IpcResponse::UpdateCheckResult {
                        update_available: true,
                        latest_version,
                        ..
                    }) = client.send(&IpcRequest::CheckForUpdates).await {
                        label_show.set_text(&format!("Yeni sürüm (v{}) hazır!", latest_version));
                        banner_show.set_visible(true);
                    }
                }
            });
        }

        // Track pointer and mouse interactions to prevent accidental auto-dismiss during dragging
        let mouse_down_inside = Rc::new(RefCell::new(false));
        let last_release_time = Rc::new(RefCell::new(std::time::Instant::now()));

        let motion_controller = EventControllerMotion::new();
        window.add_controller(motion_controller.clone());

        let click_capture = GestureClick::new();
        click_capture.set_propagation_phase(PropagationPhase::Capture);
        let md_press = Rc::clone(&mouse_down_inside);
        click_capture.connect_pressed(move |_, _, _, _| {
            *md_press.borrow_mut() = true;
        });
        let md_release = Rc::clone(&mouse_down_inside);
        let lr_release = Rc::clone(&last_release_time);
        click_capture.connect_released(move |_, _, _, _| {
            *md_release.borrow_mut() = false;
            *lr_release.borrow_mut() = std::time::Instant::now();
        });
        window.add_controller(click_capture);

        let drag_capture = GestureDrag::new();
        drag_capture.set_propagation_phase(PropagationPhase::Capture);
        let md_drag_start = Rc::clone(&mouse_down_inside);
        drag_capture.connect_drag_begin(move |_, _, _| {
            *md_drag_start.borrow_mut() = true;
        });
        let md_drag_end = Rc::clone(&mouse_down_inside);
        let lr_drag_end = Rc::clone(&last_release_time);
        drag_capture.connect_drag_end(move |_, _, _| {
            *md_drag_end.borrow_mut() = false;
            *lr_drag_end.borrow_mut() = std::time::Instant::now();
        });
        header_box.add_controller(drag_capture);

        // Auto-dismiss popup on focus lost (only when clicked outside)
        let is_mouse_down = Rc::clone(&mouse_down_inside);
        let last_release = Rc::clone(&last_release_time);
        let motion_ctrl = motion_controller.clone();
        window.connect_is_active_notify(move |w| {
            if !w.is_active() {
                let w_clone = w.clone();
                let mouse_down_check = Rc::clone(&is_mouse_down);
                let last_release_check = Rc::clone(&last_release);
                let motion_check = motion_ctrl.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
                    // Do not close if mouse button is held down (e.g. user is dragging window)
                    if *mouse_down_check.borrow() {
                        return;
                    }
                    // Grace period after release so compositor focus settles
                    if last_release_check.borrow().elapsed() < std::time::Duration::from_millis(600)
                    {
                        return;
                    }
                    // Do not close if pointer is still inside window bounds
                    if motion_check.contains_pointer() {
                        return;
                    }
                    // Truly clicked outside into another app
                    if !w_clone.is_active() {
                        w_clone.set_visible(false);
                    }
                });
            }
        });

        // 3. Compact Search Bar
        let search_entry = SearchEntry::new();
        search_entry.add_css_class("compact-search-bar");
        search_entry.set_placeholder_text(Some("Search clipboard history..."));
        main_box.append(&search_entry);

        // View Stack: smooth crossfade between list of items and empty state
        let view_stack = gtk4::Stack::new();
        view_stack.add_css_class("history-stack");
        view_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        view_stack.set_transition_duration(180);
        view_stack.set_vexpand(true);
        view_stack.set_hexpand(true);

        // Scrolled List
        let scrolled = ScrolledWindow::new();
        scrolled.add_css_class("history-scrolled");
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);
        scrolled.set_hscrollbar_policy(gtk4::PolicyType::Never);

        let list_box = ListBox::new();
        list_box.add_css_class("history-listbox");
        list_box.set_selection_mode(gtk4::SelectionMode::Single);
        scrolled.set_child(Some(&list_box));

        // Empty State StatusPage
        let empty_status = libadwaita::StatusPage::new();
        empty_status.set_icon_name(Some("edit-copy-symbolic"));
        empty_status.set_title("Clipboard History is Empty");
        empty_status.set_description(Some(
            "Items you copy with Ctrl+C will appear here.\nYou can open anytime with Super+C.",
        ));
        empty_status.set_vexpand(true);
        empty_status.set_hexpand(true);

        view_stack.add_named(&scrolled, Some("list"));
        view_stack.add_named(&empty_status, Some("empty"));
        view_stack.set_visible_child_name("empty");

        main_box.append(&view_stack);

        // Wrap content inside ToastOverlay
        let toast_overlay = libadwaita::ToastOverlay::new();
        toast_overlay.set_child(Some(&main_box));
        window.set_child(Some(&toast_overlay));

        let entries: Rc<RefCell<Vec<ClipboardEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let filtered_entries: Rc<RefCell<Vec<ClipboardEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let last_deleted: Rc<RefCell<Option<ClipboardEntry>>> = Rc::new(RefCell::new(None));
        let current_filter = Rc::new(RefCell::new(CategoryFilter::All));
        let current_query = Rc::new(RefCell::new(String::new()));
        let search_seq = Rc::new(RefCell::new(0u64));
        let daemon_connected = Rc::new(RefCell::new(true));

        let toggle_pin_holder: Rc<RefCell<Option<Rc<dyn Fn(&str)>>>> = Rc::new(RefCell::new(None));
        let delete_entry_holder: Rc<RefCell<Option<Rc<dyn Fn(&str)>>>> =
            Rc::new(RefCell::new(None));
        let diff_marked: Rc<RefCell<Option<ClipboardEntry>>> = Rc::new(RefCell::new(None));
        let mark_diff_holder: Rc<RefCell<Option<Rc<dyn Fn(&ClipboardEntry)>>>> =
            Rc::new(RefCell::new(None));
        let pinned_collapsed: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let refresh_holder: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

        // Helper to update list rows
        let populate_rows = {
            let list_box_clone = list_box.clone();
            let filtered_clone = Rc::clone(&filtered_entries);
            let win_clone = window.clone();
            let config_clone = config.clone();
            let stack_clone = view_stack.clone();
            let empty_clone = empty_status.clone();
            let current_query_clone = Rc::clone(&current_query);
            let current_filter_clone = Rc::clone(&current_filter);
            let daemon_conn_clone = Rc::clone(&daemon_connected);
            let toggle_pin_cell = Rc::clone(&toggle_pin_holder);
            let delete_entry_cell = Rc::clone(&delete_entry_holder);
            let mark_diff_cell = Rc::clone(&mark_diff_holder);
            let diff_marked_cell = Rc::clone(&diff_marked);
            let pinned_collapsed_clone = Rc::clone(&pinned_collapsed);
            let refresh_holder_clone = Rc::clone(&refresh_holder);

            Rc::new(move || {
                // Clear existing
                while let Some(child) = list_box_clone.first_child() {
                    list_box_clone.remove(&child);
                }

                let items = filtered_clone.borrow();
                if items.is_empty() {
                    if !*daemon_conn_clone.borrow() {
                        empty_clone.set_icon_name(Some("network-error-symbolic"));
                        empty_clone.set_title("Clipboard Service Not Running");
                        empty_clone.set_description(Some(
                            "Background service could not be started.\nYou can start it with 'clipboard-history daemon start' or 'systemctl --user start clipboard-history'.",
                        ));
                        let start_btn = gtk4::Button::with_label("Start Service");
                        start_btn.add_css_class("suggested-action");
                        start_btn.add_css_class("pill");
                        start_btn.set_halign(gtk4::Align::Center);
                        let refresh_cell_btn = Rc::clone(&refresh_holder_clone);
                        start_btn.connect_clicked(move |_| {
                            if let Some(ref rf) = *refresh_cell_btn.borrow() {
                                rf();
                            }
                        });
                        empty_clone.set_child(Some(&start_btn));
                    } else {
                        empty_clone.set_child(None::<&gtk4::Widget>);
                        let q = current_query_clone.borrow();
                        let cat = *current_filter_clone.borrow();
                        if !q.trim().is_empty() {
                            empty_clone.set_icon_name(Some("system-search-symbolic"));
                            empty_clone.set_title("No Results Found");
                            empty_clone.set_description(Some(&format!(
                                "No clipboard history found matching \"{}\".",
                                q.trim()
                            )));
                        } else {
                            match cat {
                                CategoryFilter::Pinned => {
                                    empty_clone.set_icon_name(Some("view-pin-symbolic"));
                                    empty_clone.set_title("No Pinned Items");
                                    empty_clone.set_description(Some(
                                        "You can press 'P' or click the pin icon to pin important items.",
                                    ));
                                }
                                CategoryFilter::Images => {
                                    empty_clone.set_icon_name(Some("image-x-generic-symbolic"));
                                    empty_clone.set_title("No Copied Images");
                                    empty_clone.set_description(Some(
                                        "Images or screenshots you copy will be listed here.",
                                    ));
                                }
                                CategoryFilter::Code => {
                                    empty_clone.set_icon_name(Some("text-x-script-symbolic"));
                                    empty_clone.set_title("No Copied Code Snippets");
                                    empty_clone.set_description(Some(
                                        "Detected code snippets will be listed here.",
                                    ));
                                }
                                _ => {
                                    empty_clone.set_icon_name(Some("edit-copy-symbolic"));
                                    empty_clone.set_title("Clipboard History is Empty");
                                    empty_clone.set_description(Some(
                                        "Items you copy with Ctrl+C will appear here.\nYou can open anytime with Super+C.",
                                    ));
                                }
                            }
                        }
                    }
                    stack_clone.set_visible_child_name("empty");
                } else {
                    stack_clone.set_visible_child_name("list");
                    let pin_fn = {
                        let cell = toggle_pin_cell.clone();
                        Rc::new(move |id: &str| {
                            if let Some(f) = cell.borrow().as_ref() {
                                f(id);
                            }
                        })
                    };
                    let del_fn = {
                        let cell = delete_entry_cell.clone();
                        Rc::new(move |id: &str| {
                            if let Some(f) = cell.borrow().as_ref() {
                                f(id);
                            }
                        })
                    };
                    let mark_fn = {
                        let cell = mark_diff_cell.clone();
                        Rc::new(move |e: &ClipboardEntry| {
                            if let Some(f) = cell.borrow().as_ref() {
                                f(e);
                            }
                        })
                    };

                    let get_diff_target =
                        |entry: &ClipboardEntry| -> Option<ClipboardEntry> {
                            if let Some(ref marked) = *diff_marked_cell.borrow() {
                                if marked.id != entry.id && marked.entry_type != EntryType::Image {
                                    return Some(marked.clone());
                                }
                            }
                            let cur_pos = items.iter().position(|e| e.id == entry.id)?;
                            items
                                .iter()
                                .skip(cur_pos + 1)
                                .find(|e| e.entry_type != EntryType::Image && e.id != entry.id)
                                .or_else(|| {
                                    items.iter().take(cur_pos).rev().find(|e| {
                                        e.entry_type != EntryType::Image && e.id != entry.id
                                    })
                                })
                                .cloned()
                        };

                    let (pinned, unpinned): (Vec<_>, Vec<_>) =
                        items.iter().cloned().partition(|e| e.is_pinned);
                    let mut first_selectable_row: Option<ListBoxRow> = None;

                    // 1. Pinned Section with Counter and Collapsible Toggle
                    if !pinned.is_empty() {
                        let is_collapsed = *pinned_collapsed_clone.borrow();
                        let chevron = if is_collapsed { "▸" } else { "▾" };
                        let pin_title = format!("📌 Pinned ({})  {}", pinned.len(), chevron);

                        let pin_header_btn = Button::with_label(&pin_title);
                        pin_header_btn.add_css_class("section-header-btn");
                        pin_header_btn.set_tooltip_text(Some(if is_collapsed {
                            "Show pinned"
                        } else {
                            "Collapse pinned"
                        }));

                        let collapse_toggle = Rc::clone(&pinned_collapsed_clone);
                        let fn_rerender = Rc::clone(&refresh_holder_clone);
                        pin_header_btn.connect_clicked(move |_| {
                            let mut c = collapse_toggle.borrow_mut();
                            *c = !*c;
                            drop(c);
                            if let Some(rf) = fn_rerender.borrow().as_ref() {
                                rf();
                            }
                        });

                        let header_row = ListBoxRow::new();
                        header_row.set_selectable(false);
                        header_row.set_activatable(false);
                        header_row.set_child(Some(&pin_header_btn));
                        list_box_clone.append(&header_row);

                        if !is_collapsed {
                            for (idx, entry) in pinned.iter().enumerate() {
                                let diff_target = get_diff_target(entry);
                                let entry_row = create_configured_entry_row(
                                    entry,
                                    idx,
                                    RowConfig {
                                        window: &win_clone,
                                        config: &config_clone,
                                        list_box: &list_box_clone,
                                        on_pin: pin_fn.clone(),
                                        on_del: del_fn.clone(),
                                        diff_target,
                                        on_mark_diff: Some(mark_fn.clone()),
                                    },
                                );
                                if first_selectable_row.is_none() {
                                    first_selectable_row = Some(entry_row.row.clone());
                                }
                                list_box_clone.append(&entry_row.row);
                            }
                        }
                    }

                    // 2. History (Unpinned) Section with Counter
                    if !unpinned.is_empty() {
                        if !pinned.is_empty() {
                            let history_title = format!("History ({})", unpinned.len());
                            let history_header = Label::new(Some(&history_title));
                            history_header.add_css_class("section-header");
                            history_header.set_xalign(0.0);
                            let header_row = ListBoxRow::new();
                            header_row.set_selectable(false);
                            header_row.set_activatable(false);
                            header_row.set_child(Some(&history_header));
                            list_box_clone.append(&header_row);
                        }

                        for (idx, entry) in unpinned.iter().take(50).enumerate() {
                            let diff_target = get_diff_target(entry);
                            let entry_row = create_configured_entry_row(
                                entry,
                                idx,
                                RowConfig {
                                    window: &win_clone,
                                    config: &config_clone,
                                    list_box: &list_box_clone,
                                    on_pin: pin_fn.clone(),
                                    on_del: del_fn.clone(),
                                    diff_target,
                                    on_mark_diff: Some(mark_fn.clone()),
                                },
                            );
                            if first_selectable_row.is_none() {
                                first_selectable_row = Some(entry_row.row.clone());
                            }
                            list_box_clone.append(&entry_row.row);
                        }
                    }

                    if let Some(first_row) = first_selectable_row {
                        list_box_clone.select_row(Some(&first_row));
                    }
                }
            })
        };

        // Helper to re-filter and populate
        let filter_and_render: Rc<dyn Fn()> = {
            let entries_clone = Rc::clone(&entries);
            let filtered_clone = Rc::clone(&filtered_entries);
            let current_query_clone = Rc::clone(&current_query);
            let current_filter_clone = Rc::clone(&current_filter);
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
        let fetch_and_refresh: Rc<dyn Fn()> = {
            let entries_c = Rc::clone(&entries);
            let daemon_conn_c = Rc::clone(&daemon_connected);
            let fn_rf = Rc::clone(&filter_and_render);
            Rc::new(move || {
                Self::refresh_entries_internal(
                    Rc::clone(&entries_c),
                    Rc::clone(&daemon_conn_c),
                    Rc::clone(&fn_rf),
                );
            })
        };
        *refresh_holder.borrow_mut() = Some(Rc::clone(&fetch_and_refresh));

        // In-memory instant pin toggle
        let toggle_pin: Rc<dyn Fn(&str)> = {
            let entries_clone = Rc::clone(&entries);
            let filtered_clone = Rc::clone(&filtered_entries);
            let fn_refresh = Rc::clone(&filter_and_render);
            Rc::new(move |id: &str| {
                let mut target_is_pinned = None;
                {
                    let mut items = entries_clone.borrow_mut();
                    if let Some(pos) = items.iter().position(|e| e.id == id) {
                        items[pos].is_pinned = !items[pos].is_pinned;
                        target_is_pinned = Some(items[pos].is_pinned);
                        items.sort_by(|a, b| {
                            b.is_pinned
                                .cmp(&a.is_pinned)
                                .then_with(|| b.last_used_at.cmp(&a.last_used_at))
                        });
                    }
                }
                {
                    let mut filt = filtered_clone.borrow_mut();
                    if let Some(pos) = filt.iter().position(|e| e.id == id) {
                        let new_state = target_is_pinned.unwrap_or(!filt[pos].is_pinned);
                        filt[pos].is_pinned = new_state;
                        target_is_pinned = Some(new_state);
                        filt.sort_by(|a, b| {
                            b.is_pinned
                                .cmp(&a.is_pinned)
                                .then_with(|| b.last_used_at.cmp(&a.last_used_at))
                        });
                    }
                }
                fn_refresh();

                if let Some(is_pinned) = target_is_pinned {
                    let id_str = id.to_string();
                    glib::MainContext::default().spawn_local(async move {
                        if let Ok(client) = IpcClient::connect().await {
                            let req = if is_pinned {
                                IpcRequest::PinEntry { id: id_str }
                            } else {
                                IpcRequest::UnpinEntry { id: id_str }
                            };
                            let _ = client.send(&req).await;
                        }
                    });
                }
            })
        };
        *toggle_pin_holder.borrow_mut() = Some(Rc::clone(&toggle_pin));

        // Undo delete logic
        let undo_delete: Rc<dyn Fn()> = {
            let entries_clone = Rc::clone(&entries);
            let last_del = Rc::clone(&last_deleted);
            let toast_ov = toast_overlay.clone();
            let fn_refresh = Rc::clone(&filter_and_render);
            Rc::new(move || {
                if let Some(entry) = last_del.borrow_mut().take() {
                    let mut items = entries_clone.borrow_mut();
                    let id_str = entry.id.clone();
                    let is_pinned = entry.is_pinned;
                    let text_content = entry.text_content.clone();
                    items.push(entry);
                    items.sort_by(|a, b| {
                        b.is_pinned
                            .cmp(&a.is_pinned)
                            .then_with(|| b.last_used_at.cmp(&a.last_used_at))
                    });
                    drop(items);
                    fn_refresh();

                    if let Some(text) = text_content {
                        glib::MainContext::default().spawn_local(async move {
                            let _ = clipboard_history_core::copy_to_system_clipboard(&text).await;
                            if is_pinned {
                                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                                if let Ok(client) = IpcClient::connect().await {
                                    let _ = client.send(&IpcRequest::PinEntry { id: id_str }).await;
                                }
                            }
                        });
                    }

                    let toast = libadwaita::Toast::new("Item restored");
                    toast_ov.add_toast(toast);
                }
            })
        };

        // In-memory instant delete logic with Toast undo action
        let delete_entry: Rc<dyn Fn(&str)> = {
            let entries_clone = Rc::clone(&entries);
            let filtered_clone = Rc::clone(&filtered_entries);
            let last_del = Rc::clone(&last_deleted);
            let toast_ov = toast_overlay.clone();
            let fn_refresh = Rc::clone(&filter_and_render);
            Rc::new(move |id: &str| {
                let mut removed_entry = None;
                {
                    let mut items = entries_clone.borrow_mut();
                    if let Some(pos) = items.iter().position(|e| e.id == id) {
                        removed_entry = Some(items.remove(pos));
                    }
                }
                {
                    let mut filt = filtered_clone.borrow_mut();
                    if let Some(pos) = filt.iter().position(|e| e.id == id) {
                        if removed_entry.is_none() {
                            removed_entry = Some(filt.remove(pos));
                        } else {
                            filt.remove(pos);
                        }
                    }
                }

                if let Some(removed) = removed_entry {
                    let id_str = id.to_string();
                    let is_snippet = removed.source_app.as_deref() == Some("Snippet");
                    *last_del.borrow_mut() = Some(removed);
                    fn_refresh();

                    let toast = libadwaita::Toast::new("Item deleted");
                    toast.set_button_label(Some("Undo"));
                    toast.set_action_name(Some("win.undo-delete"));
                    toast_ov.add_toast(toast);

                    glib::MainContext::default().spawn_local(async move {
                        if let Ok(client) = IpcClient::connect().await {
                            let req = if is_snippet {
                                IpcRequest::DeleteSnippet { id: id_str }
                            } else {
                                IpcRequest::DeleteEntry { id: id_str }
                            };
                            let _ = client.send(&req).await;
                        }
                    });
                }
            })
        };
        *delete_entry_holder.borrow_mut() = Some(Rc::clone(&delete_entry));

        let mark_for_diff: Rc<dyn Fn(&ClipboardEntry)> = {
            let diff_marked_cell = Rc::clone(&diff_marked);
            let toast_ov = toast_overlay.clone();
            let fn_refresh = Rc::clone(&filter_and_render);
            Rc::new(move |entry: &ClipboardEntry| {
                *diff_marked_cell.borrow_mut() = Some(entry.clone());
                let preview = entry.preview.chars().take(30).collect::<String>();
                toast_ov.add_toast(libadwaita::Toast::new(&format!(
                    "Marked Diff Baseline: \"{}...\"",
                    preview
                )));
                fn_refresh();
            })
        };
        *mark_diff_holder.borrow_mut() = Some(Rc::clone(&mark_for_diff));

        // Connect win.undo-delete action to window via SimpleActionGroup
        let undo_action = gio::SimpleAction::new("undo-delete", None);
        let undo_call = Rc::clone(&undo_delete);
        undo_action.connect_activate(move |_, _| {
            undo_call();
        });
        let action_group = gio::SimpleActionGroup::new();
        action_group.add_action(&undo_action);
        window.insert_action_group("win", Some(&action_group));

        // Category Tab switching logic
        let tab_buttons_rc = Rc::new(RefCell::new(tab_buttons));
        let switch_category: Rc<dyn Fn(CategoryFilter)> = {
            let cur_filter = Rc::clone(&current_filter);
            let fn_refresh = Rc::clone(&filter_and_render);
            let btns = Rc::clone(&tab_buttons_rc);
            Rc::new(move |new_cat: CategoryFilter| {
                *cur_filter.borrow_mut() = new_cat;
                let cats = [
                    CategoryFilter::All,
                    CategoryFilter::Images,
                    CategoryFilter::Code,
                ];
                let buttons = btns.borrow();
                for (i, btn) in buttons.iter().enumerate() {
                    if cats[i] == new_cat {
                        btn.add_css_class("tab-active");
                    } else {
                        btn.remove_css_class("tab-active");
                    }
                }
                fn_refresh();
            })
        };

        for (i, btn) in tab_buttons_rc.borrow().iter().enumerate() {
            let cat = tab_defs[i].0;
            let switch_fn = Rc::clone(&switch_category);
            btn.connect_clicked(move |_| {
                switch_fn(cat);
            });
        }

        // Search entry live typing with 100ms debounce and universal full-history IPC search
        let debounce_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        let trigger_search: Rc<dyn Fn(String)> = {
            let entries_clone = Rc::clone(&entries);
            let filtered_clone = Rc::clone(&filtered_entries);
            let current_filter_clone = Rc::clone(&current_filter);
            let current_query_clone = Rc::clone(&current_query);
            let pop_fn = Rc::clone(&populate_rows);
            let seq_clone = Rc::clone(&search_seq);

            Rc::new(move |query_text: String| {
                let trimmed = query_text.trim().to_string();
                *current_query_clone.borrow_mut() = trimmed.clone();

                if trimmed.is_empty() {
                    // Instant restoration: cancel in-flight sequence, restore from local cache
                    let next_seq = seq_clone.borrow().wrapping_add(1);
                    *seq_clone.borrow_mut() = next_seq;

                    let searcher = FuzzySearchEngine::new();
                    let all_items = entries_clone.borrow();
                    let cat = *current_filter_clone.borrow();
                    *filtered_clone.borrow_mut() = searcher.filter_entries(&all_items, "", cat);
                    pop_fn();
                    return;
                }

                // Active search: dispatch IPC query
                let my_seq = seq_clone.borrow().wrapping_add(1);
                *seq_clone.borrow_mut() = my_seq;

                let filtered_c = Rc::clone(&filtered_clone);
                let entries_c = Rc::clone(&entries_clone);
                let filter_c = Rc::clone(&current_filter_clone);
                let q_c = Rc::clone(&current_query_clone);
                let pop_c = Rc::clone(&pop_fn);
                let seq_c = Rc::clone(&seq_clone);
                let q_target = trimmed.clone();

                glib::MainContext::default().spawn_local(async move {
                    let search_results = if let Ok(client) = IpcClient::connect().await {
                        let req = IpcRequest::Search {
                            query: q_target.clone(),
                            limit: 100,
                            offset: 0,
                        };
                        match client.send(&req).await {
                            Ok(IpcResponse::Entries(results)) => results,
                            _ => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    };

                    // Verify sequence and current query before updating
                    if *seq_c.borrow() != my_seq || *q_c.borrow() != q_target {
                        return;
                    }

                    let searcher = FuzzySearchEngine::new();
                    let cat = *filter_c.borrow();

                    let final_entries = if !search_results.is_empty() {
                        searcher.filter_entries(&search_results, &q_target, cat)
                    } else {
                        // Fallback to local filtering if daemon offline or returned 0
                        searcher.filter_entries(&entries_c.borrow(), &q_target, cat)
                    };

                    *filtered_c.borrow_mut() = final_entries;
                    pop_c();
                });
            })
        };

        let deb_source = Rc::clone(&debounce_source);
        let trig_search = Rc::clone(&trigger_search);
        search_entry.connect_search_changed(move |entry| {
            let text = entry.text().to_string();
            if let Some(source) = deb_source.borrow_mut().take() {
                source.remove();
            }

            if text.trim().is_empty() {
                // Immediate zero-lag restore on empty
                trig_search(text);
            } else {
                // 100ms debounce on typing
                let deb_ref = Rc::clone(&deb_source);
                let trig = Rc::clone(&trig_search);
                let source_id = glib::timeout_add_local_once(
                    std::time::Duration::from_millis(100),
                    move || {
                        deb_ref.borrow_mut().take();
                        trig(text);
                    },
                );
                *deb_source.borrow_mut() = Some(source_id);
            }
        });

        // Pressing Enter in search bar activates currently selected (or first) row
        let list_for_search_activate = list_box.clone();
        search_entry.connect_activate(move |_| {
            if let Some(row) = list_for_search_activate.selected_row() {
                row.activate();
            } else if let Some(row) = list_for_search_activate.row_at_index(0) {
                row.activate();
            }
        });

        // Clear all button in Windows 11 subhead with confirmation popover
        let fn_for_clear = Rc::clone(&filter_and_render);
        let toast_for_clear = toast_overlay.clone();
        let entries_for_clear = Rc::clone(&entries);

        let clear_popover = Popover::new();
        clear_popover.set_parent(&clear_all_btn);
        clear_popover.set_autohide(true);
        clear_popover.add_css_class("clear-confirm-popover");

        let pop_box = GtkBox::new(Orientation::Vertical, 8);
        pop_box.set_margin_start(14);
        pop_box.set_margin_end(14);
        pop_box.set_margin_top(12);
        pop_box.set_margin_bottom(12);

        let pop_title = Label::new(Some("Clear All History?"));
        pop_title.add_css_class("confirm-title");
        pop_title.set_xalign(0.0);
        pop_box.append(&pop_title);

        let pop_desc = Label::new(Some(
            "Your clipboard history will be cleared.\nPinned items will be preserved.",
        ));
        pop_desc.add_css_class("confirm-desc");
        pop_desc.set_xalign(0.0);
        pop_box.append(&pop_desc);

        // Checkbox: "Also clear pinned items" (unchecked by default)
        let pin_check = CheckButton::with_label("Also clear pinned items");
        pin_check.set_active(false);
        pin_check.add_css_class("confirm-checkbox");
        pop_box.append(&pin_check);

        let btn_row = GtkBox::new(Orientation::Horizontal, 8);
        btn_row.set_margin_top(8);

        let cancel_btn = Button::with_label("Cancel");
        cancel_btn.add_css_class("flat");
        cancel_btn.set_hexpand(true);
        let pop_cancel_clone = clear_popover.clone();
        cancel_btn.connect_clicked(move |_| {
            pop_cancel_clone.popdown();
        });

        let confirm_btn = Button::with_label("Clear");
        confirm_btn.add_css_class("destructive-action");
        confirm_btn.set_hexpand(true);

        let pop_confirm_clone = clear_popover.clone();
        let pin_check_clone = pin_check.clone();
        confirm_btn.connect_clicked(move |_| {
            pop_confirm_clone.popdown();
            let clear_pinned = pin_check_clone.is_active();
            let fn_call = Rc::clone(&fn_for_clear);
            let toast = toast_for_clear.clone();
            let entries_cell = Rc::clone(&entries_for_clear);

            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = IpcClient::connect().await {
                    let _ = client
                        .send(&IpcRequest::ClearHistory {
                            include_pinned: clear_pinned,
                        })
                        .await;
                    if clear_pinned {
                        entries_cell.borrow_mut().clear();
                        toast.add_toast(libadwaita::Toast::new(
                            "All clipboard history and pinned items cleared",
                        ));
                    } else {
                        entries_cell.borrow_mut().retain(|e| e.is_pinned);
                        toast.add_toast(libadwaita::Toast::new(
                            "Clipboard history cleared (Pinned items preserved)",
                        ));
                    }
                    fn_call();
                }
            });
        });

        btn_row.append(&cancel_btn);
        btn_row.append(&confirm_btn);
        pop_box.append(&btn_row);

        clear_popover.set_child(Some(&pop_box));

        let clear_popover_show = clear_popover.clone();
        clear_all_btn.connect_clicked(move |_| {
            pin_check.set_active(false);
            clear_popover_show.popup();
        });

        // Row Activated: Direct paste
        let filtered_for_act = Rc::clone(&filtered_entries);
        let entries_for_act = Rc::clone(&entries);
        let fn_refresh_act = Rc::clone(&filter_and_render);
        let win_for_act = window.clone();
        let cfg_for_act = config.clone();
        list_box.connect_row_activated(move |_, row| {
            let entry_id = row.widget_name();
            let items = filtered_for_act.borrow();
            if let Some(entry) = items.iter().find(|e| e.id == entry_id.as_str()) {
                let entry_type = entry.entry_type;
                let auto_paste = cfg_for_act.paste.auto_paste;
                let move_to_top = cfg_for_act.paste.move_pasted_to_top;
                let delay_ms = cfg_for_act.paste.effective_delay_ms();
                let entry_id_str = entry.id.clone();
                let content_hash = entry.content_hash.clone();

                // Immediately hide popup so destination window regains focus
                win_for_act.set_visible(false);

                if entry_type == clipboard_history_core::domain::EntryType::Image {
                    let blobs_dir = AppConfig::blobs_dir();
                    let find_blob = |hash: &str| -> Option<std::path::PathBuf> {
                        let direct = blobs_dir.join(hash);
                        if direct.exists() {
                            return Some(direct);
                        }
                        let with_ext = blobs_dir.join(format!("{}.blob", hash));
                        if with_ext.exists() {
                            return Some(with_ext);
                        }
                        None
                    };
                    let img_path = entry.blob_hash.as_deref().and_then(find_blob);
                    let id_for_touch = entry_id_str.clone();
                    if let Some(p) = img_path {
                        if let Ok(bytes) = std::fs::read(&p) {
                            glib::MainContext::default().spawn_local(async move {
                                if let Ok(client) = IpcClient::connect().await {
                                    let _ = client.prepare_paste(&content_hash).await;
                                    if move_to_top {
                                        let _ = client.touch_entry(&id_for_touch).await;
                                    }
                                }
                                clipboard_history_core::clipboard::copy_image_to_system_clipboard(
                                    &bytes,
                                    Some("image/png"),
                                )
                                .await;
                                if auto_paste {
                                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms))
                                        .await;
                                    let cascade = InjectorCascade::new();
                                    let (name, success) = cascade.execute_paste().await;
                                    info!("Paste executed: injector={}, success={}", name, success);
                                }
                            });
                        }
                    }
                } else {
                    let mut text = entry
                        .text_content
                        .clone()
                        .unwrap_or_else(|| entry.preview.clone());
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

                    let text_clone = text.clone();
                    let id_for_touch = entry_id_str.clone();
                    glib::MainContext::default().spawn_local(async move {
                        if let Ok(client) = IpcClient::connect().await {
                            let _ = client.prepare_paste(&content_hash).await;
                            if move_to_top {
                                let _ = client.touch_entry(&id_for_touch).await;
                            }
                        }
                        clipboard_history_core::copy_to_system_clipboard(&text_clone).await;
                        if auto_paste {
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                            let cascade = InjectorCascade::new();
                            let (name, success) = cascade.execute_paste().await;
                            info!("Paste executed: injector={}, success={}", name, success);
                        }
                    });
                }

                if move_to_top {
                    let mut all_entries = entries_for_act.borrow_mut();
                    if let Some(pos) = all_entries.iter().position(|e| e.id == entry_id_str) {
                        all_entries[pos].last_used_at = chrono::Utc::now();
                        all_entries.sort_by(|a, b| {
                            b.is_pinned
                                .cmp(&a.is_pinned)
                                .then_with(|| b.last_used_at.cmp(&a.last_used_at))
                        });
                    }
                    drop(all_entries);
                    fn_refresh_act();
                }
            }
        });

        // Keyboard Navigation Controller (Return, Shift+Enter, Up, Down, Escape, Del, P, Q, 1..9, Ctrl+T, Ctrl+Z, Ctrl+C, Tab)
        let key_controller = EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let win_key = window.clone();
        let list_box_key = list_box.clone();
        let search_entry_key = search_entry.clone();
        let filtered_key = Rc::clone(&filtered_entries);
        let entries_key = Rc::clone(&entries);
        let fn_refresh_key = Rc::clone(&filter_and_render);
        let cfg_key = config.clone();
        let current_filter_key = Rc::clone(&current_filter);
        let switch_category_key = Rc::clone(&switch_category);
        let toggle_pin_key = Rc::clone(&toggle_pin);
        let delete_entry_key = Rc::clone(&delete_entry);
        let undo_delete_key = Rc::clone(&undo_delete);
        let toast_key = toast_overlay.clone();
        let diff_marked_key = Rc::clone(&diff_marked);
        let mark_for_diff_key = Rc::clone(&mark_for_diff);

        key_controller.connect_key_pressed(move |_, key, _, state| {
            // Dismiss popup
            if key == Key::Escape {
                win_key.set_visible(false);
                return glib::Propagation::Stop;
            }

            // Tab / Shift+Tab: cycle category tabs
            if key == Key::Tab || key == Key::ISO_Left_Tab {
                let cats = [
                    CategoryFilter::All,
                    CategoryFilter::Images,
                    CategoryFilter::Code,
                ];
                let cur = *current_filter_key.borrow();
                let cur_idx = cats.iter().position(|&c| c == cur).unwrap_or(0);
                let next_idx =
                    if key == Key::ISO_Left_Tab || state.contains(gdk4::ModifierType::SHIFT_MASK) {
                        (cur_idx + 2) % 3
                    } else {
                        (cur_idx + 1) % 3
                    };
                switch_category_key(cats[next_idx]);
                return glib::Propagation::Stop;
            }

            // Undo Delete (Ctrl+Z)
            if (key == Key::z || key == Key::Z) && state.contains(gdk4::ModifierType::CONTROL_MASK)
            {
                undo_delete_key();
                return glib::Propagation::Stop;
            }

            // Copy to clipboard without closing (Ctrl+C)
            if (key == Key::c || key == Key::C) && state.contains(gdk4::ModifierType::CONTROL_MASK)
            {
                let mut search_has_selection = false;
                if search_entry_key.has_focus() {
                    use gtk4::prelude::EditableExt;
                    if let Some((start, end)) = search_entry_key.selection_bounds() {
                        if start != end {
                            search_has_selection = true;
                        }
                    }
                }
                if !search_has_selection {
                    if let Some(selected_row) = list_box_key.selected_row() {
                        let entry_id = selected_row.widget_name();
                        let items = filtered_key.borrow();
                        if let Some(entry) = items.iter().find(|e| e.id == entry_id.as_str()) {
                            let text = entry
                                .text_content
                                .clone()
                                .unwrap_or_else(|| entry.preview.clone());
                            if let Some(display) = gdk4::Display::default() {
                                display.clipboard().set_text(&text);
                            }
                            let text_clone = text.clone();
                            let hash = clipboard_history_core::blob::BlobStore::compute_hash(text_clone.as_bytes());
                            glib::MainContext::default().spawn_local(async move {
                                if let Ok(client) = IpcClient::connect().await {
                                    let _ = client.prepare_paste(&hash).await;
                                }
                                clipboard_history_core::copy_to_system_clipboard(&text_clone).await;
                            });
                            toast_key.add_toast(libadwaita::Toast::new("Copied to clipboard"));
                            return glib::Propagation::Stop;
                        }
                    }
                }
            }

            // Down arrow: move selection down (skipping non-selectable section headers)
            if key == Key::Down {
                let start_idx = list_box_key
                    .selected_row()
                    .map(|r| r.index() + 1)
                    .unwrap_or(0);
                let mut scan_idx = start_idx;
                while let Some(next_row) = list_box_key.row_at_index(scan_idx) {
                    if next_row.is_selectable() {
                        list_box_key.select_row(Some(&next_row));
                        next_row.grab_focus();
                        return glib::Propagation::Stop;
                    }
                    scan_idx += 1;
                }
            }

            // Up arrow: move selection up (skipping non-selectable section headers)
            if key == Key::Up {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let mut scan_idx = selected_row.index() - 1;
                    while scan_idx >= 0 {
                        if let Some(prev_row) = list_box_key.row_at_index(scan_idx) {
                            if prev_row.is_selectable() {
                                list_box_key.select_row(Some(&prev_row));
                                prev_row.grab_focus();
                                return glib::Propagation::Stop;
                            }
                        }
                        scan_idx -= 1;
                    }
                }
                search_entry_key.grab_focus();
                return glib::Propagation::Stop;
            }

            // Enter / Return: activate selected row
            if (key == Key::Return || key == Key::KP_Enter)
                && !state.contains(gdk4::ModifierType::SHIFT_MASK)
            {
                if let Some(selected_row) = list_box_key.selected_row() {
                    selected_row.activate();
                    return glib::Propagation::Stop;
                } else if let Some(first_row) = list_box_key.row_at_index(0) {
                    first_row.activate();
                    return glib::Propagation::Stop;
                }
            }

            // Quick Transforms Popover (Ctrl+T)
            if (key == Key::t || key == Key::T) && state.contains(gdk4::ModifierType::CONTROL_MASK)
            {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let entry_id = selected_row.widget_name();
                    let items = filtered_key.borrow();
                    if let Some(entry) = items.iter().find(|e| e.id == entry_id.as_str()) {
                        let diff_target = if let Some(ref marked) = *diff_marked_key.borrow() {
                            if marked.id != entry.id && marked.entry_type != EntryType::Image {
                                Some(marked.clone())
                            } else {
                                None
                            }
                        } else {
                            let cur_pos = items.iter().position(|e| e.id == entry.id);
                            cur_pos.and_then(|pos| {
                                items
                                    .iter()
                                    .skip(pos + 1)
                                    .find(|e| e.entry_type != EntryType::Image && e.id != entry.id)
                                    .or_else(|| {
                                        items.iter().take(pos).rev().find(|e| {
                                            e.entry_type != EntryType::Image && e.id != entry.id
                                        })
                                    })
                                    .cloned()
                            })
                        };
                        show_transforms_popover(
                            &selected_row,
                            entry,
                            &win_key,
                            &cfg_key,
                            TransformsOptions {
                                diff_target,
                                on_pin: Some(Rc::clone(&toggle_pin_key)),
                                on_del: Some(Rc::clone(&delete_entry_key)),
                                on_mark_diff: Some(Rc::clone(&mark_for_diff_key)),
                            },
                        );
                        return glib::Propagation::Stop;
                    }
                }
            }

            // Plain Text paste (Shift + Return / Shift + KP_Enter)
            if (key == Key::Return || key == Key::KP_Enter)
                && state.contains(gdk4::ModifierType::SHIFT_MASK)
            {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let entry_id = selected_row.widget_name();
                    let items = filtered_key.borrow();
                    if let Some(entry) = items.iter().find(|e| e.id == entry_id.as_str()) {
                        let raw_text = entry.text_content.as_deref().unwrap_or(&entry.preview);
                        let plain_text = TextTransforms::strip_formatting(raw_text);
                        if let Some(display) = gdk4::Display::default() {
                            display.clipboard().set_text(&plain_text);
                        }
                        win_key.set_visible(false);
                        let auto_paste = cfg_key.paste.auto_paste;
                        let move_to_top = cfg_key.paste.move_pasted_to_top;
                        let delay_ms = cfg_key.paste.effective_delay_ms();
                        let entry_id_str = entry.id.clone();
                        let hash = clipboard_history_core::blob::BlobStore::compute_hash(plain_text.as_bytes());
                        let id_for_touch = entry_id_str.clone();
                        glib::MainContext::default().spawn_local(async move {
                            if let Ok(client) = IpcClient::connect().await {
                                let _ = client.prepare_paste(&hash).await;
                                if move_to_top {
                                    let _ = client.touch_entry(&id_for_touch).await;
                                }
                            }
                            clipboard_history_core::copy_to_system_clipboard(&plain_text).await;
                            if auto_paste {
                                tokio::time::sleep(std::time::Duration::from_millis(delay_ms))
                                    .await;
                                let cascade = InjectorCascade::new();
                                let (name, success) = cascade.execute_paste().await;
                                info!(
                                    "Plain text paste executed: injector={}, success={}",
                                    name, success
                                );
                            }
                        });

                        if move_to_top {
                            let mut all_entries = entries_key.borrow_mut();
                            if let Some(pos) = all_entries.iter().position(|e| e.id == entry_id_str) {
                                all_entries[pos].last_used_at = chrono::Utc::now();
                                all_entries.sort_by(|a, b| {
                                    b.is_pinned
                                        .cmp(&a.is_pinned)
                                        .then_with(|| b.last_used_at.cmp(&a.last_used_at))
                                });
                            }
                            drop(all_entries);
                            fn_refresh_key();
                        }

                        return glib::Propagation::Stop;
                    }
                }
            }

            // Delete key: delete entry if search is unfocused or search query is empty
            if key == Key::Delete
                && (!search_entry_key.has_focus() || search_entry_key.text().is_empty())
            {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let entry_id = selected_row.widget_name();
                    delete_entry_key(entry_id.as_str());
                    return glib::Propagation::Stop;
                }
            }

            let has_ctrl_or_alt = state.contains(gdk4::ModifierType::CONTROL_MASK)
                || state.contains(gdk4::ModifierType::ALT_MASK);

            // D key: Diff viewer comparing selected row with marked baseline (or adjacent item)
            let is_d = key == Key::d || key == Key::D;
            if is_d && (!search_entry_key.has_focus() || has_ctrl_or_alt) {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let entry_id = selected_row.widget_name();
                    let items = filtered_key.borrow();
                    if let Some(entry) = items.iter().find(|e| e.id == entry_id.as_str()) {
                        if entry.entry_type != EntryType::Image {
                            let diff_target = if let Some(ref marked) = *diff_marked_key.borrow() {
                                if marked.id != entry.id && marked.entry_type != EntryType::Image {
                                    Some(marked.clone())
                                } else {
                                    None
                                }
                            } else {
                                let cur_pos = items.iter().position(|e| e.id == entry.id);
                                cur_pos.and_then(|pos| {
                                    items
                                        .iter()
                                        .skip(pos + 1)
                                        .find(|e| e.entry_type != EntryType::Image && e.id != entry.id)
                                        .or_else(|| {
                                            items.iter().take(pos).rev().find(|e| {
                                                e.entry_type != EntryType::Image && e.id != entry.id
                                            })
                                        })
                                        .cloned()
                                })
                            };

                            if let Some(target) = diff_target {
                                let text_curr = entry
                                    .text_content
                                    .clone()
                                    .unwrap_or_else(|| entry.preview.clone());
                                let text_other = target
                                    .text_content
                                    .clone()
                                    .unwrap_or_else(|| target.preview.clone());
                                let label_curr = format!(
                                    "Selected: {}",
                                    entry.preview.chars().take(28).collect::<String>()
                                );
                                let label_other = format!(
                                    "Baseline: {}",
                                    target.preview.chars().take(28).collect::<String>()
                                );

                                win_key.set_visible(false);
                                crate::diff_dialog::DiffDialog::show(
                                    None,
                                    text_other,
                                    text_curr,
                                    label_other,
                                    label_curr,
                                );
                                return glib::Propagation::Stop;
                            } else {
                                toast_key.add_toast(libadwaita::Toast::new(
                                    "Need at least 2 text clips to compare diff",
                                ));
                                return glib::Propagation::Stop;
                            }
                        }
                    }
                }
            }

            // M key: mark current selected item as diff baseline
            let is_m = key == Key::m || key == Key::M;
            if is_m && (!search_entry_key.has_focus() || has_ctrl_or_alt) {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let entry_id = selected_row.widget_name();
                    let items = filtered_key.borrow();
                    if let Some(entry) = items.iter().find(|e| e.id == entry_id.as_str()) {
                        if entry.entry_type != EntryType::Image {
                            mark_for_diff_key(entry);
                            return glib::Propagation::Stop;
                        }
                    }
                }
            }

            // P key: toggle pin (unfocused or with Ctrl/Alt)
            let is_p = key == Key::p || key == Key::P;
            if is_p && (!search_entry_key.has_focus() || has_ctrl_or_alt) {
                if let Some(selected_row) = list_box_key.selected_row() {
                    let entry_id = selected_row.widget_name();
                    toggle_pin_key(entry_id.as_str());
                    return glib::Propagation::Stop;
                }
            }

            // Non-focused shortcuts
            if !search_entry_key.has_focus() {
                // BackSpace: remove entry
                if key == Key::BackSpace {
                    if let Some(selected_row) = list_box_key.selected_row() {
                        let entry_id = selected_row.widget_name();
                        delete_entry_key(entry_id.as_str());
                        return glib::Propagation::Stop;
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
                            win_key.set_visible(false);
                            let auto_paste = cfg_key.paste.auto_paste;
                            let move_to_top = cfg_key.paste.move_pasted_to_top;
                            let delay_ms = cfg_key.paste.effective_delay_ms();
                            let entry_id_str = entry.id.clone();
                            let content_hash = entry.content_hash.clone();
                            let id_for_touch = entry_id_str.clone();

                            if entry.entry_type == clipboard_history_core::domain::EntryType::Image {
                                let blobs_dir = AppConfig::blobs_dir();
                                let find_blob = |hash: &str| -> Option<std::path::PathBuf> {
                                    let direct = blobs_dir.join(hash);
                                    if direct.exists() {
                                        return Some(direct);
                                    }
                                    let with_ext = blobs_dir.join(format!("{}.blob", hash));
                                    if with_ext.exists() {
                                        return Some(with_ext);
                                    }
                                    None
                                };
                                let img_path = entry.blob_hash.as_deref().and_then(find_blob);
                                if let Some(p) = img_path {
                                    if let Ok(bytes) = std::fs::read(&p) {
                                        glib::MainContext::default().spawn_local(async move {
                                            if let Ok(client) = IpcClient::connect().await {
                                                let _ = client.prepare_paste(&content_hash).await;
                                                if move_to_top {
                                                    let _ = client.touch_entry(&id_for_touch).await;
                                                }
                                            }
                                            clipboard_history_core::clipboard::copy_image_to_system_clipboard(
                                                &bytes,
                                                Some("image/png"),
                                            )
                                            .await;
                                            if auto_paste {
                                                tokio::time::sleep(std::time::Duration::from_millis(delay_ms))
                                                    .await;
                                                let cascade = InjectorCascade::new();
                                                let _ = cascade.execute_paste().await;
                                            }
                                        });
                                    }
                                }
                            } else {
                                let text = entry
                                    .text_content
                                    .clone()
                                    .unwrap_or_else(|| entry.preview.clone());
                                if let Some(display) = gdk4::Display::default() {
                                    display.clipboard().set_text(&text);
                                }
                                glib::MainContext::default().spawn_local(async move {
                                    if let Ok(client) = IpcClient::connect().await {
                                        let _ = client.prepare_paste(&content_hash).await;
                                        if move_to_top {
                                            let _ = client.touch_entry(&id_for_touch).await;
                                        }
                                    }
                                    clipboard_history_core::copy_to_system_clipboard(&text).await;
                                    if auto_paste {
                                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms))
                                            .await;
                                        let cascade = InjectorCascade::new();
                                        let _ = cascade.execute_paste().await;
                                    }
                                });
                            }

                            if move_to_top {
                                let mut all_entries = entries_key.borrow_mut();
                                if let Some(pos) = all_entries.iter().position(|e| e.id == entry_id_str) {
                                    all_entries[pos].last_used_at = chrono::Utc::now();
                                    all_entries.sort_by(|a, b| {
                                        b.is_pinned
                                            .cmp(&a.is_pinned)
                                            .then_with(|| b.last_used_at.cmp(&a.last_used_at))
                                    });
                                }
                                drop(all_entries);
                                fn_refresh_key();
                            }

                            return glib::Propagation::Stop;
                        }
                    }
                }
            }

            glib::Propagation::Proceed
        });
        window.add_controller(key_controller);

        // Periodic background poll while window is open so synced peer changes reflect live
        let refresh_holder_periodic = Rc::clone(&refresh_holder);
        let win_weak = window.downgrade();
        let query_check = Rc::clone(&current_query);
        glib::timeout_add_local(std::time::Duration::from_millis(1500), move || {
            if let Some(w) = win_weak.upgrade() {
                if w.is_visible() && query_check.borrow().is_empty() {
                    if let Some(rf) = refresh_holder_periodic.borrow().as_ref() {
                        rf();
                    }
                }
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });

        Self {
            window,
            config,
            list_box,
            entries,
            filtered_entries,
            current_filter,
            current_query,
            search_seq,
            filter_and_render,
            daemon_connected,
            toast_overlay,
        }
    }

    /// Retrieves X11 Window ID (XID) for the given window surface if on X11
    pub fn get_window_xid(win: &Window) -> Option<u32> {
        let surface = win.surface()?;
        let x11_surf = surface.downcast_ref::<gdk4_x11::X11Surface>()?;
        Some(x11_surf.xid() as u32)
    }

    /// Queries the current mouse cursor location on the root screen
    pub fn get_cursor_position() -> Option<(i32, i32)> {
        let is_gnome = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase()
            .contains("gnome");
        if is_gnome {
            return None;
        }

        // 1. Try hyprctl (Hyprland Wayland)
        if which::which("hyprctl").is_ok() {
            if let Ok(output) = std::process::Command::new("hyprctl")
                .arg("cursorpos")
                .output()
            {
                if output.status.success() {
                    let s = String::from_utf8_lossy(&output.stdout);
                    let parts: Vec<&str> = s.trim().split(',').collect();
                    if parts.len() == 2 {
                        if let (Ok(x), Ok(y)) = (
                            parts[0].trim().parse::<i32>(),
                            parts[1].trim().parse::<i32>(),
                        ) {
                            if x >= 0 && y >= 0 {
                                return Some((x, y));
                            }
                        }
                    }
                }
            }
        }

        // 2. Try native X11 query pointer via x11rb
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::ConnectionExt;
        if let Ok((conn, screen_num)) = x11rb::connect(None) {
            if let Some(screen) = conn.setup().roots.get(screen_num) {
                if let Ok(cookie) = conn.query_pointer(screen.root) {
                    if let Ok(reply) = cookie.reply() {
                        if reply.root_x >= 0 && reply.root_y >= 0 {
                            return Some((reply.root_x as i32, reply.root_y as i32));
                        }
                    }
                }
            }
        }

        // 3. Fallback to xdotool getmouselocation
        if let Ok(output) = std::process::Command::new("xdotool")
            .arg("getmouselocation")
            .output()
        {
            if output.status.success() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                let mut x_val = None;
                let mut y_val = None;
                for part in out_str.split_whitespace() {
                    if let Some(val) = part.strip_prefix("x:") {
                        x_val = val.parse::<i32>().ok();
                    } else if let Some(val) = part.strip_prefix("y:") {
                        y_val = val.parse::<i32>().ok();
                    }
                }
                if let (Some(x), Some(y)) = (x_val, y_val) {
                    if x >= 0 && y >= 0 {
                        return Some((x, y));
                    }
                }
            }
        }

        None
    }

    /// Positions and presents the clipboard window near the mouse or cleanly centered
    pub fn present_near_cursor(&self) {
        gtk4::prelude::WidgetExt::realize(&self.window);

        // 1. Query mouse cursor position
        let cursor_opt = Self::get_cursor_position();

        // 2. Identify the active monitor
        let mut mon_x = 0;
        let mut mon_y = 0;
        let mut mon_w = 1920;
        let mut mon_h = 1080;

        if let Some(display) = gdk4::Display::default() {
            let monitors = display.monitors();
            if let Some((cx, cy)) = cursor_opt {
                for i in 0..monitors.n_items() {
                    if let Some(mon_obj) = monitors.item(i) {
                        if let Ok(mon) = mon_obj.downcast::<gdk4::Monitor>() {
                            let geom = mon.geometry();
                            if cx >= geom.x()
                                && cx < geom.x() + geom.width()
                                && cy >= geom.y()
                                && cy < geom.y() + geom.height()
                            {
                                mon_x = geom.x();
                                mon_y = geom.y();
                                mon_w = geom.width();
                                mon_h = geom.height();
                                break;
                            }
                        }
                    }
                }
            } else if monitors.n_items() > 0 {
                if let Some(mon_obj) = monitors.item(0) {
                    if let Ok(mon) = mon_obj.downcast::<gdk4::Monitor>() {
                        let geom = mon.geometry();
                        mon_x = geom.x();
                        mon_y = geom.y();
                        mon_w = geom.width();
                        mon_h = geom.height();
                    }
                }
            }
        }

        let max_width = (mon_w - 40).max(300);
        let max_height = (mon_h - 80).max(400);

        let target_w = (self.config.ui.window_width as i32).clamp(300, max_width);
        let target_h = (self.config.ui.window_height as i32).clamp(400, max_height);
        self.window.set_default_size(target_w, target_h);

        let win_w = self.window.width().max(target_w);
        let win_h = self.window.height().max(target_h);

        // Keep inside monitor margins
        let margin = 12;
        let mon_min_x = mon_x + margin;
        let mon_max_x = mon_x + mon_w - margin;
        let mon_min_y = mon_y + margin;
        let mon_max_y = mon_y + mon_h - margin;

        let is_gnome = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase()
            .contains("gnome");

        let (target_x, target_y) =
            if let (false, Some((cursor_x, cursor_y))) = (is_gnome, cursor_opt) {
                let offset_x = 12;
                let offset_y = 12;

                let mut tx = cursor_x + offset_x;
                if tx + win_w > mon_max_x {
                    tx = cursor_x - win_w - offset_x;
                }
                let tx = tx.clamp(mon_min_x, (mon_max_x - win_w).max(mon_min_x));

                let mut ty = cursor_y + offset_y;
                if ty + win_h > mon_max_y {
                    ty = cursor_y - win_h - offset_y;
                }
                let ty = ty.clamp(mon_min_y, (mon_max_y - win_h).max(mon_min_y));
                (tx, ty)
            } else {
                // GNOME Wayland or cursor unavailable: center cleanly on active monitor
                let tx = mon_x + (mon_w - win_w) / 2;
                let ty = mon_y + (mon_h - win_h) / 2;
                (tx.max(mon_min_x), ty.max(mon_min_y))
            };

        info!(
            "Presenting window: is_gnome={} cursor={:?} target=({},{}) monitor=({},{},{}x{})",
            is_gnome, cursor_opt, target_x, target_y, mon_x, mon_y, mon_w, mon_h
        );

        // Fetch latest entries via IPC (auto-starting daemon if offline)
        Self::refresh_entries_internal(
            Rc::clone(&self.entries),
            Rc::clone(&self.daemon_connected),
            Rc::clone(&self.filter_and_render),
        );

        // Present window
        self.window.present();

        // Immediately force activation and raise above any background/active windows
        Self::force_activate_window(&self.window);

        let win_act = self.window.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
            Self::force_activate_window(&win_act);
        });

        if !is_gnome {
            // Immediately apply move
            Self::apply_window_move(&self.window, target_x, target_y);

            // Schedule delayed moves to override window manager's initial placement
            let win_c1 = self.window.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(30), move || {
                Self::apply_window_move(&win_c1, target_x, target_y);
            });

            let win_c2 = self.window.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(80), move || {
                Self::apply_window_move(&win_c2, target_x, target_y);
            });

            let win_c3 = self.window.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                Self::apply_window_move(&win_c3, target_x, target_y);
            });
        }
    }

    /// Forces window activation and focus to the foreground across X11 and Wayland
    pub fn force_activate_window(win: &Window) {
        win.present();

        let xid_opt = Self::get_window_xid(win);
        if let Some(xid) = xid_opt {
            // 1. Send EWMH _NET_ACTIVE_WINDOW ClientMessage with real X server timestamp
            if let Ok((conn, screen_num)) = x11rb::connect(None) {
                use x11rb::connection::Connection;
                use x11rb::protocol::xproto::{
                    AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt,
                    CreateWindowAux, EventMask, PropMode, WindowClass, CLIENT_MESSAGE_EVENT,
                };
                use x11rb::wrapper::ConnectionExt as _;

                let root = conn.setup().roots[screen_num].root;
                if let (Ok(net_active_cookie), Ok(get_time_cookie)) = (
                    conn.intern_atom(false, b"_NET_ACTIVE_WINDOW"),
                    conn.intern_atom(false, b"_CLIPBOARD_GET_TIME"),
                ) {
                    if let (Ok(net_active_atom), Ok(get_time_atom)) =
                        (net_active_cookie.reply(), get_time_cookie.reply())
                    {
                        if let Ok(temp_win) = conn.generate_id() {
                            let win_aux =
                                CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE);
                            if conn
                                .create_window(
                                    0,
                                    temp_win,
                                    root,
                                    -10,
                                    -10,
                                    1,
                                    1,
                                    0,
                                    WindowClass::INPUT_OUTPUT,
                                    0,
                                    &win_aux,
                                )
                                .is_ok()
                            {
                                let _ = conn.change_property8(
                                    PropMode::REPLACE,
                                    temp_win,
                                    get_time_atom.atom,
                                    AtomEnum::CARDINAL,
                                    b"1",
                                );
                                let _ = conn.flush();

                                let mut server_time = 0u32;
                                for _ in 0..15 {
                                    if let Ok(Some(x11rb::protocol::Event::PropertyNotify(pn))) =
                                        conn.poll_for_event()
                                    {
                                        server_time = pn.time;
                                        break;
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(2));
                                }
                                let _ = conn.destroy_window(temp_win);

                                let client_msg = ClientMessageEvent {
                                    response_type: CLIENT_MESSAGE_EVENT,
                                    format: 32,
                                    sequence: 0,
                                    window: xid,
                                    type_: net_active_atom.atom,
                                    data: ClientMessageData::from([
                                        2u32, // source: pager/user direct
                                        server_time,
                                        0u32,
                                        0u32,
                                        0u32,
                                    ]),
                                };
                                let _ = conn.send_event(
                                    false,
                                    root,
                                    EventMask::SUBSTRUCTURE_REDIRECT
                                        | EventMask::SUBSTRUCTURE_NOTIFY,
                                    client_msg,
                                );
                                let _ = conn.map_window(xid);
                                let _ = conn.flush();
                            }
                        }
                    }
                }
            }

            // 2. Extra safeguard via xdotool
            let _ = std::process::Command::new("xdotool")
                .args([
                    "windowactivate",
                    &xid.to_string(),
                    "windowraise",
                    &xid.to_string(),
                ])
                .spawn();
        } else {
            let _ = std::process::Command::new("xdotool")
                .args([
                    "search",
                    "--class",
                    "clipboard-history-gui",
                    "windowactivate",
                    "windowraise",
                ])
                .spawn();
        }
    }

    /// Moves window to target coordinates across Wayland (Hyprland) and X11
    fn apply_window_move(win: &Window, target_x: i32, target_y: i32) {
        let is_gnome = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase()
            .contains("gnome");
        if is_gnome {
            return;
        }

        // 1. Hyprland Wayland move and float
        if which::which("hyprctl").is_ok() {
            let _ = std::process::Command::new("hyprctl")
                .args(["dispatch", "setfloating", "class:clipboard-history-gui"])
                .output();
            let _ = std::process::Command::new("hyprctl")
                .args([
                    "dispatch",
                    "movewindowpixel",
                    &format!("exact {} {}", target_x, target_y),
                    "class:clipboard-history-gui",
                ])
                .output();
        }

        // 2. X11 / Xwayland window move
        let xid_opt = Self::get_window_xid(win);

        if let Some(xid) = xid_opt {
            let _ = std::process::Command::new("xdotool")
                .args([
                    "windowmove",
                    &xid.to_string(),
                    &target_x.to_string(),
                    &target_y.to_string(),
                ])
                .spawn();

            if let Ok((conn, _)) = x11rb::connect(None) {
                use x11rb::connection::Connection;
                use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt};
                let aux = ConfigureWindowAux::new().x(target_x).y(target_y);
                let _ = conn.configure_window(xid, &aux);
                let _ = conn.flush();
            }
        } else {
            let _ = std::process::Command::new("xdotool")
                .args([
                    "search",
                    "--class",
                    "clipboard-history-gui",
                    "windowmove",
                    &target_x.to_string(),
                    &target_y.to_string(),
                ])
                .spawn();
        }
    }

    pub async fn ensure_daemon_running() -> Option<IpcClient> {
        if let Ok(client) = IpcClient::connect().await {
            return Some(client);
        }

        info!("Daemon not responding to IPC. Attempting auto-start...");

        // 1. Try systemctl user service if available
        if which::which("systemctl").is_ok() {
            let _ = std::process::Command::new("systemctl")
                .args([
                    "--user",
                    "import-environment",
                    "WAYLAND_DISPLAY",
                    "DISPLAY",
                    "XDG_CURRENT_DESKTOP",
                    "XDG_SESSION_TYPE",
                    "XDG_RUNTIME_DIR",
                ])
                .status();

            let _ = std::process::Command::new("systemctl")
                .args(["--user", "start", "clipboard-history.service"])
                .status();

            for _ in 0..6 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if let Ok(client) = IpcClient::connect().await {
                    info!("Connected to daemon via systemd user service.");
                    return Some(client);
                }
            }
        }

        // 2. Direct spawn fallback
        let daemon_binary = which::which("clipboard-history-daemon").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".local/bin/clipboard-history-daemon")
        });

        let mut spawn_cmd = std::process::Command::new(&daemon_binary);
        spawn_cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            spawn_cmd.process_group(0);
        }

        if spawn_cmd.spawn().is_ok() {
            for _ in 0..10 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if let Ok(client) = IpcClient::connect().await {
                    info!("Connected to daemon via spawned binary.");
                    return Some(client);
                }
            }
        }

        None
    }

    pub fn refresh_entries_internal(
        entries_cell: Rc<RefCell<Vec<ClipboardEntry>>>,
        daemon_connected_cell: Rc<RefCell<bool>>,
        fn_refresh: Rc<dyn Fn()>,
    ) {
        glib::MainContext::default().spawn_local(async move {
            match Self::ensure_daemon_running().await {
                Some(client) => {
                    *daemon_connected_cell.borrow_mut() = true;
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
                                    is_pinned: s.is_pinned,
                                    source_app: Some("Snippet".to_string()),
                                });
                            }
                        }

                        items.sort_by(|a, b| {
                            b.is_pinned
                                .cmp(&a.is_pinned)
                                .then_with(|| b.last_used_at.cmp(&a.last_used_at))
                        });

                        *entries_cell.borrow_mut() = items;
                        fn_refresh();
                    }
                }
                None => {
                    *daemon_connected_cell.borrow_mut() = false;
                    *entries_cell.borrow_mut() = Vec::new();
                    fn_refresh();
                }
            }
        });
    }
}

struct RowConfig<'a> {
    window: &'a Window,
    config: &'a AppConfig,
    list_box: &'a ListBox,
    on_pin: ActionCallback,
    on_del: ActionCallback,
    diff_target: Option<ClipboardEntry>,
    on_mark_diff: Option<EntryCallback>,
}

/// Helper to construct and configure an EntryRow with full actions, pin, and delete button handlers
fn create_configured_entry_row(
    entry: &ClipboardEntry,
    index: usize,
    cfg: RowConfig<'_>,
) -> EntryRow {
    let entry_row = EntryRow::new(entry, index);
    entry_row.row.set_widget_name(&entry.id);

    // Auto-select row on mouse hover so keyboard shortcuts (Q, P, Del, Enter) target the hovered card
    let motion_controller = EventControllerMotion::new();
    let row_hover = entry_row.row.clone();
    let list_box_hover = cfg.list_box.clone();
    motion_controller.connect_enter(move |_, _x, _y| {
        list_box_hover.select_row(Some(&row_hover));
    });
    entry_row.row.add_controller(motion_controller);

    // Actions / Transforms button (···)
    let entry_for_popover = entry.clone();
    let win_for_pop = cfg.window.clone();
    let cfg_for_pop = cfg.config.clone();
    let options = TransformsOptions {
        diff_target: cfg.diff_target,
        on_pin: Some(cfg.on_pin),
        on_del: Some(cfg.on_del),
        on_mark_diff: cfg.on_mark_diff,
    };
    entry_row.actions_btn.connect_clicked(move |btn| {
        show_transforms_popover(
            btn,
            &entry_for_popover,
            &win_for_pop,
            &cfg_for_pop,
            TransformsOptions {
                diff_target: options.diff_target.clone(),
                on_pin: options.on_pin.as_ref().map(Rc::clone),
                on_del: options.on_del.as_ref().map(Rc::clone),
                on_mark_diff: options.on_mark_diff.as_ref().map(Rc::clone),
            },
        );
    });

    entry_row
}

#![cfg(feature = "gtk")]

use crate::injector::InjectorCascade;
use crate::list_row::{clean_source_app_name, format_relative_time};
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use clipboard_history_core::ipc::IpcClient;
use clipboard_history_core::transforms::{TextTransforms, UrlCleaner};
use gdk4::prelude::*;
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Entry, Label, Orientation, Popover, Window};
use std::rc::Rc;
use tracing::info;

#[cfg(feature = "gtk")]
pub type ActionCallback = Rc<dyn Fn(&str)>;
#[cfg(feature = "gtk")]
pub type EntryCallback = Rc<dyn Fn(&ClipboardEntry)>;

#[cfg(feature = "gtk")]
#[derive(Default)]
pub struct TransformsOptions {
    pub diff_target: Option<ClipboardEntry>,
    pub on_pin: Option<ActionCallback>,
    pub on_del: Option<ActionCallback>,
    pub on_mark_diff: Option<EntryCallback>,
}

#[cfg(feature = "gtk")]
pub fn show_transforms_popover(
    parent: &impl IsA<gtk4::Widget>,
    entry: &ClipboardEntry,
    main_window: &Window,
    config: &AppConfig,
    options: TransformsOptions,
) {
    let TransformsOptions {
        diff_target,
        on_pin,
        on_del,
        on_mark_diff,
    } = options;

    let popover = Popover::new();
    popover.set_parent(parent);
    popover.set_autohide(true);
    popover.add_css_class("transforms-popover");

    let container = GtkBox::new(Orientation::Vertical, 6);
    container.set_margin_start(10);
    container.set_margin_end(10);
    container.set_margin_top(8);
    container.set_margin_bottom(10);

    // ==========================================
    // 1. Sleek Modern Meta Card (Obsidian / Mica)
    // ==========================================
    let meta_card = GtkBox::new(Orientation::Vertical, 4);
    meta_card.add_css_class("popover-meta-card");

    // Top row: App Name (Left) • Relative Time (Right)
    let meta_top_row = GtkBox::new(Orientation::Horizontal, 6);
    meta_top_row.set_valign(Align::Center);

    let clean_app = clean_source_app_name(entry.source_app.as_deref());
    let app_icon_label = if entry.source_app.as_deref() == Some("Snippet") {
        format!("📝 {}", clean_app)
    } else {
        format!("📱 {}", clean_app)
    };
    let app_label = Label::new(Some(&app_icon_label));
    app_label.add_css_class("meta-app-name");
    app_label.set_xalign(0.0);
    meta_top_row.append(&app_label);

    let meta_spacer = GtkBox::new(Orientation::Horizontal, 0);
    meta_spacer.set_hexpand(true);
    meta_top_row.append(&meta_spacer);

    let time_str = format_relative_time(entry.created_at);
    let time_label = Label::new(Some(&format!("🕒 {}", time_str)));
    time_label.add_css_class("meta-time-text");
    meta_top_row.append(&time_label);

    meta_card.append(&meta_top_row);

    // Bottom row: Details (Characters, Words, Lines, or Image dimensions / Size)
    let text_content = entry
        .text_content
        .clone()
        .unwrap_or_else(|| entry.preview.clone());

    let details_str = match entry.entry_type {
        EntryType::Image => {
            let kb = entry.size_bytes as f64 / 1024.0;
            if kb > 1024.0 {
                format!("Image • {:.2} MB", kb / 1024.0)
            } else {
                format!("Image • {:.1} KB", kb)
            }
        }
        _ => {
            let char_count = text_content.chars().count();
            let word_count = text_content.split_whitespace().count();
            let line_count = text_content.lines().count().max(1);
            format!(
                "{} characters • {} words • {} lines",
                char_count, word_count, line_count
            )
        }
    };
    let details_label = Label::new(Some(&details_str));
    details_label.add_css_class("meta-details-text");
    details_label.set_xalign(0.0);
    meta_card.append(&details_label);

    container.append(&meta_card);

    // ==========================================
    // Quick Actions (Pin & Delete)
    // ==========================================
    let actions_row = GtkBox::new(Orientation::Horizontal, 6);
    actions_row.set_margin_top(4);
    actions_row.set_margin_bottom(4);

    let pin_label = if entry.is_pinned {
        "📌 Unpin (P)"
    } else {
        "📌 Pin (P)"
    };
    let popover_pin_btn = Button::with_label(pin_label);
    popover_pin_btn.add_css_class("popover-transform-btn");
    popover_pin_btn.set_hexpand(true);

    let popover_del_btn = Button::with_label("🗑️ Delete (Del)");
    popover_del_btn.add_css_class("popover-transform-btn");
    popover_del_btn.add_css_class("destructive-action");
    popover_del_btn.set_hexpand(true);

    actions_row.append(&popover_pin_btn);
    actions_row.append(&popover_del_btn);
    container.append(&actions_row);

    if let Some(pin_fn) = on_pin {
        let id = entry.id.clone();
        let p = popover.clone();
        popover_pin_btn.connect_clicked(move |_| {
            p.popdown();
            pin_fn(&id);
        });
    }

    if let Some(del_fn) = on_del {
        let id = entry.id.clone();
        let p = popover.clone();
        popover_del_btn.connect_clicked(move |_| {
            p.popdown();
            del_fn(&id);
        });
    }

    // ==========================================
    // Visual Diff Comparison Section (Text only)
    // ==========================================
    if entry.entry_type != EntryType::Image {
        if let Some(ref target) = diff_target {
            if target.entry_type != EntryType::Image && target.id != entry.id {
                let diff_row = GtkBox::new(Orientation::Horizontal, 6);
                diff_row.set_margin_top(4);
                diff_row.set_margin_bottom(2);

                let diff_btn = Button::with_label("🔀 Compare Diff (D)");
                diff_btn.add_css_class("popover-transform-btn");
                diff_btn.add_css_class("suggested-action");
                diff_btn.set_hexpand(true);
                let target_preview = target.preview.chars().take(35).collect::<String>();
                diff_btn.set_tooltip_text(Some(&format!(
                    "Compare diff with: \"{}...\"",
                    target_preview
                )));

                let text_curr = text_content.clone();
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
                let win_diff = main_window.clone();
                let pop_diff = popover.clone();

                diff_btn.connect_clicked(move |_| {
                    pop_diff.popdown();
                    win_diff.set_visible(false);
                    crate::diff_dialog::DiffDialog::show(
                        None,
                        text_other.clone(),
                        text_curr.clone(),
                        label_other.clone(),
                        label_curr.clone(),
                    );
                });
                diff_row.append(&diff_btn);

                if let Some(ref mark_fn) = on_mark_diff {
                    let mark_btn = Button::with_label("📑 Mark");
                    mark_btn.add_css_class("popover-transform-btn");
                    mark_btn.set_tooltip_text(Some(
                        "Mark this entry as Diff baseline to compare with any other item",
                    ));
                    let entry_to_mark = entry.clone();
                    let pop_mark = popover.clone();
                    let mark_call = Rc::clone(mark_fn);
                    mark_btn.connect_clicked(move |_| {
                        pop_mark.popdown();
                        mark_call(&entry_to_mark);
                    });
                    diff_row.append(&mark_btn);
                }

                container.append(&diff_row);
            }
        } else if let Some(ref mark_fn) = on_mark_diff {
            let diff_row = GtkBox::new(Orientation::Horizontal, 6);
            diff_row.set_margin_top(4);
            diff_row.set_margin_bottom(2);

            let mark_btn = Button::with_label("📑 Mark for Diff Comparison");
            mark_btn.add_css_class("popover-transform-btn");
            mark_btn.set_hexpand(true);
            mark_btn.set_tooltip_text(Some(
                "Mark this entry as Diff baseline to compare with any other item",
            ));
            let entry_to_mark = entry.clone();
            let pop_mark = popover.clone();
            let mark_call = Rc::clone(mark_fn);
            mark_btn.connect_clicked(move |_| {
                pop_mark.popdown();
                mark_call(&entry_to_mark);
            });
            diff_row.append(&mark_btn);
            container.append(&diff_row);
        }
    }

    let move_to_top = config.paste.move_pasted_to_top;

    // ==========================================
    // 2. Color Detection & Palette (if applicable)
    // ==========================================
    if let Some(color) = TextTransforms::detect_color(&text_content) {
        let color_sec_title = Label::new(Some("Color Palette"));
        color_sec_title.add_css_class("popover-section-title");
        color_sec_title.set_xalign(0.0);
        container.append(&color_sec_title);

        let color_box = GtkBox::new(Orientation::Horizontal, 8);
        color_box.set_valign(Align::Center);

        let swatch = gtk4::DrawingArea::new();
        swatch.set_content_width(24);
        swatch.set_content_height(18);
        let r = color.r as f64 / 255.0;
        let g = color.g as f64 / 255.0;
        let b = color.b as f64 / 255.0;
        swatch.set_draw_func(move |_, cr, width, height| {
            let w = width as f64;
            let h = height as f64;
            let rad = 4.0;
            cr.new_sub_path();
            cr.arc(w - rad, rad, rad, -std::f64::consts::FRAC_PI_2, 0.0);
            cr.arc(w - rad, h - rad, rad, 0.0, std::f64::consts::FRAC_PI_2);
            cr.arc(
                rad,
                h - rad,
                rad,
                std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
            );
            cr.arc(
                rad,
                rad,
                rad,
                std::f64::consts::PI,
                3.0 * std::f64::consts::FRAC_PI_2,
            );
            cr.close_path();
            cr.set_source_rgb(r, g, b);
            let _ = cr.fill_preserve();
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        });
        color_box.append(&swatch);

        let color_label = Label::new(Some(&format!(
            "{} (RGB: {}, {}, {})",
            color.hex, color.r, color.g, color.b
        )));
        color_label.add_css_class("meta-details-text");
        color_box.append(&color_label);
        container.append(&color_box);

        let fmt_box = GtkBox::new(Orientation::Horizontal, 4);
        let hex_btn = Button::with_label("Hex");
        let rgb_btn = Button::with_label("RGB");
        let hsl_btn = Button::with_label("HSL");
        let css_btn = Button::with_label("CSS var");
        let glsl_btn = Button::with_label("GLSL");

        hex_btn.add_css_class("popover-transform-btn");
        rgb_btn.add_css_class("popover-transform-btn");
        hsl_btn.add_css_class("popover-transform-btn");
        css_btn.add_css_class("popover-transform-btn");
        glsl_btn.add_css_class("popover-transform-btn");

        fmt_box.append(&hex_btn);
        fmt_box.append(&rgb_btn);
        fmt_box.append(&hsl_btn);
        fmt_box.append(&css_btn);
        fmt_box.append(&glsl_btn);
        container.append(&fmt_box);

        let col_clone = color.clone();
        let win = main_window.clone();
        hex_btn.connect_clicked(move |_| {
            if let Some(display) = gdk4::Display::default() {
                display.clipboard().set_text(&col_clone.to_hex_string());
            }
            win.set_visible(false);
        });

        let col_clone = color.clone();
        let win = main_window.clone();
        rgb_btn.connect_clicked(move |_| {
            if let Some(display) = gdk4::Display::default() {
                display.clipboard().set_text(&col_clone.to_rgb_string());
            }
            win.set_visible(false);
        });

        let col_clone = color.clone();
        let win = main_window.clone();
        hsl_btn.connect_clicked(move |_| {
            if let Some(display) = gdk4::Display::default() {
                display.clipboard().set_text(&col_clone.to_hsl_string());
            }
            win.set_visible(false);
        });

        let col_clone = color.clone();
        let win = main_window.clone();
        css_btn.connect_clicked(move |_| {
            if let Some(display) = gdk4::Display::default() {
                display
                    .clipboard()
                    .set_text(&col_clone.to_css_var("primary-color"));
            }
            win.set_visible(false);
        });

        let col_clone = color.clone();
        let win = main_window.clone();
        glsl_btn.connect_clicked(move |_| {
            if let Some(display) = gdk4::Display::default() {
                display.clipboard().set_text(&col_clone.to_glsl_vec4());
            }
            win.set_visible(false);
        });
    }

    // ==========================================
    // 3. Quick Actions
    // ==========================================
    let actions_sec_title = Label::new(Some("Quick Actions"));
    actions_sec_title.add_css_class("popover-section-title");
    actions_sec_title.set_xalign(0.0);
    container.append(&actions_sec_title);

    let actions_row1 = GtkBox::new(Orientation::Horizontal, 4);

    // Copy to Clipboard (Only copies to clipboard, closes window)
    let copy_btn = Button::with_label("📋 Copy to Clipboard");
    copy_btn.add_css_class("popover-action-btn");
    copy_btn.add_css_class("suggested");
    let text_for_copy = text_content.clone();
    let win_for_copy = main_window.clone();
    let is_image_entry = entry.entry_type == EntryType::Image;
    let image_blob_hash = entry.blob_hash.clone();
    copy_btn.connect_clicked(move |_| {
        if is_image_entry {
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
            if let Some(p) = image_blob_hash.as_deref().and_then(find_blob) {
                if let Ok(bytes) = std::fs::read(&p) {
                    glib::MainContext::default().spawn_local(async move {
                        clipboard_history_core::clipboard::copy_image_to_system_clipboard(
                            &bytes,
                            Some("image/png"),
                        )
                        .await;
                    });
                }
            }
        } else if let Some(display) = gdk4::Display::default() {
            display.clipboard().set_text(&text_for_copy);
        }
        win_for_copy.set_visible(false);
    });
    actions_row1.append(&copy_btn);

    // Paste Plain Text & QR Code (only applicable to text entries)
    if entry.entry_type != EntryType::Image {
        // Paste Plain Text (Strips formatting and pastes)
        let plain_btn = Button::with_label("📄 Plain Text");
        plain_btn.add_css_class("popover-action-btn");
        let t_plain = text_content.clone();
        let win_plain = main_window.clone();
        let auto_paste = config.paste.auto_paste;
        let id_plain = entry.id.clone();
        plain_btn.connect_clicked(move |_| {
            let stripped = TextTransforms::strip_formatting(&t_plain);
            apply_transformed_paste(
                stripped,
                win_plain.clone(),
                auto_paste,
                id_plain.clone(),
                move_to_top,
            );
        });
        actions_row1.append(&plain_btn);

        // View QR Code
        let qr_btn = Button::with_label("📱 QR Code");
        qr_btn.add_css_class("popover-action-btn");
        let t_qr = text_content.clone();
        let win_qr = main_window.clone();
        qr_btn.connect_clicked(move |_| {
            show_qr_dialog(&win_qr, &t_qr);
        });
        actions_row1.append(&qr_btn);
    }

    container.append(&actions_row1);

    // Optional Action: OCR for image entries
    if let Some(blob_hash) = &entry.blob_hash {
        let ocr_box = GtkBox::new(Orientation::Horizontal, 4);
        let ocr_btn = Button::with_label("🔍 Extract Text (OCR)");
        ocr_btn.add_css_class("popover-action-btn");
        ocr_btn.add_css_class("suggested");
        ocr_box.append(&ocr_btn);
        container.append(&ocr_box);

        let hash_cloned = blob_hash.clone();
        let win_ocr = main_window.clone();
        let pop_ocr = popover.clone();
        let btn_ocr = ocr_btn.clone();
        let entry_id = entry.id.clone();
        let auto_paste = config.paste.auto_paste;

        ocr_btn.connect_clicked(move |_| {
            let hash = hash_cloned.clone();
            let win = win_ocr.clone();
            let pop = pop_ocr.clone();
            let btn = btn_ocr.clone();
            let id = entry_id.clone();

            btn.set_sensitive(false);
            btn.set_label("⏳ Extracting text (OCR)...");

            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = clipboard_history_core::ipc::IpcClient::connect().await {
                    match client
                        .send(&clipboard_history_core::ipc::IpcRequest::PerformOcr {
                            blob_hash: hash,
                        })
                        .await
                    {
                        Ok(clipboard_history_core::ipc::IpcResponse::OcrResult { text }) => {
                            let trimmed = text.trim();
                            if trimmed.is_empty() {
                                btn.set_label("⚠️ No text found in image");
                                btn.set_sensitive(true);
                                let b = btn.clone();
                                glib::timeout_add_local_once(
                                    std::time::Duration::from_secs(3),
                                    move || {
                                        b.set_label("🔍 Extract Text (OCR)");
                                    },
                                );
                            } else {
                                pop.popdown();
                                apply_transformed_paste(
                                    text.clone(),
                                    win.clone(),
                                    auto_paste,
                                    id,
                                    move_to_top,
                                );
                                info!(
                                    "OCR extracted {} characters and copied to clipboard",
                                    text.len()
                                );
                            }
                        }
                        Ok(clipboard_history_core::ipc::IpcResponse::Error(e)) => {
                            tracing::warn!("OCR request failed: {}", e);
                            btn.set_sensitive(true);
                            if e.contains("not installed") {
                                btn.set_label("⚠️ Tesseract not installed");
                                let b_install = btn.clone();
                                show_ocr_missing_dialog(
                                    &win,
                                    Some(std::rc::Rc::new(move || {
                                        b_install.set_label("⏳ Installing in terminal...");
                                    })),
                                );
                            } else {
                                btn.set_label("⚠️ OCR failed");
                            }
                            let b = btn.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_secs(6),
                                move || {
                                    b.set_label("🔍 Extract Text (OCR)");
                                },
                            );
                        }
                        _ => {
                            btn.set_sensitive(true);
                            btn.set_label("🔍 Extract Text (OCR)");
                        }
                    }
                } else {
                    btn.set_sensitive(true);
                    btn.set_label("⚠️ Daemon offline");
                    let b = btn.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_secs(3), move || {
                        b.set_label("🔍 Extract Text (OCR)");
                    });
                }
            });
        });
    }

    // Optional Action: De-Track if text contains tracking URL parameters
    if UrlCleaner::is_url(&text_content) || UrlCleaner::has_tracking_params(&text_content) {
        let clean_box = GtkBox::new(Orientation::Horizontal, 4);
        let clean_btn = Button::with_label("🛡️ Clean Tracking Params (De-Track)");
        clean_btn.add_css_class("popover-action-btn");
        clean_btn.add_css_class("suggested");
        clean_box.append(&clean_btn);
        container.append(&clean_box);

        let t_clean = text_content.clone();
        let win_clean = main_window.clone();
        let auto_paste = config.paste.auto_paste;
        let id_clean = entry.id.clone();
        clean_btn.connect_clicked(move |_| {
            let cleaned = UrlCleaner::clean_text_urls(&t_clean);
            apply_transformed_paste(
                cleaned,
                win_clean.clone(),
                auto_paste,
                id_clean.clone(),
                move_to_top,
            );
        });
    }

    // ==========================================
    // 4. Text Transformations
    // ==========================================
    if entry.entry_type != EntryType::Image {
        let trans_sec_title = Label::new(Some("Text Transformations"));
        trans_sec_title.add_css_class("popover-section-title");
        trans_sec_title.set_xalign(0.0);
        container.append(&trans_sec_title);

        // Row 1: UPPER, lower, Title
        let case_row1 = GtkBox::new(Orientation::Horizontal, 4);
        let upper_btn = Button::with_label("UPPERCASE");
        let lower_btn = Button::with_label("lowercase");
        let title_btn = Button::with_label("Title Case");
        upper_btn.add_css_class("popover-transform-btn");
        lower_btn.add_css_class("popover-transform-btn");
        title_btn.add_css_class("popover-transform-btn");
        upper_btn.set_hexpand(true);
        lower_btn.set_hexpand(true);
        title_btn.set_hexpand(true);
        case_row1.append(&upper_btn);
        case_row1.append(&lower_btn);
        case_row1.append(&title_btn);
        container.append(&case_row1);

        // Row 2: snake_case, kebab-case, camelCase
        let case_row2 = GtkBox::new(Orientation::Horizontal, 4);
        let snake_btn = Button::with_label("snake_case");
        let kebab_btn = Button::with_label("kebab-case");
        let camel_btn = Button::with_label("camelCase");
        snake_btn.add_css_class("popover-transform-btn");
        kebab_btn.add_css_class("popover-transform-btn");
        camel_btn.add_css_class("popover-transform-btn");
        snake_btn.set_hexpand(true);
        kebab_btn.set_hexpand(true);
        camel_btn.set_hexpand(true);
        case_row2.append(&snake_btn);
        case_row2.append(&kebab_btn);
        case_row2.append(&camel_btn);
        container.append(&case_row2);

        let auto_paste = config.paste.auto_paste;

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        upper_btn.connect_clicked(move |_| {
            apply_transformed_paste(
                TextTransforms::to_uppercase(&t),
                win.clone(),
                auto_paste,
                id.clone(),
                move_to_top,
            );
        });

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        lower_btn.connect_clicked(move |_| {
            apply_transformed_paste(
                TextTransforms::to_lowercase(&t),
                win.clone(),
                auto_paste,
                id.clone(),
                move_to_top,
            );
        });

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        title_btn.connect_clicked(move |_| {
            apply_transformed_paste(
                TextTransforms::to_title_case(&t),
                win.clone(),
                auto_paste,
                id.clone(),
                move_to_top,
            );
        });

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        snake_btn.connect_clicked(move |_| {
            apply_transformed_paste(
                TextTransforms::to_snake_case(&t),
                win.clone(),
                auto_paste,
                id.clone(),
                move_to_top,
            );
        });

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        kebab_btn.connect_clicked(move |_| {
            apply_transformed_paste(
                TextTransforms::to_kebab_case(&t),
                win.clone(),
                auto_paste,
                id.clone(),
                move_to_top,
            );
        });

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        camel_btn.connect_clicked(move |_| {
            apply_transformed_paste(
                TextTransforms::to_camel_case(&t),
                win.clone(),
                auto_paste,
                id.clone(),
                move_to_top,
            );
        });

        // ==========================================
        // 5. Developer Tools
        // ==========================================
        let dev_sec_title = Label::new(Some("Developer Tools"));
        dev_sec_title.add_css_class("popover-section-title");
        dev_sec_title.set_xalign(0.0);
        container.append(&dev_sec_title);

        let dev_row1 = GtkBox::new(Orientation::Horizontal, 4);
        let json_btn = Button::with_label("Format JSON");
        let b64_enc_btn = Button::with_label("Base64 Encode");
        let b64_dec_btn = Button::with_label("Base64 Decode");
        json_btn.add_css_class("popover-transform-btn");
        b64_enc_btn.add_css_class("popover-transform-btn");
        b64_dec_btn.add_css_class("popover-transform-btn");
        json_btn.set_hexpand(true);
        b64_enc_btn.set_hexpand(true);
        b64_dec_btn.set_hexpand(true);
        dev_row1.append(&json_btn);
        dev_row1.append(&b64_enc_btn);
        dev_row1.append(&b64_dec_btn);
        container.append(&dev_row1);

        let dev_row2 = GtkBox::new(Orientation::Horizontal, 4);
        let url_enc_btn = Button::with_label("URL Encode");
        let url_dec_btn = Button::with_label("URL Decode");
        url_enc_btn.add_css_class("popover-transform-btn");
        url_dec_btn.add_css_class("popover-transform-btn");
        url_enc_btn.set_hexpand(true);
        url_dec_btn.set_hexpand(true);
        dev_row2.append(&url_enc_btn);
        dev_row2.append(&url_dec_btn);
        container.append(&dev_row2);

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        json_btn.connect_clicked(move |_| {
            if let Ok(pretty) = TextTransforms::json_prettify(&t) {
                apply_transformed_paste(pretty, win.clone(), auto_paste, id.clone(), move_to_top);
            }
        });

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        b64_enc_btn.connect_clicked(move |_| {
            apply_transformed_paste(
                TextTransforms::base64_encode(&t),
                win.clone(),
                auto_paste,
                id.clone(),
                move_to_top,
            );
        });

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        b64_dec_btn.connect_clicked(move |_| {
            if let Ok(decoded) = TextTransforms::base64_decode(&t) {
                apply_transformed_paste(decoded, win.clone(), auto_paste, id.clone(), move_to_top);
            }
        });

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        url_enc_btn.connect_clicked(move |_| {
            apply_transformed_paste(
                TextTransforms::url_encode(&t),
                win.clone(),
                auto_paste,
                id.clone(),
                move_to_top,
            );
        });

        let t = text_content.clone();
        let win = main_window.clone();
        let id = entry.id.clone();
        url_dec_btn.connect_clicked(move |_| {
            if let Ok(decoded) = TextTransforms::url_decode(&t) {
                apply_transformed_paste(decoded, win.clone(), auto_paste, id.clone(), move_to_top);
            }
        });
    }

    popover.set_child(Some(&container));
    popover.popup();
}

#[cfg(feature = "gtk")]
pub fn show_qr_dialog(parent: &Window, text: &str) {
    let dialog = Window::builder()
        .title("QR Code")
        .transient_for(parent)
        .modal(true)
        .default_width(320)
        .default_height(360)
        .build();

    let root = GtkBox::new(Orientation::Vertical, 12);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.set_margin_top(16);
    root.set_margin_bottom(16);

    let title = Label::new(Some("Scan with Mobile Device"));
    title.add_css_class("heading");
    root.append(&title);

    match TextTransforms::generate_qr_svg(text) {
        Ok(svg_data) => {
            let bytes = glib::Bytes::from(svg_data.as_bytes());
            if let Ok(texture) = gdk4::Texture::from_bytes(&bytes) {
                let picture = gtk4::Picture::for_paintable(&texture);
                picture.set_can_shrink(true);
                picture.set_size_request(240, 240);
                root.append(&picture);
            } else {
                let err_label = Label::new(Some("Failed to generate QR SVG image"));
                root.append(&err_label);
            }
        }
        Err(e) => {
            let err_label = Label::new(Some(&format!("Failed to generate QR: {}", e)));
            root.append(&err_label);
        }
    }

    let close_btn = Button::with_label("Close");
    close_btn.add_css_class("suggested-action");
    let d = dialog.clone();
    close_btn.connect_clicked(move |_| {
        d.close();
    });
    root.append(&close_btn);

    dialog.set_child(Some(&root));
    dialog.present();
}

#[cfg(feature = "gtk")]
pub fn detect_ocr_install_command() -> (&'static str, &'static str) {
    if which::which("pacman").is_ok() {
        (
            "Arch Linux / CachyOS",
            "sudo pacman -S --needed tesseract tesseract-data-eng tesseract-data-tur",
        )
    } else if which::which("apt-get").is_ok() || which::which("apt").is_ok() {
        (
            "Debian / Ubuntu",
            "sudo apt update && sudo apt install -y tesseract-ocr tesseract-ocr-tur tesseract-ocr-eng",
        )
    } else if which::which("dnf").is_ok() {
        (
            "Fedora / RHEL",
            "sudo dnf install -y tesseract tesseract-langpack-tur tesseract-langpack-eng",
        )
    } else if which::which("zypper").is_ok() {
        (
            "openSUSE",
            "sudo zypper install -y tesseract tesseract-ocr-traineddata-tur tesseract-ocr-traineddata-eng",
        )
    } else {
        (
            "Arch Linux / CachyOS",
            "sudo pacman -S --needed tesseract tesseract-data-eng tesseract-data-tur",
        )
    }
}

#[cfg(feature = "gtk")]
fn create_ocr_install_script(
    distro_name: &str,
    install_cmd: &str,
) -> std::io::Result<std::path::PathBuf> {
    let script_path = std::env::temp_dir().join("clipboard-history-install-ocr.sh");
    let script_content = format!(
        r#"#!/usr/bin/env bash
set -e

echo "================================================================="
echo "  Clipboard History: Installing Tesseract OCR ({})"
echo "================================================================="
echo ""
echo "Running: {}"
{}
echo ""
echo "=== Downloading high-accuracy OCR models (tessdata_best) ==="
mkdir -p "${{HOME}}/.local/share/clipboard-history/tessdata"
curl -L -o "${{HOME}}/.local/share/clipboard-history/tessdata/tur.traineddata" "https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/main/tur.traineddata" || true
curl -L -o "${{HOME}}/.local/share/clipboard-history/tessdata/eng.traineddata" "https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/main/eng.traineddata" || true
echo ""
echo "================================================================="
echo "  Installation completed successfully!"
echo "  Press Enter to close this window..."
echo "================================================================="
read -r _dummy
"#,
        distro_name, install_cmd, install_cmd
    );

    std::fs::write(&script_path, script_content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(script_path)
}

#[cfg(feature = "gtk")]
pub fn show_ocr_missing_dialog(parent: &Window, on_install: Option<std::rc::Rc<dyn Fn()>>) {
    let (distro_name, install_cmd) = detect_ocr_install_command();

    let dialog = Window::builder()
        .title("Tesseract OCR Required")
        .transient_for(parent)
        .modal(true)
        .default_width(480)
        .default_height(280)
        .resizable(false)
        .build();

    let root = GtkBox::new(Orientation::Vertical, 12);
    root.set_margin_start(18);
    root.set_margin_end(18);
    root.set_margin_top(16);
    root.set_margin_bottom(16);

    let title = Label::new(Some("🔍 Install Tesseract OCR?"));
    title.add_css_class("heading");
    title.set_xalign(0.0);
    root.append(&title);

    let msg = Label::new(Some(&format!(
        "Tesseract OCR is required to extract text from images.\n\nYou can launch the installation terminal directly or copy the command for your system ({}):",
        distro_name
    )));
    msg.set_wrap(true);
    msg.set_xalign(0.0);
    root.append(&msg);

    // Monospace command entry
    let cmd_entry = Entry::new();
    cmd_entry.set_text(install_cmd);
    cmd_entry.set_editable(false);
    cmd_entry.set_hexpand(true);
    cmd_entry.add_css_class("monospace");
    root.append(&cmd_entry);

    // Dynamic status label for feedback / errors
    let status_label = Label::new(None);
    status_label.set_wrap(true);
    status_label.set_xalign(0.0);
    status_label.set_visible(false);
    root.append(&status_label);

    let btn_box = GtkBox::new(Orientation::Horizontal, 8);
    btn_box.set_halign(gtk4::Align::End);
    btn_box.set_margin_top(6);

    let cancel_btn = Button::with_label("Cancel");
    let d_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        d_cancel.close();
    });
    btn_box.append(&cancel_btn);

    // Copy command button
    let copy_btn = Button::with_label("📋 Copy Command");
    let c_btn = copy_btn.clone();
    let cmd_str = install_cmd.to_string();
    copy_btn.connect_clicked(move |_| {
        if let Some(display) = gdk4::Display::default() {
            display.clipboard().set_text(&cmd_str);
        }
        c_btn.set_label("✓ Copied!");
        let b = c_btn.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
            b.set_label("📋 Copy Command");
        });
    });
    btn_box.append(&copy_btn);

    let install_btn = Button::with_label("🚀 Install in Terminal");
    install_btn.add_css_class("suggested-action");
    let d_install = dialog.clone();
    let s_label = status_label.clone();
    install_btn.connect_clicked(move |_| {
        if launch_ocr_terminal_install() {
            if let Some(ref cb) = on_install {
                cb();
            }
            d_install.close();
        } else {
            s_label.set_text("⚠️ Could not detect or launch a terminal emulator automatically.\nPlease click 'Copy Command' above and run it in your terminal.");
            s_label.add_css_class("error");
            s_label.set_visible(true);
        }
    });
    btn_box.append(&install_btn);

    root.append(&btn_box);

    dialog.set_child(Some(&root));
    dialog.present();
}

#[cfg(feature = "gtk")]
fn launch_ocr_terminal_install() -> bool {
    let (distro_name, install_cmd) = detect_ocr_install_command();
    let script_path = match create_ocr_install_script(distro_name, install_cmd) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Failed to create OCR install script: {}", e);
            return false;
        }
    };
    let script_str = script_path.to_string_lossy().to_string();

    let mut candidate_commands: Vec<(String, Vec<String>)> = Vec::new();

    // 1. $TERMINAL environment variable if configured by user
    if let Ok(user_term) = std::env::var("TERMINAL") {
        let trimmed = user_term.trim();
        if !trimmed.is_empty() {
            candidate_commands.push((
                trimmed.to_string(),
                vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
            ));
            candidate_commands.push((
                trimmed.to_string(),
                vec!["--".to_string(), "bash".to_string(), script_str.clone()],
            ));
            candidate_commands.push((
                trimmed.to_string(),
                vec!["bash".to_string(), script_str.clone()],
            ));
        }
    }

    // 2. Standard freedesktop terminal launcher (supported on Arch, CachyOS, Fedora, etc.)
    candidate_commands.push((
        "xdg-terminal-exec".to_string(),
        vec!["--".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "xdg-terminal-exec".to_string(),
        vec!["bash".to_string(), script_str.clone()],
    ));

    // 3. Modern & Popular Linux Terminals (including CachyOS favorites)
    candidate_commands.push((
        "ghostty".to_string(),
        vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "alacritty".to_string(),
        vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "kitty".to_string(),
        vec!["--".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "wezterm".to_string(),
        vec![
            "start".to_string(),
            "--".to_string(),
            "bash".to_string(),
            script_str.clone(),
        ],
    ));
    candidate_commands.push((
        "ptyxis".to_string(),
        vec!["--".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "kgx".to_string(),
        vec!["-e".to_string(), format!("bash {}", script_str)],
    ));
    candidate_commands.push((
        "gnome-terminal".to_string(),
        vec!["--".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "konsole".to_string(),
        vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "xfce4-terminal".to_string(),
        vec!["-x".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "foot".to_string(),
        vec!["bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "footclient".to_string(),
        vec!["bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "terminator".to_string(),
        vec!["-x".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "tilix".to_string(),
        vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "qterminal".to_string(),
        vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "mate-terminal".to_string(),
        vec!["-e".to_string(), format!("bash {}", script_str)],
    ));
    candidate_commands.push((
        "lxterminal".to_string(),
        vec!["-e".to_string(), format!("bash {}", script_str)],
    ));
    candidate_commands.push((
        "x-terminal-emulator".to_string(),
        vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "xterm".to_string(),
        vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "urxvt".to_string(),
        vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
    ));
    candidate_commands.push((
        "rxvt".to_string(),
        vec!["-e".to_string(), "bash".to_string(), script_str.clone()],
    ));

    for (bin, args) in candidate_commands {
        if which::which(&bin).is_ok() {
            match std::process::Command::new(&bin).args(&args).spawn() {
                Ok(_) => {
                    tracing::info!(
                        "Successfully launched OCR install script using terminal: {}",
                        bin
                    );
                    return true;
                }
                Err(e) => {
                    tracing::warn!("Failed to spawn terminal '{}': {}", bin, e);
                }
            }
        }
    }

    false
}

fn apply_transformed_paste(
    transformed: String,
    win: Window,
    auto_paste: bool,
    entry_id: String,
    move_to_top: bool,
) {
    if let Some(display) = gdk4::Display::default() {
        display.clipboard().set_text(&transformed);
    }
    win.set_visible(false);
    glib::MainContext::default().spawn_local(async move {
        let hash = clipboard_history_core::blob::BlobStore::compute_hash(transformed.as_bytes());
        if let Ok(client) = IpcClient::connect().await {
            let _ = client.prepare_paste(&hash).await;
            if move_to_top {
                let _ = client.touch_entry(&entry_id).await;
            }
        }
        clipboard_history_core::copy_to_system_clipboard(&transformed).await;
        if auto_paste {
            let delay_ms = AppConfig::load_or_default().paste.effective_delay_ms();
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            let cascade = InjectorCascade::new();
            let (name, success) = cascade.execute_paste().await;
            info!(
                "Transformed paste executed: injector={}, success={}",
                name, success
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ocr_install_command() {
        let (distro, cmd) = detect_ocr_install_command();
        assert!(!distro.is_empty());
        assert!(!cmd.is_empty());
        assert!(cmd.contains("tesseract"));
    }

    #[test]
    fn test_create_ocr_install_script() {
        let script = create_ocr_install_script("TestDistro", "echo test").unwrap();
        assert!(script.exists());
        let content = std::fs::read_to_string(&script).unwrap();
        assert!(content.contains("TestDistro"));
        assert!(content.contains("echo test"));
        let _ = std::fs::remove_file(script);
    }
}

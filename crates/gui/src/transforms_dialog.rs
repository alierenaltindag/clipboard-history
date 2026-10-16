#![cfg(feature = "gtk")]

use crate::injector::InjectorCascade;
use crate::list_row::{clean_source_app_name, format_relative_time};
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use clipboard_history_core::transforms::{TextTransforms, UrlCleaner};
use gdk4::prelude::*;
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Label, Orientation, Popover, Window};
use std::rc::Rc;
use tracing::info;

#[cfg(feature = "gtk")]
pub type ActionCallback = Rc<dyn Fn(&str)>;

#[cfg(feature = "gtk")]
pub fn show_transforms_popover(
    parent: &impl IsA<gtk4::Widget>,
    entry: &ClipboardEntry,
    main_window: &Window,
    config: &AppConfig,
    on_pin: Option<ActionCallback>,
    on_del: Option<ActionCallback>,
) {
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

    // Helper for pasting transformed text
    let make_paste_closure = |transformed: String, win: Window, auto_paste: bool| {
        move || {
            if let Some(display) = gdk4::Display::default() {
                display.clipboard().set_text(&transformed);
            }
            win.set_visible(false);
            if auto_paste {
                glib::MainContext::default().spawn_local(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    let cascade = InjectorCascade::new();
                    let (name, success) = cascade.execute_paste().await;
                    info!(
                        "Transformed paste executed: injector={}, success={}",
                        name, success
                    );
                });
            }
        }
    };

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
    copy_btn.connect_clicked(move |_| {
        if let Some(display) = gdk4::Display::default() {
            display.clipboard().set_text(&text_for_copy);
        }
        win_for_copy.set_visible(false);
    });
    actions_row1.append(&copy_btn);

    // Paste Plain Text (Strips formatting and pastes)
    let plain_btn = Button::with_label("📄 Plain Text");
    plain_btn.add_css_class("popover-action-btn");
    let t_plain = text_content.clone();
    let win_plain = main_window.clone();
    let auto_paste = config.paste.auto_paste;
    plain_btn.connect_clicked(move |_| {
        make_paste_closure(
            TextTransforms::strip_formatting(&t_plain),
            win_plain.clone(),
            auto_paste,
        )();
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
        ocr_btn.connect_clicked(move |_| {
            let hash = hash_cloned.clone();
            let win = win_ocr.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = clipboard_history_core::ipc::IpcClient::connect().await {
                    match client
                        .send(&clipboard_history_core::ipc::IpcRequest::PerformOcr {
                            blob_hash: hash,
                        })
                        .await
                    {
                        Ok(clipboard_history_core::ipc::IpcResponse::OcrResult { text }) => {
                            if let Some(display) = gdk4::Display::default() {
                                display.clipboard().set_text(&text);
                            }
                            win.set_visible(false);
                            info!(
                                "OCR extracted {} characters and copied to clipboard",
                                text.len()
                            );
                        }
                        Ok(clipboard_history_core::ipc::IpcResponse::Error(e)) => {
                            tracing::warn!("OCR request failed: {}", e);
                        }
                        _ => {}
                    }
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
        clean_btn.connect_clicked(move |_| {
            let cleaned = UrlCleaner::clean_text_urls(&t_clean);
            make_paste_closure(cleaned, win_clean.clone(), auto_paste)();
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

        let t = text_content.clone();
        let win = main_window.clone();
        let auto_paste = config.paste.auto_paste;
        upper_btn.connect_clicked(move |_| {
            make_paste_closure(TextTransforms::to_uppercase(&t), win.clone(), auto_paste)();
        });

        let t = text_content.clone();
        let win = main_window.clone();
        lower_btn.connect_clicked(move |_| {
            make_paste_closure(TextTransforms::to_lowercase(&t), win.clone(), auto_paste)();
        });

        let t = text_content.clone();
        let win = main_window.clone();
        title_btn.connect_clicked(move |_| {
            make_paste_closure(TextTransforms::to_title_case(&t), win.clone(), auto_paste)();
        });

        let t = text_content.clone();
        let win = main_window.clone();
        snake_btn.connect_clicked(move |_| {
            make_paste_closure(TextTransforms::to_snake_case(&t), win.clone(), auto_paste)();
        });

        let t = text_content.clone();
        let win = main_window.clone();
        kebab_btn.connect_clicked(move |_| {
            make_paste_closure(TextTransforms::to_kebab_case(&t), win.clone(), auto_paste)();
        });

        let t = text_content.clone();
        let win = main_window.clone();
        camel_btn.connect_clicked(move |_| {
            make_paste_closure(TextTransforms::to_camel_case(&t), win.clone(), auto_paste)();
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
        json_btn.connect_clicked(move |_| {
            if let Ok(pretty) = TextTransforms::json_prettify(&t) {
                make_paste_closure(pretty, win.clone(), auto_paste)();
            }
        });

        let t = text_content.clone();
        let win = main_window.clone();
        b64_enc_btn.connect_clicked(move |_| {
            make_paste_closure(TextTransforms::base64_encode(&t), win.clone(), auto_paste)();
        });

        let t = text_content.clone();
        let win = main_window.clone();
        b64_dec_btn.connect_clicked(move |_| {
            if let Ok(decoded) = TextTransforms::base64_decode(&t) {
                make_paste_closure(decoded, win.clone(), auto_paste)();
            }
        });

        let t = text_content.clone();
        let win = main_window.clone();
        url_enc_btn.connect_clicked(move |_| {
            make_paste_closure(TextTransforms::url_encode(&t), win.clone(), auto_paste)();
        });

        let t = text_content.clone();
        let win = main_window.clone();
        url_dec_btn.connect_clicked(move |_| {
            if let Ok(decoded) = TextTransforms::url_decode(&t) {
                make_paste_closure(decoded, win.clone(), auto_paste)();
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

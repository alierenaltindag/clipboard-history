#![cfg(feature = "gtk")]

use crate::injector::InjectorCascade;
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::transforms::{TextTransforms, UrlCleaner};
use gdk4::prelude::*;
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Label, Orientation, Popover, Window};
use tracing::info;

#[cfg(feature = "gtk")]
pub fn show_transforms_popover(
    parent: &impl IsA<gtk4::Widget>,
    entry: &ClipboardEntry,
    main_window: &Window,
    config: &AppConfig,
) {
    let popover = Popover::new();
    popover.set_parent(parent);
    popover.set_autohide(true);

    let container = GtkBox::new(Orientation::Vertical, 6);
    container.set_margin_start(8);
    container.set_margin_end(8);
    container.set_margin_top(8);
    container.set_margin_bottom(8);

    // Title
    let title = Label::new(Some("⚡ Quick Transforms & Actions"));
    title.add_css_class("heading");
    container.append(&title);

    let text_content = entry
        .text_content
        .clone()
        .unwrap_or_else(|| entry.preview.clone());

    // 1. Color Swatch Section if applicable
    if let Some(color) = TextTransforms::detect_color(&text_content) {
        let color_box = GtkBox::new(Orientation::Horizontal, 8);
        color_box.set_margin_top(4);
        color_box.set_margin_bottom(4);

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
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.3);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        });
        color_box.append(&swatch);

        let label = Label::new(Some(&format!(
            "{} (RGB: {}, {}, {})",
            color.hex, color.r, color.g, color.b
        )));
        label.add_css_class("monospace");
        color_box.append(&label);
        container.append(&color_box);

        // Color Formats Bar
        let fmt_box = GtkBox::new(Orientation::Horizontal, 4);
        let hex_btn = Button::with_label("Hex");
        let rgb_btn = Button::with_label("RGB");
        let hsl_btn = Button::with_label("HSL");
        let css_btn = Button::with_label("CSS var");
        let glsl_btn = Button::with_label("GLSL");

        hex_btn.add_css_class("flat");
        rgb_btn.add_css_class("flat");
        hsl_btn.add_css_class("flat");
        css_btn.add_css_class("flat");
        glsl_btn.add_css_class("flat");

        fmt_box.append(&hex_btn);
        fmt_box.append(&rgb_btn);
        fmt_box.append(&hsl_btn);
        fmt_box.append(&css_btn);
        fmt_box.append(&glsl_btn);
        container.append(&fmt_box);

        let col_clone = color.clone();
        let win = main_window.clone();
        let _ap = config.paste.auto_paste;
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

    // OCR for Image entries
    if let Some(blob_hash) = &entry.blob_hash {
        let ocr_box = GtkBox::new(Orientation::Horizontal, 6);
        ocr_box.set_margin_top(4);
        let ocr_btn = Button::with_label("🔍 Extract Text (OCR)");
        ocr_btn.add_css_class("suggested-action");
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

    // 2. Case Conversion Row
    let case_label = Label::new(Some("Case Conversion:"));
    case_label.set_halign(Align::Start);
    case_label.add_css_class("caption");
    case_label.add_css_class("dim-label");
    container.append(&case_label);

    let case_box = GtkBox::new(Orientation::Horizontal, 4);

    let upper_btn = Button::with_label("UPPER");
    let lower_btn = Button::with_label("lower");
    let title_btn = Button::with_label("Title");
    let snake_btn = Button::with_label("snake_case");
    let kebab_btn = Button::with_label("kebab-case");

    upper_btn.add_css_class("flat");
    lower_btn.add_css_class("flat");
    title_btn.add_css_class("flat");
    snake_btn.add_css_class("flat");
    kebab_btn.add_css_class("flat");

    case_box.append(&upper_btn);
    case_box.append(&lower_btn);
    case_box.append(&title_btn);
    case_box.append(&snake_btn);
    case_box.append(&kebab_btn);
    container.append(&case_box);

    // 3. Developer Tools Row
    let dev_label = Label::new(Some("Developer Tools:"));
    dev_label.set_halign(Align::Start);
    dev_label.add_css_class("caption");
    dev_label.add_css_class("dim-label");
    container.append(&dev_label);

    let dev_box = GtkBox::new(Orientation::Horizontal, 4);
    let json_btn = Button::with_label("JSON Prettify");
    let b64_enc_btn = Button::with_label("Base64 Enc");
    let b64_dec_btn = Button::with_label("Base64 Dec");
    let url_enc_btn = Button::with_label("URL Enc");
    let url_dec_btn = Button::with_label("URL Dec");

    json_btn.add_css_class("flat");
    b64_enc_btn.add_css_class("flat");
    b64_dec_btn.add_css_class("flat");
    url_enc_btn.add_css_class("flat");
    url_dec_btn.add_css_class("flat");

    dev_box.append(&json_btn);
    dev_box.append(&b64_enc_btn);
    dev_box.append(&b64_dec_btn);
    dev_box.append(&url_enc_btn);
    dev_box.append(&url_dec_btn);
    container.append(&dev_box);

    // 4. QR Code & Plain Text Buttons
    let qr_box = GtkBox::new(Orientation::Horizontal, 6);
    qr_box.set_margin_top(6);

    let qr_btn = Button::with_label("📱 View QR Code");
    qr_btn.add_css_class("suggested-action");
    qr_box.append(&qr_btn);

    let plain_btn = Button::with_label("📄 Paste as Plain Text");
    plain_btn.add_css_class("flat");
    qr_box.append(&plain_btn);

    let queue_btn = Button::with_label("🔄 Add to Queue");
    queue_btn.add_css_class("flat");
    qr_box.append(&queue_btn);

    container.append(&qr_box);

    let id_for_queue = entry.id.clone();
    queue_btn.connect_clicked(move |_| {
        let id = id_for_queue.clone();
        glib::MainContext::default().spawn_local(async move {
            if let Ok(client) = clipboard_history_core::ipc::IpcClient::connect().await {
                let _ = client
                    .send(&clipboard_history_core::ipc::IpcRequest::EnqueueItems { ids: vec![id] })
                    .await;
                info!("Enqueued item into paste queue");
            }
        });
    });

    // 5. URL De-tracker Action if text is a URL or has tracking parameters
    let mut clean_btn_opt = None;
    if UrlCleaner::is_url(&text_content) || UrlCleaner::has_tracking_params(&text_content) {
        let clean_box = GtkBox::new(Orientation::Horizontal, 6);
        clean_box.set_margin_top(4);
        let clean_btn = Button::with_label("🛡️ Clean Tracking Params (De-Track)");
        clean_btn.add_css_class("suggested-action");
        clean_box.append(&clean_btn);
        container.append(&clean_box);
        clean_btn_opt = Some(clean_btn);
    }

    popover.set_child(Some(&container));

    // Connect handlers
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

    if let Some(clean_btn) = clean_btn_opt {
        let t = text_content.clone();
        let win = main_window.clone();
        let auto_paste = config.paste.auto_paste;
        clean_btn.connect_clicked(move |_| {
            let cleaned = UrlCleaner::clean_text_urls(&t);
            make_paste_closure(cleaned, win.clone(), auto_paste)();
        });
    }

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

    let t = text_content.clone();
    let win = main_window.clone();
    plain_btn.connect_clicked(move |_| {
        make_paste_closure(
            TextTransforms::strip_formatting(&t),
            win.clone(),
            auto_paste,
        )();
    });

    let t = text_content.clone();
    let win = main_window.clone();
    qr_btn.connect_clicked(move |_| {
        show_qr_dialog(&win, &t);
    });

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

    let title = Label::new(Some("Scan with Mobile Phone"));
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
                let err_label = Label::new(Some("Failed to render QR SVG paintable"));
                root.append(&err_label);
            }
        }
        Err(e) => {
            let err_label = Label::new(Some(&format!("Could not generate QR: {}", e)));
            root.append(&err_label);
        }
    }

    let close_btn = Button::with_label("Close");
    let d = dialog.clone();
    close_btn.connect_clicked(move |_| {
        d.close();
    });
    root.append(&close_btn);

    dialog.set_child(Some(&root));
    dialog.present();
}

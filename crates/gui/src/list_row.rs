#![cfg(feature = "gtk")]

use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use clipboard_history_core::transforms::TextTransforms;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, DrawingArea, Image, Label, ListBoxRow, Orientation, Picture};

#[cfg(feature = "gtk")]
pub struct EntryRow {
    pub row: ListBoxRow,
    #[allow(dead_code)]
    pub entry: ClipboardEntry,
    pub pin_btn: Button,
    pub delete_btn: Button,
    pub actions_btn: Button,
}

#[cfg(feature = "gtk")]
impl EntryRow {
    pub fn new(entry: &ClipboardEntry, index: usize) -> Self {
        let row = ListBoxRow::new();
        row.add_css_class("history-card");

        let container = GtkBox::new(Orientation::Vertical, 6);

        // Header row: index badge, type icon, type pill, color chip, source app, relative time, size, action buttons
        let header = GtkBox::new(Orientation::Horizontal, 6);
        header.set_valign(gtk4::Align::Center);

        // 1. Quick paste index keycap indicator (1..9)
        if index < 9 {
            let idx_badge = Label::new(Some(&format!("{}", index + 1)));
            idx_badge.add_css_class("keycap");
            idx_badge.set_tooltip_text(Some(&format!("Press {} to quick-paste", index + 1)));
            header.append(&idx_badge);
        }

        // 2. Type icon
        let type_icon_name = match entry.entry_type {
            EntryType::Text => "format-text-bold-symbolic",
            EntryType::Html => "text-html-symbolic",
            EntryType::Image => "image-x-generic-symbolic",
            EntryType::UriList => "folder-symbolic",
            EntryType::Code => "text-x-script-symbolic",
        };
        let type_icon = Image::from_icon_name(type_icon_name);
        type_icon.set_pixel_size(14);
        header.append(&type_icon);

        // 3. Type badge pill
        let is_snippet = entry.source_app.as_deref() == Some("Snippet");
        let (badge_text, badge_class) = if is_snippet {
            ("SNIPPET", "badge-snippet")
        } else {
            match entry.entry_type {
                EntryType::Text => ("TEXT", "badge-text"),
                EntryType::Html => ("HTML", "badge-text"),
                EntryType::Image => ("IMAGE", "badge-image"),
                EntryType::UriList => ("FILES", "badge-files"),
                EntryType::Code => ("CODE", "badge-code"),
            }
        };
        let type_badge = Label::new(Some(badge_text));
        type_badge.add_css_class("badge-type");
        type_badge.add_css_class(badge_class);
        header.append(&type_badge);

        // 4. Color swatch chip using zero-leak Cairo drawing
        if let Some(color) = TextTransforms::detect_color(&entry.preview) {
            let swatch = DrawingArea::new();
            swatch.set_content_width(22);
            swatch.set_content_height(16);
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
            swatch.set_tooltip_text(Some(&format!("Color: {}", color.hex)));
            header.append(&swatch);
        }

        // 5. Source application tag (if available and not Snippet)
        if let Some(app) = &entry.source_app {
            if app != "Snippet" && !app.trim().is_empty() {
                let app_label = Label::new(Some(app));
                app_label.add_css_class("app-tag");
                header.append(&app_label);
            }
        }

        // 6. Metadata: relative timestamp & character/size count
        let time_str = format_relative_time(entry.created_at);
        let meta_str = format_entry_meta(entry);
        let time_label = Label::new(Some(&format!("{} • {}", time_str, meta_str)));
        time_label.add_css_class("meta-tag");
        header.append(&time_label);

        // Spacer to push action buttons to the right
        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);

        // 7. Actions button (Ctrl+T / Quick Transforms)
        let actions_btn = Button::from_icon_name("view-more-symbolic");
        actions_btn.add_css_class("flat");
        actions_btn.add_css_class("card-action-btn");
        actions_btn.set_tooltip_text(Some("Quick Transforms & Actions (Ctrl+T)"));
        header.append(&actions_btn);

        // 8. Pin button
        let pin_btn = Button::from_icon_name(if entry.is_pinned {
            "view-pin-symbolic"
        } else {
            "view-pin-outline-symbolic"
        });
        pin_btn.add_css_class("flat");
        pin_btn.add_css_class("card-action-btn");
        if entry.is_pinned {
            pin_btn.add_css_class("pin-active");
            pin_btn.set_tooltip_text(Some("Unpin entry"));
        } else {
            pin_btn.set_tooltip_text(Some("Pin entry to keep permanently (P)"));
        }
        header.append(&pin_btn);

        // 9. Delete button
        let delete_btn = Button::from_icon_name("user-trash-symbolic");
        delete_btn.add_css_class("flat");
        delete_btn.add_css_class("card-action-btn");
        delete_btn.set_tooltip_text(Some("Delete entry (Del)"));
        header.append(&delete_btn);

        container.append(&header);

        // Content preview
        match entry.entry_type {
            EntryType::Image => {
                // If thumbnail blob exists on disk, display image preview thumbnail
                let blobs_dir = AppConfig::blobs_dir();
                let thumb_path = entry
                    .thumbnail_blob_hash
                    .as_ref()
                    .map(|h| blobs_dir.join(h))
                    .or_else(|| entry.blob_hash.as_ref().map(|h| blobs_dir.join(h)));

                let mut preview_added = false;
                if let Some(p) = thumb_path {
                    if p.exists() {
                        let picture = Picture::for_filename(&p);
                        picture.set_content_fit(gtk4::ContentFit::ScaleDown);
                        picture.set_can_shrink(true);
                        picture.set_size_request(-1, 80);
                        picture.add_css_class("thumbnail-preview");
                        container.append(&picture);
                        preview_added = true;
                    }
                }

                if !preview_added {
                    let preview_label = Label::new(Some(&entry.preview));
                    preview_label.set_xalign(0.0);
                    preview_label.add_css_class("meta-tag");
                    container.append(&preview_label);
                }
            }
            EntryType::Code => {
                let code_box = GtkBox::new(Orientation::Vertical, 0);
                code_box.add_css_class("code-preview-box");

                let preview_label = Label::new(Some(&entry.preview));
                preview_label.set_wrap(true);
                preview_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
                preview_label.set_xalign(0.0);
                preview_label.set_lines(3);
                preview_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                preview_label.add_css_class("monospace");

                code_box.append(&preview_label);
                container.append(&code_box);
            }
            _ => {
                let preview_label = Label::new(Some(&entry.preview));
                preview_label.set_wrap(true);
                preview_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
                preview_label.set_xalign(0.0);
                preview_label.set_lines(3);
                preview_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                container.append(&preview_label);
            }
        }

        row.set_child(Some(&container));

        Self {
            row,
            entry: entry.clone(),
            pin_btn,
            delete_btn,
            actions_btn,
        }
    }
}

/// Helper to format relative time strings cleanly
fn format_relative_time(created_at: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(created_at);
    let secs = diff.num_seconds();
    if secs < 45 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", (secs / 60).max(1))
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 172800 {
        "yesterday".to_string()
    } else {
        created_at.format("%b %d").to_string()
    }
}

/// Helper to format metadata (character count, line count, or file size)
fn format_entry_meta(entry: &ClipboardEntry) -> String {
    if entry.entry_type == EntryType::Image {
        format!("{:.1} KB", entry.size_bytes as f64 / 1024.0)
    } else if let Some(text) = &entry.text_content {
        let lines = text.lines().count();
        let chars = text.chars().count();
        if lines > 1 {
            format!("{} chars • {} lines", chars, lines)
        } else {
            format!("{} chars", chars)
        }
    } else {
        format!("{} B", entry.size_bytes)
    }
}

#![cfg(feature = "gtk")]

use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Label, ListBoxRow, Orientation, Picture};

#[cfg(feature = "gtk")]
pub struct EntryRow {
    pub row: ListBoxRow,
    #[allow(dead_code)]
    pub entry: ClipboardEntry,
    pub actions_btn: Button,
}

#[cfg(feature = "gtk")]
impl EntryRow {
    pub fn new(entry: &ClipboardEntry, _index: usize) -> Self {
        let row = ListBoxRow::new();
        row.add_css_class("history-card");

        // Main Horizontal Card Layout: Content on left, Actions on far right
        let card_box = GtkBox::new(Orientation::Horizontal, 6);
        card_box.set_valign(gtk4::Align::Center);

        // 1. Left Content Box (Badges + Text/Image)
        let content_box = GtkBox::new(Orientation::Vertical, 2);
        content_box.set_hexpand(true);
        content_box.set_valign(gtk4::Align::Center);

        // Status badges (Pinned)
        if entry.is_pinned {
            let badge_box = GtkBox::new(Orientation::Horizontal, 4);
            badge_box.set_valign(gtk4::Align::Center);
            badge_box.set_margin_bottom(1);

            let pin_tag = Label::new(Some("📌 Pinned"));
            pin_tag.add_css_class("pinned-tag");
            badge_box.append(&pin_tag);

            content_box.append(&badge_box);
        }

        // Preview content with smart truncation
        let raw_text = entry.text_content.as_deref().unwrap_or(&entry.preview);
        let truncated_text = truncate_preview_text(raw_text, 85);

        match entry.entry_type {
            EntryType::Image => {
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

                let thumb_path = entry
                    .thumbnail_blob_hash
                    .as_deref()
                    .and_then(find_blob)
                    .or_else(|| entry.blob_hash.as_deref().and_then(find_blob));

                let mut preview_added = false;
                if let Some(p) = thumb_path {
                    let picture = Picture::for_filename(&p);
                    picture.set_content_fit(gtk4::ContentFit::ScaleDown);
                    picture.set_can_shrink(true);
                    picture.set_size_request(-1, 52);
                    picture.add_css_class("thumbnail-preview");
                    content_box.append(&picture);
                    preview_added = true;
                }
                if !preview_added {
                    let preview_label = Label::new(Some(&truncated_text));
                    preview_label.set_xalign(0.0);
                    preview_label.set_lines(2);
                    preview_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                    preview_label.add_css_class("card-content-text");
                    content_box.append(&preview_label);
                }
            }
            EntryType::Code => {
                let preview_label = Label::new(Some(&truncated_text));
                preview_label.set_wrap(true);
                preview_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
                preview_label.set_xalign(0.0);
                preview_label.set_lines(2);
                preview_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                preview_label.add_css_class("card-content-text");
                preview_label.add_css_class("monospace");
                content_box.append(&preview_label);
            }
            _ => {
                let preview_label = Label::new(Some(&truncated_text));
                preview_label.set_wrap(true);
                preview_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
                preview_label.set_xalign(0.0);
                preview_label.set_lines(2);
                preview_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                preview_label.add_css_class("card-content-text");
                content_box.append(&preview_label);
            }
        }

        card_box.append(&content_box);

        // 2. Right Action Button (··· Three dots menu)
        let actions_col = GtkBox::new(Orientation::Vertical, 0);
        actions_col.set_valign(gtk4::Align::Center);
        actions_col.add_css_class("card-vertical-actions");

        // Actions button (···)
        let actions_btn = Button::from_icon_name("view-more-symbolic");
        actions_btn.add_css_class("flat");
        actions_btn.add_css_class("card-action-btn");
        actions_btn.set_tooltip_text(Some("Actions (···)"));
        actions_col.append(&actions_btn);

        card_box.append(&actions_col);

        let container = card_box;
        // 3. Hover Tooltip with Metadata (Character/Line count or Dimensions/Size)
        let tooltip_text = match entry.entry_type {
            EntryType::Image => {
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
                let img_path = entry
                    .blob_hash
                    .as_deref()
                    .and_then(find_blob)
                    .or_else(|| entry.thumbnail_blob_hash.as_deref().and_then(find_blob));
                let dims = img_path.and_then(|p| image::image_dimensions(&p).ok());
                let size_kb = (entry.size_bytes as f64) / 1024.0;
                if let Some((w, h)) = dims {
                    format!("Image • {} × {} px • {:.1} KB", w, h, size_kb)
                } else {
                    format!("Image • {:.1} KB", size_kb)
                }
            }
            EntryType::Code => {
                let text = entry.text_content.as_deref().unwrap_or(&entry.preview);
                let chars = text.chars().count();
                let lines = text.lines().count();
                format!("Code Snippet • {} characters • {} lines", chars, lines)
            }
            _ => {
                let text = entry.text_content.as_deref().unwrap_or(&entry.preview);
                let chars = text.chars().count();
                let lines = text.lines().count();
                format!("Text • {} characters • {} lines", chars, lines)
            }
        };
        row.set_tooltip_text(Some(&tooltip_text));

        row.set_child(Some(&container));

        Self {
            row,
            entry: entry.clone(),
            actions_btn,
        }
    }
}

/// Helper to clean and format source application names
pub fn clean_source_app_name(source_app: Option<&str>) -> String {
    if let Some(app) = source_app {
        let normalized = app.replace(['_', '-'], " ");
        let parts: Vec<&str> = normalized.split_whitespace().collect();
        let name = if parts.len() == 2 && parts[0].eq_ignore_ascii_case(parts[1]) {
            parts[0].to_string()
        } else if !parts.is_empty() {
            parts[parts.len() - 1].to_string()
        } else {
            app.to_string()
        };
        let mut chars = name.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    } else {
        "Unknown".to_string()
    }
}

/// Helper to format relative time strings cleanly in English
pub fn format_relative_time(created_at: chrono::DateTime<chrono::Utc>) -> String {
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
        created_at.format("%d %b").to_string()
    }
}

/// Helper to truncate long preview text and normalize linebreaks
pub fn truncate_preview_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let single_line = trimmed.replace("\r\n", " ").replace(['\n', '\t'], " ");
    let char_count = single_line.chars().count();
    if char_count > max_chars {
        let truncated: String = single_line.chars().take(max_chars).collect();
        format!("{}...", truncated.trim_end())
    } else {
        single_line
    }
}

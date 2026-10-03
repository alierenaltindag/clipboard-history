#![cfg(feature = "gtk")]

use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use clipboard_history_core::transforms::TextTransforms;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Image, Label, ListBoxRow, Orientation};

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

        let container = GtkBox::new(Orientation::Vertical, 4);

        // Header row: index badge, type icon, color chip, source / timestamp, pin, delete, action buttons
        let header = GtkBox::new(Orientation::Horizontal, 6);

        // Quick paste index indicator (1..9)
        if index < 9 {
            let idx_badge = Label::new(Some(&format!("{}.", index + 1)));
            idx_badge.add_css_class("dim-label");
            header.append(&idx_badge);
        }

        let type_icon_name = match entry.entry_type {
            EntryType::Text => "format-text-bold-symbolic",
            EntryType::Html => "text-html-symbolic",
            EntryType::Image => "image-x-generic-symbolic",
            EntryType::UriList => "folder-symbolic",
            EntryType::Code => "text-x-script-symbolic",
        };
        let type_icon = Image::from_icon_name(type_icon_name);
        header.append(&type_icon);

        // Type badge label
        let type_label = Label::new(Some(&entry.entry_type.to_string().to_uppercase()));
        type_label.add_css_class("caption");
        type_label.add_css_class("dim-label");
        header.append(&type_label);

        // Color swatch chip if color detected in text
        if let Some(color) = TextTransforms::detect_color(&entry.preview) {
            let color_chip = Label::new(Some("   "));
            let clean_hex = color.hex.trim_start_matches('#').to_lowercase();
            let swatch_class = format!("swatch-{clean_hex}");
            color_chip.add_css_class(&swatch_class);
            let provider = gtk4::CssProvider::new();
            provider.load_from_string(&format!(
                ".{swatch_class} {{ background-color: {}; border-radius: 4px; border: 1px solid rgba(0,0,0,0.3); }}",
                color.hex
            ));
            if let Some(display) = gtk4::gdk::Display::default() {
                gtk4::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
            color_chip.set_tooltip_text(Some(&format!("Color: {}", color.hex)));
            header.append(&color_chip);
        }

        // Spacer
        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);

        // Actions button (Ctrl+T / Transforms)
        let actions_btn = Button::from_icon_name("view-more-symbolic");
        actions_btn.add_css_class("flat");
        actions_btn.set_tooltip_text(Some("Quick Transforms & Actions (Ctrl+T)"));
        header.append(&actions_btn);

        // Pin button
        let pin_btn = Button::from_icon_name(if entry.is_pinned {
            "view-pin-symbolic"
        } else {
            "view-pin-outline-symbolic"
        });
        pin_btn.add_css_class("flat");
        pin_btn.set_tooltip_text(Some(if entry.is_pinned { "Unpin" } else { "Pin (P)" }));
        header.append(&pin_btn);

        // Delete button
        let delete_btn = Button::from_icon_name("user-trash-symbolic");
        delete_btn.add_css_class("flat");
        delete_btn.set_tooltip_text(Some("Delete (Del)"));
        header.append(&delete_btn);

        container.append(&header);

        // Content preview
        let preview_label = Label::new(Some(&entry.preview));
        preview_label.set_wrap(true);
        preview_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        preview_label.set_xalign(0.0);
        preview_label.set_max_width_chars(50);
        preview_label.set_lines(3);
        preview_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

        if entry.entry_type == EntryType::Code {
            preview_label.add_css_class("monospace");
        }

        container.append(&preview_label);

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

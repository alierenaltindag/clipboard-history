#[cfg(feature = "gtk")]
use clipboard_history_core::domain::{ClipboardEntry, EntryType};
#[cfg(feature = "gtk")]
use gtk4::prelude::*;
#[cfg(feature = "gtk")]
use gtk4::{Box as GtkBox, Button, Image, Label, ListBoxRow, Orientation};

#[cfg(feature = "gtk")]
pub struct EntryRow {
    pub row: ListBoxRow,
    pub entry_id: String,
}

#[cfg(feature = "gtk")]
impl EntryRow {
    pub fn new(entry: &ClipboardEntry, index: usize) -> Self {
        let row = ListBoxRow::new();
        row.add_css_class("history-card");

        let container = GtkBox::new(Orientation::Vertical, 4);

        // Header row: index badge, type icon, source / timestamp, pin & delete buttons
        let header = GtkBox::new(Orientation::Horizontal, 8);

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

        // Spacer
        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);

        // Pin button
        let pin_btn = Button::from_icon_name(if entry.is_pinned {
            "view-pin-symbolic"
        } else {
            "view-pin-outline-symbolic"
        });
        pin_btn.add_css_class("flat");
        pin_btn.set_tooltip_text(Some(if entry.is_pinned { "Unpin" } else { "Pin" }));
        header.append(&pin_btn);

        // Delete button
        let delete_btn = Button::from_icon_name("user-trash-symbolic");
        delete_btn.add_css_class("flat");
        delete_btn.set_tooltip_text(Some("Delete"));
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
            entry_id: entry.id.clone(),
        }
    }
}

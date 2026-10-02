#![cfg(feature = "gtk")]

use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Entry, Label, Orientation, TextView, Window};
use tracing::info;

pub struct SnippetDialog;

impl SnippetDialog {
    pub fn show(parent: &Window, on_created: impl Fn() + 'static) {
        let dialog = Window::builder()
            .title("New Canned Snippet")
            .transient_for(parent)
            .modal(true)
            .default_width(380)
            .default_height(420)
            .build();

        let root = GtkBox::new(Orientation::Vertical, 10);
        root.set_margin_start(16);
        root.set_margin_end(16);
        root.set_margin_top(16);
        root.set_margin_bottom(16);

        let title = Label::new(Some("📝 Add Permanent Snippet"));
        title.add_css_class("heading");
        root.append(&title);

        let desc = Label::new(Some(
            "Supports placeholders: {date}, {time}, {uuid}, {clipboard}",
        ));
        desc.add_css_class("caption");
        desc.add_css_class("dim-label");
        root.append(&desc);

        // Label input
        let label_header = Label::new(Some("Snippet Label / Title:"));
        label_header.set_halign(Align::Start);
        root.append(&label_header);

        let label_entry = Entry::new();
        label_entry.set_placeholder_text(Some("e.g., Work Email Signature, Docker Cleanup"));
        root.append(&label_entry);

        // Category input
        let cat_header = Label::new(Some("Category:"));
        cat_header.set_halign(Align::Start);
        root.append(&cat_header);

        let cat_entry = Entry::new();
        cat_entry.set_text("General");
        root.append(&cat_entry);

        // Content input
        let content_header = Label::new(Some("Content / Template:"));
        content_header.set_halign(Align::Start);
        root.append(&content_header);

        let text_view = TextView::new();
        text_view.set_vexpand(true);
        text_view.set_wrap_mode(gtk4::WrapMode::Word);
        text_view.add_css_class("card");
        root.append(&text_view);

        // Buttons
        let btn_box = GtkBox::new(Orientation::Horizontal, 8);
        btn_box.set_halign(Align::End);

        let cancel_btn = Button::with_label("Cancel");
        let save_btn = Button::with_label("Save Snippet");
        save_btn.add_css_class("suggested-action");

        btn_box.append(&cancel_btn);
        btn_box.append(&save_btn);
        root.append(&btn_box);

        let d_cancel = dialog.clone();
        cancel_btn.connect_clicked(move |_| {
            d_cancel.close();
        });

        let d_save = dialog.clone();
        save_btn.connect_clicked(move |_| {
            let label_val = label_entry.text().to_string();
            let cat_val = cat_entry.text().to_string();
            let buffer = text_view.buffer();
            let start = buffer.start_iter();
            let end = buffer.end_iter();
            let content_val = buffer.text(&start, &end, false).to_string();

            if label_val.trim().is_empty() || content_val.trim().is_empty() {
                return;
            }

            let d = d_save.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(mut client) = IpcClient::connect().await {
                    let req = IpcRequest::CreateSnippet {
                        label: label_val,
                        content: content_val,
                        category: Some(cat_val),
                    };
                    if let Ok(IpcResponse::Snippet(_)) = client.send(&req).await {
                        info!("Snippet created successfully");
                        d.close();
                    }
                }
            });
            on_created();
        });

        dialog.set_child(Some(&root));
        dialog.present();
    }
}

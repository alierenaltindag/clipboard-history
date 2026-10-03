#![cfg(feature = "gtk")]

use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use gtk4::{Box as GtkBox, Button, HeaderBar, Orientation, TextView, Window};
use libadwaita::prelude::*;
use tracing::info;

pub struct SnippetDialog;

impl SnippetDialog {
    pub fn show(parent: &Window, on_created: impl Fn() + 'static) {
        let dialog = Window::builder()
            .title("New Canned Snippet")
            .transient_for(parent)
            .modal(true)
            .default_width(440)
            .default_height(480)
            .build();

        let header = HeaderBar::new();
        header.set_show_title_buttons(true);
        let window_title = libadwaita::WindowTitle::new("New Canned Snippet", "Reusable Template");
        header.set_title_widget(Some(&window_title));

        let cancel_btn = Button::with_label("Cancel");
        cancel_btn.add_css_class("flat");
        header.pack_start(&cancel_btn);

        let save_btn = Button::with_label("Save");
        save_btn.add_css_class("suggested-action");
        header.pack_end(&save_btn);

        dialog.set_titlebar(Some(&header));

        let root = GtkBox::new(Orientation::Vertical, 12);
        root.set_margin_start(16);
        root.set_margin_end(16);
        root.set_margin_top(16);
        root.set_margin_bottom(16);

        // Group 1: Metadata
        let meta_group = libadwaita::PreferencesGroup::new();
        meta_group.set_title("Snippet Details");

        let label_row = libadwaita::EntryRow::new();
        label_row.set_title("Label / Title");
        meta_group.add(&label_row);

        let cat_row = libadwaita::EntryRow::new();
        cat_row.set_title("Category");
        cat_row.set_text("General");
        meta_group.add(&cat_row);

        root.append(&meta_group);

        // Group 2: Content Template
        let content_group = libadwaita::PreferencesGroup::new();
        content_group.set_title("Snippet Content");
        content_group.set_description(Some(
            "Supports dynamic placeholders: {date}, {time}, {uuid}, {clipboard}",
        ));

        let text_scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .vexpand(true)
            .min_content_height(140)
            .build();
        text_scrolled.add_css_class("card");

        let text_view = TextView::new();
        text_view.set_wrap_mode(gtk4::WrapMode::Word);
        text_view.set_left_margin(10);
        text_view.set_right_margin(10);
        text_view.set_top_margin(8);
        text_view.set_bottom_margin(8);
        text_scrolled.set_child(Some(&text_view));
        content_group.add(&text_scrolled);

        root.append(&content_group);

        let d_cancel = dialog.clone();
        cancel_btn.connect_clicked(move |_| {
            d_cancel.close();
        });

        let on_created = std::rc::Rc::new(on_created);
        let d_save = dialog.clone();
        let label_ref = label_row.clone();
        let cat_ref = cat_row.clone();
        save_btn.connect_clicked(move |_| {
            let label_val = label_ref.text().to_string();
            let cat_val = cat_ref.text().to_string();
            let buffer = text_view.buffer();
            let start = buffer.start_iter();
            let end = buffer.end_iter();
            let content_val = buffer.text(&start, &end, false).to_string();

            if label_val.trim().is_empty() || content_val.trim().is_empty() {
                return;
            }

            let d = d_save.clone();
            let on_done = std::rc::Rc::clone(&on_created);
            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = IpcClient::connect().await {
                    let req = IpcRequest::CreateSnippet {
                        label: label_val,
                        content: content_val,
                        category: Some(cat_val),
                    };
                    if let Ok(IpcResponse::Snippet(_)) = client.send(&req).await {
                        info!("Snippet created successfully");
                        d.close();
                        on_done();
                    }
                }
            });
        });

        dialog.set_child(Some(&root));
        dialog.present();
    }
}

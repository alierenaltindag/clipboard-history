#![cfg(feature = "gtk")]

use clipboard_history_core::transforms::{DiffEngine, DiffResult, DiffTag};
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, HeaderBar, Label, Orientation, ScrolledWindow, TextView, Window,
};
use std::cell::RefCell;
use std::rc::Rc;

pub struct DiffDialog;

impl DiffDialog {
    pub fn show(
        parent: &Window,
        initial_text_a: String,
        initial_text_b: String,
        label_a: String,
        label_b: String,
    ) {
        let dialog = Window::builder()
            .title("Clipboard Diff Viewer")
            .transient_for(parent)
            .modal(true)
            .default_width(680)
            .default_height(560)
            .build();

        let header = HeaderBar::new();
        header.set_show_title_buttons(true);

        let window_title =
            libadwaita::WindowTitle::new("Clipboard Diff Comparison", "Comparing 2 items");
        header.set_title_widget(Some(&window_title));

        let swap_btn = Button::with_label("⇄ Swap A / B");
        swap_btn.add_css_class("flat");
        header.pack_end(&swap_btn);

        let copy_btn = Button::with_label("📋 Copy Diff");
        copy_btn.add_css_class("suggested-action");
        header.pack_end(&copy_btn);

        dialog.set_titlebar(Some(&header));

        let root = GtkBox::new(Orientation::Vertical, 10);
        root.set_margin_start(16);
        root.set_margin_end(16);
        root.set_margin_top(12);
        root.set_margin_bottom(12);

        // Legend / Context Info
        let legend_box = GtkBox::new(Orientation::Horizontal, 12);
        let lbl_orig = Label::new(Some(&format!("🔴 Original (A): {}", label_a)));
        lbl_orig.add_css_class("badge-type");
        lbl_orig.add_css_class("badge-files");
        lbl_orig.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lbl_orig.set_hexpand(true);
        lbl_orig.set_xalign(0.0);

        let lbl_mod = Label::new(Some(&format!("🟢 Modified (B): {}", label_b)));
        lbl_mod.add_css_class("badge-type");
        lbl_mod.add_css_class("badge-image");
        lbl_mod.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lbl_mod.set_hexpand(true);
        lbl_mod.set_xalign(0.0);

        legend_box.append(&lbl_orig);
        legend_box.append(&lbl_mod);
        root.append(&legend_box);

        // Text view with monospace and syntax highlighting tags
        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .build();
        scrolled.add_css_class("card");

        let text_view = TextView::new();
        text_view.set_editable(false);
        text_view.set_cursor_visible(false);
        text_view.set_monospace(true);
        text_view.set_left_margin(12);
        text_view.set_right_margin(12);
        text_view.set_top_margin(8);
        text_view.set_bottom_margin(8);
        scrolled.set_child(Some(&text_view));
        root.append(&scrolled);

        // State holder
        let state = Rc::new(RefCell::new((
            initial_text_a,
            initial_text_b,
            DiffResult {
                unified: String::new(),
                additions: 0,
                deletions: 0,
                lines: Vec::new(),
            },
        )));

        // Setup tags on buffer
        let buffer = text_view.buffer();
        let tag_insert = buffer.create_tag(
            Some("diff_insert"),
            &[
                ("foreground", &"#16a34a"),
                ("background", &"#14532d33"),
                ("weight", &600),
            ],
        );
        let tag_delete = buffer.create_tag(
            Some("diff_delete"),
            &[
                ("foreground", &"#dc2626"),
                ("background", &"#7f1d1d33"),
                ("weight", &600),
            ],
        );
        let tag_equal = buffer.create_tag(Some("diff_equal"), &[("foreground", &"#94a3b8")]);

        // Helper render function
        let buffer_clone = buffer.clone();
        let window_title_clone = window_title.clone();
        let state_clone = state.clone();

        let render_diff = move || {
            let mut st = state_clone.borrow_mut();
            let res = DiffEngine::compute_diff(&st.0, &st.1);
            window_title_clone.set_subtitle(&format!(
                "+{} additions, -{} deletions ({} total lines)",
                res.additions,
                res.deletions,
                res.lines.len()
            ));

            buffer_clone.set_text("");
            let mut iter = buffer_clone.end_iter();

            for line in &res.lines {
                let (prefix, tag_ref) = match line.tag {
                    DiffTag::Insert => ("+ ", tag_insert.as_ref()),
                    DiffTag::Delete => ("- ", tag_delete.as_ref()),
                    DiffTag::Equal => ("  ", tag_equal.as_ref()),
                };

                let line_str = format!("{}{}\n", prefix, line.text);
                let start_offset = iter.offset();
                buffer_clone.insert(&mut iter, &line_str);
                let start_iter = buffer_clone.iter_at_offset(start_offset);
                if let Some(tag) = tag_ref {
                    buffer_clone.apply_tag(tag, &start_iter, &iter);
                }
            }

            st.2 = res;
        };

        render_diff();

        // Swap button
        let render_for_swap = render_diff.clone();
        let state_for_swap = state.clone();
        let lbl_orig_swap = lbl_orig.clone();
        let lbl_mod_swap = lbl_mod.clone();
        let name_a = label_a.clone();
        let name_b = label_b.clone();
        let is_swapped = Rc::new(RefCell::new(false));

        let is_swapped_clone = is_swapped.clone();
        swap_btn.connect_clicked(move |_| {
            let mut st = state_for_swap.borrow_mut();
            let temp = st.0.clone();
            st.0 = st.1.clone();
            st.1 = temp;
            drop(st);

            let mut swapped = is_swapped_clone.borrow_mut();
            *swapped = !*swapped;
            if *swapped {
                lbl_orig_swap.set_text(&format!("🔴 Original (B): {}", name_b));
                lbl_mod_swap.set_text(&format!("🟢 Modified (A): {}", name_a));
            } else {
                lbl_orig_swap.set_text(&format!("🔴 Original (A): {}", name_a));
                lbl_mod_swap.set_text(&format!("🟢 Modified (B): {}", name_b));
            }

            render_for_swap();
        });

        // Copy button
        let state_for_copy = state.clone();
        copy_btn.connect_clicked(move |_| {
            let st = state_for_copy.borrow();
            if let Some(display) = gdk4::Display::default() {
                display.clipboard().set_text(&st.2.unified);
            }
        });

        dialog.set_child(Some(&root));
        dialog.present();
    }
}

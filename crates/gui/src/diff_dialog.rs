#![cfg(feature = "gtk")]

use clipboard_history_core::transforms::{DiffEngine, DiffResult, DiffTag};
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Label, Orientation, ScrolledWindow, TextView, Window};
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
            .default_width(620)
            .default_height(540)
            .build();

        let root = GtkBox::new(Orientation::Vertical, 10);
        root.set_margin_start(16);
        root.set_margin_end(16);
        root.set_margin_top(16);
        root.set_margin_bottom(16);

        // Header section
        let header_box = GtkBox::new(Orientation::Horizontal, 12);
        let title_box = GtkBox::new(Orientation::Vertical, 2);

        let title = Label::new(Some("🔀 Clipboard Diff Comparison"));
        title.add_css_class("heading");
        title.set_halign(Align::Start);
        title_box.append(&title);

        let stats_label = Label::new(None);
        stats_label.add_css_class("caption");
        stats_label.add_css_class("dim-label");
        stats_label.set_halign(Align::Start);
        title_box.append(&stats_label);

        header_box.append(&title_box);

        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header_box.append(&spacer);

        let swap_btn = Button::with_label("⇄ Swap A / B");
        swap_btn.add_css_class("flat");
        header_box.append(&swap_btn);

        let copy_btn = Button::with_label("📋 Copy Diff");
        copy_btn.add_css_class("suggested-action");
        header_box.append(&copy_btn);

        root.append(&header_box);

        // Legend / Context Info
        let legend_box = GtkBox::new(Orientation::Horizontal, 12);
        let lbl_orig = Label::new(Some(&format!("🔴 Original (A): {}", label_a)));
        lbl_orig.add_css_class("caption");
        let lbl_mod = Label::new(Some(&format!("🟢 Modified (B): {}", label_b)));
        lbl_mod.add_css_class("caption");
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

        // Bottom Bar
        let bottom_box = GtkBox::new(Orientation::Horizontal, 8);
        let hint_label = Label::new(Some("Press Esc to close"));
        hint_label.add_css_class("dim-label");
        hint_label.add_css_class("caption");
        hint_label.set_halign(Align::Start);
        bottom_box.append(&hint_label);

        let b_spacer = GtkBox::new(Orientation::Horizontal, 0);
        b_spacer.set_hexpand(true);
        bottom_box.append(&b_spacer);

        let close_btn = Button::with_label("Close");
        let d_close = dialog.clone();
        close_btn.connect_clicked(move |_| {
            d_close.close();
        });
        bottom_box.append(&close_btn);
        root.append(&bottom_box);

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
        let stats_clone = stats_label.clone();
        let state_clone = state.clone();

        let render_diff = move || {
            let mut st = state_clone.borrow_mut();
            let res = DiffEngine::compute_diff(&st.0, &st.1);
            stats_clone.set_text(&format!(
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

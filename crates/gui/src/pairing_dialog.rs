#[cfg(feature = "gtk")]
use clipboard_history_core::ipc::{IpcClient, IpcRequest, PendingPairingInfo};
#[cfg(feature = "gtk")]
use gtk4::prelude::*;
#[cfg(feature = "gtk")]
use gtk4::{Box as GtkBox, Button, Label, Orientation, Window};
#[cfg(feature = "gtk")]
use std::cell::RefCell;
#[cfg(feature = "gtk")]
use std::collections::HashSet;
#[cfg(feature = "gtk")]
use std::rc::Rc;
#[cfg(feature = "gtk")]
use tracing::info;

#[cfg(feature = "gtk")]
pub struct PairingDialog;

#[cfg(feature = "gtk")]
impl PairingDialog {
    pub fn show(
        parent: Option<&Window>,
        req: PendingPairingInfo,
        active_set: Rc<RefCell<HashSet<String>>>,
    ) -> Window {
        let builder = Window::builder()
            .title("Clipboard Pairing Request")
            .modal(true)
            .default_width(460)
            .default_height(260)
            .resizable(false);

        let dialog = if let Some(p) = parent {
            builder.transient_for(p).build()
        } else {
            builder.build()
        };

        let root = GtkBox::new(Orientation::Vertical, 14);
        root.set_margin_start(20);
        root.set_margin_end(20);
        root.set_margin_top(20);
        root.set_margin_bottom(20);

        // Header with icon and title
        let header_box = GtkBox::new(Orientation::Horizontal, 12);
        let icon = gtk4::Image::from_icon_name("network-wireless-symbolic");
        icon.set_pixel_size(36);
        icon.add_css_class("accent");
        header_box.append(&icon);

        let text_vbox = GtkBox::new(Orientation::Vertical, 4);
        let title_label = Label::new(Some(&format!("Pair with '{}'?", req.device_name)));
        title_label.add_css_class("title-3");
        title_label.set_xalign(0.0);
        text_vbox.append(&title_label);

        let subtitle_label =
            Label::new(Some(&format!("Incoming connection from {}", req.peer_address)));
        subtitle_label.add_css_class("caption");
        subtitle_label.add_css_class("dim-label");
        subtitle_label.set_xalign(0.0);
        text_vbox.append(&subtitle_label);

        header_box.append(&text_vbox);
        root.append(&header_box);

        // Explanation text
        let desc = Label::new(Some(
            "This device wants to establish an encrypted LAN connection to synchronize your clipboard history.\n\nOnly accept if you recognize this device on your local network.",
        ));
        desc.set_wrap(true);
        desc.set_xalign(0.0);
        root.append(&desc);

        // Action buttons
        let actions_box = GtkBox::new(Orientation::Horizontal, 12);
        actions_box.set_halign(gtk4::Align::End);
        actions_box.set_margin_top(8);

        let decline_btn = Button::with_label("✕ Decline");
        decline_btn.add_css_class("destructive-action");

        let accept_btn = Button::with_label("✓ Accept & Pair");
        accept_btn.add_css_class("suggested-action");

        actions_box.append(&decline_btn);
        actions_box.append(&accept_btn);
        root.append(&actions_box);

        let r_id = req.request_id.clone();
        let r_id_acc = req.request_id.clone();
        let r_id_dec = req.request_id.clone();
        let set_acc = Rc::clone(&active_set);
        let set_dec = Rc::clone(&active_set);
        let d_acc = dialog.clone();
        let d_dec = dialog.clone();

        accept_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            set_acc.borrow_mut().remove(&r_id_acc);
            let req_id = r_id_acc.clone();
            let d = d_acc.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = IpcClient::connect().await {
                    let _ = client
                        .send(&IpcRequest::RespondPairingRequest {
                            request_id: req_id,
                            accept: true,
                        })
                        .await;
                }
                d.close();
            });
        });

        decline_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            set_dec.borrow_mut().remove(&r_id_dec);
            let req_id = r_id_dec.clone();
            let d = d_dec.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = IpcClient::connect().await {
                    let _ = client
                        .send(&IpcRequest::RespondPairingRequest {
                            request_id: req_id,
                            accept: false,
                        })
                        .await;
                }
                d.close();
            });
        });

        // If window is closed via window manager close button
        let set_close = Rc::clone(&active_set);
        let r_id_close = r_id.clone();
        dialog.connect_close_request(move |_| {
            if set_close.borrow_mut().remove(&r_id_close) {
                let req_id = r_id_close.clone();
                glib::MainContext::default().spawn_local(async move {
                    if let Ok(client) = IpcClient::connect().await {
                        let _ = client
                            .send(&IpcRequest::RespondPairingRequest {
                                request_id: req_id,
                                accept: false,
                            })
                            .await;
                    }
                });
            }
            glib::Propagation::Proceed
        });

        dialog.set_child(Some(&root));
        dialog.present();
        info!(
            "Presented pairing request modal for '{}' ({})",
            req.device_name, req.peer_address
        );
        dialog
    }
}

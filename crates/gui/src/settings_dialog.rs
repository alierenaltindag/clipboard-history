#![cfg(feature = "gtk")]

use clipboard_history_core::config::AppConfig;
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use gdk4::prelude::*;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use tracing::info;

pub struct SettingsDialog;

impl SettingsDialog {
    #[allow(clippy::type_complexity)]
    pub fn show(parent: Option<&gtk4::Window>, config: AppConfig) {
        let dialog = libadwaita::PreferencesWindow::new();
        if let Some(p) = parent {
            dialog.set_application(p.application().as_ref());
            if p.is_visible() {
                dialog.set_transient_for(Some(p));
                dialog.set_modal(true);
            }
        }
        dialog.set_title(Some("Clipboard Preferences"));
        dialog.set_default_width(620);
        dialog.set_default_height(540);

        // ==========================================
        // PAGE 1: General
        // ==========================================
        let general_page = libadwaita::PreferencesPage::new();
        general_page.set_title("General");
        general_page.set_icon_name(Some("preferences-system-symbolic"));

        // Retention & Capacity Group
        let general_group = libadwaita::PreferencesGroup::new();
        general_group.set_title("Retention & Capacity");
        general_group.set_description(Some(
            "Manage how many entries to preserve in local encrypted storage",
        ));

        let max_entries_row = libadwaita::SpinRow::new(
            Some(&gtk4::Adjustment::new(
                config.general.max_entries as f64,
                50.0,
                5000.0,
                50.0,
                100.0,
                0.0,
            )),
            1.0,
            0,
        );
        max_entries_row.set_title("Max History Entries");
        max_entries_row
            .set_subtitle("Older unpinned entries are automatically evicted when limit is reached");
        general_group.add(&max_entries_row);

        let retention_row = libadwaita::SpinRow::new(
            Some(&gtk4::Adjustment::new(
                config.general.retention_days as f64,
                1.0,
                365.0,
                1.0,
                7.0,
                0.0,
            )),
            1.0,
            0,
        );
        retention_row.set_title("Retention Days");
        retention_row.set_subtitle("Automatically delete entries older than this duration");
        general_group.add(&retention_row);

        general_page.add(&general_group);

        // Pasting Group
        let paste_group = libadwaita::PreferencesGroup::new();
        paste_group.set_title("Pasting Behavior");
        paste_group.set_description(Some(
            "Configure how selected entries are injected into active apps",
        ));

        let auto_paste_row = libadwaita::SwitchRow::new();
        auto_paste_row.set_title("Direct Keystroke Auto-Paste");
        auto_paste_row
            .set_subtitle("Automatically synthesize Ctrl+V keystrokes when an item is chosen");
        auto_paste_row.set_active(config.paste.auto_paste);
        paste_group.add(&auto_paste_row);

        general_page.add(&paste_group);
        dialog.add(&general_page);

        // ==========================================
        // PAGE 2: Privacy & Security
        // ==========================================
        let security_page = libadwaita::PreferencesPage::new();
        security_page.set_title("Privacy & Security");
        security_page.set_icon_name(Some("security-high-symbolic"));

        // Protection Rules Group
        let security_group = libadwaita::PreferencesGroup::new();
        security_group.set_title("Protection Rules");
        security_group.set_description(Some(
            "Prevent sensitive passwords and private browsing data from leaking",
        ));

        let pwd_row = libadwaita::SwitchRow::new();
        pwd_row.set_title("Protect Password Managers");
        pwd_row.set_subtitle("Ignore clipboard captures from KeePassXC, 1Password, and Bitwarden");
        pwd_row.set_active(config.security.ignore_password_managers);
        security_group.add(&pwd_row);

        let incognito_row = libadwaita::SwitchRow::new();
        incognito_row.set_title("Ignore Private & Incognito Browsing");
        incognito_row
            .set_subtitle("Do not record copies made inside incognito or private browsing windows");
        incognito_row.set_active(config.security.ignore_incognito_windows);
        security_group.add(&incognito_row);

        let clean_urls_row = libadwaita::SwitchRow::new();
        clean_urls_row.set_title("URL De-Tracker & Privacy Cleaner");
        clean_urls_row.set_subtitle(
            "Strip tracking query parameters (utm_*, fbclid, gclid) from copied links",
        );
        clean_urls_row.set_active(config.security.auto_clean_tracking_urls);
        security_group.add(&clean_urls_row);

        security_page.add(&security_group);

        // Application Filter Group
        let filter_group = libadwaita::PreferencesGroup::new();
        filter_group.set_title("Application Filtering");
        filter_group.set_description(Some(
            "Control which desktop applications are monitored for clipboard events",
        ));

        let mode_row = libadwaita::ComboRow::new();
        let model = gtk4::StringList::new(&[
            "Blacklist (Ignore specified apps)",
            "Whitelist (Only save from specified apps)",
        ]);
        mode_row.set_model(Some(&model));
        mode_row.set_title("Filter Mode");
        mode_row.set_selected(match config.security.app_filter_mode {
            clipboard_history_core::config::AppFilterMode::Blacklist => 0,
            clipboard_history_core::config::AppFilterMode::Whitelist => 1,
        });
        filter_group.add(&mode_row);

        let list_row = libadwaita::EntryRow::new();
        list_row.set_title("Filtered Apps (comma-separated)");
        list_row.set_text(&config.security.app_filter_list.join(", "));
        filter_group.add(&list_row);

        security_page.add(&filter_group);
        dialog.add(&security_page);

        // ==========================================
        // PAGE 3: Network Sync
        // ==========================================
        // PAGE 5: Network Sync
        // ==========================================
        let sync_page = libadwaita::PreferencesPage::new();
        sync_page.set_title("Network Sync");
        sync_page.set_icon_name(Some("network-wireless-symbolic"));

        let sync_group = libadwaita::PreferencesGroup::new();
        sync_group.set_title("P2P Local Network Sync");
        sync_group.set_description(Some(
            "Synchronize clipboard items securely across LAN devices using AES-256-GCM",
        ));

        let sync_toggle_row = libadwaita::SwitchRow::new();
        sync_toggle_row.set_title("Enable LAN Clipboard Sync");
        sync_toggle_row
            .set_subtitle("Automatically discover and sync clipboard with trusted devices on LAN");
        sync_toggle_row.set_active(config.sync.enabled);
        sync_group.add(&sync_toggle_row);

        let dev_name_row = libadwaita::EntryRow::new();
        dev_name_row.set_title("Device Name");
        dev_name_row.set_text(&config.sync.device_name);
        sync_group.add(&dev_name_row);

        let pin_row = libadwaita::EntryRow::new();
        pin_row.set_title("Pairing PIN");
        pin_row.set_text(&config.sync.pairing_pin);
        sync_group.add(&pin_row);

        let local_ip = clipboard_history_core::sync::get_primary_local_ip()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let info_row = libadwaita::ActionRow::new();
        info_row.set_title("Local Network Info for This Device");
        info_row.set_subtitle(&format!(
            "Local IP: {} • Port: {}",
            local_ip, config.sync.listen_port
        ));
        let copy_ip_btn = gtk4::Button::from_icon_name("edit-copy-symbolic");
        copy_ip_btn.set_tooltip_text(Some("Copy IP Address"));
        copy_ip_btn.add_css_class("flat");
        let ip_to_copy = format!("{}:{}", local_ip, config.sync.listen_port);
        let btn_ip_c = copy_ip_btn.clone();
        copy_ip_btn.connect_clicked(move |_| {
            if let Some(disp) = gdk4::Display::default() {
                disp.clipboard().set_text(&ip_to_copy);
            }
            btn_ip_c.set_icon_name("emblem-ok-symbolic");
            let b_reset = btn_ip_c.clone();
            glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
                b_reset.set_icon_name("edit-copy-symbolic");
            });
        });
        info_row.add_suffix(&copy_ip_btn);
        sync_group.add(&info_row);

        sync_page.add(&sync_group);

        // Connected & Discovered Peers Group
        let peers_group = libadwaita::PreferencesGroup::new();
        peers_group.set_title("Discovered Devices & Pairings");
        peers_group.set_description(Some(
            "Devices on your local network. Click the button to send a pairing request. Continuous sync starts once accepted.",
        ));

        let scan_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
        scan_btn.set_tooltip_text(Some("Scan Network"));
        scan_btn.add_css_class("flat");
        peers_group.set_header_suffix(Some(&scan_btn));

        let peers_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        peers_group.add(&peers_box);
        sync_page.add(&peers_group);

        // Manual Peers Group
        let manual_group = libadwaita::PreferencesGroup::new();
        manual_group.set_title("Manual Peer IP / Port");
        manual_group.set_description(Some(
            "Add peer addresses manually for networks where UDP broadcast is restricted (e.g. 192.168.1.50:54123)",
        ));

        let add_peer_row = libadwaita::EntryRow::new();
        add_peer_row.set_title("Add Peer (IP:Port)");
        add_peer_row.set_show_apply_button(true);
        manual_group.add(&add_peer_row);

        let manual_list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        manual_group.add(&manual_list_box);
        sync_page.add(&manual_group);

        // Automated Firewall & Port Check Group
        let firewall_group = libadwaita::PreferencesGroup::new();
        firewall_group.set_title("Firewall & Port Check");
        firewall_group.set_description(Some(
            "Verifies that P2P sync ports (54123 UDP/TCP and 54124 TCP) are accessible on your local network",
        ));

        let fw_refresh_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
        fw_refresh_btn.set_tooltip_text(Some("Recheck Port Status"));
        fw_refresh_btn.add_css_class("flat");
        firewall_group.set_header_suffix(Some(&fw_refresh_btn));

        let fw_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        firewall_group.add(&fw_box);
        sync_page.add(&firewall_group);

        // Network & Security Requirements Guide Group
        let security_guide_group = libadwaita::PreferencesGroup::new();
        security_guide_group.set_title("Network & Security Requirements");
        security_guide_group.set_description(Some(
            "Local network and security settings to check if devices cannot discover each other",
        ));

        let ap_row = libadwaita::ActionRow::new();
        ap_row.set_title("1. Wi-Fi Client Isolation (AP / Client Isolation)");
        ap_row.set_subtitle(
            "If 'AP Isolation' or 'Guest Network' is enabled on your router, devices cannot communicate directly; disable it.",
        );
        security_guide_group.add(&ap_row);

        let sub_row = libadwaita::ActionRow::new();
        sub_row.set_title("2. Same Subnet (LAN) and VPN");
        sub_row.set_subtitle(
            "Devices must be connected to the same local Wi-Fi/LAN subnet. Active VPN connections may isolate local traffic.",
        );
        security_guide_group.add(&sub_row);

        let pin_check_row = libadwaita::ActionRow::new();
        pin_check_row.set_title("3. Pairing PIN Code Verification");
        pin_check_row
            .set_subtitle("The Pairing PIN on both devices must match exactly (case-sensitive).");
        security_guide_group.add(&pin_check_row);

        sync_page.add(&security_guide_group);
        dialog.add(&sync_page);

        // Connect setting listeners to save to config.toml and sync with daemon via IPC
        let cfg_cell = Rc::new(RefCell::new(config));

        let cfg_pwd = Rc::clone(&cfg_cell);
        pwd_row.connect_active_notify(move |row| {
            cfg_pwd.borrow_mut().security.ignore_password_managers = row.is_active();
            Self::save_and_sync(&cfg_pwd.borrow());
        });

        let cfg_incog = Rc::clone(&cfg_cell);
        incognito_row.connect_active_notify(move |row| {
            cfg_incog.borrow_mut().security.ignore_incognito_windows = row.is_active();
            Self::save_and_sync(&cfg_incog.borrow());
        });

        let cfg_clean = Rc::clone(&cfg_cell);
        clean_urls_row.connect_active_notify(move |row| {
            cfg_clean.borrow_mut().security.auto_clean_tracking_urls = row.is_active();
            Self::save_and_sync(&cfg_clean.borrow());
        });

        let cfg_mode = Rc::clone(&cfg_cell);
        mode_row.connect_selected_notify(move |row| {
            let mode = if row.selected() == 1 {
                clipboard_history_core::config::AppFilterMode::Whitelist
            } else {
                clipboard_history_core::config::AppFilterMode::Blacklist
            };
            cfg_mode.borrow_mut().security.app_filter_mode = mode;
            Self::save_and_sync(&cfg_mode.borrow());
        });

        let cfg_list = Rc::clone(&cfg_cell);
        list_row.connect_changed(move |row| {
            let text = row.text().to_string();
            let items = text
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            cfg_list.borrow_mut().security.app_filter_list = items;
            Self::save_and_sync(&cfg_list.borrow());
        });

        let cfg_paste = Rc::clone(&cfg_cell);
        auto_paste_row.connect_active_notify(move |row| {
            cfg_paste.borrow_mut().paste.auto_paste = row.is_active();
            Self::save_and_sync(&cfg_paste.borrow());
        });

        let cfg_entries = Rc::clone(&cfg_cell);
        max_entries_row.connect_value_notify(move |row| {
            cfg_entries.borrow_mut().general.max_entries = row.value() as usize;
            Self::save_and_sync(&cfg_entries.borrow());
        });

        let cfg_ret = Rc::clone(&cfg_cell);
        retention_row.connect_value_notify(move |row| {
            cfg_ret.borrow_mut().general.retention_days = row.value() as u32;
            Self::save_and_sync(&cfg_ret.borrow());
        });

        let cfg_sync = Rc::clone(&cfg_cell);
        sync_toggle_row.connect_active_notify(move |row| {
            cfg_sync.borrow_mut().sync.enabled = row.is_active();
            Self::save_and_sync(&cfg_sync.borrow());
        });

        let debounce_timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let deb_clone = Rc::clone(&debounce_timer);
        let cfg_pin = Rc::clone(&cfg_cell);
        pin_row.connect_changed(move |row| {
            if let Some(source) = deb_clone.borrow_mut().take() {
                source.remove();
            }
            let new_pin = row.text().to_string();
            let cfg_ref = Rc::clone(&cfg_pin);
            let deb_ref = Rc::clone(&deb_clone);
            let source_id =
                glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
                    deb_ref.borrow_mut().take();
                    cfg_ref.borrow_mut().sync.pairing_pin = new_pin;
                    Self::save_and_sync(&cfg_ref.borrow());
                });
            *deb_clone.borrow_mut() = Some(source_id);
        });

        // Device name listener
        let cfg_dev = Rc::clone(&cfg_cell);
        dev_name_row.connect_changed(move |row| {
            cfg_dev.borrow_mut().sync.device_name = row.text().to_string();
            Self::save_and_sync(&cfg_dev.borrow());
        });

        // Query discovered peers from daemon with manual scan and pairing support
        let refresh_peers_cell: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let cfg_for_refresh = Rc::clone(&cfg_cell);
        let refresh_peers: Rc<dyn Fn()> = {
            let peers_box = peers_box.clone();
            let cfg_ref = Rc::clone(&cfg_for_refresh);
            let holder = Rc::clone(&refresh_peers_cell);
            Rc::new(move || {
                let box_clone = peers_box.clone();
                let cfg_inner = Rc::clone(&cfg_ref);
                let holder_inner = Rc::clone(&holder);

                glib::MainContext::default().spawn_local(async move {
                    let mut pending_pairings = Vec::new();
                    let mut trusted_peers = Vec::new();
                    let mut discovered_peers = Vec::new();

                    if let Ok(client) = IpcClient::connect().await {
                        if let Ok(IpcResponse::PendingPairings(p)) =
                            client.send(&IpcRequest::GetPendingPairingRequests).await
                        {
                            pending_pairings = p;
                        }
                        if let Ok(IpcResponse::TrustedPeers(t)) =
                            client.send(&IpcRequest::GetTrustedPeers).await
                        {
                            trusted_peers = t;
                        }
                        if let Ok(IpcResponse::SyncPeers(d)) =
                            client.send(&IpcRequest::GetSyncPeers).await
                        {
                            discovered_peers = d;
                        }
                    }

                    let manual_peers = cfg_inner.borrow().sync.peer_addresses.clone();

                    while let Some(child) = box_clone.first_child() {
                        box_clone.remove(&child);
                    }

                    let has_items = !pending_pairings.is_empty()
                        || !discovered_peers.is_empty()
                        || !trusted_peers.is_empty()
                        || !manual_peers.is_empty();

                    if !has_items {
                        let empty_row = libadwaita::ActionRow::new();
                        empty_row.set_title("No active devices found on network");
                        empty_row.set_subtitle(
                            "Click 'Scan Network' to search for devices or add an IP address manually in 'Add Peer' below.",
                        );

                        let scan_action_btn = gtk4::Button::builder()
                            .label("Scan Network")
                            .icon_name("view-refresh-symbolic")
                            .css_classes(["suggested-action"])
                            .build();
                        let holder_scan = Rc::clone(&holder_inner);
                        scan_action_btn.connect_clicked(move |btn| {
                            btn.set_sensitive(false);
                            let h = Rc::clone(&holder_scan);
                            glib::MainContext::default().spawn_local(async move {
                                if let Ok(client) = IpcClient::connect().await {
                                    let _ = client.send(&IpcRequest::TriggerSyncScan).await;
                                }
                                glib::timeout_add_local_once(
                                    std::time::Duration::from_millis(1200),
                                    move || {
                                        if let Some(ref f) = *h.borrow() {
                                            f();
                                        }
                                    },
                                );
                            });
                        });
                        empty_row.add_suffix(&scan_action_btn);
                        box_clone.append(&empty_row);
                        return;
                    }

                    // 1. Pending Incoming Pairing Requests
                    for req in pending_pairings {
                        let row = libadwaita::ActionRow::new();
                        row.set_title(&format!("Pairing Request: {}", req.device_name));
                        row.set_subtitle(&format!(
                            "Address: {}. This device wants to sync clipboards.",
                            req.peer_address
                        ));

                        let actions_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

                        let accept_btn = gtk4::Button::with_label("✓ Accept");
                        accept_btn.add_css_class("suggested-action");
                        let req_id_acc = req.request_id.clone();
                        let holder_acc = Rc::clone(&holder_inner);
                        accept_btn.connect_clicked(move |btn| {
                            btn.set_sensitive(false);
                            let r_id = req_id_acc.clone();
                            let h = Rc::clone(&holder_acc);
                            glib::MainContext::default().spawn_local(async move {
                                if let Ok(client) = IpcClient::connect().await {
                                    let _ = client
                                        .send(&IpcRequest::RespondPairingRequest {
                                            request_id: r_id,
                                            accept: true,
                                        })
                                        .await;
                                }
                                if let Some(ref f) = *h.borrow() {
                                    f();
                                }
                            });
                        });
                        actions_box.append(&accept_btn);

                        let reject_btn = gtk4::Button::with_label("✕ Reject");
                        reject_btn.add_css_class("destructive-action");
                        let req_id_rej = req.request_id.clone();
                        let holder_rej = Rc::clone(&holder_inner);
                        reject_btn.connect_clicked(move |btn| {
                            btn.set_sensitive(false);
                            let r_id = req_id_rej.clone();
                            let h = Rc::clone(&holder_rej);
                            glib::MainContext::default().spawn_local(async move {
                                if let Ok(client) = IpcClient::connect().await {
                                    let _ = client
                                        .send(&IpcRequest::RespondPairingRequest {
                                            request_id: r_id,
                                            accept: false,
                                        })
                                        .await;
                                }
                                if let Some(ref f) = *h.borrow() {
                                    f();
                                }
                            });
                        });
                        actions_box.append(&reject_btn);

                        row.add_suffix(&actions_box);
                        box_clone.append(&row);
                    }

                    // 2. Discovered, manual, and trusted peers
                    struct PeerEntry {
                        display_name: String,
                        addr: String,
                        is_trusted: bool,
                    }

                    let mut peer_map: Vec<PeerEntry> = Vec::new();

                    let check_trusted = |addr: &str, name: &str| -> bool {
                        let clean_ip = if let Some(idx) = addr.find(':') {
                            &addr[..idx]
                        } else {
                            addr
                        };
                        trusted_peers.iter().any(|t| {
                            t == addr
                                || t == clean_ip
                                || (!t.is_empty() && addr.starts_with(t))
                                || (!name.is_empty() && t == name)
                        })
                    };

                    for p in discovered_peers {
                        let (name, addr) = if let Some(idx) = p.find(" (") {
                            let name = p[..idx].trim().to_string();
                            let rest = p[idx + 2..].trim_end_matches(')').trim().to_string();
                            (name, rest)
                        } else {
                            (String::new(), p)
                        };

                        let trusted = check_trusted(&addr, &name);
                        peer_map.push(PeerEntry {
                            display_name: if name.is_empty() { addr.clone() } else { name },
                            addr,
                            is_trusted: trusted,
                        });
                    }

                    for m in manual_peers {
                        if !peer_map.iter().any(|p| p.addr == m || p.addr.starts_with(&m)) {
                            let trusted = check_trusted(&m, "");
                            peer_map.push(PeerEntry {
                                display_name: m.clone(),
                                addr: m,
                                is_trusted: trusted,
                            });
                        }
                    }

                    for t in &trusted_peers {
                        if (t.contains('.') || t.contains(':'))
                            && !peer_map.iter().any(|p| p.addr == *t || p.addr.starts_with(t))
                        {
                            peer_map.push(PeerEntry {
                                display_name: t.clone(),
                                addr: t.clone(),
                                is_trusted: true,
                            });
                        }
                    }

                    for peer in peer_map {
                        let row = libadwaita::ActionRow::new();
                        row.set_title(&peer.display_name);
                        row.set_subtitle(&peer.addr);

                        if peer.is_trusted {
                            let badge = gtk4::Label::new(Some("✓ Paired (Active Sync)"));
                            badge.add_css_class("success");
                            badge.add_css_class("caption");
                            row.add_suffix(&badge);

                            let unpair_btn = gtk4::Button::with_label("Unpair");
                            unpair_btn.add_css_class("flat");
                            let addr_unpair = peer.addr.clone();
                            let holder_unp = Rc::clone(&holder_inner);
                            unpair_btn.connect_clicked(move |btn| {
                                btn.set_sensitive(false);
                                let a = addr_unpair.clone();
                                let h = Rc::clone(&holder_unp);
                                glib::MainContext::default().spawn_local(async move {
                                    if let Ok(client) = IpcClient::connect().await {
                                        let _ = client
                                            .send(&IpcRequest::UnpairPeer {
                                                peer_identifier: a,
                                            })
                                            .await;
                                    }
                                    if let Some(ref f) = *h.borrow() {
                                        f();
                                    }
                                });
                            });
                            row.add_suffix(&unpair_btn);
                        } else {
                            let pair_btn = gtk4::Button::with_label("Send Sync Request");
                            pair_btn.add_css_class("suggested-action");

                            let addr_pair = peer.addr.clone();
                            let holder_pair = Rc::clone(&holder_inner);
                            let btn_pair = pair_btn.clone();
                            let row_pair = row.clone();

                            let send_pair_action = {
                                let addr_pair = addr_pair.clone();
                                let holder_pair = Rc::clone(&holder_pair);
                                let btn_pair = btn_pair.clone();
                                let row_pair = row_pair.clone();
                                move || {
                                    btn_pair.set_sensitive(false);
                                    btn_pair.set_label("Sending Request...");
                                    row_pair.set_subtitle("Waiting for remote confirmation (45s)...");

                                    let a = addr_pair.clone();
                                    let h = Rc::clone(&holder_pair);
                                    let b = btn_pair.clone();
                                    let r = row_pair.clone();

                                    glib::MainContext::default().spawn_local(async move {
                                        if let Ok(client) = IpcClient::connect().await {
                                            match client
                                                .send(&IpcRequest::SendPairingRequest {
                                                    peer_address: a.clone(),
                                                })
                                                .await
                                            {
                                                Ok(IpcResponse::PairingResult {
                                                    success: true,
                                                    message,
                                                    ..
                                                }) => {
                                                    b.set_label("✓ Paired!");
                                                    b.remove_css_class("suggested-action");
                                                    b.add_css_class("success");
                                                    r.set_subtitle(&message);
                                                    glib::timeout_add_local_once(
                                                        std::time::Duration::from_millis(1500),
                                                        move || {
                                                            if let Some(ref f) = *h.borrow() {
                                                                f();
                                                            }
                                                        },
                                                    );
                                                }
                                                Ok(IpcResponse::PairingResult {
                                                    success: false,
                                                    message,
                                                    ..
                                                }) => {
                                                    b.set_label("Failed");
                                                    b.set_tooltip_text(Some(&message));
                                                    r.set_subtitle(&message);
                                                    let h_fail = Rc::clone(&h);
                                                    glib::timeout_add_local_once(
                                                        std::time::Duration::from_secs(3),
                                                        move || {
                                                            if let Some(ref f) = *h_fail.borrow() {
                                                                f();
                                                            }
                                                        },
                                                    );
                                                }
                                                _ => {
                                                    b.set_label("Error");
                                                    let h_err = Rc::clone(&h);
                                                    glib::timeout_add_local_once(
                                                        std::time::Duration::from_secs(3),
                                                        move || {
                                                            if let Some(ref f) = *h_err.borrow() {
                                                                f();
                                                            }
                                                        },
                                                    );
                                                }
                                            }
                                        } else {
                                            b.set_label("IPC Error");
                                            b.set_sensitive(true);
                                        }
                                    });
                                }
                            };

                            let sp_clone = send_pair_action.clone();
                            pair_btn.connect_clicked(move |_| {
                                sp_clone();
                            });

                            row.set_activatable(true);
                            row.connect_activated(move |_| {
                                send_pair_action();
                            });

                            row.add_suffix(&pair_btn);
                        }

                        box_clone.append(&row);
                    }
                });
            })
        };
        *refresh_peers_cell.borrow_mut() = Some(Rc::clone(&refresh_peers));

        // Initial peer load
        refresh_peers();

        // Automated firewall check runner
        let refresh_firewall = Rc::new({
            let fw_box = fw_box.clone();
            move || {
                let box_clone = fw_box.clone();
                glib::MainContext::default().spawn_local(async move {
                    while let Some(child) = box_clone.first_child() {
                        box_clone.remove(&child);
                    }
                    let loading_row = libadwaita::ActionRow::new();
                    loading_row.set_title("Checking firewall and port rules...");
                    let spinner = gtk4::Spinner::new();
                    spinner.start();
                    loading_row.add_suffix(&spinner);
                    box_clone.append(&loading_row);

                    let res = if let Ok(client) = IpcClient::connect().await {
                        if let Ok(IpcResponse::FirewallStatus(status)) =
                            client.send(&IpcRequest::CheckFirewall).await
                        {
                            Some(status)
                        } else {
                            None
                        }
                    } else {
                        Some(clipboard_history_core::check_firewall_status())
                    };

                    while let Some(child) = box_clone.first_child() {
                        box_clone.remove(&child);
                    }

                    if let Some(status) = res {
                        let status_row = libadwaita::ActionRow::new();
                        status_row.set_title(&format!("Firewall: {}", status.firewall_name));
                        status_row.set_subtitle(&status.description);

                        let badge = gtk4::Label::new(None);
                        badge.add_css_class("caption");

                        if status.has_issue {
                            badge.set_text("Port Rule Required");
                            badge.add_css_class("warning");
                            status_row.add_suffix(&badge);
                            box_clone.append(&status_row);

                            if let Some(cmd) = status.recommended_command {
                                let cmd_row = libadwaita::ActionRow::new();
                                cmd_row.set_title("Command to Resolve Issue");
                                cmd_row.set_subtitle(&cmd);

                                let copy_btn = gtk4::Button::from_icon_name("edit-copy-symbolic");
                                copy_btn.set_tooltip_text(Some("Copy Command"));
                                copy_btn.add_css_class("flat");

                                let cmd_str = cmd.clone();
                                let btn_copy = copy_btn.clone();
                                copy_btn.connect_clicked(move |_| {
                                    if let Some(disp) = gdk4::Display::default() {
                                        disp.clipboard().set_text(&cmd_str);
                                    }
                                    let c = cmd_str.clone();
                                    glib::MainContext::default().spawn_local(async move {
                                        let _ =
                                            clipboard_history_core::copy_to_system_clipboard(&c)
                                                .await;
                                    });
                                    btn_copy.set_icon_name("emblem-ok-symbolic");
                                    btn_copy.set_tooltip_text(Some("Command Copied!"));
                                    let btn_reset = btn_copy.clone();
                                    glib::timeout_add_local_once(
                                        std::time::Duration::from_secs(2),
                                        move || {
                                            btn_reset.set_icon_name("edit-copy-symbolic");
                                            btn_reset.set_tooltip_text(Some("Copy Command"));
                                        },
                                    );
                                });

                                cmd_row.add_suffix(&copy_btn);
                                box_clone.append(&cmd_row);
                            }
                        } else {
                            badge.set_text("Ports Open (Ready)");
                            badge.add_css_class("success");
                            status_row.add_suffix(&badge);
                            box_clone.append(&status_row);
                        }
                    } else {
                        let err_row = libadwaita::ActionRow::new();
                        err_row.set_title("Could not check firewall status");
                        err_row.set_subtitle("Could not connect to background daemon service.");
                        box_clone.append(&err_row);
                    }
                });
            }
        });

        // Initial firewall check
        refresh_firewall();

        let fw_refresh_click = Rc::clone(&refresh_firewall);
        fw_refresh_btn.connect_clicked(move |_| {
            fw_refresh_click();
        });

        // Scan button click handler
        let refresh_for_scan = Rc::clone(&refresh_peers);
        let fw_refresh_for_scan = Rc::clone(&refresh_firewall);
        let peers_box_scan = peers_box.clone();
        scan_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            let btn_clone = btn.clone();
            let ref_fn = Rc::clone(&refresh_for_scan);
            let fw_ref_fn = Rc::clone(&fw_refresh_for_scan);
            let box_ref = peers_box_scan.clone();

            while let Some(child) = box_ref.first_child() {
                box_ref.remove(&child);
            }
            let scanning_row = libadwaita::ActionRow::new();
            scanning_row.set_title("Scanning local network...");
            scanning_row.set_subtitle("Broadcasting UDP probes across subnet...");
            let spinner = gtk4::Spinner::new();
            spinner.start();
            scanning_row.add_suffix(&spinner);
            box_ref.append(&scanning_row);

            glib::MainContext::default().spawn_local(async move {
                if let Ok(client) = IpcClient::connect().await {
                    let _ = client.send(&IpcRequest::TriggerSyncScan).await;
                }
                glib::timeout_add_local_once(std::time::Duration::from_millis(1200), move || {
                    ref_fn();
                    fw_ref_fn();
                    btn_clone.set_sensitive(true);
                });
            });
        });

        // Manual peers list management
        let render_manual_peers: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let refresh_for_render = Rc::clone(&refresh_peers);
        let render_fn: Rc<dyn Fn()> = {
            let cfg_ref = Rc::clone(&cfg_cell);
            let box_ref = manual_list_box.clone();
            let holder = Rc::clone(&render_manual_peers);
            let refresh_del_base = Rc::clone(&refresh_for_render);
            Rc::new(move || {
                while let Some(child) = box_ref.first_child() {
                    box_ref.remove(&child);
                }
                let peers = cfg_ref.borrow().sync.peer_addresses.clone();
                for (idx, addr) in peers.into_iter().enumerate() {
                    let row = libadwaita::ActionRow::new();
                    row.set_title(&addr);

                    let del_btn = gtk4::Button::from_icon_name("user-trash-symbolic");
                    del_btn.add_css_class("flat");
                    del_btn.add_css_class("circular");
                    let cfg_del = Rc::clone(&cfg_ref);
                    let holder_del = Rc::clone(&holder);
                    let refresh_del = Rc::clone(&refresh_del_base);
                    del_btn.connect_clicked(move |_| {
                        let mut guard = cfg_del.borrow_mut();
                        if idx < guard.sync.peer_addresses.len() {
                            guard.sync.peer_addresses.remove(idx);
                        }
                        drop(guard);
                        Self::save_and_sync(&cfg_del.borrow());
                        if let Some(ref f) = *holder_del.borrow() {
                            f();
                        }
                        refresh_del();
                    });
                    row.add_suffix(&del_btn);
                    box_ref.append(&row);
                }
            })
        };
        *render_manual_peers.borrow_mut() = Some(Rc::clone(&render_fn));
        render_fn();

        let cfg_add = Rc::clone(&cfg_cell);
        let holder_add = Rc::clone(&render_manual_peers);
        let refresh_add = Rc::clone(&refresh_peers);
        add_peer_row.connect_apply(move |row| {
            let addr = row.text().trim().to_string();
            if !addr.is_empty() {
                let mut guard = cfg_add.borrow_mut();
                if !guard.sync.peer_addresses.contains(&addr) {
                    guard.sync.peer_addresses.push(addr);
                }
                drop(guard);
                Self::save_and_sync(&cfg_add.borrow());
                row.set_text("");
                if let Some(ref f) = *holder_add.borrow() {
                    f();
                }
                refresh_add();
            }
        });

        // Auto-refresh discovered peers and pending pairing requests every 3 seconds while dialog is open
        let dlg_weak = dialog.downgrade();
        let ref_periodic = Rc::clone(&refresh_peers);
        glib::timeout_add_local(std::time::Duration::from_secs(3), move || {
            if let Some(dlg) = dlg_weak.upgrade() {
                if dlg.is_visible() {
                    ref_periodic();
                    return glib::ControlFlow::Continue;
                }
            }
            glib::ControlFlow::Break
        });

        dialog.present();
    }

    fn save_and_sync(cfg: &AppConfig) {
        if let Err(e) = cfg.save(AppConfig::config_path()) {
            tracing::warn!("Failed to save config: {}", e);
        } else {
            info!("Updated configuration saved to disk");
        }

        let to_send = cfg.clone();
        glib::MainContext::default().spawn_local(async move {
            if let Ok(client) = IpcClient::connect().await {
                let _ = client
                    .send(&IpcRequest::UpdateConfig {
                        config: Box::new(to_send),
                    })
                    .await;
                info!("Sent updated configuration to daemon via IPC");
            }
        });
    }
}

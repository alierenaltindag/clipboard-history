use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::injector::InjectorCascade;
use clipboard_history_core::security::CryptoEngine;
use clipboard_history_core::storage::SqliteRepository;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};
use zbus::interface;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

/// Robust executable path finder
pub fn get_executable_path(name: &str) -> PathBuf {
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            let candidate = parent.join(name);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    if let Some(dirs) = directories::BaseDirs::new() {
        let candidate = dirs.home_dir().join(".local/bin").join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    if let Ok(path) = which::which(name) {
        return path;
    }
    PathBuf::from(name)
}

fn load_tray_icon_pixmap() -> Vec<(i32, i32, Vec<u8>)> {
    static ICON_32_BYTES: &[u8] =
        include_bytes!("../../../packaging/desktop/icons/hicolor/32x32/apps/clipboard-history.png");
    if let Ok(img) = image::load_from_memory(ICON_32_BYTES) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let mut argb = Vec::with_capacity((width * height * 4) as usize);
        for pixel in rgba.pixels() {
            let [r, g, b, a] = pixel.0;
            argb.push(a);
            argb.push(r);
            argb.push(g);
            argb.push(b);
        }
        vec![(width as i32, height as i32, argb)]
    } else {
        vec![]
    }
}

fn ov<T: Into<Value<'static>>>(val: T) -> OwnedValue {
    OwnedValue::try_from(val.into()).expect("Failed to convert Value to OwnedValue")
}

pub struct StatusNotifierItem {
    is_paused: Arc<AtomicBool>,
    icon_pixmap: Vec<(i32, i32, Vec<u8>)>,
}

impl StatusNotifierItem {
    pub fn new(is_paused: Arc<AtomicBool>) -> Self {
        let icon_pixmap = load_tray_icon_pixmap();
        Self {
            is_paused,
            icon_pixmap,
        }
    }
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl StatusNotifierItem {
    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "clipboard-history"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        "Clipboard History"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        if self.is_paused.load(Ordering::Relaxed) {
            "Passive"
        } else {
            "Active"
        }
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        if self.is_paused.load(Ordering::Relaxed) {
            "action-unavailable-symbolic"
        } else {
            "clipboard-history"
        }
    }

    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        "clipboard-history"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> String {
        directories::BaseDirs::new()
            .map(|d| d.data_dir().join("icons").to_string_lossy().to_string())
            .unwrap_or_default()
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> &[(i32, i32, Vec<u8>)] {
        &self.icon_pixmap
    }

    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        Vec::new()
    }

    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        Vec::new()
    }

    #[zbus(property)]
    fn attention_movie_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/MenuBar").unwrap()
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    #[allow(clippy::type_complexity)]
    fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        let is_p = self.is_paused.load(Ordering::Relaxed);
        (
            if is_p {
                "action-unavailable-symbolic".to_string()
            } else {
                "clipboard-history".to_string()
            },
            Vec::new(),
            "Clipboard History".to_string(),
            if is_p {
                "Clipboard tracking paused".to_string()
            } else {
                "Clipboard tracking active (Super+C)".to_string()
            },
        )
    }

    /// Primary left-click on tray icon toggles the GUI popup (debounced to avoid toggle storms)
    async fn activate(&self, _x: i32, _y: i32) {
        static LAST_ACTIVATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let prev = LAST_ACTIVATE.swap(now, std::sync::atomic::Ordering::Relaxed);
        if now.saturating_sub(prev) < 500 {
            debug!("Tray activate debounced");
            return;
        }

        info!("Tray icon activated: triggering popup window");
        let bin = get_executable_path("clipboard-history");
        let _ = tokio::process::Command::new(bin).arg("toggle").spawn();
    }

    /// Secondary click (middle-click) toggles Pause / Resume tracking
    async fn secondary_activate(&self, _x: i32, _y: i32) {
        let currently_paused = self.is_paused.load(Ordering::Relaxed);
        let new_state = !currently_paused;
        self.is_paused.store(new_state, Ordering::Relaxed);
        info!(
            "Tray icon secondary activate: pause state toggled to {}",
            new_state
        );
    }

    /// Right-click context menu action (menu is provided by DBusMenu at /MenuBar)
    async fn context_menu(&self, _x: i32, _y: i32) {
        debug!("Tray icon context menu requested (handled via DBusMenu /MenuBar)");
    }

    async fn scroll(&self, _delta: i32, _orientation: &str) {}
}

#[derive(Clone)]
pub struct DbusMenuServer {
    repo: SqliteRepository,
    crypto: Arc<CryptoEngine>,
    config: Arc<RwLock<AppConfig>>,
    is_paused: Arc<AtomicBool>,
    revision: Arc<std::sync::atomic::AtomicU32>,
}

impl DbusMenuServer {
    pub fn new(
        repo: SqliteRepository,
        crypto: Arc<CryptoEngine>,
        config: Arc<RwLock<AppConfig>>,
        is_paused: Arc<AtomicBool>,
    ) -> Self {
        Self {
            repo,
            crypto,
            config,
            is_paused,
            revision: Arc::new(std::sync::atomic::AtomicU32::new(1)),
        }
    }

    fn decrypt_entry(&self, entry: &mut ClipboardEntry) {
        if let Ok(Some((wrapped_dek, _))) = self.repo.get_entry_dek(&entry.id) {
            if self
                .crypto
                .decrypt_entry_envelope(entry, &wrapped_dek)
                .is_ok()
            {
                return;
            }
        }
        if let Some(ref text) = entry.text_content {
            if let Ok(dec) = self.crypto.decrypt_str(text) {
                entry.text_content = Some(dec);
            }
        }
        if let Ok(dec_preview) = self.crypto.decrypt_str(&entry.preview) {
            entry.preview = dec_preview;
        }
    }

    async fn build_menu_layout(&self) -> (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>) {
        let mut root_props = HashMap::new();
        root_props.insert("children-display".to_string(), ov("submenu"));

        let mut children = Vec::new();

        // 1. Show Clipboard History
        let mut show_props = HashMap::new();
        show_props.insert(
            "label".to_string(),
            ov("📋 Open Clipboard History (Super+C)"),
        );
        show_props.insert("icon-name".to_string(), ov("clipboard-history"));
        show_props.insert("type".to_string(), ov("standard"));
        children.push(Self::make_item_value(1, show_props, Vec::new()));

        // 2. Separator
        let mut sep1_props = HashMap::new();
        sep1_props.insert("type".to_string(), ov("separator"));
        children.push(Self::make_item_value(2, sep1_props, Vec::new()));

        // 3. Recent History Entries (up to 10)
        let recent = self.repo.list(10, 0, None, false).unwrap_or_default();
        if recent.is_empty() {
            let mut empty_props = HashMap::new();
            empty_props.insert("label".to_string(), ov("(Clipboard history is empty)"));
            empty_props.insert("enabled".to_string(), ov(false));
            children.push(Self::make_item_value(99, empty_props, Vec::new()));
        } else {
            for (idx, mut entry) in recent.into_iter().enumerate() {
                self.decrypt_entry(&mut entry);
                let raw_text = entry
                    .text_content
                    .as_deref()
                    .unwrap_or(entry.preview.as_str());
                let clean_line = raw_text.replace('\n', " ").replace('\r', "");
                let trimmed = clean_line.trim();
                let short_preview = if trimmed.chars().count() > 42 {
                    format!("{}…", trimmed.chars().take(42).collect::<String>())
                } else if trimmed.is_empty() {
                    format!("[{}]", entry.entry_type)
                } else {
                    trimmed.to_string()
                };

                // In DBusMenu, double underscore displays as single underscore
                let safe_label = format!("{}. {}", idx + 1, short_preview.replace('_', "__"));

                let icon = match entry.entry_type {
                    clipboard_history_core::domain::EntryType::Image => "image-x-generic",
                    clipboard_history_core::domain::EntryType::Code => "text-x-script",
                    clipboard_history_core::domain::EntryType::UriList => "applications-internet",
                    _ => "text-x-generic",
                };

                let mut item_props = HashMap::new();
                item_props.insert("label".to_string(), ov(safe_label));
                item_props.insert("icon-name".to_string(), ov(icon));
                item_props.insert("type".to_string(), ov("standard"));
                children.push(Self::make_item_value(
                    100 + idx as i32,
                    item_props,
                    Vec::new(),
                ));
            }
        }

        // 4. Separator
        let mut sep2_props = HashMap::new();
        sep2_props.insert("type".to_string(), ov("separator"));
        children.push(Self::make_item_value(50, sep2_props, Vec::new()));

        // 5. Pause / Resume Tracking
        let is_paused = self.is_paused.load(Ordering::Relaxed);
        let mut pause_props = HashMap::new();
        pause_props.insert(
            "label".to_string(),
            ov(if is_paused {
                "▶  Resume Monitoring"
            } else {
                "⏸  Pause Monitoring"
            }),
        );
        pause_props.insert(
            "icon-name".to_string(),
            ov(if is_paused {
                "media-playback-start-symbolic"
            } else {
                "media-playback-pause-symbolic"
            }),
        );
        children.push(Self::make_item_value(51, pause_props, Vec::new()));

        // 6. Clear History
        let mut clear_props = HashMap::new();
        clear_props.insert("label".to_string(), ov("🗑  Clear History"));
        clear_props.insert("icon-name".to_string(), ov("edit-clear-all-symbolic"));
        children.push(Self::make_item_value(52, clear_props, Vec::new()));

        // 7. Preferences
        let mut pref_props = HashMap::new();
        pref_props.insert("label".to_string(), ov("⚙  Preferences..."));
        pref_props.insert("icon-name".to_string(), ov("preferences-system-symbolic"));
        children.push(Self::make_item_value(53, pref_props, Vec::new()));

        // 8. Separator
        let mut sep3_props = HashMap::new();
        sep3_props.insert("type".to_string(), ov("separator"));
        children.push(Self::make_item_value(54, sep3_props, Vec::new()));

        // 9. Quit
        let mut quit_props = HashMap::new();
        quit_props.insert("label".to_string(), ov("❌  Quit Clipboard History"));
        quit_props.insert("icon-name".to_string(), ov("application-exit-symbolic"));
        children.push(Self::make_item_value(55, quit_props, Vec::new()));

        (0, root_props, children)
    }

    fn make_item_value(
        id: i32,
        props: HashMap<String, OwnedValue>,
        children: Vec<OwnedValue>,
    ) -> OwnedValue {
        let tuple = (id, props, children);
        OwnedValue::try_from(Value::from(tuple)).unwrap()
    }
}

#[interface(name = "com.canonical.dbusmenu")]
impl DbusMenuServer {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        let path = directories::BaseDirs::new()
            .map(|d| d.data_dir().join("icons").to_string_lossy().to_string())
            .unwrap_or_default();
        vec![path]
    }

    async fn get_layout(
        &self,
        parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> zbus::fdo::Result<(u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>))> {
        if parent_id != 0 {
            return Err(zbus::fdo::Error::Failed(
                "Only root parentId 0 is supported".to_string(),
            ));
        }
        let layout = self.build_menu_layout().await;
        let rev = self.revision.fetch_add(1, Ordering::Relaxed);
        Ok((rev, layout))
    }

    async fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> zbus::fdo::Result<Vec<(i32, HashMap<String, OwnedValue>)>> {
        let (_, _, children) = self.build_menu_layout().await;
        let mut results = Vec::new();
        for child in children {
            if let Ok(Value::Structure(st)) = Value::try_from(&child) {
                let fields = st.into_fields();
                if fields.len() >= 2 {
                    if let Ok(item_id) = i32::try_from(&fields[0]) {
                        if ids.is_empty() || ids.contains(&item_id) {
                            if let Ok(props) = HashMap::<String, OwnedValue>::try_from(
                                fields[1].try_clone().unwrap(),
                            ) {
                                results.push((item_id, props));
                            }
                        }
                    }
                }
            }
        }
        Ok(results)
    }

    async fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let props = self
            .get_group_properties(vec![id], vec![name.clone()])
            .await?;
        if let Some((_, map)) = props.into_iter().next() {
            if let Some(val) = map.get(&name) {
                return Ok(val.try_clone().unwrap());
            }
        }
        Ok(ov(""))
    }

    async fn event(
        &self,
        id: i32,
        event_id: String,
        _data: OwnedValue,
        _timestamp: u32,
    ) -> zbus::fdo::Result<()> {
        if event_id != "clicked" {
            return Ok(());
        }

        info!("Tray menu item clicked: id={}", id);

        match id {
            1 => {
                // Show GUI
                let bin = get_executable_path("clipboard-history");
                let _ = tokio::process::Command::new(bin).arg("toggle").spawn();
            }
            51 => {
                // Toggle Pause
                let cur = self.is_paused.load(Ordering::Relaxed);
                self.is_paused.store(!cur, Ordering::Relaxed);
                info!("Clipboard monitoring toggled to {}", !cur);
            }
            52 => {
                // Clear History
                let _ = self.repo.clear(false);
                info!("Cleared unpinned clipboard history from tray");
            }
            53 => {
                // Preferences
                let bin = get_executable_path("clipboard-history-gui");
                let _ = tokio::process::Command::new(bin).arg("--settings").spawn();
            }
            55 => {
                // Quit Daemon
                info!("Quit selected from tray menu. Exiting daemon.");
                std::process::exit(0);
            }
            item_id if (100..200).contains(&item_id) => {
                // Clicked recent item
                let index = (item_id - 100) as usize;
                let recent = self.repo.list(10, 0, None, false).unwrap_or_default();
                if let Some(mut entry) = recent.into_iter().nth(index) {
                    self.decrypt_entry(&mut entry);
                    let text = entry.text_content.clone().unwrap_or(entry.preview.clone());
                    clipboard_history_core::copy_to_system_clipboard(&text).await;
                    info!("Copied item {} to system clipboard from tray", index + 1);

                    let paste_cfg = self.config.read().await.paste.clone();
                    if paste_cfg.auto_paste {
                        let delay_ms = paste_cfg.effective_delay_ms();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                            let cascade = InjectorCascade::new();
                            let (name, success) = cascade.execute_paste().await;
                            info!(
                                "Auto-paste from tray: injector={}, success={}",
                                name, success
                            );
                        });
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn event_group(
        &self,
        events: Vec<(i32, String, OwnedValue, u32)>,
    ) -> zbus::fdo::Result<Vec<i32>> {
        for (id, event_id, data, timestamp) in events {
            let _ = self.event(id, event_id, data, timestamp).await;
        }
        Ok(Vec::new())
    }

    async fn about_to_show(&self, _id: i32) -> zbus::fdo::Result<bool> {
        Ok(true)
    }

    async fn about_to_show_group(&self, _ids: Vec<i32>) -> zbus::fdo::Result<(Vec<i32>, Vec<i32>)> {
        Ok((vec![0], Vec::new()))
    }
}

/// Spawns the D-Bus StatusNotifierItem and DBusMenu services and registers with StatusNotifierWatcher
pub async fn start_tray_service(
    repo: SqliteRepository,
    crypto: Arc<CryptoEngine>,
    config: Arc<RwLock<AppConfig>>,
    is_paused: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sni = StatusNotifierItem::new(Arc::clone(&is_paused));
    let menu = DbusMenuServer::new(repo, crypto, config, Arc::clone(&is_paused));

    let service_name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());

    let connection = zbus::connection::Builder::session()?
        .name(service_name.as_str())?
        .serve_at("/StatusNotifierItem", sni)?
        .serve_at("/MenuBar", menu)?
        .build()
        .await?;

    info!("D-Bus StatusNotifierItem served at {}", service_name);

    // Register with StatusNotifierWatcher
    let service_str = service_name.clone();
    let watcher_proxy = zbus::Proxy::new(
        &connection,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    )
    .await;

    if let Ok(watcher) = watcher_proxy {
        if let Err(e) = watcher
            .call_method("RegisterStatusNotifierItem", &(service_str.as_str()))
            .await
        {
            debug!(
                "Could not register with StatusNotifierWatcher (desktop may not have an active tray watcher): {}",
                e
            );
        } else {
            info!(
                "Registered StatusNotifierItem with org.kde.StatusNotifierWatcher: {}",
                service_str
            );
        }
    } else {
        debug!("StatusNotifierWatcher not present on session bus");
    }

    // Keep connection alive in background
    futures_util::future::pending::<()>().await;
    Ok(())
}

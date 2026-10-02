use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info};
use zbus::interface;

pub struct StatusNotifierItem {
    is_paused: Arc<AtomicBool>,
}

impl StatusNotifierItem {
    pub fn new(is_paused: Arc<AtomicBool>) -> Self {
        Self { is_paused }
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
    fn item_is_menu(&self) -> bool {
        false
    }

    /// Primary left-click on tray icon toggles the Win+V GUI popup
    async fn activate(&self, _x: i32, _y: i32) {
        info!("Tray icon activated: triggering popup window");
        let _ = tokio::process::Command::new("clipboard-history")
            .arg("toggle")
            .spawn();
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

    /// Right-click context menu action: toggles popup or pause
    async fn context_menu(&self, _x: i32, _y: i32) {
        debug!("Tray icon context menu requested");
        let _ = tokio::process::Command::new("clipboard-history")
            .arg("toggle")
            .spawn();
    }

    async fn scroll(&self, _delta: i32, _orientation: &str) {}
}

/// Spawns the D-Bus StatusNotifierItem service and registers with StatusNotifierWatcher
pub async fn start_tray_service(
    is_paused: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sni = StatusNotifierItem::new(is_paused);
    let service_name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());

    let connection = zbus::connection::Builder::session()?
        .name(service_name.as_str())?
        .serve_at("/StatusNotifierItem", sni)?
        .build()
        .await?;

    info!("D-Bus StatusNotifierItem served at {}", service_name);

    // Attempt to register with StatusNotifierWatcher if available on the desktop
    let proxy = zbus::Proxy::new(
        &connection,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    )
    .await;

    if let Ok(watcher) = proxy {
        let path = "/StatusNotifierItem";
        if let Err(e) = watcher
            .call_method("RegisterStatusNotifierItem", &(path))
            .await
        {
            debug!(
                "Could not register with StatusNotifierWatcher (desktop may not have an active tray watcher): {}",
                e
            );
        } else {
            info!("Registered StatusNotifierItem with org.kde.StatusNotifierWatcher");
        }
    } else {
        debug!("StatusNotifierWatcher not present on session bus");
    }

    // Keep connection alive in background
    futures_util::future::pending::<()>().await;
    Ok(())
}

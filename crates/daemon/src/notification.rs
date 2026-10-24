use crate::server::PendingPairingsMap;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info};

/// Dispatches an actionable desktop notification for an incoming pairing request.
/// Uses FreeDesktop Notifications D-Bus spec (org.freedesktop.Notifications) via zbus,
/// with robust fallback to notify-send across different Linux distributions.
pub async fn dispatch_pairing_notification(
    origin_device: String,
    peer_addr: String,
    request_id: String,
    pending_pairings: PendingPairingsMap,
) {
    let body = format!(
        "'{}' ({}) wants to sync clipboard with this computer.",
        origin_device, peer_addr
    );

    // 1. Try D-Bus FreeDesktop Notifications interface via zbus
    if try_dbus_notification(
        &origin_device,
        &peer_addr,
        &body,
        &request_id,
        pending_pairings.clone(),
    )
    .await
    .is_ok()
    {
        return;
    }

    // 2. Fallback to CLI notify-send with distribution-compatible flags
    tokio::spawn(async move {
        fallback_notify_send(&body, &request_id, pending_pairings).await;
    });
}

async fn try_dbus_notification(
    origin_device: &str,
    peer_addr: &str,
    body: &str,
    request_id: &str,
    pending_pairings: PendingPairingsMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await?;

    let mut hints: HashMap<&str, zbus::zvariant::Value> = HashMap::new();
    hints.insert("urgency", zbus::zvariant::Value::U8(2)); // Critical urgency

    let actions = vec!["accept", "Accept", "reject", "Decline"];

    // Subscribe to ActionInvoked signal before sending notification to avoid race conditions
    let mut signal_stream = proxy.receive_signal("ActionInvoked").await?;

    let notif_id: u32 = proxy
        .call(
            "Notify",
            &(
                "Clipboard History",
                0u32,
                "network-wireless-symbolic",
                "Clipboard Pairing Request",
                body,
                actions,
                hints,
                45_000i32,
            ),
        )
        .await?;

    info!(
        "Dispatched FreeDesktop pairing notification (id={}) for '{}' ({})",
        notif_id, origin_device, peer_addr
    );

    let req_id = request_id.to_string();
    tokio::spawn(async move {
        let timeout_fut = tokio::time::sleep(Duration::from_secs(45));
        tokio::pin!(timeout_fut);

        loop {
            tokio::select! {
                _ = &mut timeout_fut => {
                    debug!("D-Bus pairing notification timed out (id={})", notif_id);
                    break;
                }
                Some(msg) = signal_stream.next() => {
                    if let Ok((id, action_key)) = msg.body().deserialize::<(u32, String)>() {
                        if id == notif_id {
                            info!("D-Bus notification action invoked: id={}, action={}", id, action_key);
                            if action_key == "accept" || action_key == "reject" {
                                let mut map = pending_pairings.write().await;
                                if let Some(item) = map.remove(&req_id) {
                                    let _ = item.tx.send(action_key == "accept");
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(())
}

async fn fallback_notify_send(body: &str, request_id: &str, pending_pairings: PendingPairingsMap) {
    // Try with -A flag first (standard on many Arch/CachyOS and Ubuntu systems)
    let res = tokio::process::Command::new("notify-send")
        .args([
            "-a",
            "Clipboard History",
            "-i",
            "network-wireless-symbolic",
            "-u",
            "critical",
            "-A",
            "accept=Accept",
            "-A",
            "reject=Decline",
            "Clipboard Pairing Request",
            body,
        ])
        .output()
        .await;

    let output = match res {
        Ok(out) if out.status.success() => Some(out),
        _ => {
            // Fall back to --action format
            tokio::process::Command::new("notify-send")
                .args([
                    "-a",
                    "Clipboard History",
                    "-i",
                    "network-wireless-symbolic",
                    "-u",
                    "critical",
                    "--action=accept=Accept",
                    "--action=reject=Decline",
                    "Clipboard Pairing Request",
                    body,
                ])
                .output()
                .await
                .ok()
        }
    };

    if let Some(out) = output {
        let action = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if action == "accept" || action == "reject" {
            let mut map = pending_pairings.write().await;
            if let Some(item) = map.remove(request_id) {
                let _ = item.tx.send(action == "accept");
            }
        }
    }
}

/// Dispatches an actionable desktop notification for an available new version.
pub async fn dispatch_update_notification(release: clipboard_history_core::updater::ReleaseInfo) {
    let body = format!(
        "A new version ({}) is available!\nWould you like to update now?",
        release.tag_name
    );

    if try_dbus_update_notification(&release, &body).await.is_ok() {
        return;
    }

    tokio::spawn(async move {
        fallback_update_notify_send(&release, &body).await;
    });
}

async fn try_dbus_update_notification(
    release: &clipboard_history_core::updater::ReleaseInfo,
    body: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await?;

    let mut hints: HashMap<&str, zbus::zvariant::Value> = HashMap::new();
    hints.insert("urgency", zbus::zvariant::Value::U8(1));

    let actions = vec!["update", "Update Now", "dismiss", "Later"];
    let mut signal_stream = proxy.receive_signal("ActionInvoked").await?;

    let notif_id: u32 = proxy
        .call(
            "Notify",
            &(
                "Clipboard History",
                0u32,
                "system-software-update",
                "Update Available",
                body,
                actions,
                hints,
                30_000i32,
            ),
        )
        .await?;

    info!(
        "Dispatched FreeDesktop update notification (id={})",
        notif_id
    );

    let release_clone = release.clone();
    tokio::spawn(async move {
        let timeout_fut = tokio::time::sleep(Duration::from_secs(30));
        tokio::pin!(timeout_fut);

        loop {
            tokio::select! {
                _ = &mut timeout_fut => break,
                Some(msg) = signal_stream.next() => {
                    if let Ok((id, action_key)) = msg.body().deserialize::<(u32, String)>() {
                        if id == notif_id {
                            if action_key == "update" {
                                execute_confirmed_update(&release_clone).await;
                            }
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(())
}

async fn fallback_update_notify_send(
    release: &clipboard_history_core::updater::ReleaseInfo,
    body: &str,
) {
    let res = tokio::process::Command::new("notify-send")
        .args([
            "-a",
            "Clipboard History",
            "-i",
            "system-software-update",
            "-u",
            "normal",
            "-A",
            "update=Update Now",
            "-A",
            "dismiss=Later",
            "Update Available",
            body,
        ])
        .output()
        .await;

    if let Ok(out) = res {
        let action = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if action == "update" {
            execute_confirmed_update(release).await;
        }
    }
}

async fn execute_confirmed_update(release: &clipboard_history_core::updater::ReleaseInfo) {
    info!(
        "Starting user-confirmed update for release {}",
        release.tag_name
    );
    match clipboard_history_core::updater::download_and_verify(release).await {
        Ok(tarball_path) => {
            match clipboard_history_core::updater::install_update_from_tarball(&tarball_path) {
                Ok(installed) => {
                    info!("Updated binaries: {:?}", installed);
                    dispatch_update_success_notification(&release.version).await;
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    let _ = clipboard_history_core::updater::restart_daemon_service().await;
                }
                Err(e) => {
                    tracing::error!("Failed to install update: {}", e);
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to download update: {}", e);
        }
    }
}

/// Dispatches a simple success desktop notification when an update finishes.
pub async fn dispatch_update_success_notification(version: &str) {
    let msg = format!(
        "Clipboard History has been successfully updated to v{}. Please restart the application to use the new version.",
        version
    );
    let _ = tokio::process::Command::new("notify-send")
        .args([
            "-a",
            "Clipboard History",
            "-i",
            "emblem-default",
            "Update Successful",
            &msg,
        ])
        .output()
        .await;
}

use clap::Subcommand;
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use clipboard_history_core::updater::{
    check_for_updates, download_and_verify, install_update_from_tarball, restart_daemon_service,
};
use std::path::Path;

#[derive(Subcommand, Debug, Clone)]
pub enum UpdateAction {
    #[command(about = "Check if a newer version is available on GitHub")]
    Check,

    #[command(about = "Download, verify SHA-256, and install the latest version")]
    Apply,
}

pub async fn handle_update(action: Option<UpdateAction>, socket_path: &Path) {
    let act = action.unwrap_or(UpdateAction::Check);

    match act {
        UpdateAction::Check => {
            println!("🔍 Checking GitHub for Clipboard History updates...");
            let current_version = env!("CARGO_PKG_VERSION");
            println!("Installed version: v{}", current_version);

            // Attempt via daemon IPC first
            let mut checked_via_ipc = false;
            if let Ok(IpcResponse::UpdateCheckResult {
                update_available,
                latest_version,
                release_notes,
                ..
            }) = IpcClient::send_request(socket_path, &IpcRequest::CheckForUpdates).await
            {
                checked_via_ipc = true;
                if update_available {
                    println!("✨ A newer version is available: v{}!", latest_version);
                    if let Some(notes) = release_notes {
                        println!("\nRelease Notes:\n{}", notes.trim());
                    }
                    println!("\nTo apply this update, run:");
                    println!("  clipboard-history update apply");
                } else {
                    println!("✅ You are running the latest version (v{}).", current_version);
                }
            }

            // Fallback to direct check if daemon IPC was unavailable
            if !checked_via_ipc {
                match check_for_updates(current_version).await {
                    Ok(Some(release)) => {
                        println!("✨ A newer version is available: v{}!", release.version);
                        if let Some(notes) = release.release_notes {
                            println!("\nRelease Notes:\n{}", notes.trim());
                        }
                        println!("\nTo apply this update, run:");
                        println!("  clipboard-history update apply");
                    }
                    Ok(None) => {
                        println!("✅ You are running the latest version (v{}).", current_version);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to check for updates: {}", e);
                    }
                }
            }
        }

        UpdateAction::Apply => {
            println!("🚀 Preparing to update Clipboard History...");
            let current_version = env!("CARGO_PKG_VERSION");

            // Try applying through running daemon first
            let mut applied_via_ipc = false;
            println!("📦 Requesting daemon to download, verify, and install update...");
            if let Ok(IpcResponse::UpdateApplyResult { success, message }) =
                IpcClient::send_request(socket_path, &IpcRequest::ApplyUpdate).await
            {
                applied_via_ipc = true;
                if success {
                    println!("✅ {}", message);
                    println!("🎉 Update applied successfully! Daemon is restarting.");
                } else {
                    eprintln!("❌ Update failed: {}", message);
                }
            }

            // Direct standalone fallback if daemon was not running
            if !applied_via_ipc {
                println!("ℹ️ Daemon not reachable; performing direct standalone update...");
                match check_for_updates(current_version).await {
                    Ok(Some(release)) => {
                        println!("📥 Downloading {}...", release.tarball_name);
                        match download_and_verify(&release).await {
                            Ok(tar_path) => {
                                println!("🔒 SHA-256 verified. Installing binaries to ~/.local/bin/...");
                                match install_update_from_tarball(&tar_path) {
                                    Ok(installed) => {
                                        println!(
                                            "✅ Successfully updated: {}",
                                            installed.join(", ")
                                        );
                                        let _ = restart_daemon_service().await;
                                        println!("🎉 Update complete! New version: v{}", release.version);
                                    }
                                    Err(e) => {
                                        eprintln!("❌ Failed to install binaries: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("❌ Download/verification failed: {}", e);
                            }
                        }
                    }
                    Ok(None) => {
                        println!("✅ Already running the latest version (v{}).", current_version);
                    }
                    Err(e) => {
                        eprintln!("❌ Update check failed: {}", e);
                    }
                }
            }
        }
    }
}

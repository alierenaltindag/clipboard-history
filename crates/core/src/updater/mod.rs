use crate::error::{CoreError, Result};
use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tar::Archive;
use tracing::{debug, error, info, warn};

pub const GITHUB_REPO: &str = "alierenaltindag/clipboard-history";
pub const USER_AGENT: &str = "ClipboardHistory-Updater";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub version: String,
    pub tag_name: String,
    pub release_notes: Option<String>,
    pub tarball_url: String,
    pub tarball_name: String,
    pub checksums_url: Option<String>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GitHubAsset>,
    draft: bool,
    prerelease: bool,
}

/// Compares two version strings (handles optional 'v' prefix) using semver.
/// Returns true if `candidate` is strictly greater than `current`.
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    let clean_curr = current.trim().trim_start_matches('v');
    let clean_cand = candidate.trim().trim_start_matches('v');

    match (Version::parse(clean_curr), Version::parse(clean_cand)) {
        (Ok(curr), Ok(cand)) => cand > curr,
        _ => clean_cand != clean_curr,
    }
}

/// Queries GitHub REST API for the latest release and checks if a newer version exists.
pub async fn check_for_updates(current_version: &str) -> Result<Option<ReleaseInfo>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| CoreError::Update(format!("Failed to build HTTP client: {}", e)))?;

    debug!("Checking for updates at {}", url);
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| CoreError::Update(format!("Network request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(CoreError::Update(format!(
            "GitHub API responded with status {}",
            response.status()
        )));
    }

    let release: GitHubRelease = response
        .json()
        .await
        .map_err(|e| CoreError::Update(format!("Failed to parse GitHub response: {}", e)))?;

    if release.draft || release.prerelease {
        info!("Latest release is draft or prerelease; skipping");
        return Ok(None);
    }

    let latest_version = release.tag_name.trim_start_matches('v').to_string();
    if !is_newer_version(current_version, &latest_version) {
        info!(
            "Current version {} is up to date (latest: {})",
            current_version, latest_version
        );
        return Ok(None);
    }

    // Find linux x86_64 tarball asset and SHA256SUMS.txt
    let mut tarball_asset: Option<(String, String)> = None;
    let mut checksums_asset: Option<String> = None;

    for asset in release.assets {
        if asset.name.ends_with(".tar.gz")
            && (asset.name.contains("x86_64") || asset.name.contains("linux"))
        {
            tarball_asset = Some((asset.name, asset.browser_download_url));
        } else if asset.name == "SHA256SUMS.txt" || asset.name.ends_with(".sha256") {
            checksums_asset = Some(asset.browser_download_url);
        }
    }

    let (tarball_name, tarball_url) = match tarball_asset {
        Some(t) => t,
        None => {
            return Err(CoreError::Update(
                "No compatible Linux x86_64 tar.gz asset found in latest GitHub release"
                    .to_string(),
            ));
        }
    };

    Ok(Some(ReleaseInfo {
        version: latest_version,
        tag_name: release.tag_name,
        release_notes: release.body,
        tarball_url,
        tarball_name,
        checksums_url: checksums_asset,
    }))
}

/// Verifies tarball bytes against a SHA256 checksums file content.
pub fn parse_checksums_and_verify(
    tarball_name: &str,
    tarball_bytes: &[u8],
    checksums_content: &str,
) -> Result<()> {
    let mut hasher = Sha256::new();
    hasher.update(tarball_bytes);
    let calculated_hash = hex::encode(hasher.finalize());

    let mut expected_hash: Option<String> = None;

    for line in checksums_content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let hash = parts[0].trim().to_lowercase();
            let filename = parts[1].trim().trim_start_matches('*');
            if filename == tarball_name || filename.ends_with(tarball_name) {
                expected_hash = Some(hash);
                break;
            }
        }
    }

    match expected_hash {
        Some(expected) => {
            if calculated_hash.eq_ignore_ascii_case(&expected) {
                info!("SHA-256 verification passed for {}", tarball_name);
                Ok(())
            } else {
                Err(CoreError::Update(format!(
                    "SHA-256 checksum mismatch for {}! Expected: {}, Calculated: {}",
                    tarball_name, expected, calculated_hash
                )))
            }
        }
        None => {
            warn!(
                "Filename {} not found in SHA256SUMS.txt. Allowing download but logging warning.",
                tarball_name
            );
            Ok(())
        }
    }
}

/// Downloads release tarball and SHA256 checksums, validates the hash, and saves to a temp file.
pub async fn download_and_verify(release: &ReleaseInfo) -> Result<PathBuf> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| CoreError::Update(format!("Failed to build HTTP client: {}", e)))?;

    info!("Downloading update package from {}", release.tarball_url);
    let tar_resp = client
        .get(&release.tarball_url)
        .send()
        .await
        .map_err(|e| CoreError::Update(format!("Failed to download tarball: {}", e)))?;

    if !tar_resp.status().is_success() {
        return Err(CoreError::Update(format!(
            "Tarball download failed with status {}",
            tar_resp.status()
        )));
    }

    let tar_bytes = tar_resp
        .bytes()
        .await
        .map_err(|e| CoreError::Update(format!("Failed to read tarball body: {}", e)))?;

    // Download and verify checksum if available
    if let Some(ref checksum_url) = release.checksums_url {
        debug!("Downloading checksums from {}", checksum_url);
        if let Ok(chk_resp) = client.get(checksum_url).send().await {
            if chk_resp.status().is_success() {
                if let Ok(chk_text) = chk_resp.text().await {
                    parse_checksums_and_verify(
                        &release.tarball_name,
                        &tar_bytes,
                        &chk_text,
                    )?;
                }
            }
        }
    }

    // Save to temp file
    let temp_dir = tempfile::Builder::new()
        .prefix("clipboard-history-update-")
        .tempdir()
        .map_err(|e| CoreError::Update(format!("Failed to create temp directory: {}", e)))?;

    let tar_path = temp_dir.path().join(&release.tarball_name);
    fs::write(&tar_path, &tar_bytes)
        .map_err(|e| CoreError::Update(format!("Failed to write downloaded archive: {}", e)))?;

    // Prevent temp_dir from being deleted upon scope exit by keeping path
    let kept_dir = temp_dir.keep();
    let persistent_path = kept_dir.join(&release.tarball_name);
    Ok(persistent_path)
}

/// Unpacks the downloaded archive and updates the binaries in `~/.local/bin/`.
/// Creates backups (.bak) of existing binaries and performs rollback on failure.
pub fn install_update_from_tarball(tarball_path: &Path) -> Result<Vec<String>> {
    let tar_file = File::open(tarball_path)
        .map_err(|e| CoreError::Update(format!("Could not open archive: {}", e)))?;
    let tar_gz = GzDecoder::new(tar_file);
    let mut archive = Archive::new(tar_gz);

    // Determine target bin dir (~/.local/bin)
    let home = std::env::var("HOME").map_err(|_| {
        CoreError::Update("Could not determine HOME directory for binary installation".into())
    })?;
    let bin_dir = PathBuf::from(home).join(".local/bin");
    fs::create_dir_all(&bin_dir)?;

    let target_binaries = [
        "clipboard-history",
        "clipboard-history-daemon",
        "clipboard-history-gui",
    ];

    let mut extracted_bins: Vec<(String, PathBuf)> = Vec::new();

    let temp_extract_dir = tempfile::Builder::new()
        .prefix("ch_extract_")
        .tempdir()
        .map_err(|e| CoreError::Update(format!("Failed to create temp extract dir: {}", e)))?;

    for entry_result in archive.entries().map_err(|e| CoreError::Update(e.to_string()))? {
        let mut entry = entry_result.map_err(|e| CoreError::Update(e.to_string()))?;
        let entry_path = entry.path().map_err(|e| CoreError::Update(e.to_string()))?.to_path_buf();

        if let Some(file_name) = entry_path.file_name().and_then(|f| f.to_str()) {
            if target_binaries.contains(&file_name) {
                let temp_dest = temp_extract_dir.path().join(file_name);
                entry
                    .unpack(&temp_dest)
                    .map_err(|e| CoreError::Update(format!("Failed to unpack {}: {}", file_name, e)))?;
                // Ensure executable permission
                let _ = fs::set_permissions(&temp_dest, Permissions::from_mode(0o755));
                extracted_bins.push((file_name.to_string(), temp_dest));
            }
        }
    }

    if extracted_bins.is_empty() {
        return Err(CoreError::Update(
            "Archive did not contain any valid clipboard-history binary executables".to_string(),
        ));
    }

    // Atomic installation with rollback support
    let mut installed = Vec::new();
    let mut backups: Vec<(PathBuf, PathBuf)> = Vec::new(); // (current_bin, backup_bin)

    let mut install_result: Result<()> = Ok(());

    for (name, temp_file) in &extracted_bins {
        let dest_bin = bin_dir.join(name);
        let backup_bin = bin_dir.join(format!("{}.bak", name));

        // Backup existing binary if present
        if dest_bin.exists() {
            if let Err(e) = fs::copy(&dest_bin, &backup_bin) {
                install_result = Err(CoreError::Update(format!(
                    "Failed to backup {}: {}",
                    name, e
                )));
                break;
            }
            backups.push((dest_bin.clone(), backup_bin.clone()));
        }

        // Write to dest.new and atomic rename
        let new_dest = bin_dir.join(format!("{}.new", name));
        if let Err(e) = fs::copy(temp_file, &new_dest) {
            install_result = Err(CoreError::Update(format!(
                "Failed to copy new binary {}: {}",
                name, e
            )));
            break;
        }
        let _ = fs::set_permissions(&new_dest, Permissions::from_mode(0o755));

        if let Err(e) = fs::rename(&new_dest, &dest_bin) {
            install_result = Err(CoreError::Update(format!(
                "Failed to atomically swap {}: {}",
                name, e
            )));
            break;
        }

        installed.push(name.clone());
    }

    // Handle rollback if installation failed mid-way
    if let Err(err) = install_result {
        error!("Update installation failed: {}. Initiating rollback...", err);
        for (dest, backup) in backups {
            if backup.exists() {
                let _ = fs::rename(&backup, &dest);
            }
        }
        return Err(err);
    }

    // Clean up backups on success
    for (_, backup) in backups {
        let _ = fs::remove_file(backup);
    }

    info!(
        "Successfully updated {} binaries in {}",
        installed.len(),
        bin_dir.display()
    );
    Ok(installed)
}

/// Triggers user systemd service restart for clipboard-history daemon.
pub async fn restart_daemon_service() -> Result<()> {
    info!("Restarting clipboard-history user systemd service...");
    let output = tokio::process::Command::new("systemctl")
        .args(["--user", "restart", "clipboard-history"])
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            info!("clipboard-history service restarted successfully");
            Ok(())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            warn!("systemctl --user restart returned error: {}", stderr);
            // Non-fatal if systemd service is not configured or user is running standalone
            Ok(())
        }
        Err(e) => {
            warn!("Failed to invoke systemctl: {}", e);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("1.2.2", "1.2.3"));
        assert!(is_newer_version("v1.2.2", "v1.3.0"));
        assert!(is_newer_version("1.2.2", "v2.0.0"));
        assert!(!is_newer_version("1.2.2", "1.2.2"));
        assert!(!is_newer_version("1.2.3", "1.2.2"));
        assert!(!is_newer_version("v1.2.3", "1.2.3"));
    }

    #[test]
    fn test_parse_checksums_and_verify() {
        let content = b"dummy archive content for testing";
        let mut hasher = Sha256::new();
        hasher.update(content);
        let hash = hex::encode(hasher.finalize());

        let checksums_file = format!(
            "abc12345 other-file.tar.gz\n{} clipboard-history-1.2.3.tar.gz\n",
            hash
        );

        let res = parse_checksums_and_verify(
            "clipboard-history-1.2.3.tar.gz",
            content,
            &checksums_file,
        );
        assert!(res.is_ok());

        // Tampered content test
        let tampered_content = b"tampered content";
        let err_res = parse_checksums_and_verify(
            "clipboard-history-1.2.3.tar.gz",
            tampered_content,
            &checksums_file,
        );
        assert!(err_res.is_err());
    }
}

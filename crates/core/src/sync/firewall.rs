use crate::ipc::protocol::FirewallCheckResult;
use std::process::Command;

/// Detects the host's active local network subnet (e.g., 192.168.1.0/24).
/// Filters out virtual interfaces (docker, br-, veth, tun, wg) to find the physical LAN.
pub fn detect_local_subnet() -> String {
    // 1. Try reading routes via `ip -4 route show scope link`
    if let Ok(output) = Command::new("ip")
        .args(["-4", "route", "show", "scope", "link"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 && parts[1] == "dev" {
                    let dev = parts[2];
                    if !dev.starts_with("docker")
                        && !dev.starts_with("br-")
                        && !dev.starts_with("veth")
                        && !dev.starts_with("tun")
                        && !dev.starts_with("wg")
                        && !dev.starts_with("virbr")
                    {
                        let cidr = parts[0];
                        if cidr.contains('/')
                            && (cidr.starts_with("192.168.")
                                || cidr.starts_with("10.")
                                || cidr.starts_with("172."))
                        {
                            return cidr.to_string();
                        }
                    }
                }
            }
        }
    }

    // 2. Fallback to standard private Class C / B subnets (RFC 1918)
    "192.168.0.0/16".to_string()
}

/// Checks the host system's firewall status and port rules for Network Sync (54123 UDP/TCP and 54124 TCP)
pub fn check_firewall_status() -> FirewallCheckResult {
    let subnet = detect_local_subnet();

    // 1. Check Firewalld (Fedora / RHEL / CentOS / Arch with firewalld)
    if is_service_active("firewalld") {
        let has_rule = check_firewalld_allowed(54123);

        if has_rule {
            return FirewallCheckResult {
                is_firewall_active: true,
                firewall_name: "Firewalld".to_string(),
                has_issue: false,
                description: format!(
                    "Firewalld is active and sync ports are allowed for local network ({}).",
                    subnet
                ),
                recommended_command: None,
            };
        } else {
            let cmd = format!(
                "sudo firewall-cmd --permanent --add-rich-rule='rule family=\"ipv4\" source address=\"{}\" port port=\"54123\" protocol=\"udp\" accept' && sudo firewall-cmd --permanent --add-rich-rule='rule family=\"ipv4\" source address=\"{}\" port port=\"54123\" protocol=\"tcp\" accept' && sudo firewall-cmd --permanent --add-rich-rule='rule family=\"ipv4\" source address=\"{}\" port port=\"54124\" protocol=\"tcp\" accept' && sudo firewall-cmd --reload",
                subnet, subnet, subnet
            );
            return FirewallCheckResult {
                is_firewall_active: true,
                firewall_name: "Firewalld".to_string(),
                has_issue: true,
                description: format!(
                    "Firewalld is active. Port permissions must be granted for sync to work from your local network ({}).",
                    subnet
                ),
                recommended_command: Some(cmd),
            };
        }
    }

    // 2. Check UFW (Ubuntu / Debian / Linux Mint)
    if is_ufw_active() {
        let (ports_open, is_certain) = check_ufw_ports();
        if is_certain && ports_open {
            return FirewallCheckResult {
                is_firewall_active: true,
                firewall_name: "UFW".to_string(),
                has_issue: false,
                description: format!(
                    "UFW is active and 54123/54124 port rules are configured for local network ({}).",
                    subnet
                ),
                recommended_command: None,
            };
        } else {
            let cmd = format!(
                "sudo ufw allow from {} to any port 54123 && sudo ufw allow from {} to any port 54124 && sudo chmod 644 /etc/ufw/user.rules /etc/ufw/user6.rules 2>/dev/null || true",
                subnet, subnet
            );
            return FirewallCheckResult {
                is_firewall_active: true,
                firewall_name: "UFW".to_string(),
                has_issue: true,
                description: format!(
                    "UFW is active. For security, port permissions for 54123 and 54124 should only be granted to devices on your local network ({}).",
                    subnet
                ),
                recommended_command: Some(cmd),
            };
        }
    } else if std::path::Path::new("/etc/ufw/ufw.conf").exists() || is_service_active("ufw") {
        return FirewallCheckResult {
            is_firewall_active: false,
            firewall_name: "UFW (Inactive / Disabled)".to_string(),
            has_issue: false,
            description:
                "UFW is installed on your system but disabled (Ports are open and accessible)."
                    .to_string(),
            recommended_command: None,
        };
    }

    // 3. Check nftables (Arch / CachyOS / Debian)
    if is_service_active("nftables") {
        let cmd = format!(
            "sudo nft add rule inet filter input ip saddr {} udp dport 54123 accept && sudo nft add rule inet filter input ip saddr {} tcp dport {{ 54123, 54124 }} accept",
            subnet, subnet
        );
        return FirewallCheckResult {
            is_firewall_active: true,
            firewall_name: "nftables".to_string(),
            has_issue: true,
            description: format!(
                "nftables service is active. Ensure sync ports are open for your local network ({}).",
                subnet
            ),
            recommended_command: Some(cmd),
        };
    }

    // 4. Check iptables
    if is_service_active("iptables") {
        let cmd = format!(
            "sudo iptables -A INPUT -s {} -p udp --dport 54123 -j ACCEPT && sudo iptables -A INPUT -s {} -p tcp -m multiport --dports 54123,54124 -j ACCEPT",
            subnet, subnet
        );
        return FirewallCheckResult {
            is_firewall_active: true,
            firewall_name: "iptables".to_string(),
            has_issue: true,
            description: format!(
                "iptables service is active. Sync ports must be allowed for your local network ({}).",
                subnet
            ),
            recommended_command: Some(cmd),
        };
    }

    // 5. No active firewall service detected
    FirewallCheckResult {
        is_firewall_active: false,
        firewall_name: "None / Inactive".to_string(),
        has_issue: false,
        description:
            "No active firewall restriction detected on your system (Ports are unblocked)."
                .to_string(),
        recommended_command: None,
    }
}

fn is_service_active(service: &str) -> bool {
    Command::new("systemctl")
        .args(["is-active", "--quiet", service])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn check_firewalld_allowed(port: u16) -> bool {
    // 1. Direct query-port check
    let port_str = port.to_string();
    let udp_ok = query_firewalld_port(port, "udp");
    let tcp_ok = query_firewalld_port(port, "tcp");
    if udp_ok && tcp_ok {
        return true;
    }

    // 2. Rich rules or zone list inspection
    if let Ok(output) = Command::new("firewall-cmd").arg("--list-all").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.contains(&port_str) || text.contains("54124") {
                return true;
            }
        }
    }
    false
}

fn query_firewalld_port(port: u16, proto: &str) -> bool {
    let port_arg = format!("{}/{}", port, proto);
    Command::new("firewall-cmd")
        .args(["--query-port", &port_arg])
        .output()
        .map(|o| {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout).trim() == "yes"
            } else {
                false
            }
        })
        .unwrap_or(false)
}

fn is_ufw_active() -> bool {
    // 1. On Debian/Ubuntu systems, ufw.service runs on boot and remains in 'active (exited)' state
    // due to RemainAfterExit=yes. However, ufw-init immediately exits without applying any iptables
    // rules if ENABLED=no in /etc/ufw/ufw.conf.
    // Therefore, systemctl is-active ufw returns true even when UFW is completely inactive!
    if let Ok(content) = std::fs::read_to_string("/etc/ufw/ufw.conf") {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(val) = trimmed.strip_prefix("ENABLED=") {
                let v = val
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim()
                    .to_lowercase();
                return v == "yes" || v == "true";
            }
        }
        return false;
    }

    // 2. If ufw.conf cannot be read or doesn't exist, check systemd and try passwordless sudo ufw status
    if is_service_active("ufw") {
        if let Ok(output) = Command::new("sudo").args(["-n", "ufw", "status"]).output() {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                return text.contains("Status: active");
            }
        }
    }

    false
}

fn check_ufw_ports() -> (bool, bool) {
    // 1. Check if /etc/ufw/user.rules can be read directly
    if let Ok(rules) = std::fs::read_to_string("/etc/ufw/user.rules") {
        let has_rule = rules.contains("54123");
        return (has_rule, true);
    }
    // 2. Try running ufw status if passwordless sudo is configured
    if let Ok(output) = Command::new("sudo").args(["-n", "ufw", "status"]).output() {
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout);
            let has_rule = out_str.contains("54123") && out_str.contains("ALLOW");
            return (has_rule, true);
        }
    }
    (false, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_local_subnet_format() {
        let subnet = detect_local_subnet();
        assert!(subnet.contains('/'));
        assert!(
            subnet.starts_with("192.168.")
                || subnet.starts_with("10.")
                || subnet.starts_with("172.")
        );
    }

    #[test]
    fn test_check_firewall_status_runs_without_panicking() {
        let result = check_firewall_status();
        assert!(!result.firewall_name.is_empty());
        assert!(!result.description.is_empty());
        if result.has_issue {
            assert!(result.recommended_command.is_some());
            let cmd = result.recommended_command.unwrap();
            assert!(cmd.contains("54123"));
        }
    }

    #[test]
    fn test_ufw_detection_logic() {
        let active = is_ufw_active();
        if let Ok(content) = std::fs::read_to_string("/etc/ufw/ufw.conf") {
            if content.contains("ENABLED=no") {
                assert!(!active, "UFW should not be reported active when ENABLED=no");
            }
        }
    }
}

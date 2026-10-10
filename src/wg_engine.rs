use base64::{engine::general_purpose::STANDARD, Engine as _};
use qrcode::render::svg;
use qrcode::QrCode;
use std::io::Write;
use std::process::{Command, Stdio};

fn safe_fill_random_bytes(dest: &mut [u8]) {
    use std::fs::File;
    use std::io::Read;
    
    if let Ok(mut f) = File::open("/dev/urandom") {
        if f.read_exact(dest).is_ok() {
            return;
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    
    let mut state = (now ^ 0xDEADBEEFCAFEBABE) as u64;
    for slot in dest.iter_mut() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *slot = (state >> 33) as u8;
    }
}

/// Generate WireGuard private/public keypair using native `wg genkey` and `wg pubkey`
pub fn generate_keypair() -> (String, String) {
    if let Ok(output) = Command::new("wg").arg("genkey").output() {
        if output.status.success() {
            let private_key = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(public_key) = derive_public_key(&private_key) {
                return (private_key, public_key);
            }
        }
    }

    // Fallback if `wg` CLI is not in PATH
    let mut private_bytes = [0u8; 32];
    safe_fill_random_bytes(&mut private_bytes);
    private_bytes[0] &= 248;
    private_bytes[31] &= 127;
    private_bytes[31] |= 64;
    let private_key = STANDARD.encode(private_bytes);
    let public_key = derive_public_key(&private_key).unwrap_or_else(|| STANDARD.encode([0u8; 32]));
    (private_key, public_key)
}

/// Derive WireGuard Public Key from Base64 Private Key using native `wg pubkey`
pub fn derive_public_key(private_key_b64: &str) -> Option<String> {
    let clean_priv = private_key_b64.trim();
    if clean_priv.is_empty() {
        return None;
    }

    if let Ok(mut child) = Command::new("wg")
        .arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(clean_priv.as_bytes());
            let _ = stdin.write_all(b"\n");
        }
        if let Ok(output) = child.wait_with_output() {
            if output.status.success() {
                let pubkey = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !pubkey.is_empty() {
                    return Some(pubkey);
                }
            }
        }
    }

    None
}

/// Generate WireGuard Pre-Shared Key using native `wg genpsk`
pub fn generate_preshared_key() -> String {
    if let Ok(output) = Command::new("wg").arg("genpsk").output() {
        if output.status.success() {
            let psk = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !psk.is_empty() {
                return psk;
            }
        }
    }

    let mut psk_bytes = [0u8; 32];
    safe_fill_random_bytes(&mut psk_bytes);
    STANDARD.encode(psk_bytes)
}

/// Allocate next available IP in CIDR pool (e.g. 10.8.0.1/24 -> 10.8.0.2/32, 10.8.0.3/32)
pub fn allocate_next_ip(address_pool: &str, existing_ips: &[String]) -> String {
    let base_ip = address_pool
        .split('/')
        .next()
        .unwrap_or("10.8.0.1")
        .trim();
    let parts: Vec<&str> = base_ip.split('.').collect();

    if parts.len() == 4 {
        let prefix = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
        let mut used_octets: Vec<u8> = Vec::new();

        // Extract used last octets from all comma-separated allowed_ips lists
        for ip_entry in existing_ips {
            for item in ip_entry.split(',') {
                let clean_item = item.trim();
                let ip_only = clean_item.split('/').next().unwrap_or("").trim();
                if ip_only.starts_with(&prefix) {
                    if let Some(last) = ip_only.split('.').last() {
                        if let Ok(octet) = last.parse::<u8>() {
                            used_octets.push(octet);
                        }
                    }
                }
            }
        }

        // Reserve .1 host
        used_octets.push(1);
        if let Ok(server_octet) = parts[3].parse::<u8>() {
            used_octets.push(server_octet);
        }

        for candidate in 2..254 {
            if !used_octets.contains(&candidate) {
                return format!("{}.{}/32", prefix, candidate);
            }
        }
    }

    // Fallback default
    format!("{}.{}/32", base_ip, rand::random::<u8>())
}

/// Allocate the next available IPv6 address from pool (e.g. fd00:90::1/64 -> fd00:90::2/128)
pub fn allocate_next_ipv6(pool_v6: &str, existing_ips: &[String]) -> String {
    let prefix = if let Some(slash_idx) = pool_v6.find('/') {
        &pool_v6[..slash_idx]
    } else {
        pool_v6
    };

    let base_prefix = if prefix.ends_with("::1") {
        &prefix[..prefix.len() - 1]
    } else if prefix.ends_with("::") {
        prefix
    } else if let Some(last_colon) = prefix.rfind(':') {
        &prefix[..=last_colon]
    } else {
        "fd00:90::"
    };

    let mut used_suffixes = Vec::new();
    for ip_str in existing_ips {
        for part in ip_str.split(',') {
            let part = part.trim();
            if part.contains(':') {
                let clean = part.split('/').next().unwrap_or(part);
                if let Some(suffix) = clean.strip_prefix(base_prefix) {
                    if let Ok(num) = u32::from_str_radix(suffix, 16) {
                        used_suffixes.push(num);
                    }
                }
            }
        }
    }

    used_suffixes.push(1); // Reserve ::1 for server

    for candidate in 2..65535 {
        if !used_suffixes.contains(&candidate) {
            return format!("{}{:x}/128", base_prefix, candidate);
        }
    }

    format!("{}2/128", base_prefix)
}

/// Generate WireGuard client configuration (.conf)
pub fn build_client_config(
    client_private_key: &str,
    client_ip: &str,
    server_public_key: &str,
    server_endpoint: &str,
    dns_servers: &str,
    preshared_key: Option<&str>,
    persistent_keepalive: u16,
    mtu: Option<u32>,
) -> String {
    let mut conf = String::new();
    conf.push_str("[Interface]\n");
    conf.push_str(&format!("PrivateKey = {}\n", client_private_key));
    conf.push_str(&format!("Address = {}\n", client_ip));

    if let Some(m) = mtu {
        if m > 0 && m != 1500 {
            conf.push_str(&format!("MTU = {}\n", m));
        }
    }

    if !dns_servers.trim().is_empty() {
        conf.push_str(&format!("DNS = {}\n", dns_servers));
    }

    conf.push_str("\n[Peer]\n");
    conf.push_str(&format!("PublicKey = {}\n", server_public_key));

    if let Some(psk) = preshared_key {
        if !psk.is_empty() {
            conf.push_str(&format!("PresharedKey = {}\n", psk));
        }
    }

    conf.push_str(&format!("Endpoint = {}\n", server_endpoint));
    conf.push_str("AllowedIPs = 0.0.0.0/0, ::/0\n");

    if persistent_keepalive > 0 {
        conf.push_str(&format!(
            "PersistentKeepalive = {}\n",
            persistent_keepalive
        ));
    }

    conf
}

/// Generate WireGuard server configuration (.conf)
pub fn build_server_config(
    iface: &crate::models::WgInterface,
    peers: &[crate::models::WgPeer],
) -> String {
    let mut conf = String::new();
    conf.push_str(&format!("# WireGuard Interface {} Managed by Wireguard-UI\n", iface.name));
    conf.push_str("[Interface]\n");
    conf.push_str(&format!("PrivateKey = {}\n", iface.private_key));

    let address_line = if let Some(ref v6) = iface.address_pool_v6 {
        if !v6.trim().is_empty() {
            format!("{}, {}", iface.address_pool, v6.trim())
        } else {
            iface.address_pool.clone()
        }
    } else {
        iface.address_pool.clone()
    };
    conf.push_str(&format!("Address = {}\n", address_line));
    conf.push_str(&format!("ListenPort = {}\n", iface.listen_port));

    if let Some(m) = iface.mtu {
        if m > 0 && m != 1500 {
            conf.push_str(&format!("MTU = {}\n", m));
        }
    }

    let mut post_up = iface.post_up.clone();
    let mut post_down = iface.post_down.clone();

    if iface.mss_clamping {
        let mss_rule_up = format!("iptables -t mangle -A POSTROUTING -p tcp --tcp-flags SYN,RST SYN -o {} -j TCPMSS --clamp-mss-to-pmtu", iface.id);
        let mss_rule_down = format!("iptables -t mangle -D POSTROUTING -p tcp --tcp-flags SYN,RST SYN -o {} -j TCPMSS --clamp-mss-to-pmtu", iface.id);
        if !post_up.contains("TCPMSS") {
            if !post_up.is_empty() { post_up.push_str("; "); }
            post_up.push_str(&mss_rule_up);
        }
        if !post_down.contains("TCPMSS") {
            if !post_down.is_empty() { post_down.push_str("; "); }
            post_down.push_str(&mss_rule_down);
        }
    }

    if !post_up.is_empty() {
        conf.push_str(&format!("PostUp = {}\n", post_up));
    }
    if !post_down.is_empty() {
        conf.push_str(&format!("PostDown = {}\n", post_down));
    }

    for peer in peers {
        if !peer.enabled {
            continue;
        }

        conf.push_str(&format!("\n# Peer: {} ({})\n", peer.name, peer.peer_type));
        conf.push_str("[Peer]\n");
        conf.push_str(&format!("PublicKey = {}\n", peer.public_key));

        if let Some(psk) = &peer.preshared_key {
            if !psk.is_empty() {
                conf.push_str(&format!("PresharedKey = {}\n", psk));
            }
        }

        conf.push_str(&format!("AllowedIPs = {}\n", peer.allowed_ips));

        if let Some(endpoint) = &peer.endpoint {
            if !endpoint.is_empty() {
                conf.push_str(&format!("Endpoint = {}\n", endpoint));
            }
        }

        if peer.persistent_keepalive > 0 {
            conf.push_str(&format!(
                "PersistentKeepalive = {}\n",
                peer.persistent_keepalive
            ));
        }
    }

    conf
}

pub fn get_remote_probe_command(next_ip: &str) -> String {
    format!(
        "(umask 077 && mkdir -p /etc/wireguard && chmod 700 /etc/wireguard && ([ -f /etc/wireguard/private.key ] || wg genkey > /etc/wireguard/private.key); wg pubkey < /etc/wireguard/private.key > /etc/wireguard/public.key && chmod 600 /etc/wireguard/*.key); echo -e \"\\n=== REMOTE HOST INFO ===\" && echo \"PUBLIC_KEY:  $(cat /etc/wireguard/public.key)\" && echo \"ENDPOINT:    $(curl -s --connect-timeout 3 https://api.ipify.org || curl -s --connect-timeout 3 ifconfig.me || echo 'YOUR_PUBLIC_IP'):51820\" && echo \"SUBNETS:     {},$(ip -o route | awk '$1 ~ /\\// && $1 !~ /^default|^169\\.254|^127\\./ {{print $1}}' | head -n 3 | tr '\\n' ',' | sed 's/,$//')\"",
        next_ip
    )
}

pub fn get_hub_endpoint(iface: &crate::models::WgInterface) -> String {
    if let Some(ref ep) = iface.gateway_endpoint {
        if !ep.trim().is_empty() {
            return if ep.contains(':') {
                ep.trim().to_string()
            } else {
                format!("{}:{}", ep.trim(), iface.listen_port)
            };
        }
    }
    if let Ok(host) = std::env::var("SSH_HOST") {
        return format!("{}:{}", host, iface.listen_port);
    }
    format!("10.1.100.90:{}", iface.listen_port)
}

pub fn build_site_config(
    peer: &crate::models::WgPeer,
    iface: &crate::models::WgInterface,
) -> String {
    let hub_endpoint = get_hub_endpoint(iface);

    let peer_address: String = peer
        .allowed_ips
        .split(',')
        .map(|s| s.trim())
        .find(|s| s.ends_with("/32") || s.ends_with("/128"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let first = peer.allowed_ips.split(',').next().unwrap_or("10.100.90.50/32").trim();
            if first.ends_with(".0/24") {
                first.replace(".0/24", ".1/32")
            } else if !first.contains('/') {
                format!("{}/32", first)
            } else {
                first.to_string()
            }
        });

    let mut allowed_ips_list: Vec<String> = vec![iface.address_pool.clone()];
    if let Some(ref v6) = iface.address_pool_v6 {
        if !v6.trim().is_empty() {
            allowed_ips_list.push(v6.trim().to_string());
        }
    }
    if let Some(ref subnets) = iface.hub_subnets {
        for s in subnets.split(',') {
            let s_trim = s.trim();
            if !s_trim.is_empty() && !allowed_ips_list.contains(&s_trim.to_string()) {
                allowed_ips_list.push(s_trim.to_string());
            }
        }
    }
    let hub_allowed_ips = allowed_ips_list.join(", ");

    let mut conf = String::new();
    conf.push_str(&format!("# WireGuard Site-to-Site Gateway Config: {}\n", peer.name));
    conf.push_str("# Save to /etc/wireguard/wg0.conf on Remote Gateway Host\n\n");
    conf.push_str("[Interface]\n");
    conf.push_str("# Remote Host Private Key:\n");
    conf.push_str("PrivateKey = <PASTE_REMOTE_PRIVATE_KEY_HERE_OR_AUTOGENERATED>\n");
    conf.push_str(&format!("Address = {}\n", peer_address));
    conf.push_str("ListenPort = 51820\n");

    if let Some(m) = iface.mtu {
        if m > 0 && m != 1500 {
            conf.push_str(&format!("MTU = {}\n", m));
        }
    }

    conf.push_str("PostUp = iptables -A FORWARD -i wg0 -j ACCEPT; iptables -t nat -A POSTROUTING -o WGUI_IFACE -j MASQUERADE\n");
    conf.push_str("PostDown = iptables -D FORWARD -i wg0 -j ACCEPT; iptables -t nat -D POSTROUTING -o WGUI_IFACE -j MASQUERADE\n\n");
    conf.push_str("# Central WireGuard Hub Gateway Node\n");
    conf.push_str("[Peer]\n");
    conf.push_str(&format!("PublicKey = {}\n", iface.public_key));
    conf.push_str(&format!("Endpoint = {}\n", hub_endpoint));
    conf.push_str(&format!("AllowedIPs = {}\n", hub_allowed_ips));
    conf.push_str(&format!("PersistentKeepalive = {}\n", peer.persistent_keepalive));

    conf
}

pub fn apply_wireguard_config(iface: &crate::models::WgInterface, peers: &[crate::models::WgPeer], force_restart: bool) {
    let conf = build_server_config(iface, peers);

    let conf_path = format!("/etc/wireguard/{}.conf", iface.id);
    if let Err(e) = std::fs::write(&conf_path, &conf) {
        tracing::warn!("Could not write {}: {}", conf_path, e);
    } else {
        tracing::info!("Updated {}", conf_path);
    }

    if is_ssh_sync_enabled() {
        let ssh_target = get_ssh_target();
        let proxmox_cmd = if force_restart {
            format!(
                "cat << 'EOF' > /etc/wireguard/{}.conf\n{}\nEOF\nwg-quick down {} 2>/dev/null; wg-quick up {} 2>/dev/null",
                iface.id,
                conf,
                iface.id,
                iface.id,
            )
        } else {
            format!(
                "cat << 'EOF' > /etc/wireguard/{}.conf\n{}\nEOF\nwg-quick strip {} > /tmp/{}_strip.conf 2>/dev/null && wg syncconf {} /tmp/{}_strip.conf 2>/dev/null || (wg-quick down {} 2>/dev/null; wg-quick up {} 2>/dev/null) 2>/dev/null",
                &iface.id,
                &conf,
                &iface.id,
                &iface.id,
                &iface.id,
                &iface.id,
                &iface.id,
                &iface.id,
            )
        };
        match std::process::Command::new("ssh")
            .args(ssh_cmd_args(&ssh_target, &proxmox_cmd))
            .output()
        {
            Ok(out) => {
                tracing::info!(
                    "SSH sync to {} status: {}, stdout: {}, stderr: {}",
                    ssh_target,
                    out.status,
                    String::from_utf8_lossy(&out.stdout).trim(),
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Err(e) => {
                tracing::warn!("Failed to execute SSH sync to {}: {}", ssh_target, e);
            }
        }
    } else {
        let local_cmd = if force_restart {
            format!("wg-quick down {} 2>/dev/null; wg-quick up {} 2>/dev/null", iface.id, iface.id)
        } else {
            format!(
                "wg-quick strip {} > /tmp/{}_strip.conf 2>/dev/null && wg syncconf {} /tmp/{}_strip.conf 2>/dev/null || (wg-quick down {} 2>/dev/null; wg-quick up {} 2>/dev/null) 2>/dev/null || true",
                iface.id,
                iface.id,
                iface.id,
                iface.id,
                iface.id,
                iface.id,
            )
        };
        let _ = std::process::Command::new("sh")
            .args(["-c", &local_cmd])
            .output();
    }
}

pub fn detect_hub_subnets() -> Vec<String> {
    let route_cmd = "ip -o route | awk '$1 ~ /\\// && $1 !~ /^default|^169\\.254|^127\\.|^172\\.(1[6-9]|2[0-9]|3[0-1])\\./ && $0 !~ /dev docker0|dev br-/ {print $1}'";
    let output = if is_ssh_sync_enabled() {
        let target = get_ssh_target();
        std::process::Command::new("ssh")
            .args(ssh_cmd_args(&target, route_cmd))
            .output()
    } else {
        std::process::Command::new("sh")
            .arg("-c")
            .arg(route_cmd)
            .output()
    };

    let mut subnets = Vec::new();
    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let item = line.trim();
                if !item.is_empty() && item.contains('/') && !subnets.contains(&item.to_string()) {
                    subnets.push(item.to_string());
                }
            }
        }
    }
    subnets
}

pub fn is_ssh_sync_enabled() -> bool {
    if let Ok(v) = std::env::var("ENABLE_SSH_SYNC") {
        return matches!(v.to_lowercase().trim(), "true" | "1" | "yes" | "enabled");
    }
    std::env::var("SSH_HOST").is_ok()
}

pub fn get_ssh_target() -> String {
    let user = std::env::var("SSH_USER").unwrap_or_else(|_| "root".into());
    let host = std::env::var("SSH_HOST").unwrap_or_else(|_| "192.168.1.101".into());
    format!("{}@{}", user, host)
}

/// Standardized OpenSSH multiplexing arguments (ControlMaster / ControlPersist)
/// Enables sub-10ms SSH command execution over persistent UNIX control socket.
pub fn ssh_cmd_args(target: &str, remote_cmd: &str) -> Vec<String> {
    vec![
        "-o".into(), "StrictHostKeyChecking=no".into(),
        "-o".into(), "ControlMaster=auto".into(),
        "-o".into(), "ControlPath=/tmp/ssh_mux_%h_%p_%r".into(),
        "-o".into(), "ControlPersist=10m".into(),
        "-o".into(), "ConnectTimeout=3".into(),
        target.to_string(),
        remote_cmd.to_string(),
    ]
}

/// Generate SVG Data URI for QR Code scanning
pub fn generate_qr_svg_data_uri(content: &str) -> Result<String, String> {
    let code = QrCode::new(content.as_bytes()).map_err(|e| format!("QR Code Error: {}", e))?;
    let image = code
        .render::<svg::Color>()
        .min_dimensions(300, 300)
        .dark_color(svg::Color("#0f172a"))
        .light_color(svg::Color("#ffffff"))
        .build();

    let encoded_svg = STANDARD.encode(image.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{}", encoded_svg))
}

pub struct ParsedWgConf {
    pub interface: crate::models::WgInterface,
    pub peers: Vec<crate::models::WgPeer>,
}

/// Parse an existing /etc/wireguard/wg0.conf file into interface and peer models
pub fn parse_existing_wg_conf(conf_str: &str) -> Option<ParsedWgConf> {
    let mut priv_key = String::new();
    let mut address_pool = "10.100.90.2/24".to_string();
    let mut listen_port: u16 = 51821;
    let mut post_up = String::new();
    let mut post_down = String::new();
    let mut dns = "1.1.1.1, 8.8.8.8".to_string();

    let mut peers = Vec::new();
    let mut in_interface = false;
    let mut in_peer = false;
    let mut current_comment = String::new();

    let mut peer_pubkey = String::new();
    let mut peer_psk = Option::<String>::None;
    let mut peer_allowed_ips = String::new();
    let mut peer_endpoint = Option::<String>::None;
    let mut peer_keepalive: u16 = 25;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for line in conf_str.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('#') {
            let comment_text = trimmed.trim_start_matches('#').trim();
            if !comment_text.is_empty() && !comment_text.starts_with("WireGuard") {
                current_comment = comment_text.to_string();
            }
            continue;
        }

        if trimmed.eq_ignore_ascii_case("[Interface]") {
            in_interface = true;
            in_peer = false;
            continue;
        }

        if trimmed.eq_ignore_ascii_case("[Peer]") {
            if in_peer && !peer_pubkey.is_empty() {
                // Save previous peer
                let name = if !current_comment.is_empty() {
                    let n = current_comment.clone();
                    current_comment.clear();
                    n
                } else {
                    format!("Peer-{}", &peer_pubkey[..6.min(peer_pubkey.len())])
                };

                let p_type = if peer_endpoint.is_some() || peer_allowed_ips.contains("/24") || peer_allowed_ips.contains("/22") {
                    crate::models::PeerType::SiteToSite
                } else {
                    crate::models::PeerType::Client
                };

                peers.push(crate::models::WgPeer {
                    id: format!("peer-{}", rand::random::<u32>()),
                    interface_id: "wg0".to_string(),
                    name,
                    peer_type: p_type,
                    public_key: peer_pubkey.clone(),
                    private_key: None,
                    preshared_key: peer_psk.clone(),
                    allowed_ips: peer_allowed_ips.clone(),
                    endpoint: peer_endpoint.clone(),
                    persistent_keepalive: peer_keepalive,
                    enabled: true,
                    notes: None,
                    created_at: now,
                    last_handshake: None,
                    bytes_rcvd: 0,
                    bytes_sent: 0,
                    rx_speed_bps: 0,
                    tx_speed_bps: 0,
                    ping_latency_ms: None,
                });
            }

            in_interface = false;
            in_peer = true;
            peer_pubkey.clear();
            peer_psk = None;
            peer_allowed_ips.clear();
            peer_endpoint = None;
            peer_keepalive = 25;
            continue;
        }

        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim();
            let val = val.trim();

            if in_interface {
                match key.to_lowercase().as_str() {
                    "privatekey" => priv_key = val.to_string(),
                    "address" => address_pool = val.to_string(),
                    "listenport" => listen_port = val.parse().unwrap_or(51821),
                    "dns" => dns = val.to_string(),
                    "postup" => post_up = val.to_string(),
                    "postdown" => post_down = val.to_string(),
                    _ => {}
                }
            } else if in_peer {
                match key.to_lowercase().as_str() {
                    "publickey" => peer_pubkey = val.to_string(),
                    "presharedkey" => peer_psk = Some(val.to_string()),
                    "allowedips" => peer_allowed_ips = val.to_string(),
                    "endpoint" => peer_endpoint = Some(val.to_string()),
                    "persistentkeepalive" => peer_keepalive = val.parse().unwrap_or(25),
                    _ => {}
                }
            }
        }
    }

    if in_peer && !peer_pubkey.is_empty() {
        let name = if !current_comment.is_empty() {
            current_comment
        } else {
            format!("Peer-{}", &peer_pubkey[..6.min(peer_pubkey.len())])
        };

        let p_type = if peer_endpoint.is_some() || peer_allowed_ips.contains("/24") || peer_allowed_ips.contains("/22") {
            crate::models::PeerType::SiteToSite
        } else {
            crate::models::PeerType::Client
        };

        peers.push(crate::models::WgPeer {
            id: format!("peer-{}", rand::random::<u32>()),
            interface_id: "wg0".to_string(),
            name,
            peer_type: p_type,
            public_key: peer_pubkey,
            private_key: None,
            preshared_key: peer_psk,
            allowed_ips: peer_allowed_ips,
            endpoint: peer_endpoint,
            persistent_keepalive: peer_keepalive,
            enabled: true,
            notes: None,
            created_at: now,
            last_handshake: None,
            bytes_rcvd: 0,
            bytes_sent: 0,
            rx_speed_bps: 0,
            tx_speed_bps: 0,
            ping_latency_ms: None,
        });
    }

    if priv_key.is_empty() {
        return None;
    }

    // Generate public key pseudo representation for interface
    let (_, pub_key) = generate_keypair();

    let iface = crate::models::WgInterface {
        id: "wg0".to_string(),
        name: "wg0".to_string(),
        private_key: priv_key,
        public_key: pub_key,
        listen_port,
        address_pool,
        dns,
        post_up,
        post_down,
        webhook_url: None,
        webhook_token: None,
        gateway_endpoint: None,
        hub_subnets: None,
        mtu: Some(1420),
        mss_clamping: true,
        address_pool_v6: None,
        dns_v6: None,
        enabled: true,
        created_at: now,
    };

    Some(ParsedWgConf { interface: iface, peers })
}

use std::collections::HashMap;
use std::sync::Mutex;

static PREV_STATS: Mutex<Option<HashMap<String, (u64, u64, std::time::Instant)>>> = Mutex::new(None);
static PING_CACHE: Mutex<Option<HashMap<String, (Option<u32>, std::time::Instant)>>> = Mutex::new(None);
static LIVE_STATS_CACHE: Mutex<Option<HashMap<String, LivePeerStat>>> = Mutex::new(None);

#[derive(Clone, Debug)]
pub struct LivePeerStat {
    #[allow(dead_code)]
    pub public_key: String,
    pub endpoint: Option<String>,
    pub last_handshake: Option<i64>,
    pub bytes_rcvd: u64,
    pub bytes_sent: u64,
    pub rx_speed_bps: u64,
    pub tx_speed_bps: u64,
    pub ping_latency_ms: Option<u32>,
}

pub fn ping_peer_ip(ip_subnet: &str) -> Option<u32> {
    let raw_ip = ip_subnet.split(',').next()?.trim();
    let clean_ip = raw_ip.split('/').next()?.trim();
    if clean_ip.is_empty() {
        return None;
    }

    let now = std::time::Instant::now();
    {
        let mut guard = PING_CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some((cached_val, cached_time)) = cache.get(clean_ip) {
            if now.duration_since(*cached_time).as_secs() < 15 {
                return *cached_val;
            }
        }
    }

    let is_ssh = is_ssh_sync_enabled();
    let ssh_target = get_ssh_target();

    let output = if is_ssh {
        std::process::Command::new("ssh")
            .args(ssh_cmd_args(&ssh_target, &format!("ping -c 1 -W 1 {}", clean_ip)))
            .output()
            .ok()?
    } else {
        std::process::Command::new("ping")
            .args(["-c", "1", "-W", "1", clean_ip])
            .output()
            .ok()?
    };

    let result = if !output.status.success() {
        None
    } else {
        let out_str = String::from_utf8_lossy(&output.stdout);
        if let Some(pos) = out_str.find("time=") {
            let after = &out_str[pos + 5..];
            let num_str = after.split_whitespace().next()?;
            num_str.parse::<f64>().ok().map(|ms| ms.round() as u32)
        } else if let Some(pos) = out_str.find("rtt min/avg/max") {
            let after = &out_str[pos..];
            if let Some(eq_pos) = after.find('=') {
                let values = &after[eq_pos + 1..];
                let parts: Vec<&str> = values.trim().split('/').collect();
                if parts.len() >= 2 {
                    parts[1].trim().parse::<f64>().ok().map(|avg| avg.round() as u32)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    {
        let mut guard = PING_CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert(clean_ip.to_string(), (result, now));
    }

    result
}

/// Instant retrieval of live WireGuard statistics directly from in-memory cache (0ms latency).
/// If cache is empty upon startup, automatically triggers a synchronous refresh.
pub fn fetch_live_wireguard_stats() -> HashMap<String, LivePeerStat> {
    if let Ok(guard) = LIVE_STATS_CACHE.lock() {
        if let Some(ref cached) = *guard {
            return cached.clone();
        }
    }
    refresh_live_wireguard_stats()
}

/// Query live WireGuard transfer stats, calculate real-time speeds (B/s), and update memory cache.
pub fn refresh_live_wireguard_stats() -> HashMap<String, LivePeerStat> {
    let mut results = HashMap::new();

    // Query wg show all dump via SSH if SSH sync is enabled, else query local wg
    let dump_output = if is_ssh_sync_enabled() {
        let ssh_target = get_ssh_target();
        std::process::Command::new("ssh")
            .args(ssh_cmd_args(&ssh_target, "wg show all dump"))
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() && !out.stdout.is_empty() {
                    Some(String::from_utf8_lossy(&out.stdout).to_string())
                } else {
                    None
                }
            })
            .or_else(|| {
                std::process::Command::new("wg")
                    .args(["show", "all", "dump"])
                    .output()
                    .ok()
                    .and_then(|out| {
                        if out.status.success() && !out.stdout.is_empty() {
                            Some(String::from_utf8_lossy(&out.stdout).to_string())
                        } else {
                            None
                        }
                    })
            })
    } else {
        std::process::Command::new("wg")
            .args(["show", "all", "dump"])
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() && !out.stdout.is_empty() {
                    Some(String::from_utf8_lossy(&out.stdout).to_string())
                } else {
                    None
                }
            })
    };

    let conf_str = match dump_output {
        Some(s) => s,
        None => return results,
    };

    let now_instant = std::time::Instant::now();
    let mut prev_map_guard = PREV_STATS.lock().unwrap();
    let prev_map = prev_map_guard.get_or_insert_with(HashMap::new);

    for line in conf_str.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        let (pubkey, endpoint, allowed_ips, hs_ts, rx, tx) = if parts.len() >= 9 {
            // "wg show all dump" format (interface prefix)
            let pubkey = parts[1].to_string();
            let endpoint = if parts[3] != "(none)" && !parts[3].is_empty() { Some(parts[3].to_string()) } else { None };
            let allowed_ips = parts[4].to_string();
            let hs_ts: i64 = parts[5].parse().unwrap_or(0);
            let rx: u64 = parts[6].parse().unwrap_or(0);
            let tx: u64 = parts[7].parse().unwrap_or(0);
            (pubkey, endpoint, allowed_ips, hs_ts, rx, tx)
        } else if parts.len() >= 8 {
            // "wg show <iface> dump" format (no interface prefix)
            let pubkey = parts[0].to_string();
            let endpoint = if parts[2] != "(none)" && !parts[2].is_empty() { Some(parts[2].to_string()) } else { None };
            let allowed_ips = parts[3].to_string();
            let hs_ts: i64 = parts[4].parse().unwrap_or(0);
            let rx: u64 = parts[5].parse().unwrap_or(0);
            let tx: u64 = parts[6].parse().unwrap_or(0);
            (pubkey, endpoint, allowed_ips, hs_ts, rx, tx)
        } else {
            continue;
        };

        let last_handshake = if hs_ts > 0 { Some(hs_ts) } else { None };
        let ping_latency_ms = if last_handshake.is_some() {
            ping_peer_ip(&allowed_ips)
        } else {
            None
        };

        let (rx_speed, tx_speed) = if let Some((prev_rx, prev_tx, prev_inst)) = prev_map.get(&pubkey) {
            let elapsed_secs = now_instant.duration_since(*prev_inst).as_secs_f64();
            if elapsed_secs > 0.1 && rx >= *prev_rx && tx >= *prev_tx {
                let rx_sp = ((rx - prev_rx) as f64 / elapsed_secs) as u64;
                let tx_sp = ((tx - prev_tx) as f64 / elapsed_secs) as u64;
                (rx_sp, tx_sp)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        prev_map.insert(pubkey.clone(), (rx, tx, now_instant));

        results.insert(pubkey.clone(), LivePeerStat {
            public_key: pubkey,
            endpoint,
            last_handshake,
            bytes_rcvd: rx,
            bytes_sent: tx,
            rx_speed_bps: rx_speed,
            tx_speed_bps: tx_speed,
            ping_latency_ms,
        });
    }

    // Save snapshot to memory cache
    if let Ok(mut guard) = LIVE_STATS_CACHE.lock() {
        *guard = Some(results.clone());
    }

    results
}

/// Start high-performance asynchronous background telemetry loop (2s poll, 60s bandwidth sample)
pub fn start_background_telemetry_loop(store: crate::store::DbStore) {
    tokio::spawn(async move {
        // Initial warmup
        let _ = tokio::task::spawn_blocking(refresh_live_wireguard_stats).await;

        let mut poll_interval = tokio::time::interval(std::time::Duration::from_secs(2));
        let mut sample_counter: u32 = 0;

        loop {
            poll_interval.tick().await;

            // 1. Refresh live telemetry in background thread without blocking async I/O
            let live_stats = tokio::task::spawn_blocking(refresh_live_wireguard_stats)
                .await
                .unwrap_or_default();

            // 2. Continuous online/offline transition checking and webhook alerts
            if let Ok(interfaces) = store.get_interfaces() {
                for iface in interfaces {
                    if let Ok(mut peers) = store.get_peers_by_interface(&iface.id) {
                        for peer in peers.iter_mut() {
                            if let Some(stat) = live_stats.get(&peer.public_key) {
                                peer.bytes_rcvd = stat.bytes_rcvd;
                                peer.bytes_sent = stat.bytes_sent;
                                peer.rx_speed_bps = stat.rx_speed_bps;
                                peer.tx_speed_bps = stat.tx_speed_bps;
                                peer.last_handshake = stat.last_handshake;
                                peer.ping_latency_ms = stat.ping_latency_ms;
                                if stat.endpoint.is_some() {
                                    peer.endpoint = stat.endpoint.clone();
                                }
                            }
                        }
                        check_peer_transitions_and_notify(&peers, iface.webhook_url.as_deref(), iface.webhook_token.as_deref());
                    }
                }
            }

            // 3. Every 60 seconds (30 ticks), record aggregate bandwidth sample into SQLite database
            sample_counter += 1;
            if sample_counter >= 30 {
                sample_counter = 0;
                if let Ok(peers) = store.get_peers() {
                    let mut total_rx_bps = 0;
                    let mut total_tx_bps = 0;
                    let mut total_rx = 0;
                    let mut total_tx = 0;

                    for peer in peers {
                        if let Some(stat) = live_stats.get(&peer.public_key) {
                            total_rx += stat.bytes_rcvd;
                            total_tx += stat.bytes_sent;
                            total_rx_bps += stat.rx_speed_bps;
                            total_tx_bps += stat.tx_speed_bps;
                        }
                    }
                    let _ = store.save_bandwidth_sample(total_rx_bps, total_tx_bps, total_rx, total_tx);
                }
            }
        }
    });
}

static PEER_ONLINE_STATES: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

pub fn check_peer_transitions_and_notify(peers: &[crate::models::WgPeer], webhook_url: Option<&str>, webhook_token: Option<&str>) {
    let url_opt = webhook_url.map(|s| s.to_string()).or_else(|| std::env::var("WEBHOOK_URL").ok());
    let url = match url_opt {
        Some(u) if !u.trim().is_empty() => u.trim().to_string(),
        _ => return,
    };

    let now_sec = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut states_guard = PEER_ONLINE_STATES.lock().unwrap();
    let states = states_guard.get_or_insert_with(HashMap::new);

    for peer in peers {
        let is_online = peer.last_handshake.map_or(false, |ts| (now_sec - ts) < 180);

        if let Some(&prev_state) = states.get(&peer.public_key) {
            if prev_state && !is_online {
                let msg = format!("🔴 WireGuard Alert: Peer '{}' ({}) is OFFLINE! Handshake expired.", peer.name, peer.allowed_ips);
                send_webhook_alert(&url, webhook_token, &format!("🔴 Tunnel Offline: {}", peer.name), &msg);
            } else if !prev_state && is_online {
                let lat_str = peer.ping_latency_ms.map_or_else(|| "".to_string(), |ms| format!(" Latency: {}ms.", ms));
                let msg = format!("🟢 WireGuard Alert: Peer '{}' ({}) is ONLINE!{}", peer.name, peer.allowed_ips, lat_str);
                send_webhook_alert(&url, webhook_token, &format!("🟢 Tunnel Online: {}", peer.name), &msg);
            }
        }

        states.insert(peer.public_key.clone(), is_online);
    }
}

pub fn send_webhook_alert(url: &str, token: Option<&str>, title: &str, message: &str) {
    let url_clone = url.to_string();
    let token_clone = token.map(|t| t.to_string()).or_else(|| std::env::var("WEBHOOK_TOKEN").ok());
    let title_clone = title.to_string();
    let msg_clone = message.to_string();

    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("curl");
        cmd.args(["-s", "-X", "POST"]);

        if let Some(ref tk) = token_clone {
            let tk_trimmed = tk.trim();
            if !tk_trimmed.is_empty() {
                let auth_hdr = if tk_trimmed.starts_with("Bearer ") || tk_trimmed.starts_with("Basic ") {
                    tk_trimmed.to_string()
                } else if tk_trimmed.contains('=') || tk_trimmed.len() >= 16 {
                    format!("Basic {}", tk_trimmed)
                } else {
                    format!("Bearer {}", tk_trimmed)
                };
                cmd.args(["-H", &format!("Authorization: {}", auth_hdr)]);
            }
        }

        if url_clone.contains("ntfy") || url_clone.contains("/WireguardUI") {
            cmd.args([
                "-H", &format!("Title: {}", title_clone),
                "-H", "Tags: wireguard,network",
                "-d", &msg_clone,
                &url_clone
            ]);
        } else {
            let payload = serde_json::json!({
                "topic": "wireguard",
                "title": title_clone,
                "message": msg_clone,
                "content": msg_clone,
                "text": msg_clone,
                "tags": ["wireguard", "network"]
            });
            cmd.args([
                "-H", "Content-Type: application/json",
                "-d", &payload.to_string(),
                &url_clone
            ]);
        }

        let _ = cmd.output();
    });
}

pub fn start_iperf3_daemon() {
    std::thread::spawn(|| {
        if is_ssh_sync_enabled() {
            let target = get_ssh_target();
            let _ = std::process::Command::new("ssh")
                .args(ssh_cmd_args(&target, "iperf3 -s -D -p 5201 2>/dev/null || true"))
                .output();
        } else {
            let _ = std::process::Command::new("iperf3")
                .args(["-s", "-D", "-p", "5201"])
                .output();
        }
    });
}

async fn collect_iperf_diagnostics(clean_ip: &str, port: u16, executed_cmd: &str) -> crate::models::IperfTestDiagnostics {
    let (route_out, ping_out, port_out, hub_ip_out) = if is_ssh_sync_enabled() {
        let target = get_ssh_target();
        let diag_script = format!(
            "ip route get {0} 2>&1; echo '===DIAG_SPLIT==='; ping -c 1 -W 2 {0} 2>&1; echo '===DIAG_SPLIT==='; nc -zv -w 2 {0} {1} 2>&1 || true; echo '===DIAG_SPLIT==='; ip -4 addr show dev wg0 | grep inet | awk '{{print $2}}' | head -n1 || true",
            clean_ip, port
        );
        let out = tokio::process::Command::new("ssh")
            .args(ssh_cmd_args(&target, &diag_script))
            .output()
            .await;
        match out {
            Ok(o) => {
                let full = String::from_utf8_lossy(&o.stdout).to_string();
                let parts: Vec<&str> = full.split("===DIAG_SPLIT===").collect();
                (
                    parts.get(0).map(|s| s.trim().to_string()).unwrap_or_default(),
                    parts.get(1).map(|s| s.trim().to_string()).unwrap_or_default(),
                    parts.get(2).map(|s| s.trim().to_string()).unwrap_or_default(),
                    parts.get(3).map(|s| s.trim().to_string()).unwrap_or_default(),
                )
            }
            Err(_) => (String::new(), String::new(), String::new(), String::new()),
        }
    } else {
        let r = std::process::Command::new("ip").args(["route", "get", clean_ip]).output().ok();
        let p = std::process::Command::new("ping").args(["-c", "1", "-W", "2", clean_ip]).output().ok();
        let pt = std::process::Command::new("nc").args(["-zv", "-w", "2", clean_ip, &port.to_string()]).output().ok();
        let h = std::process::Command::new("ip").args(["-4", "addr", "show", "dev", "wg0"]).output().ok();
        (
            r.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default(),
            p.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default(),
            pt.map(|o| String::from_utf8_lossy(&o.stderr).to_string()).unwrap_or_default(),
            h.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default(),
        )
    };

    let ping_reachable = ping_out.contains("bytes from") || ping_out.contains("1 received") || ping_out.contains("1 packets received");
    let ping_rtt_ms = if ping_reachable {
        ping_out.lines().find(|l| l.contains("time=")).and_then(|l| {
            l.split("time=").nth(1)?.split_whitespace().next()?.replace("ms", "").trim().parse::<f64>().ok()
        })
    } else {
        None
    };

    let tcp_port_open = port_out.contains("open") || port_out.contains("succeeded") || port_out.contains("Connected");

    let hub_tunnel_ip = if !hub_ip_out.trim().is_empty() {
        let first_ip = hub_ip_out.lines().find(|l| l.contains('.')).unwrap_or(&hub_ip_out);
        Some(first_ip.split('/').next().unwrap_or(first_ip).trim().to_string())
    } else {
        Some("10.100.90.2".to_string())
    };

    let (recommendation, remote_fix_command) = if !ping_reachable {
        (
            Some(format!("Tunnel IP {} is unreachable via ICMP Ping. The remote peer may be offline, WireGuard tunnel handshake is missing, or interface wg0 is down.", clean_ip)),
            Some("(command -v wg >/dev/null || sudo apt-get install -y wireguard) && sudo wg-quick up wg0".to_string())
        )
    } else if !tcp_port_open {
        (
            Some(format!("WireGuard tunnel to {} is ONLINE (Ping: {:.1} ms), but no iperf3 server daemon is responding on port {}. Run the fix command below on the remote router to start iperf3.", clean_ip, ping_rtt_ms.unwrap_or(0.0), port)),
            Some(format!("(command -v iperf3 >/dev/null || sudo apt-get update && sudo apt-get install -y iperf3 || sudo apk add iperf3 || sudo yum install -y iperf3) && sudo iperf3 -s -D -p {}", port))
        )
    } else {
        (
            Some("Network routing and port 5201 handshake are healthy.".to_string()),
            None
        )
    };

    crate::models::IperfTestDiagnostics {
        route_info: if !route_out.is_empty() { Some(route_out) } else { None },
        ping_reachable,
        ping_rtt_ms,
        tcp_port_open,
        executed_command: executed_cmd.to_string(),
        hub_tunnel_ip,
        recommendation,
        remote_fix_command,
    }
}

pub async fn run_iperf3_client(
    target_ip: &str,
    port: u16,
    duration: u32,
    reverse: bool,
    parallel: u32,
) -> crate::models::IperfTestResult {
    let clean_ip = target_ip.split('/').next().unwrap_or(target_ip).trim();
    let port_str = port.to_string();
    let duration_str = duration.max(1).min(60).to_string();
    let parallel_str = parallel.max(1).min(16).to_string();

    let full_cmd_str = format!(
        "iperf3 -c {} -p {} -t {} -P {}{} -J",
        clean_ip, port, duration, parallel, if reverse { " -R" } else { "" }
    );

    let mut args = vec![
        "-c", clean_ip,
        "-p", &port_str,
        "-t", &duration_str,
        "-P", &parallel_str,
        "-J",
    ];

    if reverse {
        args.push("-R");
    }

    let cmd_output = if is_ssh_sync_enabled() {
        let target = get_ssh_target();
        tokio::time::timeout(
            std::time::Duration::from_secs((duration + 10) as u64),
            tokio::process::Command::new("ssh")
                .args(ssh_cmd_args(&target, &full_cmd_str))
                .output(),
        )
        .await
    } else {
        tokio::time::timeout(
            std::time::Duration::from_secs((duration + 10) as u64),
            tokio::process::Command::new("iperf3")
                .args(&args)
                .output(),
        )
        .await
    };

    match cmd_output {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();

            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(err_str) = json_val.get("error").and_then(|e| e.as_str()) {
                    let diags = collect_iperf_diagnostics(clean_ip, port, &full_cmd_str).await;
                    return crate::models::IperfTestResult {
                        success: false,
                        target_ip: clean_ip.to_string(),
                        port,
                        duration_seconds: duration as f64,
                        reverse,
                        bits_per_second: 0.0,
                        formatted_speed: "0 bps".to_string(),
                        bytes_transferred: 0,
                        retransmits: None,
                        rtt_ms: None,
                        raw_summary: Some(stdout),
                        error: Some(err_str.to_string()),
                        diagnostics: Some(diags),
                    };
                }

                let end = json_val.get("end");
                let sum_received = end.and_then(|e| e.get("sum_received"));
                let sum_sent = end.and_then(|e| e.get("sum_sent"));

                let effective_sum = sum_received.or(sum_sent).or_else(|| end.and_then(|e| e.get("sum")));

                let bps = effective_sum
                    .and_then(|s| s.get("bits_per_second"))
                    .and_then(|b| b.as_f64())
                    .unwrap_or(0.0);

                let bytes = effective_sum
                    .and_then(|s| s.get("bytes"))
                    .and_then(|b| b.as_u64())
                    .unwrap_or(0);

                let retransmits = sum_sent.and_then(|s| s.get("retransmits")).and_then(|r| r.as_u64());

                let formatted_speed = if bps >= 1_000_000_000.0 {
                    format!("{:.2} Gbps", bps / 1_000_000_000.0)
                } else if bps >= 1_000_000.0 {
                    format!("{:.2} Mbps", bps / 1_000_000.0)
                } else if bps >= 1_000.0 {
                    format!("{:.2} Kbps", bps / 1_000.0)
                } else {
                    format!("{:.0} bps", bps)
                };

                let dur = effective_sum
                    .and_then(|s| s.get("seconds"))
                    .and_then(|sec| sec.as_f64())
                    .unwrap_or(duration as f64);

                crate::models::IperfTestResult {
                    success: true,
                    target_ip: clean_ip.to_string(),
                    port,
                    duration_seconds: dur,
                    reverse,
                    bits_per_second: bps,
                    formatted_speed,
                    bytes_transferred: bytes,
                    retransmits,
                    rtt_ms: None,
                    raw_summary: Some(stdout),
                    error: None,
                    diagnostics: None,
                }
            } else {
                let err_msg = if !stderr.trim().is_empty() {
                    stderr
                } else if !stdout.trim().is_empty() {
                    stdout
                } else {
                    "Connection refused or remote host does not have iperf3 server listening on port 5201".to_string()
                };

                let diags = collect_iperf_diagnostics(clean_ip, port, &full_cmd_str).await;

                crate::models::IperfTestResult {
                    success: false,
                    target_ip: clean_ip.to_string(),
                    port,
                    duration_seconds: duration as f64,
                    reverse,
                    bits_per_second: 0.0,
                    formatted_speed: "0 bps".to_string(),
                    bytes_transferred: 0,
                    retransmits: None,
                    rtt_ms: None,
                    raw_summary: None,
                    error: Some(err_msg),
                    diagnostics: Some(diags),
                }
            }
        }
        Ok(Err(e)) => {
            let diags = collect_iperf_diagnostics(clean_ip, port, &full_cmd_str).await;
            crate::models::IperfTestResult {
                success: false,
                target_ip: clean_ip.to_string(),
                port,
                duration_seconds: duration as f64,
                reverse,
                bits_per_second: 0.0,
                formatted_speed: "0 bps".to_string(),
                bytes_transferred: 0,
                retransmits: None,
                rtt_ms: None,
                raw_summary: None,
                error: Some(format!("Failed to execute iperf3 binary: {}", e)),
                diagnostics: Some(diags),
            }
        }
        Err(_) => {
            let diags = collect_iperf_diagnostics(clean_ip, port, &full_cmd_str).await;
            crate::models::IperfTestResult {
                success: false,
                target_ip: clean_ip.to_string(),
                port,
                duration_seconds: duration as f64,
                reverse,
                bits_per_second: 0.0,
                formatted_speed: "0 bps".to_string(),
                bytes_transferred: 0,
                retransmits: None,
                rtt_ms: None,
                raw_summary: None,
                error: Some(format!("iperf3 test timed out after {} seconds", duration + 10)),
                diagnostics: Some(diags),
            }
        }
    }
}

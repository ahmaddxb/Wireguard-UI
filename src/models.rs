use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PeerType {
    Client,
    SiteToSite,
}

impl std::fmt::Display for PeerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerType::Client => write!(f, "client"),
            PeerType::SiteToSite => write!(f, "site_to_site"),
        }
    }
}

impl std::str::FromStr for PeerType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "client" => Ok(PeerType::Client),
            "site_to_site" | "sitetosite" | "site-to-site" => Ok(PeerType::SiteToSite),
            _ => Err(format!("Unknown peer type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WgInterface {
    pub id: String,
    pub name: String,
    pub private_key: String,
    pub public_key: String,
    pub listen_port: u16,
    pub address_pool: String, // e.g., "10.8.0.1/24"
    pub dns: String,          // e.g., "1.1.1.1, 8.8.8.8"
    pub post_up: String,
    pub post_down: String,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub webhook_token: Option<String>,
    /// Public IP or hostname of this WireGuard hub, used in generated remote peer configs.
    /// e.g., "192.168.1.101" or "vpn.example.com"
    #[serde(default)]
    pub gateway_endpoint: Option<String>,
    /// Local LAN subnets behind the hub accessible by remote sites (e.g. "192.168.1.0/24, 10.1.100.0/24")
    #[serde(default)]
    pub hub_subnets: Option<String>,
    #[serde(default = "default_mtu")]
    pub mtu: Option<u32>,
    #[serde(default = "default_true")]
    pub mss_clamping: bool,
    #[serde(default)]
    pub address_pool_v6: Option<String>, // e.g. "fd00:90::1/64"
    #[serde(default)]
    pub dns_v6: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
}

fn default_mtu() -> Option<u32> {
    Some(1420)
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WgPeer {
    pub id: String,
    pub interface_id: String,
    pub name: String,
    pub peer_type: PeerType,
    pub public_key: String,
    pub private_key: Option<String>, // Saved for clients to download .conf / QR
    pub preshared_key: Option<String>,
    pub allowed_ips: String,         // e.g., "10.8.0.2/32, fd00:90::2/128"
    pub endpoint: Option<String>,    // e.g., "branch.company.com:51820"
    pub persistent_keepalive: u16,   // e.g., 25
    pub enabled: bool,
    pub notes: Option<String>,
    pub created_at: i64,
    pub last_handshake: Option<i64>,
    pub bytes_rcvd: u64,
    pub bytes_sent: u64,
    #[serde(default)]
    pub rx_speed_bps: u64,
    #[serde(default)]
    pub tx_speed_bps: u64,
    #[serde(default)]
    pub ping_latency_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthSample {
    pub timestamp: i64,
    pub rx_speed_bps: u64,
    pub tx_speed_bps: u64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateInterfacePayload {
    pub id: String, // e.g. "wg1"
    pub name: String,
    pub listen_port: u16,
    pub address_pool: String,
    #[serde(default)]
    pub address_pool_v6: Option<String>,
    pub dns: Option<String>,
    #[serde(default)]
    pub dns_v6: Option<String>,
    pub gateway_endpoint: Option<String>,
    pub hub_subnets: Option<String>,
    #[serde(default = "default_mtu")]
    pub mtu: Option<u32>,
    #[serde(default = "default_true")]
    pub mss_clamping: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateInterfacePayload {
    pub name: String,
    pub listen_port: u16,
    pub address_pool: String,
    pub address_pool_v6: Option<String>,
    pub dns: String,
    pub dns_v6: Option<String>,
    pub post_up: String,
    pub post_down: String,
    pub webhook_url: Option<String>,
    pub webhook_token: Option<String>,
    pub gateway_endpoint: Option<String>,
    pub hub_subnets: Option<String>,
    pub mtu: Option<u32>,
    pub mss_clamping: Option<bool>,
    pub public_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateClientPayload {
    #[serde(default)]
    pub interface_id: Option<String>,
    pub name: String,
    pub public_key: Option<String>,
    pub preshared_key: Option<String>,
    pub generate_psk: Option<bool>,
    pub custom_ip: Option<String>,
    pub custom_ip_v6: Option<String>,
    #[allow(dead_code)]
    pub dns: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSitePayload {
    #[serde(default)]
    pub interface_id: Option<String>,
    pub name: String,
    pub remote_public_key: String,
    pub remote_allowed_ips: String, // e.g. "192.168.20.0/24"
    pub endpoint: Option<String>,   // e.g. "203.0.113.5:51820"
    pub preshared_key: Option<String>,
    pub persistent_keepalive: Option<u16>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePeerPayload {
    pub name: String,
    pub public_key: String,
    pub allowed_ips: String,
    pub endpoint: Option<String>,
    pub preshared_key: Option<Option<String>>,
    pub persistent_keepalive: Option<u16>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TestNotificationPayload {
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub webhook_token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerQrResponse {
    pub peer_id: String,
    pub config_content: String,
    pub qr_svg_data_uri: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiteConfigResponse {
    pub peer_id: String,
    pub peer_name: String,
    pub remote_config: String,
    pub bootstrap_cmd: String,
    pub probe_cmd: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterfaceOverview {
    pub interface: WgInterface,
    pub total_clients: usize,
    pub active_clients: usize,
    pub total_site_tunnels: usize,
    pub active_site_tunnels: usize,
    pub total_bytes_rcvd: u64,
    pub total_bytes_sent: u64,
    pub total_rx_speed_bps: u64,
    pub total_tx_speed_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub action: String,
    pub target: String,
    pub details: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IperfTestPayload {
    pub target_ip: Option<String>,
    #[serde(default = "default_iperf_port")]
    pub port: u16,
    #[serde(default = "default_iperf_duration")]
    pub duration: u32,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default = "default_iperf_parallel")]
    pub parallel: u32,
}

fn default_iperf_port() -> u16 { 5201 }
fn default_iperf_duration() -> u32 { 5 }
fn default_iperf_parallel() -> u32 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IperfTestDiagnostics {
    pub route_info: Option<String>,
    pub ping_reachable: bool,
    pub ping_rtt_ms: Option<f64>,
    pub tcp_port_open: bool,
    pub executed_command: String,
    pub hub_tunnel_ip: Option<String>,
    pub recommendation: Option<String>,
    pub remote_fix_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IperfTestResult {
    pub success: bool,
    pub target_ip: String,
    pub port: u16,
    pub duration_seconds: f64,
    pub reverse: bool,
    pub bits_per_second: f64,
    pub formatted_speed: String,
    pub bytes_transferred: u64,
    pub retransmits: Option<u64>,
    pub rtt_ms: Option<f64>,
    pub raw_summary: Option<String>,
    pub error: Option<String>,
    pub diagnostics: Option<IperfTestDiagnostics>,
}

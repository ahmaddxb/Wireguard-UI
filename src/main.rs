mod embedded_ui;
mod models;
mod store;
mod wg_engine;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use models::*;
use std::net::SocketAddr;
use store::DbStore;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    eprintln!(">>> Starting WireGuard WebUI server...");

    // Initialize logging
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "wireguard_ui=info,tower_http=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .try_init();

    // Initialize SQLite store (creates wireguard_ui.db file in working dir or DB_PATH)
    eprintln!(">>> Initializing database store...");
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "wireguard_ui.db".into());
    let store = DbStore::new(&db_path)
        .or_else(|_| DbStore::in_memory())
        .expect("Failed to initialize database store");

    // Ensure default interface exists
    let _ = store.get_or_create_default_interface();

    // Ensure Hub iperf3 daemon is running
    wg_engine::start_iperf3_daemon();

    // API Routes
    let api_routes = Router::new()
        .route("/overview", get(get_overview))
        .route("/interfaces", get(list_interfaces).post(create_interface_handler))
        .route("/interfaces/:id", get(get_interface_by_id_handler).put(update_interface_by_id_handler).delete(delete_interface_handler))
        .route("/interfaces/:id/overview", get(get_interface_overview))
        .route("/interfaces/:id/export", get(export_interface_config_handler))
        .route("/interface", get(get_interface).put(update_interface))
        .route("/interface/detect-subnets", get(detect_subnets_handler))
        .route("/peers", get(list_peers))
        .route("/peers/client", post(create_client_peer))
        .route("/peers/site", post(create_site_peer))
        .route("/peers/site/probe-command", get(get_probe_command))
        .route("/peers/:id/qr", get(get_peer_qr))
        .route("/peers/:id/site-config", get(get_site_config))
        .route("/peers/:id/download", get(download_peer_config))
        .route("/peers/:id/toggle", post(toggle_peer))
        .route("/peers/:id/iperf3", post(run_peer_iperf3))
        .route("/peers/:id", put(update_peer).delete(delete_peer))
        .route("/iperf3/run", post(run_custom_iperf3))
        .route("/config/export", get(export_server_config))
        .route("/analytics/bandwidth", get(get_bandwidth_analytics))
        .route("/notifications/test", post(send_test_notification))
        .route("/logs", get(get_logs))
        .route("/topology", get(get_topology))
        .with_state(store.clone());

    let app = Router::new()
        .nest("/api", api_routes)
        .fallback(embedded_ui::static_handler)
        .layer(CorsLayer::permissive());

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".into())
        .parse()
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    eprintln!(">>> WireGuard WebUI listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.expect("Failed to bind TcpListener");
    eprintln!(">>> TcpListener bound successfully, starting axum::serve...");

    // Sync WireGuard config on startup
    sync_wireguard(&store, false);

    // Start background telemetry & bandwidth sampler worker
    wg_engine::start_background_telemetry_loop(store.clone());

    axum::serve(listener, app).await.expect("axum::serve failed");
}

fn sync_wireguard(store: &DbStore, force_restart: bool) {
    if let Ok(interfaces) = store.get_interfaces() {
        for mut iface in interfaces {
            if let Some(true_pub) = wg_engine::derive_public_key(&iface.private_key) {
                if true_pub != iface.public_key {
                    tracing::info!("Auto-repairing interface {} public key to true Curve25519 {}", iface.id, true_pub);
                    iface.public_key = true_pub;
                    let _ = store.save_interface(&iface);
                }
            }

            if let Ok(mut peers) = store.get_peers_by_interface(&iface.id) {
                for peer in peers.iter_mut() {
                    if let Some(ref privk) = peer.private_key {
                        if let Some(true_pub) = wg_engine::derive_public_key(privk) {
                            if true_pub != peer.public_key {
                                tracing::info!("Auto-repairing peer {} ({}) public key to true Curve25519 {}", peer.name, peer.id, true_pub);
                                peer.public_key = true_pub;
                                let _ = store.save_peer(peer);
                            }
                        }
                    }
                }
                wg_engine::apply_wireguard_config(&iface, &peers, force_restart);
            }
        }
    }
}

async fn list_interfaces(State(store): State<DbStore>) -> Result<Json<Vec<WgInterface>>, StatusCode> {
    let mut ifaces = store.get_interfaces().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if ifaces.is_empty() {
        if let Ok(def) = store.get_or_create_default_interface() {
            ifaces.push(def);
        }
    }
    Ok(Json(ifaces))
}

async fn create_interface_handler(
    State(store): State<DbStore>,
    Json(payload): Json<models::CreateInterfacePayload>,
) -> Result<Json<WgInterface>, StatusCode> {
    let (priv_key, pub_key) = wg_engine::generate_keypair();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let iface = WgInterface {
        id: payload.id.clone(),
        name: payload.name,
        private_key: priv_key,
        public_key: pub_key,
        listen_port: payload.listen_port,
        address_pool: payload.address_pool,
        address_pool_v6: payload.address_pool_v6,
        dns: payload.dns.unwrap_or_else(|| "1.1.1.1, 8.8.8.8".to_string()),
        dns_v6: payload.dns_v6,
        post_up: format!("iptables -A FORWARD -i {0} -j ACCEPT; iptables -A FORWARD -o {0} -j ACCEPT; iptables -t nat -A POSTROUTING -o vmbr0 -j MASQUERADE; iptables -t mangle -A POSTROUTING -p tcp --tcp-flags SYN,RST SYN -o {0} -j TCPMSS --clamp-mss-to-pmtu", payload.id),
        post_down: format!("iptables -D FORWARD -i {0} -j ACCEPT; iptables -D FORWARD -o {0} -j ACCEPT; iptables -t nat -D POSTROUTING -o vmbr0 -j MASQUERADE; iptables -t mangle -D POSTROUTING -p tcp --tcp-flags SYN,RST SYN -o {0} -j TCPMSS --clamp-mss-to-pmtu", payload.id),
        webhook_url: None,
        webhook_token: None,
        gateway_endpoint: payload.gateway_endpoint,
        hub_subnets: payload.hub_subnets,
        mtu: payload.mtu,
        mss_clamping: payload.mss_clamping,
        enabled: true,
        created_at: now,
    };

    store.save_interface(&iface).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = store.add_audit_log("CREATE_INTERFACE", &iface.name, &format!("Created interface {}", iface.id));
    sync_wireguard(&store, false);

    Ok(Json(iface))
}

async fn get_interface_by_id_handler(
    State(store): State<DbStore>,
    Path(id): Path<String>,
) -> Result<Json<WgInterface>, StatusCode> {
    let iface = store.get_interface_by_id(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(iface))
}

async fn update_interface_by_id_handler(
    State(store): State<DbStore>,
    Path(id): Path<String>,
    Json(payload): Json<models::UpdateInterfacePayload>,
) -> Result<Json<WgInterface>, StatusCode> {
    let mut iface = store.get_interface_by_id(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    iface.name = payload.name;
    iface.listen_port = payload.listen_port;
    iface.address_pool = payload.address_pool;
    iface.address_pool_v6 = payload.address_pool_v6;
    iface.dns = payload.dns;
    iface.dns_v6 = payload.dns_v6;
    iface.post_up = payload.post_up;
    iface.post_down = payload.post_down;
    iface.webhook_url = payload.webhook_url;
    iface.webhook_token = payload.webhook_token;
    iface.gateway_endpoint = payload.gateway_endpoint;
    iface.hub_subnets = payload.hub_subnets;
    iface.mtu = payload.mtu;
    if let Some(m) = payload.mss_clamping {
        iface.mss_clamping = m;
    }
    if let Some(pk) = payload.public_key {
        if !pk.trim().is_empty() {
            iface.public_key = pk;
        }
    }

    store.update_interface(&iface).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = store.add_audit_log("UPDATE_INTERFACE", &iface.name, &format!("Updated interface {}", iface.id));
    sync_wireguard(&store, false);

    Ok(Json(iface))
}

async fn delete_interface_handler(
    State(store): State<DbStore>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    store.delete_interface(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = store.add_audit_log("DELETE_INTERFACE", &id, &format!("Deleted interface {}", id));
    Ok(StatusCode::NO_CONTENT)
}

async fn get_interface_overview(
    State(store): State<DbStore>,
    Path(id): Path<String>,
) -> Result<Json<InterfaceOverview>, StatusCode> {
    let iface = store.get_interface_by_id(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut peers = store.get_peers_by_interface(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let live_stats = wg_engine::fetch_live_wireguard_stats();

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

    let total_clients = peers.iter().filter(|p| p.peer_type == PeerType::Client).count();
    let active_clients = peers.iter().filter(|p| p.peer_type == PeerType::Client && p.enabled).count();

    let total_site_tunnels = peers.iter().filter(|p| p.peer_type == PeerType::SiteToSite).count();
    let active_site_tunnels = peers.iter().filter(|p| p.peer_type == PeerType::SiteToSite && p.enabled).count();

    let total_bytes_rcvd = peers.iter().map(|p| p.bytes_rcvd).sum();
    let total_bytes_sent = peers.iter().map(|p| p.bytes_sent).sum();
    let total_rx_speed_bps = peers.iter().map(|p| p.rx_speed_bps).sum();
    let total_tx_speed_bps = peers.iter().map(|p| p.tx_speed_bps).sum();

    Ok(Json(InterfaceOverview {
        interface: iface,
        total_clients,
        active_clients,
        total_site_tunnels,
        active_site_tunnels,
        total_bytes_rcvd,
        total_bytes_sent,
        total_rx_speed_bps,
        total_tx_speed_bps,
    }))
}

async fn get_overview(State(store): State<DbStore>) -> Result<Json<InterfaceOverview>, StatusCode> {
    let iface = store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut peers = store.get_peers().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let live_stats = wg_engine::fetch_live_wireguard_stats();

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

    let total_clients = peers.iter().filter(|p| p.peer_type == PeerType::Client).count();
    let active_clients = peers.iter().filter(|p| p.peer_type == PeerType::Client && p.enabled).count();

    let total_site_tunnels = peers.iter().filter(|p| p.peer_type == PeerType::SiteToSite).count();
    let active_site_tunnels = peers.iter().filter(|p| p.peer_type == PeerType::SiteToSite && p.enabled).count();

    let total_bytes_rcvd = peers.iter().map(|p| p.bytes_rcvd).sum();
    let total_bytes_sent = peers.iter().map(|p| p.bytes_sent).sum();
    let total_rx_speed_bps = peers.iter().map(|p| p.rx_speed_bps).sum();
    let total_tx_speed_bps = peers.iter().map(|p| p.tx_speed_bps).sum();

    Ok(Json(InterfaceOverview {
        interface: iface,
        total_clients,
        active_clients,
        total_site_tunnels,
        active_site_tunnels,
        total_bytes_rcvd,
        total_bytes_sent,
        total_rx_speed_bps,
        total_tx_speed_bps,
    }))
}

async fn send_test_notification(
    State(store): State<DbStore>,
    Json(payload): Json<TestNotificationPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let iface = store.get_or_create_default_interface().map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to load WireGuard interface"}))))?;

    let url = payload.webhook_url
        .filter(|u| !u.trim().is_empty())
        .or(iface.webhook_url)
        .or_else(|| std::env::var("WEBHOOK_URL").ok())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Webhook URL is missing. Please enter a Webhook URL."}))))?;

    let token = payload.webhook_token
        .filter(|t| !t.trim().is_empty())
        .or(iface.webhook_token)
        .or_else(|| std::env::var("WEBHOOK_TOKEN").ok());

    wg_engine::send_webhook_alert(
        &url,
        token.as_deref(),
        "🔔 WireGuard-UI Test Alert",
        "This is a test notification from your WireGuard-UI portal! Webhook & authentication are working perfectly.",
    );

    let _ = store.add_audit_log("TEST_NOTIFICATION", "WEBHOOK", &format!("Sent test notification to {}", url));

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Test notification dispatched to Webhook server!"
    })))
}

async fn get_bandwidth_analytics(State(store): State<DbStore>) -> Result<Json<Vec<BandwidthSample>>, StatusCode> {
    let samples = store.get_bandwidth_history(144).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(samples))
}

async fn get_interface(State(store): State<DbStore>) -> Result<Json<WgInterface>, StatusCode> {
    let iface = store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(iface))
}

async fn detect_subnets_handler() -> Result<Json<serde_json::Value>, StatusCode> {
    let subnets = wg_engine::detect_hub_subnets();
    Ok(Json(serde_json::json!({
        "subnets": subnets.join(", ")
    })))
}

async fn update_interface(
    State(store): State<DbStore>,
    Json(payload): Json<WgInterface>,
) -> Result<Json<WgInterface>, StatusCode> {
    store.update_interface(&payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = store.add_audit_log("UPDATE_INTERFACE", &payload.name, "Updated WireGuard interface settings");
    sync_wireguard(&store, false);
    Ok(Json(payload))
}

async fn list_peers(State(store): State<DbStore>) -> Result<Json<Vec<WgPeer>>, StatusCode> {
    let mut peers = store.get_peers().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let live_stats = wg_engine::fetch_live_wireguard_stats();

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

    Ok(Json(peers))
}

async fn create_client_peer(
    State(store): State<DbStore>,
    Json(payload): Json<CreateClientPayload>,
) -> Result<Json<WgPeer>, StatusCode> {
    let iface = if let Some(ref if_id) = payload.interface_id {
        store.get_interface_by_id(if_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .unwrap_or(store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
    } else {
        store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let existing_peers = store.get_peers_by_interface(&iface.id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (priv_key, pub_key) = if let Some(ref pk) = payload.public_key {
        if !pk.trim().is_empty() {
            (None, pk.trim().to_string())
        } else {
            let (privk, pubk) = wg_engine::generate_keypair();
            (Some(privk), pubk)
        }
    } else {
        let (privk, pubk) = wg_engine::generate_keypair();
        (Some(privk), pubk)
    };

    let ps_key: Option<String> = if let Some(ref psk) = payload.preshared_key {
        if !psk.trim().is_empty() {
            Some(psk.trim().to_string())
        } else {
            None
        }
    } else if payload.generate_psk == Some(true) {
        Some(wg_engine::generate_preshared_key())
    } else {
        None
    };

    let existing_ips: Vec<String> = existing_peers.iter().map(|p| p.allowed_ips.clone()).collect();
    let allocated_v4 = match payload.custom_ip {
        Some(ip) if !ip.trim().is_empty() => {
            if ip.contains('/') { ip } else { format!("{}/32", ip) }
        }
        _ => wg_engine::allocate_next_ip(&iface.address_pool, &existing_ips),
    };

    let allocated_v6 = match payload.custom_ip_v6 {
        Some(ip) if !ip.trim().is_empty() => {
            if ip.contains('/') { Some(ip) } else { Some(format!("{}/128", ip)) }
        }
        _ => {
            if let Some(ref v6_pool) = iface.address_pool_v6 {
                if !v6_pool.trim().is_empty() {
                    Some(wg_engine::allocate_next_ipv6(v6_pool.trim(), &existing_ips))
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    let allocated_ip = if let Some(v6) = allocated_v6 {
        format!("{}, {}", allocated_v4, v6)
    } else {
        allocated_v4
    };

    let id = format!("peer-{}", rand::random::<u32>());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let peer = WgPeer {
        id: id.clone(),
        interface_id: iface.id.clone(),
        name: payload.name,
        peer_type: PeerType::Client,
        public_key: pub_key,
        private_key: priv_key,
        preshared_key: ps_key,
        allowed_ips: allocated_ip,
        endpoint: None,
        persistent_keepalive: 25,
        enabled: true,
        notes: payload.notes,
        created_at: now,
        last_handshake: None,
        bytes_rcvd: 0,
        bytes_sent: 0,
        rx_speed_bps: 0,
        tx_speed_bps: 0,
        ping_latency_ms: None,
    };

    store.save_peer(&peer).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = store.add_audit_log("CREATE_CLIENT_PEER", &peer.name, &format!("Allocated IP: {}", peer.allowed_ips));
    sync_wireguard(&store, false);
    Ok(Json(peer))
}

async fn create_site_peer(
    State(store): State<DbStore>,
    Json(payload): Json<CreateSitePayload>,
) -> Result<Json<WgPeer>, StatusCode> {
    let iface = if let Some(ref if_id) = payload.interface_id {
        store.get_interface_by_id(if_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .unwrap_or(store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
    } else {
        store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let preshared_key = payload
        .preshared_key
        .filter(|value| !value.trim().is_empty());

    let existing_peers = store.get_peers_by_interface(&iface.id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let existing_ips: Vec<String> = existing_peers.iter().map(|p| p.allowed_ips.clone()).collect();
    let next_tunnel_ip = wg_engine::allocate_next_ip(&iface.address_pool, &existing_ips);

    let allowed_ips = if payload.remote_allowed_ips.trim().is_empty() {
        next_tunnel_ip
    } else if payload.remote_allowed_ips.contains(&next_tunnel_ip) || payload.remote_allowed_ips.contains("/32") {
        payload.remote_allowed_ips
    } else {
        format!("{}, {}", next_tunnel_ip, payload.remote_allowed_ips)
    };

    let id = format!("site-{}", rand::random::<u32>());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let peer = WgPeer {
        id: id.clone(),
        interface_id: iface.id.clone(),
        name: payload.name,
        peer_type: PeerType::SiteToSite,
        public_key: payload.remote_public_key,
        private_key: None,
        preshared_key,
        allowed_ips,
        endpoint: payload.endpoint,
        persistent_keepalive: payload.persistent_keepalive.unwrap_or(25),
        enabled: true,
        notes: payload.notes,
        created_at: now,
        last_handshake: None,
        bytes_rcvd: 0,
        bytes_sent: 0,
        rx_speed_bps: 0,
        tx_speed_bps: 0,
        ping_latency_ms: None,
    };

    store.save_peer(&peer).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = store.add_audit_log("CREATE_SITE_PEER", &peer.name, &format!("Routes: {}", peer.allowed_ips));
    sync_wireguard(&store, false);

    Ok(Json(peer))
}

async fn get_peer_qr(
    State(store): State<DbStore>,
    Path(id): Path<String>,
) -> Result<Json<PeerQrResponse>, StatusCode> {
    let peer = store.get_peer_by_id(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let iface = store.get_interface_by_id(&peer.interface_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .unwrap_or(store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?);

    let client_priv = peer.private_key.as_deref().unwrap_or("<PRIVATE_KEY_HIDDEN>");
    let server_endpoint = wg_engine::get_hub_endpoint(&iface);

    let dns_str = if let Some(ref v6_dns) = iface.dns_v6 {
        if !v6_dns.trim().is_empty() {
            format!("{}, {}", iface.dns, v6_dns.trim())
        } else {
            iface.dns.clone()
        }
    } else {
        iface.dns.clone()
    };

    let conf = wg_engine::build_client_config(
        client_priv,
        &peer.allowed_ips,
        &iface.public_key,
        &server_endpoint,
        &dns_str,
        peer.preshared_key.as_deref(),
        peer.persistent_keepalive,
        iface.mtu,
    );

    let qr_svg = wg_engine::generate_qr_svg_data_uri(&conf).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PeerQrResponse {
        peer_id: peer.id,
        config_content: conf,
        qr_svg_data_uri: qr_svg,
    }))
}

async fn get_site_config(
    State(store): State<DbStore>,
    Path(id): Path<String>,
) -> Result<Json<SiteConfigResponse>, StatusCode> {
    let peer = store.get_peer_by_id(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let iface = store.get_interface_by_id(&peer.interface_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .unwrap_or(store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?);

    let existing_peers = store.get_peers_by_interface(&iface.id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let existing_ips: Vec<String> = existing_peers.iter().map(|p| p.allowed_ips.clone()).collect();
    let next_ip = wg_engine::allocate_next_ip(&iface.address_pool, &existing_ips);

    let remote_config = wg_engine::build_site_config(&peer, &iface);
    let probe_cmd = wg_engine::get_remote_probe_command(&next_ip);

    let bootstrap_cmd = format!(
        "IFACE=$(ip route show default | awk 'NR==1{{print $5}}')\n(umask 077 && mkdir -p /etc/wireguard && chmod 700 /etc/wireguard && ([ -f /etc/wireguard/private.key ] || wg genkey > /etc/wireguard/private.key); wg pubkey < /etc/wireguard/private.key > /etc/wireguard/public.key && chmod 600 /etc/wireguard/*.key)\ncat << EOF > /etc/wireguard/wg0.conf\n{}\nEOF\nsystemctl enable --now wg-quick@wg0 2>/dev/null || wg-quick up wg0\necho -e \"\\n✅ WireGuard setup complete! Public Key: $(cat /etc/wireguard/public.key 2>/dev/null)\"",
        remote_config
            .replace("<PASTE_REMOTE_PRIVATE_KEY_HERE_OR_AUTOGENERATED>", "$(cat /etc/wireguard/private.key)")
            .replace("WGUI_IFACE", "$IFACE")
    );

    Ok(Json(SiteConfigResponse {
        peer_id: peer.id,
        peer_name: peer.name,
        remote_config,
        bootstrap_cmd,
        probe_cmd,
    }))
}

async fn get_probe_command(State(store): State<DbStore>) -> Result<Json<serde_json::Value>, StatusCode> {
    let iface = store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let existing_peers = store.get_peers_by_interface(&iface.id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let existing_ips: Vec<String> = existing_peers.iter().map(|p| p.allowed_ips.clone()).collect();
    let next_ip = wg_engine::allocate_next_ip(&iface.address_pool, &existing_ips);
    let probe_cmd = wg_engine::get_remote_probe_command(&next_ip);

    Ok(Json(serde_json::json!({
        "next_ip": next_ip,
        "probe_cmd": probe_cmd
    })))
}

async fn download_peer_config(
    State(store): State<DbStore>,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    let peer = store.get_peer_by_id(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let iface = store.get_interface_by_id(&peer.interface_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .unwrap_or(store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?);
    let client_priv = peer.private_key.as_deref().unwrap_or("<PRIVATE_KEY_HIDDEN>");
    let server_endpoint = wg_engine::get_hub_endpoint(&iface);

    let dns_str = if let Some(ref v6_dns) = iface.dns_v6 {
        if !v6_dns.trim().is_empty() {
            format!("{}, {}", iface.dns, v6_dns.trim())
        } else {
            iface.dns.clone()
        }
    } else {
        iface.dns.clone()
    };

    let conf = wg_engine::build_client_config(
        client_priv,
        &peer.allowed_ips,
        &iface.public_key,
        &server_endpoint,
        &dns_str,
        peer.preshared_key.as_deref(),
        peer.persistent_keepalive,
        iface.mtu,
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/plain; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}.conf\"", peer.name.to_lowercase().replace(' ', "_"))
            .parse()
            .unwrap(),
    );

    Ok((headers, conf).into_response())
}

async fn run_peer_iperf3(
    State(store): State<DbStore>,
    Path(id): Path<String>,
    Json(payload): Json<models::IperfTestPayload>,
) -> Result<Json<models::IperfTestResult>, StatusCode> {
    let peer = store.get_peer_by_id(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let target_ip = if let Some(ref custom_ip) = payload.target_ip {
        if !custom_ip.trim().is_empty() {
            custom_ip.clone()
        } else {
            peer.allowed_ips.split(',').next().unwrap_or("").trim().to_string()
        }
    } else {
        peer.allowed_ips.split(',').next().unwrap_or("").trim().to_string()
    };

    let result = wg_engine::run_iperf3_client(
        &target_ip,
        payload.port,
        payload.duration,
        payload.reverse,
        payload.parallel,
    ).await;

    let _ = store.add_audit_log(
        "IPERF3_TEST",
        &peer.name,
        &format!("Target: {}, Speed: {}, Success: {}", target_ip, result.formatted_speed, result.success),
    );

    Ok(Json(result))
}

async fn run_custom_iperf3(
    State(store): State<DbStore>,
    Json(payload): Json<models::IperfTestPayload>,
) -> Result<Json<models::IperfTestResult>, StatusCode> {
    let target_ip = payload.target_ip.as_deref().unwrap_or("127.0.0.1");
    let result = wg_engine::run_iperf3_client(
        target_ip,
        payload.port,
        payload.duration,
        payload.reverse,
        payload.parallel,
    ).await;

    let _ = store.add_audit_log(
        "IPERF3_CUSTOM_TEST",
        target_ip,
        &format!("Speed: {}, Success: {}", result.formatted_speed, result.success),
    );

    Ok(Json(result))
}

async fn toggle_peer(
    State(store): State<DbStore>,
    Path(id): Path<String>,
) -> Result<Json<WgPeer>, StatusCode> {
    let mut peer = store.get_peer_by_id(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    peer.enabled = !peer.enabled;
    store.toggle_peer_enabled(&id, peer.enabled).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = store.add_audit_log("TOGGLE_PEER", &peer.name, &format!("Enabled: {}", peer.enabled));
    sync_wireguard(&store, false);

    Ok(Json(peer))
}

async fn delete_peer(
    State(store): State<DbStore>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let peer = store.get_peer_by_id(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(p) = peer {
        store.delete_peer(&id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let _ = store.add_audit_log("DELETE_PEER", &p.name, "Peer removed");
        sync_wireguard(&store, false);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn update_peer(
    State(store): State<DbStore>,
    Path(id): Path<String>,
    Json(payload): Json<UpdatePeerPayload>,
) -> Result<Json<WgPeer>, StatusCode> {
    let mut peer = store
        .get_peer_by_id(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    peer.name = payload.name;
    peer.public_key = payload.public_key;
    peer.allowed_ips = payload.allowed_ips;
    peer.endpoint = payload.endpoint;
    let psk_cleared = match payload.preshared_key {
        Some(Some(ref psk)) => psk.trim().is_empty(),
        Some(None) => true,
        None => false,
    };

    match payload.preshared_key {
        Some(Some(psk)) => {
            peer.preshared_key = if psk.trim().is_empty() { None } else { Some(psk) };
        }
        Some(None) => {
            peer.preshared_key = None;
        }
        None => {}
    }
    peer.persistent_keepalive = payload.persistent_keepalive.unwrap_or(25);
    peer.notes = payload.notes;

    if psk_cleared {
        // Explicitly clear PSK live in WireGuard kernel without bringing down the interface
        let pubkey = &peer.public_key;
        if wg_engine::is_ssh_sync_enabled() {
            let ssh_target = wg_engine::get_ssh_target();
            let remote_cmd = format!("wg set wg0 peer {} preshared-key /dev/null 2>/dev/null || true", pubkey);
            let _ = std::process::Command::new("ssh")
                .args(wg_engine::ssh_cmd_args(&ssh_target, &remote_cmd))
                .output();
        } else {
            let local_cmd = format!("wg set wg0 peer {} preshared-key /dev/null 2>/dev/null || true", pubkey);
            let _ = std::process::Command::new("sh").args(["-c", &local_cmd]).output();
        }
    }

    store.save_peer(&peer).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = store.add_audit_log("UPDATE_PEER", &peer.name, &format!("Updated routes: {}", peer.allowed_ips));
    sync_wireguard(&store, false);

    Ok(Json(peer))
}

#[derive(serde::Deserialize)]
struct ExportConfigQuery {
    interface_id: Option<String>,
    id: Option<String>,
}

fn serve_interface_config(store: &DbStore, id: &str) -> Result<Response, StatusCode> {
    let iface = store
        .get_interface_by_id(id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let peers = store
        .get_peers_by_interface(&iface.id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let server_conf = wg_engine::build_server_config(&iface, &peers);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/plain; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}.conf\"", iface.id)
            .parse()
            .unwrap(),
    );

    Ok((headers, server_conf).into_response())
}

async fn export_interface_config_handler(
    State(store): State<DbStore>,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    serve_interface_config(&store, &id)
}

async fn export_server_config(
    State(store): State<DbStore>,
    Query(query): Query<ExportConfigQuery>,
) -> Result<Response, StatusCode> {
    if let Some(ref id) = query.interface_id.as_ref().or(query.id.as_ref()) {
        serve_interface_config(&store, id)
    } else {
        let iface = store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        serve_interface_config(&store, &iface.id)
    }
}

async fn get_logs(State(store): State<DbStore>) -> Result<Json<Vec<AuditLog>>, StatusCode> {
    let logs = store.get_audit_logs(50).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(logs))
}

#[derive(serde::Serialize)]
struct TopologyData {
    nodes: Vec<TopologyNode>,
    links: Vec<TopologyLink>,
}

#[derive(serde::Serialize)]
struct TopologyNode {
    id: String,
    label: String,
    node_type: String, // "hub", "client", "site"
    status: String,   // "online", "disabled"
    details: String,
}

#[derive(serde::Serialize)]
struct TopologyLink {
    source: String,
    target: String,
    label: String,
}

async fn get_topology(State(store): State<DbStore>) -> Result<Json<TopologyData>, StatusCode> {
    let iface = store.get_or_create_default_interface().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let peers = store.get_peers().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let live_stats = wg_engine::fetch_live_wireguard_stats();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut nodes = Vec::new();
    let mut links = Vec::new();

    // Central WireGuard Gateway Node
    nodes.push(TopologyNode {
        id: iface.id.clone(),
        label: format!("WireGuard Gateway ({})", iface.name),
        node_type: "hub".to_string(),
        status: "online".to_string(),
        details: format!("Port: {} | Pool: {}", iface.listen_port, iface.address_pool),
    });

    for peer in peers {
        let n_type = match peer.peer_type {
            PeerType::Client => "client",
            PeerType::SiteToSite => "site",
        };

        let is_handshake_active = if let Some(stat) = live_stats.get(&peer.public_key) {
            stat.last_handshake.map_or(false, |ts| (now - ts) < 180)
        } else {
            peer.last_handshake.map_or(false, |ts| (now - ts) < 180)
        };

        let status_str = if !peer.enabled {
            "disabled"
        } else if is_handshake_active {
            "online"
        } else {
            "offline"
        };

        nodes.push(TopologyNode {
            id: peer.id.clone(),
            label: peer.name.clone(),
            node_type: n_type.to_string(),
            status: status_str.to_string(),
            details: peer.allowed_ips.clone(),
        });

        links.push(TopologyLink {
            source: iface.id.clone(),
            target: peer.id.clone(),
            label: peer.allowed_ips.clone(),
        });
    }

    Ok(Json(TopologyData { nodes, links }))
}

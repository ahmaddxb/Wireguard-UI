use crate::models::{AuditLog, PeerType, WgInterface, WgPeer};
use rusqlite::{params, Connection, Result};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct DbStore {
    conn: Arc<Mutex<Connection>>,
}

impl DbStore {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_tables()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_tables()?;
        Ok(store)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "CREATE TABLE IF NOT EXISTS interfaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                private_key TEXT NOT NULL,
                public_key TEXT NOT NULL,
                listen_port INTEGER NOT NULL,
                address_pool TEXT NOT NULL,
                dns TEXT NOT NULL,
                post_up TEXT NOT NULL,
                post_down TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );",
            [],
        )?;

        let _ = conn.execute("ALTER TABLE interfaces ADD COLUMN webhook_url TEXT;", []);
        let _ = conn.execute("ALTER TABLE interfaces ADD COLUMN webhook_token TEXT;", []);
        let _ = conn.execute("ALTER TABLE interfaces ADD COLUMN public_key_override TEXT;", []);
        let _ = conn.execute("ALTER TABLE interfaces ADD COLUMN gateway_endpoint TEXT;", []);
        let _ = conn.execute("ALTER TABLE interfaces ADD COLUMN hub_subnets TEXT;", []);
        let _ = conn.execute("ALTER TABLE interfaces ADD COLUMN mtu INTEGER DEFAULT 1420;", []);
        let _ = conn.execute("ALTER TABLE interfaces ADD COLUMN mss_clamping INTEGER DEFAULT 1;", []);
        let _ = conn.execute("ALTER TABLE interfaces ADD COLUMN address_pool_v6 TEXT;", []);
        let _ = conn.execute("ALTER TABLE interfaces ADD COLUMN dns_v6 TEXT;", []);

        conn.execute(
            "CREATE TABLE IF NOT EXISTS peers (
                id TEXT PRIMARY KEY,
                interface_id TEXT NOT NULL,
                name TEXT NOT NULL,
                peer_type TEXT NOT NULL,
                public_key TEXT NOT NULL,
                private_key TEXT,
                preshared_key TEXT,
                allowed_ips TEXT NOT NULL,
                endpoint TEXT,
                persistent_keepalive INTEGER NOT NULL,
                enabled INTEGER NOT NULL,
                notes TEXT,
                created_at INTEGER NOT NULL,
                last_handshake INTEGER,
                bytes_rcvd INTEGER DEFAULT 0,
                bytes_sent INTEGER DEFAULT 0
            );",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS audit_logs (
                id TEXT PRIMARY KEY,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                details TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            );",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS bandwidth_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                rx_speed_bps INTEGER NOT NULL,
                tx_speed_bps INTEGER NOT NULL,
                total_rx_bytes INTEGER NOT NULL,
                total_tx_bytes INTEGER NOT NULL
            );",
            [],
        )?;

        Ok(())
    }

    pub fn get_interfaces(&self) -> Result<Vec<WgInterface>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, private_key, public_key, listen_port, address_pool, dns, post_up, post_down, enabled, created_at, webhook_url, webhook_token, gateway_endpoint, hub_subnets, mtu, mss_clamping, address_pool_v6, dns_v6 FROM interfaces ORDER BY id ASC;"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WgInterface {
                id: row.get(0)?,
                name: row.get(1)?,
                private_key: row.get(2)?,
                public_key: row.get(3)?,
                listen_port: row.get(4)?,
                address_pool: row.get(5)?,
                dns: row.get(6)?,
                post_up: row.get(7)?,
                post_down: row.get(8)?,
                enabled: row.get::<_, i32>(9)? != 0,
                created_at: row.get(10)?,
                webhook_url: row.get(11).ok(),
                webhook_token: row.get(12).ok(),
                gateway_endpoint: row.get(13).ok(),
                hub_subnets: row.get(14).ok(),
                mtu: row.get::<_, Option<u32>>(15).unwrap_or(Some(1420)),
                mss_clamping: row.get::<_, Option<i32>>(16).unwrap_or(Some(1)).unwrap_or(1) != 0,
                address_pool_v6: row.get(17).ok(),
                dns_v6: row.get(18).ok(),
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_interface_by_id(&self, id: &str) -> Result<Option<WgInterface>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, private_key, public_key, listen_port, address_pool, dns, post_up, post_down, enabled, created_at, webhook_url, webhook_token, gateway_endpoint, hub_subnets, mtu, mss_clamping, address_pool_v6, dns_v6 FROM interfaces WHERE id = ?1 LIMIT 1;"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(WgInterface {
                id: row.get(0)?,
                name: row.get(1)?,
                private_key: row.get(2)?,
                public_key: row.get(3)?,
                listen_port: row.get(4)?,
                address_pool: row.get(5)?,
                dns: row.get(6)?,
                post_up: row.get(7)?,
                post_down: row.get(8)?,
                enabled: row.get::<_, i32>(9)? != 0,
                created_at: row.get(10)?,
                webhook_url: row.get(11).ok(),
                webhook_token: row.get(12).ok(),
                gateway_endpoint: row.get(13).ok(),
                hub_subnets: row.get(14).ok(),
                mtu: row.get::<_, Option<u32>>(15).unwrap_or(Some(1420)),
                mss_clamping: row.get::<_, Option<i32>>(16).unwrap_or(Some(1)).unwrap_or(1) != 0,
                address_pool_v6: row.get(17).ok(),
                dns_v6: row.get(18).ok(),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_or_create_default_interface(&self) -> Result<WgInterface> {
        if let Ok(interfaces) = self.get_interfaces() {
            if let Some(first) = interfaces.into_iter().next() {
                return Ok(first);
            }
        }

        let conn = self.conn.lock().unwrap();

        // Check if an existing /etc/wireguard/wg0.conf file exists locally or remotely via SSH if enabled
        let conf_content = std::fs::read_to_string("/etc/wireguard/wg0.conf").or_else(|_| {
            if crate::wg_engine::is_ssh_sync_enabled() {
                let target = crate::wg_engine::get_ssh_target();
                let output = std::process::Command::new("ssh")
                    .args(crate::wg_engine::ssh_cmd_args(&target, "cat /etc/wireguard/wg0.conf"))
                    .output();
                match output {
                    Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
                    _ => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "No remote conf")),
                }
            } else {
                Err(std::io::Error::new(std::io::ErrorKind::NotFound, "SSH sync disabled"))
            }
        });

        if let Ok(existing_conf) = conf_content {
            if let Some(parsed) = crate::wg_engine::parse_existing_wg_conf(&existing_conf) {
                let iface = parsed.interface;
                conn.execute(
                    "INSERT INTO interfaces (id, name, private_key, public_key, listen_port, address_pool, dns, post_up, post_down, enabled, created_at, webhook_url, webhook_token, mtu, mss_clamping, address_pool_v6, dns_v6)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17);",
                    params![
                        iface.id,
                        iface.name,
                        iface.private_key,
                        iface.public_key,
                        iface.listen_port,
                        iface.address_pool,
                        iface.dns,
                        iface.post_up,
                        iface.post_down,
                        iface.enabled as i32,
                        iface.created_at,
                        iface.webhook_url,
                        iface.webhook_token,
                        iface.mtu,
                        iface.mss_clamping as i32,
                        iface.address_pool_v6,
                        iface.dns_v6
                    ],
                )?;

                for peer in parsed.peers {
                    let _ = conn.execute(
                        "INSERT OR REPLACE INTO peers (id, interface_id, name, peer_type, public_key, private_key, preshared_key, allowed_ips, endpoint, persistent_keepalive, enabled, notes, created_at, last_handshake, bytes_rcvd, bytes_sent)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16);",
                        params![
                            peer.id,
                            peer.interface_id,
                            peer.name,
                            peer.peer_type.to_string(),
                            peer.public_key,
                            peer.private_key,
                            peer.preshared_key,
                            peer.allowed_ips,
                            peer.endpoint,
                            peer.persistent_keepalive,
                            peer.enabled as i32,
                            peer.notes,
                            peer.created_at,
                            peer.last_handshake,
                            peer.bytes_rcvd as i64,
                            peer.bytes_sent as i64
                        ],
                    )?;
                }

                return Ok(iface);
            }
        }

        // Generate keypair for initial default wg0 interface
        let (priv_key, pub_key) = crate::wg_engine::generate_keypair();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let iface = WgInterface {
            id: "wg0".to_string(),
            name: "wg0".to_string(),
            private_key: priv_key,
            public_key: pub_key,
            listen_port: 51820,
            address_pool: "10.8.0.1/24".to_string(),
            dns: "1.1.1.1, 8.8.8.8".to_string(),
            post_up: "iptables -A FORWARD -i %i -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE".to_string(),
            post_down: "iptables -D FORWARD -i %i -j ACCEPT; iptables -t nat -D POSTROUTING -o eth0 -j MASQUERADE".to_string(),
            webhook_url: std::env::var("WEBHOOK_URL").ok(),
            webhook_token: std::env::var("WEBHOOK_TOKEN").ok(),
            gateway_endpoint: None,
            hub_subnets: None,
            mtu: Some(1420),
            mss_clamping: true,
            address_pool_v6: None,
            dns_v6: None,
            enabled: true,
            created_at: now,
        };

        conn.execute(
            "INSERT INTO interfaces (id, name, private_key, public_key, listen_port, address_pool, dns, post_up, post_down, enabled, created_at, webhook_url, webhook_token, mtu, mss_clamping, address_pool_v6, dns_v6)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17);",
            params![
                iface.id,
                iface.name,
                iface.private_key,
                iface.public_key,
                iface.listen_port,
                iface.address_pool,
                iface.dns,
                iface.post_up,
                iface.post_down,
                iface.enabled as i32,
                iface.created_at,
                iface.webhook_url,
                iface.webhook_token,
                iface.mtu,
                iface.mss_clamping as i32,
                iface.address_pool_v6,
                iface.dns_v6
            ],
        )?;

        Ok(iface)
    }

    pub fn save_interface(&self, iface: &WgInterface) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO interfaces (id, name, private_key, public_key, listen_port, address_pool, dns, post_up, post_down, enabled, created_at, webhook_url, webhook_token, gateway_endpoint, hub_subnets, mtu, mss_clamping, address_pool_v6, dns_v6)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19);",
            params![
                iface.id,
                iface.name,
                iface.private_key,
                iface.public_key,
                iface.listen_port,
                iface.address_pool,
                iface.dns,
                iface.post_up,
                iface.post_down,
                iface.enabled as i32,
                iface.created_at,
                iface.webhook_url,
                iface.webhook_token,
                iface.gateway_endpoint,
                iface.hub_subnets,
                iface.mtu,
                iface.mss_clamping as i32,
                iface.address_pool_v6,
                iface.dns_v6
            ],
        )?;
        Ok(())
    }

    pub fn delete_interface(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM interfaces WHERE id = ?1;", params![id])?;
        conn.execute("DELETE FROM peers WHERE interface_id = ?1;", params![id])?;
        Ok(())
    }

    pub fn update_interface(&self, iface: &WgInterface) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE interfaces SET name=?1, listen_port=?2, address_pool=?3, dns=?4, post_up=?5, post_down=?6, enabled=?7, webhook_url=?8, webhook_token=?9, public_key=?11, gateway_endpoint=?12, hub_subnets=?13, mtu=?14, mss_clamping=?15, address_pool_v6=?16, dns_v6=?17 WHERE id=?10;",
            params![
                iface.name,
                iface.listen_port,
                iface.address_pool,
                iface.dns,
                iface.post_up,
                iface.post_down,
                iface.enabled as i32,
                iface.webhook_url,
                iface.webhook_token,
                iface.id,
                iface.public_key,
                iface.gateway_endpoint,
                iface.hub_subnets,
                iface.mtu,
                iface.mss_clamping as i32,
                iface.address_pool_v6,
                iface.dns_v6
            ],
        )?;
        Ok(())
    }

    pub fn get_peers_by_interface(&self, iface_id: &str) -> Result<Vec<WgPeer>> {
        let all = self.get_peers()?;
        Ok(all.into_iter().filter(|p| p.interface_id == iface_id).collect())
    }

    pub fn get_peers(&self) -> Result<Vec<WgPeer>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, interface_id, name, peer_type, public_key, private_key, preshared_key, allowed_ips, endpoint, persistent_keepalive, enabled, notes, created_at, last_handshake, bytes_rcvd, bytes_sent FROM peers ORDER BY created_at DESC;"
        )?;

        let peer_iter = stmt.query_map([], |row| {
            let p_type_str: String = row.get(3)?;
            let p_type = p_type_str.parse::<PeerType>().unwrap_or(PeerType::Client);
            Ok(WgPeer {
                id: row.get(0)?,
                interface_id: row.get(1)?,
                name: row.get(2)?,
                peer_type: p_type,
                public_key: row.get(4)?,
                private_key: row.get(5)?,
                preshared_key: row.get(6)?,
                allowed_ips: row.get(7)?,
                endpoint: row.get(8)?,
                persistent_keepalive: row.get(9)?,
                enabled: row.get::<_, i32>(10)? != 0,
                notes: row.get(11)?,
                created_at: row.get(12)?,
                last_handshake: row.get(13)?,
                bytes_rcvd: row.get::<_, Option<i64>>(14)?.unwrap_or(0) as u64,
                bytes_sent: row.get::<_, Option<i64>>(15)?.unwrap_or(0) as u64,
                rx_speed_bps: 0,
                tx_speed_bps: 0,
                ping_latency_ms: None,
            })
        })?;

        let mut peers = Vec::new();
        for peer in peer_iter {
            peers.push(peer?);
        }
        Ok(peers)
    }

    pub fn get_peer_by_id(&self, id: &str) -> Result<Option<WgPeer>> {
        let peers = self.get_peers()?;
        Ok(peers.into_iter().find(|p| p.id == id))
    }

    pub fn save_peer(&self, peer: &WgPeer) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO peers (id, interface_id, name, peer_type, public_key, private_key, preshared_key, allowed_ips, endpoint, persistent_keepalive, enabled, notes, created_at, last_handshake, bytes_rcvd, bytes_sent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16);",
            params![
                peer.id,
                peer.interface_id,
                peer.name,
                peer.peer_type.to_string(),
                peer.public_key,
                peer.private_key,
                peer.preshared_key,
                peer.allowed_ips,
                peer.endpoint,
                peer.persistent_keepalive,
                peer.enabled as i32,
                peer.notes,
                peer.created_at,
                peer.last_handshake,
                peer.bytes_rcvd as i64,
                peer.bytes_sent as i64
            ],
        )?;
        Ok(())
    }

    pub fn delete_peer(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM peers WHERE id = ?1;", params![id])?;
        Ok(())
    }

    pub fn toggle_peer_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE peers SET enabled = ?1 WHERE id = ?2;",
            params![enabled as i32, id],
        )?;
        Ok(())
    }

    pub fn add_audit_log(&self, action: &str, target: &str, details: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let id = format!("log-{}", rand::random::<u32>());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO audit_logs (id, action, target, details, timestamp) VALUES (?1, ?2, ?3, ?4, ?5);",
            params![id, action, target, details, now],
        )?;
        Ok(())
    }

    pub fn get_audit_logs(&self, limit: usize) -> Result<Vec<AuditLog>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, action, target, details, timestamp FROM audit_logs ORDER BY timestamp DESC LIMIT ?1;"
        )?;
        let logs_iter = stmt.query_map(params![limit as i64], |row| {
            Ok(AuditLog {
                id: row.get(0)?,
                action: row.get(1)?,
                target: row.get(2)?,
                details: row.get(3)?,
                timestamp: row.get(4)?,
            })
        })?;

        let mut logs = Vec::new();
        for log in logs_iter {
            logs.push(log?);
        }
        Ok(logs)
    }

    pub fn save_bandwidth_sample(&self, rx_sp: u64, tx_sp: u64, rx_bytes: u64, tx_bytes: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        conn.execute(
            "INSERT INTO bandwidth_history (timestamp, rx_speed_bps, tx_speed_bps, total_rx_bytes, total_tx_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params![now, rx_sp as i64, tx_sp as i64, rx_bytes as i64, tx_bytes as i64],
        )?;

        // Prune entries older than 24 hours
        let cutoff = now - 86400;
        let _ = conn.execute("DELETE FROM bandwidth_history WHERE timestamp < ?1;", params![cutoff]);

        Ok(())
    }

    pub fn get_bandwidth_history(&self, limit: usize) -> Result<Vec<crate::models::BandwidthSample>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT timestamp, rx_speed_bps, tx_speed_bps, total_rx_bytes, total_tx_bytes
             FROM bandwidth_history ORDER BY id DESC LIMIT ?1;",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            let ts: i64 = row.get(0)?;
            let rx_sp: i64 = row.get(1)?;
            let tx_sp: i64 = row.get(2)?;
            let rx_b: i64 = row.get(3)?;
            let tx_b: i64 = row.get(4)?;

            Ok(crate::models::BandwidthSample {
                timestamp: ts,
                rx_speed_bps: rx_sp as u64,
                tx_speed_bps: tx_sp as u64,
                total_rx_bytes: rx_b as u64,
                total_tx_bytes: tx_b as u64,
            })
        })?;

        let mut samples = Vec::new();
        for r in rows {
            samples.push(r?);
        }
        samples.reverse();
        Ok(samples)
    }

    pub fn prune_bandwidth_history(&self, older_than_seconds: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let cutoff = now - older_than_seconds;
        let deleted = conn.execute("DELETE FROM bandwidth_history WHERE timestamp < ?1;", params![cutoff])?;
        Ok(deleted)
    }

    pub fn get_db_stats(&self) -> Result<(u64, u64)> {
        let conn = self.conn.lock().unwrap();
        let samples_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM bandwidth_history;", [], |r| r.get(0))
            .unwrap_or(0);
        let audit_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_logs;", [], |r| r.get(0))
            .unwrap_or(0);
        Ok((samples_count as u64, audit_count as u64))
    }
}

# ⚡ WireGuard WebUI (Rust + Single Binary + Docker)

A high-performance, ultra-low footprint WireGuard management portal built with **Rust (Axum + Tokio + SQLite)** and an embedded **Single-Page Application (SPA)** dashboard.

Designed for modern home labs, Proxmox VE hypervisors, cloud VPS instances, and enterprise site-to-site VPN networks.

---

## 🌟 Key Features

- **🚀 Ultra-Low Footprint**: ~6.1 MB single binary, ~18 MB Docker image, consuming under **8 MB of RAM**.
- **⚡ Sub-Millisecond Telemetry Engine**: Asynchronous in-memory RAM caching serving `/api/overview`, `/api/peers`, and `/api/topology` in **0.8 ms** with zero UI lag.
- **🛡️ Multi-Interface Architecture**: Run multiple isolated interfaces concurrently (e.g. `wg0` roadwarrior on port 51821, `wg1` mesh on port 51850) with instant 0ms header tab switching.
- **⚡ Zero-Downtime Kernel Sync**: Dynamically syncs WireGuard configurations (`/etc/wireguard/*.conf`) live using `wg syncconf` and `wg set` without dropping active tunnels or restarting interfaces (even when deleting or updating Pre-Shared Keys).
- **🌐 Site-to-Site Auto-Setup Assistant**: 1-line auto-install scripts pre-detect egress interfaces (`enp0s3`), pre-allocate the next free Hub tunnel IP (`10.100.90.x/32`), and dynamically format `SUBNETS:` output.
- **📶 Full IPv6 Dual-Stack & MTU Clamping**: Automated IPv4/IPv6 CIDR pool allocation (`fd00:90::/64`), configurable MTU (e.g. `1280` for mobile 4G/5G compatibility), and automated TCP MSS clamping (`iptables -t mangle`).
- **🔍 Gateway Endpoint & Public Key Manager**: Configure the Hub Gateway Public Key and Public Endpoint directly from the UI with 1-click IP Auto-Detection.
- **📊 Interactive 24-Hour Bandwidth Graphs**: SQLite-backed periodic 60s sampling rendering smooth SVG gradient curves for Inbound (Rx - Emerald Green) and Outbound (Tx - Vibrant Purple) throughput. Features auto-scaled Y-axis reference grid lines, real-time Peak Rx/Tx telemetry badges, and an interactive crosshair tooltip with edge-aware coordinate clamping displaying exact speeds and elapsed timestamps.
- **🟢 Live Ping Latency & Telemetry**: Multi-interface telemetry via `wg show all dump` with intelligent RTT caching (`🟢 4 ms`, `🟡 45 ms`, `🔴 Timeout`) measured directly over active tunnels.
- **🔔 Webhook & Ntfy Offline Alerts**: Instant 24/7 background push notifications via Ntfy, Telegram, Discord, or custom HTTP webhooks with Bearer token authentication when tunnels go offline or reconnect.
- **📋 One-Click Copy & Batch Client Export**: Copy raw client `.conf` files directly to clipboard (with HTTP cross-browser fallback) or export all client configurations in one batch.
- **📱 Native Curve25519 Cryptography & QR Codes**: Native `wireguard-tools` keypair generation (`wg genkey`, `wg pubkey`), optional Pre-Shared Keys (`wg genpsk`), and instant QR code profiles for mobile devices.
- **🗺️ Visual Network Topology Map**: Interactive SVG graph rendering hub-and-spoke topologies across all connected gateways and client devices.
- **🔒 Proxmox VE & OpenSSH Multiplexing**: Remote hypervisor telemetry using persistent OpenSSH `ControlMaster` sockets (~10ms round-trip) so stats inside LXC containers match hypervisor host kernel stats 1:1 with containerized security.
- **🛡️ Audit Logging & Security**: Persistent SQLite audit logging for every peer addition, edit, toggle, notification test, and system event.

---

## 📁 Directory Structure

```text
Wireguard-UI/
├── .agents/
│   └── AGENTS.md               # Architecture manual & AI agent guidelines
├── build/
│   └── wireguard-ui            # Compiled standalone Linux x86_64 binary (~6.1 MB)
├── src/                        # Rust backend source code
│   ├── main.rs                 # Axum REST API routes & startup engine
│   ├── models.rs               # Interface, Peer, Overview, Bandwidth & Topology structs
│   ├── store.rs                # SQLite persistence layer, history table & auto-importer
│   ├── wg_engine.rs            # Curve25519 keys, QR rendering, ping, webhooks & wg syncconf
│   └── embedded_ui.rs          # Static web asset embedding (rust-embed)
├── ui/                         # Single-Page Web App source files
│   ├── index.html              # Glassmorphism HTML dashboard & bandwidth chart SVG
│   ├── style.css              # Dark theme CSS stylesheet & metric badges
│   └── app.js                 # Vanilla JS telemetry controller & clipboard handler
├── Dockerfile                  # Multi-stage Rust build -> Alpine runtime
├── docker-compose.yml          # Production Docker Compose specification
└── Cargo.toml                  # Rust project manifest & dependencies
```

---

## 🚀 Deployment Option 1: Docker Compose (Recommended)

### 1. `docker-compose.yml` Configuration

```yaml
services:
  wireguard-ui:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: wireguard-ui
    restart: unless-stopped
    network_mode: host # Allows container to bind to host interface ports
    cap_add:
      - NET_ADMIN
      - SYS_MODULE
    volumes:
      # SQLite database persistence
      - ./wireguard_ui.db:/app/wireguard_ui.db
      # Server WireGuard configuration directory
      - /etc/wireguard:/etc/wireguard
      # Kernel modules directory
      - /lib/modules:/lib/modules
      # Passwordless SSH keys for remote Proxmox Host orchestration (Read-Only)
      - /root/.ssh:/root/.ssh:ro
    environment:
      # Web portal port
      - PORT=8085
      # Logging level
      - RUST_LOG=wireguard_ui=info
      # Remote SSH Synchronization:
      # Set ENABLE_SSH_SYNC=true to control a remote Proxmox Host over SSH.
      # Set ENABLE_SSH_SYNC=false when running natively on Proxmox VE or local Linux host.
      - ENABLE_SSH_SYNC=true
      # Target Proxmox VE Hypervisor IP address
      - SSH_HOST=192.168.1.101
      # Remote SSH user
      - SSH_USER=root
      # Optional Alert Webhook URL & Token (Ntfy / Telegram / Discord):
      - WEBHOOK_URL=https://ntfy.sh/my_wireguard_alerts
      - WEBHOOK_TOKEN=tk_1a2b3c4d5e...
```

### 2. Launch the Container

```bash
docker compose up -d --build
```

Access the dashboard at: **`http://<YOUR_SERVER_IP>:8085`**

---

## 📦 Deployment Option 2: Single Standalone Binary

The standalone compiled binary is saved directly at [`build/wireguard-ui`](file:///z:/Github/Wireguard-UI/build/wireguard-ui).

### 1. Run Directly from Terminal

```bash
# Make executable
chmod +x build/wireguard-ui

# Launch binary
PORT=8085 WEBHOOK_URL=https://ntfy.sh/my_alerts ./build/wireguard-ui
```

---

### 2. Install as a `systemd` Service (Proxmox VE / Debian / Ubuntu)

To run `wireguard-ui` automatically on system boot:

1. **Install Binary**:
   ```bash
   cp build/wireguard-ui /usr/local/bin/wireguard-ui
   chmod +x /usr/local/bin/wireguard-ui
   ```

2. **Create Service Unit File** (`/etc/systemd/system/wireguard-ui.service`):
   ```ini
   [Unit]
   Description=WireGuard WebUI Management Portal
   After=network.target wireguard.service

   [Service]
   Type=simple
   WorkingDirectory=/etc/wireguard
   ExecStart=/usr/local/bin/wireguard-ui
   Restart=always
   RestartSec=5
   Environment=PORT=8085
   Environment=RUST_LOG=wireguard_ui=info
   Environment=WEBHOOK_URL=https://ntfy.sh/my_wireguard_alerts
   Environment=WEBHOOK_TOKEN=tk_1a2b3c4d5e...

   [Install]
   WantedBy=multi-user.target
   ```

3. **Enable and Start Service**:
   ```bash
   systemctl daemon-reload
   systemctl enable --now wireguard-ui
   ```

---

## ⚙️ Environment Variables

| Variable | Default | Description |
| :--- | :--- | :--- |
| `PORT` | `8085` | HTTP web portal listening port |
| `ENABLE_SSH_SYNC` | `false` | Enable/Disable remote SSH synchronization (`true`/`false`) |
| `SSH_HOST` | `192.168.1.101` | Remote host IP/hostname for SSH sync |
| `SSH_USER` | `root` | Remote SSH username |
| `WEBHOOK_URL` | *None* | Webhook URL for Ntfy, Telegram, Discord, or custom HTTP alerts |
| `WEBHOOK_TOKEN` | *None* | Bearer Token / Ntfy Auth Token for notification webhooks |
| `RUST_LOG` | `wireguard_ui=info` | Logging verbosity (`trace`, `debug`, `info`, `warn`, `error`, `off`) |
| `DB_PATH` | `wireguard_ui.db` | Path to SQLite database file |
| `WG_CONF_PATH` | `/etc/wireguard/wg0.conf` | Path to server WireGuard configuration file |

### 📊 `RUST_LOG` Verbosity Levels

| Level | Value | Description |
| :--- | :--- | :--- |
| **`trace`** | `wireguard_ui=trace` | Ultra-verbose execution traces (database parameters, internal loops). |
| **`debug`** | `wireguard_ui=debug` | Detailed troubleshooting logs (SSH outputs, ping RTTs, webhook payloads). |
| **`info`** *(Default)* | `wireguard_ui=info` | Normal operational logging (startup, peer creation, status toggles). |
| **`warn`** | `wireguard_ui=warn` | Minor warnings only (ping timeouts, non-fatal webhook failures). |
| **`error`** | `wireguard_ui=error` | Critical errors only (database connection failures, port bind errors). |
| **`off`** | `off` | Completely disables all logging output. |

#### Useful Logging Examples:
- **App Debug Mode (Recommended for troubleshooting)**:
  `RUST_LOG=wireguard_ui=debug ./wireguard-ui`
- **Target Specific Submodule (e.g. Engine or Webhooks)**:
  `RUST_LOG=wireguard_ui::wg_engine=debug,wireguard_ui=info`
- **Full Debug Mode (Includes HTTP web framework requests)**:
  `RUST_LOG=debug`

---

## ⚡ Architecture & Performance: LXC Container vs. Proxmox Host

WireGuard WebUI utilizes an **Asynchronous In-Memory Telemetry Cache** paired with **OpenSSH Connection Multiplexing (`ControlMaster`)**. This delivers bare-metal sub-millisecond dashboard speed while preserving full container isolation.

### 📊 Performance & Security Comparison

| Dimension / Metric | 🐳 LXC Container (Default / Recommended) | 🖥️ Bare-Metal Proxmox Host | Verdict |
| :--- | :--- | :--- | :--- |
| **API Response Time (`/api/overview`)** | **~0.8 ms** *(Sub-millisecond from RAM)* | **~0.8 ms** *(Sub-millisecond from RAM)* | 🤝 **Tie (Identical)** — Zero perceptible difference; served from memory cache. |
| **Background Kernel Polling (`wg show all dump`)** | **~10–20 ms** *(Over persistent SSH ControlMaster socket)* | **~2–5 ms** *(Local subprocess execution)* | ⚡ **Negligible** — Runs in background Tokio worker every 2s, completely invisible to the UI. |
| **Config Sync Latency (`wg syncconf` on save)** | **~25–40 ms** *(Multiplexed SSH)* | **~5–10 ms** *(Local shell)* | ⚡ **Imperceptible** — Applying peer changes takes under 0.05s in both modes. |
| **Security & Blast Radius** | 🟢 **High Isolation**: Web portal and SQLite database are isolated in an unprivileged/isolated container. | 🔴 **Hypervisor Risk**: Web server runs as `root` directly on the hypervisor hosting all VMs, storage pools, and cluster networks. | 🏆 **LXC Container** |
| **Hypervisor Host Cleanliness** | 🟢 **Zero host clutter**: No custom systemd units, SQLite DBs, or non-PVE ports on PVE root filesystem. | 🟠 **Host Clutter**: Custom systemd unit, SQLite database file, and logs placed on hypervisor storage. | 🏆 **LXC Container** |
| **Backups & Disaster Recovery** | 🟢 **1-Click PBS Backup**: Fully snapshotted & restored with Proxmox Backup Server. | 🟠 **Manual**: Host configs must be manually tracked outside PVE backup routines. | 🏆 **LXC Container** |

### 🚀 How the Sub-Millisecond Telemetry Engine Works

1. **In-Memory RAM Cache (0ms REST latency)**:
   A dedicated Tokio background worker refreshes peer speeds, transfer bytes, and ICMP ping latencies every **2 seconds** in memory. Web UI requests (`/api/overview`, `/api/peers`, `/api/topology`) read directly from RAM in **0.8ms** without waiting on remote or local shell executions.

2. **OpenSSH Connection Multiplexing (`ControlMaster`)**:
   When communicating with the Proxmox host (`192.168.1.101`), OpenSSH maintains a persistent background UNIX control socket (`/tmp/ssh_mux_*`) with `ControlPersist=10m`. Subsequent kernel queries skip the TCP 3-way handshake and SSH cryptographic key exchange, dropping SSH round-trip latency from **~250ms down to ~10ms**.

3. **Autonomous 24/7 Webhook Worker**:
   Tunnel status transitions (Online 🟢 / Offline 🔴) are evaluated against kernel handshakes in real time. Alerts are dispatched with automatic exponential backoff to prevent webhook spam.

4. **Strict Remote vs. Local Kernel Isolation**:
   When `ENABLE_SSH_SYNC=true`, the container purely acts as an orchestration control plane and completely bypasses local `wg-quick` and `wg syncconf` calls. All WireGuard kernel operations execute exclusively on the remote target (`192.168.1.101`), preventing rogue/duplicate WireGuard interfaces (`wg0`, `wg1`) and listening ports (`51821`, `51850`) from spawning inside the container or LXC host network namespace.

---

## 🔔 Setting Up Notifications (Ntfy / Telegram / Discord)

1. Open **Settings** in the WebUI.
2. Enter your **Alert Webhook URL** (e.g. `https://ntfy.sh/my_wireguard_alerts`).
3. Enter your **Auth Token** if required by your Ntfy server.
4. Click **Save Interface Settings**.
5. Click **`🔔 Send Test Notification`** to test your setup!

---

## 🛠️ Building from Source

To compile a fresh standalone binary from source into `build/`:

```bash
# Using Cargo (requires Rust toolchain)
cargo build --release
cp target/release/wireguard-ui build/wireguard-ui

# Or using Docker Builder (produces clean Linux x86_64 binary)
docker build --target builder -t wg-builder .
docker run --rm -v $(pwd)/build:/out wg-builder cp /usr/src/wireguard-ui/target/release/wireguard-ui /out/wireguard-ui
```

---

## 📡 REST API Reference

### 🛡️ Interfaces & Overview
| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/api/interfaces` | List all configured WireGuard interfaces (`wg0`, `wg1`, etc.) |
| `POST` | `/api/interfaces` | Create a new isolated WireGuard interface |
| `PUT` | `/api/interfaces/:id` | Update interface settings (port, pool, MTU, MSS clamping, scripts) |
| `DELETE` | `/api/interfaces/:id` | Delete an interface and its associated peers |
| `GET` | `/api/interfaces/:id/overview` | Fetch interface-specific telemetry, totals & live speeds |
| `GET` | `/api/interfaces/:id/export` | Download `<interface>.conf` server configuration file |
| `GET` | `/api/config/export` | Export server configuration (accepts optional `?interface_id=<id>` or `?id=<id>`) |
| `GET` | `/api/overview` | Global active interface overview, client counts & speeds |

### 👥 Peers & Tunnels
| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/api/peers` | List all peers enriched with live traffic telemetry & ping latencies |
| `POST` | `/api/peers/client` | Create new client (auto IP allocation + keypair) |
| `POST` | `/api/peers/site` | Establish new Site-to-Site subnet route |
| `PUT` | `/api/peers/:id` | Update peer configuration / subnet routes |
| `POST` | `/api/peers/:id/toggle` | Enable / Disable peer & hot-reload kernel live |
| `DELETE` | `/api/peers/:id` | Remove peer & purge kernel route |
| `GET` | `/api/peers/:id/qr` | Get QR Code SVG data URI & config download |
| `GET` | `/api/peers/site/probe-command` | Generate dynamic 1-line auto-install script with pre-allocated IP |

### 📊 Analytics, Diagnostics & System
| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/api/analytics/bandwidth` | Fetch 24-hour historical bandwidth telemetry samples |
| `GET` | `/api/topology` | Fetch SVG visual network topology nodes & links |
| `POST` | `/api/notifications/test` | Trigger a test alert to the configured Webhook URL |
| `GET` | `/api/logs` | Fetch system audit log history |

---

## 📜 License

MIT License. Free for personal, home lab, and commercial deployments.

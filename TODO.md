# 📋 WireGuard WebUI - Project Roadmap & TODO

This document tracks upcoming features, architectural improvements, and planned enhancements for **WireGuard WebUI**.

---

## 🔐 1. Authentication & Access Control

- [ ] **Admin Authentication**:
  - Optional password/token protection for WebUI (`AUTH_PASSWORD` or `SESSION_SECRET`).
  - Session cookie or Bearer token support.
- [ ] **Role-Based Access Control (RBAC)**:
  - `Admin`: Full control (Interface, Peers, Engine, Config sync).
  - `Viewer / Read-Only`: Dashboard telemetry & topology view without modification rights.
  - `User Portal`: Self-service portal for users to view/download only their assigned peer config & QR code.
- [ ] **SSO / OIDC Integration**:
  - Support OAuth2/OIDC providers (Authentik, Authelia, Keycloak, GitHub, Google).
  - Reverse proxy header authentication (`X-Forwarded-User`, `Remote-User`).

---

## 👥 2. Peer & Client Management

- [ ] **Peer Expiration & Time-Limited Access**:
  - Set optional expiry date/time per peer (auto-disable expired clients).
  - Guest/temporary access tunnels.
- [ ] **Custom DNS & Routing Presets per Peer**:
  - Override interface-level DNS with client-specific DNS servers (e.g., AdGuard, Pi-hole).
  - Config preset selector: **Full Tunnel** (`0.0.0.0/0, ::/0`) vs. **Split Tunnel** (LAN subnets only).
- [ ] **Peer Grouping & Tags**:
  - Tagging system (e.g., `#homelab`, `#mobile`, `#office`, `#iot`).
  - Filter and bulk-manage peers by tag.
- [x] **Client Keypair & PSK Management**:
  - [x] Native `wireguard-tools` Curve25519 keypair generation (`wg genkey`, `wg pubkey`).
  - [x] Optional Pre-Shared Keys (`wg genpsk`) with zero-downtime PSK removal (`/dev/null`).
  - [x] Custom client public key input and live Edit Client modal.
- [x] **Interface Selector in Modals**:
  - [x] Target interface dropdown selector in Add Client Peer and Add Site Tunnel modals.
  - [x] Auto-switching to target interface upon creation.
- [ ] **Bulk Operations & Batch Export**:
  - Multi-select peers for batch enable, disable, or delete.
  - Download all active peer configs as a single `.zip` bundle.
  - Direct 1-click QR code image download (`.png` / `.svg`) in UI.

---

## 🌐 3. Multi-Interface & Advanced Networking

- [x] **Multiple WireGuard Interfaces**:
  - [x] Support multiple concurrent interfaces (e.g., `wg0` on port 51821, `wg1` on port 51850).
  - [x] Interface switcher tabs in WebUI header with instant 0ms cached rendering.
  - [x] Multi-interface kernel telemetry querying `wg show all dump`.
  - [x] Per-interface server config export (`GET /api/interfaces/:id/export` & dynamic `Export <id>.conf` button).
- [x] **Full IPv6 Dual-Stack Support**:
  - [x] IPv6 CIDR pool allocation (e.g., `fd00:90::/64`).
  - [x] Dual-stack client configuration templates (`Address = 10.100.90.2/32, fd00:90::2/128`).
- [x] **MTU & MSS Clamping Controls**:
  - [x] Configurable MTU per interface / peer (e.g. `1280` for mobile 4G/5G compatibility).
  - [x] Automated TCP MSS clamping in `PostUp` firewall rules (`iptables -t mangle ... TCPMSS --clamp-mss-to-pmtu`).

---

## 📊 4. Telemetry, Analytics & Monitoring

- [ ] **Prometheus Metrics Endpoint**:
  - Expose `/api/metrics` (or `/metrics`) for Prometheus scraping and Grafana dashboards.
  - Metrics: Total Rx/Tx bytes, active peer count, handshake timestamps, ping RTTs.
- [ ] **Per-Peer Historical Bandwidth & Quotas**:
  - Hourly / Daily / Monthly bandwidth breakdown per individual peer.
  - Optional monthly transfer quota limits (with alert/auto-disable on threshold).
- [ ] **Advanced Ping & Health Telemetry**:
  - Packet loss percentage and jitter tracking in addition to RTT latency.
  - Configurable peer dead-man alert if no handshake seen for > *X* minutes.

---

## 💾 5. Backup, Restore & Migration

- [ ] **1-Click WebUI Backup & Restore**:
  - Export entire database + interface configuration as a `.json` or `.db` snapshot.
  - Restore / import wizard with configuration validation.
- [ ] **Config Importer**:
  - Import existing `/etc/wireguard/wg0.conf` or configurations from *wg-easy* / *Subspace*.
- [ ] **Scheduled Automated Backups**:
  - Background cron to dump SQLite database to a local backup path or remote WebDAV / S3.

---

## 🎨 6. UI / UX Enhancements

- [x] **Client & Site-to-Site Card Action Revamp**:
  - [x] Streamlined SVG icon buttons with unified small-pill layout.
  - [x] Distinct visual state pills for tunnel status (`🟢 Active` vs `⏸️ Paused`).
  - [x] Compact crimson-hover trash icon button for peer deletion.
- [x] **Modern Button Design System (Linear / Raycast / Vercel Aesthetic)**:
  - [x] Top-edge inner highlight specular lines (`box-shadow: inset 0 1px 0 rgba(255,255,255,0.25)`).
  - [x] Micro-interactions & tactile press scaling (`transform: scale(0.97)` on active click).
  - [x] Interface switcher active glow indicators on `[ 🛡️ wg0 ] [ 🌐 wg1 ]`.
- [x] **Search & Smart Live Filtering & Drag-and-Drop**:
  - [x] Instant search input on Clients and Site-to-Site tabs to filter peers by Name, IP, Notes, or Public Key in real time.
  - [x] Multi-mode sorting (Custom arrangement, Name A-Z/Z-A, Online Status, Ping Latency, Real-Time Speed, Total Traffic).
  - [x] HTML5 drag-and-drop peer card reordering with persistent local arrangement caching.
- [x] **Overview & Bandwidth Chart Enhancements**:
  - [x] Interactive SVG hover crosshair & glassmorphic tooltip showing exact timestamp, elapsed time, and inbound/outbound speeds.
  - [x] Y-axis background reference grid lines with auto-scaled transfer rate units (`B/s`, `KB/s`, `MB/s`, `GB/s`).
  - [x] Real-time Peak Rx and Peak Tx telemetry badges in the chart header.
  - [x] Edge-aware pixel bounding calculation preventing tooltip clipping across all viewport boundaries.
  - [x] Distinct visual styling: Emerald Green (`#10b981`) Inbound (Rx) and Vibrant Purple (`#a855f7`) Outbound (Tx).
- [ ] **Ergonomics & Modal Upgrades**:
  - [ ] Auto-suggest and pre-fill next available IP address in Add Client Peer modal.
  - [ ] Custom glassmorphic confirmation modal with destructive warning prompts (replacing browser `confirm()`).
  - [ ] Click-to-copy micro-tooltips on IP badges throughout tables and cards.
- [ ] **Responsive & Mobile Layout**:
  - [ ] Collapsible sidebar drawer with top hamburger toggle for tablet/mobile devices.
  - [ ] Responsive card grid margins and touch-friendly targets for mobile browsers.
- [ ] **Theme Selector**:
  - [ ] Dark Mode (current default), Cyberpunk/Neon Dark, and Clean Light Mode.
- [ ] **Real-Time WebSockets / SSE**:
  - [ ] Server-Sent Events (SSE) or WebSockets for continuous telemetry streaming.
- [ ] **PWA (Progressive Web App) Support**:
  - [ ] Web app manifest and service worker for mobile home screen installation.

---

## ⚙️ 7. Comprehensive Settings Hub & App Management

- [ ] **Unified Settings Architecture**:
  - [ ] Reorganize sidebar "Interface Settings" and "Notifications & Alerts" into a multi-section **Settings Hub** with sub-tabs / pills:
    - `[ 🛡️ WireGuard Interfaces ]` `[ ⚙️ App Preferences ]` `[ 🖥️ Engine & Diagnostics ]` `[ 🔔 Notifications ]` `[ 💾 Backup & Restore ]`
- [ ] **Interface Settings & Firewall Snippets Upgrade**:
  - [ ] **1-Click Firewall Snippet Buttons** for `PostUp` and `PostDown`:
    - `[ + NAT Masquerade ]` (auto-injects paired `iptables -t nat -A/-D POSTROUTING -o <iface> -j MASQUERADE`).
    - `[ + LAN Bidirectional Forwarding ]` (`iptables -A/-D FORWARD -i %i -j ACCEPT; iptables -A/-D FORWARD -o %i -j ACCEPT`).
    - `[ + Client Isolation ]` (Zero-trust peer separation: `iptables -I/-D FORWARD -i %i -o %i -j DROP`).
    - `[ + TCP MSS Clamping ]` (`iptables -t mangle -A/-D POSTROUTING -p tcp --tcp-flags SYN,RST SYN -o %i -j TCPMSS --clamp-mss-to-pmtu`).
    - Synchronized dual-box injection (appending rule in `PostUp` while automatically generating paired teardown `-D` rule in `PostDown`).
    - Safe "Clear Scripts" reset helper.
  - [ ] **Live IP Pool & CIDR Inspector**:
    - Real-time address pool stats badge (usable IP range, total assignable slots, active peer allocation percentage).
    - Inline input validation for CIDRs, port bounds (1–65535), and Curve25519 44-character Base64 public keys.
  - [ ] **Server Keypair Helper**:
    - "Auto-Derive Public Key" trigger querying interface private key / host kernel.
  - [ ] **Live Server Configuration Preview**:
    - Embedded syntax-highlighted modal/drawer previewing assembled `<iface>.conf` with all associated peers before export.
  - [ ] **Kernel Lifecycle Actions**:
    - "⚡ Force Kernel Sync (`wg syncconf`)" action trigger directly from the interface settings card.
- [ ] **Global App & Client Presets**:
  - [ ] Default Tunnel Mode preset for new clients (**Full Tunnel** `0.0.0.0/0, ::/0` vs **Split Tunnel**).
  - [ ] Default Client MTU (`1420` default, or `1280` for mobile cellular).
  - [ ] Default Client DNS servers (e.g. Pi-hole, AdGuard Home, Cloudflare).
  - [ ] Configurable telemetry polling interval (`1s`, `2s`, `5s`, `10s`).
- [ ] **Hypervisor & Engine Diagnostics**:
  - [ ] Live OpenSSH connection status & multiplexing socket check (`/tmp/ssh_mux_...`).
  - [ ] "Test Remote SSH Connection" diagnostic trigger directly from UI.
  - [ ] Kernel wireguard and iptables module version detection.
- [ ] **Database & Telemetry Maintenance**:
  - [ ] 1-click manual download of SQLite database snapshot (`wireguard_ui.db`).
  - [ ] Telemetry history pruning (clean up bandwidth samples older than *X* days).

---

## 🛠️ 8. DevOps, CI/CD & Cross-Platform

- [ ] **GitHub Actions CI/CD Pipeline**:
  - Automated `cargo test`, `cargo clippy`, and `cargo check`.
  - Multi-architecture Docker builds (`linux/amd64`, `linux/arm64` for Raspberry Pi).
- [ ] **Unit & Integration Tests**:
  - IP allocation collision tests.
  - Zero-downtime `wg syncconf` configuration parser tests.
  - SQLite schema migration regression tests.

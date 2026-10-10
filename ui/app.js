let currentPeers = [];
let allPeersCache = [];
let currentQrPeerId = null;
let currentInterfaceId = localStorage.getItem('wgui_current_interface') || 'wg0';
let currentInterfaces = [];
let siteViewMode = localStorage.getItem('wgui_site_view_mode') || 'grid';
let clientViewMode = localStorage.getItem('wgui_client_view_mode') || 'grid';
let siteSortOrder = localStorage.getItem('wgui_site_sort') || 'custom';
let clientSortOrder = localStorage.getItem('wgui_client_sort') || 'custom';
let isDraggingActive = false;
let dashboardPollingTimer = null;

document.addEventListener('DOMContentLoaded', () => {
    initNavigation();
    initViewControls();
    loadAppPresets();
    loadInterfaces();
    loadDashboardData();
    startDashboardPolling();
});

function initViewControls() {
    const siteSelect = document.getElementById('site-sort-select');
    if (siteSelect) siteSelect.value = siteSortOrder;
    const clientSelect = document.getElementById('client-sort-select');
    if (clientSelect) clientSelect.value = clientSortOrder;
    setSiteViewMode(siteViewMode, false);
    setClientViewMode(clientViewMode, false);
}

// Navigation Setup
function initNavigation() {
    const navItems = document.querySelectorAll('.nav-item');
    navItems.forEach(item => {
        item.addEventListener('click', () => {
            const tabName = item.getAttribute('data-tab');
            switchTab(tabName);
        });
    });
}

function switchTab(tabName) {
    document.querySelectorAll('.nav-item').forEach(el => el.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(el => el.classList.remove('active'));

    const btn = document.querySelector(`.nav-item[data-tab="${tabName}"]`);
    let tab = document.getElementById(`tab-${tabName}`);
    if (!tab && tabName === 'sitetosite') {
        tab = document.getElementById('tab-sites');
    }
    if (!tab && tabName === 'sites') {
        tab = document.getElementById('tab-sites') || document.getElementById('tab-sitetosite');
    }
    if (!tab && tabName === 'notifications') {
        tab = document.getElementById('tab-settings');
    }
    const navBtn = btn || document.querySelector(`.nav-item[data-tab="settings"]`);

    if (tab) {
        if (navBtn) navBtn.classList.add('active');
        tab.classList.add('active');

        // Update page title
        const titles = {
            overview: ['Overview Dashboard', 'System state, active connections & bandwidth telemetry'],
            clients: ['Client Device Peers', 'Roadwarrior clients with QR code profiles & auto IP allocation'],
            sites: ['Site-to-Site Tunnels', 'Gateway-to-gateway subnet routing & persistent keepalives'],
            sitetosite: ['Site-to-Site Tunnels', 'Gateway-to-gateway subnet routing & persistent keepalives'],
            topology: ['Visual Network Topology', 'Hub-and-spoke mesh visualizer across connected gateways and clients'],
            settings: ['Comprehensive Settings Hub', 'Multi-interface configuration, global presets, diagnostics and database maintenance'],
            notifications: ['Comprehensive Settings Hub', 'Multi-interface configuration, global presets, diagnostics and database maintenance'],
            audit: ['Audit Logs', 'History of peer additions, configuration changes and interface events']
        };

        if (titles[tabName]) {
            document.getElementById('page-title').textContent = titles[tabName][0];
            document.getElementById('page-subtitle').textContent = titles[tabName][1];
        }

        if (tabName === 'topology') loadTopology();
        if (tabName === 'settings') loadSettings();
        if (tabName === 'notifications') {
            switchSettingsPane('pane-notifications');
            loadSettings();
        }
        if (tabName === 'audit') loadAuditLogs();
    }
}

// Interface Management & Switching
async function loadInterfaces() {
    try {
        const res = await fetch('/api/interfaces');
        if (res.ok) {
            currentInterfaces = await res.json();
            if (currentInterfaces.length === 0) {
                currentInterfaces = [{ id: 'wg0', name: 'wg0', listen_port: 51821 }];
            }
            if (!currentInterfaces.some(i => i.id === currentInterfaceId)) {
                currentInterfaceId = currentInterfaces[0].id;
                localStorage.setItem('wgui_current_interface', currentInterfaceId);
            }
            renderInterfaceTabs();
        }
    } catch (err) {
        console.error('Failed to load interfaces:', err);
    }
}

function renderInterfaceTabs() {
    const container = document.getElementById('interface-tabs');
    if (!container) return;
    container.innerHTML = '';

    currentInterfaces.forEach(iface => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = `iface-tab-btn ${iface.id === currentInterfaceId ? 'active' : ''}`;
        btn.innerHTML = `🛡️ <span>${escapeHtml(iface.id)}</span>`;
        btn.title = `${iface.name} (Port: ${iface.listen_port})`;
        btn.onclick = () => switchInterface(iface.id);
        container.appendChild(btn);
    });

    const exportBtnLabel = document.getElementById('export-conf-btn-label');
    if (exportBtnLabel) {
        exportBtnLabel.textContent = `Export ${currentInterfaceId}.conf`;
    }
}

function switchInterface(id) {
    if (currentInterfaceId === id) return;
    currentInterfaceId = id;
    localStorage.setItem('wgui_current_interface', id);
    renderInterfaceTabs();

    // Instant settings population from cached interfaces list
    const activeTab = document.querySelector('.nav-item.active');
    if (activeTab && activeTab.getAttribute('data-tab') === 'settings') {
        const cached = currentInterfaces.find(i => i.id === id);
        if (cached) applySettingsToForm(cached);
        loadSettings();
    }
    loadDashboardData();
    showToast(`Switched active interface to ${id}`, 'info');
}

function openNewInterfaceModal() {
    const modal = document.getElementById('modal-new-interface');
    if (modal) modal.classList.add('active');
}

async function submitNewInterface(e) {
    e.preventDefault();
    const id = document.getElementById('new-iface-id').value.trim();
    const name = document.getElementById('new-iface-name').value.trim();
    const port = parseInt(document.getElementById('new-iface-port').value, 10);
    const pool = document.getElementById('new-iface-pool').value.trim();
    const pool_v6 = document.getElementById('new-iface-pool-v6').value.trim() || null;
    const mtuVal = parseInt(document.getElementById('new-iface-mtu').value, 10);
    const mss_clamping = document.getElementById('new-iface-mss-clamping').checked;

    const payload = {
        id,
        name,
        listen_port: port,
        address_pool: pool,
        address_pool_v6: pool_v6,
        dns: '1.1.1.1, 8.8.8.8',
        dns_v6: null,
        gateway_endpoint: null,
        hub_subnets: null,
        mtu: isNaN(mtuVal) ? 1420 : mtuVal,
        mss_clamping
    };

    try {
        const res = await fetch('/api/interfaces', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });

        if (res.ok) {
            closeModals();
            await loadInterfaces();
            switchInterface(id);
            showToast(`Created interface ${id} successfully!`, 'success');
        } else {
            showToast('Failed to create interface — verify port and identifier', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Error creating interface', 'error');
    }
}

// Data Fetchers
async function loadDashboardData() {
    // 1. Instant 0ms render from cache (no blank screen!)
    if (allPeersCache.length > 0) {
        currentPeers = allPeersCache.filter(p => !p.interface_id || p.interface_id === currentInterfaceId);
        renderPeersOverview(currentPeers);
        renderClients(currentPeers);
        renderSites(currentPeers);
    }

    try {
        const [overviewRes, peersRes] = await Promise.all([
            fetch(`/api/interfaces/${currentInterfaceId}/overview`).then(r => r.ok ? r : fetch('/api/overview')),
            fetch('/api/peers')
        ]);

        if (overviewRes.ok) {
            const overview = await overviewRes.json();
            updateMetrics(overview);
        }

        if (peersRes.ok) {
            allPeersCache = await peersRes.json();
            currentPeers = allPeersCache.filter(p => !p.interface_id || p.interface_id === currentInterfaceId);
            renderPeersOverview(currentPeers);
            renderClients(currentPeers);
            renderSites(currentPeers);
            renderBandwidthChart();
        }
    } catch (err) {
        console.error('Failed to load dashboard data:', err);
    }
}

function updateMetrics(overview) {
    if (!overview || !overview.interface) return;
    const clientsEl = document.getElementById('metric-clients');
    if (clientsEl) clientsEl.textContent = `${overview.active_clients} / ${overview.total_clients}`;
    const sitesEl = document.getElementById('metric-sites');
    if (sitesEl) sitesEl.textContent = `${overview.active_site_tunnels} / ${overview.total_site_tunnels}`;
    const portEl = document.getElementById('metric-port');
    if (portEl) portEl.textContent = overview.interface.listen_port;
    
    let poolText = `Pool: ${overview.interface.address_pool}`;
    if (overview.interface.address_pool_v6) {
        poolText += ` • ${overview.interface.address_pool_v6}`;
    }
    const poolEl = document.getElementById('metric-pool');
    if (poolEl) poolEl.textContent = poolText;

    const totalRcvd = formatBytes(overview.total_bytes_rcvd || 0);
    const totalSent = formatBytes(overview.total_bytes_sent || 0);
    const rxSpeed = formatSpeed(overview.total_rx_speed_bps || 0);
    const txSpeed = formatSpeed(overview.total_tx_speed_bps || 0);

    const rxSpeedEl = document.getElementById('metric-rx-speed');
    const txSpeedEl = document.getElementById('metric-tx-speed');
    const rxTotalEl = document.getElementById('metric-rx-total');
    const txTotalEl = document.getElementById('metric-tx-total');

    if (rxSpeedEl) rxSpeedEl.textContent = rxSpeed;
    if (txSpeedEl) txSpeedEl.textContent = txSpeed;
    if (rxTotalEl) rxTotalEl.textContent = `${totalRcvd} total`;
    if (txTotalEl) txTotalEl.textContent = `${totalSent} total`;
}

function renderPeersOverview(peers) {
    const tbody = document.getElementById('overview-peers-table');
    tbody.innerHTML = '';

    if (peers.length === 0) {
        tbody.innerHTML = `<tr><td colspan="6" style="text-align: center; color: var(--text-muted); padding: 2rem;">No WireGuard peers configured yet. Click "Add Client Peer" or "Add Site Tunnel" to get started.</td></tr>`;
        return;
    }

    peers.forEach(peer => {
        const tr = document.createElement('tr');
        const badgeClass = peer.peer_type === 'client' ? 'green' : 'purple';
        const hs = formatHandshake(peer.last_handshake);
        const rcvd = formatBytes(peer.bytes_rcvd);
        const sent = formatBytes(peer.bytes_sent);
        const rxSp = formatSpeed(peer.rx_speed_bps);
        const txSp = formatSpeed(peer.tx_speed_bps);

        tr.innerHTML = `
            <td>
                <strong>${escapeHtml(peer.name)}</strong>
                <div style="font-size: 0.75rem; color: var(--text-dim); font-family: var(--font-mono);">${peer.public_key.substring(0, 16)}...</div>
            </td>
            <td><span class="metric-badge ${badgeClass}">${peer.peer_type}</span></td>
            <td><code style="color: var(--accent-cyan); font-family: var(--font-mono);">${peer.allowed_ips}</code></td>
            <td><span class="metric-badge ${hs.class}">${hs.text}</span></td>
            <td style="font-family: var(--font-mono); font-size: 0.85rem;">
                <span style="color: var(--accent-green);">↓ ${rcvd}</span> / <span style="color: var(--accent-purple);">↑ ${sent}</span>
            </td>
            <td style="font-family: var(--font-mono); font-size: 0.85rem;">
                ${(peer.rx_speed_bps > 0 || peer.tx_speed_bps > 0) ? `<span style="color: var(--accent-green); font-weight: 600;">⚡ ↓ ${rxSp} ↑ ${txSp}</span>` : '<span style="color: var(--text-dim);">Idle</span>'}
            </td>
        `;
        tbody.appendChild(tr);
    });
}

function setSiteViewMode(mode, triggerRender = true) {
    siteViewMode = mode;
    localStorage.setItem('wgui_site_view_mode', mode);
    const gridBtn = document.getElementById('site-view-grid-btn');
    const tableBtn = document.getElementById('site-view-table-btn');
    const gridContainer = document.getElementById('sites-grid');
    const tableContainer = document.getElementById('sites-table-container');

    if (gridBtn) gridBtn.classList.toggle('active', mode === 'grid');
    if (tableBtn) tableBtn.classList.toggle('active', mode === 'table');
    if (gridContainer) gridContainer.style.display = mode === 'grid' ? 'grid' : 'none';
    if (tableContainer) tableContainer.style.display = mode === 'table' ? 'block' : 'none';

    if (triggerRender && currentPeers.length > 0) {
        renderSites(currentPeers);
    }
}

function setClientViewMode(mode, triggerRender = true) {
    clientViewMode = mode;
    localStorage.setItem('wgui_client_view_mode', mode);
    const gridBtn = document.getElementById('client-view-grid-btn');
    const tableBtn = document.getElementById('client-view-table-btn');
    const gridContainer = document.getElementById('clients-grid');
    const tableContainer = document.getElementById('clients-table-container');

    if (gridBtn) gridBtn.classList.toggle('active', mode === 'grid');
    if (tableBtn) tableBtn.classList.toggle('active', mode === 'table');
    if (gridContainer) gridContainer.style.display = mode === 'grid' ? 'grid' : 'none';
    if (tableContainer) tableContainer.style.display = mode === 'table' ? 'block' : 'none';

    if (triggerRender && currentPeers.length > 0) {
        renderClients(currentPeers);
    }
}

function changeSiteSort(val) {
    siteSortOrder = val;
    localStorage.setItem('wgui_site_sort', val);
    renderSites(currentPeers);
}

function changeClientSort(val) {
    clientSortOrder = val;
    localStorage.setItem('wgui_client_sort', val);
    renderClients(currentPeers);
}

function filterSites() {
    renderSites(currentPeers);
}

function filterClients() {
    renderClients(currentPeers);
}

function getSortedAndFilteredPeers(peers, type, searchInputId, sortMode, storageKey) {
    let list = peers.filter(p => p.peer_type === type);

    const input = document.getElementById(searchInputId);
    if (input && input.value && input.value.trim()) {
        const q = input.value.trim().toLowerCase();
        list = list.filter(p =>
            (p.name && p.name.toLowerCase().includes(q)) ||
            (p.allowed_ips && p.allowed_ips.toLowerCase().includes(q)) ||
            (p.endpoint && p.endpoint.toLowerCase().includes(q)) ||
            (p.public_key && p.public_key.toLowerCase().includes(q)) ||
            (p.notes && p.notes.toLowerCase().includes(q))
        );
    }

    if (sortMode === 'custom') {
        let customOrder = [];
        try {
            const raw = localStorage.getItem(storageKey);
            if (raw) customOrder = JSON.parse(raw);
        } catch {}
        if (Array.isArray(customOrder) && customOrder.length > 0) {
            list.sort((a, b) => {
                const idxA = customOrder.indexOf(a.id);
                const idxB = customOrder.indexOf(b.id);
                if (idxA !== -1 && idxB !== -1) return idxA - idxB;
                if (idxA !== -1) return -1;
                if (idxB !== -1) return 1;
                return (b.created_at || 0) - (a.created_at || 0);
            });
        }
    } else if (sortMode === 'name_asc') {
        list.sort((a, b) => (a.name || '').localeCompare(b.name || '', undefined, { sensitivity: 'base', numeric: true }));
    } else if (sortMode === 'name_desc') {
        list.sort((a, b) => (b.name || '').localeCompare(a.name || '', undefined, { sensitivity: 'base', numeric: true }));
    } else if (sortMode === 'status') {
        list.sort((a, b) => {
            const nowSec = Math.floor(Date.now() / 1000);
            const aActive = a.enabled && a.last_handshake && (nowSec - a.last_handshake) < 180 ? 1 : 0;
            const bActive = b.enabled && b.last_handshake && (nowSec - b.last_handshake) < 180 ? 1 : 0;
            if (bActive !== aActive) return bActive - aActive;
            return (b.enabled ? 1 : 0) - (a.enabled ? 1 : 0);
        });
    } else if (sortMode === 'latency') {
        list.sort((a, b) => {
            const latA = a.ping_latency_ms != null ? a.ping_latency_ms : 999999;
            const latB = b.ping_latency_ms != null ? b.ping_latency_ms : 999999;
            return latA - latB;
        });
    } else if (sortMode === 'speed') {
        list.sort((a, b) => ((b.rx_speed_bps || 0) + (b.tx_speed_bps || 0)) - ((a.rx_speed_bps || 0) + (a.tx_speed_bps || 0)));
    } else if (sortMode === 'traffic') {
        list.sort((a, b) => ((b.bytes_rcvd || 0) + (b.bytes_sent || 0)) - ((a.bytes_rcvd || 0) + (a.bytes_sent || 0)));
    }

    return list;
}

function handleReorderDrop(draggedId, targetId, storageKey, type) {
    if (!draggedId || !targetId || draggedId === targetId) return;

    let currentList = currentPeers.filter(p => p.peer_type === type);
    let customOrder = [];
    try {
        const raw = localStorage.getItem(storageKey);
        if (raw) customOrder = JSON.parse(raw);
    } catch {}

    if (!Array.isArray(customOrder) || customOrder.length === 0) {
        customOrder = currentList.map(p => p.id);
    } else {
        currentList.forEach(p => {
            if (!customOrder.includes(p.id)) customOrder.push(p.id);
        });
    }

    const fromIndex = customOrder.indexOf(draggedId);
    const toIndex = customOrder.indexOf(targetId);

    if (fromIndex !== -1 && toIndex !== -1) {
        customOrder.splice(fromIndex, 1);
        customOrder.splice(toIndex, 0, draggedId);
        localStorage.setItem(storageKey, JSON.stringify(customOrder));

        if (type === 'site_to_site') {
            siteSortOrder = 'custom';
            localStorage.setItem('wgui_site_sort', 'custom');
            const sel = document.getElementById('site-sort-select');
            if (sel) sel.value = 'custom';
            renderSites(currentPeers);
        } else {
            clientSortOrder = 'custom';
            localStorage.setItem('wgui_client_sort', 'custom');
            const sel = document.getElementById('client-sort-select');
            if (sel) sel.value = 'custom';
            renderClients(currentPeers);
        }
        showToast('Arrangement order updated!', 'success');
    }
}

function attachDragEvents(element, peerId, storageKey, type) {
    element.setAttribute('draggable', 'true');
    element.dataset.peerId = peerId;

    element.addEventListener('dragstart', (e) => {
        isDraggingActive = true;
        e.dataTransfer.setData('text/plain', peerId);
        e.dataTransfer.effectAllowed = 'move';
        element.classList.add('is-dragging');
    });

    element.addEventListener('dragover', (e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        if (!element.classList.contains('is-dragging')) {
            element.classList.add('drag-over');
        }
    });

    element.addEventListener('dragleave', () => {
        element.classList.remove('drag-over');
    });

    element.addEventListener('drop', (e) => {
        e.preventDefault();
        element.classList.remove('drag-over');
        const draggedId = e.dataTransfer.getData('text/plain');
        handleReorderDrop(draggedId, peerId, storageKey, type);
    });

    element.addEventListener('dragend', () => {
        isDraggingActive = false;
        document.querySelectorAll('.peer-card, .data-table tr').forEach(el => {
            el.classList.remove('is-dragging', 'drag-over');
        });
    });
}

function renderClients(peers) {
    const grid = document.getElementById('clients-grid');
    const tableBody = document.getElementById('clients-table-body');
    const clients = getSortedAndFilteredPeers(peers, 'client', 'client-search', clientSortOrder, 'wgui_clients_order');

    if (grid) grid.innerHTML = '';
    if (tableBody) tableBody.innerHTML = '';

    if (clients.length === 0) {
        if (grid) grid.innerHTML = `<div style="grid-column: 1/-1; text-align: center; color: var(--text-muted); padding: 3rem;">No client devices found matching filter.</div>`;
        if (tableBody) tableBody.innerHTML = `<tr><td colspan="9" style="text-align: center; color: var(--text-muted); padding: 2rem;">No client devices found.</td></tr>`;
        return;
    }

    clients.forEach(peer => {
        const hs = formatHandshake(peer.last_handshake);
        const rcvd = formatBytes(peer.bytes_rcvd);
        const sent = formatBytes(peer.bytes_sent);
        const rxSp = formatSpeed(peer.rx_speed_bps);
        const txSp = formatSpeed(peer.tx_speed_bps);
        const lat = formatLatency(peer.ping_latency_ms);

        // Render Card
        if (grid) {
            const card = document.createElement('div');
            card.className = 'peer-card';
            attachDragEvents(card, peer.id, 'wgui_clients_order', 'client');

            card.innerHTML = `
                <div class="peer-card-header">
                    <div style="display: flex; align-items: flex-start; gap: 0.35rem;">
                        <span class="drag-handle" title="Drag to rearrange">⋮⋮</span>
                        <div>
                            <div class="peer-name">${escapeHtml(peer.name)}</div>
                            <div class="peer-sub">${peer.allowed_ips}</div>
                        </div>
                    </div>
                    <span class="peer-badge ${peer.enabled ? 'enabled' : 'disabled'}">${peer.enabled ? 'Active' : 'Disabled'}</span>
                </div>
                <div class="peer-details">
                    <div class="peer-detail-item">
                        <span>Handshake:</span>
                        <span class="metric-badge ${hs.class}">${hs.text}</span>
                    </div>
                    <div class="peer-detail-item">
                        <span>Ping Latency:</span>
                        <span>${lat}</span>
                    </div>
                    <div class="peer-detail-item">
                        <span>Data Transferred:</span>
                        <span class="val" style="font-family: var(--font-mono);"><span style="color: var(--accent-green);">↓ ${rcvd}</span> <span style="color: var(--accent-purple); margin-left: 0.25rem;">↑ ${sent}</span></span>
                    </div>
                    <div class="peer-detail-item">
                        <span>Transfer Speed:</span>
                        <span class="val" style="font-family: var(--font-mono);">${(peer.rx_speed_bps > 0 || peer.tx_speed_bps > 0) ? `<span style="color: var(--accent-green); font-weight:600;">⚡ ↓ ${rxSp} ↑ ${txSp}</span>` : '<span style="color: var(--text-dim);">0 B/s</span>'}</span>
                    </div>
                    <div class="peer-detail-item">
                        <span>Public Key:</span>
                        <span class="val" title="${peer.public_key}">${peer.public_key.substring(0, 12)}...</span>
                    </div>
                    ${peer.notes ? `<div class="peer-detail-item"><span>Notes:</span><span class="val">${escapeHtml(peer.notes)}</span></div>` : ''}
                </div>
                <div class="peer-actions">
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openIperfModal('${peer.id}', '${escapeHtml(peer.name)}')" title="Run iPerf3 Speedtest">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon></svg>
                        <span>Speedtest</span>
                    </button>
                    <button type="button" class="btn btn-secondary btn-sm" onclick="showQrModal('${peer.id}', '${escapeHtml(peer.name)}')" title="View QR Code & Download Config">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"></rect><rect x="14" y="3" width="7" height="7"></rect><rect x="14" y="14" width="7" height="7"></rect><rect x="3" y="14" width="7" height="7"></rect></svg>
                        <span>QR & Conf</span>
                    </button>
                    <button type="button" class="btn btn-secondary btn-sm" onclick="copyClientConfigDirect('${peer.id}')" title="Copy raw client config to clipboard">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
                        <span>Copy</span>
                    </button>
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openEditClientModal('${peer.id}')" title="Edit Peer Details">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path></svg>
                        <span>Edit</span>
                    </button>
                    <button type="button" class="btn ${peer.enabled ? 'btn-toggle-active' : 'btn-toggle-paused'} btn-sm" onclick="togglePeer('${peer.id}')" title="${peer.enabled ? 'Disable tunnel' : 'Enable tunnel'}">
                        <span class="status-dot ${peer.enabled ? 'green' : 'amber'}" style="width: 6px; height: 6px;"></span>
                        <span>${peer.enabled ? 'Active' : 'Paused'}</span>
                    </button>
                    <button type="button" class="btn btn-danger-icon btn-sm" onclick="deletePeer('${peer.id}', '${escapeHtml(peer.name)}')" title="Delete Client Peer">
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                    </button>
                </div>
            `;
            grid.appendChild(card);
        }

        // Render Table Row
        if (tableBody) {
            const tr = document.createElement('tr');
            attachDragEvents(tr, peer.id, 'wgui_clients_order', 'client');

            tr.innerHTML = `
                <td><span class="drag-handle" title="Drag to rearrange">⋮⋮</span></td>
                <td>
                    <strong>${escapeHtml(peer.name)}</strong>
                    <span class="peer-badge ${peer.enabled ? 'enabled' : 'disabled'}" style="margin-left: 0.5rem; font-size: 0.65rem;">${peer.enabled ? 'Active' : 'Off'}</span>
                    ${peer.notes ? `<div style="font-size: 0.75rem; color: var(--text-muted);">${escapeHtml(peer.notes)}</div>` : ''}
                </td>
                <td><code style="color: var(--accent-cyan); font-family: var(--font-mono); font-size: 0.8rem;">${peer.allowed_ips}</code></td>
                <td><span class="metric-badge ${hs.class}">${hs.text}</span></td>
                <td>${lat}</td>
                <td style="font-family: var(--font-mono); font-size: 0.82rem;"><span style="color: var(--accent-green);">↓ ${rcvd}</span> <span style="color: var(--accent-purple);">↑ ${sent}</span></td>
                <td style="font-family: var(--font-mono); font-size: 0.82rem;">${(peer.rx_speed_bps > 0 || peer.tx_speed_bps > 0) ? `<span style="color: var(--accent-green); font-weight:600;">⚡ ↓ ${rxSp} ↑ ${txSp}</span>` : '<span style="color: var(--text-dim);">0 B/s</span>'}</td>
                <td><span style="font-family: var(--font-mono); font-size: 0.75rem; color: var(--text-muted);" title="${peer.public_key}">${peer.public_key.substring(0, 12)}...</span></td>
                <td style="white-space: nowrap;">
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openIperfModal('${peer.id}', '${escapeHtml(peer.name)}')" title="Speedtest">⚡ Test</button>
                    <button type="button" class="btn btn-secondary btn-sm" onclick="showQrModal('${peer.id}', '${escapeHtml(peer.name)}')" title="QR Code">QR</button>
                    <button type="button" class="btn btn-secondary btn-sm" onclick="copyClientConfigDirect('${peer.id}')" title="Copy Conf">Copy</button>
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openEditClientModal('${peer.id}')" title="Edit">Edit</button>
                    <button type="button" class="btn ${peer.enabled ? 'btn-toggle-active' : 'btn-toggle-paused'} btn-sm" onclick="togglePeer('${peer.id}')">${peer.enabled ? 'Active' : 'Paused'}</button>
                    <button type="button" class="btn btn-danger-icon btn-sm" onclick="deletePeer('${peer.id}', '${escapeHtml(peer.name)}')" title="Delete"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg></button>
                </td>
            `;
            tableBody.appendChild(tr);
        }
    });
}

function renderSites(peers) {
    const grid = document.getElementById('sites-grid');
    const tableBody = document.getElementById('sites-table-body');
    const sites = getSortedAndFilteredPeers(peers, 'site_to_site', 'site-search', siteSortOrder, 'wgui_sites_order');

    if (grid) grid.innerHTML = '';
    if (tableBody) tableBody.innerHTML = '';

    if (sites.length === 0) {
        if (grid) grid.innerHTML = `<div style="grid-column: 1/-1; text-align: center; color: var(--text-muted); padding: 3rem;">No site-to-site tunnels found matching filter.</div>`;
        if (tableBody) tableBody.innerHTML = `<tr><td colspan="10" style="text-align: center; color: var(--text-muted); padding: 2rem;">No site-to-site tunnels found.</td></tr>`;
        return;
    }

    sites.forEach(peer => {
        const hs = formatHandshake(peer.last_handshake);
        const rcvd = formatBytes(peer.bytes_rcvd);
        const sent = formatBytes(peer.bytes_sent);
        const rxSp = formatSpeed(peer.rx_speed_bps);
        const txSp = formatSpeed(peer.tx_speed_bps);
        const lat = formatLatency(peer.ping_latency_ms);

        // Render Card
        if (grid) {
            const card = document.createElement('div');
            card.className = 'peer-card';
            attachDragEvents(card, peer.id, 'wgui_sites_order', 'site_to_site');

            card.innerHTML = `
                <div class="peer-card-header">
                    <div style="display: flex; align-items: flex-start; gap: 0.35rem;">
                        <span class="drag-handle" title="Drag to rearrange sequence">⋮⋮</span>
                        <div>
                            <div class="peer-name">${escapeHtml(peer.name)}</div>
                            <div class="peer-type-label">Site-to-Site Gateway</div>
                        </div>
                    </div>
                    <div style="display: flex; gap: 0.4rem; align-items: center;">
                        <span class="peer-badge ${peer.enabled ? 'enabled' : 'disabled'}">${peer.enabled ? 'Active' : 'Off'}</span>
                        <span class="metric-badge ${hs.class}">${hs.text}</span>
                    </div>
                </div>
                <div class="peer-details">
                    <div class="peer-detail-item">
                        <span>Remote Endpoint:</span>
                        <span class="val" style="color: var(--accent-cyan); font-family: var(--font-mono);">${peer.endpoint || 'Dynamic / Auto'}</span>
                    </div>
                    <div class="peer-detail-item">
                        <span>Subnet Routes:</span>
                        <span class="val" style="color: var(--accent-purple); font-family: var(--font-mono);">${peer.allowed_ips}</span>
                    </div>
                    <div class="peer-detail-item">
                        <span>Ping Latency:</span>
                        <span>${lat}</span>
                    </div>
                    <div class="peer-detail-item">
                        <span>Total Traffic:</span>
                        <span class="val" style="font-family: var(--font-mono);"><span style="color: var(--accent-green);">↓ ${rcvd}</span> <span style="color: var(--accent-purple); margin-left: 0.25rem;">↑ ${sent}</span></span>
                    </div>
                    <div class="peer-detail-item">
                        <span>Live Speed:</span>
                        <span class="val" style="font-family: var(--font-mono);">${(peer.rx_speed_bps > 0 || peer.tx_speed_bps > 0) ? `<span style="color: var(--accent-green); font-weight:600;">⚡ ↓ ${rxSp} ↑ ${txSp}</span>` : '<span style="color: var(--text-dim);">0 B/s</span>'}</span>
                    </div>
                    <div class="peer-detail-item">
                        <span>Remote Key:</span>
                        <span class="val" title="${peer.public_key}">${peer.public_key.substring(0, 14)}...</span>
                    </div>
                </div>
                <div class="peer-actions">
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openIperfModal('${peer.id}', '${escapeHtml(peer.name)}')" title="Run iPerf3 Speedtest">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon></svg>
                        <span>Speedtest</span>
                    </button>
                    <button type="button" class="btn btn-primary btn-sm" onclick="showSiteSetupModal('${peer.id}')" title="Site Setup Assistant & Script">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>
                        <span>Setup Assistant</span>
                    </button>
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openEditSiteModal('${peer.id}')" title="Edit Subnet Routes">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path></svg>
                        <span>Edit</span>
                    </button>
                    <button type="button" class="btn ${peer.enabled ? 'btn-toggle-active' : 'btn-toggle-paused'} btn-sm" onclick="togglePeer('${peer.id}')" title="${peer.enabled ? 'Disable tunnel' : 'Enable tunnel'}">
                        <span class="status-dot ${peer.enabled ? 'green' : 'amber'}" style="width: 6px; height: 6px;"></span>
                        <span>${peer.enabled ? 'Active' : 'Paused'}</span>
                    </button>
                    <button type="button" class="btn btn-danger-icon btn-sm" onclick="deletePeer('${peer.id}', '${escapeHtml(peer.name)}')" title="Delete Site Tunnel">
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                    </button>
                </div>
            `;
            grid.appendChild(card);
        }

        // Render Table Row
        if (tableBody) {
            const tr = document.createElement('tr');
            attachDragEvents(tr, peer.id, 'wgui_sites_order', 'site_to_site');

            tr.innerHTML = `
                <td><span class="drag-handle" title="Drag to rearrange">⋮⋮</span></td>
                <td>
                    <strong>${escapeHtml(peer.name)}</strong>
                    <span class="peer-badge ${peer.enabled ? 'enabled' : 'disabled'}" style="margin-left: 0.5rem; font-size: 0.65rem;">${peer.enabled ? 'Online' : 'Off'}</span>
                </td>
                <td><span style="font-family: var(--font-mono); font-size: 0.8rem; color: var(--accent-cyan);">${peer.endpoint || 'Dynamic'}</span></td>
                <td><code style="color: var(--accent-purple); font-family: var(--font-mono); font-size: 0.8rem;">${peer.allowed_ips}</code></td>
                <td><span class="metric-badge ${hs.class}">${hs.text}</span></td>
                <td>${lat}</td>
                <td style="font-family: var(--font-mono); font-size: 0.82rem;"><span style="color: var(--accent-green);">↓ ${rcvd}</span> <span style="color: var(--accent-purple);">↑ ${sent}</span></td>
                <td style="font-family: var(--font-mono); font-size: 0.82rem;">${(peer.rx_speed_bps > 0 || peer.tx_speed_bps > 0) ? `<span style="color: var(--accent-green); font-weight:600;">⚡ ↓ ${rxSp} ↑ ${txSp}</span>` : '<span style="color: var(--text-dim);">0 B/s</span>'}</td>
                <td><span style="font-family: var(--font-mono); font-size: 0.75rem; color: var(--text-muted);" title="${peer.public_key}">${peer.public_key.substring(0, 12)}...</span></td>
                <td style="white-space: nowrap;">
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openIperfModal('${peer.id}', '${escapeHtml(peer.name)}')" title="Speedtest">⚡ Test</button>
                    <button type="button" class="btn btn-primary btn-sm" onclick="showSiteSetupModal('${peer.id}')" title="Setup Assistant">🚀 Setup</button>
                    <button type="button" class="btn btn-secondary btn-sm" onclick="openEditSiteModal('${peer.id}')" title="Edit">Edit</button>
                    <button type="button" class="btn ${peer.enabled ? 'btn-toggle-active' : 'btn-toggle-paused'} btn-sm" onclick="togglePeer('${peer.id}')">${peer.enabled ? 'Active' : 'Paused'}</button>
                    <button type="button" class="btn btn-danger-icon btn-sm" onclick="deletePeer('${peer.id}', '${escapeHtml(peer.name)}')" title="Delete"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg></button>
                </td>
            `;
            tableBody.appendChild(tr);
        }
    });
}

function openEditClientModal(peerId) {
    const peer = currentPeers.find(p => p.id === peerId);
    if (!peer) return;

    document.getElementById('edit-client-id').value = peer.id;
    document.getElementById('edit-client-pubkey').value = peer.public_key || '';
    document.getElementById('edit-client-name-input').value = peer.name || '';
    document.getElementById('edit-client-ip-input').value = peer.allowed_ips || '';
    document.getElementById('edit-client-notes-input').value = peer.notes || '';

    document.getElementById('modal-edit-client').classList.add('active');
}

async function submitEditClient(e) {
    e.preventDefault();
    const id = document.getElementById('edit-client-id').value;
    const public_key = document.getElementById('edit-client-pubkey').value;
    const name = document.getElementById('edit-client-name-input').value.trim();
    const allowed_ips = document.getElementById('edit-client-ip-input').value.trim();
    const notes = document.getElementById('edit-client-notes-input').value.trim();

    const payload = {
        name,
        public_key,
        allowed_ips,
        endpoint: null,
        persistent_keepalive: 20,
        notes: notes ? notes : null
    };

    try {
        const res = await fetch(`/api/peers/${id}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });

        if (res.ok) {
            closeModals();
            showToast(`Updated client: ${name}`, 'success');
            loadDashboardData();
        } else {
            alert('Failed to update client device');
        }
    } catch (err) {
        console.error(err);
        alert('Connection error updating client device');
    }
}

function openEditSiteModal(peerId) {
    const peer = currentPeers.find(p => p.id === peerId);
    if (!peer) return;

    document.getElementById('edit-site-id').value = peer.id;
    document.getElementById('edit-site-name-input').value = peer.name || '';
    document.getElementById('edit-site-pubkey-input').value = peer.public_key || '';
    document.getElementById('edit-site-psk-input').value = peer.preshared_key || '';
    document.getElementById('edit-site-subnets-input').value = peer.allowed_ips || '';
    document.getElementById('edit-site-endpoint-input').value = peer.endpoint || '';
    document.getElementById('edit-site-keepalive-input').value = peer.persistent_keepalive || 25;

    document.getElementById('modal-edit-site').classList.add('active');
}

async function submitEditSite(e) {
    e.preventDefault();
    const id = document.getElementById('edit-site-id').value;
    const name = document.getElementById('edit-site-name-input').value.trim();
    const public_key = document.getElementById('edit-site-pubkey-input').value.trim();
    const preshared_key = document.getElementById('edit-site-psk-input').value.trim() || null;
    const allowed_ips = document.getElementById('edit-site-subnets-input').value.trim();
    const endpoint = document.getElementById('edit-site-endpoint-input').value.trim() || null;
    const persistent_keepalive = parseInt(document.getElementById('edit-site-keepalive-input').value) || 25;

    const payload = {
        name,
        public_key,
        preshared_key,
        allowed_ips,
        endpoint,
        persistent_keepalive,
        notes: null
    };

    try {
        const res = await fetch(`/api/peers/${id}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });

        if (res.ok) {
            closeModals();
            showToast(`Updated site tunnel: ${name}`, 'success');
            loadDashboardData();
        } else {
            alert('Failed to update site-to-site tunnel');
        }
    } catch (err) {
        console.error(err);
        alert('Connection error updating site tunnel');
    }
}

let iperfIsReverse = false;

function setIperfDirection(reverse) {
    iperfIsReverse = reverse;
    const upBtn = document.getElementById('iperf-dir-upload');
    const dnBtn = document.getElementById('iperf-dir-download');
    if (upBtn) upBtn.classList.toggle('active', !reverse);
    if (dnBtn) dnBtn.classList.toggle('active', reverse);
}

function openIperfModal(peerId, peerName) {
    const peer = currentPeers.find(p => p.id === peerId);
    if (!peer) return;

    const rawIp = peer.allowed_ips.split(',').map(s => s.trim()).find(s => s.endsWith('/32')) || peer.allowed_ips.split(',')[0].trim();
    const cleanIp = rawIp.split('/')[0];

    document.getElementById('iperf-modal-title').textContent = `Speedtest: ${peer.name}`;
    document.getElementById('iperf-modal-subtitle').textContent = `Target Tunnel IP: ${cleanIp}`;
    document.getElementById('iperf-target-display').textContent = `${cleanIp} ${peer.endpoint ? `[${peer.endpoint}]` : ''}`;
    document.getElementById('iperf-peer-id').value = peer.id;
    document.getElementById('iperf-target-ip').value = cleanIp;

    // Reset UI state
    document.getElementById('iperf-status-badge').textContent = 'Ready to Benchmark';
    document.getElementById('iperf-status-badge').style.background = 'rgba(99, 102, 241, 0.2)';
    document.getElementById('iperf-status-badge').style.color = 'var(--accent-blue)';
    document.getElementById('iperf-speed-val').textContent = '---';
    document.getElementById('iperf-transferred-val').textContent = '--';
    document.getElementById('iperf-retrans-val').textContent = '--';
    document.getElementById('iperf-duration-val').textContent = '--';
    document.getElementById('iperf-progress-container').style.display = 'none';
    document.getElementById('iperf-progress-bar').style.width = '0%';
    document.getElementById('iperf-error-box').style.display = 'none';
    const diagBox = document.getElementById('iperf-diag-container');
    if (diagBox) diagBox.style.display = 'none';
    document.getElementById('iperf-start-btn').disabled = false;
    document.getElementById('iperf-start-btn').textContent = '⚡ Run Speedtest';

    document.getElementById('modal-iperf3').classList.add('active');
}

function renderIperfDiagnostics(diags, targetIp, port) {
    const container = document.getElementById('iperf-diag-container');
    if (!container || !diags) return;

    container.style.display = 'block';

    const routeVal = document.getElementById('iperf-diag-route-val');
    const pingVal = document.getElementById('iperf-diag-ping-val');
    const portVal = document.getElementById('iperf-diag-port-val');
    const recomBox = document.getElementById('iperf-diag-recom');
    const fixBox = document.getElementById('iperf-diag-fix-box');
    const fixText = document.getElementById('iperf-diag-fix-text');
    const peerCmdText = document.getElementById('iperf-diag-peer-cmd-text');

    if (routeVal) {
        if (diags.route_info && diags.route_info.includes('dev wg0')) {
            routeVal.innerHTML = '<span style="color: var(--accent-green);">✅ Active via wg0</span>';
        } else if (diags.route_info) {
            routeVal.innerHTML = `<span style="color: var(--accent-cyan); font-size: 0.7rem;">${escapeHtml(diags.route_info.split('\n')[0].substring(0, 20))}</span>`;
        } else {
            routeVal.innerHTML = '<span style="color: var(--accent-red);">🔴 No Route</span>';
        }
    }

    if (pingVal) {
        if (diags.ping_reachable) {
            pingVal.innerHTML = `<span style="color: var(--accent-green);">🟢 ${diags.ping_rtt_ms != null ? `${diags.ping_rtt_ms.toFixed(1)} ms` : 'Reachable'}</span>`;
        } else {
            pingVal.innerHTML = '<span style="color: var(--accent-red);">🔴 Timeout</span>';
        }
    }

    if (portVal) {
        if (diags.tcp_port_open) {
            portVal.innerHTML = '<span style="color: var(--accent-green);">🟢 Listening</span>';
        } else {
            portVal.innerHTML = '<span style="color: var(--accent-red);">🔴 Closed / Filtered</span>';
        }
    }

    if (recomBox) {
        recomBox.textContent = diags.recommendation || 'Please verify remote firewall and iperf3 server daemon status.';
    }

    if (fixBox && fixText) {
        if (diags.remote_fix_command) {
            fixText.value = diags.remote_fix_command;
            fixBox.style.display = 'block';
        } else {
            fixBox.style.display = 'none';
        }
    }

    if (peerCmdText) {
        const hubIp = diags.hub_tunnel_ip || '10.100.90.2';
        peerCmdText.textContent = `iperf3 -c ${hubIp} -p 5201 -t 5`;
    }
}

async function copyIperfFixCommand() {
    const el = document.getElementById('iperf-diag-fix-text');
    if (el) {
        if (el.select) el.select();
        const text = el.value || el.textContent;
        const copied = await copyToClipboard(text);
        if (copied) {
            showToast('1-Line Remote Fix Command copied! Paste into remote SSH terminal.', 'success');
        } else {
            showToast('Selected command text - press Ctrl+C to copy!', 'info');
        }
    }
}

async function copyIperfPeerTestCommand() {
    const el = document.getElementById('iperf-diag-peer-cmd-text');
    if (el) {
        const text = el.textContent || el.innerText;
        const copied = await copyToClipboard(text);
        if (copied) {
            showToast('Peer-to-Hub test command copied! Run on remote machine.', 'success');
        } else {
            showToast('Command ready for manual copy', 'info');
        }
    }
}

async function startIperfTest() {
    const peerId = document.getElementById('iperf-peer-id').value;
    const targetIp = document.getElementById('iperf-target-ip').value;
    const port = parseInt(document.getElementById('iperf-port-input').value) || 5201;
    const duration = parseInt(document.getElementById('iperf-duration-select').value) || 5;
    const parallel = parseInt(document.getElementById('iperf-parallel-select').value) || 1;

    const startBtn = document.getElementById('iperf-start-btn');
    const badge = document.getElementById('iperf-status-badge');
    const speedVal = document.getElementById('iperf-speed-val');
    const progressContainer = document.getElementById('iperf-progress-container');
    const progressBar = document.getElementById('iperf-progress-bar');
    const errorBox = document.getElementById('iperf-error-box');
    const diagContainer = document.getElementById('iperf-diag-container');

    startBtn.disabled = true;
    startBtn.textContent = '⏳ Running Test...';
    badge.textContent = `Benchmarking (${duration}s ${iperfIsReverse ? 'Download' : 'Upload'})...`;
    badge.style.background = 'rgba(56, 189, 248, 0.2)';
    badge.style.color = 'var(--accent-cyan)';
    speedVal.textContent = 'Testing...';
    errorBox.style.display = 'none';
    if (diagContainer) diagContainer.style.display = 'none';

    progressContainer.style.display = 'block';
    progressBar.style.width = '10%';

    let progressTimer = setInterval(() => {
        let currentW = parseFloat(progressBar.style.width) || 10;
        if (currentW < 90) {
            progressBar.style.width = `${currentW + (80 / (duration * 10))}%`;
        }
    }, 100);

    const payload = {
        target_ip: targetIp,
        port,
        duration,
        reverse: iperfIsReverse,
        parallel
    };

    try {
        const url = peerId ? `/api/peers/${peerId}/iperf3` : '/api/iperf3/run';
        const res = await fetch(url, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });

        clearInterval(progressTimer);
        progressBar.style.width = '100%';

        if (res.ok) {
            const data = await res.json();
            if (data.success) {
                badge.textContent = '✅ Benchmark Complete';
                badge.style.background = 'rgba(16, 185, 129, 0.2)';
                badge.style.color = 'var(--accent-green)';
                speedVal.textContent = data.formatted_speed;
                document.getElementById('iperf-transferred-val').textContent = formatBytes(data.bytes_transferred);
                document.getElementById('iperf-retrans-val').textContent = data.retransmits != null ? `${data.retransmits} pkts` : '0';
                document.getElementById('iperf-duration-val').textContent = `${data.duration_seconds.toFixed(2)}s`;
                showToast(`Throughput result: ${data.formatted_speed}`, 'success');
            } else {
                badge.textContent = '❌ Test Failed';
                badge.style.background = 'rgba(239, 68, 68, 0.2)';
                badge.style.color = 'var(--accent-red)';
                speedVal.textContent = '0 Mbps';
                errorBox.textContent = `Error: ${data.error || 'Connection refused or host unreachable'}`;
                errorBox.style.display = 'block';
                if (data.diagnostics) {
                    renderIperfDiagnostics(data.diagnostics, targetIp, port);
                }
                showToast('iperf3 benchmark failed — see diagnostic breakdown below', 'error');
            }
        } else {
            badge.textContent = '❌ Server Error';
            badge.style.background = 'rgba(239, 68, 68, 0.2)';
            badge.style.color = 'var(--accent-red)';
            speedVal.textContent = 'Failed';
            errorBox.textContent = 'Internal API error executing iperf3 benchmark.';
            errorBox.style.display = 'block';
        }
    } catch (err) {
        clearInterval(progressTimer);
        console.error(err);
        badge.textContent = '❌ Connection Error';
        badge.style.background = 'rgba(239, 68, 68, 0.2)';
        badge.style.color = 'var(--accent-red)';
        speedVal.textContent = 'Error';
        errorBox.textContent = `Network exception: ${err.message}`;
        errorBox.style.display = 'block';
    } finally {
        startBtn.disabled = false;
        startBtn.textContent = '⚡ Run Speedtest Again';
    }
}

async function copyRemoteProbeCmd() {
    const el = document.getElementById('probe-cmd-text');
    if (el) {
        if (el.select) el.select();
        const text = el.value || el.textContent;
        const copied = await copyToClipboard(text);
        if (copied) {
            showToast('Remote host probe command copied! Paste into SSH on remote machine.', 'success');
        } else {
            showToast('Selected command text - press Ctrl+C to copy!', 'info');
        }
    }
}

async function showSiteSetupModal(peerId) {
    try {
        const res = await fetch(`/api/peers/${peerId}/site-config`);
        if (res.ok) {
            const data = await res.json();
            document.getElementById('site-setup-title').textContent = `🚀 Remote Gateway Setup: ${data.peer_name}`;
            document.getElementById('site-bootstrap-text').value = data.bootstrap_cmd;
            document.getElementById('site-config-text').value = data.remote_config;
            document.getElementById('modal-site-setup').classList.add('active');
        }
    } catch (err) {
        console.error(err);
        showToast('Failed to generate site setup config', 'error');
    }
}

async function copySiteBootstrapCmd() {
    const el = document.getElementById('site-bootstrap-text');
    if (el && el.value) {
        const copied = await copyToClipboard(el.value);
        if (copied) {
            showToast('1-Line Remote Setup script copied! Paste into SSH on remote router.', 'success');
        }
    }
}

async function copySiteConfigText() {
    const el = document.getElementById('site-config-text');
    if (el && el.value) {
        const copied = await copyToClipboard(el.value);
        if (copied) {
            showToast('Raw /etc/wireguard/wg0.conf config copied to clipboard!', 'success');
        }
    }
}

function openEditSiteModal(peerId) {
    const peer = currentPeers.find(p => p.id === peerId);
    if (!peer) return;

    document.getElementById('edit-site-id').value = peer.id;
    document.getElementById('edit-site-name-input').value = peer.name || '';
    document.getElementById('edit-site-pubkey-input').value = peer.public_key || '';
    document.getElementById('edit-site-psk-input').value = peer.preshared_key || '';
    document.getElementById('edit-site-subnets-input').value = peer.allowed_ips || '';
    document.getElementById('edit-site-endpoint-input').value = peer.endpoint || '';
    document.getElementById('edit-site-keepalive-input').value = peer.persistent_keepalive || 25;

    document.getElementById('modal-edit-site').classList.add('active');
}

async function submitEditSite(e) {
    e.preventDefault();
    const id = document.getElementById('edit-site-id').value;
    const name = document.getElementById('edit-site-name-input').value.trim();
    const public_key = document.getElementById('edit-site-pubkey-input').value.trim();
    const allowed_ips = document.getElementById('edit-site-subnets-input').value.trim();
    const endpointStr = document.getElementById('edit-site-endpoint-input').value.trim();
    const psk = document.getElementById('edit-site-psk-input').value.trim();
    const persistent_keepalive = parseInt(document.getElementById('edit-site-keepalive-input').value, 10) || 25;

    const payload = {
        name,
        public_key,
        allowed_ips,
        endpoint: endpointStr ? endpointStr : null,
        preshared_key: psk,
        persistent_keepalive,
        notes: null
    };

    try {
        const res = await fetch(`/api/peers/${id}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });

        if (res.ok) {
            closeModals();
            loadDashboardData();
        } else {
            alert('Failed to update site-to-site tunnel');
        }
    } catch (err) {
        console.error(err);
        alert('Connection error updating tunnel');
    }
}

function openEditClientModal(peerId) {
    const peer = currentPeers.find(p => p.id === peerId);
    if (!peer) return;

    document.getElementById('edit-client-id').value = peer.id;
    document.getElementById('edit-client-name-input').value = peer.name || '';
    document.getElementById('edit-client-pubkey-input').value = peer.public_key || '';
    document.getElementById('edit-client-psk-input').value = peer.preshared_key || '';
    document.getElementById('edit-client-ip-input').value = peer.allowed_ips || '';
    document.getElementById('edit-client-notes-input').value = peer.notes || '';

    document.getElementById('modal-edit-client').classList.add('active');
}

async function submitEditClient(e) {
    e.preventDefault();
    const id = document.getElementById('edit-client-id').value;
    const name = document.getElementById('edit-client-name-input').value.trim();
    const public_key = document.getElementById('edit-client-pubkey-input').value.trim();
    const allowed_ips = document.getElementById('edit-client-ip-input').value.trim();
    const psk = document.getElementById('edit-client-psk-input').value.trim();
    const notes = document.getElementById('edit-client-notes-input').value.trim() || null;

    const payload = {
        name,
        public_key,
        allowed_ips,
        endpoint: null,
        preshared_key: psk ? psk : null,
        persistent_keepalive: 25,
        notes
    };

    try {
        const res = await fetch(`/api/peers/${id}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });

        if (res.ok) {
            closeModals();
            loadDashboardData();
            showToast('Client peer updated and WireGuard synchronized!', 'success');
        } else {
            showToast('Failed to update client peer', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Connection error updating peer', 'error');
    }
}

// Modal Handlers
function populateInterfaceDropdown(selectId) {
    const sel = document.getElementById(selectId);
    if (!sel) return;
    sel.innerHTML = '';
    currentInterfaces.forEach(iface => {
        const opt = document.createElement('option');
        opt.value = iface.id;
        opt.textContent = `🛡️ ${iface.id} — ${iface.name} (Port: ${iface.listen_port})`;
        if (iface.id === currentInterfaceId) {
            opt.selected = true;
        }
        sel.appendChild(opt);
    });
}

function openNewClientModal() {
    populateInterfaceDropdown('client-interface-select');

    // Auto-suggest next available IP address
    const iface = currentInterfaces.find(i => i.id === currentInterfaceId);
    if (iface && iface.address_pool) {
        const base = iface.address_pool.split('/')[0];
        const parts = base.split('.');
        if (parts.length === 4) {
            const prefix = `${parts[0]}.${parts[1]}.${parts[2]}`;
            const existingLastOctets = new Set();
            (allPeersCache || []).filter(p => p.interface_id === currentInterfaceId).forEach(p => {
                const pIp = (p.allowed_ips || '').split('/')[0].split(',')[0].trim();
                const pParts = pIp.split('.');
                if (pParts.length === 4 && pParts[0] === parts[0] && pParts[1] === parts[1] && pParts[2] === parts[2]) {
                    existingLastOctets.add(parseInt(pParts[3], 10));
                }
            });
            let nextNum = 2;
            while (existingLastOctets.has(nextNum) && nextNum < 254) {
                nextNum++;
            }
            const ipInput = document.getElementById('client-ip-input');
            if (ipInput) {
                ipInput.placeholder = `${prefix}.${nextNum}/32`;
            }
        }
    }

    document.getElementById('modal-client').classList.add('active');
}

async function openNewSiteModal() {
    populateInterfaceDropdown('site-interface-select');
    document.getElementById('modal-site').classList.add('active');
    try {
        const res = await fetch('/api/peers/site/probe-command');
        if (res.ok) {
            const data = await res.json();
            const probeEl = document.getElementById('probe-cmd-text');
            if (probeEl && data.probe_cmd) {
                probeEl.value = data.probe_cmd;
            }
            const subnetsInput = document.getElementById('site-subnets-input');
            if (subnetsInput && data.next_ip) {
                subnetsInput.placeholder = `${data.next_ip}, 192.168.10.0/24`;
            }
        }
    } catch (err) {
        console.error('Error fetching site probe command:', err);
    }
}

function closeModals() {
    document.querySelectorAll('.modal-overlay').forEach(el => el.classList.remove('active'));
}

async function submitNewClient(e) {
    e.preventDefault();
    const pskVal = document.getElementById('client-psk-input')?.value.trim() || null;
    const generatePsk = document.getElementById('client-generate-psk')?.checked || false;
    const pubKeyVal = document.getElementById('client-pubkey-input')?.value.trim() || null;
    const targetIface = document.getElementById('client-interface-select')?.value || currentInterfaceId;

    const payload = {
        name: document.getElementById('client-name-input').value,
        interface_id: targetIface,
        public_key: pubKeyVal,
        preshared_key: pskVal,
        generate_psk: generatePsk,
        custom_ip: document.getElementById('client-ip-input').value || null,
        custom_ip_v6: document.getElementById('client-ipv6-input')?.value || null,
        notes: document.getElementById('client-notes-input').value || null
    };

    try {
        const res = await fetch('/api/peers/client', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });

        if (res.ok) {
            const newPeer = await res.json();
            closeModals();
            if (currentInterfaceId !== targetIface) {
                switchInterface(targetIface);
            } else {
                loadDashboardData();
            }
            showQrModal(newPeer.id, newPeer.name);
            showToast(`Client peer ${newPeer.name} created on ${targetIface}!`, 'success');
        } else {
            showToast('Failed to create client peer', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Error creating client peer', 'error');
    }
}

async function submitNewSite(e) {
    e.preventDefault();
    const psk = document.getElementById('site-psk-input').value.trim();
    const targetIface = document.getElementById('site-interface-select')?.value || currentInterfaceId;
    const payload = {
        name: document.getElementById('site-name-input').value,
        interface_id: targetIface,
        remote_public_key: document.getElementById('site-pubkey-input').value,
        remote_allowed_ips: document.getElementById('site-subnets-input').value,
        endpoint: document.getElementById('site-endpoint-input').value || null,
        preshared_key: psk || null,
        persistent_keepalive: parseInt(document.getElementById('site-keepalive-input').value) || 25
    };

    try {
        const res = await fetch('/api/peers/site', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });

        if (res.ok) {
            closeModals();
            if (currentInterfaceId !== targetIface) {
                switchInterface(targetIface);
            } else {
                loadDashboardData();
            }
            showToast(`Site tunnel created on ${targetIface}!`, 'success');
        } else {
            showToast('Failed to create site tunnel', 'error');
        }
    } catch (err) {
        console.error(err);
    }
}

async function showQrModal(peerId, peerName) {
    currentQrPeerId = peerId;
    try {
        const res = await fetch(`/api/peers/${peerId}/qr`);
        if (res.ok) {
            const data = await res.json();
            document.getElementById('qr-modal-title').textContent = `Client Profile: ${peerName}`;
            document.getElementById('qr-image').src = data.qr_svg_data_uri;
            document.getElementById('config-text').value = data.config_content;
            document.getElementById('modal-qr').classList.add('active');
        }
    } catch (err) {
        console.error(err);
    }
}

function downloadCurrentClientConfig() {
    if (currentQrPeerId) {
        window.location.href = `/api/peers/${currentQrPeerId}/download`;
    }
}

async function copyConfigText() {
    const textarea = document.getElementById('config-text');
    if (textarea && textarea.value) {
        const copied = await copyToClipboard(textarea.value);
        if (copied) {
            showToast('WireGuard client configuration copied to clipboard!', 'success');
        } else {
            showToast('Failed to copy config to clipboard', 'error');
        }
    }
}

async function togglePeer(id) {
    try {
        await fetch(`/api/peers/${id}/toggle`, { method: 'POST' });
        loadDashboardData();
    } catch (err) {
        console.error(err);
    }
}

async function deletePeer(id, name) {
    if (confirm(`Are you sure you want to remove peer "${name}"?`)) {
        try {
            await fetch(`/api/peers/${id}`, { method: 'DELETE' });
            loadDashboardData();
        } catch (err) {
            console.error(err);
        }
    }
}

function exportServerConfig() {
    const ifaceId = currentInterfaceId || 'wg0';
    window.location.href = `/api/interfaces/${encodeURIComponent(ifaceId)}/export`;
}

// Topology Renderer
async function loadTopology() {
    try {
        const res = await fetch('/api/topology');
        if (!res.ok) return;
        const data = await res.json();

        const svg = document.getElementById('topology-svg');
        svg.innerHTML = '';

        const width = svg.clientWidth || 960;
        const height = svg.clientHeight || 760;
        const centerX = width / 2;
        const centerY = height / 2;

        // SVG Filters for glowing nodes
        const defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs');
        defs.innerHTML = `
            <filter id="glow-hub" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur stdDeviation="5" result="blur" />
                <feComposite in="SourceGraphic" in2="blur" operator="over" />
            </filter>
        `;
        svg.appendChild(defs);

        const hubNode = data.nodes.find(n => n.node_type === 'hub');
        const otherNodes = data.nodes.filter(n => n.node_type !== 'hub');

        // Dynamic Radial Radius (spacious layout so nodes & labels never overlap or get clipped)
        const count = otherNodes.length;
        const radius = Math.min((width / 2) - 150, (height / 2) - 120, Math.max(220, 170 + count * 10));

        // Draw Links (Connecting Tunnel Lines)
        otherNodes.forEach((node, idx) => {
            const angle = (idx / Math.max(1, otherNodes.length)) * 2 * Math.PI - Math.PI / 2;
            const x = centerX + radius * Math.cos(angle);
            const y = centerY + radius * Math.sin(angle);

            const isOnline = node.status === 'online';
            const isOffline = node.status === 'offline';
            const color = isOnline ? '#6366f1' : (isOffline ? '#ef4444' : '#64748b');

            const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
            line.setAttribute('x1', centerX);
            line.setAttribute('y1', centerY);
            line.setAttribute('x2', x);
            line.setAttribute('y2', y);
            line.setAttribute('stroke', color);
            line.setAttribute('stroke-width', isOnline ? '2.5' : '1.5');
            line.setAttribute('stroke-dasharray', isOnline ? '6 4' : '3 3');
            line.setAttribute('opacity', isOnline ? '0.85' : (isOffline ? '0.5' : '0.25'));
            line.setAttribute('class', isOnline ? 'pulse-line' : '');
            svg.appendChild(line);
        });

        // Draw Connected Nodes (Roadwarriors & Site-to-Site Gateways)
        otherNodes.forEach((node, idx) => {
            const angle = (idx / Math.max(1, otherNodes.length)) * 2 * Math.PI - Math.PI / 2;
            const x = centerX + radius * Math.cos(angle);
            const y = centerY + radius * Math.sin(angle);
            const color = node.status === 'offline' ? '#ef4444' : (node.status === 'disabled' ? '#64748b' : (node.node_type === 'site' ? '#a855f7' : '#10b981'));
            const details = node.status === 'offline' ? `${node.details} • Never Connected` : node.details;
            
            drawNode(svg, x, y, node.label, details, color, false, angle, node.status);
        });

        // Draw Central Hub Node (wg0)
        if (hubNode) {
            drawNode(svg, centerX, centerY, hubNode.label, hubNode.details || 'WireGuard Interface (wg0)', '#6366f1', true, 0, 'online');
        }
    } catch (err) {
        console.error('Topology load error:', err);
    }
}

function drawNode(svg, x, y, titleText, detailsText, color, isHub, angle, status) {
    const group = document.createElementNS('http://www.w3.org/2000/svg', 'g');
    group.setAttribute('class', 'topology-node-group');

    // Outer Animated Ping Ring for active Tunnels
    if (isHub || status === 'online') {
        const ring = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
        ring.setAttribute('cx', x);
        ring.setAttribute('cy', y);
        ring.setAttribute('r', isHub ? '32' : '22');
        ring.setAttribute('fill', 'none');
        ring.setAttribute('stroke', color);
        ring.setAttribute('stroke-width', '2');
        ring.setAttribute('class', 'ping-ring');
        group.appendChild(ring);
    }

    // Main Circle Node
    const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    circle.setAttribute('cx', x);
    circle.setAttribute('cy', y);
    circle.setAttribute('r', isHub ? '24' : '16');
    circle.setAttribute('fill', color);
    circle.setAttribute('stroke', '#ffffff');
    circle.setAttribute('stroke-width', '2.5');
    if (isHub) circle.setAttribute('filter', 'url(#glow-hub)');
    group.appendChild(circle);

    // Emoji / Type Badge Icon inside Circle
    const icon = document.createElementNS('http://www.w3.org/2000/svg', 'text');
    icon.setAttribute('x', x);
    icon.setAttribute('y', y + (isHub ? 5 : 4));
    icon.setAttribute('text-anchor', 'middle');
    icon.setAttribute('fill', '#ffffff');
    icon.setAttribute('font-size', isHub ? '14' : '10');
    icon.setAttribute('font-weight', 'bold');
    icon.textContent = isHub ? '⚡' : (color === '#a855f7' ? '🌐' : (color === '#ef4444' ? '🔴' : '📱'));
    group.appendChild(icon);

    // Smart Label Position Calculation (Prevents Overlapping)
    let labelY = y;
    let labelX = x;
    const sinAngle = Math.sin(angle);
    const cosAngle = Math.cos(angle);

    if (isHub) {
        labelY = y + 44;
    } else {
        if (sinAngle < -0.4) {
            // Top half -> place label ABOVE node
            labelY = y - 30;
        } else if (sinAngle > 0.4) {
            // Bottom half -> place label BELOW node
            labelY = y + 34;
        } else if (cosAngle < 0) {
            // Left side -> shift to left
            labelX = x - 22;
            labelY = y + 4;
        } else {
            // Right side -> shift to right
            labelX = x + 22;
            labelY = y + 4;
        }
    }

    // Text Container Group with Translucent Glassmorphism Card
    const textGroup = document.createElementNS('http://www.w3.org/2000/svg', 'g');

    const bgRect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    bgRect.setAttribute('fill', 'rgba(15, 23, 42, 0.92)');
    bgRect.setAttribute('stroke', 'rgba(255, 255, 255, 0.15)');
    bgRect.setAttribute('stroke-width', '1');
    bgRect.setAttribute('rx', '6');

    const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
    text.setAttribute('x', labelX);
    text.setAttribute('y', labelY);
    text.setAttribute('text-anchor', (labelX === x) ? 'middle' : (cosAngle < 0 ? 'end' : 'start'));

    // Line 1: Title
    const tspanTitle = document.createElementNS('http://www.w3.org/2000/svg', 'tspan');
    tspanTitle.setAttribute('x', labelX);
    tspanTitle.setAttribute('dy', '0');
    tspanTitle.setAttribute('font-size', '12');
    tspanTitle.setAttribute('font-weight', '700');
    tspanTitle.setAttribute('fill', '#f8fafc');
    tspanTitle.textContent = titleText;
    text.appendChild(tspanTitle);

    // Line 2: Details (Formatted Subnets & IP)
    if (detailsText) {
        const formattedDetails = detailsText.replace(/,/g, '  •');
        const tspanDetails = document.createElementNS('http://www.w3.org/2000/svg', 'tspan');
        tspanDetails.setAttribute('x', labelX);
        tspanDetails.setAttribute('dy', '15');
        tspanDetails.setAttribute('font-size', '10');
        tspanDetails.setAttribute('font-weight', '500');
        tspanDetails.setAttribute('fill', '#94a3b8');
        tspanDetails.textContent = formattedDetails;
        text.appendChild(tspanDetails);
    }

    textGroup.appendChild(bgRect);
    textGroup.appendChild(text);
    group.appendChild(textGroup);

    svg.appendChild(group);

    // Calculate dynamic bounding box for background pill card
    requestAnimationFrame(() => {
        try {
            const bbox = text.getBBox();
            const padX = 8;
            const padY = 4;
            bgRect.setAttribute('x', bbox.x - padX);
            bgRect.setAttribute('y', bbox.y - padY);
            bgRect.setAttribute('width', bbox.width + (padX * 2));
            bgRect.setAttribute('height', bbox.height + (padY * 2));
        } catch (e) {
            // SVG fallback
        }
    });
}

// Settings
function applySettingsToForm(iface) {
    if (!iface) return;
    const titleEl = document.getElementById('settings-interface-title');
    if (titleEl) titleEl.textContent = `🛡️ Interface: ${iface.id} (${iface.name || iface.id})`;

    const setVal = (id, v) => { const el = document.getElementById(id); if (el) el.value = v ?? ''; };
    setVal('setting-name', iface.name);
    setVal('setting-port', iface.listen_port);
    setVal('setting-pool', iface.address_pool);
    setVal('setting-pool-v6', iface.address_pool_v6 || '');
    setVal('setting-dns', iface.dns);
    setVal('setting-dns-v6', iface.dns_v6 || '');
    setVal('setting-mtu', iface.mtu || 1420);
    const mssEl = document.getElementById('setting-mss-clamping');
    if (mssEl) mssEl.checked = iface.mss_clamping !== false;
    setVal('setting-postup', iface.post_up);
    setVal('setting-postdown', iface.post_down);
    setVal('setting-public-key', iface.public_key || '');
    setVal('setting-gateway-endpoint', iface.gateway_endpoint || '');
    setVal('setting-hub-subnets', iface.hub_subnets || '');
    setVal('setting-webhook', iface.webhook_url || '');
    setVal('setting-webhook-token', iface.webhook_token || '');

    calculatePoolInspection();
}

async function loadSettings() {
    // 1. Instant 0ms render from memory cache
    const cached = currentInterfaces.find(i => i.id === currentInterfaceId);
    if (cached) {
        applySettingsToForm(cached);
    }

    // 2. Fetch fresh updates in parallel without blocking
    try {
        const res = await fetch(`/api/interfaces/${currentInterfaceId}`).then(r => r.ok ? r : fetch('/api/interface'));
        if (res.ok) {
            const iface = await res.json();
            applySettingsToForm(iface);
            const idx = currentInterfaces.findIndex(i => i.id === iface.id);
            if (idx !== -1) {
                currentInterfaces[idx] = iface;
            } else {
                currentInterfaces.push(iface);
            }
        }
    } catch (err) {
        console.error(err);
    }
}

async function saveInterfaceSettings(e) {
    e.preventDefault();
    const webhookVal = document.getElementById('setting-webhook').value.trim();
    const webhookTokenVal = document.getElementById('setting-webhook-token').value.trim();
    const poolV6 = document.getElementById('setting-pool-v6')?.value.trim() || null;
    const dnsV6 = document.getElementById('setting-dns-v6')?.value.trim() || null;
    const mtuVal = parseInt(document.getElementById('setting-mtu')?.value, 10);
    const mss_clamping = document.getElementById('setting-mss-clamping')?.checked ?? true;

    const payload = {
        id: currentInterfaceId,
        name: document.getElementById('setting-name').value,
        private_key: '',
        public_key: document.getElementById('setting-public-key').value.trim() || null,
        gateway_endpoint: document.getElementById('setting-gateway-endpoint').value.trim() || null,
        hub_subnets: document.getElementById('setting-hub-subnets').value.trim() || null,
        listen_port: parseInt(document.getElementById('setting-port').value),
        address_pool: document.getElementById('setting-pool').value,
        address_pool_v6: poolV6,
        dns: document.getElementById('setting-dns').value,
        dns_v6: dnsV6,
        post_up: document.getElementById('setting-postup').value,
        post_down: document.getElementById('setting-postdown').value,
        webhook_url: webhookVal ? webhookVal : null,
        webhook_token: webhookTokenVal ? webhookTokenVal : null,
        mtu: isNaN(mtuVal) ? 1420 : mtuVal,
        mss_clamping,
        enabled: true,
        created_at: 0
    };

    try {
        const res = await fetch(`/api/interfaces/${currentInterfaceId}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });
        if (res.ok) {
            showToast(`Interface ${currentInterfaceId} settings updated successfully!`, 'success');
            loadInterfaces();
            loadDashboardData();
        } else {
            showToast('Failed to save interface settings', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Failed to save interface settings', 'error');
    }
}

async function detectGatewayEndpoint() {
    const btn = document.querySelector('[onclick="detectGatewayEndpoint()"]');
    const input = document.getElementById('setting-gateway-endpoint');
    btn.textContent = '⏳ Detecting...';
    btn.disabled = true;
    try {
        const res = await fetch('https://api.ipify.org?format=text');
        if (res.ok) {
            input.value = await res.text();
            showToast('Public IP detected!', 'success');
        } else {
            showToast('Could not detect public IP', 'error');
        }
    } catch {
        showToast('Could not detect public IP — check internet access', 'error');
    } finally {
        btn.textContent = '🔍 Auto-Detect';
        btn.disabled = false;
    }
}

async function detectHubSubnets() {
    const btn = document.querySelector('[onclick="detectHubSubnets()"]');
    const input = document.getElementById('setting-hub-subnets');
    btn.textContent = '⏳ Detecting...';
    btn.disabled = true;
    try {
        const res = await fetch('/api/interface/detect-subnets');
        if (res.ok) {
            const data = await res.json();
            if (data.subnets) {
                input.value = data.subnets;
                showToast(`Detected Hub subnets: ${data.subnets}`, 'success');
            } else {
                showToast('No active physical LAN subnets detected', 'info');
            }
        } else {
            showToast('Failed to detect Hub subnets from host', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Error querying Hub subnets', 'error');
    } finally {
        btn.textContent = '🔍 Auto-Detect';
        btn.disabled = false;
    }
}

async function saveNotificationSettings(e) {
    e.preventDefault();
    const webhookVal = document.getElementById('setting-webhook').value.trim();
    const webhookTokenVal = document.getElementById('setting-webhook-token').value.trim();

    try {
        const resIface = await fetch('/api/interface');
        let iface = {};
        if (resIface.ok) {
            iface = await resIface.json();
        }

        const payload = {
            id: iface.id || 'wg0',
            name: iface.name || 'wg0',
            private_key: '',
            public_key: '',
            listen_port: iface.listen_port || 51820,
            address_pool: iface.address_pool || '10.8.0.1/24',
            dns: iface.dns || '1.1.1.1',
            post_up: iface.post_up || '',
            post_down: iface.post_down || '',
            webhook_url: webhookVal ? webhookVal : null,
            webhook_token: webhookTokenVal ? webhookTokenVal : null,
            enabled: true,
            created_at: 0
        };

        const res = await fetch('/api/interface', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });
        if (res.ok) {
            showToast('Notification & alert settings saved successfully!', 'success');
            loadDashboardData();
        } else {
            showToast('Failed to save notification settings', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Error saving notification settings', 'error');
    }
}

async function sendTestNotification(btnEl) {
    const urlVal = document.getElementById('setting-webhook').value.trim();
    const tokenVal = document.getElementById('setting-webhook-token').value.trim();

    if (!urlVal) {
        showToast('Please enter an Alert Webhook URL before testing', 'error');
        return;
    }

    const btn = btnEl || (window.event ? window.event.currentTarget || window.event.target : null);
    let oldText = '';
    if (btn) {
        oldText = btn.innerHTML;
        btn.innerHTML = '⏳ Sending Notification...';
        btn.disabled = true;
    }

    try {
        const res = await fetch('/api/notifications/test', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                webhook_url: urlVal,
                webhook_token: tokenVal ? tokenVal : null
            })
        });

        const data = await res.json();
        if (res.ok) {
            showToast(data.message || 'Test notification sent!', 'success');
        } else {
            showToast(data.error || 'Failed to send test notification', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Error dispatching test notification', 'error');
    } finally {
        if (btn) {
            btn.innerHTML = oldText || '🔔 Send Test Notification';
            btn.disabled = false;
        }
    }
}

// ============================================================================
// Settings Hub Controller & Feature Handlers
// ============================================================================

function switchSettingsPane(paneId) {
    document.querySelectorAll('.settings-nav-btn').forEach(btn => {
        btn.classList.toggle('active', btn.getAttribute('data-pane') === paneId);
    });
    document.querySelectorAll('.settings-pane').forEach(pane => {
        pane.classList.toggle('active', pane.id === paneId);
    });
    if (paneId === 'pane-diagnostics') loadDiagnostics();
    if (paneId === 'pane-backup') loadBackupStats();
}

function calculatePoolInspection() {
    const poolInput = document.getElementById('setting-pool');
    if (!poolInput) return;
    const poolVal = poolInput.value.trim();
    const badgeEl = document.getElementById('ip-pool-status-badge');
    const rangeEl = document.getElementById('ip-stat-range');
    const capEl = document.getElementById('ip-stat-capacity');
    const allocEl = document.getElementById('ip-stat-allocated');
    const progEl = document.getElementById('ip-stat-progress');

    const cidrMatch = poolVal.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\/(\d{1,2})$/);
    if (!cidrMatch) {
        if (badgeEl) {
            badgeEl.textContent = 'Invalid CIDR';
            badgeEl.className = 'metric-badge red';
        }
        if (rangeEl) rangeEl.textContent = 'Invalid Format';
        if (capEl) capEl.textContent = '-';
        return;
    }

    const [_, o1, o2, o3, o4, prefixStr] = cidrMatch;
    const prefix = parseInt(prefixStr, 10);
    if (prefix < 8 || prefix > 30) {
        if (badgeEl) {
            badgeEl.textContent = 'Prefix /8 - /30';
            badgeEl.className = 'metric-badge red';
        }
        return;
    }

    if (badgeEl) {
        badgeEl.textContent = `Valid /${prefix} Pool`;
        badgeEl.className = 'metric-badge green';
    }

    const totalHosts = Math.pow(2, 32 - prefix) - 2;
    const baseOctets = `${o1}.${o2}.${o3}`;
    if (rangeEl) {
        if (prefix >= 24) {
            rangeEl.textContent = `${baseOctets}.2 - ${baseOctets}.${Math.min(254, totalHosts)}`;
        } else {
            rangeEl.textContent = `${o1}.${o2}.0.2 - Dynamic Hub Pool`;
        }
    }
    if (capEl) capEl.textContent = `${totalHosts.toLocaleString()} Usable IPs`;

    const peersOnIface = (allPeersCache || []).filter(p => p.interface_id === currentInterfaceId);
    const peerCount = peersOnIface.length;
    const percent = Math.min(100, Math.round((peerCount / Math.max(1, totalHosts)) * 100));

    if (allocEl) allocEl.textContent = `${peerCount} Peers (${percent}%)`;
    if (progEl) progEl.style.width = `${Math.max(2, percent)}%`;
}

function insertFirewallSnippet(type) {
    const postUpEl = document.getElementById('setting-postup');
    const postDownEl = document.getElementById('setting-postdown');
    if (!postUpEl || !postDownEl) return;

    let upRule = '';
    let downRule = '';

    if (type === 'nat') {
        const egress = prompt('Enter egress physical network interface for NAT Masquerade (e.g. eth0, vmbr0):', 'vmbr0');
        if (!egress) return;
        upRule = `iptables -t nat -A POSTROUTING -o ${egress} -j MASQUERADE`;
        downRule = `iptables -t nat -D POSTROUTING -o ${egress} -j MASQUERADE`;
    } else if (type === 'forward') {
        upRule = `iptables -A FORWARD -i %i -j ACCEPT; iptables -A FORWARD -o %i -j ACCEPT`;
        downRule = `iptables -D FORWARD -i %i -j ACCEPT; iptables -D FORWARD -o %i -j ACCEPT`;
    } else if (type === 'isolate') {
        upRule = `iptables -I FORWARD -i %i -o %i -j DROP`;
        downRule = `iptables -D FORWARD -i %i -o %i -j DROP`;
    } else if (type === 'mss') {
        upRule = `iptables -t mangle -A POSTROUTING -p tcp --tcp-flags SYN,RST SYN -o %i -j TCPMSS --clamp-mss-to-pmtu`;
        downRule = `iptables -t mangle -D POSTROUTING -p tcp --tcp-flags SYN,RST SYN -o %i -j TCPMSS --clamp-mss-to-pmtu`;
    }

    if (upRule) {
        postUpEl.value = postUpEl.value.trim() ? `${postUpEl.value.trim()}; ${upRule}` : upRule;
        postDownEl.value = postDownEl.value.trim() ? `${postDownEl.value.trim()}; ${downRule}` : downRule;
        showToast('Injected firewall rules into PostUp and PostDown!', 'success');
    }
}

function clearFirewallScripts() {
    if (confirm('Clear both PostUp and PostDown script areas?')) {
        const postUpEl = document.getElementById('setting-postup');
        const postDownEl = document.getElementById('setting-postdown');
        if (postUpEl) postUpEl.value = '';
        if (postDownEl) postDownEl.value = '';
        showToast('Firewall scripts cleared', 'info');
    }
}

async function derivePublicKeyFromPrivate() {
    try {
        const res = await fetch(`/api/interfaces/${currentInterfaceId}/derive-key`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({})
        });
        if (res.ok) {
            const data = await res.json();
            const pubEl = document.getElementById('setting-public-key');
            if (pubEl && data.public_key) {
                pubEl.value = data.public_key;
                showToast('Curve25519 Public Key derived successfully!', 'success');
            }
        } else {
            showToast('Failed to auto-derive public key', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Error deriving public key', 'error');
    }
}

async function forceKernelSync() {
    try {
        const res = await fetch(`/api/interfaces/${currentInterfaceId}/sync`, { method: 'POST' });
        if (res.ok) {
            showToast(`Kernel synchronized for ${currentInterfaceId} (wg syncconf)!`, 'success');
            loadDashboardData();
        } else {
            showToast('Failed to synchronize kernel interface', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Error syncing interface', 'error');
    }
}

async function previewCurrentInterfaceConfig() {
    try {
        const res = await fetch(`/api/interfaces/${currentInterfaceId}/preview`);
        if (res.ok) {
            const data = await res.json();
            document.getElementById('config-preview-title').textContent = `🛡️ Live Server Config Preview (${data.interface_id}.conf)`;
            document.getElementById('config-preview-content').value = data.config;
            document.getElementById('modal-config-preview').classList.add('active');
        } else {
            showToast('Could not generate server config preview', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Error generating config preview', 'error');
    }
}

async function copyConfigPreviewText() {
    const el = document.getElementById('config-preview-content');
    if (el && el.value) {
        const copied = await copyToClipboard(el.value);
        if (copied) {
            showToast('Server configuration copied to clipboard!', 'success');
        } else {
            showToast('Failed to copy config to clipboard', 'error');
        }
    }
}

function saveAppPresets(e) {
    if (e) e.preventDefault();
    const mode = document.getElementById('preset-mode-split')?.checked ? 'split' : 'full';
    const mtu = document.getElementById('preset-client-mtu')?.value || '1420';
    const dns = document.getElementById('preset-client-dns')?.value || '1.1.1.1, 1.0.0.1';
    const poll = document.getElementById('preset-polling-rate')?.value || '2000';

    localStorage.setItem('wg_preset_mode', mode);
    localStorage.setItem('wg_preset_mtu', mtu);
    localStorage.setItem('wg_preset_dns', dns);
    localStorage.setItem('wg_preset_poll', poll);

    restartPollingWithInterval(parseInt(poll, 10));
    showToast('App and client presets saved!', 'success');
}

function loadAppPresets() {
    const mode = localStorage.getItem('wg_preset_mode') || 'full';
    const mtu = localStorage.getItem('wg_preset_mtu') || '1420';
    const dns = localStorage.getItem('wg_preset_dns') || '1.1.1.1, 1.0.0.1';
    const poll = localStorage.getItem('wg_preset_poll') || '2000';

    if (mode === 'split') {
        const splitRadio = document.getElementById('preset-mode-split');
        if (splitRadio) splitRadio.checked = true;
    } else {
        const fullRadio = document.getElementById('preset-mode-full');
        if (fullRadio) fullRadio.checked = true;
    }

    const mtuEl = document.getElementById('preset-client-mtu');
    if (mtuEl) mtuEl.value = mtu;

    const dnsEl = document.getElementById('preset-client-dns');
    if (dnsEl) dnsEl.value = dns;

    const pollEl = document.getElementById('preset-polling-rate');
    if (pollEl) pollEl.value = poll;
}

function setPresetMtu(val) {
    const el = document.getElementById('preset-client-mtu');
    if (el) el.value = val;
}

function setPresetDns(val) {
    const el = document.getElementById('preset-client-dns');
    if (el) el.value = val;
}

function startDashboardPolling() {
    const savedRate = parseInt(localStorage.getItem('wg_preset_poll') || '2000', 10);
    restartPollingWithInterval(savedRate);
}

function restartPollingWithInterval(intervalMs) {
    if (dashboardPollingTimer) {
        clearInterval(dashboardPollingTimer);
    }
    const safeInterval = Math.max(1000, isNaN(intervalMs) ? 2000 : intervalMs);
    dashboardPollingTimer = setInterval(loadDashboardData, safeInterval);
}

async function loadDiagnostics() {
    try {
        const res = await fetch('/api/diagnostics');
        if (res.ok) {
            const data = await res.json();
            const sshModeEl = document.getElementById('diag-ssh-mode');
            const sshTargetEl = document.getElementById('diag-ssh-target');
            const sshMuxEl = document.getElementById('diag-ssh-mux');

            if (sshModeEl) {
                sshModeEl.textContent = data.ssh_sync_enabled ? 'Remote SSH Sync' : 'Local Linux Host';
                sshModeEl.style.color = data.ssh_sync_enabled ? 'var(--accent-cyan)' : 'var(--accent-green)';
            }
            if (sshTargetEl) sshTargetEl.textContent = data.ssh_target || 'N/A';
            if (sshMuxEl) {
                sshMuxEl.textContent = data.ssh_mux_active ? 'Active (OpenSSH Mux)' : 'Standby / Ready';
                sshMuxEl.style.color = data.ssh_mux_active ? 'var(--accent-green)' : 'var(--text-muted)';
            }
        }
    } catch (err) {
        console.error('Failed to load diagnostics:', err);
    }
}

async function testRemoteSshConnection(btn) {
    if (btn) {
        btn.textContent = '⏳ Testing Connection...';
        btn.disabled = true;
    }
    const logEl = document.getElementById('diag-console-log');
    const badgeEl = document.getElementById('diag-latency-badge');

    if (logEl) logEl.textContent = 'Executing OpenSSH diagnostic check...';

    try {
        const res = await fetch('/api/diagnostics/ssh-test', { method: 'POST' });
        if (res.ok) {
            const data = await res.json();
            if (badgeEl) {
                badgeEl.textContent = `${data.latency_ms} ms RTT`;
                badgeEl.className = data.success ? 'metric-badge green' : 'metric-badge red';
                badgeEl.style.display = 'inline-block';
            }
            if (logEl) {
                logEl.textContent = `[${new Date().toLocaleTimeString()}] Target: ${data.target} (Latency: ${data.latency_ms} ms)\nStatus: ${data.success ? 'SUCCESS' : 'FAILED'}\n\n${data.output}`;
            }
            showToast(data.success ? `SSH Test Passed (${data.latency_ms} ms)` : 'SSH Test Failed', data.success ? 'success' : 'error');
        } else {
            if (logEl) logEl.textContent = 'Failed to execute SSH test from server';
            showToast('SSH Diagnostic failed', 'error');
        }
    } catch (err) {
        if (logEl) logEl.textContent = 'Error: ' + err.message;
        showToast('SSH Test error', 'error');
    } finally {
        if (btn) {
            btn.textContent = '🔍 Test Remote SSH Connection & Latency';
            btn.disabled = false;
        }
    }
}

async function loadBackupStats() {
    try {
        const res = await fetch('/api/database/stats');
        if (res.ok) {
            const data = await res.json();
            const pathEl = document.getElementById('backup-db-path');
            const sizeEl = document.getElementById('backup-db-size');
            const samplesEl = document.getElementById('backup-db-samples');

            if (pathEl) pathEl.textContent = data.db_path || 'wireguard_ui.db';
            if (sizeEl) {
                const bytes = data.db_size_bytes || 0;
                sizeEl.textContent = bytes > 1048576 ? `${(bytes / 1048576).toFixed(2)} MB` : `${(bytes / 1024).toFixed(1)} KB`;
            }
            if (samplesEl) {
                samplesEl.textContent = `${(data.bandwidth_samples_count || 0).toLocaleString()} samples (${(data.audit_logs_count || 0).toLocaleString()} audit logs)`;
            }
        }
    } catch (err) {
        console.error('Failed to load backup stats:', err);
    }
}

function downloadDatabaseBackup() {
    window.location.href = '/api/database/backup';
    showToast('Database snapshot download initiated', 'info');
}

async function pruneBandwidthHistory(btn) {
    const daysSelect = document.getElementById('prune-days-select');
    const days = parseInt(daysSelect?.value || '7', 10);

    if (confirm(`Are you sure you want to prune bandwidth samples older than ${days} days?`)) {
        if (btn) btn.disabled = true;
        try {
            const res = await fetch('/api/analytics/prune', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ days })
            });
            if (res.ok) {
                const data = await res.json();
                showToast(data.message || 'Bandwidth history pruned successfully!', 'success');
                loadBackupStats();
                loadDashboardData();
            } else {
                showToast('Failed to prune bandwidth history', 'error');
            }
        } catch (err) {
            console.error(err);
            showToast('Error pruning bandwidth history', 'error');
        } finally {
            if (btn) btn.disabled = false;
        }
    }
}

async function loadAuditLogs() {
    try {
        const res = await fetch('/api/logs');
        if (res.ok) {
            const logs = await res.json();
            const tbody = document.getElementById('audit-logs-table');
            tbody.innerHTML = '';

            logs.forEach(log => {
                const tr = document.createElement('tr');
                const dateStr = new Date(log.timestamp * 1000).toLocaleString();
                tr.innerHTML = `
                    <td style="font-family: var(--font-mono); font-size: 0.8rem; color: var(--text-dim);">${dateStr}</td>
                    <td><span class="tag">${log.action}</span></td>
                    <td><strong>${escapeHtml(log.target)}</strong></td>
                    <td style="color: var(--text-muted);">${escapeHtml(log.details)}</td>
                `;
                tbody.appendChild(tr);
            });
        }
    } catch (err) {
        console.error(err);
    }
}

// Helpers
function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function formatSpeed(bytesPerSec) {
    if (!bytesPerSec || bytesPerSec === 0) return '0 B/s';
    const k = 1024;
    const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
    const i = Math.floor(Math.log(bytesPerSec) / Math.log(k));
    return parseFloat((bytesPerSec / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function formatHandshake(ts) {
    if (!ts || ts === 0) return { text: 'Never Connected', class: 'gray' };
    const nowSec = Math.floor(Date.now() / 1000);
    const diff = nowSec - ts;
    if (diff < 0) return { text: 'Just Now', class: 'green' };
    if (diff < 120) return { text: `${diff}s ago`, class: 'green' };
    if (diff < 3600) return { text: `${Math.floor(diff / 60)}m ago`, class: 'yellow' };
    if (diff < 86400) return { text: `${Math.floor(diff / 3600)}h ago`, class: 'gray' };
    return { text: `${Math.floor(diff / 86400)}d ago`, class: 'gray' };
}

function escapeHtml(str) {
    if (!str) return '';
    return str.replace(/[&<>"']/g, match => ({
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
    }[match]));
}

function formatLatency(ms) {
    if (ms === null || ms === undefined || ms === 0) return '<span class="metric-badge gray">--</span>';
    if (ms < 50) return `<span class="metric-badge green">🟢 ${ms} ms</span>`;
    if (ms < 150) return `<span class="metric-badge yellow">🟡 ${ms} ms</span>`;
    return `<span class="metric-badge red">🔴 ${ms} ms</span>`;
}

function showToast(msg, type = 'success') {
    const container = document.getElementById('toast-container');
    if (!container) return;
    const toast = document.createElement('div');
    toast.style.cssText = `background: var(--bg-card); color: var(--text-main); border: 1px solid var(--border-glow); padding: 0.75rem 1.25rem; border-radius: var(--radius-md); box-shadow: 0 8px 24px rgba(0,0,0,0.4); font-size: 0.9rem; font-weight: 500; display: flex; align-items: center; gap: 0.5rem; transition: all 0.3s ease; opacity: 0; transform: translateY(10px);`;
    toast.innerHTML = `<span style="color: ${type === 'success' ? 'var(--accent-green)' : 'var(--accent-red)'}">●</span> ${escapeHtml(msg)}`;
    container.appendChild(toast);
    setTimeout(() => { toast.style.opacity = '1'; toast.style.transform = 'translateY(0)'; }, 10);
    setTimeout(() => { toast.style.opacity = '0'; toast.style.transform = 'translateY(10px)'; setTimeout(() => toast.remove(), 300); }, 3000);
}

async function copyToClipboard(text) {
    if (navigator.clipboard && window.isSecureContext) {
        try {
            await navigator.clipboard.writeText(text);
            return true;
        } catch (err) {
            // Fall through to execCommand
        }
    }
    const textarea = document.createElement('textarea');
    textarea.value = text;
    textarea.style.position = 'fixed';
    textarea.style.left = '-999999px';
    textarea.style.top = '-999999px';
    document.body.appendChild(textarea);
    textarea.focus();
    textarea.select();
    let success = false;
    try {
        success = document.execCommand('copy');
    } catch (err) {
        success = false;
    }
    document.body.removeChild(textarea);
    return success;
}

async function copyClientConfigDirect(peerId) {
    try {
        const res = await fetch(`/api/peers/${peerId}/download`);
        if (res.ok) {
            const text = await res.text();
            const copied = await copyToClipboard(text);
            if (copied) {
                showToast('Client configuration copied to clipboard!', 'success');
            } else {
                showToast('Failed to copy config to clipboard', 'error');
            }
        } else {
            showToast('Failed to fetch client config', 'error');
        }
    } catch (err) {
        console.error(err);
        showToast('Error copying config', 'error');
    }
}

async function exportAllClients() {
    const clients = currentPeers.filter(p => p.peer_type === 'client');
    if (clients.length === 0) {
        showToast('No clients to export', 'error');
        return;
    }
    for (const client of clients) {
        const link = document.createElement('a');
        link.href = `/api/peers/${client.id}/download`;
        link.download = `${client.name.replace(/[^a-z0-9]/gi, '_').toLowerCase()}.conf`;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        await new Promise(r => setTimeout(r, 250));
    }
    showToast(`Exported ${clients.length} client configuration files`, 'success');
}

let bandwidthChartState = null;

async function renderBandwidthChart() {
    try {
        const res = await fetch('/api/analytics/bandwidth');
        if (!res.ok) return;
        const rawSamples = await res.json();
        const svg = document.getElementById('bandwidth-chart');
        const wrapper = document.getElementById('bandwidth-chart-wrapper');
        const tooltip = document.getElementById('chart-tooltip');
        if (!svg || !wrapper || !Array.isArray(rawSamples) || rawSamples.length === 0) return;

        // Samples come newest-first from SQLite, reverse so chronological is left-to-right (past -> present)
        const samples = rawSamples.slice().reverse();

        let maxRx = 0, maxTx = 0;
        samples.forEach(s => {
            if (s.rx_speed_bps > maxRx) maxRx = s.rx_speed_bps;
            if (s.tx_speed_bps > maxTx) maxTx = s.tx_speed_bps;
        });

        // Update peak speed badges in UI header
        const peakRxEl = document.getElementById('chart-peak-rx');
        const peakTxEl = document.getElementById('chart-peak-tx');
        if (peakRxEl) peakRxEl.textContent = formatSpeed(maxRx);
        if (peakTxEl) peakTxEl.textContent = formatSpeed(maxTx);

        const w = 800, h = 210;
        const padLeft = 68, padRight = 20, padTop = 22, padBottom = 32;
        const plotW = w - padLeft - padRight;
        const plotH = h - padTop - padBottom;

        let peakMax = Math.max(maxRx, maxTx);
        let maxSpeed = peakMax <= 0 ? 1024 : peakMax * 1.15; // 15% headroom for clean visual peaks

        // 1. Generate Y-Axis Grid Lines & Speed Labels (100%, 66%, 33%, 0%)
        const yLevels = [1.0, 0.66, 0.33, 0.0];
        let gridSvg = '';
        yLevels.forEach(lvl => {
            const y = padTop + (1.0 - lvl) * plotH;
            const speedVal = lvl * (maxSpeed / 1.15);
            gridSvg += `
                <line x1="${padLeft}" y1="${y.toFixed(1)}" x2="${w - padRight}" y2="${y.toFixed(1)}" stroke="rgba(255, 255, 255, 0.08)" stroke-dasharray="3 3" stroke-width="1"/>
                <text x="${padLeft - 8}" y="${(y + 3.5).toFixed(1)}" fill="#64748b" font-size="10" font-family="ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace" text-anchor="end">${formatSpeed(speedVal)}</text>
            `;
        });

        // 2. Generate X-Axis Time Markers
        let timeSvg = '';
        const timeFractions = [0, 0.25, 0.5, 0.75, 1.0];
        timeFractions.forEach(frac => {
            const idx = Math.min(Math.floor(frac * (samples.length - 1)), samples.length - 1);
            if (idx >= 0 && samples[idx]) {
                const x = padLeft + (idx / Math.max(samples.length - 1, 1)) * plotW;
                const s = samples[idx];
                const d = new Date(s.timestamp * 1000);
                const timeStr = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
                timeSvg += `
                    <line x1="${x.toFixed(1)}" y1="${padTop + plotH}" x2="${x.toFixed(1)}" y2="${padTop + plotH + 4}" stroke="rgba(255, 255, 255, 0.15)" stroke-width="1"/>
                    <text x="${x.toFixed(1)}" y="${padTop + plotH + 18}" fill="#64748b" font-size="10" font-family="ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace" text-anchor="middle">${timeStr}</text>
                `;
            }
        });

        // 3. Calculate Path Coordinates
        let rxPoints = [], txPoints = [];
        const dataCoords = [];
        const step = plotW / Math.max(samples.length - 1, 1);

        samples.forEach((s, i) => {
            const x = padLeft + i * step;
            const yRx = padTop + plotH - ((s.rx_speed_bps / maxSpeed) * plotH);
            const yTx = padTop + plotH - ((s.tx_speed_bps / maxSpeed) * plotH);
            rxPoints.push(`${x.toFixed(1)},${yRx.toFixed(1)}`);
            txPoints.push(`${x.toFixed(1)},${yTx.toFixed(1)}`);
            dataCoords.push({ x, yRx, yTx, sample: s });
        });

        const rxD = 'M ' + rxPoints.join(' L ');
        const txD = 'M ' + txPoints.join(' L ');
        const rxArea = rxD + ` L ${(padLeft + plotW).toFixed(1)},${(padTop + plotH).toFixed(1)} L ${padLeft.toFixed(1)},${(padTop + plotH).toFixed(1)} Z`;
        const txArea = txD + ` L ${(padLeft + plotW).toFixed(1)},${(padTop + plotH).toFixed(1)} L ${padLeft.toFixed(1)},${(padTop + plotH).toFixed(1)} Z`;

        svg.innerHTML = `
            <defs>
                <linearGradient id="rxGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color="#10b981" stop-opacity="0.38"/>
                    <stop offset="100%" stop-color="#10b981" stop-opacity="0.0"/>
                </linearGradient>
                <linearGradient id="txGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color="#a855f7" stop-opacity="0.38"/>
                    <stop offset="100%" stop-color="#a855f7" stop-opacity="0.0"/>
                </linearGradient>
            </defs>
            <!-- Background Grid & Axes -->
            ${gridSvg}
            ${timeSvg}
            <!-- Area Fills -->
            <path d="${rxArea}" fill="url(#rxGrad)"/>
            <path d="${txArea}" fill="url(#txGrad)"/>
            <!-- Foreground Curves -->
            <path d="${rxD}" fill="none" stroke="#10b981" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
            <path d="${txD}" fill="none" stroke="#a855f7" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
            <!-- Interactive Hover Group -->
            <g id="chart-hover-group" style="display: none;">
                <line id="chart-hover-line" x1="0" y1="${padTop}" x2="0" y2="${padTop + plotH}" stroke="rgba(255, 255, 255, 0.45)" stroke-dasharray="3 3" stroke-width="1.5"/>
                <circle id="chart-dot-rx" cx="0" cy="0" r="4.5" fill="#10b981" stroke="#ffffff" stroke-width="2"/>
                <circle id="chart-dot-tx" cx="0" cy="0" r="4.5" fill="#a855f7" stroke="#ffffff" stroke-width="2"/>
            </g>
        `;

        bandwidthChartState = { samples, dataCoords, padLeft, padRight, padTop, padBottom, plotW, plotH, w, h, step };

        // Attach mouse events once
        if (!wrapper.dataset.hoverBound) {
            wrapper.dataset.hoverBound = 'true';

            wrapper.addEventListener('mousemove', (e) => {
                if (!bandwidthChartState || bandwidthChartState.dataCoords.length === 0) return;
                const rect = wrapper.getBoundingClientRect();
                const mouseX = e.clientX - rect.left;
                const mouseY = e.clientY - rect.top;

                // Scale to SVG 800x210 coordinate space
                const scaleX = bandwidthChartState.w / rect.width;
                const svgMouseX = mouseX * scaleX;

                // Find closest sample point
                let nearestIdx = Math.round((svgMouseX - bandwidthChartState.padLeft) / bandwidthChartState.step);
                nearestIdx = Math.max(0, Math.min(nearestIdx, bandwidthChartState.dataCoords.length - 1));
                const pt = bandwidthChartState.dataCoords[nearestIdx];

                const hoverGroup = document.getElementById('chart-hover-group');
                const hoverLine = document.getElementById('chart-hover-line');
                const dotRx = document.getElementById('chart-dot-rx');
                const dotTx = document.getElementById('chart-dot-tx');

                if (hoverGroup && hoverLine && dotRx && dotTx) {
                    hoverGroup.style.display = 'block';
                    hoverLine.setAttribute('x1', pt.x.toFixed(1));
                    hoverLine.setAttribute('x2', pt.x.toFixed(1));
                    dotRx.setAttribute('cx', pt.x.toFixed(1));
                    dotRx.setAttribute('cy', pt.yRx.toFixed(1));
                    dotTx.setAttribute('cx', pt.x.toFixed(1));
                    dotTx.setAttribute('cy', pt.yTx.toFixed(1));
                }

                if (tooltip) {
                    const timeStr = new Date(pt.sample.timestamp * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
                    const elapsedMins = Math.max(0, Math.floor((Date.now() / 1000 - pt.sample.timestamp) / 60));
                    const agoStr = elapsedMins === 0 ? 'Just now' : `${elapsedMins}m ago`;

                    tooltip.innerHTML = `
                        <div class="chart-tooltip-time">
                            <span style="font-weight: 600; color: #f8fafc;">🕒 ${timeStr}</span>
                            <span style="color: #94a3b8; font-size: 0.72rem;">${agoStr}</span>
                        </div>
                        <div class="chart-tooltip-row">
                            <span class="chart-tooltip-label" style="color: var(--accent-green);">
                                <span class="chart-dot" style="background: var(--accent-green);"></span> Inbound (Rx):
                            </span>
                            <strong style="color: var(--accent-green); margin-left: 0.75rem;">${formatSpeed(pt.sample.rx_speed_bps)}</strong>
                        </div>
                        <div class="chart-tooltip-row">
                            <span class="chart-tooltip-label" style="color: var(--accent-purple);">
                                <span class="chart-dot" style="background: var(--accent-purple);"></span> Outbound (Tx):
                            </span>
                            <strong style="color: var(--accent-purple); margin-left: 0.75rem;">${formatSpeed(pt.sample.tx_speed_bps)}</strong>
                        </div>
                    `;

                    tooltip.style.display = 'block';

                    // Compute tooltip dimensions
                    const tooltipW = tooltip.offsetWidth || 185;
                    const tooltipH = tooltip.offsetHeight || 75;

                    // Place tooltip to the right of cursor by default, flip to left if near right edge
                    let left = mouseX + 15;
                    if (left + tooltipW > rect.width - 12) {
                        left = mouseX - tooltipW - 15;
                    }
                    // Hard clamp inside container bounds (never clip on left or right)
                    left = Math.max(8, Math.min(left, rect.width - tooltipW - 8));

                    // Place tooltip above cursor by default, flip below if near top
                    let top = mouseY - tooltipH - 12;
                    if (top < 8) {
                        top = mouseY + 18;
                    }
                    // Hard clamp inside container bounds (never clip on top or bottom)
                    top = Math.max(8, Math.min(top, rect.height - tooltipH - 8));

                    tooltip.style.left = `${Math.round(left)}px`;
                    tooltip.style.top = `${Math.round(top)}px`;
                    tooltip.style.transform = 'none';
                }
            });

            wrapper.addEventListener('mouseleave', () => {
                const hoverGroup = document.getElementById('chart-hover-group');
                if (hoverGroup) hoverGroup.style.display = 'none';
                if (tooltip) tooltip.style.display = 'none';
            });
        }
    } catch (err) {
        console.error('Chart error:', err);
    }
}



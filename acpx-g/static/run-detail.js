// ─── acpx-g Run Detail Page (Nexus) ────────────────────────────

let runDetailState = {
  run: null,
  selectedNodeId: null,
  pollTimer: null,
};

function renderRunDetailPage(runId) {
  return `
  <div class="run-detail-page stagger">
    <div style="margin-bottom:16px;">
      <button class="back-btn" id="btnBackToRuns">
        <i data-lucide="arrow-left" style="width:14px;height:14px"></i>
        返回运行记录
      </button>
    </div>

    <div id="runDetailHeader">
      <div class="skeleton skeleton-card" style="height:60px;"></div>
    </div>

    <div id="runDetailStats" class="session-stats-row" style="display:none;"></div>

    <div class="run-detail-layout" id="runDetailLayout">
      <div class="run-detail-main">
        <div class="card" id="topologyCard">
          <div class="card-header">
            <span class="card-title">DAG 拓扑</span>
            <span id="topologyLive" style="font-size:11px;color:var(--status-active);display:none;align-items:center;gap:4px;">
              <span style="width:6px;height:6px;border-radius:50%;background:var(--status-active);animation:dot-breathe 2s ease-in-out infinite;"></span>
              实时
            </span>
          </div>
          <div class="card-body" style="padding:12px;">
            <div id="topologyContainer" class="topology-container" style="min-height:200px;">
              <div class="spinner-lg"></div>
            </div>
          </div>
        </div>

        <div class="card" id="nodeLogCard" style="display:none;margin-top:16px;">
          <div class="card-header">
            <span class="card-title" id="nodeLogTitle">节点日志</span>
            <button class="btn-icon btn-sm" id="btnCloseNodeLog"><i data-lucide="x" style="width:14px;height:14px"></i></button>
          </div>
          <div class="card-body" id="nodeLogBody" style="padding:0;"></div>
        </div>
      </div>

      <div class="run-detail-sidebar" id="runDetailSidebar">
        <div class="context-panel" id="contextPanel">
          <div class="context-section">
            <div class="context-section-title">工作流信息</div>
            <div id="contextInfo">加载中...</div>
          </div>
          <div class="context-section">
            <div class="context-section-title">节点统计</div>
            <div id="contextStats">加载中...</div>
          </div>
          <div class="context-section">
            <div class="context-section-title">节点列表</div>
            <div id="contextNodeList">加载中...</div>
          </div>
          <div class="context-section" id="contextActions" style="border-top:1px solid var(--border-subtle);padding:16px 20px;">
          </div>
        </div>
      </div>
    </div>
  </div>`;
}

function initRunDetail(runId) {
  loadRunDetail(runId);

  document.getElementById('btnBackToRuns')?.addEventListener('click', () => { location.hash = '#runs'; });
  document.getElementById('btnCloseNodeLog')?.addEventListener('click', closeNodeLog);

  // Event delegation for context panel
  document.getElementById('contextNodeList')?.addEventListener('click', (e) => {
    const item = e.target.closest('[data-node-id]');
    if (item?.dataset.nodeId) showNodeLogDetail(item.dataset.nodeId);
  });

  document.getElementById('contextActions')?.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-action]');
    if (!btn) return;
    const action = btn.dataset.action;
    const runId = btn.dataset.runId;
    if (action === 'cancel') cancelRun(runId);
    else if (action === 'rerun') rerunRun(runId);
    else if (action === 'delete') deleteRun(runId);
  });

  document.getElementById('contextInfo')?.addEventListener('click', (e) => {
    const el = e.target.closest('[data-action="copy-id"]');
    if (el?.dataset.id) copyToClipboard(el.dataset.id);
  });

  // Event delegation for topology SVG nodes
  document.getElementById('topologyContainer')?.addEventListener('click', (e) => {
    const g = e.target.closest('.topo-node-group');
    if (g?.dataset.nodeId) showNodeLogDetail(g.dataset.nodeId);
  });

  // Escape key navigates back to runs list
  const escHandler = (e) => {
    if (e.key === 'Escape' && AppState.currentPage === 'run-detail') {
      location.hash = '#runs';
    }
  };
  document.addEventListener('keydown', escHandler);
  // Store handler ref so we can clean up on navigate away
  AppState._runDetailEscHandler = escHandler;
}

async function loadRunDetail(runId) {
  const header = document.getElementById('runDetailHeader');
  if (!header) return;

  header.innerHTML = '<div class="skeleton skeleton-card" style="height:60px;"></div>';

  try {
    const run = await api(`${API_WF}/${runId}`);
    runDetailState.run = run;
    renderRunDetailHeader(run);
    renderRunDetailStats(run);
    renderTopology(run);
    renderContextPanel(run);
    renderContextActions(run);

    // Poll if running — clear existing timer first to prevent leak on refresh
    if (runDetailState.pollTimer) {
      clearInterval(runDetailState.pollTimer);
      AppState.pollTimers = AppState.pollTimers.filter(t => t !== runDetailState.pollTimer);
      runDetailState.pollTimer = null;
    }
    if (run.status === 'running' || run.status === 'pending') {
      runDetailState.pollTimer = setInterval(() => pollRunDetail(runId), 3000);
      AppState.pollTimers.push(runDetailState.pollTimer);
    }

    updateSidebarStatus();
  } catch (e) {
    header.innerHTML = `
      <div class="empty-state">
        <div class="empty-state-icon"><i data-lucide="alert-triangle" style="width:28px;height:28px"></i></div>
        <div class="empty-state-title">加载失败</div>
        <div class="empty-state-desc">${escapeHtml(e.message)}</div>
      </div>`;
    lucide.createIcons({ nodes: [header] });
    // Hide loading skeletons on error
    const stats = document.getElementById('runDetailStats');
    if (stats) stats.style.display = 'none';
    const layout = document.getElementById('runDetailLayout');
    if (layout) layout.style.display = 'none';
  }
}

async function pollRunDetail(runId) {
  try {
    const run = await api(`${API_WF}/${runId}`);
    runDetailState.run = run;
    renderRunDetailStats(run);
    renderTopology(run);
    renderContextPanel(run);
    renderContextActions(run);

    if (run.status !== 'running' && run.status !== 'pending') {
      clearInterval(runDetailState.pollTimer);
      runDetailState.pollTimer = null;
      renderRunDetailHeader(run);
      const live = document.getElementById('topologyLive');
      if (live) live.style.display = 'none';
    }
  } catch (e) {
    clearInterval(runDetailState.pollTimer);
    runDetailState.pollTimer = null;
    showToast('实时更新已停止: ' + e.message, 'warning');
  }
}

function renderRunDetailHeader(run) {
  const header = document.getElementById('runDetailHeader');
  if (!header) return;

  // Update page title with workflow name for tab identification
  document.title = `${run.workflow_name || '运行详情'} — ACPX-G`;

  header.innerHTML = `
    <div class="session-header" style="display:flex;align-items:flex-start;justify-content:space-between;">
      <div>
        <div class="session-title">${escapeHtml(run.workflow_name)}</div>
        <div class="session-meta">
          <span class="status-indicator">
            <span class="status-dot ${statusClass(run.status)}"></span>
            <span class="status-text">${statusText(run.status)}</span>
          </span>
          <span>·</span>
          <span>版本 ${escapeHtml(run.workflow_version || '1.0')}</span>
          <span>·</span>
          <span>${relativeTime(run.created_at)}</span>
        </div>
      </div>
      <button class="btn btn-sm btn-ghost" id="btnRefreshRunDetail" title="刷新"><i data-lucide="refresh-cw" style="width:14px;height:14px"></i></button>
    </div>`;

  document.getElementById('btnRefreshRunDetail')?.addEventListener('click', () => loadRunDetail(run.id));

  if (run.error_message) {
    header.innerHTML += `
      <div style="margin-top:8px;padding:8px 12px;border-radius:var(--radius);background:rgba(239,68,68,0.08);border:1px solid rgba(239,68,68,0.2);font-size:12px;color:var(--status-error);font-family:var(--font-mono);">
        ${escapeHtml(run.error_message)}
      </div>`;
  }

  lucide.createIcons({ nodes: [header] });
}

function renderRunDetailStats(run) {
  const el = document.getElementById('runDetailStats');
  if (!el) return;

  const nodes = run.nodes || [];
  const successCount = nodes.filter(n => n.status === 'success').length;
  const failedCount = nodes.filter(n => n.status === 'failed').length;
  const runningCount = nodes.filter(n => n.status === 'running').length;

  el.style.display = 'flex';
  el.innerHTML = `
    <div class="session-stat">
      <div class="session-stat-icon brand"><i data-lucide="git-branch" style="width:12px;height:12px"></i></div>
      <div>
        <div class="session-stat-label">节点</div>
        <div class="session-stat-value">${nodes.length}</div>
      </div>
    </div>
    <div class="session-stat">
      <div class="session-stat-icon green"><i data-lucide="check-circle" style="width:12px;height:12px"></i></div>
      <div>
        <div class="session-stat-label">成功</div>
        <div class="session-stat-value">${successCount}</div>
      </div>
    </div>
    <div class="session-stat">
      <div class="session-stat-icon"><i data-lucide="x-circle" style="width:12px;height:12px;${failedCount ? 'color:var(--status-error)' : ''}"></i></div>
      <div>
        <div class="session-stat-label">失败</div>
        <div class="session-stat-value">${failedCount}</div>
      </div>
    </div>
    <div class="session-stat">
      <div class="session-stat-icon cyan"><i data-lucide="clock" style="width:12px;height:12px"></i></div>
      <div>
        <div class="session-stat-label">耗时</div>
        <div class="session-stat-value">${formatDuration(run.started_at, run.finished_at)}</div>
      </div>
    </div>`;

  lucide.createIcons({ nodes: [el] });
}

function renderTopology(run) {
  const container = document.getElementById('topologyContainer');
  const live = document.getElementById('topologyLive');
  if (!container) return;

  const nodes = run.nodes || [];
  if (!nodes.length) {
    container.innerHTML = '<div class="empty-state" style="padding:20px;"><span style="color:var(--text-dim);font-size:12px;">无节点</span></div>';
    return;
  }

  if (run.status === 'running' && live) live.style.display = 'flex';

  // Build Dagre graph
  if (typeof dagre === 'undefined') {
    container.innerHTML = '<div style="color:var(--text-dim);font-size:12px;">Dagre 库加载中...</div>';
    return;
  }

  const NODE_W = 160, NODE_H = 52;
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: 'TB', nodesep: 40, ranksep: 60, marginx: 40, marginy: 30 });
  g.setDefaultEdgeLabel(() => ({}));

  nodes.forEach(n => g.setNode(n.node_id, { width: NODE_W, height: NODE_H }));
  nodes.forEach(n => {
    const raw = n.depends || [];
    let deps;
    try { deps = Array.isArray(raw) ? raw : (typeof raw === 'string' ? JSON.parse(raw || '[]') : []); } catch (_) { deps = []; }
    deps.forEach(dep => { if (g.node(dep)) g.setEdge(dep, n.node_id); });
  });

  dagre.layout(g);

  // Render SVG
  const bounds = { x1: Infinity, y1: Infinity, x2: -Infinity, y2: -Infinity };
  g.nodes().forEach(id => {
    const pos = g.node(id);
    if (!pos) return;
    bounds.x1 = Math.min(bounds.x1, pos.x - NODE_W / 2);
    bounds.y1 = Math.min(bounds.y1, pos.y - NODE_H / 2);
    bounds.x2 = Math.max(bounds.x2, pos.x + NODE_W / 2);
    bounds.y2 = Math.max(bounds.y2, pos.y + NODE_H / 2);
  });

  const pad = 30;
  const contentW = bounds.x2 - bounds.x1 + pad * 2;
  const contentH = bounds.y2 - bounds.y1 + pad * 2;
  // Lock minimum viewBox so nodes don't scale up when there are few
  const MIN_SVG = 400;
  const svgW = Math.max(contentW, MIN_SVG);
  const svgH = Math.max(contentH, MIN_SVG);
  const ox = -bounds.x1 + pad + (svgW - contentW) / 2;
  const oy = -bounds.y1 + pad + (svgH - contentH) / 2;

  const statusColors = {
    pending: '#94A3B8', running: '#F59E0B', success: '#10B981',
    failed: '#EF4444', cancelled: '#F97316', skipped: '#CBD5E1',
  };

  const typeColors = {
    shell: '#6366F1', agent: '#8250DF', reference: '#10B981',
  };

  let svg = `<svg viewBox="0 0 ${svgW} ${svgH}" width="100%" style="max-height:400px;">`;
  svg += '<defs>';
  svg += '<filter id="topoShadow"><feDropShadow dx="0" dy="1" stdDeviation="2" flood-opacity="0.08"/></filter>';
  svg += `<linearGradient id="lineGradActive" x1="0%" y1="0%" x2="100%" y2="0%">
    <stop offset="0%" stop-color="#6366F1" stop-opacity="0.5"/>
    <stop offset="100%" stop-color="#22D3EE" stop-opacity="0.3"/>
  </linearGradient>`;
  svg += '</defs>';

  // Edges
  g.edges().forEach(({ v, w }) => {
    const from = g.node(v);
    const to = g.node(w);
    if (!from || !to) return;
    const x1 = from.x + ox, y1 = from.y + NODE_H / 2 + oy;
    const x2 = to.x + ox, y2 = to.y - NODE_H / 2 + oy;

    const fromNode = nodes.find(n => n.node_id === v);
    const toNode = nodes.find(n => n.node_id === w);
    const isActive = fromNode?.status === 'success' && (toNode?.status === 'success' || toNode?.status === 'running');

    svg += `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" ${isActive ? 'stroke="#6366F1" stroke-width="2" stroke-opacity="0.7"' : 'class="topo-line"'}/>`;
  });

  // Nodes
  g.nodes().forEach(id => {
    const pos = g.node(id);
    if (!pos) return;
    const node = nodes.find(n => n.node_id === id);
    if (!node) return;

    const x = pos.x - NODE_W / 2 + ox;
    const y = pos.y - NODE_H / 2 + oy;
    const color = statusColors[node.status] || '#94A3B8';
    const isRunning = node.status === 'running';

    const animStyle = isRunning ? 'style="animation: glow-breathe 3s ease-in-out infinite;"' : '';
    const pulseCircle = isRunning ? `<circle cx="${pos.x + ox}" cy="${pos.y + oy}" r="${NODE_H / 2 + 4}" fill="none" stroke="${color}" stroke-width="1" opacity="0.3" style="animation: pulse-ring 2s ease-out infinite;"/>` : '';

    const statusDot = `<circle cx="${x + 16}" cy="${pos.y + oy}" r="4" fill="${color}"/>`;

    // Truncate node ID for display (max 16 chars to fit in NODE_W=160)
    const maxTextLen = 16;
    const displayId = id.length > maxTextLen ? id.substring(0, maxTextLen - 1) + '…' : id;

    svg += `<g class="topo-node-group" style="cursor:pointer;" data-node-id="${escapeHtml(id)}">`;
    svg += `<title>${escapeHtml(id)} — ${statusText(node.status)}</title>`;
    svg += pulseCircle;
    svg += `<rect x="${x}" y="${y}" width="${NODE_W}" height="${NODE_H}" rx="10" fill="#FFFFFF" stroke="${color}" stroke-width="${isRunning ? 2 : 1.5}" filter="url(#topoShadow)" ${animStyle}/>`;
    svg += statusDot;
    svg += `<text x="${x + 28}" y="${pos.y + oy - 2}" text-anchor="start" fill="var(--text-bright, #0F172A)" font-family="var(--font-display, system-ui)" font-size="12" font-weight="600">${escapeHtml(displayId)}</text>`;
    svg += `<text x="${x + 28}" y="${pos.y + oy + 12}" text-anchor="start" fill="${color}" font-size="10">${statusText(node.status)} · ${nodeTypeLabel(node.node_type)}</text>`;
    svg += '</g>';
  });

  svg += '</svg>';
  container.innerHTML = svg;
}

function renderContextPanel(run) {
  const nodes = run.nodes || [];

  // Info
  const infoEl = document.getElementById('contextInfo');
  if (infoEl) {
    infoEl.innerHTML = `
      <div class="context-info-row"><span class="context-info-label">工作流</span><span class="context-info-value">${escapeHtml(run.workflow_name)}</span></div>
      <div class="context-info-row"><span class="context-info-label">版本</span><span class="context-info-value">v${escapeHtml(run.workflow_version || '1.0')}</span></div>
      <div class="context-info-row"><span class="context-info-label">状态</span><span class="context-info-value" style="color:${run.status === 'success' ? 'var(--status-active)' : run.status === 'failed' ? 'var(--status-error)' : 'var(--text-bright)'}">${statusText(run.status)}</span></div>
      <div class="context-info-row"><span class="context-info-label">ID</span><span class="context-info-value" style="font-size:10px;cursor:pointer;" title="点击复制完整 ID" data-action="copy-id" data-id="${escapeHtml(run.id)}">${escapeHtml(run.id?.substring(0, 12))}...</span></div>
    `;
  }

  // Stats
  const statsEl = document.getElementById('contextStats');
  if (statsEl) {
    const statusCounts = {};
    nodes.forEach(n => { statusCounts[n.status] = (statusCounts[n.status] || 0) + 1; });
    const total = nodes.length || 1;

    let html = '<div class="tool-bar-group">';
    const order = ['success', 'running', 'pending', 'failed', 'cancelled', 'skipped'];
    const barColors = { success: 'green', running: 'brand', pending: 'brand', failed: 'brand', cancelled: 'brand', skipped: 'brand' };

    order.forEach(status => {
      const count = statusCounts[status] || 0;
      if (!count) return;
      const pct = Math.round((count / total) * 100);
      html += `
        <div class="tool-bar-item">
          <span class="tool-bar-name">${statusText(status)}</span>
          <div class="tool-bar-track"><div class="tool-bar-fill ${barColors[status]}" style="width:${pct}%"></div></div>
          <span class="tool-bar-count">${count}</span>
        </div>`;
    });

    html += '</div>';
    statsEl.innerHTML = html;
  }

  // Node List
  const listEl = document.getElementById('contextNodeList');
  if (listEl) {
    listEl.innerHTML = nodes.slice().reverse().map(n => `
      <div class="context-node-item ${runDetailState.selectedNodeId === n.node_id ? 'selected' : ''}" data-node-id="${escapeHtml(n.node_id)}" style="display:flex;align-items:center;gap:8px;padding:6px 0;cursor:pointer;border-bottom:1px solid var(--border-subtle);">
        <span class="status-dot ${statusClass(n.status)}" style="width:6px;height:6px;"></span>
        <span style="flex:1;font-size:12px;color:var(--text-primary);font-family:var(--font-mono);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(n.node_id)}</span>
        <span style="font-size:10px;color:var(--text-dim);">${nodeTypeLabel(n.node_type)}</span>
      </div>
    `).join('');
  }
}

function renderContextActions(run) {
  const el = document.getElementById('contextActions');
  if (!el) return;

  let html = '';
  if (run.status === 'running' || run.status === 'pending') {
    html += `<button class="btn btn-sm btn-danger" style="width:100%;" data-action="cancel" data-run-id="${escapeHtml(run.id)}"><i data-lucide="square" style="width:14px;height:14px"></i> 取消运行</button>`;
  } else {
    html += `<button class="btn btn-sm btn-secondary" style="width:100%;" data-action="rerun" data-run-id="${escapeHtml(run.id)}"><i data-lucide="rotate-cw" style="width:14px;height:14px"></i> 重新运行</button>`;
    html += `<button class="btn btn-sm btn-danger-ghost" style="width:100%;margin-top:6px;" data-action="delete" data-run-id="${escapeHtml(run.id)}"><i data-lucide="trash-2" style="width:14px;height:14px"></i> 删除</button>`;
  }

  el.innerHTML = html;
  lucide.createIcons({ nodes: [el] });
}

async function showNodeLogDetail(nodeId) {
  runDetailState.selectedNodeId = nodeId;
  const run = runDetailState.run;
  if (!run) return;

  const card = document.getElementById('nodeLogCard');
  const title = document.getElementById('nodeLogTitle');
  const body = document.getElementById('nodeLogBody');
  if (!card || !title || !body) return;

  card.style.display = 'block';
  title.textContent = `节点日志: ${nodeId}`;

  body.innerHTML = '<div style="padding:20px;display:flex;justify-content:center;"><div class="spinner-lg"></div></div>';
  body.scrollTop = 0;

  // Highlight in context list
  document.querySelectorAll('.context-node-item').forEach(el => {
    el.classList.toggle('selected', el.dataset.nodeId === nodeId);
  });

  try {
    const logs = await api(`${API_WF}/${run.id}/nodes/${encodeURIComponent(nodeId)}/logs`);
    const node = (run.nodes || []).find(n => n.node_id === nodeId);

    let html = '';

    if (logs.stdout) {
      html += `
        <div style="border-bottom:1px solid var(--border-subtle);">
          <div style="display:flex;align-items:center;justify-content:space-between;padding:8px 16px;border-bottom:1px solid var(--border-subtle);">
            <span style="font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:0.05em;color:var(--text-dim);">STDOUT</span>
            <button class="code-copy-btn log-copy-btn" data-log-id="log-stdout">复制</button>
          </div>
          <pre class="node-log-pre" id="log-stdout">${escapeHtml(logs.stdout)}</pre>
        </div>`;
    }

    if (logs.stderr) {
      html += `
        <div style="border-bottom:1px solid var(--border-subtle);">
          <div style="display:flex;align-items:center;justify-content:space-between;padding:8px 16px;border-bottom:1px solid var(--border-subtle);">
            <span style="font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:0.05em;color:var(--status-error);">STDERR</span>
            <button class="code-copy-btn log-copy-btn" data-log-id="log-stderr">复制</button>
          </div>
          <pre class="node-log-pre node-log-stderr" id="log-stderr">${escapeHtml(logs.stderr)}</pre>
        </div>`;
    }

    if (node?.error_message) {
      html += `
        <div>
          <div style="padding:8px 16px;border-bottom:1px solid var(--border-subtle);">
            <span style="font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:0.05em;color:var(--status-error);">ERROR</span>
          </div>
          <div style="padding:8px 16px;font-size:12px;color:var(--status-error);font-family:var(--font-mono);">${escapeHtml(node.error_message)}</div>
        </div>`;
    }

    if (!logs.stdout && !logs.stderr && !node?.error_message) {
      html = '<div style="padding:20px;text-align:center;color:var(--text-dim);font-size:12px;">无输出</div>';
    }

    body.innerHTML = html;

    // Bind log copy buttons
    body.querySelectorAll('.log-copy-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const targetId = btn.dataset.logId;
        const pre = targetId ? document.getElementById(targetId) : null;
        if (pre) copyToClipboard(pre.textContent);
      });
    });
  } catch (e) {
    body.innerHTML = `<div style="padding:20px;text-align:center;color:var(--status-error);font-size:12px;">${escapeHtml(e.message)}</div>`;
  }
}

function closeNodeLog() {
  const card = document.getElementById('nodeLogCard');
  if (card) card.style.display = 'none';
  runDetailState.selectedNodeId = null;
  document.querySelectorAll('.context-node-item').forEach(el => el.classList.remove('selected'));
}

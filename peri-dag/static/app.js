// ── State ───────────────────────────────────────────────────────────
const API_WF = '/api/v1/workflows';
const API_TPL = '/api/v1/templates';
let selectedRunId = null;
let selectedTemplateName = null;
let allTemplates = [];
let refreshTimer = null;
let dagZoom = 1, dagPanX = 0, dagPanY = 0;
let dragging = false, dragStart = {};

// ── Toast notifications ──────────────────────────────────────────────
function showToast(msg, type='error') {
  let container = document.getElementById('toast-container');
  if (!container) {
    container = document.createElement('div');
    container.id = 'toast-container';
    document.body.appendChild(container);
  }
  const el = document.createElement('div');
  el.className = `toast toast-${type}`;
  el.textContent = msg;
  container.appendChild(el);
  setTimeout(() => { el.classList.add('fade-out'); setTimeout(() => el.remove(), 300); }, 4000);
}

// ── Status helpers ──────────────────────────────────────────────────
const NODE_ICONS = { shell: '\u25B6', agent: '\u2726', reference: '\u21BB' };

function sb(s) { return `sb-${s || 'pending'}`; }
function statusLabel(s) { return (s || 'pending'); }
function statusBadge(s) {
  return `<span class="status-badge ${sb(s)}"><span class="dot"></span>${statusLabel(s)}</span>`;
}

function nodeFill(s) {
  return { pending:'#eaeef2', running:'#ddf4ff', success:'#dafbe1', failed:'#ffebe9', skipped:'#f6f8fa' }[s] || '#eaeef2';
}
function nodeStroke(s) {
  return { pending:'#8b949e', running:'#0969da', success:'#1a7f37', failed:'#cf222e', skipped:'#8b949e' }[s] || '#8b949e';
}

// ── Mode switching ──────────────────────────────────────────────────
function showTemplatePreview(name) {
  selectedTemplateName = name;
  selectedRunId = null;
  clearInterval(refreshTimer);

  const tpl = allTemplates.find(t => t.name === name);
  if (!tpl) return;

  document.querySelectorAll('.template-card').forEach(el => {
    el.classList.toggle('active', el.dataset.name === name);
  });
  document.querySelectorAll('.run-item').forEach(el => el.classList.remove('active'));

  const hdr = document.getElementById('preview-header');
  hdr.classList.add('visible');
  document.getElementById('preview-name').textContent = tpl.name;
  document.getElementById('preview-desc').textContent = tpl.description || `v${tpl.version} · ${tpl.node_count} nodes`;
  document.getElementById('preview-run-btn').onclick = () => runTemplate(name);

  // Render inputs form if the template declares inputs
  const inputsEl = document.getElementById('preview-inputs');
  const inputs = tpl.inputs || {};
  const inputKeys = Object.keys(inputs);
  if (inputKeys.length > 0) {
    inputsEl.style.display = 'block';
    inputsEl.innerHTML = '<div class="inputs-title">Inputs</div>' +
      inputKeys.map(k => {
        const def = inputs[k];
        const req = def.required ? ' <span class="req">*</span>' : '';
        const ph = def.default ? `placeholder="default: ${esc(def.default)}"` : '';
        return `<div class="input-field">
          <label>${esc(k)}${req}</label>
          <input type="text" data-input-key="${esc(k)}" ${ph} value="${esc(def.default || '')}">
        </div>`;
      }).join('');
  } else {
    inputsEl.style.display = 'none';
    inputsEl.innerHTML = '';
  }

  const previewNodes = (tpl.nodes || []).map(n => ({
    node_id: n.id, node_type: n.node_type, depends: n.depends, status: 'pending'
  }));
  renderGraph(previewNodes, null);

  document.getElementById('log-panel').innerHTML = '<div class="log-empty">Run this template to see logs</div>';
}

function showRunView() {
  selectedTemplateName = null;
  document.getElementById('preview-header').classList.remove('visible');
  document.querySelectorAll('.template-card').forEach(el => el.classList.remove('active'));
}

// ── Templates ───────────────────────────────────────────────────────
async function loadTemplates() {
  const el = document.getElementById('templates-section');
  try {
    const r = await fetch(API_TPL);
    const d = await r.json();
    const tpls = d.templates || [];
    allTemplates = tpls;
    if (!tpls.length) {
      el.innerHTML = '<div class="template-empty">No templates found<br><span style="font-size:11px">Start with: peri-dag --workflow-dir ./examples</span></div>';
      document.getElementById('graph-placeholder').textContent = 'Add a workflow directory to get started';
      document.getElementById('log-panel').innerHTML = '<div class="log-empty">Workflow logs will appear here</div>';
      return;
    }
    el.innerHTML = tpls.map(t => `
      <div class="template-card${selectedTemplateName===t.name?' active':''}" data-name="${esc(t.name)}">
        <div class="template-name">${esc(t.name)}</div>
        <div class="template-desc">${esc(t.description || 'No description')}</div>
        <div class="template-meta">
          <span class="tag">v${esc(t.version)}</span>
          <span class="tag">${t.node_count} nodes</span>
          <span class="tag" title="${esc(t.file_path)}">${esc(basename(t.file_path))}</span>
        </div>
        <div class="template-actions">
          <button class="btn btn-primary btn-sm run-btn" data-name="${esc(t.name)}">&#9654; Run</button>
        </div>
      </div>
    `).join('');

    // Event delegation for template cards
    el.querySelectorAll('.template-card').forEach(card => {
      card.addEventListener('click', () => showTemplatePreview(card.dataset.name));
    });
    el.querySelectorAll('.run-btn').forEach(btn => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        runTemplateFromCard(btn.dataset.name);
      });
    });

    // Auto-select first template on initial load
    if (!selectedRunId && !selectedTemplateName) {
      showTemplatePreview(tpls[0].name);
    }
  } catch(e) {
    el.innerHTML = '<div class="template-empty" style="color:#cf222e">Failed to load templates</div>';
  }
}

// Run from card: select template first (shows inputs form), then run
function runTemplateFromCard(name) {
  showTemplatePreview(name);
  const tpl = allTemplates.find(t => t.name === name);
  const inputKeys = Object.keys(tpl?.inputs || {});
  if (inputKeys.length === 0) {
    // No inputs required, run immediately
    runTemplate(name);
  }
  // If template has inputs, user fills them in the preview form and clicks Run
}

async function runTemplate(name) {
  // Collect inputs from the preview form with validation
  const inputsEl = document.getElementById('preview-inputs');
  const inputs = {};
  let valid = true;
  if (inputsEl) {
    inputsEl.querySelectorAll('input[data-input-key]').forEach(inp => {
      inp.style.borderColor = '';
      const key = inp.dataset.inputKey;
      const label = inp.closest('.input-field')?.querySelector('label');
      const isRequired = label && label.querySelector('.req');
      if (!inp.value.trim() && isRequired) {
        inp.style.borderColor = '#cf222e';
        valid = false;
      } else if (inp.value.trim()) {
        inputs[key] = inp.value.trim();
      }
    });
  }

  if (!valid) {
    showToast('Please fill in all required inputs', 'error');
    return;
  }

  try {
    const r = await fetch(`${API_TPL}/${encodeURIComponent(name)}/run`, {
      method:'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ inputs: Object.keys(inputs).length ? inputs : undefined }),
    });
    const d = await r.json();
    if (r.ok) {
      selectedRunId = d.run_id;
      showRunView();
      loadRuns();
      selectRun(d.run_id);
      showToast('Workflow started', 'success');
    } else {
      showToast(d.error || 'Failed to start workflow', 'error');
    }
  } catch(e) {
    showToast('Network error: ' + e.message, 'error');
  }
}

// ── Runs ────────────────────────────────────────────────────────────
async function loadRuns() {
  const el = document.getElementById('run-list');
  try {
    const r = await fetch(API_WF);
    const d = await r.json();
    const runs = d.runs || [];
    document.getElementById('run-count').textContent = runs.length ? `${runs.length} total` : '';
    if (!runs.length) { el.innerHTML = '<div class="log-empty" style="padding:24px">No runs yet</div>'; return; }
    el.innerHTML = runs.map(run => {
      const cls = run.id === selectedRunId ? 'run-item active' : 'run-item';
      return `<div class="${cls}" data-run-id="${esc(run.id)}">
        <div class="run-name">${statusBadge(run.status)} ${esc(run.workflow_name)}</div>
        <div class="run-meta">
          <span>v${esc(run.workflow_version)}</span>
          <span>${run.node_count} nodes</span>
          <span>${fmtTime(run.created_at)}</span>
          <span>${fmtDuration(run.started_at, run.finished_at)}</span>
        </div>
      </div>`;
    }).join('');

    // Event delegation for run items
    el.querySelectorAll('.run-item').forEach(item => {
      item.addEventListener('click', () => selectRun(item.dataset.runId));
    });
  } catch(e) {
    el.innerHTML = '<div class="log-empty" style="color:#cf222e">Failed to load</div>';
  }
}

// ── Select Run ──────────────────────────────────────────────────────
async function selectRun(id) {
  selectedRunId = id;
  showRunView();
  loadRuns();
  clearInterval(refreshTimer);

  try {
    const r = await fetch(`${API_WF}/${id}`);
    const run = await r.json();
    renderGraph(run.nodes || [], run);
    renderLogs(run);

    // Schedule polling for active runs — single fetch, no duplicate
    if (run.status === 'running' || run.status === 'pending') {
      refreshTimer = setInterval(() => selectRun(id), 2000);
    }
  } catch(e) {}
}

// ── DAG Graph ───────────────────────────────────────────────────────
const NODE_W = 140, NODE_H = 48, LEVEL_GAP = 90, NODE_GAP = 14, PAD = 40;

function renderGraph(nodes, runCtx) {
  const ph = document.getElementById('graph-placeholder');
  const svgEl = document.getElementById('graph-svg');
  const zoomCtls = document.getElementById('zoom-ctls');

  if (!nodes.length) {
    ph.style.display = 'flex'; svgEl.style.display = 'none'; zoomCtls.style.display = 'none';
    return;
  }

  ph.style.display = 'none';
  svgEl.style.display = 'block';
  zoomCtls.style.display = 'flex';

  const idxMap = {}; nodes.forEach((n,i) => idxMap[n.node_id] = i);
  const adj = nodes.map(() => []);
  const deg = nodes.map(() => 0);
  nodes.forEach((n,i) => {
    (n.depends || []).forEach(d => {
      const j = idxMap[d];
      if (j !== undefined) { adj[j].push(i); deg[i]++; }
    });
  });

  // Topological sort
  const levels = [];
  const inDeg = [...deg];
  let queue = nodes.map((_,i)=>i).filter(i => inDeg[i]===0);
  while (queue.length) {
    levels.push([...queue]);
    const next = [];
    queue.forEach(i => adj[i].forEach(j => { inDeg[j]--; if (inDeg[j]===0) next.push(j); }));
    queue = next;
  }
  const placed = new Set(levels.flat());
  const remaining = nodes.map((_,i)=>i).filter(i => !placed.has(i));
  if (remaining.length) levels.push(remaining);

  // Layout — top-to-bottom
  const maxPerLevel = Math.max(1, ...levels.map(l => l.length));
  const totalW = maxPerLevel * (NODE_W + NODE_GAP) - NODE_GAP + PAD * 2;
  const totalH = levels.length * (NODE_H + LEVEL_GAP) - LEVEL_GAP + PAD * 2;

  // Only reset zoom/pan when switching between different content (run vs template)
  const prevNodes = svgEl.dataset.nodeCount || '';
  const curNodes = String(nodes.length);
  if (prevNodes !== curNodes) {
    dagZoom = 1; dagPanX = 0; dagPanY = 0;
    svgEl.dataset.nodeCount = curNodes;
  }

  const pos = {};
  levels.forEach((level, li) => {
    const y = PAD + li * (NODE_H + LEVEL_GAP);
    const startX = PAD + (maxPerLevel - level.length) * (NODE_W + NODE_GAP) / 2;
    level.forEach((ni, si) => {
      pos[ni] = { x: startX + si * (NODE_W + NODE_GAP), y };
    });
  });

  let html = `<defs><marker id="arrow" markerWidth="8" markerHeight="6" refX="8" refY="3" orient="auto">
    <path d="M0,0 L8,3 L0,6 Z" class="edge-arrow"/>
  </marker></defs>`;

  // Edges
  nodes.forEach((n,i) => {
    (n.depends || []).forEach(d => {
      const j = idxMap[d];
      if (j === undefined || !pos[i] || !pos[j]) return;
      const from = pos[j], to = pos[i];
      const x1 = from.x + NODE_W / 2, y1 = from.y + NODE_H;
      const x2 = to.x + NODE_W / 2, y2 = to.y;
      html += `<path d="M${x1} ${y1} C${x1} ${(y1+y2)/2} ${x2} ${(y1+y2)/2} ${x2} ${y2}" class="edge-line" marker-end="url(#arrow)"/>`;
    });
  });

  // Nodes
  const isPreview = !runCtx;
  nodes.forEach((n,i) => {
    if (!pos[i]) return;
    const {x, y} = pos[i];
    const status = n.status || 'pending';
    const icon = NODE_ICONS[n.node_type] || '';

    let clickAttr = '';
    let subText = esc(n.node_type) + ' \u00b7 ' + statusLabel(status);
    if (!isPreview && n.node_id) {
      clickAttr = ` data-jump-node="${cssEsc(n.node_id)}"`;
      if (n.exit_code != null) subText += ' exit=' + n.exit_code;
    }

    html += `<g class="node-group"${clickAttr}>
      <rect x="${x}" y="${y}" width="${NODE_W}" height="${NODE_H}" class="node-rect" fill="${nodeFill(status)}" stroke="${nodeStroke(status)}"/>
      <text x="${x + 8}" y="${y + 16}" class="node-icon">${icon}</text>
      <text x="${x + NODE_W/2}" y="${y + 16}" class="node-title">${esc(n.node_id)}</text>
      <text x="${x + NODE_W/2}" y="${y + 34}" class="node-sub">${subText}</text>
    </g>`;
  });

  svgEl.setAttribute('viewBox', `0 0 ${Math.max(totalW,400)} ${Math.max(totalH,200)}`);
  svgEl.innerHTML = html;
  applyTransform();

  // Event delegation for DAG node clicks → jump to log
  svgEl.querySelectorAll('.node-group[data-jump-node]').forEach(g => {
    g.addEventListener('click', () => jumpToLog(g.dataset.jumpNode));
  });
}

function applyTransform() {
  const svg = document.getElementById('graph-svg');
  svg.style.transform = `translate(${dagPanX}px,${dagPanY}px) scale(${dagZoom})`;
  svg.style.transformOrigin = '0 0';
}

function zoomIn()  { dagZoom = Math.min(3, dagZoom * 1.25); applyTransform(); }
function zoomOut() { dagZoom = Math.max(0.25, dagZoom / 1.25); applyTransform(); }
function zoomFit() { dagZoom = 1; dagPanX = 0; dagPanY = 0; applyTransform(); }

// Pan with mouse drag
(function() {
  const svg = document.getElementById('graph-svg');
  svg.addEventListener('mousedown', e => {
    dragging = true; dragStart = { x: e.clientX - dagPanX, y: e.clientY - dagPanY };
    e.preventDefault();
  });
  window.addEventListener('mousemove', e => {
    if (!dragging) return;
    dagPanX = e.clientX - dragStart.x;
    dagPanY = e.clientY - dragStart.y;
    applyTransform();
  });
  window.addEventListener('mouseup', () => { dragging = false; });
  svg.addEventListener('wheel', e => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    dagZoom = Math.max(0.25, Math.min(3, dagZoom * delta));
    applyTransform();
  });
})();

function jumpToLog(nodeRunId) {
  const el = document.querySelector(`.log-section[data-node="${cssEsc(nodeRunId)}"]`);
  if (el) {
    el.scrollIntoView({ behavior:'smooth', block:'center' });
    const body = el.querySelector('.log-body');
    if (body && !body.classList.contains('open')) el.querySelector('.log-header').click();
  }
}

// ── Logs ────────────────────────────────────────────────────────────
function renderLogs(run) {
  const nodes = run.nodes || [];
  const panel = document.getElementById('log-panel');
  if (!nodes.length) { panel.innerHTML = '<div class="log-empty">No nodes</div>'; return; }

  panel.innerHTML = `<h3>${statusBadge(run.status)} ${esc(run.workflow_name)} <span style="font-weight:400">${fmtDuration(run.started_at, run.finished_at)}</span></h3>` +
    nodes.map(n => {
      const isFailed = n.status === 'failed';
      const hasError = !!n.error_message;
      const bodyClass = (isFailed || hasError) ? 'log-body open' : 'log-body';
      const sectionClass = isFailed ? 'log-section failed' : 'log-section';

      let bodyContent = '';
      if (n.error_message) bodyContent += `<div class="log-error-banner">${esc(n.error_message)}</div>`;
      bodyContent += (n.stdout ? `<pre>${esc(n.stdout)}</pre>` : '');
      bodyContent += (n.stderr ? `<pre class="stderr">${esc(n.stderr)}</pre>` : '');
      if (!n.stdout && !n.stderr && !n.error_message) {
        bodyContent = '<div style="font-size:11px;color:var(--text2);padding:8px">No output</div>';
      }

      return `<div class="${sectionClass}" data-node="${cssEsc(n.node_id)}">
        <div class="log-header" data-node-id="${esc(n.node_id)}">
          <span class="left"><b>${esc(n.node_id)}</b> <span style="color:var(--text2)">${n.node_type}</span></span>
          <span class="right">
            ${statusBadge(n.status)}
            ${n.attempt>0 ? `<span>retry ${n.attempt}</span>` : ''}
            ${n.exit_code!=null ? `<span>exit=${n.exit_code}</span>` : ''}
            ${n.started_at ? `<span>${fmtDuration(n.started_at, n.finished_at)}</span>` : ''}
          </span>
        </div>
        <div class="${bodyClass}" id="${domId(n.node_id)}">${bodyContent}</div>
      </div>`;
    }).join('');

  // Event delegation for log headers
  panel.querySelectorAll('.log-header[data-node-id]').forEach(hdr => {
    hdr.addEventListener('click', () => toggleLog(hdr, hdr.dataset.nodeId));
  });
}

function toggleLog(header, nodeId) {
  const body = document.getElementById(domId(nodeId));
  if (!body) return;
  if (!body.classList.contains('open')) {
    body.classList.add('open');
    fetchLogs(nodeId, body);
  } else {
    body.classList.remove('open');
  }
}

// Escape a string for use in CSS attribute selectors
function cssEsc(s) { return s.replace(/([\\"])/g, '\\$1'); }

// Sanitize node_id for use as a DOM id (replace / with -)
function domId(nodeId) { return 'log-' + nodeId.replace(/[^a-zA-Z0-9_-]/g, '_'); }

async function fetchLogs(nodeId, el) {
  if (!selectedRunId) return;
  try {
    const r = await fetch(`${API_WF}/${selectedRunId}/nodes/${encodeURIComponent(nodeId)}/logs`);
    const d = await r.json();
    let html = '';
    if (d.error_message) html += `<div class="log-error-banner">${esc(d.error_message)}</div>`;
    html += (d.stdout ? `<pre>${esc(d.stdout)}</pre>` : '');
    html += (d.stderr ? `<pre class="stderr">${esc(d.stderr)}</pre>` : '');
    if (!html) html = '<div style="font-size:11px;color:var(--text2);padding:8px">No output</div>';
    el.innerHTML = html;
  } catch(e) {
    el.innerHTML = '<div style="color:#cf222e;font-size:11px">Failed to load</div>';
  }
}

// ── Helpers ─────────────────────────────────────────────────────────
function fmtTime(t) { if(!t)return'-'; const d=new Date(t); return d.toLocaleTimeString(); }
function fmtDuration(start, end) {
  if (!start) return '';
  const s = new Date(start);
  const e = end ? new Date(end) : new Date();
  const ms = e - s;
  if (ms < 1000) return ms + 'ms';
  if (ms < 60000) return (ms / 1000).toFixed(1) + 's';
  const m = Math.floor(ms / 60000);
  const sec = Math.floor((ms % 60000) / 1000);
  return m + 'm ' + sec + 's';
}
function esc(s) { return (s||'').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }
function basename(p) { return (p||'').split('/').pop(); }

// ── API Docs Modal ──────────────────────────────────────────────────
function toggleApiDocs() {
  const modal = document.getElementById('api-modal');
  const open = modal.style.display === 'none';
  modal.style.display = open ? '' : 'none';
  if (open && !modal.dataset.loaded) {
    renderApiDocs();
    modal.dataset.loaded = '1';
  }
}

function curlBlock(code) {
  const id = 'curl-' + Math.random().toString(36).slice(2, 8);
  return `<div class="api-curl"><button class="curl-copy" data-curl="${esc(code)}" onclick="copyCurl(this)">Copy</button>${esc(code)}</div>`;
}

function copyCurl(btn) {
  const text = btn.dataset.curl;
  if (!text) return;
  navigator.clipboard.writeText(text).then(() => showToast('Copied to clipboard', 'success'));
}

function renderApiDocs() {
  const H = location.host;
  document.getElementById('api-docs-body').innerHTML = `
    <div class="api-section">
      <h3><span class="method method-post">POST</span> /api/v1/workflows</h3>
      <p>Submit a workflow YAML for execution. Returns the created run ID.</p>
      <table class="api-param-table">
        <tr><th>Field</th><th>Type</th><th>Description</th></tr>
        <tr><td><code>yaml</code></td><td>string</td><td>Required. Workflow YAML content.</td></tr>
        <tr><td><code>inputs</code></td><td>object</td><td>Optional. Key-value inputs for template variables.</td></tr>
      </table>
      ${curlBlock(`curl -X POST http://${H}/api/v1/workflows \\
  -H 'Content-Type: application/json' \\
  -d '{"yaml": "name: hello\\nversion: \\"1.0\\"\\nnodes:\\n  - id: greet\\n    type: shell\\n    run: echo hello"}'`)}
      <div class="api-response">// Response 201
{ "run_id": "019...", "status": "pending" }</div>
    </div>

    <div class="api-section">
      <h3><span class="method method-get">GET</span> /api/v1/workflows</h3>
      <p>List recent workflow runs (last 50).</p>
      ${curlBlock(`curl http://${H}/api/v1/workflows`)}
      <div class="api-response">// Response 200
{ "runs": [{ "id": "...", "workflow_name": "hello", "status": "success", ... }] }</div>
    </div>

    <div class="api-section">
      <h3><span class="method method-get">GET</span> /api/v1/workflows/:run_id</h3>
      <p>Get a single run with all node details, status, stdout, and stderr.</p>
      ${curlBlock(`curl http://${H}/api/v1/workflows/{run_id}`)}
      <div class="api-response">// Response 200
{ "id": "...", "status": "success", "nodes": [{ "node_id": "greet", "status": "success", "stdout": "hello\\n" }] }</div>
    </div>

    <div class="api-section">
      <h3><span class="method method-get">GET</span> /api/v1/workflows/:run_id/nodes/:node_id/logs</h3>
      <p>Get logs for a specific node in a run. Uses the node's business ID (not DB id).</p>
      ${curlBlock(`curl http://${H}/api/v1/workflows/{run_id}/nodes/greet/logs`)}
      <div class="api-response">// Response 200
{ "node_id": "greet", "status": "success", "stdout": "hello\\n", "stderr": null, "exit_code": 0 }</div>
    </div>

    <div class="api-section">
      <h3><span class="method method-get">GET</span> /api/v1/templates</h3>
      <p>List all workflow templates discovered from the watched directory.</p>
      ${curlBlock(`curl http://${H}/api/v1/templates`)}
      <div class="api-response">// Response 200
{ "templates": [{ "name": "ci-pipeline", "version": "1.0", "node_count": 4, "inputs": {...} }] }</div>
    </div>

    <div class="api-section">
      <h3><span class="method method-post">POST</span> /api/v1/templates/:name/run</h3>
      <p>Run a template by name. Optionally pass inputs for template variables.</p>
      <table class="api-param-table">
        <tr><th>Field</th><th>Type</th><th>Description</th></tr>
        <tr><td><code>inputs</code></td><td>object</td><td>Optional. Key-value inputs matching the template's declared inputs.</td></tr>
      </table>
      ${curlBlock(`curl -X POST http://${H}/api/v1/templates/ci-pipeline/run \\
  -H 'Content-Type: application/json' \\
  -d '{"inputs": {"tag": "v1.2.3", "env": "staging"}}'`)}
      ${curlBlock(`# Run without inputs
curl -X POST http://${H}/api/v1/templates/simple-ci/run`}
      <div class="api-response">// Response 201
{ "run_id": "019...", "status": "pending", "template": "ci-pipeline" }</div>
    </div>
  `;
}

// ── Init ────────────────────────────────────────────────────────────
loadTemplates();
loadRuns();
setInterval(loadTemplates, 15000);
setInterval(loadRuns, 5000);

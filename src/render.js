// ── Rendering functions ──

let _lastHeatmapData = null;
let _lastHeatmapHtml = '';

function renderHeatmap(data) {
  // Skip re-render if data hasn't changed
  if (data === _lastHeatmapData) return;
  _lastHeatmapData = data;

  const el = document.getElementById('heatmap');
  if (!data || data.length === 0) {
    el.innerHTML = '<div class="no-data">No activity data</div>';
    _lastHeatmapHtml = '';
    return;
  }

  const map = {};
  data.forEach(d => { map[d.date] = d.count; });

  const today = new Date();
  const year = today.getFullYear();
  const month = today.getMonth();

  const firstDay = new Date(year, month, 1);
  const startDay = firstDay.getDay();
  const startDate = new Date(firstDay);
  startDate.setDate(1 - startDay);

  const endDate = new Date(today);

  const weeks = [];
  const d = new Date(startDate);
  const allCounts = [];
  while (d <= endDate) {
    const week = [];
    for (let i = 0; i < 7; i++) {
      const ds = d.toISOString().slice(0, 10);
      const count = map[ds] || 0;
      const inMonth = d.getMonth() === month;
      week.push({ date: ds, count, inMonth });
      if (inMonth) allCounts.push(count);
      d.setDate(d.getDate() + 1);
    }
    weeks.push(week);
  }

  const maxCount = Math.max(1, ...allCounts);
  function level(count) {
    if (count === 0) return 0;
    const p = count / maxCount;
    if (p <= 0.25) return 1;
    if (p <= 0.50) return 2;
    if (p <= 0.75) return 3;
    return 4;
  }

  const monthLabel = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'][month];
  const dayNames = ['', 'M', '', 'W', '', 'F', ''];

  const html = `
    <div class="heatmap-month-label">${monthLabel}</div>
    <div class="heatmap-grid" style="display:grid;grid-template-columns:auto repeat(${weeks.length},1fr);gap:2px;align-items:center;">
      ${dayNames.map((dn, i) => `
        <span class="heatmap-day">${dn}</span>
        ${weeks.map(week => {
          const cell = week[i];
          return cell.inMonth
            ? `<div class="heatmap-cell l${level(cell.count)}" title="${cell.date}: ${fmt(cell.count)} tokens"></div>`
            : `<div class="heatmap-cell off-month" title="${cell.date}"></div>`;
        }).join('')}
      `).join('')}
    </div>
  `;

  if (html !== _lastHeatmapHtml) {
    el.innerHTML = html;
    _lastHeatmapHtml = html;
  }
}

let _lastTrendData = null;
let _lastTrendRange = null;
let _lastTrendHtml = '';

function renderTrend(data, days) {
  // Skip re-render if data and range haven't changed
  if (data === _lastTrendData && days === _lastTrendRange) return;
  _lastTrendData = data;
  _lastTrendRange = days;

  const el = document.getElementById('trend-chart');
  if (!data || data.length === 0) {
    el.innerHTML = '<div class="no-data">No trend data</div>';
    _lastTrendHtml = '';
    return;
  }

  const map = {};
  data.forEach(d => { map[d.date] = d.count; });

  const series = [];
  const today = new Date();
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(today);
    d.setDate(today.getDate() - i);
    const ds = d.toISOString().slice(0, 10);
    series.push({ date: ds, count: map[ds] || 0, day: d.getDate() });
  }

  const maxCount = Math.max(1, ...series.map(s => s.count));
  const total = series.reduce((a, s) => a + s.count, 0);
  const avg = Math.round(total / days);
  const gap = days >= 30 ? 2 : 3;

  const html = `
    <div class="trend-bars" style="gap:${gap}px">
      ${series.map(s => {
        const h = s.count === 0 ? 2 : Math.max(2, (s.count / maxCount) * 100);
        return `<div class="trend-bar-wrap" title="${s.date}: ${fmt(s.count)} tokens">
          <div class="trend-bar" style="height:${h}%"></div>
        </div>`;
      }).join('')}
    </div>
    <div class="trend-axis">
      <span>${series[0].date.slice(5)}</span>
      <span class="trend-avg">avg ${fmt(avg)}/d</span>
      <span>${series[series.length - 1].date.slice(5)}</span>
    </div>
  `;

  if (html !== _lastTrendHtml) {
    el.innerHTML = html;
    _lastTrendHtml = html;
  }
}

const expandedSessions = new Set();
const SESSION_LIMIT = 5;

function updateSessions(sessions) {
  // Prune stale session IDs
  if (sessions && sessions.length > 0) {
    const incoming = new Set(sessions.map(([id]) => id));
    for (const id of expandedSessions) {
      if (!incoming.has(id)) expandedSessions.delete(id);
    }
  }

  const el = document.getElementById('sessions-list');
  if (!sessions || sessions.length === 0) {
    el.innerHTML = '<div class="no-data">No sessions found</div>';
    return;
  }
  const total = sessions.length;
  const top = sessions.slice(0, SESSION_LIMIT);
  const moreCount = total - top.length;

  el.innerHTML = top.map(([id, s]) => {
    const expanded = expandedSessions.has(id);
    const shortId = id.length > 8 ? id.slice(0, 8) : id;
    return `
    <div class="session-item${expanded ? ' expanded' : ''}" data-sid="${id}">
      <div class="session-row">
        <span class="session-id" title="${id}">
          <span class="chevron">${expanded ? '▾' : '▸'}</span>
          ${shortId}
        </span>
        <div class="session-right">
          <span class="session-total">${fmt(s.total_tokens)}</span>
        </div>
      </div>
      ${expanded ? `
      <div class="session-detail">
        <span class="sd-label">Input</span><span class="sd-value">${fmt(s.input_tokens)}</span>
        <span class="sd-label">Output</span><span class="sd-value">${fmt(s.output_tokens)}</span>
        <span class="sd-label">Cache R</span><span class="sd-value">${fmt(s.cache_read_tokens)}</span>
        <span class="sd-label">Cache W</span><span class="sd-value">${fmt(s.cache_write_tokens)}</span>
        <span class="sd-label">Requests</span><span class="sd-value">${s.request_count ?? 0}</span>
        <span class="sd-label">Cost</span><span class="sd-value">${fmtCost(s.estimated_cost_usd)}</span>
        <span class="sd-label">Model</span><span class="sd-value sd-model">${s.model || '--'}</span>
      </div>` : ''}
    </div>`;
  }).join('') + (moreCount > 0
    ? `<div class="session-more">+${moreCount} more · total ${total} sessions</div>`
    : '');
}

function updateStats(stats) {
  const effectiveInput = stats.input_tokens + (stats.cache_read_tokens || 0) + (stats.cache_write_tokens || 0);
  document.getElementById('effective-input').textContent = fmt(effectiveInput);

  const cached = stats.cache_read_tokens || 0;
  const pct = stats.cache_hit_pct || 0;
  if (cached > 0) {
    document.getElementById('input-detail').textContent = fmt(cached) + ' cached (' + fmtPct(pct) + ' hit)';
  } else {
    document.getElementById('input-detail').textContent = '';
  }

  const usagePct = stats.context_usage_pct || 0;
  const ctxWindow = stats.context_window || 0;
  const fill = document.getElementById('context-fill');
  fill.style.width = Math.min(usagePct, 100) + '%';
  fill.classList.toggle('warn', usagePct > 50);
  fill.classList.toggle('danger', usagePct > 80);
  document.getElementById('context-pct').textContent =
    ctxWindow > 0 ? Math.round(usagePct) + '%' : '--';

  document.getElementById('output-tokens').textContent = fmt(stats.output_tokens);
  document.getElementById('request-count').textContent = stats.request_count ?? '--';
  document.getElementById('total-tokens').textContent = fmt(stats.total_tokens);
  document.getElementById('model-name').textContent = stats.model || '--';
  document.getElementById('model-name').style.color = '';

  const dot = document.querySelector('.dot');
  dot.classList.toggle('active', stats.total_tokens > 0);
  dot.classList.toggle('inactive', stats.total_tokens === 0);
}

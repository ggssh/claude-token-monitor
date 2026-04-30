// ── IPC ──
const T = window.__TAURI__;
const invoke = T?.invoke || T?.core?.invoke || T?.tauri?.invoke;

const statusEl = document.getElementById('model-name');
function showError(msg) {
  statusEl.textContent = msg;
  statusEl.style.color = '#f85149';
}
if (!invoke) {
  showError('IPC not available');
  throw new Error('Tauri IPC not available');
}

// ── State ──
let lastSessions = null;
let lastActivity = null;
let currentTrendRange = 14;

// ── Refresh ──
async function refreshStats() {
  try {
    const stats = await invoke('get_stats');
    if (stats) updateStats(stats);
  } catch (e) {
    showError('Err: ' + (e?.message || e));
  }
}

async function refreshAll() {
  try {
    const [sessions, activity] = await Promise.all([
      invoke('get_all_sessions'),
      invoke('get_daily_activity'),
    ]);
    if (sessions) {
      lastSessions = sessions;
      updateSessions(sessions);
    }
    if (activity) {
      lastActivity = activity;
      renderHeatmap(activity);
      renderTrend(activity, currentTrendRange);
    }
  } catch (e) {
    console.error('refresh error:', e);
  }
}

async function refresh() {
  await Promise.all([refreshStats(), refreshAll()]);
}

// ── Events ──
document.getElementById('sessions-list').addEventListener('click', (e) => {
  const item = e.target.closest('.session-item');
  if (!item) return;
  const sid = item.dataset.sid;
  if (expandedSessions.has(sid)) {
    expandedSessions.delete(sid);
  } else {
    expandedSessions.add(sid);
  }
  if (lastSessions) updateSessions(lastSessions);
});

let syncing = false;
const syncBtn = document.getElementById('sync-btn');
syncBtn.addEventListener('click', () => {
  if (syncing) return;
  syncing = true;
  syncBtn.classList.add('spinning');
  refresh().finally(() => {
    setTimeout(() => {
      syncBtn.classList.remove('spinning');
      syncing = false;
    }, 600);
  });
});

document.getElementById('trend-range-toggle').addEventListener('click', (e) => {
  const btn = e.target.closest('.range-btn');
  if (!btn) return;
  const range = parseInt(btn.dataset.range, 10);
  if (range === currentTrendRange) return;
  currentTrendRange = range;
  document.querySelectorAll('.range-btn').forEach(b => b.classList.remove('active'));
  btn.classList.add('active');
  if (lastActivity) renderTrend(lastActivity, currentTrendRange);
});

// ── Theme sync ──
getTheme().then((theme) => {
  applyTheme(theme);
  invoke('update_menu_theme', { theme });
});

const listen = T?.event?.listen || T?.core?.listen;
if (listen) {
  listen('theme-select', (e) => {
    const theme = e.payload || '';
    saveTheme(theme);
    invoke('update_menu_theme', { theme });
  });
}

// ── Visibility-based polling ──
let pollTimer = null;

function startPolling() {
  if (pollTimer) return;
  refresh();
  pollTimer = setInterval(refresh, 15000);
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

document.addEventListener('visibilitychange', () => {
  document.hidden ? stopPolling() : startPolling();
});

startPolling();

// ── Theme management ──
// Theme is persisted on the Rust side via config/config.json.
// The frontend reads it through IPC and applies the data-theme attribute.

const _themeInvoke = (() => {
  const T = window.__TAURI__;
  return T?.invoke || T?.core?.invoke || T?.tauri?.invoke;
})();

async function getTheme() {
  try {
    const cfg = await _themeInvoke('get_config');
    return cfg?.theme || '';
  } catch {
    return '';
  }
}

function applyTheme(theme) {
  document.documentElement.setAttribute('data-theme', theme);
}

async function saveTheme(theme) {
  try {
    await _themeInvoke('save_theme', { theme });
    applyTheme(theme);
  } catch (e) {
    console.error('save theme error:', e);
  }
}

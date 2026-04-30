use crate::config::{self, AppConfig};
use crate::token_monitor::{self, current_generation, daily_activity, today_total, DailyActivity, TokenStats};
use crate::AppState;

#[tauri::command]
pub fn get_stats(state: tauri::State<AppState>) -> TokenStats {
    state.stats.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub fn get_today_stats() -> TokenStats {
    today_total()
}

#[tauri::command]
pub fn get_all_sessions(state: tauri::State<AppState>) -> Vec<(String, TokenStats)> {
    let gen = current_generation();
    let mut cache = state.data_cache.lock().unwrap_or_else(|e| e.into_inner());
    if cache.sessions.is_none() || cache.generation != gen {
        let sessions = token_monitor::find_all_sessions();
        let mut v: Vec<_> = sessions.into_iter().collect();
        v.sort_by(|a, b| b.1.total_tokens.cmp(&a.1.total_tokens));
        cache.sessions = Some(v.clone());
        cache.generation = gen;
        v
    } else {
        cache.sessions.clone().unwrap()
    }
}

#[tauri::command]
pub fn get_daily_activity(state: tauri::State<AppState>) -> Vec<DailyActivity> {
    let gen = current_generation();
    let mut cache = state.data_cache.lock().unwrap_or_else(|e| e.into_inner());
    if cache.activity.is_none() || cache.generation != gen {
        let activity = daily_activity(91);
        cache.activity = Some(activity.clone());
        cache.generation = gen;
        activity
    } else {
        cache.activity.clone().unwrap()
    }
}

#[tauri::command]
pub fn update_menu_theme(state: tauri::State<AppState>, theme: &str) {
    let (def, glass) = match theme {
        "" => ("✓ Default", "Liquid Glass"),
        _ => ("Default", "✓ Liquid Glass"),
    };
    let _ = state.theme_default.set_text(def);
    let _ = state.theme_glass.set_text(glass);
}

#[tauri::command]
pub fn get_config(state: tauri::State<AppState>) -> AppConfig {
    config::load_config(&state.config_path)
}

#[tauri::command]
pub fn save_theme(state: tauri::State<AppState>, theme: String) {
    let mut cfg = config::load_config(&state.config_path);
    cfg.theme = theme;
    config::save_config(&state.config_path, &cfg);
    let (def, glass) = match cfg.theme.as_str() {
        "" => ("✓ Default", "Liquid Glass"),
        _ => ("Default", "✓ Liquid Glass"),
    };
    let _ = state.theme_default.set_text(def);
    let _ = state.theme_glass.set_text(glass);
}

mod commands;
mod config;
mod token_monitor;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{MenuBuilder, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tokio::sync::broadcast;
use token_monitor::{
    find_active_session_file, find_claude_projects_dir, parse_token_stats, spawn_file_watcher,
    DataCache, TokenStats,
};

pub(crate) struct AppState {
    pub stats: Arc<Mutex<TokenStats>>,
    #[allow(dead_code)]
    pub shutdown_tx: broadcast::Sender<()>,
    pub theme_default: MenuItem<tauri::Wry>,
    pub theme_glass: MenuItem<tauri::Wry>,
    pub config_path: PathBuf,
    pub data_cache: Mutex<DataCache>,
}

/// Last known position of the tray icon (updated on each click).
static TRAY_POS: std::sync::Mutex<Option<(i32, i32)>> = std::sync::Mutex::new(None);

fn update_tray<R: Runtime>(app: &tauri::AppHandle<R>, stats: &TokenStats) {
    if let Some(tray) = app.tray_by_id("token-tray") {
        let _ = tray.set_title(None::<&str>);

        let pct = stats.context_usage_pct;
        let icon = if pct >= 60.0 {
            tauri::include_image!("./icons/trayIconHighTemplate@2x.png")
        } else if pct >= 25.0 {
            tauri::include_image!("./icons/trayIconMediumTemplate@2x.png")
        } else {
            tauri::include_image!("./icons/trayIconTemplate@2x.png")
        };
        let _ = tray.set_icon(Some(icon));
    }
}

fn toggle_panel<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("panel") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            if let Some((tx, _)) = *TRAY_POS.lock().unwrap_or_else(|e| e.into_inner()) {
                let panel_w = 480.0;
                let x = (tx as f64 - panel_w / 2.0).max(0.0) as i32;
                let y = 28;
                let _ = window.set_position(PhysicalPosition::new(x, y));
            }
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn panel_url() -> WebviewUrl {
    #[cfg(dev)]
    {
        WebviewUrl::External(
            url::Url::parse("http://127.0.0.1:1420/index.html").expect("valid dev URL"),
        )
    }
    #[cfg(not(dev))]
    {
        WebviewUrl::App("index.html".into())
    }
}

fn build_tray(
    app: &tauri::App,
    theme_default: &MenuItem<tauri::Wry>,
    theme_glass: &MenuItem<tauri::Wry>,
) -> tauri::Result<()> {
    let toggle_item = MenuItem::with_id(app, "toggle_panel", "Toggle Panel", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let themes_menu = Submenu::with_id_and_items(
        app,
        "themes",
        "Themes",
        true,
        &[theme_default, theme_glass],
    )?;

    let menu = MenuBuilder::new(app)
        .item(&toggle_item)
        .item(&themes_menu)
        .item(&separator)
        .item(&quit_item)
        .build()?;

    let tray_icon = tauri::include_image!("./icons/trayIconTemplate@2x.png");

    let _tray = Box::leak(Box::new(
        TrayIconBuilder::with_id("token-tray")
            .icon(tray_icon)
            .icon_as_template(true)
            .menu(&menu)
            .tooltip("Claude Token Monitor")
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray_icon, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    position,
                    ..
                } = event
                {
                    *TRAY_POS.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some((position.x as i32, position.y as i32));
                    toggle_panel(tray_icon.app_handle());
                }
            })
            .on_menu_event(|app, event| {
                let id = event.id().as_ref();
                if id == "toggle_panel" {
                    toggle_panel(app);
                } else if id == "quit" {
                    app.exit(0);
                } else if let Some(theme) = id.strip_prefix("theme:") {
                    let _ = app.emit("theme-select", theme);
                }
            })
            .build(app)?,
    ));

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(1);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let session_file = find_active_session_file();
            let projects_dir = find_claude_projects_dir();

            let default_stats = session_file
                .as_ref()
                .map(|p| parse_token_stats(p))
                .unwrap_or_default();
            let stats = Arc::new(Mutex::new(default_stats));

            let stats_for_title = stats.clone();
            let tx_for_periodic = shutdown_tx.clone();

            let cfg_path = config::config_path(app.handle());
            if let Some(dir) = cfg_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }

            let saved_theme = config::load_config(&cfg_path).theme;
            let theme_default = MenuItem::with_id(app, "theme:", "Default", true, None::<&str>)?;
            let theme_glass =
                MenuItem::with_id(app, "theme:liquid-glass", "Liquid Glass", true, None::<&str>)?;

            app.manage(AppState {
                stats,
                shutdown_tx,
                theme_default: theme_default.clone(),
                theme_glass: theme_glass.clone(),
                config_path: cfg_path,
                data_cache: Mutex::new(DataCache::new()),
            });

            if let Some(dir) = projects_dir {
                spawn_file_watcher(dir, session_file, stats_for_title.clone(), shutdown_rx);
            }

            build_tray(app, &theme_default, &theme_glass)?;

            // Apply saved theme checkmark
            let (def_text, glass_text) = match saved_theme.as_str() {
                "" => ("✓ Default", "Liquid Glass"),
                _ => ("Default", "✓ Liquid Glass"),
            };
            let _ = theme_default.set_text(def_text);
            let _ = theme_glass.set_text(glass_text);

            let current = stats_for_title.lock().unwrap_or_else(|e| e.into_inner()).clone();
            update_tray(app.handle(), &current);

            // Panel window
            let panel = WebviewWindowBuilder::new(app, "panel", panel_url())
                .title("Token Monitor")
                .inner_size(480.0, 500.0)
                .resizable(false)
                .minimizable(false)
                .maximizable(false)
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .build()?;

            #[cfg(target_os = "macos")]
            {
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
                let _ =
                    apply_vibrancy(&panel, NSVisualEffectMaterial::HudWindow, None, Some(12.0));
            }

            let panel_clone = panel.clone();
            panel.on_window_event(move |event| match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = panel_clone.hide();
                }
                WindowEvent::Focused(false) => {
                    let _ = panel_clone.hide();
                }
                _ => {}
            });

            // Periodic tray title update
            let app_handle = app.handle().clone();
            let stats_clone = stats_for_title;
            let mut periodic_rx = tx_for_periodic.subscribe();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(10));
                if periodic_rx.try_recv().is_ok() {
                    return;
                }
                let s = stats_clone.lock().unwrap_or_else(|e| e.into_inner()).clone();
                app_handle.emit("stats-update", &s).unwrap_or_default();
                update_tray(&app_handle, &s);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_stats,
            commands::get_today_stats,
            commands::get_all_sessions,
            commands::get_daily_activity,
            commands::update_menu_theme,
            commands::get_config,
            commands::save_theme,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

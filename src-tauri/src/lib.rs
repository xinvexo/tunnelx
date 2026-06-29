use crate::state::AppState;
use crate::store::load_app_data;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_prevent_default::{Flags, KeyboardShortcut, ModifierKey};

mod commands;
mod diag;
mod domain;
mod error;
mod paths;
mod providers;
mod services;
mod state;
mod store;
mod sync_ext;

const DISCONNECTED_TRAY_ICON: &[u8] = include_bytes!("../icons/tray-disconnected.png");
const CONNECTED_TRAY_ICON: &[u8] = include_bytes!("../icons/tray-connected.png");

fn connection_tray_icon(connected: bool) -> tauri::Result<Image<'static>> {
    Image::from_bytes(if connected {
        CONNECTED_TRAY_ICON
    } else {
        DISCONNECTED_TRAY_ICON
    })
}

pub(crate) fn refresh_connection_icon<R: Runtime>(app: &AppHandle<R>) {
    let count = active_connection_count(app);

    if let Some(tray) = app.tray_by_id("main-tray") {
        if let Ok(tray_icon) = connection_tray_icon(count > 0) {
            let _ = tray.set_icon(Some(tray_icon));
        }
        // 连接数变化时重建菜单，让顶部「N 个连接运行中」状态行保持最新。
        if let Ok(menu) = tray_menu(app, current_tray_locale().as_deref()) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn cleanup_before_exit_once(app: &AppHandle) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let state = app.state::<AppState>();
    providers::registry::stop_active_tunnels(app, state.inner());
    refresh_connection_icon(app);
}

fn active_connection_count<R: Runtime>(app: &AppHandle<R>) -> usize {
    let state = app.state::<AppState>();
    providers::registry::active_tunnel_count(&state)
}

/// 托盘菜单由后端线程刷新，需在进程内缓存前端同步过来的 locale。
fn tray_locale_slot() -> &'static Mutex<Option<String>> {
    static SLOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn current_tray_locale() -> Option<String> {
    tray_locale_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn tray_status_label(count: usize, locale: Option<&str>) -> String {
    let zh = locale.map(is_zh_locale).unwrap_or_else(system_prefers_zh);
    if zh {
        format!("{count} 个连接运行中")
    } else if count == 1 {
        "1 connection running".to_string()
    } else {
        format!("{count} connections running")
    }
}

fn set_activation_regular<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(ActivationPolicy::Regular);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}

fn set_activation_accessory<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(ActivationPolicy::Accessory);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}

fn show_main(app: &AppHandle) {
    set_activation_regular(app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_skip_taskbar(false);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    } else {
        // 轻量模式下窗口已被销毁，点击托盘时重新创建并渲染。
        let _ = create_main_window(app);
    }
}

/// 轻量模式关闭窗口后会销毁 webview；再次打开托盘时按 tauri.conf 的窗口参数重建。
fn create_main_window(app: &AppHandle) -> tauri::Result<()> {
    #[allow(unused_mut)]
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
            .title("TunnelX")
            .inner_size(1100.0, 720.0)
            .min_inner_size(1024.0, 640.0)
            .visible(false);

    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.decorations(false);
    }

    let window = builder.build()?;
    attach_close_behavior(app, &window);
    refresh_connection_icon(app);
    set_activation_regular(app);
    let _ = window.set_skip_taskbar(false);
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

fn system_prefers_zh() -> bool {
    std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .map(|value| value.to_ascii_lowercase().starts_with("zh"))
        .unwrap_or(false)
}

fn is_zh_locale(locale: &str) -> bool {
    locale.to_ascii_lowercase().starts_with("zh")
}

fn tray_menu_labels(locale: Option<&str>) -> (&'static str, &'static str, &'static str) {
    if locale.map(is_zh_locale).unwrap_or_else(system_prefers_zh) {
        ("显示主界面", "全部断开", "退出")
    } else {
        ("Show Window", "Disconnect All", "Quit")
    }
}

fn tray_menu<R: Runtime>(app: &AppHandle<R>, locale: Option<&str>) -> tauri::Result<Menu<R>> {
    let (show_label, disconnect_label, quit_label) = tray_menu_labels(locale);
    let show_i = MenuItem::with_id(app, "show", show_label, true, None::<&str>)?;
    let disconnect_i =
        MenuItem::with_id(app, "disconnect_all", disconnect_label, true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    let count = active_connection_count(app);
    if count > 0 {
        let status_i = MenuItem::with_id(
            app,
            "status",
            tray_status_label(count, locale),
            false,
            None::<&str>,
        )?;
        let sep = PredefinedMenuItem::separator(app)?;
        Menu::with_items(app, &[&status_i, &sep, &show_i, &disconnect_i, &quit_i])
    } else {
        Menu::with_items(app, &[&show_i, &quit_i])
    }
}

pub(crate) fn set_tray_locale<R: Runtime>(app: &AppHandle<R>, locale: &str) -> tauri::Result<()> {
    if let Ok(mut slot) = tray_locale_slot().lock() {
        *slot = Some(locale.to_string());
    }
    refresh_connection_icon(app);
    Ok(())
}

/// 普通模式关闭时隐藏到托盘；轻量模式关闭时销毁 webview，应用继续常驻。
fn attach_close_behavior(app: &AppHandle, window: &WebviewWindow) {
    let handle = app.clone();
    let win = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            let lightweight = handle
                .state::<AppState>()
                .config
                .settings()
                .lightweight_mode;
            if !lightweight {
                api.prevent_close();
                let _ = win.set_skip_taskbar(true);
                let _ = win.hide();
            }
            set_activation_accessory(&handle);
        }
    });
}

/// 建立托盘图标：左键/右键只弹出菜单，显示窗口通过菜单项触发。
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let icon = connection_tray_icon(false)?;
    let menu = tray_menu(app.handle(), None)?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("TunnelX")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => {
                cleanup_before_exit_once(app);
                app.exit(0);
            }
            "disconnect_all" => {
                let state = app.state::<AppState>();
                providers::registry::stop_active_tunnels(app, state.inner());
                refresh_connection_icon(app);
            }
            "show" => show_main(app),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn prevent_default_plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    use ModifierKey::{AltKey, CtrlKey, MetaKey, ShiftKey};

    tauri_plugin_prevent_default::Builder::new()
        .with_flags(
            Flags::CONTEXT_MENU
                | Flags::DEV_TOOLS
                | Flags::DOWNLOADS
                | Flags::RELOAD
                | Flags::SOURCE
                | Flags::OPEN
                | Flags::PRINT,
        )
        .shortcut(KeyboardShortcut::new("F11"))
        .shortcut(KeyboardShortcut::new("F12"))
        .shortcut(KeyboardShortcut::with_modifiers("c", &[CtrlKey, ShiftKey]))
        .shortcut(KeyboardShortcut::with_modifiers("j", &[CtrlKey, ShiftKey]))
        .shortcut(KeyboardShortcut::with_modifiers("c", &[MetaKey, AltKey]))
        .shortcut(KeyboardShortcut::with_modifiers("i", &[MetaKey, AltKey]))
        .shortcut(KeyboardShortcut::with_modifiers("j", &[MetaKey, AltKey]))
        .shortcut(KeyboardShortcut::with_meta("l"))
        .shortcut(KeyboardShortcut::with_meta("o"))
        .shortcut(KeyboardShortcut::with_meta("p"))
        .shortcut(KeyboardShortcut::with_meta("r"))
        .shortcut(KeyboardShortcut::with_meta("s"))
        .shortcut(KeyboardShortcut::with_meta("u"))
        .shortcut(KeyboardShortcut::with_modifiers("r", &[MetaKey, ShiftKey]))
        .shortcut(KeyboardShortcut::with_modifiers("j", &[MetaKey, ShiftKey]))
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        // Single-instance must be registered first; a second launch only focuses the main window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .plugin(prevent_default_plugin())
        .plugin(store::database_guard_plugin())
        .plugin(store::sql_plugin())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            // Corrupt local data is backed up and switched to read-only mode; keep the UI usable.
            if let Err(e) = load_app_data(&handle, &app.state::<AppState>()) {
                diag::warn("app", format!("data store load failed; starting in degraded mode: {e}"));
                let (title, body) = if system_prefers_zh() {
                    (
                        "TunnelX 数据保护",
                        format!(
                            "本地数据存储加载失败，数据库已备份，应用进入只读保护模式。\n本次启动使用空数据，您的改动不会被保存，请检查备份后重启应用。\n\n详情：{e}"
                        ),
                    )
                } else {
                    (
                        "TunnelX data protection",
                        format!(
                            "Failed to load the local data store. The database was backed up and the app is now in read-only protection mode.\nThis session starts with empty data and changes will NOT be saved — check the backup, then restart.\n\nDetails: {e}"
                        ),
                    )
                };
                // setup runs before the event loop, so use the non-blocking dialog API.
                use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                handle
                    .dialog()
                    .message(body)
                    .kind(MessageDialogKind::Warning)
                    .title(title)
                    .show(|_| {});
            }
            let recovered_watchdog =
                match services::process_watchdog::recover_if_available(
                    &handle,
                    app.state::<AppState>().inner(),
                ) {
                    Ok(recovered) => recovered,
                    Err(error) => {
                        diag::warn("app", format!("watchdog recovery failed: {error}"));
                        false
                    }
                };
            if !recovered_watchdog {
                providers::registry::cleanup_on_start(&handle, app.state::<AppState>().inner());
            }

            setup_tray(app)?;

            let settings = app.state::<AppState>().config.settings();

            if let Some(win) = app.get_webview_window("main") {
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = win.set_decorations(false);
                }

                if !settings.silent_start {
                    set_activation_regular(&handle);
                    let _ = win.show();
                    let _ = win.set_focus();
                } else {
                    let _ = win.set_skip_taskbar(true);
                    set_activation_accessory(&handle);
                }

                attach_close_behavior(&handle, &win);
            }

            if !recovered_watchdog {
                providers::registry::auto_connect(&handle, app.state::<AppState>().inner());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // profile
            providers::frp::commands::list_profiles,
            providers::frp::commands::get_profile,
            providers::frp::commands::update_profile,
            providers::frp::commands::reorder_profiles,
            // proxy
            providers::frp::commands::update_proxies,
            // runtime
            providers::frp::commands::frpc_needs_restart,
            providers::frp::commands::frpc_proxy_status,
            providers::frp::commands::proxy_traffic_stats,
            // version
            providers::frp::commands::list_frpc_versions,
            providers::frp::commands::check_frpc_update,
            providers::frp::commands::install_frpc_version,
            providers::frp::commands::activate_frpc_version,
            providers::frp::commands::remove_frpc_version,
            providers::frp::commands::uninstall_active_frpc_version,
            providers::frp::commands::active_frpc_version,
            providers::frp::commands::get_frpc_mirror,
            providers::frp::commands::set_frpc_mirror,
            // update
            commands::app_version,
            commands::check_update,
            commands::install_update,
            // settings
            commands::get_settings,
            commands::set_settings,
            commands::set_tray_locale,
            // providers
            commands::list_providers,
            commands::connection_order,
            commands::reorder_connections,
            commands::provider_status,
            commands::provider_login,
            commands::provider_tunnels,
            commands::provider_create_tunnel,
            commands::provider_update_tunnel,
            commands::provider_duplicate_tunnel,
            commands::provider_delete_tunnel,
            commands::provider_start_tunnel,
            commands::provider_stop_tunnel,
            commands::provider_tunnel_status,
            commands::provider_tunnel_logs,
            commands::watchdog_logs,
            commands::provider_metrics,
            commands::runtime_metrics,
            commands::runtime_traffic_summary,
            // cloudflare
            providers::cloudflare::commands::cloudflare_data,
            providers::cloudflare::commands::cloudflare_login_connection,
            providers::cloudflare::commands::cloudflare_install_cloudflared,
            providers::cloudflare::commands::cloudflare_uninstall_cloudflared,
            providers::cloudflare::commands::cloudflare_check_cloudflared_update,
            providers::cloudflare::commands::cloudflare_verify_token,
            providers::cloudflare::commands::cloudflare_list_accounts,
            providers::cloudflare::commands::cloudflare_list_zones,
            providers::cloudflare::commands::cloudflare_list_remote_tunnels,
            // ngrok
            providers::ngrok::commands::ngrok_install_runtime,
            providers::ngrok::commands::ngrok_uninstall_runtime,
            providers::ngrok::commands::ngrok_check_runtime_update,
            providers::ngrok::commands::ngrok_authenticate_tunnel,
            // cpolar
            providers::cpolar::commands::cpolar_install_runtime,
            providers::cpolar::commands::cpolar_uninstall_runtime,
            providers::cpolar::commands::cpolar_check_runtime_update,
            providers::cpolar::commands::cpolar_authenticate_tunnel,
            // Pinggy
            providers::pinggy::commands::pinggy_install_runtime,
            providers::pinggy::commands::pinggy_uninstall_runtime,
            providers::pinggy::commands::pinggy_check_runtime_update,
            providers::pinggy::commands::pinggy_authenticate_tunnel,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            #[cfg(not(target_os = "macos"))]
            let _ = app;
            // Closing the main window should keep the tray app alive; only explicit Quit exits.
            match event {
                tauri::RunEvent::ExitRequested {
                    code: None, api, ..
                } => {
                    api.prevent_exit();
                }
                tauri::RunEvent::ExitRequested { code: Some(_), .. } => {
                    cleanup_before_exit_once(app);
                }
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => show_main(app),
                _ => {}
            }
        });
}

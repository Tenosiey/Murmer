//! Entry point for the Tauri application.
//!
//! Sets up the system tray and window event handlers before running the app.

/// WebKitGTK's DMA-BUF renderer is known to glitch or fall back to software
/// rendering on the proprietary NVIDIA driver (tauri-apps/tauri#9304).
/// Disable it there unless the user already chose a setting themselves.
#[cfg(target_os = "linux")]
fn apply_webkitgtk_workarounds() {
    let nvidia = std::path::Path::new("/proc/driver/nvidia/version").exists();
    if nvidia && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    // The tray icon goes through libayatana-appindicator, which logs a
    // "deprecated, use libayatana-appindicator-glib" warning on every start.
    // Switching backends is up to tauri/tao upstream, so drop that domain's
    // warnings instead of spamming stderr.
    glib::log_set_handler(
        Some("libayatana-appindicator"),
        glib::LogLevels::LEVEL_WARNING,
        false,
        false,
        |_, _, _| {},
    );
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    use tauri::{
        menu::{MenuBuilder, MenuItemBuilder},
        tray::{TrayIconBuilder, TrayIconEvent},
        Manager,
    };
    use tauri_plugin_window_state::Builder as WindowStateBuilder;
    use tracing_subscriber::EnvFilter;

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("murmer_client=info,tauri=info")),
        )
        .with_target(false)
        .compact()
        .try_init();

    #[cfg(target_os = "linux")]
    apply_webkitgtk_workarounds();

    tauri::Builder::default()
        .plugin(WindowStateBuilder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // create tray menu
            let open = MenuItemBuilder::with_id("open", "Open").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Close").build(app)?;
            let tray_menu = MenuBuilder::new(app).item(&open).item(&quit).build()?;
            TrayIconBuilder::new().menu(&tray_menu).build(app)?;
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|app, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .run(tauri::generate_context!())?;

    Ok(())
}

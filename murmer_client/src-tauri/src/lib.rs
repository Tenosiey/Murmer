//! Entry point for the Tauri application.
//!
//! Sets up the system tray and window event handlers before running the app.

/// Id the tray is registered under so `set_tray_theme` can look it back up.
const TRAY_ID: &str = "main";

/// The two logo variants, baked into the binary so the tray can switch between
/// them without touching the filesystem at runtime.
const TRAY_ICON_DARK: &[u8] = include_bytes!("../icons/tray-dark.png");
const TRAY_ICON_LIGHT: &[u8] = include_bytes!("../icons/tray-light.png");

fn tray_icon(theme: &str) -> tauri::Result<tauri::image::Image<'static>> {
    let bytes = if theme == "light" {
        TRAY_ICON_LIGHT
    } else {
        TRAY_ICON_DARK
    };
    tauri::image::Image::from_bytes(bytes)
}

/// Switches the tray icon to the light or dark logo. Called by the client
/// whenever the theme store changes (see `src/lib/stores/theme.ts`); any value
/// other than "light" falls back to the dark logo.
#[tauri::command]
fn set_tray_theme(app: tauri::AppHandle, theme: String) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "tray icon not found".to_string())?;
    let icon = tray_icon(&theme).map_err(|e| e.to_string())?;
    tray.set_icon(Some(icon)).map_err(|e| e.to_string())
}

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![set_tray_theme])
        .setup(|app| {
            // create tray menu
            let open = MenuItemBuilder::with_id("open", "Open").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Close").build(app)?;
            let tray_menu = MenuBuilder::new(app).item(&open).item(&quit).build()?;
            // The tray icon is built here instead of being declared via
            // `app.trayIcon` in tauri.conf.json: that config makes Tauri create
            // its own menu-less tray icon on top of this one, leaving two
            // entries in the notification area.
            //
            // Starts on the dark logo to match the theme store's default; the
            // client calls `set_tray_theme` on startup to correct it when the
            // user is on the light theme.
            TrayIconBuilder::with_id(TRAY_ID)
                .menu(&tray_menu)
                .tooltip("Murmer")
                .icon(tray_icon("dark")?)
                .build(app)?;
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

#[cfg(test)]
mod tests {
    use super::tray_icon;
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::Shortcut;

    /// Both logo variants are baked in with `include_bytes!` and only decoded
    /// when the theme changes, so a truncated file or a missing "image-png"
    /// feature would otherwise stay hidden until a user toggles the theme.
    #[test]
    fn tray_icons_decode_for_both_themes() {
        for theme in ["dark", "light"] {
            let icon = tray_icon(theme).unwrap_or_else(|e| panic!("{theme} tray icon: {e}"));
            assert_eq!(
                (icon.width(), icon.height()),
                (64, 64),
                "{theme} tray icon has an unexpected size"
            );
        }
    }

    /// The client only ever sends "dark"/"light", but the command takes a
    /// free-form string over IPC — anything else must degrade to the dark
    /// logo instead of leaving the tray blank.
    #[test]
    fn unknown_theme_falls_back_to_the_dark_icon() {
        let fallback = tray_icon("wat").expect("fallback icon");
        let dark = tray_icon("dark").expect("dark icon");
        assert_eq!(fallback.rgba(), dark.rgba());
    }

    /// The web client registers its hotkey combos (see
    /// `src/lib/stores/hotkeys.ts`) verbatim as global shortcuts, so the
    /// combo format must stay parseable by the global-shortcut plugin.
    #[test]
    fn client_hotkey_combos_parse_as_shortcuts() {
        // "Super+…" is what the client's comboToAccelerator produces for
        // "Meta+…" combos; the plugin does not accept the token "Meta".
        for combo in [
            "Ctrl+Shift+M",
            "Ctrl+Shift+O",
            "Ctrl+Shift+V",
            "Ctrl+F",
            "F1",
            "Ctrl+Alt+K",
            "Super+Enter",
            "Alt+Space",
            "Ctrl+ArrowUp",
        ] {
            assert!(
                Shortcut::from_str(combo).is_ok(),
                "combo '{combo}' does not parse as a global shortcut"
            );
        }
    }
}

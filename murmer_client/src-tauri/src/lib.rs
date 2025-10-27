//! Entry point for the Tauri application.
//!
//! Sets up the system tray and window event handlers before running the app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    use tauri::{
        menu::{MenuBuilder, MenuItemBuilder},
        tray::{TrayIconBuilder, TrayIconEvent},
        Manager,
    };
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
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
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let window_clone = window.clone();
                api.prevent_close();
                window
                    .app_handle()
                    .dialog()
                    .message("Do you want to minimize or close the client?")
                    .title("Quit?")
                    .buttons(MessageDialogButtons::OkCancelCustom(
                        "Minimize".into(),
                        "Close".into(),
                    ))
                    .show(move |minimize| {
                        if minimize {
                            let _ = window_clone.hide();
                        } else {
                            window_clone.app_handle().exit(0);
                        }
                    });
            }
        })
        .run(tauri::generate_context!())?;

    Ok(())
}

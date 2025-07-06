// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::{
        menu::{MenuBuilder, MenuItemBuilder},
        tray::{TrayIconBuilder, TrayIconEvent},
        Manager,
    };
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // create tray menu
            let open = MenuItemBuilder::with_id("open", "Öffnen").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Schließen").build(app)?;
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
                    .message("Möchtest du den Client minimieren oder schließen?")
                    .title("Beenden?")
                    .buttons(MessageDialogButtons::OkCancelCustom(
                        "Minimieren".into(),
                        "Schließen".into(),
                    ))
                    .show(move |minimize| {
                        if minimize {
                            let _ = window_clone.hide();
                        } else {
                            std::process::exit(0);
                        }
                    });
            }
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

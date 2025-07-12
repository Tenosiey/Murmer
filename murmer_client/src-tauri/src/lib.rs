// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use bincode;
use dirs;
use ed25519_dalek::{Signer, SigningKey};
use once_cell::sync::OnceCell;
use rand::rngs::OsRng;
use std::fs;
use std::path::PathBuf;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

static SIGNING_KEY: OnceCell<SigningKey> = OnceCell::new();

fn keypair_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("murmer")
        .join("keypair.bin")
}

fn init_keypair() -> SigningKey {
    if let Some(k) = SIGNING_KEY.get() {
        return k.clone();
    }
    let path = keypair_path();
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(kp) = bincode::deserialize::<SigningKey>(&bytes) {
            SIGNING_KEY.set(kp.clone()).ok();
            return kp;
        }
    }
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mut csprng = OsRng;
    let kp = SigningKey::generate(&mut csprng);
    if let Ok(data) = bincode::serialize(&kp) {
        let _ = fs::write(&path, data);
    }
    SIGNING_KEY.set(kp.clone()).ok();
    kp
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_public_key() -> Result<String, String> {
    let kp = init_keypair();
    Ok(hex::encode(kp.verifying_key().to_bytes()))
}

#[tauri::command]
fn sign_data(data: String) -> Result<String, String> {
    let kp = init_keypair();
    let sig = kp.sign(data.as_bytes());
    Ok(hex::encode(sig.to_bytes()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let _ = init_keypair();
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
                            std::process::exit(0);
                        }
                    });
            }
        })
        .invoke_handler(tauri::generate_handler![greet, get_public_key, sign_data])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = murmer_client_lib::run() {
        eprintln!("failed to launch Murmer client: {error}");
        std::process::exit(1);
    }
}

use base64::Engine;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use url::Url;

include!("types.rs");
include!("protected_storage.rs");
include!("storage.rs");
include!("bootstrap.rs");
include!("auth.rs");
include!("launch.rs");
include!("commands.rs");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let window_config = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .cloned()
                .ok_or_else(|| io::Error::other("main window configuration is missing"))?;
            let data_directory = local_webview_data_directory().map_err(io::Error::other)?;

            WebviewWindowBuilder::from_config(app.handle(), &window_config)?
                .data_directory(data_directory)
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap,
            sign_in,
            sign_out,
            select_data_directory,
            open_managed_directory,
            save_settings,
            launch,
            terminate_minecraft,
            submit_launcher_event,
            open_external,
            get_window_state,
            minimize_window,
            toggle_maximize_window,
            close_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

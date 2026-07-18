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
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use url::Url;

mod types;
use types::*;
mod protected_storage;
use protected_storage::*;
mod storage;
use storage::*;
mod bootstrap;
use bootstrap::*;
mod auth;
use auth::*;
mod launch_utils;
use launch_utils::*;
mod install_paths;
use install_paths::*;
mod install_state;
use install_state::*;
mod installation;
use installation::*;
mod modpack;
use modpack::*;
mod launch_plan;
use launch_plan::*;
mod minecraft_process;
use minecraft_process::*;
mod minecraft;
use minecraft::*;
mod commands;
use commands::*;

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
            commands::get_bootstrap,
            commands::sign_in,
            commands::sign_out,
            commands::select_account,
            commands::select_data_directory,
            commands::open_managed_directory,
            commands::save_settings,
            commands::launch,
            commands::terminate_minecraft,
            commands::open_external,
            commands::get_window_state,
            commands::minimize_window,
            commands::toggle_maximize_window,
            commands::close_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

const EMBEDDED_APP_CONFIG: &str = include_str!("../../config/app.config.json");
const EMBEDDED_CLIENT_CONFIG: &str = include_str!("../../config/client.config.json");
const EMBEDDED_DISTRIBUTION_CONFIG: &str = include_str!("../../config/distribution.json");
const EMBEDDED_SERVER_MANIFEST: &str = include_str!("../../config/server.manifest.json");

fn storage_root_path() -> PathBuf {
    if let Ok(app_data_path) = std::env::var("APPDATA") {
        return PathBuf::from(app_data_path).join(STORAGE_ROOT_DIR_NAME);
    }

    dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join(STORAGE_ROOT_DIR_NAME)
}

fn local_webview_data_directory() -> Result<PathBuf, String> {
    let local_data_root = std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(dirs::data_local_dir)
        .ok_or_else(|| "Windows Local AppData 경로를 찾지 못했습니다.".to_string())?;
    let current_path = local_data_root.join(LOCAL_WEBVIEW_DATA_DIR_NAME);
    let legacy_path = local_data_root.join(LEGACY_LOCAL_WEBVIEW_DATA_DIR_NAME);

    if !current_path.exists() && legacy_path.exists() {
        fs::rename(&legacy_path, &current_path).map_err(|error| {
            contextual_error(
                &format!(
                    "WebView Local AppData 폴더를 새 이름으로 이동하지 못했습니다 (from: {}, to: {})",
                    display_path(&legacy_path),
                    display_path(&current_path)
                ),
                error,
            )
        })?;
    }

    fs::create_dir_all(&current_path)
        .map_err(|error| io_error("WebView Local AppData 폴더를 만들지 못했습니다", &current_path, error))?;
    Ok(current_path)
}

fn user_config_path() -> PathBuf {
    storage_root_path().join("user-config.json")
}

fn default_data_directory() -> PathBuf {
    storage_root_path().join("data")
}

fn game_lock_path() -> PathBuf {
    storage_root_path().join("game.lock")
}

fn stale_game_lock_age_ms(lock_path: &Path) -> Option<i64> {
    let content = fs::read_to_string(lock_path).ok()?;
    let created_at = serde_json::from_str::<Value>(&content)
        .ok()
        .and_then(|state| state.get("createdAt").and_then(Value::as_i64))
        .or_else(|| content.trim().parse::<i64>().ok())?;
    let age_ms = now_ms().saturating_sub(created_at);

    if age_ms >= STALE_GAME_LOCK_MS {
        Some(age_ms)
    } else {
        None
    }
}

fn game_lock_process_ids(lock_path: &Path) -> Option<(Option<u32>, Option<u32>)> {
    let content = fs::read_to_string(lock_path).ok()?;
    let state = serde_json::from_str::<Value>(&content).ok()?;
    let launcher_process_id = state
        .get("launcherProcessId")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    let minecraft_process_id = state
        .get("minecraftProcessId")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    Some((launcher_process_id, minecraft_process_id))
}

#[cfg(windows)]
fn process_id_is_running(process_id: u32) -> Option<bool> {
    let script = format!(
        "if ($null -ne (Get-Process -Id {process_id} -ErrorAction SilentlyContinue)) {{ '1' }}"
    );

    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "1")
}

#[cfg(not(windows))]
fn process_id_is_running(process_id: u32) -> Option<bool> {
    Command::new("kill")
        .args(["-0", &process_id.to_string()])
        .status()
        .ok()
        .map(|status| status.success())
}

#[cfg(windows)]
fn minecraft_process_is_running() -> Option<bool> {
    let script = r#"
$process = Get-CimInstance Win32_Process |
  Where-Object {
    ($_.Name -match '(?i)^minecraft.*\.exe$') -or
    (($_.Name -match '(?i)^javaw?\.exe$') -and ($_.CommandLine -match '(?i)minecraft'))
  } |
  Select-Object -First 1
if ($null -ne $process) { '1' }
"#;

    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "1")
}

fn existing_game_lock_is_active(lock_path: &Path) -> bool {
    if let Some((launcher_process_id, minecraft_process_id)) = game_lock_process_ids(lock_path) {
        for process_id in [minecraft_process_id, launcher_process_id]
            .into_iter()
            .flatten()
        {
            match process_id_is_running(process_id) {
                Some(true) | None => return true,
                Some(false) => {}
            }
        }

        return false;
    }

    match minecraft_process_is_running() {
        Some(true) => true,
        None => stale_game_lock_age_ms(lock_path).is_none(),
        Some(false) => false,
    }
}

fn write_game_lock(file: &mut File, minecraft_process_id: Option<u32>) -> Result<(), String> {
    let state = json!({
        "createdAt": now_ms(),
        "launcherProcessId": std::process::id(),
        "minecraftProcessId": minecraft_process_id
    });
    let content = serde_json::to_vec(&state)
        .map_err(|error| format!("게임 실행 잠금 정보를 만들지 못했습니다: {error}"))?;

    file.set_len(0)
        .map_err(|error| format!("게임 실행 잠금 파일을 초기화하지 못했습니다: {error}"))?;
    file.write_all(&content)
        .map_err(|error| format!("게임 실행 잠금 파일을 쓰지 못했습니다: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("게임 실행 잠금 파일을 동기화하지 못했습니다: {error}"))
}

fn update_game_lock_process_id(process_id: u32) -> Result<(), String> {
    let lock_path = game_lock_path();
    let mut file = OpenOptions::new()
        .write(true)
        .open(&lock_path)
        .map_err(|error| io_error("게임 실행 잠금 파일을 열지 못했습니다", &lock_path, error))?;
    write_game_lock(&mut file, Some(process_id))
}

#[cfg(not(windows))]
fn minecraft_process_is_running() -> Option<bool> {
    Some(false)
}

fn try_acquire_game_lock() -> Result<bool, String> {
    if GAME_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(false);
    }

    let lock_path = game_lock_path();

    if let Some(parent) = lock_path.parent() {
        if fs::create_dir_all(parent).is_err() {
            GAME_RUNNING.store(false, Ordering::SeqCst);
            return Err(format!(
                "게임 실행 잠금 폴더를 만들지 못했습니다: {}",
                display_path(parent)
            ));
        }
    }

    for attempt in 0..2 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                if let Err(error) = write_game_lock(&mut file, None) {
                    drop(file);
                    let _ = fs::remove_file(&lock_path);
                    GAME_RUNNING.store(false, Ordering::SeqCst);
                    return Err(error);
                }
                return Ok(true);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempt == 0 => {
                if !existing_game_lock_is_active(&lock_path) {
                    let _ = fs::remove_file(&lock_path);
                    continue;
                }
            }
            Err(error) => {
                GAME_RUNNING.store(false, Ordering::SeqCst);
                return Err(format!(
                    "게임 실행 잠금을 만들지 못했습니다: {} ({error})",
                    display_path(&lock_path)
                ));
            }
        }

        break;
    }

    GAME_RUNNING.store(false, Ordering::SeqCst);
    Ok(false)
}

fn release_game_lock() {
    GAME_RUNNING.store(false, Ordering::SeqCst);
    let _ = fs::remove_file(game_lock_path());
}

fn embedded_config_for(relative_path: &str) -> Option<&'static str> {
    match relative_path {
        "config/app.config.json" => Some(EMBEDDED_APP_CONFIG),
        "config/client.config.json" => Some(EMBEDDED_CLIENT_CONFIG),
        "config/distribution.json" => Some(EMBEDDED_DISTRIBUTION_CONFIG),
        "config/server.manifest.json" => Some(EMBEDDED_SERVER_MANIFEST),
        _ => None,
    }
}

fn seed_embedded_project_file_if_missing(relative_path: &str) -> Result<(), String> {
    let content = embedded_config_for(relative_path)
        .ok_or_else(|| format!("Missing embedded project file: {relative_path}"))?;
    let path = storage_root_path().join(relative_path);

    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error("초기 설정 폴더를 만들지 못했습니다", parent, error))?;
    }

    fs::write(&path, content).map_err(|error| io_error("초기 설정 파일을 쓰지 못했습니다", &path, error))
}

fn seed_default_config_files_for_first_run() -> Result<(), String> {
    for relative_path in [
        "config/app.config.json",
        "config/client.config.json",
        "config/distribution.json",
        "config/server.manifest.json",
    ] {
        seed_embedded_project_file_if_missing(relative_path)?;
    }

    Ok(())
}

fn read_embedded_json_file(relative_path: &str) -> Result<Value, String> {
    let content = embedded_config_for(relative_path)
        .ok_or_else(|| format!("Missing embedded project file: {relative_path}"))?;
    serde_json::from_str(content)
        .map_err(|error| format!("내장 JSON 파일을 파싱하지 못했습니다 ({relative_path}): {error}"))
}

fn read_seeded_or_embedded_json_file(relative_path: &str) -> Result<Value, String> {
    let path = storage_root_path().join(relative_path);

    if path.exists() {
        return read_json_file(&path);
    }

    read_embedded_json_file(relative_path)
}

const TRUSTED_BROWSER_HOSTS: &[&str] = &[
    "login.microsoftonline.com",
    "microsoft.com",
    "minecraft.net",
    "mojang.com",
    "github.com",
];

const TRUSTED_DOWNLOAD_HOSTS: &[&str] = &[
    "piston-data.mojang.com",
    "piston-meta.mojang.com",
    "launcher.mojang.com",
    "libraries.minecraft.net",
    "resources.download.minecraft.net",
    "meta.fabricmc.net",
    "maven.fabricmc.net",
    "github.com",
    "githubusercontent.com",
    "github-releases.githubusercontent.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "codeload.github.com",
    "api.adoptium.net",
    "adoptium.net",
];

fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn url_host_matches_any(url: &Url, domains: &[&str]) -> bool {
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };

    domains.iter().any(|domain| host_matches(&host, domain))
}

fn is_trusted_browser_host(url: &Url) -> bool {
    url_host_matches_any(url, TRUSTED_BROWSER_HOSTS)
}

fn is_trusted_download_host(url: &Url) -> bool {
    url_host_matches_any(url, TRUSTED_DOWNLOAD_HOSTS)
}

fn validate_download_url(value: &str) -> Result<Url, String> {
    let parsed_url =
        Url::parse(value).map_err(|_| format!("다운로드 URL이 올바르지 않습니다: {value}"))?;

    if parsed_url.scheme() != "https" {
        return Err(format!("다운로드 URL은 HTTPS만 허용됩니다: {value}"));
    }

    if !is_trusted_download_host(&parsed_url) {
        return Err(format!("허용되지 않은 다운로드 도메인입니다: {value}"));
    }

    Ok(parsed_url)
}

fn read_distribution_manifest() -> Result<Value, String> {
    read_seeded_or_embedded_json_file("config/distribution.json")
}

fn read_json_file(path: &Path) -> Result<Value, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| io_error("JSON 파일을 읽지 못했습니다", path, error))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("JSON 파일을 파싱하지 못했습니다: {} ({error})", display_path(path)))
}

fn path_with_extra_extension(path: &Path, extension: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("config");

    path.with_file_name(format!("{file_name}.{extension}"))
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("파일을 삭제하지 못했습니다", path, error)),
    }
}

fn sync_parent_directory_best_effort(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
    }
}

fn recover_interrupted_user_config_write(path: &Path) -> Result<(), String> {
    let temp_path = path_with_extra_extension(path, "tmp");
    let backup_path = path_with_extra_extension(path, "bak");

    if path.exists() {
        remove_file_if_exists(&temp_path)?;
        return Ok(());
    }

    if backup_path.exists() {
        fs::rename(&backup_path, path).map_err(|error| {
            contextual_error(
                &format!(
                    "사용자 설정 백업 파일을 복구하지 못했습니다 (from: {}, to: {})",
                    display_path(&backup_path),
                    display_path(path)
                ),
                error,
            )
        })?;
        sync_parent_directory_best_effort(path);
        remove_file_if_exists(&temp_path)?;
        return Ok(());
    }

    if temp_path.exists() {
        fs::rename(&temp_path, path).map_err(|error| {
            contextual_error(
                &format!(
                    "사용자 설정 임시 파일을 복구하지 못했습니다 (from: {}, to: {})",
                    display_path(&temp_path),
                    display_path(path)
                ),
                error,
            )
        })?;
        sync_parent_directory_best_effort(path);
    }

    Ok(())
}

fn load_app_config() -> Result<Value, String> {
    read_seeded_or_embedded_json_file("config/app.config.json")
}

fn load_client_config() -> Result<Value, String> {
    read_seeded_or_embedded_json_file("config/client.config.json")
}

fn load_server_manifest() -> Result<Value, String> {
    read_seeded_or_embedded_json_file("config/server.manifest.json")
}

fn default_user_config() -> Value {
    json!({
        "settings": {
            "dataDirectory": default_data_directory().to_string_lossy(),
            "allowPrerelease": false,
            "maxRamMb": 8192,
            "gameResolution": "default",
            "extraJvmArgs": "",
            "extraGameArgs": "",
            "discordRichPresenceEnabled": false
        },
        "authSession": null,
        "lastDiagnostics": []
    })
}

fn merge_defaults(defaults: &Value, current: &Value) -> Value {
    match (defaults, current) {
        (Value::Object(default_map), Value::Object(current_map)) => {
            let mut merged = default_map.clone();

            for (key, value) in current_map {
                let next_value = default_map
                    .get(key)
                    .map(|default_value| merge_defaults(default_value, value))
                    .unwrap_or_else(|| value.clone());
                merged.insert(key.clone(), next_value);
            }

            Value::Object(merged)
        }
        (_, Value::Null) => defaults.clone(),
        (_, value) => value.clone(),
    }
}

fn load_or_create_user_config() -> Result<Value, String> {
    let path = user_config_path();
    let defaults = default_user_config();

    recover_interrupted_user_config_write(&path)?;

    if !path.exists() {
        seed_default_config_files_for_first_run()?;
        save_user_config(&defaults)?;
        return Ok(defaults);
    }

    let backup_path = path_with_extra_extension(&path, "bak");
    let current = match read_json_file(&path) {
        Ok(current) => {
            remove_file_if_exists(&backup_path)?;
            current
        }
        Err(primary_error) => {
            if !backup_path.exists() {
                return Err(format!(
                    "사용자 설정 파일이 손상되어 읽지 못했습니다. 원본 파일은 보존했습니다: {} ({primary_error})",
                    display_path(&path)
                ));
            }

            let backup = read_json_file(&backup_path).map_err(|backup_error| {
                format!(
                    "사용자 설정과 백업 파일을 모두 읽지 못했습니다. 두 파일은 보존했습니다. primary: {primary_error}; backup: {backup_error}"
                )
            })?;
            let corrupt_path = path_with_extra_extension(&path, &format!("corrupt-{}", now_ms()));

            fs::rename(&path, &corrupt_path).map_err(|error| {
                contextual_error(
                    &format!(
                        "손상된 사용자 설정 파일을 보존 위치로 이동하지 못했습니다 (from: {}, to: {})",
                        display_path(&path),
                        display_path(&corrupt_path)
                    ),
                    error,
                )
            })?;

            if let Err(error) = fs::rename(&backup_path, &path) {
                let _ = fs::rename(&corrupt_path, &path);
                return Err(contextual_error(
                    &format!(
                        "유효한 사용자 설정 백업을 복구하지 못했습니다 (from: {}, to: {})",
                        display_path(&backup_path),
                        display_path(&path)
                    ),
                    error,
                ));
            }

            sync_parent_directory_best_effort(&path);
            backup
        }
    };
    let mut merged = merge_defaults(&defaults, &current);

    if unprotect_auth_session_from_storage(&mut merged).is_err() {
        if let Some(config) = merged.as_object_mut() {
            config.insert("authSession".to_string(), Value::Null);
        }
    }

    if let Some(config) = merged.as_object_mut() {
        config.remove("lastLaunchPlan");
    }

    if let Some(settings) = merged.get_mut("settings").and_then(Value::as_object_mut) {
        let needs_default_data_directory = settings
            .get("dataDirectory")
            .and_then(Value::as_str)
            .is_none_or(|path| path.trim().is_empty());

        if needs_default_data_directory {
            settings.insert(
                "dataDirectory".to_string(),
                Value::String(default_data_directory().to_string_lossy().into_owned()),
            );
        }

        settings.remove("discordWebhookUrl");
        settings.remove("discordNoticesEnabled");
    }

    save_user_config(&merged)?;
    Ok(merged)
}

fn write_user_config_file_atomically(path: &Path, content: &str) -> Result<(), String> {
    let temp_path = path_with_extra_extension(path, "tmp");
    let backup_path = path_with_extra_extension(path, "bak");

    remove_file_if_exists(&temp_path)?;

    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| io_error("사용자 설정 임시 파일을 만들지 못했습니다", &temp_path, error))?;
    temp_file
        .write_all(content.as_bytes())
        .map_err(|error| io_error("사용자 설정 임시 파일을 쓰지 못했습니다", &temp_path, error))?;
    temp_file
        .sync_all()
        .map_err(|error| io_error("사용자 설정 임시 파일을 동기화하지 못했습니다", &temp_path, error))?;
    drop(temp_file);

    #[cfg(windows)]
    {
        remove_file_if_exists(&backup_path)?;

        if path.exists() {
            fs::rename(path, &backup_path).map_err(|error| {
                contextual_error(
                    &format!(
                        "기존 사용자 설정 파일을 백업하지 못했습니다 (from: {}, to: {})",
                        display_path(path),
                        display_path(&backup_path)
                    ),
                    error,
                )
            })?;
        }

        if let Err(error) = fs::rename(&temp_path, path) {
            if backup_path.exists() {
                let _ = fs::rename(&backup_path, path);
            }

            return Err(contextual_error(
                &format!(
                    "사용자 설정 임시 파일을 적용하지 못했습니다 (from: {}, to: {})",
                    display_path(&temp_path),
                    display_path(path)
                ),
                error,
            ));
        }

        remove_file_if_exists(&backup_path)?;
    }

    #[cfg(not(windows))]
    {
        fs::rename(&temp_path, path).map_err(|error| {
            contextual_error(
                &format!(
                    "사용자 설정 임시 파일을 적용하지 못했습니다 (from: {}, to: {})",
                    display_path(&temp_path),
                    display_path(path)
                ),
                error,
            )
        })?;
        remove_file_if_exists(&backup_path)?;
    }

    sync_parent_directory_best_effort(path);
    Ok(())
}

fn save_user_config(config: &Value) -> Result<(), String> {
    let path = user_config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error("사용자 설정 폴더를 만들지 못했습니다", parent, error))?;
    }

    let protected_config = protect_auth_session_for_storage(config)?;
    let content = serde_json::to_string_pretty(&protected_config).map_err(|error| error.to_string())?;
    write_user_config_file_atomically(&path, &format!("{content}\n"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

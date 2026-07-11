fn window_state(app: &tauri::AppHandle) -> WindowState {
    let maximized = app
        .get_webview_window("main")
        .and_then(|window| window.is_maximized().ok())
        .unwrap_or(false);

    WindowState { maximized }
}

fn emit_window_state(app: &tauri::AppHandle) {
    let _ = app.emit("window:state-changed", window_state(app));
}

#[tauri::command]
fn get_bootstrap() -> Result<Value, CommandError> {
    build_bootstrap_payload().map_err(command_error)
}

#[tauri::command]
async fn sign_in(app: tauri::AppHandle) -> Result<Value, CommandError> {
    if AUTH_PENDING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(json!({
            "ok": false,
            "code": "AUTH_ALREADY_RUNNING",
            "message": "Microsoft 로그인이 이미 진행 중입니다.",
            "bootstrap": build_bootstrap_payload().map_err(command_error)?
        }));
    }

    let sign_in_result = run_sign_in(&app).await;
    AUTH_PENDING.store(false, Ordering::SeqCst);

    match sign_in_result {
        Ok(mut result) => {
            if let Some(object) = result.as_object_mut() {
                object.insert(
                    "bootstrap".to_string(),
                    build_bootstrap_payload().map_err(command_error)?,
                );
            }

            Ok(result)
        }
        Err(error) => {
            let classified_error = classify_auth_error(error);

            Ok(json!({
                "ok": false,
                "code": classified_error.code,
                "message": classified_error.message,
                "bootstrap": build_bootstrap_payload().map_err(command_error)?
            }))
        }
    }
}

#[tauri::command]
fn sign_out() -> Result<Value, CommandError> {
    let mut user_config = load_or_create_user_config().map_err(command_error)?;

    if let Some(config) = user_config.as_object_mut() {
        config.insert("authSession".to_string(), Value::Null);
    }

    save_user_config(&user_config).map_err(command_error)?;

    Ok(json!({
        "ok": true,
        "bootstrap": build_bootstrap_payload().map_err(command_error)?
    }))
}

#[tauri::command]
fn select_data_directory(current_path: Option<String>) -> SelectDirectoryResult {
    let mut dialog = rfd::FileDialog::new().set_title("게임 저장 폴더 선택");

    if let Some(path) = current_path.filter(|path| !path.trim().is_empty()) {
        dialog = dialog.set_directory(path);
    }

    match dialog.pick_folder() {
        Some(path) => SelectDirectoryResult {
            canceled: false,
            path: Some(path.to_string_lossy().into_owned()),
        },
        None => SelectDirectoryResult {
            canceled: true,
            path: None,
        },
    }
}

fn managed_minecraft_profile_dir() -> Result<PathBuf, String> {
    Ok(profile_root_path())
}

fn managed_directory_path(kind: &str) -> Result<PathBuf, String> {
    let profile_dir = managed_minecraft_profile_dir()?;

    match kind {
        "profile" => Ok(profile_dir),
        "logs" => Ok(profile_dir.join("logs")),
        "screenshots" => Ok(profile_dir.join("screenshots")),
        _ => Err("알 수 없는 폴더 종류입니다.".to_string()),
    }
}

fn open_directory(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map_err(|error| io_error("폴더를 열지 못했습니다", path, error))?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| io_error("폴더를 열지 못했습니다", path, error))?;
        return Ok(());
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| io_error("폴더를 열지 못했습니다", path, error))?;
        Ok(())
    }
}

#[tauri::command]
fn open_managed_directory(kind: String) -> Result<Value, CommandError> {
    let path = managed_directory_path(kind.trim()).map_err(command_error)?;
    fs::create_dir_all(&path).map_err(|error| command_error(io_error("폴더를 만들지 못했습니다", &path, error)))?;
    open_directory(&path).map_err(command_error)?;

    Ok(json!({
        "ok": true,
        "path": path
    }))
}

const RISKY_JVM_ARG_PREFIXES: &[&str] = &[
    "-javaagent",
    "-agentlib",
    "-agentpath",
    "-Xbootclasspath",
    "-Djava.library.path",
    "-Dorg.lwjgl.librarypath",
    "-Djna.library.path",
    "-Dsun.boot.library.path",
    "-Dlog4j.configurationFile",
    "-Dlog4j2.configurationFile",
    "-Dcom.sun.management.jmxremote",
    "--patch-module",
    "--add-opens",
    "--add-exports",
];

const RISKY_GAME_ARG_NAMES: &[&str] = &[
    "--accessToken",
    "--uuid",
    "--username",
    "--userProperties",
    "--xuid",
    "--clientId",
    "--quickPlayPath",
    "--quickPlayMultiplayer",
    "--server",
    "--port",
];

fn split_setting_args(value: &str) -> impl Iterator<Item = &str> {
    value.split_whitespace().filter(|arg| !arg.is_empty())
}

fn matches_named_arg(arg: &str, name: &str) -> bool {
    arg == name || arg.strip_prefix(name).is_some_and(|suffix| suffix.starts_with('='))
}

fn unsafe_settings_warnings(patch: &Map<String, Value>) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Some(extra_jvm_args) = patch.get("extraJvmArgs").and_then(Value::as_str) {
        for arg in split_setting_args(extra_jvm_args) {
            if RISKY_JVM_ARG_PREFIXES
                .iter()
                .any(|prefix| arg.starts_with(prefix))
            {
                warnings.push(format!(
                    "JVM 인자 '{arg}'은 외부 코드, 네이티브 라이브러리, 런타임 보안 완화 설정으로 악용될 수 있어 권고되지 않습니다."
                ));
            }
        }
    }

    if let Some(extra_game_args) = patch.get("extraGameArgs").and_then(Value::as_str) {
        for arg in split_setting_args(extra_game_args) {
            if RISKY_GAME_ARG_NAMES
                .iter()
                .any(|name| matches_named_arg(arg, name))
            {
                warnings.push(format!(
                    "게임 인자 '{arg}'은 계정 세션, 플레이어 식별자, 접속 대상 같은 런처가 관리해야 할 값을 덮어쓸 수 있어 권고되지 않습니다."
                ));
            }
        }
    }

    warnings
}

fn validate_setting_string(key: &str, value: Value, max_len: usize) -> Result<Value, String> {
    let Some(text) = value.as_str() else {
        return Err(format!("{key} 설정은 문자열이어야 합니다."));
    };

    if text.len() > max_len {
        return Err(format!("{key} 설정이 너무 깁니다."));
    }

    Ok(Value::String(text.to_string()))
}

fn validate_setting_bool(key: &str, value: Value) -> Result<Value, String> {
    if !value.is_boolean() {
        return Err(format!("{key} 설정은 true/false 값이어야 합니다."));
    }

    Ok(value)
}

fn validate_max_ram_mb(value: Value, minimum_ram_mb: u64) -> Result<Value, String> {
    let Some(max_ram_mb) = value.as_u64() else {
        return Err("maxRamMb 설정은 숫자여야 합니다.".to_string());
    };

    if max_ram_mb < minimum_ram_mb || max_ram_mb > 131_072 {
        return Err(format!("maxRamMb 설정은 {minimum_ram_mb}MB 이상 131072MB 이하이어야 합니다."));
    }

    Ok(Value::Number(max_ram_mb.into()))
}

fn validate_game_resolution(value: Value) -> Result<Value, String> {
    let Some(text) = value.as_str() else {
        return Err("gameResolution 설정은 문자열이어야 합니다.".to_string());
    };
    let resolution = text.trim();

    if !GAME_RESOLUTION_OPTIONS.contains(&resolution) {
        return Err("gameResolution 설정은 드롭다운의 허용된 화면 크기여야 합니다.".to_string());
    }

    Ok(Value::String(resolution.to_string()))
}

fn validate_settings_patch(
    patch: Map<String, Value>,
    unsafe_acknowledged: bool,
) -> Result<Map<String, Value>, String> {
    let warnings = unsafe_settings_warnings(&patch);
    let minimum_ram_mb = load_server_manifest()?
        .pointer("/java/minimumRamMb")
        .and_then(Value::as_u64)
        .unwrap_or(1024);

    if !warnings.is_empty() && !unsafe_acknowledged {
        return Err(format!(
            "비권장 설정은 확인 후 저장할 수 있습니다. {}",
            warnings.join(" ")
        ));
    }

    let mut sanitized = Map::new();

    for (key, value) in patch {
        let sanitized_value = match key.as_str() {
            "dataDirectory" => validate_setting_string(&key, value, 4096)?,
            "extraJvmArgs" | "extraGameArgs" => validate_setting_string(&key, value, 4096)?,
            "allowPrerelease" => validate_setting_bool(&key, value)?,
            "maxRamMb" => validate_max_ram_mb(value, minimum_ram_mb)?,
            "gameResolution" => validate_game_resolution(value)?,
            _ => return Err(format!("허용되지 않은 설정 항목입니다: {key}")),
        };

        sanitized.insert(key, sanitized_value);
    }

    Ok(sanitized)
}

#[tauri::command]
fn save_settings(
    patch: Map<String, Value>,
    unsafe_acknowledged: Option<bool>,
) -> Result<Value, CommandError> {
    let patch =
        validate_settings_patch(patch, unsafe_acknowledged.unwrap_or(false)).map_err(command_error)?;
    let mut user_config = load_or_create_user_config().map_err(command_error)?;

    if let Some(settings) = user_config
        .get_mut("settings")
        .and_then(Value::as_object_mut)
    {
        for (key, value) in patch {
            settings.insert(key, value);
        }
    }

    save_user_config(&user_config).map_err(command_error)?;
    build_bootstrap_payload().map_err(command_error)
}


fn launch_blocking(app: tauri::AppHandle) -> Result<Value, CommandError> {
    if LAUNCH_PENDING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(json!({
            "ok": true,
            "mode": "ignored"
        }));
    }

    match try_acquire_game_lock() {
        Ok(true) => {}
        Ok(false) => {
            LAUNCH_PENDING.store(false, Ordering::SeqCst);
            return Ok(json!({
                "ok": false,
                "mode": "blocked",
                "message": "이미 Minecraft 실행이 진행 중입니다. 실행 중인 게임을 종료한 뒤 다시 시도해 주세요."
            }));
        }
        Err(error) => {
            LAUNCH_PENDING.store(false, Ordering::SeqCst);
            return Ok(json!({
                "ok": false,
                "mode": "failed",
                "message": "Minecraft 실행 준비를 시작하지 못했습니다.",
                "errorDetail": error
            }));
        }
    }

    let result = launch_minecraft(&app);
    LAUNCH_PENDING.store(false, Ordering::SeqCst);

    let should_release_lock = result
        .as_ref()
        .map(|value| value.get("mode").and_then(Value::as_str) != Some("launched"))
        .unwrap_or(true);

    if should_release_lock {
        release_game_lock();
    }

    match result {
        Ok(result) => Ok(result),
        Err(error) => {
            Ok(json!({
                "ok": false,
                "mode": "failed",
                "message": "Minecraft 실행을 시작하지 못했습니다.",
                "errorDetail": error
            }))
        }
    }
}

#[tauri::command]
async fn launch(app: tauri::AppHandle) -> Result<Value, CommandError> {
    tauri::async_runtime::spawn_blocking(move || launch_blocking(app))
        .await
        .map_err(command_error)?
}

#[tauri::command]
async fn terminate_minecraft() -> Result<Value, CommandError> {
    tauri::async_runtime::spawn_blocking(terminate_launched_minecraft)
        .await
        .map_err(command_error)?
        .map_err(command_error)
}

#[tauri::command]
async fn submit_launcher_event(
    event_type: String,
    metadata: Map<String, Value>,
) -> Result<Value, CommandError> {
    tauri::async_runtime::spawn_blocking(move || submit_launcher_event_blocking(event_type, metadata))
        .await
        .map_err(command_error)?
        .map_err(command_error)
}

fn submit_launcher_event_blocking(
    event_type: String,
    metadata: Map<String, Value>,
) -> Result<Value, String> {
    let event_type = event_type.trim();
    if event_type.is_empty() || event_type.len() > 80 {
        return Err("런처 이벤트 이름이 올바르지 않습니다.".to_string());
    }

    let app_config = load_app_config()?;
    let Some((endpoint, token)) = launcher_companion_endpoint(&app_config)? else {
        return Ok(json!({
            "ok": true,
            "submitted": false,
            "skipped": true,
            "reason": "launcher_companion_disabled"
        }));
    };

    let user_config = load_or_create_user_config()?;
    let auth = auth_summary(&user_config);
    if !auth.signed_in {
        return Ok(json!({
            "ok": true,
            "submitted": false,
            "skipped": true,
            "reason": "not_signed_in"
        }));
    }

    let mut body = Map::new();
    body.insert("eventType".to_string(), Value::String(event_type.to_string()));
    body.insert("source".to_string(), Value::String("launcher".to_string()));

    if let Some(player_name) = auth.player_name {
        body.insert("playerName".to_string(), Value::String(player_name));
    }

    if let Some(profile_id) = auth.profile_id.and_then(|value| normalize_minecraft_uuid(&value)) {
        body.insert("playerUuid".to_string(), Value::String(profile_id));
    }

    let sanitized_metadata = sanitize_launcher_event_metadata(metadata)?;
    body.insert("metadata".to_string(), Value::Object(sanitized_metadata));

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|error| format!("런처 이벤트 HTTP client를 만들지 못했습니다: {error}"))?;
    let response = client
        .post(endpoint)
        .bearer_auth(token)
        .json(&Value::Object(body))
        .send()
        .map_err(|error| format!("런처 이벤트를 서버에 보내지 못했습니다: {error}"))?;
    let status = response.status();
    let payload = response
        .json::<Value>()
        .unwrap_or_else(|_| json!({ "ok": status.is_success() }));

    if !status.is_success() {
        return Ok(json!({
            "ok": false,
            "submitted": true,
            "status": status.as_u16(),
            "response": payload
        }));
    }

    Ok(json!({
        "ok": true,
        "submitted": true,
        "status": status.as_u16(),
        "response": payload
    }))
}

fn launcher_companion_endpoint(app_config: &Value) -> Result<Option<(Url, String)>, String> {
    let Some(config) = app_config.get("launcherCompanion").and_then(Value::as_object) else {
        return Ok(None);
    };

    if !config
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(None);
    }

    let Some(base_url) = config
        .get("apiBaseUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let Some(token) = config
        .get("bearerToken")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let mut endpoint = Url::parse(base_url)
        .map_err(|_| "런처 companion API 주소가 올바르지 않습니다.".to_string())?;
    if endpoint.scheme() != "https" && endpoint.host_str() != Some("127.0.0.1") && endpoint.host_str() != Some("localhost") {
        return Err("런처 companion API는 HTTPS 주소만 사용할 수 있습니다.".to_string());
    }

    let mut path = endpoint.path().trim_end_matches('/').to_string();
    if !path.ends_with("/events/launcher") {
        path.push_str("/events/launcher");
    }
    endpoint.set_path(&path);

    Ok(Some((endpoint, token.to_string())))
}

fn normalize_minecraft_uuid(value: &str) -> Option<String> {
    let compact: String = value.chars().filter(|character| *character != '-').collect();
    if compact.len() != 32 || !compact.chars().all(|character| character.is_ascii_hexdigit()) {
        return None;
    }

    Some(format!(
        "{}-{}-{}-{}-{}",
        &compact[0..8],
        &compact[8..12],
        &compact[12..16],
        &compact[16..20],
        &compact[20..32]
    ))
}

fn sanitize_launcher_event_metadata(metadata: Map<String, Value>) -> Result<Map<String, Value>, String> {
    const MAX_METADATA_FIELDS: usize = 16;
    const MAX_METADATA_VALUE_LEN: usize = 160;

    if metadata.len() > MAX_METADATA_FIELDS {
        return Err("런처 이벤트 metadata 항목이 너무 많습니다.".to_string());
    }

    let mut sanitized = Map::new();
    for (key, value) in metadata {
        let trimmed_key = key.trim();
        if trimmed_key.is_empty() || trimmed_key.len() > 48 {
            return Err("런처 이벤트 metadata key가 올바르지 않습니다.".to_string());
        }

        let text_value = match value {
            Value::Null => String::new(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            Value::String(value) => value,
            other => serde_json::to_string(&other).unwrap_or_default(),
        };
        let normalized_value: String = text_value.chars().take(MAX_METADATA_VALUE_LEN).collect();
        sanitized.insert(trimmed_key.to_string(), Value::String(normalized_value));
    }

    Ok(sanitized)
}

#[tauri::command]
fn open_external(url: String) -> Result<Value, CommandError> {
    validate_external_url(&url).map_err(command_error)?;
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(command_error)?;
    Ok(json!({ "ok": true }))
}

fn validate_external_url(value: &str) -> Result<(), String> {
    let parsed_url = Url::parse(value)
        .map_err(|_| "허용되지 않은 외부 URL입니다.".to_string())?;

    if parsed_url.scheme() != "https" {
        return Err("외부 링크는 HTTPS만 열 수 있습니다.".to_string());
    }

    let app_config = load_app_config()?;
    let server_manifest = load_server_manifest()?;

    if is_configured_support_url(&parsed_url, &app_config)
        || is_configured_server_host(&parsed_url, &server_manifest)
        || is_trusted_browser_host(&parsed_url)
    {
        return Ok(());
    }

    Err("허용되지 않은 외부 도메인입니다.".to_string())
}

fn is_configured_support_url(parsed_url: &Url, app_config: &Value) -> bool {
    let Some(support_url) = app_config
        .get("supportUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    let Ok(configured_url) = Url::parse(support_url) else {
        return false;
    };

    if configured_url.scheme() != "https" {
        return false;
    }

    if parsed_url.host_str() != configured_url.host_str()
        || parsed_url.port_or_known_default() != configured_url.port_or_known_default()
    {
        return false;
    }

    path_is_at_or_below(parsed_url.path(), configured_url.path())
}

fn is_configured_server_host(parsed_url: &Url, server_manifest: &Value) -> bool {
    let Some(server_host) = server_manifest
        .get("address")
        .and_then(Value::as_str)
        .and_then(normalize_server_host)
    else {
        return false;
    };
    let Some(url_host) = parsed_url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };

    url_host == server_host
}

fn path_is_at_or_below(path: &str, allowed_path: &str) -> bool {
    let normalized_allowed_path = if allowed_path.is_empty() {
        "/"
    } else {
        allowed_path
    };

    normalized_allowed_path == "/"
        || path == normalized_allowed_path
        || path
            .strip_prefix(normalized_allowed_path)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn normalize_server_host(value: &str) -> Option<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return None;
    }

    let url_like = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed_url = Url::parse(&url_like).ok()?;

    parsed_url.host_str().map(str::to_ascii_lowercase)
}

#[tauri::command]
fn get_window_state(app: tauri::AppHandle) -> WindowState {
    window_state(&app)
}

#[tauri::command]
fn minimize_window(app: tauri::AppHandle) -> WindowState {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }

    emit_window_state(&app);
    window_state(&app)
}

#[tauri::command]
fn toggle_maximize_window(app: tauri::AppHandle) -> WindowState {
    if let Some(window) = app.get_webview_window("main") {
        let is_maximized = window.is_maximized().unwrap_or(false);
        let _ = if is_maximized {
            window.unmaximize()
        } else {
            window.maximize()
        };
    }

    emit_window_state(&app);
    window_state(&app)
}

#[tauri::command]
fn close_window(app: tauri::AppHandle) -> Result<Value, CommandError> {
    if let Some(window) = app.get_webview_window("main") {
        window.close().map_err(command_error)?;
    }

    Ok(json!({ "ok": true }))
}

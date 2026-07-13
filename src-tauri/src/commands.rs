use crate::*;

pub(crate) fn window_state(app: &tauri::AppHandle) -> WindowState {
    let maximized = app
        .get_webview_window("main")
        .and_then(|window| window.is_maximized().ok())
        .unwrap_or(false);

    WindowState { maximized }
}

pub(crate) fn emit_window_state(app: &tauri::AppHandle) {
    let _ = app.emit("window:state-changed", window_state(app));
}

#[tauri::command]
pub(crate) fn get_bootstrap() -> Result<Value, CommandError> {
    build_bootstrap_payload().map_err(command_error)
}

#[tauri::command]
pub(crate) async fn sign_in(app: tauri::AppHandle) -> Result<Value, CommandError> {
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
pub(crate) fn sign_out() -> Result<Value, CommandError> {
    let _mutation_guard = lock_user_config_mutation().map_err(command_error)?;
    let mut user_config = load_or_create_user_config().map_err(command_error)?;
    let previous = user_config.clone();

    if let Some(config) = user_config.as_object_mut() {
        config.insert("authSession".to_string(), Value::Null);
    }

    save_user_config_if_changed(&previous, &user_config).map_err(command_error)?;

    Ok(json!({
        "ok": true,
        "bootstrap": build_bootstrap_payload().map_err(command_error)?
    }))
}

#[tauri::command]
pub(crate) fn select_data_directory(current_path: Option<String>) -> SelectDirectoryResult {
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

pub(crate) fn managed_minecraft_profile_dir() -> Result<PathBuf, String> {
    Ok(profile_root_path())
}

pub(crate) fn managed_directory_path(kind: &str) -> Result<PathBuf, String> {
    let profile_dir = managed_minecraft_profile_dir()?;

    match kind {
        "profile" => Ok(profile_dir),
        "logs" => Ok(profile_dir.join("logs")),
        "screenshots" => Ok(profile_dir.join("screenshots")),
        _ => Err("알 수 없는 폴더 종류입니다.".to_string()),
    }
}

pub(crate) fn open_directory(path: &Path) -> Result<(), String> {
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
pub(crate) fn open_managed_directory(kind: String) -> Result<Value, CommandError> {
    let path = managed_directory_path(kind.trim()).map_err(command_error)?;
    fs::create_dir_all(&path)
        .map_err(|error| command_error(io_error("폴더를 만들지 못했습니다", &path, error)))?;
    open_directory(&path).map_err(command_error)?;

    Ok(json!({
        "ok": true,
        "path": path
    }))
}

pub(crate) const RISKY_JVM_ARG_PREFIXES: &[&str] = &[
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

pub(crate) const RISKY_GAME_ARG_NAMES: &[&str] = &[
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

pub(crate) fn setting_args(value: Option<&Value>) -> impl Iterator<Item = &str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|argument| !argument.is_empty())
}

pub(crate) fn matches_named_arg(arg: &str, name: &str) -> bool {
    arg == name
        || arg
            .strip_prefix(name)
            .is_some_and(|suffix| suffix.starts_with('='))
}

pub(crate) fn unsafe_settings_warnings(patch: &Map<String, Value>) -> Vec<String> {
    let mut warnings = Vec::new();

    if patch.contains_key("extraJvmArgs") {
        for arg in setting_args(patch.get("extraJvmArgs")) {
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

    if patch.contains_key("extraGameArgs") {
        for arg in setting_args(patch.get("extraGameArgs")) {
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

pub(crate) fn validate_setting_string(
    key: &str,
    value: Value,
    max_len: usize,
) -> Result<Value, String> {
    let Some(text) = value.as_str() else {
        return Err(format!("{key} 설정은 문자열이어야 합니다."));
    };

    if text.len() > max_len {
        return Err(format!("{key} 설정이 너무 깁니다."));
    }

    Ok(Value::String(text.to_string()))
}

pub(crate) fn validate_setting_bool(key: &str, value: Value) -> Result<Value, String> {
    if !value.is_boolean() {
        return Err(format!("{key} 설정은 true/false 값이어야 합니다."));
    }

    Ok(value)
}

pub(crate) fn validate_setting_args(key: &str, value: Value) -> Result<Value, String> {
    pub(crate) const MAX_ARGUMENT_COUNT: usize = 128;
    pub(crate) const MAX_ARGUMENT_LEN: usize = 4096;
    let Some(arguments) = value.as_array() else {
        return Err(format!("{key} 설정은 문자열 배열이어야 합니다."));
    };
    if arguments.len() > MAX_ARGUMENT_COUNT {
        return Err(format!("{key} 설정의 인자 수가 너무 많습니다."));
    }
    let mut sanitized = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let Some(argument) = argument.as_str() else {
            return Err(format!("{key} 설정의 각 인자는 문자열이어야 합니다."));
        };
        let argument = argument.trim();
        if argument.is_empty() {
            continue;
        }
        if argument.len() > MAX_ARGUMENT_LEN {
            return Err(format!("{key} 설정에 너무 긴 인자가 있습니다."));
        }
        sanitized.push(Value::String(argument.to_string()));
    }
    Ok(Value::Array(sanitized))
}

pub(crate) fn validate_max_ram_mb(value: Value, minimum_ram_mb: u64) -> Result<Value, String> {
    let Some(max_ram_mb) = value.as_u64() else {
        return Err("maxRamMb 설정은 숫자여야 합니다.".to_string());
    };

    if max_ram_mb < minimum_ram_mb || max_ram_mb > 131_072 {
        return Err(format!(
            "maxRamMb 설정은 {minimum_ram_mb}MB 이상 131072MB 이하이어야 합니다."
        ));
    }

    Ok(Value::Number(max_ram_mb.into()))
}

pub(crate) fn validate_game_resolution(value: Value) -> Result<Value, String> {
    let Some(text) = value.as_str() else {
        return Err("gameResolution 설정은 문자열이어야 합니다.".to_string());
    };
    let resolution = text.trim();

    if !GAME_RESOLUTION_OPTIONS.contains(&resolution) {
        return Err("gameResolution 설정은 드롭다운의 허용된 화면 크기여야 합니다.".to_string());
    }

    Ok(Value::String(resolution.to_string()))
}

pub(crate) fn validate_settings_patch(
    patch: Map<String, Value>,
) -> Result<(Map<String, Value>, Vec<String>), String> {
    let minimum_ram_mb = load_server_manifest()?
        .pointer("/java/minimumRamMb")
        .and_then(Value::as_u64)
        .unwrap_or(1024);

    let mut sanitized = Map::new();

    for (key, value) in patch {
        let sanitized_value = match key.as_str() {
            "dataDirectory" => validate_setting_string(&key, value, 4096)?,
            "extraJvmArgs" | "extraGameArgs" => validate_setting_args(&key, value)?,
            "allowPrerelease" => validate_setting_bool(&key, value)?,
            "maxRamMb" => validate_max_ram_mb(value, minimum_ram_mb)?,
            "gameResolution" => validate_game_resolution(value)?,
            _ => return Err(format!("허용되지 않은 설정 항목입니다: {key}")),
        };

        sanitized.insert(key, sanitized_value);
    }

    let warnings = unsafe_settings_warnings(&sanitized);
    Ok((sanitized, warnings))
}

#[tauri::command]
pub(crate) fn save_settings(
    patch: Map<String, Value>,
    unsafe_acknowledged: Option<bool>,
) -> Result<Value, CommandError> {
    let (patch, warnings) = validate_settings_patch(patch).map_err(command_error)?;
    if !warnings.is_empty() && !unsafe_acknowledged.unwrap_or(false) {
        return Ok(json!({
            "ok": false,
            "requiresConfirmation": true,
            "warnings": warnings
        }));
    }
    let _mutation_guard = lock_user_config_mutation().map_err(command_error)?;
    let mut user_config = load_or_create_user_config().map_err(command_error)?;
    let previous = user_config.clone();

    if let Some(settings) = user_config
        .get_mut("settings")
        .and_then(Value::as_object_mut)
    {
        for (key, value) in patch {
            settings.insert(key, value);
        }
    }

    save_user_config_if_changed(&previous, &user_config).map_err(command_error)?;
    Ok(json!({
        "ok": true,
        "bootstrap": build_bootstrap_payload().map_err(command_error)?
    }))
}

pub(crate) fn launch_blocking(app: tauri::AppHandle) -> Result<Value, CommandError> {
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
        Err(error) => Ok(json!({
            "ok": false,
            "mode": "failed",
            "message": "Minecraft 실행을 시작하지 못했습니다.",
            "errorDetail": error
        })),
    }
}

#[tauri::command]
pub(crate) async fn launch(app: tauri::AppHandle) -> Result<Value, CommandError> {
    tauri::async_runtime::spawn_blocking(move || launch_blocking(app))
        .await
        .map_err(command_error)?
}

#[tauri::command]
pub(crate) async fn terminate_minecraft() -> Result<Value, CommandError> {
    tauri::async_runtime::spawn_blocking(terminate_launched_minecraft)
        .await
        .map_err(command_error)?
        .map_err(command_error)
}

#[tauri::command]
pub(crate) fn open_external(url: String) -> Result<Value, CommandError> {
    validate_external_url(&url).map_err(command_error)?;
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(command_error)?;
    Ok(json!({ "ok": true }))
}

pub(crate) fn validate_external_url(value: &str) -> Result<(), String> {
    let parsed_url = Url::parse(value).map_err(|_| "허용되지 않은 외부 URL입니다.".to_string())?;

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

pub(crate) fn is_configured_support_url(parsed_url: &Url, app_config: &Value) -> bool {
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

pub(crate) fn is_configured_server_host(parsed_url: &Url, server_manifest: &Value) -> bool {
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

pub(crate) fn path_is_at_or_below(path: &str, allowed_path: &str) -> bool {
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

pub(crate) fn normalize_server_host(value: &str) -> Option<String> {
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
pub(crate) fn get_window_state(app: tauri::AppHandle) -> WindowState {
    window_state(&app)
}

#[tauri::command]
pub(crate) fn minimize_window(app: tauri::AppHandle) -> WindowState {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }

    emit_window_state(&app);
    window_state(&app)
}

#[tauri::command]
pub(crate) fn toggle_maximize_window(app: tauri::AppHandle) -> WindowState {
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
pub(crate) fn close_window(app: tauri::AppHandle) -> Result<Value, CommandError> {
    if let Some(window) = app.get_webview_window("main") {
        window.close().map_err(command_error)?;
    }

    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod settings_validation_tests {
    use super::*;

    #[test]
    fn argument_settings_require_arrays_and_preserve_spaces() {
        let value = json!(["-Dconfig=C:\\Program Files\\Star Prison\\config.json"]);
        let sanitized = validate_setting_args("extraJvmArgs", value.clone()).unwrap();
        assert_eq!(sanitized, value);
        assert!(validate_setting_args("extraJvmArgs", json!("legacy string")).is_err());
    }

    #[test]
    fn risky_arguments_are_classified_only_by_the_backend() {
        let patch = Map::from_iter([(
            "extraJvmArgs".to_string(),
            json!(["-javaagent=C:\\agent.jar"]),
        )]);
        assert_eq!(unsafe_settings_warnings(&patch).len(), 1);
    }
}

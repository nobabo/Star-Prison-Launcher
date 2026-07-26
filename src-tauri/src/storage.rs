use crate::*;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

pub(crate) const EMBEDDED_APP_CONFIG: &str = include_str!("../../config/app.config.json");
pub(crate) const EMBEDDED_CLIENT_CONFIG: &str = include_str!("../../config/client.config.json");
pub(crate) const EMBEDDED_DISTRIBUTION_CONFIG: &str =
    include_str!("../../config/distribution.json");
pub(crate) const EMBEDDED_SERVER_MANIFEST: &str = include_str!("../../config/server.manifest.json");
const APP_CONFIG_VERSION_KEY: &str = "configVersion";
const CLIENT_CONFIG_VERSION_KEY: &str = "schemaVersion";

pub(crate) fn storage_root_path() -> PathBuf {
    if let Ok(app_data_path) = std::env::var("APPDATA") {
        return PathBuf::from(app_data_path).join(STORAGE_ROOT_DIR_NAME);
    }

    dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join(STORAGE_ROOT_DIR_NAME)
}

pub(crate) fn launcher_logs_directory_path() -> PathBuf {
    storage_root_path().join("logs")
}

pub(crate) fn local_webview_data_directory() -> Result<PathBuf, String> {
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

    fs::create_dir_all(&current_path).map_err(|error| {
        io_error(
            "WebView Local AppData 폴더를 만들지 못했습니다",
            &current_path,
            error,
        )
    })?;
    Ok(current_path)
}

pub(crate) fn user_config_path() -> PathBuf {
    storage_root_path().join("user-config.json")
}

pub(crate) fn default_data_directory() -> PathBuf {
    storage_root_path().join("data")
}

pub(crate) fn game_lock_path() -> PathBuf {
    storage_root_path().join("game.lock")
}

pub(crate) fn game_lock_process_ids(lock_path: &Path) -> Option<((u32, u64), Option<(u32, u64)>)> {
    let content = fs::read_to_string(lock_path).ok()?;
    let state = serde_json::from_str::<Value>(&content).ok()?;
    if state.get("schemaVersion").and_then(Value::as_u64) != Some(2) {
        return None;
    }
    let launcher_process_id = state
        .get("launcherProcessId")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())?;
    let launcher_started_at = state
        .get("launcherProcessStartedAt")
        .and_then(Value::as_u64)?;
    let minecraft_process_id = state.get("minecraftProcessId")?;
    let minecraft_started_at = state.get("minecraftProcessStartedAt")?;
    let minecraft = if minecraft_process_id.is_null() && minecraft_started_at.is_null() {
        None
    } else {
        Some((
            minecraft_process_id
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())?,
            minecraft_started_at.as_u64()?,
        ))
    };
    Some(((launcher_process_id, launcher_started_at), minecraft))
}

#[cfg(windows)]
pub(crate) fn process_started_at(process_id: u32) -> Option<u64> {
    unsafe {
        let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process_handle.is_null() {
            return None;
        }
        let mut creation_time = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit_time = creation_time;
        let mut kernel_time = creation_time;
        let mut user_time = creation_time;
        let succeeded = GetProcessTimes(
            process_handle,
            &mut creation_time,
            &mut exit_time,
            &mut kernel_time,
            &mut user_time,
        ) != 0;
        let _ = CloseHandle(process_handle);
        succeeded.then_some(
            ((creation_time.dwHighDateTime as u64) << 32) | creation_time.dwLowDateTime as u64,
        )
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn process_started_at(process_id: u32) -> Option<u64> {
    let content = fs::read_to_string(format!("/proc/{process_id}/stat")).ok()?;
    let command_end = content.rfind(')')?;
    content[command_end + 1..]
        .split_whitespace()
        .nth(19)
        .and_then(|value| value.parse().ok())
}

#[cfg(all(not(windows), not(target_os = "linux")))]
pub(crate) fn process_started_at(_process_id: u32) -> Option<u64> {
    None
}

pub(crate) fn existing_game_lock_is_active(lock_path: &Path) -> bool {
    let Some((launcher, minecraft)) = game_lock_process_ids(lock_path) else {
        // 구형/손상 잠금은 소유권을 입증하지 못하므로 외부 Minecraft를 추측하지 않는다.
        return false;
    };

    // 실행 잠금은 런처 생명주기에만 귀속한다. 런처가 종료됐다면 그 런처가
    // 시작한 Minecraft가 아직 살아 있어도 다음 런처 실행을 막지 않는다.
    if process_started_at(launcher.0) != Some(launcher.1) {
        return false;
    }

    // 런처가 살아 있더라도 Minecraft가 이미 종료된 뒤 남은 잠금은 정리한다.
    // 실행 준비 중에는 Minecraft PID가 아직 없으므로 런처 잠금만 유지한다.
    minecraft.is_none_or(|process| process_started_at(process.0) == Some(process.1))
}

pub(crate) fn write_game_lock(
    file: &mut File,
    minecraft_process_id: Option<u32>,
) -> Result<(), String> {
    let launcher_process_id = std::process::id();
    let launcher_started_at = process_started_at(launcher_process_id).ok_or_else(|| {
        format!("런처 프로세스 생성 시각을 확인하지 못했습니다. PID: {launcher_process_id}")
    })?;
    let minecraft_started_at = minecraft_process_id
        .map(|process_id| {
            process_started_at(process_id).ok_or_else(|| {
                format!("Minecraft 프로세스 생성 시각을 확인하지 못했습니다. PID: {process_id}")
            })
        })
        .transpose()?;
    let state = json!({
        "schemaVersion": 2,
        "createdAt": now_ms(),
        "launcherProcessId": launcher_process_id,
        "launcherProcessStartedAt": launcher_started_at,
        "minecraftProcessId": minecraft_process_id,
        "minecraftProcessStartedAt": minecraft_started_at
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

pub(crate) fn update_game_lock_process_id(process_id: u32) -> Result<(), String> {
    let lock_path = game_lock_path();
    let mut file = OpenOptions::new()
        .write(true)
        .open(&lock_path)
        .map_err(|error| io_error("게임 실행 잠금 파일을 열지 못했습니다", &lock_path, error))?;
    write_game_lock(&mut file, Some(process_id))
}

pub(crate) fn try_acquire_game_lock() -> Result<bool, String> {
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

pub(crate) fn release_game_lock() {
    GAME_RUNNING.store(false, Ordering::SeqCst);
    let _ = fs::remove_file(game_lock_path());
}

pub(crate) fn embedded_config_for(relative_path: &str) -> Option<&'static str> {
    match relative_path {
        "config/app.config.json" => Some(EMBEDDED_APP_CONFIG),
        "config/client.config.json" => Some(EMBEDDED_CLIENT_CONFIG),
        "config/distribution.json" => Some(EMBEDDED_DISTRIBUTION_CONFIG),
        "config/server.manifest.json" => Some(EMBEDDED_SERVER_MANIFEST),
        _ => None,
    }
}

pub(crate) fn seed_embedded_project_file_if_missing(relative_path: &str) -> Result<(), String> {
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

    fs::write(&path, content)
        .map_err(|error| io_error("초기 설정 파일을 쓰지 못했습니다", &path, error))
}

pub(crate) fn seed_default_config_files_for_first_run() -> Result<(), String> {
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

pub(crate) fn read_embedded_json_file(relative_path: &str) -> Result<Value, String> {
    let content = embedded_config_for(relative_path)
        .ok_or_else(|| format!("Missing embedded project file: {relative_path}"))?;
    serde_json::from_str(content)
        .map_err(|error| format!("내장 JSON 파일을 파싱하지 못했습니다 ({relative_path}): {error}"))
}

pub(crate) fn read_seeded_or_embedded_json_file(relative_path: &str) -> Result<Value, String> {
    let path = storage_root_path().join(relative_path);

    if path.exists() {
        return read_json_file(&path);
    }

    read_embedded_json_file(relative_path)
}

pub(crate) const TRUSTED_BROWSER_HOSTS: &[&str] = &[
    "login.microsoftonline.com",
    "microsoft.com",
    "minecraft.net",
    "mojang.com",
    "github.com",
];

pub(crate) const TRUSTED_DOWNLOAD_HOSTS: &[&str] = &[
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

pub(crate) fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

pub(crate) fn url_host_matches_any(url: &Url, domains: &[&str]) -> bool {
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };

    domains.iter().any(|domain| host_matches(&host, domain))
}

pub(crate) fn is_trusted_browser_host(url: &Url) -> bool {
    url_host_matches_any(url, TRUSTED_BROWSER_HOSTS)
}

pub(crate) fn is_trusted_download_host(url: &Url) -> bool {
    url_host_matches_any(url, TRUSTED_DOWNLOAD_HOSTS)
}

pub(crate) fn validate_download_url(value: &str) -> Result<Url, String> {
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

pub(crate) fn distribution_manifest_cache_path() -> PathBuf {
    storage_root_path()
        .join("config")
        .join("distribution.remote.json")
}

pub(crate) fn validate_distribution_manifest(manifest: &Value) -> Result<(), String> {
    if manifest.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err("배포 manifest의 schemaVersion은 1이어야 합니다.".to_string());
    }

    let stable_channel = manifest
        .get("channels")
        .and_then(Value::as_object)
        .and_then(|channels| channels.get("stable"))
        .and_then(Value::as_object)
        .ok_or_else(|| "배포 manifest에 stable 채널이 없습니다.".to_string())?;

    for field in ["runtime", "clientBundle"] {
        if !stable_channel.get(field).is_some_and(Value::is_object) {
            return Err(format!(
                "배포 manifest stable 채널의 {field} 정보가 없습니다."
            ));
        }
    }

    Ok(())
}

pub(crate) fn read_cached_distribution_manifest() -> Result<Value, String> {
    let path = distribution_manifest_cache_path();
    let manifest = read_json_file(&path)?;
    validate_distribution_manifest(&manifest)?;
    Ok(manifest)
}

pub(crate) fn cache_remote_distribution_manifest(manifest: &Value) -> Result<(), String> {
    let path = distribution_manifest_cache_path();
    let parent = path
        .parent()
        .ok_or_else(|| "배포 manifest 캐시 폴더를 찾지 못했습니다.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| io_error("배포 manifest 캐시 폴더를 만들지 못했습니다", parent, error))?;
    let content = serde_json::to_string_pretty(manifest)
        .map_err(|error| format!("배포 manifest 캐시를 직렬화하지 못했습니다: {error}"))?;
    fs::write(&path, format!("{content}\n"))
        .map_err(|error| io_error("배포 manifest 캐시를 쓰지 못했습니다", &path, error))?;
    Ok(())
}

pub(crate) fn embedded_distribution_manifest_url() -> Result<String, String> {
    let embedded_app_config = read_embedded_json_file("config/app.config.json")?;
    embedded_app_config
        .get("distributionManifest")
        .and_then(Value::as_object)
        .and_then(|manifest| manifest.get("url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "내장 app config에 distribution manifest URL이 없습니다.".to_string())
}

pub(crate) fn read_distribution_manifest() -> Result<Value, String> {
    let remote_url = embedded_distribution_manifest_url().ok();

    if let Some(remote_url) = remote_url.as_deref() {
        match read_remote_json_once(remote_url).and_then(|manifest| {
            validate_distribution_manifest(&manifest)?;
            Ok(manifest)
        }) {
            Ok(manifest) => {
                eprintln!("GitHub 배포 manifest 사용: {remote_url}");
                if let Err(error) = cache_remote_distribution_manifest(&manifest) {
                    eprintln!("GitHub 배포 manifest 캐시 저장 실패: {error}");
                }
                return Ok(manifest);
            }
            Err(error) => {
                eprintln!("GitHub 배포 manifest 갱신 실패: {error}");
                match read_cached_distribution_manifest() {
                    Ok(manifest) => {
                        eprintln!("캐시된 GitHub 배포 manifest 사용");
                        return Ok(manifest);
                    }
                    Err(cache_error) => {
                        eprintln!("캐시된 배포 manifest를 사용할 수 없습니다: {cache_error}");
                    }
                }
            }
        }
    }

    eprintln!("내장 배포 manifest 사용");
    let manifest = read_embedded_json_file("config/distribution.json")?;
    validate_distribution_manifest(&manifest)?;
    Ok(manifest)
}

pub(crate) fn read_json_file(path: &Path) -> Result<Value, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| io_error("JSON 파일을 읽지 못했습니다", path, error))?;
    serde_json::from_str(&content).map_err(|error| {
        format!(
            "JSON 파일을 파싱하지 못했습니다: {} ({error})",
            display_path(path)
        )
    })
}

pub(crate) fn path_with_extra_extension(path: &Path, extension: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("config");

    path.with_file_name(format!("{file_name}.{extension}"))
}

pub(crate) fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("파일을 삭제하지 못했습니다", path, error)),
    }
}

pub(crate) fn sync_parent_directory_best_effort(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
    }
}

pub(crate) fn recover_interrupted_user_config_write(path: &Path) -> Result<(), String> {
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

fn app_config_version(config: &Value) -> u64 {
    config
        .get(APP_CONFIG_VERSION_KEY)
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn migrate_seeded_app_config(embedded: &Value, seeded: &Value) -> Result<(Value, bool), String> {
    let embedded_object = embedded
        .as_object()
        .ok_or_else(|| "내장 app config는 JSON object여야 합니다.".to_string())?;
    let seeded_object = seeded
        .as_object()
        .ok_or_else(|| "로컬 app config는 JSON object여야 합니다.".to_string())?;
    let embedded_version = app_config_version(embedded);
    let seeded_version = app_config_version(seeded);

    if embedded_version == 0 || seeded_version >= embedded_version {
        return Ok((seeded.clone(), false));
    }

    let mut migrated = seeded_object.clone();

    for (key, value) in embedded_object {
        migrated.insert(key.clone(), value.clone());
    }

    Ok((Value::Object(migrated), true))
}

pub(crate) fn load_app_config() -> Result<Value, String> {
    let _guard = APP_CONFIG_MIGRATION_LOCK
        .lock()
        .map_err(|_| "앱 설정 마이그레이션 잠금이 손상되었습니다.".to_string())?;
    let embedded = read_embedded_json_file("config/app.config.json")?;
    let path = storage_root_path().join("config/app.config.json");

    if !path.exists() {
        return Ok(embedded);
    }

    let seeded = read_json_file(&path)?;
    let (migrated, needs_save) = migrate_seeded_app_config(&embedded, &seeded)?;

    if needs_save {
        let content = serde_json::to_string_pretty(&migrated).map_err(|error| error.to_string())?;
        write_config_file_atomically(&path, &format!("{content}\n"))?;
    }

    Ok(migrated)
}

fn client_config_version(config: &Value) -> u64 {
    config
        .get(CLIENT_CONFIG_VERSION_KEY)
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn migrate_seeded_client_config(embedded: &Value, seeded: &Value) -> Result<(Value, bool), String> {
    let embedded_version = client_config_version(embedded);
    let seeded_version = client_config_version(seeded);

    if embedded_version == 0 || seeded_version >= embedded_version {
        return Ok((seeded.clone(), false));
    }

    let mut migrated = seeded
        .as_object()
        .cloned()
        .ok_or_else(|| "로컬 client config는 JSON object여야 합니다.".to_string())?;
    migrated.insert(
        CLIENT_CONFIG_VERSION_KEY.to_string(),
        Value::Number(embedded_version.into()),
    );

    if let Some(language) = embedded.pointer("/options/lang").cloned() {
        let options = migrated
            .entry("options".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| "로컬 client config의 options는 JSON object여야 합니다.".to_string())?;
        options.insert("lang".to_string(), language);
    }

    Ok((Value::Object(migrated), true))
}

pub(crate) fn load_client_config() -> Result<Value, String> {
    let _guard = CLIENT_CONFIG_MIGRATION_LOCK
        .lock()
        .map_err(|_| "클라이언트 설정 마이그레이션 잠금이 손상되었습니다.".to_string())?;
    let embedded = read_embedded_json_file("config/client.config.json")?;
    let path = storage_root_path().join("config/client.config.json");

    if !path.exists() {
        return Ok(embedded);
    }

    let seeded = read_json_file(&path)?;
    let (migrated, needs_save) = migrate_seeded_client_config(&embedded, &seeded)?;

    if needs_save {
        let content = serde_json::to_string_pretty(&migrated).map_err(|error| error.to_string())?;
        write_config_file_atomically(&path, &format!("{content}\n"))?;
    }

    Ok(migrated)
}

pub(crate) fn load_server_manifest() -> Result<Value, String> {
    read_seeded_or_embedded_json_file("config/server.manifest.json")
}

pub(crate) fn default_user_config() -> Value {
    json!({
        "settings": {
            "dataDirectory": default_data_directory().to_string_lossy(),
            "allowPrerelease": false,
            "maxRamMb": 8192,
            "gameResolution": "default",
            "extraJvmArgs": [],
            "extraGameArgs": []
        },
        "authSession": null,
        "authAccounts": [],
        "lastDiagnostics": []
    })
}

pub(crate) fn migrate_argument_array_setting(settings: &mut Map<String, Value>, key: &str) -> bool {
    let Some(value) = settings.get(key).cloned() else {
        return false;
    };
    let migrated = match value {
        Value::String(value) => Value::Array(
            value
                .split_whitespace()
                .filter(|argument| !argument.is_empty())
                .map(|argument| Value::String(argument.to_string()))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values),
        _ => Value::Array(Vec::new()),
    };
    if settings.get(key) == Some(&migrated) {
        return false;
    }
    settings.insert(key.to_string(), migrated);
    true
}

pub(crate) fn merge_defaults(defaults: &Value, current: &Value) -> Value {
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

pub(crate) fn load_or_create_user_config() -> Result<Value, String> {
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
    let mut needs_save = merged != current;

    if unprotect_auth_session_from_storage(&mut merged).is_err() {
        if let Some(config) = merged.as_object_mut() {
            config.insert("authSession".to_string(), Value::Null);
            config.insert("authAccounts".to_string(), Value::Array(Vec::new()));
            needs_save = true;
        }
    }

    if let Some(config) = merged.as_object_mut() {
        let has_saved_accounts = config
            .get("authAccounts")
            .and_then(Value::as_array)
            .is_some_and(|accounts| !accounts.is_empty());

        if !has_saved_accounts {
            if let Some(session) = config
                .get("authSession")
                .filter(|value| value.is_object())
                .cloned()
            {
                config.insert("authAccounts".to_string(), Value::Array(vec![session]));
                needs_save = true;
            }
        }

        needs_save |= config.remove("lastLaunchPlan").is_some();
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
            needs_save = true;
        }

        needs_save |= settings.remove("discordWebhookUrl").is_some();
        needs_save |= settings.remove("discordNoticesEnabled").is_some();
        needs_save |= migrate_argument_array_setting(settings, "extraJvmArgs");
        needs_save |= migrate_argument_array_setting(settings, "extraGameArgs");
    }

    if needs_save {
        save_user_config(&merged)?;
    }
    Ok(merged)
}

pub(crate) fn write_config_file_atomically(path: &Path, content: &str) -> Result<(), String> {
    let temp_path = path_with_extra_extension(path, "tmp");
    let backup_path = path_with_extra_extension(path, "bak");

    remove_file_if_exists(&temp_path)?;

    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| io_error("설정 임시 파일을 만들지 못했습니다", &temp_path, error))?;
    temp_file
        .write_all(content.as_bytes())
        .map_err(|error| io_error("설정 임시 파일을 쓰지 못했습니다", &temp_path, error))?;
    temp_file
        .sync_all()
        .map_err(|error| io_error("설정 임시 파일을 동기화하지 못했습니다", &temp_path, error))?;
    drop(temp_file);

    #[cfg(windows)]
    {
        remove_file_if_exists(&backup_path)?;

        if path.exists() {
            fs::rename(path, &backup_path).map_err(|error| {
                contextual_error(
                    &format!(
                        "기존 설정 파일을 백업하지 못했습니다 (from: {}, to: {})",
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
                    "설정 임시 파일을 적용하지 못했습니다 (from: {}, to: {})",
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
                    "설정 임시 파일을 적용하지 못했습니다 (from: {}, to: {})",
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

pub(crate) fn save_user_config(config: &Value) -> Result<(), String> {
    let _guard = USER_CONFIG_WRITE_LOCK
        .lock()
        .map_err(|_| "사용자 설정 저장 잠금이 손상되었습니다.".to_string())?;
    let path = user_config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error("사용자 설정 폴더를 만들지 못했습니다", parent, error))?;
    }

    let protected_config = protect_auth_session_for_storage(config)?;
    let content =
        serde_json::to_string_pretty(&protected_config).map_err(|error| error.to_string())?;
    write_config_file_atomically(&path, &format!("{content}\n"))
}

pub(crate) fn save_user_config_if_changed(
    previous: &Value,
    current: &Value,
) -> Result<bool, String> {
    if previous == current {
        return Ok(false);
    }
    save_user_config(current)?;
    Ok(true)
}

pub(crate) fn lock_user_config_mutation() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    USER_CONFIG_MUTATION_LOCK
        .lock()
        .map_err(|_| "사용자 설정 변경 잠금이 손상되었습니다.".to_string())
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod user_config_migration_tests {
    use super::*;

    #[test]
    fn migrates_legacy_argument_strings_to_arrays() {
        let mut settings = Map::from_iter([(
            "extraJvmArgs".to_string(),
            Value::String("-Xmx2G -Ddemo=true".to_string()),
        )]);
        assert!(migrate_argument_array_setting(
            &mut settings,
            "extraJvmArgs"
        ));
        assert_eq!(settings["extraJvmArgs"], json!(["-Xmx2G", "-Ddemo=true"]));
        assert!(!migrate_argument_array_setting(
            &mut settings,
            "extraJvmArgs"
        ));
    }

    #[test]
    fn migrates_seeded_app_config_by_version_and_preserves_local_only_keys() {
        let embedded = json!({
            "configVersion": 2,
            "supportUrl": "https://example.com/new",
            "discordNotices": {
                "endpointUrl": "https://notices.example.com/notices"
            }
        });
        let seeded = json!({
            "supportUrl": "https://example.com/old",
            "discordNotices": {
                "endpointUrl": "https://old.example.com/notices"
            },
            "launcherCompanion": {
                "bearerToken": "local-only"
            }
        });

        let (migrated, changed) = migrate_seeded_app_config(&embedded, &seeded).unwrap();

        assert!(changed);
        assert_eq!(migrated["configVersion"], 2);
        assert_eq!(migrated["supportUrl"], "https://example.com/new");
        assert_eq!(
            migrated["discordNotices"]["endpointUrl"],
            "https://notices.example.com/notices"
        );
        assert_eq!(migrated["launcherCompanion"]["bearerToken"], "local-only");
    }

    #[test]
    fn keeps_seeded_app_config_when_version_is_current() {
        let embedded = json!({
            "configVersion": 2,
            "supportUrl": "https://example.com/new"
        });
        let seeded = json!({
            "configVersion": 2,
            "supportUrl": "https://example.com/local"
        });

        let (migrated, changed) = migrate_seeded_app_config(&embedded, &seeded).unwrap();

        assert!(!changed);
        assert_eq!(migrated, seeded);
    }
    #[test]
    fn migrates_client_language_without_resetting_other_options() {
        let embedded = json!({
            "schemaVersion": 2,
            "options": {
                "lang": "ko_kr",
                "renderDistance": "12"
            }
        });
        let seeded = json!({
            "schemaVersion": 1,
            "options": {
                "lang": "en_us",
                "renderDistance": "24"
            }
        });

        let (migrated, changed) = migrate_seeded_client_config(&embedded, &seeded).unwrap();

        assert!(changed);
        assert_eq!(migrated["schemaVersion"], 2);
        assert_eq!(migrated["options"]["lang"], "ko_kr");
        assert_eq!(migrated["options"]["renderDistance"], "24");
    }

    fn game_lock_test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "star-prison-{label}-{}-{}.lock",
            std::process::id(),
            now_ms()
        ))
    }

    #[test]
    fn game_lock_requires_the_same_process_instance() {
        let path = game_lock_test_path("process-identity");
        let process_id = std::process::id();
        let started_at = process_started_at(process_id).expect("current process start time");
        let lock = json!({
            "schemaVersion": 2,
            "createdAt": now_ms(),
            "launcherProcessId": process_id,
            "launcherProcessStartedAt": started_at,
            "minecraftProcessId": null,
            "minecraftProcessStartedAt": null
        });
        fs::write(&path, serde_json::to_vec(&lock).unwrap()).unwrap();
        assert!(existing_game_lock_is_active(&path));

        let reused_pid_lock = json!({
            "schemaVersion": 2,
            "createdAt": now_ms(),
            "launcherProcessId": process_id,
            "launcherProcessStartedAt": started_at + 1,
            "minecraftProcessId": null,
            "minecraftProcessStartedAt": null
        });
        fs::write(&path, serde_json::to_vec(&reused_pid_lock).unwrap()).unwrap();
        assert!(!existing_game_lock_is_active(&path));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn legacy_lock_does_not_fall_back_to_global_minecraft_scan() {
        let path = game_lock_test_path("legacy");
        let legacy_lock = json!({
            "createdAt": now_ms(),
            "launcherProcessId": std::process::id(),
            "minecraftProcessId": null
        });
        fs::write(&path, serde_json::to_vec(&legacy_lock).unwrap()).unwrap();
        assert!(!existing_game_lock_is_active(&path));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn game_lock_is_stale_when_minecraft_process_has_ended() {
        let path = game_lock_test_path("ended-minecraft");
        let process_id = std::process::id();
        let started_at = process_started_at(process_id).expect("current process start time");
        let lock = json!({
            "schemaVersion": 2,
            "createdAt": now_ms(),
            "launcherProcessId": process_id,
            "launcherProcessStartedAt": started_at,
            "minecraftProcessId": process_id,
            "minecraftProcessStartedAt": started_at + 1
        });
        fs::write(&path, serde_json::to_vec(&lock).unwrap()).unwrap();

        assert!(!existing_game_lock_is_active(&path));
        fs::remove_file(path).unwrap();
    }
}

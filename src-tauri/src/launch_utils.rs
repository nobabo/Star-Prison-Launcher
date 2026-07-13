use crate::*;

pub(crate) const GAME_RESOLUTION_OPTIONS: &[&str] = &[
    "default",
    "1280x720",
    "1366x768",
    "1600x900",
    "1920x1080",
    "2560x1440",
];

pub(crate) fn value_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn value_string_vec(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn split_user_args(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|argument| !argument.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn emit_launch_state(app: &tauri::AppHandle, label: &str, progress: f64) {
    let _ = app.emit(
        "launcher:launch-state-changed",
        json!({
            "label": label,
            "progress": progress
        }),
    );
}

pub(crate) fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn io_error(action: &str, path: &Path, error: impl std::fmt::Display) -> String {
    format!("{action}: {} ({error})", display_path(path))
}

pub(crate) fn contextual_error(context: &str, error: impl std::fmt::Display) -> String {
    format!("{context}: {error}")
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| io_error("SHA-256 계산을 위해 파일을 열지 못했습니다", path, error))?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)
        .map_err(|error| io_error("SHA-256 계산 중 파일을 읽지 못했습니다", path, error))?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn sha1_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| io_error("SHA-1 계산을 위해 파일을 열지 못했습니다", path, error))?;
    let mut hasher = Sha1::new();
    io::copy(&mut file, &mut hasher)
        .map_err(|error| io_error("SHA-1 계산 중 파일을 읽지 못했습니다", path, error))?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Clone, Copy)]
pub(crate) struct ZipExtractionLimits {
    pub(crate) max_file_count: usize,
    pub(crate) max_entry_count: usize,
    pub(crate) max_path_depth: usize,
    pub(crate) max_uncompressed_size: u64,
}

pub(crate) const RUNTIME_ZIP_EXTRACTION_LIMITS: ZipExtractionLimits = ZipExtractionLimits {
    max_file_count: 20_000,
    max_entry_count: 24_000,
    max_path_depth: 32,
    max_uncompressed_size: 1024 * 1024 * 1024,
};
pub(crate) const CLIENT_ZIP_EXTRACTION_LIMITS: ZipExtractionLimits = ZipExtractionLimits {
    max_file_count: 50_000,
    max_entry_count: 60_000,
    max_path_depth: 32,
    max_uncompressed_size: 2 * 1024 * 1024 * 1024,
};
pub(crate) const NATIVE_ZIP_EXTRACTION_LIMITS: ZipExtractionLimits = ZipExtractionLimits {
    max_file_count: 2_000,
    max_entry_count: 2_500,
    max_path_depth: 16,
    max_uncompressed_size: 512 * 1024 * 1024,
};
pub(crate) const MAX_UNSIZED_DOWNLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub(crate) fn account_zip_entry(
    path: &Path,
    name: &str,
    is_directory: bool,
    uncompressed_size: u64,
    limits: ZipExtractionLimits,
    file_count: &mut usize,
    total_uncompressed_size: &mut u64,
) -> Result<(), String> {
    let path_depth = path.components().count();
    if path_depth > limits.max_path_depth {
        return Err(format!(
            "압축 파일 경로 중첩이 제한을 초과했습니다: {path_depth}/{} ({})",
            limits.max_path_depth,
            path.display()
        ));
    }
    if is_directory {
        return (uncompressed_size == 0).then_some(()).ok_or_else(|| {
            format!("압축 파일 디렉터리 엔트리의 크기 정보가 비정상입니다: {name}")
        });
    }

    *file_count = file_count
        .checked_add(1)
        .ok_or_else(|| "압축 파일 개수 계산이 초과되었습니다.".to_string())?;
    if *file_count > limits.max_file_count {
        return Err(format!(
            "압축 파일 개수가 제한을 초과했습니다: {file_count}/{}",
            limits.max_file_count
        ));
    }
    *total_uncompressed_size = total_uncompressed_size
        .checked_add(uncompressed_size)
        .ok_or_else(|| "압축 해제 크기 계산이 overflow되었습니다.".to_string())?;
    if *total_uncompressed_size > limits.max_uncompressed_size {
        return Err(format!(
            "압축 해제 크기가 제한을 초과했습니다: {}/{} bytes",
            total_uncompressed_size, limits.max_uncompressed_size
        ));
    }
    Ok(())
}

pub(crate) fn validate_zip_archive_limits(
    zip_path: &Path,
    limits: ZipExtractionLimits,
) -> Result<usize, String> {
    let file = File::open(zip_path)
        .map_err(|error| io_error("압축 파일을 열지 못했습니다", zip_path, error))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        contextual_error(
            &format!("압축 파일을 읽지 못했습니다 ({})", display_path(zip_path)),
            error,
        )
    })?;
    let entry_count = archive.len();

    if entry_count > limits.max_entry_count {
        return Err(format!(
            "압축 파일 엔트리 수가 제한을 초과했습니다: {entry_count}/{}",
            limits.max_entry_count
        ));
    }

    let mut file_count = 0usize;
    let mut total_uncompressed_size = 0u64;
    let mut seen_paths = HashSet::new();

    for index in 0..entry_count {
        let file = archive.by_index(index).map_err(|error| {
            contextual_error(
                &format!("압축 파일 엔트리를 읽지 못했습니다 (index: {index})"),
                error,
            )
        })?;
        let enclosed_path = file.enclosed_name().ok_or_else(|| {
            format!(
                "압축 파일에 허용되지 않은 경로가 포함되어 있습니다: {}",
                file.name()
            )
        })?;
        if !seen_paths.insert(enclosed_path.to_path_buf()) {
            return Err(format!(
                "압축 파일에 중복 경로가 포함되어 있습니다: {}",
                enclosed_path.display()
            ));
        }

        account_zip_entry(
            &enclosed_path,
            file.name(),
            file.is_dir(),
            file.size(),
            limits,
            &mut file_count,
            &mut total_uncompressed_size,
        )?;
    }

    Ok(file_count)
}

pub(crate) fn extract_zip_file_with_limits_and_progress<F>(
    zip_path: &Path,
    destination: &Path,
    limits: ZipExtractionLimits,
    root_prefixes: &[&str],
    preserve_existing_files: bool,
    mut on_file: F,
) -> Result<(), String>
where
    F: FnMut(usize, usize, &Path),
{
    let preflight_file_count = preserve_existing_files
        .then(|| validate_zip_archive_limits(zip_path, limits))
        .transpose()?;
    fs::create_dir_all(destination).map_err(|error| {
        io_error(
            "압축 해제 대상 폴더를 만들지 못했습니다",
            destination,
            error,
        )
    })?;

    let file = File::open(zip_path)
        .map_err(|error| io_error("압축 파일을 열지 못했습니다", zip_path, error))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        contextual_error(
            &format!("압축 파일을 읽지 못했습니다 ({})", display_path(zip_path)),
            error,
        )
    })?;
    let mut seen_paths = HashSet::new();
    let mut seen_output_paths = HashSet::new();
    let entry_count = archive.len();
    if entry_count > limits.max_entry_count {
        return Err(format!(
            "압축 파일 엔트리 수가 제한을 초과했습니다: {entry_count}/{}",
            limits.max_entry_count
        ));
    }
    let total_file_count = preflight_file_count.unwrap_or(entry_count);
    let mut file_count = 0usize;
    let mut total_uncompressed_size = 0u64;
    let mut extracted_file_count = 0usize;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| {
            contextual_error(
                &format!("압축 파일 엔트리를 읽지 못했습니다 (index: {index})"),
                error,
            )
        })?;
        let enclosed_path = file.enclosed_name().ok_or_else(|| {
            format!(
                "압축 파일에 허용되지 않은 경로가 포함되어 있습니다: {}",
                file.name()
            )
        })?;
        if !seen_paths.insert(enclosed_path.to_path_buf()) {
            return Err(format!(
                "압축 파일에 중복 경로가 포함되어 있습니다: {}",
                enclosed_path.display()
            ));
        }

        account_zip_entry(
            &enclosed_path,
            file.name(),
            file.is_dir(),
            file.size(),
            limits,
            &mut file_count,
            &mut total_uncompressed_size,
        )?;

        let Some(relative_output_path) = zip_entry_output_path(&enclosed_path, root_prefixes)
        else {
            continue;
        };

        if !seen_output_paths.insert(relative_output_path.clone()) {
            return Err(format!(
                "압축 파일에 중복 출력 경로가 포함되어 있습니다: {}",
                relative_output_path.display()
            ));
        }

        let output_path = destination.join(relative_output_path);

        if file.is_dir() {
            if !preserve_existing_files || !output_path.exists() {
                fs::create_dir_all(&output_path).map_err(|error| {
                    io_error("압축 해제 폴더를 만들지 못했습니다", &output_path, error)
                })?;
            }
            continue;
        }

        if preserve_existing_files && output_path.exists() {
            continue;
        }

        extracted_file_count += 1;
        on_file(extracted_file_count, total_file_count, &output_path);

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                io_error(
                    "압축 해제 대상 상위 폴더를 만들지 못했습니다",
                    parent,
                    error,
                )
            })?;
        }

        let expected_size = file.size();
        let mut output_file = File::create(&output_path)
            .map_err(|error| io_error("압축 해제 파일을 만들지 못했습니다", &output_path, error))?;
        let mut limited_reader = (&mut file).take(expected_size + 1);
        let written_size = io::copy(&mut limited_reader, &mut output_file)
            .map_err(|error| io_error("압축 해제 파일을 쓰지 못했습니다", &output_path, error))?;

        if written_size != expected_size {
            return Err(format!(
                "압축 파일 엔트리의 실제 해제 크기가 비정상입니다: {} (expected: {expected_size}, actual: {written_size})",
                output_path.display()
            ));
        }
    }

    Ok(())
}

pub(crate) fn extract_zip_file_with_limits(
    zip_path: &Path,
    destination: &Path,
    limits: ZipExtractionLimits,
) -> Result<(), String> {
    extract_zip_file_with_limits_and_progress(
        zip_path,
        destination,
        limits,
        &[],
        false,
        |_, _, _| {},
    )
}

pub(crate) fn zip_entry_output_path(
    enclosed_path: &Path,
    root_prefixes: &[&str],
) -> Option<PathBuf> {
    for root_prefix in root_prefixes {
        let Ok(stripped_path) = enclosed_path.strip_prefix(root_prefix) else {
            continue;
        };

        if stripped_path.as_os_str().is_empty() {
            return None;
        }

        return Some(stripped_path.to_path_buf());
    }

    Some(enclosed_path.to_path_buf())
}

pub(crate) fn remove_path_if_exists(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|error| io_error("폴더를 삭제하지 못했습니다", path, error))
    } else {
        fs::remove_file(path).map_err(|error| io_error("파일을 삭제하지 못했습니다", path, error))
    }
}

pub(crate) fn remove_empty_dir_best_effort(path: &Path) {
    match fs::remove_dir(path) {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
            ) => {}
        Err(_) => {}
    }
}

pub(crate) fn cleanup_empty_launcher_cache_dirs(data_directory: &Path) {
    for relative_path in [
        Path::new("downloads").join("release-archives"),
        Path::new("downloads").join("modpack-files"),
        Path::new("downloads").join("runtime"),
        Path::new("downloads").join("client"),
        PathBuf::from("downloads"),
        Path::new("staged").join("profile"),
        PathBuf::from("staged"),
        Path::new("runtime").join("staged"),
    ] {
        remove_empty_dir_best_effort(&data_directory.join(relative_path));
    }
}

pub(crate) fn remove_path_if_exists_with_permission_repair(path: &Path) -> Result<(), String> {
    match remove_path_if_exists(path) {
        Ok(()) => Ok(()),
        Err(first_error) => {
            repair_path_permissions_deep(path)?;
            remove_path_if_exists(path).map_err(|second_error| {
                format!(
                    "경로 삭제가 권한 보정 후에도 실패했습니다: {} first: {first_error}; second: {second_error}",
                    display_path(path)
                )
            })
        }
    }
}

pub(crate) fn extract_zip_archive(
    zip_path: &Path,
    destination: &Path,
    limits: ZipExtractionLimits,
) -> Result<(), String> {
    remove_path_if_exists_with_permission_repair(destination)?;
    extract_zip_file_with_limits(zip_path, destination, limits)
}

pub(crate) fn progress_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("파일")
        .to_string()
}

pub(crate) fn extract_zip_archive_with_limits_and_progress(
    app: &tauri::AppHandle,
    zip_path: &Path,
    destination: &Path,
    limits: ZipExtractionLimits,
    root_prefixes: &[&str],
    progress_start: f64,
    progress_end: f64,
) -> Result<(), String> {
    remove_path_if_exists_with_permission_repair(destination)?;
    extract_zip_file_with_limits_and_progress(
        zip_path,
        destination,
        limits,
        root_prefixes,
        false,
        |file_index, total_file_count, output_path| {
            let progress = if total_file_count > 0 {
                let ratio = file_index as f64 / total_file_count as f64;
                progress_start + ((progress_end - progress_start) * ratio)
            } else {
                progress_start
            };
            emit_launch_state(
                app,
                &format!("{} 설치중", progress_file_name(output_path)),
                progress,
            );
        },
    )
}

pub(crate) fn extract_zip_archive_preserving_existing_files_with_limits_and_progress(
    app: &tauri::AppHandle,
    zip_path: &Path,
    destination: &Path,
    limits: ZipExtractionLimits,
    root_prefixes: &[&str],
    progress_start: f64,
    progress_end: f64,
) -> Result<(), String> {
    extract_zip_file_with_limits_and_progress(
        zip_path,
        destination,
        limits,
        root_prefixes,
        true,
        |file_index, total_file_count, output_path| {
            let progress = if total_file_count > 0 {
                let ratio = file_index as f64 / total_file_count as f64;
                progress_start + ((progress_end - progress_start) * ratio)
            } else {
                progress_start
            };
            emit_launch_state(
                app,
                &format!("{} 설치중", progress_file_name(output_path)),
                progress,
            );
        },
    )
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
pub(crate) fn clear_readonly_recursively(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::metadata(path)
        .map_err(|error| io_error("권한 보정 대상 정보를 읽지 못했습니다", path, error))?;

    if metadata.is_dir() {
        for entry in fs::read_dir(path)
            .map_err(|error| io_error("권한 보정 대상 폴더를 읽지 못했습니다", path, error))?
        {
            let entry = entry.map_err(|error| {
                contextual_error("권한 보정 대상 항목을 읽지 못했습니다", error)
            })?;
            clear_readonly_recursively(&entry.path())?;
        }
    }

    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)
            .map_err(|error| io_error("읽기 전용 속성을 해제하지 못했습니다", path, error))?;
    }

    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn clear_readonly_recursively(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub(crate) fn current_windows_accounts() -> Vec<String> {
    let username = std::env::var("USERNAME")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let domain = std::env::var("USERDOMAIN")
        .ok()
        .filter(|value| !value.trim().is_empty());

    let mut accounts = Vec::new();

    if let (Some(domain), Some(username)) = (&domain, &username) {
        accounts.push(format!("{domain}\\{username}"));
    }

    if let Some(username) = username {
        if !accounts.iter().any(|account| account == &username) {
            accounts.push(username);
        }
    }

    accounts
}

#[cfg(windows)]
pub(crate) fn grant_current_user_full_control(path: &Path, recursive: bool) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let accounts = current_windows_accounts();
    if accounts.is_empty() {
        return Ok(());
    }

    let mut errors = Vec::new();

    for account in accounts {
        let output = Command::new("icacls")
            .arg(path)
            .arg("/grant")
            .arg(format!("{account}:(OI)(CI)F"))
            .arg("/C")
            .arg("/Q")
            .args(if recursive { &["/T"][..] } else { &[][..] })
            .output()
            .map_err(|error| {
                contextual_error("icacls 권한 보정 명령을 실행하지 못했습니다", error)
            })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        errors.push(format!(
            "{account}: status={} stdout=\"{}\" stderr=\"{}\"",
            output.status, stdout, stderr
        ));
    }

    Err(format!(
        "현재 사용자에게 폴더 권한을 부여하지 못했습니다: {} ({})",
        display_path(path),
        errors.join("; ")
    ))
}

#[cfg(not(windows))]
pub(crate) fn grant_current_user_full_control(
    _path: &Path,
    _recursive: bool,
) -> Result<(), String> {
    Ok(())
}

pub(crate) fn prepare_path_for_replacement(path: &Path) -> Result<(), String> {
    clear_readonly_recursively(path)?;
    grant_current_user_full_control(path, false)
}

pub(crate) fn repair_path_permissions_deep(path: &Path) -> Result<(), String> {
    clear_readonly_recursively(path)?;
    grant_current_user_full_control(path, true)
}

pub(crate) fn rename_with_permission_repair(
    from: &Path,
    to: &Path,
    action: &str,
) -> Result<(), String> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(first_error) => {
            let first_error_message = first_error.to_string();
            let first_error_kind = first_error.kind();

            if let Some(parent) = to.parent() {
                repair_path_permissions_deep(parent)?;
            }
            repair_path_permissions_deep(from)?;
            if to.exists() {
                repair_path_permissions_deep(to)?;
            }

            let mut last_error = None;
            for attempt in 1..=3 {
                match fs::rename(from, to) {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        last_error = Some(error);
                        if attempt < 3 {
                            std::thread::sleep(Duration::from_millis(250 * attempt));
                        }
                    }
                }
            }

            let second_error = last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "알 수 없는 오류".to_string());
            let locked_hint = if first_error_kind == io::ErrorKind::PermissionDenied {
                " 실행 중인 Minecraft/Java 또는 파일 탐색기/백신이 폴더를 사용 중일 수 있습니다. 게임과 관련 창을 모두 닫은 뒤 다시 시도해 주세요."
            } else {
                ""
            };

            Err(format!(
                "{action} (from: {}, to: {}) 권한 보정 후에도 실패했습니다.{locked_hint} first: {first_error_message}; second: {second_error}",
                display_path(from),
                display_path(to)
            ))
        }
    }
}

pub(crate) fn replace_directory_atomic(
    current_path: &Path,
    staged_path: &Path,
) -> Result<(), String> {
    if !staged_path.exists() {
        return Err(format!(
            "새 설치 폴더가 없습니다: {}",
            display_path(staged_path)
        ));
    }

    if let Some(parent) = current_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            io_error(
                "현재 설치 위치의 상위 폴더를 만들지 못했습니다",
                parent,
                error,
            )
        })?;
        prepare_path_for_replacement(parent)?;
    }

    prepare_path_for_replacement(staged_path)?;
    if current_path.exists() {
        prepare_path_for_replacement(current_path)?;
    }

    let backup_path = current_path.with_file_name(format!(
        "{}.backup-{}",
        current_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("current"),
        now_ms()
    ));
    let current_exists = current_path.exists();

    if current_exists {
        prepare_path_for_replacement(&backup_path)?;
        remove_path_if_exists(&backup_path)?;
        rename_with_permission_repair(
            current_path,
            &backup_path,
            "기존 폴더를 백업 위치로 이동하지 못했습니다",
        )?;
    }

    if let Err(error) = rename_with_permission_repair(
        staged_path,
        current_path,
        "새 폴더를 현재 설치 위치로 이동하지 못했습니다",
    ) {
        if current_path.exists() {
            let _ = prepare_path_for_replacement(current_path);
            let _ = remove_path_if_exists(current_path);
        }

        if backup_path.exists() {
            let _ = prepare_path_for_replacement(&backup_path);
            let _ = fs::rename(&backup_path, current_path);
        }

        return Err(error);
    }

    prepare_path_for_replacement(&backup_path)?;
    remove_path_if_exists(&backup_path)?;
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum ChecksumAlgorithm {
    Sha1,
    Sha256,
}

impl ChecksumAlgorithm {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA-1",
            Self::Sha256 => "SHA-256",
        }
    }

    pub(crate) fn hash_file(self, path: &Path) -> Result<String, String> {
        match self {
            Self::Sha1 => sha1_file(path),
            Self::Sha256 => sha256_file(path),
        }
    }
}

pub(crate) fn checksum_matches(
    path: &Path,
    expected_checksum: &str,
    algorithm: ChecksumAlgorithm,
) -> bool {
    algorithm
        .hash_file(path)
        .map(|actual_checksum| actual_checksum.eq_ignore_ascii_case(expected_checksum))
        .unwrap_or(false)
}

pub(crate) fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error("상위 폴더를 만들지 못했습니다", parent, error))?;
    }

    Ok(())
}

pub(crate) fn partial_download_path(target_file_path: &Path) -> PathBuf {
    let file_name = target_file_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    target_file_path.with_file_name(format!("{file_name}.partial"))
}

pub(crate) fn download_file_once(
    resource_url: &str,
    partial_file_path: &Path,
    maximum_size: u64,
) -> Result<(), String> {
    ensure_parent_dir(partial_file_path)?;
    let requested_url = validate_download_url(resource_url)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("다운로드 클라이언트를 만들지 못했습니다: {error}"))?;

    let mut existing_size = fs::metadata(partial_file_path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if existing_size > maximum_size {
        remove_path_if_exists(partial_file_path)?;
        existing_size = 0;
    }

    let send_request = |range_start: Option<u64>| {
        let mut request = client.get(requested_url.clone());
        if let Some(range_start) = range_start {
            request = request.header(reqwest::header::RANGE, format!("bytes={range_start}-"));
        }
        request
            .send()
            .map_err(|error| format!("다운로드 요청 실패 ({resource_url}): {error}"))
    };

    let mut response = send_request((existing_size > 0).then_some(existing_size))?;
    if existing_size > 0 && response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        remove_path_if_exists(partial_file_path)?;
        existing_size = 0;
        response = send_request(None)?;
    }

    let final_url = response.url().clone();
    validate_download_url(final_url.as_str()).map_err(|error| {
        format!("다운로드 redirect URL 검증 실패 ({resource_url} -> {final_url}): {error}")
    })?;

    let status = response.status();
    let append = existing_size > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
    if existing_size > 0 && !append {
        existing_size = 0;
    }

    if append {
        let expected_prefix = format!("bytes {existing_size}-");
        let content_range_matches = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with(&expected_prefix));
        if !content_range_matches {
            remove_path_if_exists(partial_file_path)?;
            return Err(format!(
                "다운로드 이어받기 응답 범위가 올바르지 않습니다 ({resource_url})"
            ));
        }
    }

    let response = response
        .error_for_status()
        .map_err(|error| format!("다운로드 응답 오류 ({resource_url}): {error}"))?;
    if response
        .content_length()
        .is_some_and(|content_length| existing_size.saturating_add(content_length) > maximum_size)
    {
        return Err(format!(
            "다운로드 응답 크기가 제한을 초과했습니다 ({resource_url}): {}/{} bytes",
            existing_size.saturating_add(response.content_length().unwrap_or_default()),
            maximum_size
        ));
    }

    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let mut partial_file = options.open(partial_file_path).map_err(|error| {
        io_error(
            "다운로드 임시 파일을 열지 못했습니다",
            partial_file_path,
            error,
        )
    })?;
    let remaining_size = maximum_size.saturating_sub(existing_size);
    let written_size = io::copy(
        &mut response.take(remaining_size.saturating_add(1)),
        &mut partial_file,
    )
    .map_err(|error| {
        contextual_error(
            &format!(
                "다운로드 저장 실패 (url: {resource_url}, partial: {})",
                display_path(partial_file_path)
            ),
            error,
        )
    })?;
    let total_size = existing_size.saturating_add(written_size);

    if total_size > maximum_size {
        remove_path_if_exists(partial_file_path)?;
        return Err(format!(
            "다운로드 데이터가 크기 제한을 초과했습니다 ({resource_url}): {total_size}/{maximum_size} bytes"
        ));
    }

    partial_file.sync_all().map_err(|error| {
        io_error(
            "다운로드 임시 파일을 동기화하지 못했습니다",
            partial_file_path,
            error,
        )
    })?;

    Ok(())
}

pub(crate) fn copy_or_download_file(
    resource_url: &str,
    target_file_path: &Path,
    expected_size: Option<u64>,
) -> Result<(), String> {
    ensure_parent_dir(target_file_path)?;
    let partial_file_path = partial_download_path(target_file_path);
    validate_download_url(resource_url)?;

    if let (Some(expected_size), Ok(metadata)) = (expected_size, fs::metadata(&partial_file_path)) {
        if metadata.is_file() && metadata.len() == expected_size {
            if target_file_path.exists() {
                remove_path_if_exists(target_file_path)?;
            }
            fs::rename(&partial_file_path, target_file_path).map_err(|error| {
                contextual_error(
                    &format!(
                        "완료된 다운로드 임시 파일을 적용하지 못했습니다 (from: {}, to: {})",
                        display_path(&partial_file_path),
                        display_path(target_file_path)
                    ),
                    error,
                )
            })?;
            return Ok(());
        }

        if metadata.len() > expected_size {
            remove_path_if_exists(&partial_file_path)?;
        }
    }

    let mut last_error = None;
    for attempt in 1..=3 {
        match download_file_once(
            resource_url,
            &partial_file_path,
            expected_size.unwrap_or(MAX_UNSIZED_DOWNLOAD_BYTES),
        ) {
            Ok(()) => {
                let downloaded_size = fs::metadata(&partial_file_path)
                    .map_err(|error| {
                        io_error(
                            "다운로드 임시 파일 정보를 읽지 못했습니다",
                            &partial_file_path,
                            error,
                        )
                    })?
                    .len();

                if expected_size.is_none_or(|expected| downloaded_size == expected) {
                    last_error = None;
                    break;
                }

                last_error = Some(format!(
                    "다운로드가 아직 완료되지 않았습니다 ({resource_url}): {downloaded_size}/{} bytes",
                    expected_size.unwrap_or_default()
                ));
            }
            Err(error) => {
                last_error = Some(error);
            }
        }

        if attempt < 3 {
            std::thread::sleep(Duration::from_millis(500 * attempt));
        }
    }

    if let Some(error) = last_error {
        return Err(format!(
            "{error} 이어받기 파일을 보존했습니다: {}",
            display_path(&partial_file_path)
        ));
    }

    if target_file_path.exists() {
        remove_path_if_exists(target_file_path)?;
    }

    fs::rename(&partial_file_path, target_file_path).map_err(|error| {
        contextual_error(
            &format!(
                "다운로드 임시 파일을 대상 위치로 이동하지 못했습니다 (from: {}, to: {})",
                display_path(&partial_file_path),
                display_path(target_file_path)
            ),
            error,
        )
    })?;
    Ok(())
}

pub(crate) fn ensure_cached_artifact(
    resource_url: &str,
    target_path: &Path,
    sha256: Option<&str>,
    size: Option<u64>,
    label: &str,
) -> Result<PathBuf, String> {
    ensure_cached_artifact_with_checksum(
        resource_url,
        target_path,
        sha256,
        ChecksumAlgorithm::Sha256,
        size,
        label,
    )
}

pub(crate) fn ensure_cached_artifact_with_checksum(
    resource_url: &str,
    target_path: &Path,
    checksum: Option<&str>,
    checksum_algorithm: ChecksumAlgorithm,
    size: Option<u64>,
    label: &str,
) -> Result<PathBuf, String> {
    if target_path.exists() {
        let metadata = fs::metadata(target_path)
            .map_err(|error| io_error("캐시 파일 정보를 읽지 못했습니다", target_path, error))?;

        if metadata.is_file()
            && size.is_none_or(|expected_size| metadata.len() == expected_size)
            && checksum
                .filter(|value| !value.trim().is_empty())
                .is_none_or(|expected_checksum| {
                    checksum_matches(target_path, expected_checksum, checksum_algorithm)
                })
        {
            return Ok(target_path.to_path_buf());
        }

        remove_path_if_exists(target_path)?;
    }

    copy_or_download_file(resource_url, target_path, size).map_err(|error| {
        format!(
            "{label} 다운로드/저장 실패 (url: {resource_url}, target: {}): {error}",
            display_path(target_path)
        )
    })?;
    let metadata = fs::metadata(target_path).map_err(|error| {
        io_error(
            "다운로드된 캐시 파일 정보를 읽지 못했습니다",
            target_path,
            error,
        )
    })?;

    if size.is_some_and(|expected_size| metadata.len() != expected_size) {
        remove_path_if_exists(target_path)?;
        return Err(format!("{label} size mismatch"));
    }

    if checksum
        .filter(|value| !value.trim().is_empty())
        .is_some_and(|expected_checksum| {
            !checksum_matches(target_path, expected_checksum, checksum_algorithm)
        })
    {
        remove_path_if_exists(target_path)?;
        return Err(format!(
            "{label} {} checksum mismatch",
            checksum_algorithm.label()
        ));
    }

    Ok(target_path.to_path_buf())
}

#[cfg(test)]
mod launch_download_resume_tests {
    use super::*;

    #[test]
    pub(crate) fn zip_budget_checks_depth_count_and_uncompressed_size() {
        let limits = ZipExtractionLimits {
            max_file_count: 1,
            max_entry_count: 2,
            max_path_depth: 2,
            max_uncompressed_size: 4,
        };
        let mut file_count = 0;
        let mut total_size = 0;

        account_zip_entry(
            Path::new("mods/a.jar"),
            "mods/a.jar",
            false,
            4,
            limits,
            &mut file_count,
            &mut total_size,
        )
        .expect("first bounded file should be accepted");
        assert!(account_zip_entry(
            Path::new("mods/deep/b.jar"),
            "mods/deep/b.jar",
            false,
            1,
            limits,
            &mut file_count,
            &mut total_size,
        )
        .is_err());
        assert_eq!(file_count, 1);
        assert_eq!(total_size, 4);

        assert!(account_zip_entry(
            Path::new("b.jar"),
            "b.jar",
            false,
            1,
            limits,
            &mut file_count,
            &mut total_size,
        )
        .is_err());

        let mut fresh_file_count = 0;
        let mut fresh_total_size = 0;
        assert!(account_zip_entry(
            Path::new("large.jar"),
            "large.jar",
            false,
            5,
            limits,
            &mut fresh_file_count,
            &mut fresh_total_size,
        )
        .is_err());
    }

    #[test]
    pub(crate) fn partial_download_path_preserves_original_file_name() {
        let target = Path::new("downloads").join("mods.zip");
        assert_eq!(
            partial_download_path(&target),
            Path::new("downloads").join("mods.zip.partial")
        );
    }

    #[test]
    pub(crate) fn completed_partial_file_is_promoted_without_redownload() {
        let directory = std::env::temp_dir().join(format!(
            "star-prison-download-test-{}-{}",
            std::process::id(),
            now_ms()
        ));
        fs::create_dir_all(&directory).expect("test directory should be created");
        let target = directory.join("artifact.bin");
        let partial = partial_download_path(&target);
        fs::write(&partial, b"done").expect("partial file should be written");

        copy_or_download_file(
            "https://github.com/nobabo/Star-Prison-Launcher",
            &target,
            Some(4),
        )
        .expect("completed partial should be promoted");

        assert_eq!(fs::read(&target).expect("target should exist"), b"done");
        assert!(!partial.exists());
        fs::remove_dir_all(directory).expect("test directory should be removed");
    }
}

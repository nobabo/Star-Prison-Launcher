fn client_option_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).ok(),
        Value::Null => None,
    }
}

fn apply_client_config_options(game_directory: &Path) -> Result<(), String> {
    let options_path = game_directory.join("options.txt");

    let client_config = load_client_config()?;
    let Some(configured_options) = client_config.get("options").and_then(Value::as_object) else {
        return Ok(());
    };

    if configured_options.is_empty() {
        return Ok(());
    }

    let mut lines = if options_path.exists() {
        fs::read_to_string(&options_path)
            .map_err(|error| io_error("Minecraft 옵션 파일을 읽지 못했습니다", &options_path, error))?
            .lines()
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    let mut existing_keys = HashSet::new();

    for line in &lines {
        if let Some((key, _)) = line.split_once(':') {
            existing_keys.insert(key.to_string());
        }
    }

    let mut changed = false;
    for (key, value) in configured_options {
        if !existing_keys.contains(key) {
            let Some(value) = client_option_value(value) else {
                continue;
            };
            lines.push(format!("{key}:{value}"));
            changed = true;
        }
    }

    if !changed {
        return Ok(());
    }

    fs::create_dir_all(game_directory)
        .map_err(|error| io_error("Minecraft 옵션 폴더를 만들지 못했습니다", game_directory, error))?;
    fs::write(&options_path, format!("{}\n", lines.join("\n")))
        .map_err(|error| io_error("Minecraft 옵션 파일을 쓰지 못했습니다", &options_path, error))
}

fn descriptor_string<'a>(descriptor: &'a Value, field: &str) -> Result<&'a str, String> {
    descriptor
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("배포 manifest 항목이 비어 있습니다: {field}"))
}

fn descriptor_size(descriptor: &Value, field: &str) -> Result<u64, String> {
    descriptor
        .get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("배포 manifest 크기 항목이 올바르지 않습니다: {field}"))
}

fn descriptor_optional_string<'a>(descriptor: &'a Value, field: &str) -> Option<&'a str> {
    descriptor
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn descriptor_optional_size(descriptor: &Value, field: &str) -> Option<u64> {
    descriptor
        .get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
}

fn descriptor_required_sha256<'a>(descriptor: &'a Value, label: &str) -> Result<&'a str, String> {
    descriptor_optional_string(descriptor, "sha256")
        .ok_or_else(|| format!("{label} SHA-256 checksum이 manifest에 없습니다. 다운로드를 중단합니다."))
}

fn find_extracted_runtime_root(staged_path: &Path, executable_path: &str) -> Option<PathBuf> {
    let direct_root = staged_path.to_path_buf();

    if direct_root.join(executable_path).exists() {
        return Some(direct_root);
    }

    let entries = fs::read_dir(staged_path).ok()?;

    for entry in entries.flatten() {
        let candidate_root = entry.path();

        if candidate_root.is_dir() && candidate_root.join(executable_path).exists() {
            return Some(candidate_root);
        }
    }

    None
}

fn verify_runtime_installation(data_directory: &Path, runtime: &Value) -> Result<PathBuf, String> {
    let executable_path = runtime_executable_path(data_directory, runtime);

    if executable_path.exists() {
        return Ok(executable_path);
    }

    Err(format!(
        "설치된 Java runtime 실행 파일을 찾지 못했습니다: {}",
        executable_path.to_string_lossy()
    ))
}

fn migrate_legacy_runtime_if_needed(data_directory: &Path, runtime: &Value) -> Result<(), String> {
    let current_path = runtime_current_path(data_directory, runtime);
    let legacy_path = legacy_runtime_current_path(data_directory, runtime);

    if current_path.exists() || !legacy_path.exists() {
        return Ok(());
    }

    if let Some(parent) = current_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error("Java runtime 폴더를 만들지 못했습니다", parent, error))?;
    }

    fs::rename(&legacy_path, &current_path).map_err(|error| {
        contextual_error(
            &format!(
                "레거시 Java runtime 경로를 새 경로로 이동하지 못했습니다 (from: {}, to: {})",
                display_path(&legacy_path),
                display_path(&current_path)
            ),
            error,
        )
    })?;
    let _ = fs::remove_dir(data_directory.join("runtime").join("current"));
    Ok(())
}

fn ensure_runtime_installed(
    app: &tauri::AppHandle,
    data_directory: &Path,
    runtime: &Value,
    force_install: bool,
) -> Result<PathBuf, String> {
    emit_launch_state(app, "실행 준비 파일 확인", 0.16);
    migrate_legacy_runtime_if_needed(data_directory, runtime)?;
    let download_path = runtime_download_path(data_directory, runtime);

    if !force_install {
        if let Ok(executable_path) = verify_runtime_installation(data_directory, runtime) {
            remove_file_if_exists(&download_path)?;
            remove_path_if_exists(&runtime_staged_root_path(data_directory))?;
            cleanup_empty_launcher_cache_dirs(data_directory);
            return Ok(executable_path);
        }
    }

    emit_launch_state(app, "실행 준비 파일 다운로드", 0.24);
    ensure_cached_artifact(
        descriptor_string(runtime, "url")?,
        &download_path,
        Some(descriptor_required_sha256(runtime, "Runtime archive")?),
        descriptor_optional_size(runtime, "size"),
        "Runtime archive",
    )?;

    emit_launch_state(app, "실행 준비 파일 압축 해제", 0.44);
    let staged_path = runtime_staged_path(data_directory, runtime);
    extract_zip_archive(
        &download_path,
        &staged_path,
        RUNTIME_ZIP_EXTRACTION_LIMITS,
    )?;

    let executable_path =
        runtime
            .get("executablePath")
            .and_then(Value::as_str)
            .unwrap_or(if cfg!(windows) {
                "bin/javaw.exe"
            } else {
                "bin/java"
            });
    let runtime_root = find_extracted_runtime_root(&staged_path, executable_path)
        .ok_or_else(|| "Runtime 압축 해제 후 Java 실행 파일을 찾지 못했습니다.".to_string())?;

    if runtime_root != staged_path {
        let normalized_staged_path = staged_path.with_file_name(format!(
            "{}-normalized",
            staged_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("runtime")
        ));
        remove_path_if_exists(&normalized_staged_path)?;
        fs::rename(&runtime_root, &normalized_staged_path).map_err(|error| {
            contextual_error(
                &format!(
                    "Java runtime 루트 폴더를 정규화 위치로 이동하지 못했습니다 (from: {}, to: {})",
                    display_path(&runtime_root),
                    display_path(&normalized_staged_path)
                ),
                error,
            )
        })?;
        remove_path_if_exists(&staged_path)?;
        fs::rename(&normalized_staged_path, &staged_path).map_err(|error| {
            contextual_error(
                &format!(
                    "Java runtime 정규화 폴더를 staged 위치로 이동하지 못했습니다 (from: {}, to: {})",
                    display_path(&normalized_staged_path),
                    display_path(&staged_path)
                ),
                error,
            )
        })?;
    }

    emit_launch_state(app, "실행 준비 파일 적용", 0.50);
    replace_directory_atomic(&runtime_current_path(data_directory, runtime), &staged_path)?;
    let executable_path = verify_runtime_installation(data_directory, runtime)?;
    remove_file_if_exists(&download_path)?;
    remove_path_if_exists(&runtime_staged_root_path(data_directory))?;
    cleanup_empty_launcher_cache_dirs(data_directory);
    Ok(executable_path)
}

fn read_remote_json_once(url: &str) -> Result<Value, String> {
    let requested_url = validate_download_url(url)?;
    let response = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|error| format!("원격 JSON HTTP client를 만들지 못했습니다 ({url}): {error}"))?
        .get(requested_url)
        .send()
        .map_err(|error| format!("원격 JSON 요청 실패 ({url}): {error}"))?;
    let final_url = response.url().clone();
    validate_download_url(final_url.as_str()).map_err(|error| {
        format!("원격 JSON redirect URL 검증 실패 ({url} -> {final_url}): {error}")
    })?;

    let response = response
        .error_for_status()
        .map_err(|error| format!("원격 JSON 응답 오류 ({url}): {error}"))?;
    const MAX_REMOTE_JSON_BYTES: u64 = 16 * 1024 * 1024;

    if response
        .content_length()
        .is_some_and(|content_length| content_length > MAX_REMOTE_JSON_BYTES)
    {
        return Err(format!("원격 JSON 응답이 너무 큽니다 ({url})"));
    }

    let mut bytes = Vec::new();
    response
        .take(MAX_REMOTE_JSON_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("원격 JSON 응답을 읽지 못했습니다 ({url}): {error}"))?;
    if bytes.len() as u64 > MAX_REMOTE_JSON_BYTES {
        return Err(format!("원격 JSON 응답이 크기 제한을 초과했습니다 ({url})"));
    }

    serde_json::from_slice(&bytes)
        .map_err(|error| format!("원격 JSON 파싱 실패 ({url}): {error}"))
}

fn validated_artifact_relative_path(value: &str) -> Result<PathBuf, String> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    let mut has_component = false;

    if path.is_absolute()
        || path.components().any(|component| {
            has_component = true;
            !matches!(component, std::path::Component::Normal(_))
        })
        || !has_component
    {
        return Err(format!("다운로드 artifact 경로가 올바르지 않습니다: {value}"));
    }

    Ok(path.to_path_buf())
}

fn read_remote_json(url: &str) -> Result<Value, String> {
    let mut last_error = None;

    for attempt in 1..=3 {
        match read_remote_json_once(url) {
            Ok(value) => return Ok(value),
            Err(error) => {
                last_error = Some(error);
                if attempt < 3 {
                    std::thread::sleep(Duration::from_millis(500 * attempt));
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| format!("원격 JSON 요청 실패 ({url})")))
}

fn maven_artifact_relative_path(name: &str) -> Result<String, String> {
    let parts = name.split(':').collect::<Vec<_>>();

    if parts.len() < 3 {
        return Err(format!("Maven 좌표가 올바르지 않습니다: {name}"));
    }

    let group_path = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();
    let file_name = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };

    Ok(format!("{group_path}/{artifact}/{version}/{file_name}"))
}

fn ensure_remote_artifact(
    url: &str,
    target_path: &Path,
    sha1: Option<&str>,
    size: Option<u64>,
    label: &str,
) -> Result<(), String> {
    ensure_cached_artifact_with_checksum(
        url,
        target_path,
        sha1,
        ChecksumAlgorithm::Sha1,
        size,
        label,
    )
    .map(|_| ())
}

// Keep this conservative to avoid CDN throttling and Windows disk/AV spikes.
const MINECRAFT_DOWNLOAD_CONCURRENCY: usize = 10;

struct RemoteArtifactTask {
    url: String,
    target_path: PathBuf,
    sha1: Option<String>,
    size: Option<u64>,
    label: String,
    error_context: String,
    progress_label: String,
}

fn remote_artifact_task(
    url: &str,
    target_path: PathBuf,
    sha1: Option<&str>,
    size: Option<u64>,
    label: &str,
    error_context: String,
) -> RemoteArtifactTask {
    RemoteArtifactTask {
        url: url.to_string(),
        target_path,
        sha1: sha1.map(str::to_string),
        size,
        label: label.to_string(),
        error_context,
        progress_label: label.to_string(),
    }
}

fn run_remote_artifact_task(task: &RemoteArtifactTask) -> Result<(), String> {
    ensure_remote_artifact(
        &task.url,
        &task.target_path,
        task.sha1.as_deref(),
        task.size,
        &task.label,
    )
    .map_err(|error| format!("{}: {error}", task.error_context))
}

fn ensure_remote_artifacts_parallel<F>(
    tasks: Vec<RemoteArtifactTask>,
    mut on_task_finished: F,
) -> Result<(), String>
where
    F: FnMut(usize, usize, &RemoteArtifactTask),
{
    if tasks.is_empty() {
        return Ok(());
    }

    let total = tasks.len();
    let mut completed = 0usize;
    let worker_count = total.min(MINECRAFT_DOWNLOAD_CONCURRENCY.max(1));
    let next_index = std::sync::atomic::AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let (tx, rx) = mpsc::channel::<(usize, Result<(), String>)>();
    let mut first_error = None;

    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);

        for _ in 0..worker_count {
            let tx = tx.clone();
            let tasks = &tasks;
            let next_index = &next_index;
            let failed = &failed;

            handles.push(scope.spawn(move || loop {
                if failed.load(Ordering::SeqCst) {
                    break;
                }

                let index = next_index.fetch_add(1, Ordering::SeqCst);
                if index >= tasks.len() {
                    break;
                }

                let result = run_remote_artifact_task(&tasks[index]);
                if result.is_err() {
                    failed.store(true, Ordering::SeqCst);
                }
                if tx.send((index, result)).is_err() {
                    break;
                }
            }));
        }

        drop(tx);

        for (index, result) in rx {
            completed += 1;
            let task = &tasks[index];
            on_task_finished(completed, total, task);

            if let Err(error) = result {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }

        for handle in handles {
            if handle.join().is_err() && first_error.is_none() {
                first_error = Some("다운로드 작업이 중단되었습니다.".to_string());
            }
        }
    });

    if let Some(error) = first_error {
        return Err(error);
    }

    Ok(())
}

fn push_unique_download_task(
    tasks: &mut Vec<RemoteArtifactTask>,
    seen_targets: &mut HashSet<PathBuf>,
    task: RemoteArtifactTask,
) {
    if seen_targets.insert(task.target_path.clone()) {
        tasks.push(task);
    }
}

fn library_allowed_on_windows(library: &Value) -> bool {
    let Some(rules) = library.get("rules").and_then(Value::as_array) else {
        return true;
    };
    let mut allowed = false;

    for rule in rules {
        let action = rule
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("allow");
        let os_matches = rule
            .get("os")
            .and_then(Value::as_object)
            .and_then(|os| os.get("name"))
            .and_then(Value::as_str)
            .is_none_or(|name| name == "windows");

        if os_matches {
            allowed = action != "disallow";
        }
    }

    allowed
}

fn flatten_argument_list(raw_arguments: Option<&Value>) -> Vec<String> {
    let mut args = Vec::new();

    for entry in raw_arguments
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        if let Some(value) = entry.as_str() {
            args.push(value.to_string());
            continue;
        }

        if !entry.is_object() || !library_allowed_on_windows(entry) {
            continue;
        }

        match entry.get("value") {
            Some(Value::String(value)) => args.push(value.to_string()),
            Some(Value::Array(values)) => {
                args.extend(values.iter().filter_map(Value::as_str).map(str::to_string));
            }
            _ => {}
        }
    }

    args
}

fn normalize_launch_args(args: Vec<String>, version_id: &str, asset_index_name: &str) -> Vec<String> {
    let mut normalized = Vec::new();
    let mut index = 0;

    while index < args.len() {
        let current = &args[index];
        let next = args.get(index + 1);
        let is_quick_play_arg = current.starts_with("--quickPlay");

        if current == "-cp"
            || current == "${classpath}"
            || current == "--demo"
            || (current == "--clientId" && next.is_some_and(|value| value == "${clientid}"))
            || (current == "--xuid" && next.is_some_and(|value| value == "${auth_xuid}"))
            || is_quick_play_arg
            || current.starts_with("${quickPlay")
        {
            index += if current == "-cp"
                || current == "--clientId"
                || current == "--xuid"
                || (is_quick_play_arg && next.is_some_and(|value| value.starts_with("${quickPlay")))
            {
                2
            } else {
                1
            };
            continue;
        }

        normalized.push(
            current
                .replace("${version_name}", version_id)
                .replace("${assets_root}", "${assets_directory}")
                .replace("${assets_index_name}", asset_index_name)
                .replace("${version_type}", "release"),
        );
        index += 1;
    }

    normalized
}

fn latest_fabric_loader_version(minecraft_version: &str) -> Result<String, String> {
    let url = format!("https://meta.fabricmc.net/v2/versions/loader/{minecraft_version}");
    let versions = read_remote_json(&url)?;

    versions
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.pointer("/loader/version"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("Fabric loader 버전을 찾지 못했습니다: {minecraft_version}"))
}

fn minecraft_version_metadata(minecraft_version: &str) -> Result<Value, String> {
    let manifest = read_remote_json("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")?;
    let version_url = manifest
        .get("versions")
        .and_then(Value::as_array)
        .and_then(|versions| {
            versions.iter().find_map(|version| {
                (version.get("id").and_then(Value::as_str) == Some(minecraft_version))
                    .then(|| version.get("url").and_then(Value::as_str))
                    .flatten()
            })
        })
        .ok_or_else(|| format!("Minecraft version manifest를 찾지 못했습니다: {minecraft_version}"))?;

    read_remote_json(version_url)
}

fn maven_library_download_task(
    libraries_root: &Path,
    library: &Value,
) -> Result<Option<(RemoteArtifactTask, String)>, String> {
    if !library_allowed_on_windows(library) {
        return Ok(None);
    }

    if let Some(artifact) = library.pointer("/downloads/artifact") {
        let artifact_path = descriptor_string(artifact, "path")?.to_string();
        let artifact_relative_path = validated_artifact_relative_path(&artifact_path)?;
        let target_path = libraries_root.join(&artifact_relative_path);
        let classpath_entry = format!("libraries/{}", artifact_relative_path.to_string_lossy().replace('\\', "/"));
        let task = remote_artifact_task(
            descriptor_string(artifact, "url")?,
            target_path,
            descriptor_optional_string(artifact, "sha1"),
            descriptor_optional_size(artifact, "size"),
            &artifact_path,
            format!("Minecraft 라이브러리 설치 실패 ({artifact_path})"),
        );
        return Ok(Some((task, classpath_entry)));
    }

    let name = descriptor_string(library, "name")?;
    let relative_path = maven_artifact_relative_path(name)?;
    let base_url = library
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("https://libraries.minecraft.net/");
    let url = format!("{}/{relative_path}", base_url.trim_end_matches('/'));
    let artifact_relative_path = validated_artifact_relative_path(&relative_path)?;
    let target_path = libraries_root.join(&artifact_relative_path);
    let classpath_entry = format!("libraries/{relative_path}");

    let task = remote_artifact_task(
        &url,
        target_path,
        descriptor_optional_string(library, "sha1"),
        descriptor_optional_size(library, "size"),
        &relative_path,
        format!("Minecraft 라이브러리 설치 실패 ({relative_path})"),
    );
    Ok(Some((task, classpath_entry)))
}

fn native_library_download_task(
    libraries_root: &Path,
    library: &Value,
) -> Result<Option<(RemoteArtifactTask, String, PathBuf)>, String> {
    if !library_allowed_on_windows(library) {
        return Ok(None);
    }

    let Some(native_key) = library
        .get("natives")
        .and_then(Value::as_object)
        .and_then(|natives| natives.get("windows"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let native_key = native_key.replace(
        "${arch}",
        if cfg!(target_pointer_width = "64") {
            "64"
        } else {
            "32"
        },
    );
    let Some(classifier) = library
        .pointer(&format!("/downloads/classifiers/{native_key}"))
    else {
        return Ok(None);
    };
    let artifact_path = descriptor_string(classifier, "path")?.to_string();
    let artifact_relative_path = validated_artifact_relative_path(&artifact_path)?;
    let target_path = libraries_root.join(&artifact_relative_path);

    let task = remote_artifact_task(
        descriptor_string(classifier, "url")?,
        target_path.clone(),
        descriptor_optional_string(classifier, "sha1"),
        descriptor_optional_size(classifier, "size"),
        &artifact_path,
        format!("Minecraft native 라이브러리 설치 실패 ({artifact_path})"),
    );
    Ok(Some((task, artifact_path, target_path)))
}

fn ensure_minecraft_libraries_parallel(
    libraries_root: &Path,
    natives_root: &Path,
    libraries: &[Value],
    classpath: &mut Vec<String>,
) -> Result<(), String> {
    let mut downloads = Vec::new();
    let mut seen_targets = HashSet::new();
    let mut classpath_entries = Vec::new();
    let mut native_archives = Vec::new();
    let mut seen_native_archives = HashSet::new();

    for library in libraries {
        if let Some((task, classpath_entry)) =
            maven_library_download_task(libraries_root, library)?
        {
            classpath_entries.push(classpath_entry);
            push_unique_download_task(&mut downloads, &mut seen_targets, task);
        }

        if let Some((task, artifact_path, target_path)) =
            native_library_download_task(libraries_root, library)?
        {
            if seen_native_archives.insert(target_path.clone()) {
                native_archives.push((artifact_path, target_path));
            }
            push_unique_download_task(&mut downloads, &mut seen_targets, task);
        }
    }

    ensure_remote_artifacts_parallel(downloads, |_, _, _| {})?;
    classpath.extend(classpath_entries);

    for (artifact_path, target_path) in native_archives {
        extract_zip_file_with_limits(
            &target_path,
            natives_root,
            NATIVE_ZIP_EXTRACTION_LIMITS,
        )
        .map_err(|error| {
            format!("Minecraft native 라이브러리 압축 해제 실패 ({artifact_path}): {error}")
        })?;
    }

    Ok(())
}

fn ensure_maven_libraries_parallel(
    libraries_root: &Path,
    libraries: &[Value],
    classpath: &mut Vec<String>,
) -> Result<(), String> {
    let mut downloads = Vec::new();
    let mut seen_targets = HashSet::new();
    let mut classpath_entries = Vec::new();

    for library in libraries {
        if let Some((task, classpath_entry)) =
            maven_library_download_task(libraries_root, library)?
        {
            classpath_entries.push(classpath_entry);
            push_unique_download_task(&mut downloads, &mut seen_targets, task);
        }
    }

    ensure_remote_artifacts_parallel(downloads, |_, _, _| {})?;
    classpath.extend(classpath_entries);
    Ok(())
}

fn minecraft_asset_file_name(asset_name: &str, hash: &str) -> String {
    Path::new(asset_name)
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .filter(|file_name| !file_name.trim().is_empty())
        .unwrap_or(hash)
        .to_string()
}

fn ensure_minecraft_assets(
    app: &tauri::AppHandle,
    assets_root: &Path,
    version_json: &Value,
) -> Result<String, String> {
    let asset_index = version_json
        .get("assetIndex")
        .ok_or_else(|| "Minecraft assetIndex가 없습니다.".to_string())?;
    let asset_index_id = descriptor_string(asset_index, "id")?.to_string();
    let asset_index_path = assets_root
        .join("indexes")
        .join(format!("{asset_index_id}.json"));

    ensure_remote_artifact(
        descriptor_string(asset_index, "url")?,
        &asset_index_path,
        descriptor_optional_string(asset_index, "sha1"),
        descriptor_optional_size(asset_index, "size"),
        "asset index",
    )
    .map_err(|error| format!("Minecraft asset index 설치 실패: {error}"))?;

    let asset_index_json = read_json_file(&asset_index_path)?;
    let objects = asset_index_json
        .get("objects")
        .and_then(Value::as_object)
        .ok_or_else(|| "Minecraft asset index objects가 없습니다.".to_string())?;

    let mut asset_downloads = Vec::new();
    let mut seen_targets = HashSet::new();

    for (asset_name, object) in objects {
        let Some(hash) = object.get("hash").and_then(Value::as_str) else {
            continue;
        };
        if hash.len() != 40 || !hash.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err(format!(
                "Minecraft asset hash가 올바르지 않습니다: {asset_name} ({hash})"
            ));
        }

        let object_path = assets_root.join("objects").join(&hash[0..2]).join(hash);
        let url = format!("https://resources.download.minecraft.net/{}/{}", &hash[0..2], hash);
        let mut task = remote_artifact_task(
            &url,
            object_path.clone(),
            Some(hash),
            object.get("size").and_then(Value::as_u64),
            hash,
            format!(
                "Minecraft 리소스 다운로드 실패 (asset: {asset_name}, hash: {hash}, target: {})",
                display_path(&object_path)
            ),
        );
        task.progress_label = minecraft_asset_file_name(asset_name, hash);
        push_unique_download_task(&mut asset_downloads, &mut seen_targets, task);
    }

    ensure_remote_artifacts_parallel(asset_downloads, |completed, total, task| {
        let progress = 0.76 + ((completed as f64 / total as f64) * 0.10);
        emit_launch_state(
            app,
            &format!("{} 다운로드중", task.progress_label),
            progress,
        );
    })?;

    Ok(asset_index_id)
}

fn ensure_fabric_remote_client_installed(
    app: &tauri::AppHandle,
    data_directory: &Path,
    server_manifest: &Value,
    channel: &Value,
    client_bundle: &Value,
    install_checkpoint: &mut Value,
) -> Result<PathBuf, String> {
    let minecraft_version = descriptor_string(server_manifest, "minecraftVersion")?;
    let loader_version = match client_bundle
        .get("loaderVersion")
        .and_then(Value::as_str)
        .unwrap_or("latest")
    {
        "latest" => latest_fabric_loader_version(minecraft_version)?,
        version => version.to_string(),
    };
    let fabric_profile_url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{minecraft_version}/{loader_version}/profile/json"
    );

    let staged_path = profile_staged_path(data_directory, channel);
    fs::create_dir_all(&staged_path)
        .map_err(|error| io_error("Fabric staged 폴더를 만들지 못했습니다", &staged_path, error))?;

    emit_launch_state(app, "Minecraft 설치 1/5: 실행 정보 확인", 0.58);
    let metadata_root = data_directory
        .join("downloads")
        .join("minecraft-metadata")
        .join(minecraft_version);
    let version_json_path = metadata_root.join("version.json");
    let version_json = match read_json_file(&version_json_path) {
        Ok(value)
            if value.get("id").and_then(Value::as_str) == Some(minecraft_version) =>
        {
            value
        }
        _ => {
            let value = minecraft_version_metadata(minecraft_version)?;
            write_json_state_atomic(&version_json_path, &value, "Minecraft version metadata")?;
            value
        }
    };
    let fabric_profile_path = metadata_root.join(format!("fabric-{loader_version}.json"));
    let fabric_profile = match read_json_file(&fabric_profile_path) {
        Ok(value)
            if value
                .get("mainClass")
                .and_then(Value::as_str)
                .is_some() =>
        {
            value
        }
        _ => {
            let value = read_remote_json(&fabric_profile_url)?;
            write_json_state_atomic(&fabric_profile_path, &value, "Fabric profile metadata")?;
            value
        }
    };
    mark_install_checkpoint(install_checkpoint, MINECRAFT_INSTALL_STAGES[0])?;

    let game_root = staged_path.clone();
    let libraries_root = game_root.join("libraries");
    let natives_root = game_root.join("natives");
    let versions_root = game_root.join("versions").join(minecraft_version);
    let assets_root = game_root.join("assets");
    fs::create_dir_all(&libraries_root)
        .map_err(|error| io_error("Minecraft libraries 폴더를 만들지 못했습니다", &libraries_root, error))?;
    fs::create_dir_all(&natives_root)
        .map_err(|error| io_error("Minecraft natives 폴더를 만들지 못했습니다", &natives_root, error))?;
    fs::create_dir_all(&versions_root)
        .map_err(|error| io_error("Minecraft versions 폴더를 만들지 못했습니다", &versions_root, error))?;
    apply_client_config_options(&game_root)?;

    emit_launch_state(app, "Minecraft 설치 2/5: 기본 라이브러리", 0.64);
    let mut classpath = Vec::new();
    ensure_minecraft_libraries_parallel(
        &libraries_root,
        &natives_root,
        version_json
            .get("libraries")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &mut classpath,
    )?;
    mark_install_checkpoint(install_checkpoint, MINECRAFT_INSTALL_STAGES[1])?;

    emit_launch_state(app, "Minecraft 설치 3/5: Fabric 라이브러리", 0.70);
    ensure_maven_libraries_parallel(
        &libraries_root,
        fabric_profile
            .get("libraries")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &mut classpath,
    )?;
    mark_install_checkpoint(install_checkpoint, MINECRAFT_INSTALL_STAGES[2])?;

    let client_download = version_json
        .pointer("/downloads/client")
        .ok_or_else(|| "Minecraft client 다운로드 정보가 없습니다.".to_string())?;
    let client_jar_relative = format!("versions/{minecraft_version}/{minecraft_version}.jar");
    emit_launch_state(app, "Minecraft 설치 4/5: 클라이언트 파일", 0.73);
    ensure_remote_artifact(
        descriptor_string(client_download, "url")?,
        &staged_path.join(&client_jar_relative),
        descriptor_optional_string(client_download, "sha1"),
        descriptor_optional_size(client_download, "size"),
        "Minecraft client jar",
    )
    .map_err(|error| format!("Minecraft client jar 설치 실패: {error}"))?;
    classpath.push(client_jar_relative);
    mark_install_checkpoint(install_checkpoint, MINECRAFT_INSTALL_STAGES[3])?;

    emit_launch_state(app, "Minecraft 설치 5/5: 리소스 파일", 0.76);
    let asset_index_name = ensure_minecraft_assets(app, &assets_root, &version_json)?;
    mark_install_checkpoint(install_checkpoint, MINECRAFT_INSTALL_STAGES[4])?;
    let version_id = fabric_profile
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or(minecraft_version);
    let jvm_args = normalize_launch_args(
        [
            flatten_argument_list(version_json.pointer("/arguments/jvm")),
            flatten_argument_list(fabric_profile.pointer("/arguments/jvm")),
        ]
        .concat(),
        version_id,
        &asset_index_name,
    );
    let game_args = normalize_launch_args(
        [
            flatten_argument_list(version_json.pointer("/arguments/game")),
            flatten_argument_list(fabric_profile.pointer("/arguments/game")),
        ]
        .concat(),
        version_id,
        &asset_index_name,
    );
    let launch_profile = json!({
        "schemaVersion": 1,
        "bundleId": client_bundle.get("bundleId").cloned().unwrap_or(Value::String("star-prison-client".to_string())),
        "bundleVersion": channel.get("version").cloned().unwrap_or(Value::String("1.0.0".to_string())),
        "minecraftVersion": minecraft_version,
        "loader": "fabric",
        "mainClass": fabric_profile.get("mainClass").cloned().unwrap_or(Value::String("net.fabricmc.loader.impl.launch.knot.KnotClient".to_string())),
        "classpath": classpath,
        "jvmArgs": jvm_args,
        "gameArgs": game_args,
        "gameDirectory": ".",
        "assetsDirectory": "assets",
        "nativesDirectory": "natives",
        "loggingConfigFile": "",
        "auth": {
            "required": true
        },
        "java": {
            "requiredMajorVersion": server_manifest.pointer("/java/requiredMajorVersion").cloned().unwrap_or(Value::from(21))
        },
        "metadata": {
            "source": "fabricRemote",
            "fabricLoaderVersion": loader_version,
            "fabricProfileUrl": fabric_profile_url
        }
    });
    let launch_profile_path = staged_path.join("launch-profile.json");
    write_normalized_launch_profile(&launch_profile_path, launch_profile)?;

    emit_launch_state(app, "게임 파일 적용", 0.88);
    replace_directory_atomic(
        &profile_root_path(),
        &staged_path,
    )?;
    remove_path_if_exists(&profile_staged_root_path(data_directory))?;
    cleanup_empty_launcher_cache_dirs(data_directory);
    verify_profile_installation()
}

fn verify_profile_installation() -> Result<PathBuf, String> {
    let current_path = profile_root_path();
    let launch_profile_path = current_path.join("launch-profile.json");

    if !current_path.exists() {
        return Err("클라이언트 설치 폴더가 없습니다.".to_string());
    }

    if !launch_profile_path.exists() {
        return Err("launch-profile.json 파일이 없습니다.".to_string());
    }

    ensure_required_launch_profile_fields(&launch_profile_path)?;

    Ok(current_path)
}

fn write_launch_profile_from_channel(staged_path: &Path, channel: &Value) -> Result<(), String> {
    let launch_profile_path = staged_path.join("launch-profile.json");

    if launch_profile_path.exists() {
        return Ok(());
    }

    let Some(launch_profile) = channel
        .get("launchProfile")
        .filter(|value| value.is_object())
    else {
        return Ok(());
    };

    write_normalized_launch_profile(&launch_profile_path, launch_profile.clone())
}

fn ensure_required_launch_profile_fields(launch_profile_path: &Path) -> Result<(), String> {
    let launch_profile = read_json_file(launch_profile_path)?;

    for field in [
        "bundleId",
        "bundleVersion",
        "minecraftVersion",
        "loader",
        "mainClass",
        "gameDirectory",
        "assetsDirectory",
        "nativesDirectory",
    ] {
        descriptor_string(&launch_profile, field)?;
    }

    if value_string_vec(launch_profile.get("classpath")).is_empty() {
        return Err("launch-profile.json classpath 항목이 비어 있습니다.".to_string());
    }

    if value_string_vec(launch_profile.get("gameArgs"))
        .iter()
        .any(|arg| arg == "--demo" || arg.starts_with("--quickPlay") || arg.starts_with("${quickPlay"))
    {
        return Err("launch-profile.json에 레거시 quick play 또는 demo 인자가 남아 있습니다.".to_string());
    }

    Ok(())
}

fn normalize_profile_relative_path(value: &str) -> String {
    let normalized = value.replace('\\', "/");

    if normalized == "game/.minecraft" {
        return ".".to_string();
    }

    normalized
        .strip_prefix("game/.minecraft/")
        .unwrap_or(&normalized)
        .to_string()
}

fn normalize_profile_launch_profile(launch_profile: &mut Value) {
    let Some(profile) = launch_profile.as_object_mut() else {
        return;
    };

    for field in ["gameDirectory", "assetsDirectory", "nativesDirectory"] {
        if let Some(Value::String(value)) = profile.get_mut(field) {
            *value = normalize_profile_relative_path(value);
        }
    }

    for field in ["classpath", "classPath"] {
        let Some(Value::Array(entries)) = profile.get_mut(field) else {
            continue;
        };

        for entry in entries {
            if let Value::String(value) = entry {
                *value = normalize_profile_relative_path(value);
            }
        }
    }
}

fn write_normalized_launch_profile(path: &Path, mut launch_profile: Value) -> Result<(), String> {
    normalize_profile_launch_profile(&mut launch_profile);
    ensure_parent_dir(path)?;
    fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&launch_profile).map_err(|error| error.to_string())?
        ),
    )
    .map_err(|error| io_error("launch-profile.json 파일을 쓰지 못했습니다", path, error))
}

fn migrate_legacy_client_profile_if_needed(
    data_directory: &Path,
    server_manifest: &Value,
) -> Result<(), String> {
    let profile_path = profile_root_path();
    let legacy_data_profile_path = legacy_data_profile_path(data_directory);
    let legacy_current_path = legacy_client_current_path(data_directory, server_manifest);
    let legacy_profile_path = legacy_current_path.join("game").join(".minecraft");
    let legacy_launch_profile_path = legacy_current_path.join("launch-profile.json");

    if !profile_path.exists() && legacy_data_profile_path.exists() {
        if let Some(parent) = profile_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("Minecraft 프로필 상위 폴더를 만들지 못했습니다", parent, error))?;
        }

        fs::rename(&legacy_data_profile_path, &profile_path).map_err(|error| {
            contextual_error(
                &format!(
                    "data 안의 Minecraft 프로필을 루트 경로로 이동하지 못했습니다 (from: {}, to: {})",
                    display_path(&legacy_data_profile_path),
                    display_path(&profile_path)
                ),
                error,
            )
        })?;
    }

    if !profile_path.exists() && legacy_profile_path.exists() {
        if let Some(parent) = profile_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("Minecraft 프로필 상위 폴더를 만들지 못했습니다", parent, error))?;
        }

        fs::rename(&legacy_profile_path, &profile_path).map_err(|error| {
            contextual_error(
                &format!(
                    "레거시 Minecraft 프로필을 새 경로로 이동하지 못했습니다 (from: {}, to: {})",
                    display_path(&legacy_profile_path),
                    display_path(&profile_path)
                ),
                error,
            )
        })?;
    }

    let launch_profile_path = profile_path.join("launch-profile.json");

    if profile_path.exists() && !launch_profile_path.exists() && legacy_launch_profile_path.exists() {
        let legacy_launch_profile = read_json_file(&legacy_launch_profile_path)?;
        write_normalized_launch_profile(&launch_profile_path, legacy_launch_profile)?;
    }

    if profile_path.exists() {
        let _ = remove_path_if_exists(&legacy_current_path);
    }

    let _ = remove_path_if_exists(&legacy_client_staged_root_path(
        data_directory,
        server_manifest,
    ));
    Ok(())
}

fn ensure_client_bundle_installed(
    app: &tauri::AppHandle,
    data_directory: &Path,
    server_manifest: &Value,
    channel: &Value,
    force_install: bool,
    install_checkpoint: &mut Value,
) -> Result<PathBuf, String> {
    emit_launch_state(app, "게임 파일 확인", 0.54);
    migrate_legacy_client_profile_if_needed(data_directory, server_manifest)?;

    if !force_install {
        if let Ok(current_path) = verify_profile_installation() {
            for stage in MINECRAFT_INSTALL_STAGES {
                mark_install_checkpoint(install_checkpoint, stage)?;
            }
            remove_path_if_exists(&profile_staged_root_path(data_directory))?;
            cleanup_empty_launcher_cache_dirs(data_directory);
            return Ok(current_path);
        }
    }

    let client_bundle = channel
        .get("clientBundle")
        .ok_or_else(|| "배포 manifest에 clientBundle 정보가 없습니다.".to_string())?;

    if client_bundle.get("sourceType").and_then(Value::as_str) == Some("fabricRemote") {
        return ensure_fabric_remote_client_installed(
            app,
            data_directory,
            server_manifest,
            channel,
            client_bundle,
            install_checkpoint,
        );
    }

    let download_path = client_download_path(data_directory, channel);

    emit_launch_state(app, "게임 파일 다운로드", 0.60);
    ensure_cached_artifact(
        descriptor_string(client_bundle, "url")?,
        &download_path,
        Some(descriptor_string(client_bundle, "sha256")?),
        Some(descriptor_size(client_bundle, "size")?),
        "Client bundle",
    )?;

    emit_launch_state(app, "게임 파일 압축 해제", 0.80);
    let staged_path = profile_staged_path(data_directory, channel);
    extract_zip_archive(
        &download_path,
        &staged_path,
        CLIENT_ZIP_EXTRACTION_LIMITS,
    )?;
    write_launch_profile_from_channel(&staged_path, channel)?;

    let launch_profile_path = staged_path.join("launch-profile.json");

    if !launch_profile_path.exists() {
        remove_path_if_exists(&staged_path)?;
        return Err("클라이언트 번들 안에 launch-profile.json이 없습니다.".to_string());
    }

    if let Err(error) = ensure_required_launch_profile_fields(&launch_profile_path) {
        remove_path_if_exists(&staged_path)?;
        return Err(error);
    }

    emit_launch_state(app, "게임 파일 적용", 0.88);
    replace_directory_atomic(
        &profile_root_path(),
        &staged_path,
    )?;
    remove_path_if_exists(&profile_staged_root_path(data_directory))?;
    cleanup_empty_launcher_cache_dirs(data_directory);
    verify_profile_installation()
}

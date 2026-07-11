fn modpack_manifest_cache_path(data_directory: &Path, descriptor: &Value) -> PathBuf {
    data_directory
        .join("downloads")
        .join("modpack-manifests")
        .join(format!(
            "{}-{}.json",
            descriptor
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("modpack"),
            descriptor
                .get("sha256")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ))
}

fn modpack_file_cache_path(data_directory: &Path, file_entry: &Value) -> PathBuf {
    data_directory.join("downloads").join("modpack-files").join(
        file_entry
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
    )
}

fn release_archive_cache_path(
    data_directory: &Path,
    descriptor: &Value,
    fallback_name: &str,
) -> Result<PathBuf, String> {
    let file_name = descriptor
        .get("fileName")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback_name);
    let file_name_path = Path::new(file_name);
    let mut components = file_name_path.components();

    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(format!("release archive 파일 이름이 올바르지 않습니다: {file_name}"));
    }

    Ok(data_directory
        .join("downloads")
        .join("release-archives")
        .join(file_name))
}

fn launcher_release_archive_state_path(archive_key: &str) -> PathBuf {
    storage_root_path()
        .join("config")
        .join(format!("{archive_key}.json"))
}

fn legacy_release_archive_state_path(data_directory: &Path, archive_key: &str) -> PathBuf {
    data_directory
        .join("state")
        .join("release-archives")
        .join(format!("{archive_key}.json"))
}

fn migrate_legacy_release_archive_state(data_directory: &Path, archive_key: &str) -> Result<(), String> {
    let path = launcher_release_archive_state_path(archive_key);
    let legacy_path = legacy_release_archive_state_path(data_directory, archive_key);

    if path.exists() || !legacy_path.exists() {
        return Ok(());
    }

    ensure_parent_dir(&path)?;
    fs::rename(&legacy_path, &path).map_err(|error| {
        contextual_error(
            &format!(
                "release archive 상태 파일을 config 폴더로 이동하지 못했습니다 (from: {}, to: {})",
                display_path(&legacy_path),
                display_path(&path)
            ),
            error,
        )
    })?;

    if let Some(parent) = legacy_path.parent() {
        let _ = fs::remove_dir(parent);
    }
    let _ = fs::remove_dir(data_directory.join("state"));
    Ok(())
}

fn release_archive_signature(descriptor: &Value) -> Value {
    json!({
        "layoutVersion": 2,
        "version": descriptor.get("version").cloned().unwrap_or(Value::Null),
        "fileName": descriptor.get("fileName").cloned().unwrap_or(Value::Null),
        "url": descriptor.get("url").cloned().unwrap_or(Value::Null),
        "sha256": descriptor.get("sha256").cloned().unwrap_or(Value::Null),
        "size": descriptor.get("size").cloned().unwrap_or(Value::Null)
    })
}

fn validate_modpack_manifest(
    modpack_manifest: &Value,
    require_file_urls: bool,
) -> Result<(), String> {
    descriptor_size(modpack_manifest, "schemaVersion")?;
    descriptor_string(modpack_manifest, "id")?;
    descriptor_string(modpack_manifest, "version")?;
    descriptor_string(modpack_manifest, "minecraftVersion")?;
    descriptor_string(modpack_manifest, "loader")?;

    let files = modpack_manifest
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "modpack manifest files 항목이 배열이 아닙니다.".to_string())?;

    for (index, file_entry) in files.iter().enumerate() {
        descriptor_string(file_entry, "path")
            .map_err(|error| format!("files[{index}]: {error}"))?;

        let kind = descriptor_string(file_entry, "kind")
            .map_err(|error| format!("files[{index}]: {error}"))?;

        if !matches!(
            kind,
            "mod" | "config" | "config-seed" | "shaderpack" | "resourcepack"
        ) {
            return Err(format!("지원하지 않는 modpack 파일 kind입니다: {kind}"));
        }

        if require_file_urls {
            descriptor_string(file_entry, "url")
                .map_err(|error| format!("files[{index}]: {error}"))?;
        }

        let require_integrity = require_file_urls || !matches!(kind, "mod" | "shaderpack");
        if require_integrity {
            descriptor_string(file_entry, "sha256")
                .map_err(|error| format!("files[{index}]: {error}"))?;
            descriptor_size(file_entry, "size")
                .map_err(|error| format!("files[{index}]: {error}"))?;
        }

        if file_entry
            .get("required")
            .and_then(Value::as_bool)
            .is_none()
        {
            return Err(format!("files[{index}].required는 boolean이어야 합니다."));
        }
    }

    Ok(())
}

fn assert_compatible_modpack(
    server_manifest: &Value,
    launch_plan: &Value,
    descriptor: &Value,
    modpack_manifest: &Value,
) -> Result<(), String> {
    if modpack_manifest.get("version").and_then(Value::as_str)
        != descriptor.get("version").and_then(Value::as_str)
    {
        return Err("modpack manifest version이 distribution descriptor와 다릅니다.".to_string());
    }

    if modpack_manifest
        .get("minecraftVersion")
        .and_then(Value::as_str)
        != server_manifest
            .get("minecraftVersion")
            .and_then(Value::as_str)
    {
        return Err("modpack manifest Minecraft 버전이 server manifest와 다릅니다.".to_string());
    }

    if let Some(launch_minecraft_version) =
        launch_plan.get("minecraftVersion").and_then(Value::as_str)
    {
        if modpack_manifest
            .get("minecraftVersion")
            .and_then(Value::as_str)
            != Some(launch_minecraft_version)
        {
            return Err("modpack manifest Minecraft 버전이 launch-profile과 다릅니다.".to_string());
        }
    }

    if let Some(loader) = launch_plan.get("loader").and_then(Value::as_str) {
        if modpack_manifest.get("loader").and_then(Value::as_str) != Some(loader) {
            return Err("modpack manifest loader가 launch-profile과 다릅니다.".to_string());
        }
    }

    Ok(())
}

fn release_archive_target_root(bundle_root: &Path, archive_key: &str) -> Result<PathBuf, String> {
    match archive_key {
        "mods" => Ok(managed_file_allowed_root(bundle_root, "mod")),
        "config" => Ok(managed_file_allowed_root(bundle_root, "config")),
        "shaderpacks" => Ok(managed_file_allowed_root(bundle_root, "shaderpack")),
        _ => Err(format!("지원하지 않는 아카이브입니다: {archive_key}")),
    }
}

fn release_archive_root_prefixes(archive_key: &str) -> Result<&'static [&'static str], String> {
    match archive_key {
        "mods" => Ok(&["mods"]),
        "config" => Ok(&["config"]),
        "shaderpacks" => Ok(&["shaderpacks", "shaders"]),
        _ => Err(format!("지원하지 않는 아카이브입니다: {archive_key}")),
    }
}

fn release_archive_extraction_limits(archive_key: &str) -> Result<ZipExtractionLimits, String> {
    match archive_key {
        "mods" => Ok(ZipExtractionLimits {
            max_file_count: 80,
            max_entry_count: 96,
            max_uncompressed_size: 100 * 1024 * 1024,
        }),
        "config" => Ok(ZipExtractionLimits {
            max_file_count: 160,
            max_entry_count: 192,
            max_uncompressed_size: 20 * 1024 * 1024,
        }),
        "shaderpacks" => Ok(ZipExtractionLimits {
            max_file_count: 20,
            max_entry_count: 24,
            max_uncompressed_size: 30 * 1024 * 1024,
        }),
        _ => Err(format!("지원하지 않는 아카이브입니다: {archive_key}")),
    }
}

fn release_archive_already_installed(
    data_directory: &Path,
    target_root: &Path,
    archive_key: &str,
    descriptor: &Value,
) -> bool {
    let _ = migrate_legacy_release_archive_state(data_directory, archive_key);

    if !target_root.exists() {
        return false;
    }

    read_json_file(&launcher_release_archive_state_path(archive_key))
        .ok()
        .and_then(|state| state.get("descriptor").cloned())
        .is_some_and(|signature| signature == release_archive_signature(descriptor))
}

fn save_release_archive_state(
    archive_key: &str,
    descriptor: &Value,
    target_root: &Path,
) -> Result<(), String> {
    let path = launcher_release_archive_state_path(archive_key);
    ensure_parent_dir(&path)?;
    let state = json!({
        "schemaVersion": 1,
        "archive": archive_key,
        "descriptor": release_archive_signature(descriptor),
        "targetRoot": display_path(target_root),
        "installedAt": now_ms().to_string()
    });

    fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&state).map_err(|error| error.to_string())?
        ),
    )
    .map_err(|error| io_error("release archive 설치 상태 파일을 쓰지 못했습니다", &path, error))
}

struct ReleaseArchiveInstallSpec<'a> {
    archive_key: &'a str,
    descriptor: Option<&'a Value>,
    fallback_name: &'a str,
    download_label: &'a str,
    install_label: &'a str,
    progress: f64,
    force_install: bool,
}

fn ensure_release_archive_installed(
    app: &tauri::AppHandle,
    data_directory: &Path,
    bundle_root: &Path,
    spec: ReleaseArchiveInstallSpec<'_>,
) -> Result<bool, String> {
    let Some(descriptor) = spec.descriptor.filter(|value| !value.is_null()) else {
        return Ok(false);
    };
    let target_root = release_archive_target_root(bundle_root, spec.archive_key)?;
    let archive_path = release_archive_cache_path(data_directory, descriptor, spec.fallback_name)?;

    if !spec.force_install
        && release_archive_already_installed(data_directory, &target_root, spec.archive_key, descriptor)
    {
        remove_file_if_exists(&archive_path)?;
        cleanup_empty_launcher_cache_dirs(data_directory);
        return Ok(false);
    }

    let url = descriptor_string(descriptor, "url")?;
    let file_name = descriptor
        .get("fileName")
        .and_then(Value::as_str)
        .unwrap_or(spec.fallback_name);

    emit_launch_state(app, &format!("{file_name} 다운로드중"), spec.progress);
    ensure_cached_artifact(
        url,
        &archive_path,
        Some(descriptor_required_sha256(descriptor, spec.download_label)?),
        descriptor_optional_size(descriptor, "size"),
        spec.download_label,
    )?;

    emit_launch_state(app, spec.install_label, spec.progress + 0.01);
    let extraction_limits = release_archive_extraction_limits(spec.archive_key)?;
    let root_prefixes = release_archive_root_prefixes(spec.archive_key)?;
    let preserve_existing_files = matches!(spec.archive_key, "config" | "shaderpacks");

    let extraction_result = if preserve_existing_files {
        extract_zip_archive_preserving_existing_files_with_limits_and_progress(
            app,
            &archive_path,
            &target_root,
            extraction_limits,
            root_prefixes,
            spec.progress + 0.01,
            spec.progress + 0.03,
        )
    } else {
        extract_zip_archive_with_limits_and_progress(
            app,
            &archive_path,
            &target_root,
            extraction_limits,
            root_prefixes,
            spec.progress + 0.01,
            spec.progress + 0.03,
        )
    };

    if let Err(error) = extraction_result {
        let _ = remove_path_if_exists(&archive_path);
        return Err(format!(
            "{} 실패: {error} 손상되었거나 안전 제한을 초과한 압축 파일을 삭제했습니다.",
            spec.install_label
        ));
    }

    save_release_archive_state(spec.archive_key, descriptor, &target_root)?;
    remove_file_if_exists(&archive_path)?;
    cleanup_empty_launcher_cache_dirs(data_directory);

    Ok(true)
}

fn ensure_release_archives_synchronized(
    app: &tauri::AppHandle,
    data_directory: &Path,
    bundle_root: &Path,
    archives: Option<&Value>,
    force_archive_keys: &[&str],
) -> Result<usize, String> {
    let Some(archives) = archives.and_then(Value::as_object) else {
        return Ok(0);
    };
    let mut changed_count = 0;

    if ensure_release_archive_installed(
        app,
        data_directory,
        bundle_root,
        ReleaseArchiveInstallSpec {
            archive_key: "mods",
            descriptor: archives.get("mods"),
            fallback_name: "Mods.zip",
            download_label: "모드 아카이브",
            install_label: "모드 파일 설치",
            progress: 0.89,
            force_install: force_archive_keys.contains(&"mods"),
        },
    )? {
        changed_count += 1;
    }

    if ensure_release_archive_installed(
        app,
        data_directory,
        bundle_root,
        ReleaseArchiveInstallSpec {
            archive_key: "config",
            descriptor: archives.get("config"),
            fallback_name: "config.zip",
            download_label: "설정 아카이브",
            install_label: "모드 설정 설치",
            progress: 0.90,
            force_install: force_archive_keys.contains(&"config"),
        },
    )? {
        changed_count += 1;
    }

    if ensure_release_archive_installed(
        app,
        data_directory,
        bundle_root,
        ReleaseArchiveInstallSpec {
            archive_key: "shaderpacks",
            descriptor: archives.get("shaderpacks"),
            fallback_name: "shaderpacks.zip",
            download_label: "쉐이더 아카이브",
            install_label: "쉐이더 파일 설치",
            progress: 0.91,
            force_install: force_archive_keys.contains(&"shaderpacks"),
        },
    )? {
        changed_count += 1;
    }

    Ok(changed_count)
}

fn path_is_inside(root: &Path, target: &Path) -> bool {
    let Ok(relative) = target.strip_prefix(root) else {
        return false;
    };

    !relative
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn managed_file_allowed_root(bundle_root: &Path, kind: &str) -> PathBuf {
    match kind {
        "mod" => bundle_root.join("mods"),
        "shaderpack" => bundle_root.join("shaderpacks"),
        "resourcepack" => bundle_root.join("resourcepacks"),
        "config" | "config-seed" => bundle_root.join("config"),
        _ => bundle_root.to_path_buf(),
    }
}

fn resolve_managed_target_path(bundle_root: &Path, file_entry: &Value) -> Result<PathBuf, String> {
    let relative_path = descriptor_string(file_entry, "path")?;
    let kind = descriptor_string(file_entry, "kind")?;
    let relative = Path::new(relative_path);

    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!("관리 파일 경로가 상대 경로가 아닙니다: {relative_path}"));
    }

    let target_path = bundle_root.join(relative);

    if !path_is_inside(bundle_root, &target_path) {
        return Err(format!(
            "관리 파일 경로가 번들 루트 밖으로 나갑니다: {relative_path}"
        ));
    }

    let allowed_root = managed_file_allowed_root(bundle_root, kind);

    if !path_is_inside(&allowed_root, &target_path) {
        return Err(format!(
            "관리 파일 경로가 허용된 {kind} 루트 밖입니다: {relative_path}"
        ));
    }

    Ok(target_path)
}

fn copy_cache_file_to_target(cache_path: &Path, target_path: &Path) -> Result<(), String> {
    ensure_parent_dir(target_path)?;
    let temp_path = target_path.with_extension(format!("tmp-{}", now_ms()));
    remove_path_if_exists(&temp_path)?;
    fs::copy(cache_path, &temp_path).map_err(|error| error.to_string())?;
    remove_path_if_exists(target_path)?;
    fs::rename(temp_path, target_path).map_err(|error| error.to_string())
}

fn ensure_managed_file_installed(
    data_directory: &Path,
    bundle_root: &Path,
    file_entry: &Value,
    seed_only: bool,
) -> Result<bool, String> {
    let target_path = resolve_managed_target_path(bundle_root, file_entry)?;

    if seed_only && target_path.exists() {
        return Ok(false);
    }

    let expected_size = descriptor_size(file_entry, "size")?;
    let expected_sha256 = descriptor_string(file_entry, "sha256")?;

    if target_path.exists() {
        let metadata = fs::metadata(&target_path).map_err(|error| error.to_string())?;

        if metadata.is_file()
            && metadata.len() == expected_size
            && checksum_matches(&target_path, expected_sha256, ChecksumAlgorithm::Sha256)
        {
            return Ok(false);
        }
    }

    let cache_path = modpack_file_cache_path(data_directory, file_entry);
    ensure_cached_artifact(
        descriptor_string(file_entry, "url")?,
        &cache_path,
        Some(expected_sha256),
        Some(expected_size),
        descriptor_string(file_entry, "path")?,
    )?;
    copy_cache_file_to_target(&cache_path, &target_path)?;

    Ok(true)
}

fn reconcile_mods_directory(bundle_root: &Path, modpack_manifest: &Value) -> Result<(), String> {
    let mods_root = managed_file_allowed_root(bundle_root, "mod");
    let mut expected_paths = HashSet::new();

    for file_entry in modpack_manifest
        .get("files")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        if file_entry.get("kind").and_then(Value::as_str) == Some("mod") {
            expected_paths.insert(resolve_managed_target_path(bundle_root, file_entry)?);
        }
    }

    fs::create_dir_all(&mods_root).map_err(|error| error.to_string())?;
    prune_unexpected_files(&mods_root, &mods_root, &expected_paths)
}

fn prune_unexpected_files(
    root: &Path,
    directory: &Path,
    expected_paths: &HashSet<PathBuf>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;

        if file_type.is_symlink() {
            if expected_paths.contains(&path) {
                return Err(format!(
                    "관리 대상 mods 경로에는 심볼릭 링크나 junction을 사용할 수 없습니다: {}",
                    display_path(&path)
                ));
            }
            fs::remove_file(&path).map_err(|error| error.to_string())?;
        } else if file_type.is_dir() {
            prune_unexpected_files(root, &path, expected_paths)?;

            if path != root
                && fs::read_dir(&path)
                    .map_err(|error| error.to_string())?
                    .next()
                    .is_none()
            {
                fs::remove_dir(&path).map_err(|error| error.to_string())?;
            }
        } else if file_type.is_file() && !expected_paths.contains(&path) {
            fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

fn ensure_modpack_synchronized(
    app: &tauri::AppHandle,
    data_directory: &Path,
    bundle_root: &Path,
    server_manifest: &Value,
    launch_plan: Option<&Value>,
    descriptor: Option<&Value>,
) -> Result<(Option<Value>, Option<PathBuf>, usize), String> {
    let Some(descriptor) = descriptor.filter(|value| !value.is_null()) else {
        return Ok((None, None, 0));
    };
    let launch_plan = launch_plan
        .ok_or_else(|| "modpack 동기화에는 launch-profile.json이 필요합니다.".to_string())?;
    let embedded_manifest = descriptor
        .get("files")
        .and_then(Value::as_array)
        .is_some();

    let (modpack_manifest, manifest_path) = if embedded_manifest {
        (descriptor.clone(), None)
    } else {
        let manifest_cache_path = modpack_manifest_cache_path(data_directory, descriptor);
        emit_launch_state(app, "모드 구성 확인", 0.89);
        ensure_cached_artifact(
            descriptor_string(descriptor, "url")?,
            &manifest_cache_path,
            Some(descriptor_string(descriptor, "sha256")?),
            Some(descriptor_size(descriptor, "size")?),
            "Modpack manifest",
        )?;

        (read_json_file(&manifest_cache_path)?, Some(manifest_cache_path))
    };

    validate_modpack_manifest(&modpack_manifest, !embedded_manifest)?;
    assert_compatible_modpack(server_manifest, launch_plan, descriptor, &modpack_manifest)?;

    let mut changed_count = 0;
    if !embedded_manifest {
        let files = modpack_manifest
            .get("files")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        emit_launch_state(app, "모드 파일 다운로드", 0.90);
        for file_entry in files {
            let kind = descriptor_string(file_entry, "kind")?;
            let seed_only = matches!(kind, "config" | "config-seed");

            if ensure_managed_file_installed(data_directory, bundle_root, file_entry, seed_only)? {
                changed_count += 1;
            }
        }
    }

    emit_launch_state(app, "모드 구성 정리", 0.93);
    reconcile_mods_directory(bundle_root, &modpack_manifest)?;

    Ok((Some(modpack_manifest), manifest_path, changed_count))
}

use crate::*;

pub(crate) fn minimize_launcher_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }
}

pub(crate) fn focus_launcher_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }

    emit_window_state(app);
}

pub(crate) fn selected_distribution_channel<'a>(
    distribution: &'a Value,
    user_config: &Value,
    app_config: &Value,
) -> Result<(&'a str, &'a Value), String> {
    let allow_prerelease = user_config
        .pointer("/settings/allowPrerelease")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || app_config
            .get("allowPrerelease")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let channel_name = if allow_prerelease {
        "prerelease"
    } else {
        "stable"
    };

    let (selected_name, channel) = distribution
        .pointer(&format!("/channels/{channel_name}"))
        .filter(|channel| !channel.is_null())
        .map(|channel| (channel_name, channel))
        .or_else(|| {
            distribution
                .pointer("/channels/stable")
                .filter(|channel| !channel.is_null())
                .map(|channel| ("stable", channel))
        })
        .ok_or_else(|| "배포 채널 정보를 찾지 못했습니다.".to_string())?;

    Ok((selected_name, channel))
}

pub(crate) fn load_instance_launch_plan(instance_dir: &Path) -> Option<(Value, PathBuf)> {
    for file_name in [
        "launch-profile.json",
        "launcher-launch.json",
        "launch-plan.json",
        "launch.json",
    ] {
        let path = instance_dir.join(file_name);

        if path.exists() {
            if let Ok(plan) = read_json_file(&path) {
                return Some((plan, instance_dir.to_path_buf()));
            }
        }
    }

    None
}

pub(crate) fn manifest_working_directory(server_manifest: &Value) -> String {
    server_manifest
        .pointer("/launch/workingDirectory")
        .and_then(Value::as_str)
        .unwrap_or("instances/default")
        .to_string()
}

pub(crate) fn build_replacement_map(
    bundle_root: &Path,
    server_manifest: &Value,
    user_config: &Value,
    launch_plan: Option<&Value>,
) -> Result<Map<String, Value>, String> {
    let session = user_config
        .get("authSession")
        .and_then(Value::as_object)
        .ok_or_else(|| "저장된 Minecraft 세션을 찾지 못했습니다.".to_string())?;
    let settings = user_config
        .get("settings")
        .and_then(Value::as_object)
        .ok_or_else(|| "사용자 설정을 찾지 못했습니다.".to_string())?;
    let mut variables = Map::new();
    let game_directory = launch_plan
        .and_then(|plan| plan.get("gameDirectory"))
        .and_then(Value::as_str)
        .map(|path| bundle_root.join(path))
        .unwrap_or_else(|| bundle_root.to_path_buf());
    let assets_directory = launch_plan
        .and_then(|plan| plan.get("assetsDirectory"))
        .and_then(Value::as_str)
        .map(|path| bundle_root.join(path))
        .unwrap_or_else(|| game_directory.join("assets"));
    let natives_directory = launch_plan
        .and_then(|plan| plan.get("nativesDirectory"))
        .or_else(|| launch_plan.and_then(|plan| plan.get("nativeDirectory")))
        .and_then(Value::as_str)
        .map(|path| bundle_root.join(path))
        .unwrap_or_else(|| game_directory.join("natives"));
    let logging_config_file = launch_plan
        .and_then(|plan| plan.get("loggingConfigFile"))
        .and_then(Value::as_str)
        .map(|path| bundle_root.join(path).to_string_lossy().into_owned())
        .unwrap_or_default();
    variables.insert(
        "auth_player_name".to_string(),
        session.get("playerName").cloned().unwrap_or(Value::Null),
    );
    variables.insert(
        "auth_uuid".to_string(),
        session.get("profileId").cloned().unwrap_or(Value::Null),
    );
    variables.insert(
        "version_name".to_string(),
        server_manifest
            .get("minecraftVersion")
            .cloned()
            .unwrap_or(Value::String("star-prison".to_string())),
    );
    variables.insert(
        "game_directory".to_string(),
        Value::String(game_directory.to_string_lossy().into_owned()),
    );
    variables.insert(
        "assets_root".to_string(),
        Value::String(assets_directory.to_string_lossy().into_owned()),
    );
    variables.insert(
        "assets_directory".to_string(),
        Value::String(assets_directory.to_string_lossy().into_owned()),
    );
    variables.insert(
        "assets_index_name".to_string(),
        server_manifest
            .get("minecraftVersion")
            .cloned()
            .unwrap_or(Value::String(String::new())),
    );
    variables.insert("user_type".to_string(), Value::String("msa".to_string()));
    variables.insert(
        "version_type".to_string(),
        Value::String("release".to_string()),
    );
    variables.insert(
        "launcher_name".to_string(),
        Value::String("star-prison-launcher".to_string()),
    );
    variables.insert(
        "launcher_version".to_string(),
        Value::String(current_launcher_version()),
    );
    variables.insert(
        "natives_directory".to_string(),
        Value::String(natives_directory.to_string_lossy().into_owned()),
    );
    variables.insert(
        "logging_config_file".to_string(),
        Value::String(logging_config_file),
    );
    variables.insert(
        "quick_play_path".to_string(),
        Value::String(
            game_directory
                .join("quickPlay")
                .join("launcher-quick-play.json")
                .to_string_lossy()
                .into_owned(),
        ),
    );
    variables.insert(
        "server_address".to_string(),
        Value::String(
            server_manifest
                .get("address")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|address| !address.is_empty())
                .ok_or_else(|| "config/server.manifest.json에 address 값이 없습니다.".to_string())?
                .to_string(),
        ),
    );
    variables.insert(
        "classpath_separator".to_string(),
        Value::String(if cfg!(windows) { ";" } else { ":" }.to_string()),
    );
    variables.insert(
        "extra_jvm_args".to_string(),
        settings.get("extraJvmArgs").cloned().unwrap_or(Value::Null),
    );
    variables.insert(
        "extra_game_args".to_string(),
        settings
            .get("extraGameArgs")
            .cloned()
            .unwrap_or(Value::Null),
    );

    Ok(variables)
}

pub(crate) fn expand_argument(value: &str, variables: &Map<String, Value>) -> String {
    let mut expanded = value.to_string();

    for (key, replacement) in variables {
        let replacement = replacement
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| replacement.to_string().trim_matches('"').to_string());
        expanded = expanded.replace(&format!("${{{key}}}"), &replacement);
    }

    expanded
}

pub(crate) fn expand_arguments(values: Vec<String>, variables: &Map<String, Value>) -> Vec<String> {
    values
        .iter()
        .map(|value| expand_argument(value, variables))
        .filter(|value| !value.trim().is_empty())
        .collect()
}

pub(crate) fn classpath_from_plan(
    plan: &Value,
    bundle_root: &Path,
    variables: &Map<String, Value>,
) -> Option<String> {
    let entries = value_string_vec(plan.get("classpath").or_else(|| plan.get("classPath")));

    if entries.is_empty() {
        return None;
    }

    let separator = if cfg!(windows) { ";" } else { ":" };
    let mut classpath_entries = Vec::new();

    for entry in entries {
        let expanded_entry = expand_argument(&entry, variables);
        let path = PathBuf::from(&expanded_entry);
        let resolved_path = if path.is_absolute() {
            path
        } else {
            bundle_root.join(path)
        };

        classpath_entries.push(resolved_path.clone());
        classpath_entries.extend(native_classpath_entries_for(&resolved_path));
    }

    classpath_entries.sort();
    classpath_entries.dedup();

    Some(
        classpath_entries
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(separator),
    )
}

pub(crate) fn native_classpath_entries_for(library_path: &Path) -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }

    let Some(parent) = library_path.parent() else {
        return Vec::new();
    };

    let Some(file_stem) = library_path.file_stem().and_then(|value| value.to_str()) else {
        return Vec::new();
    };

    let native_path = parent.join(format!("{file_stem}-natives-windows.jar"));

    if native_path.exists() {
        vec![native_path]
    } else {
        Vec::new()
    }
}

pub(crate) fn session_required_string(user_config: &Value, field: &str) -> Result<String, String> {
    user_config
        .get("authSession")
        .and_then(Value::as_object)
        .and_then(|session| session.get(field))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("저장된 Minecraft 세션에 {field} 값이 없습니다."))
}

pub(crate) fn is_managed_auth_arg(arg: &str) -> bool {
    arg == "--accessToken" || arg.starts_with("--accessToken=")
}

pub(crate) fn remove_managed_auth_args(args: Vec<String>) -> Vec<String> {
    let mut sanitized = Vec::with_capacity(args.len());
    let mut index = 0;

    while index < args.len() {
        let current = &args[index];

        if is_managed_auth_arg(current) {
            index += if current == "--accessToken" && args.get(index + 1).is_some() {
                2
            } else {
                1
            };
            continue;
        }

        sanitized.push(current.clone());
        index += 1;
    }

    sanitized
}

pub(crate) fn append_managed_auth_args(
    args: &mut Vec<String>,
    user_config: &Value,
) -> Result<(), String> {
    let access_token = session_required_string(user_config, "accessToken")?;

    args.push("--accessToken".to_string());
    args.push(access_token);
    Ok(())
}

pub(crate) fn selected_game_resolution(settings: &Map<String, Value>) -> Option<(&str, &str)> {
    let resolution = settings.get("gameResolution")?.as_str()?.trim();

    if !GAME_RESOLUTION_OPTIONS.contains(&resolution) {
        return None;
    }

    resolution.split_once('x')
}

pub(crate) fn append_game_resolution_args(args: &mut Vec<String>, settings: &Map<String, Value>) {
    if let Some((width, height)) = selected_game_resolution(settings) {
        args.push("--width".to_string());
        args.push(width.to_string());
        args.push("--height".to_string());
        args.push(height.to_string());
    }
}

pub(crate) fn build_launch_arguments(
    bundle_root: &Path,
    server_manifest: &Value,
    user_config: &Value,
    launch_plan: Option<&Value>,
) -> Result<Vec<String>, String> {
    let settings = user_config
        .get("settings")
        .and_then(Value::as_object)
        .ok_or_else(|| "사용자 설정을 찾지 못했습니다.".to_string())?;
    let launch_config = server_manifest
        .get("launch")
        .and_then(Value::as_object)
        .ok_or_else(|| "서버 manifest에 launch 설정이 없습니다.".to_string())?;
    let variables = build_replacement_map(bundle_root, server_manifest, user_config, launch_plan)?;
    let max_ram_mb = settings
        .get("maxRamMb")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            server_manifest
                .pointer("/java/recommendedRamMb")
                .and_then(Value::as_u64)
                .unwrap_or(8192)
        });
    let mut args = Vec::new();

    args.push(format!("-Xmx{max_ram_mb}M"));
    args.extend(
        expand_arguments(value_string_vec(launch_config.get("jvmArgs")), &variables)
            .into_iter()
            .filter(|arg| !arg.starts_with("-Xmx")),
    );

    if let Some(plan) = launch_plan {
        args.extend(
            expand_arguments(value_string_vec(plan.get("jvmArgs")), &variables)
                .into_iter()
                .filter(|arg| !arg.starts_with("-Xmx")),
        );
    }

    args.extend(split_user_args(settings.get("extraJvmArgs")));

    if let Some(plan) = launch_plan {
        if let Some(classpath) = classpath_from_plan(plan, bundle_root, &variables) {
            args.push("-cp".to_string());
            args.push(classpath);
        }

        if let Some(main_class) = value_string(plan.get("mainClass")) {
            args.push(main_class);
        } else if let Some(jar_path) = value_string(plan.get("jar")) {
            let expanded_jar_path = expand_argument(&jar_path, &variables);
            let jar_path = PathBuf::from(&expanded_jar_path);

            args.push("-jar".to_string());
            args.push(
                if jar_path.is_absolute() {
                    jar_path
                } else {
                    bundle_root.join(jar_path)
                }
                .to_string_lossy()
                .into_owned(),
            );
        } else {
            return Err("launch plan에 mainClass 또는 jar 항목이 없습니다.".to_string());
        }

        args.extend(remove_managed_auth_args(expand_arguments(
            value_string_vec(plan.get("gameArgs")),
            &variables,
        )));
    } else {
        let fallback_jar = bundle_root.join("client.jar");

        if !fallback_jar.exists() {
            return Err(format!(
                "클라이언트 실행 계획을 찾지 못했습니다. {} 파일이나 {} 파일을 준비해 주세요.",
                bundle_root.join("launch-profile.json").to_string_lossy(),
                fallback_jar.to_string_lossy()
            ));
        }

        args.push("-jar".to_string());
        args.push(fallback_jar.to_string_lossy().into_owned());
    }

    args.extend(remove_managed_auth_args(expand_arguments(
        value_string_vec(launch_config.get("gameArgs")),
        &variables,
    )));

    append_managed_auth_args(&mut args, user_config)?;

    if !args.iter().any(|arg| arg == "--quickPlayMultiplayer") {
        args.extend(expand_arguments(
            vec![
                "--quickPlayPath".to_string(),
                "${quick_play_path}".to_string(),
                "--quickPlayMultiplayer".to_string(),
                "${server_address}".to_string(),
            ],
            &variables,
        ));
    }

    args.extend(remove_managed_auth_args(split_user_args(
        settings.get("extraGameArgs"),
    )));
    append_game_resolution_args(&mut args, settings);

    Ok(args)
}
